use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct FilterTask<'a> {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub description: &'a str,
    pub status: &'a str,
}

pub fn adjacent_visible_id(ids: &[i64], current: Option<i64>, older: bool) -> Option<i64> {
    match current.and_then(|current| ids.iter().position(|id| *id == current)) {
        Some(index) if older => index.checked_sub(1).map(|index| ids[index]),
        Some(index) => ids.get(index + 1).copied(),
        None if older => ids.last().copied(),
        None if current.is_some() => ids.first().copied(),
        None => None,
    }
}

pub struct ListRow {
    pub text: String,
    pub task_id: Option<i64>,
    pub dirty: bool,
    pub description_start: Option<usize>,
    pub tag_range: Option<(usize, usize)>,
    pub preview_end: Option<usize>,
}

pub struct TagTask<'a> {
    pub id: i64,
    pub tags: &'a [String],
    pub archived: bool,
    pub context_only: bool,
}

/// Set tag byte ranges after description offsets; match only actual label prefixes.
pub fn set_tag_ranges(rows: &mut [ListRow], tasks: &[TagTask<'_>]) {
    let prefixes: HashMap<_, _> = tasks
        .iter()
        .filter(|task| !task.tags.is_empty())
        .map(|task| {
            let mut prefix = if task.archived { "[archived] " } else { "" }.to_owned();
            if task.context_only {
                prefix.push_str("[context] ");
            }
            let tag_start = prefix.len();
            for (index, tag) in task.tags.iter().enumerate() {
                if index > 0 {
                    prefix.push(' ');
                }
                prefix.push('[');
                prefix.push_str(tag);
                prefix.push(']');
            }
            (task.id, (prefix, tag_start))
        })
        .collect();
    let mut previous = None;
    let mut column = 0;
    let mut consumed = 0;
    for row in rows {
        row.tag_range = None;
        if row.task_id != previous {
            column = row
                .description_start
                .map_or(row.text.chars().count(), |start| {
                    row.text[..start].chars().count()
                });
            consumed = 0;
            previous = row.task_id;
        }
        let Some((prefix, tag_start)) = row.task_id.and_then(|id| prefixes.get(&id)) else {
            continue;
        };
        let start = row
            .text
            .char_indices()
            .nth(column)
            .map_or(row.text.len(), |(index, _)| index);
        let end = row.preview_end.unwrap_or(row.text.len());
        let payload = row.text.get(start..end).unwrap_or("");
        let matched: usize = payload
            .chars()
            .zip(prefix[consumed..].chars())
            .take_while(|(actual, expected)| actual == expected)
            .map(|(actual, _)| actual.len_utf8())
            .sum();
        let highlight_start = consumed.max(*tag_start);
        let highlight_end = consumed + matched;
        if highlight_start < highlight_end {
            row.tag_range = Some((start + highlight_start - consumed, start + matched));
        }
        consumed = if matched < payload.len() {
            prefix.len()
        } else {
            highlight_end
        };
    }
}

pub fn visible_ids(rows: &[ListRow]) -> Vec<i64> {
    let mut ids = Vec::new();
    for id in rows.iter().filter_map(|row| row.task_id) {
        if ids.last() != Some(&id) {
            ids.push(id);
        }
    }
    ids
}

pub fn rows(tree: &str, width: usize) -> Vec<ListRow> {
    let mut task_id = None;
    let mut task_rows = 0;
    let mut rows: Vec<ListRow> = Vec::new();
    for (index, line) in tree.lines().enumerate() {
        if index == 0
            && (line.split_whitespace().eq(["ID", "STATUS", "TASK"])
                || line.split_whitespace().eq(["ID", "TASK"]))
        {
            continue;
        }
        if line.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            task_id = line
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
            task_rows = 0;
        }
        if task_id.is_some() {
            task_rows += 1;
            if task_rows > 3 {
                if task_rows == 4 {
                    let last = rows.last_mut().expect("task has three preview rows");
                    let mut cells = 0;
                    last.text = last
                        .text
                        .graphemes(true)
                        .take_while(|grapheme| {
                            cells += grapheme.width();
                            cells <= width.saturating_sub(3)
                        })
                        .collect();
                    last.preview_end = Some(last.text.len());
                    last.text.push_str(&".".repeat(width.min(3)));
                }
                continue;
            }
        }
        rows.push(ListRow {
            text: line.to_owned(),
            task_id,
            dirty: false,
            description_start: None,
            tag_range: None,
            preview_end: None,
        });
    }
    rows
}

/// Locate first description rows from task hierarchy, never from description text.
/// Metadata widths mirror output::task_tree; large IDs can grow beyond six cells.
pub fn set_dirty_markers(
    rows: &mut [ListRow],
    tasks: &[FilterTask<'_>],
    dirty: &HashSet<i64>,
    show_status: bool,
) {
    let ids: HashSet<_> = tasks.iter().map(|task| task.id).collect();
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut stack = Vec::new();
    for task in tasks {
        match task.parent_id.filter(|id| ids.contains(id)) {
            Some(parent) => children.entry(parent).or_default().push(task.id),
            None => stack.push((task.id, 0_usize)),
        }
    }
    let mut columns = HashMap::new();
    while let Some((id, depth)) = stack.pop() {
        let metadata = id.to_string().len().max(6) + 1 + if show_status { 13 } else { 0 };
        columns.insert(id, metadata + depth * 4);
        if let Some(children) = children.get(&id) {
            stack.extend(children.iter().map(|id| (*id, depth + 1)));
        }
    }
    let mut previous = None;
    for row in rows {
        row.dirty = row.task_id.is_some_and(|id| dirty.contains(&id));
        row.description_start = if row.task_id != previous {
            row.task_id.and_then(|id| columns.get(&id)).map(|column| {
                row.text
                    .char_indices()
                    .nth(*column)
                    .map_or(row.text.len(), |(index, _)| index)
            })
        } else {
            None
        };
        previous = row.task_id;
    }
}

/// Reveal whole selected preview; anchor oversized previews at their first row.
pub fn scroll_to(rows: &[ListRow], selected: Option<i64>, top: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    let last = rows.len().saturating_sub(height);
    let next = match selected.and_then(|id| rows.iter().position(|row| row.task_id == Some(id))) {
        Some(start) => {
            let count = rows[start..]
                .iter()
                .take_while(|row| row.task_id == selected)
                .count();
            if start < top || count > height {
                start
            } else if start + count > top.saturating_add(height) {
                start + count - height
            } else {
                top
            }
        }
        None => last,
    };
    next.min(last)
}

pub fn wheel_top(top: usize, rows: usize, height: usize, down: bool) -> usize {
    let last = rows.saturating_sub(height);
    if down {
        top.saturating_add(3).min(last)
    } else {
        top.saturating_sub(3).min(last)
    }
}
