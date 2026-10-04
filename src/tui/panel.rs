use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct FilterTask<'a> {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub description: &'a str,
    pub status: &'a str,
}

pub struct FilteredTasks {
    pub included_ids: HashSet<i64>,
}

pub fn filter_tasks(tasks: &[FilterTask<'_>], query: &str, show_completed: bool) -> FilteredTasks {
    let query = query.to_lowercase();
    let positions: HashMap<_, _> = tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| show_completed || task.status != "completed")
        .map(|(index, task)| (task.id, index))
        .collect();
    let mut included_ids = HashSet::new();
    for task in tasks
        .iter()
        .filter(|task| show_completed || task.status != "completed")
    {
        if !query.is_empty() && !task.description.to_lowercase().contains(&query) {
            continue;
        }
        let mut current = Some(task.id);
        while let Some(id) = current {
            let Some(&index) = positions.get(&id) else {
                break;
            };
            if !included_ids.insert(id) {
                break;
            }
            current = tasks[index].parent_id;
        }
    }
    FilteredTasks { included_ids }
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

pub fn scroll_to(rows: &[ListRow], selected: Option<i64>, top: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    let last = rows.len().saturating_sub(height);
    let next = match selected.and_then(|id| rows.iter().position(|row| row.task_id == Some(id))) {
        Some(index) if index < top => index,
        Some(index) if index >= top.saturating_add(height) => index + 1 - height,
        Some(_) => top,
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
