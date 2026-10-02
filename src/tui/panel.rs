use std::collections::{HashMap, HashSet};

pub struct FilterTask<'a> {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub description: &'a str,
}

pub struct FilteredTasks {
    pub included_ids: HashSet<i64>,
    pub ordered_ids: Vec<i64>,
}

pub fn filter_tasks(tasks: &[FilterTask<'_>], query: &str) -> FilteredTasks {
    let query = query.to_lowercase();
    let positions: HashMap<_, _> = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| (task.id, index))
        .collect();
    let mut included_ids = HashSet::new();
    for task in tasks {
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
    let ordered_ids = tasks
        .iter()
        .filter(|task| included_ids.contains(&task.id))
        .map(|task| task.id)
        .collect();
    FilteredTasks {
        included_ids,
        ordered_ids,
    }
}

pub fn adjacent_visible_id(ids: &[i64], current: Option<i64>, older: bool) -> Option<i64> {
    if older {
        ids.iter()
            .rev()
            .copied()
            .find(|id| current.is_none_or(|current| *id < current))
    } else {
        current.and_then(|current| ids.iter().copied().find(|id| *id > current))
    }
}

pub struct ListRow {
    pub text: String,
    pub task_id: Option<i64>,
}

pub fn rows(tree: &str) -> Vec<ListRow> {
    let mut task_id = None;
    tree.lines()
        .map(|line| {
            if line.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                task_id = line
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse().ok());
            }
            ListRow {
                text: line.to_owned(),
                task_id,
            }
        })
        .collect()
}

pub fn scroll_to(rows: &[ListRow], selected: Option<i64>, top: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    let last = rows.len().saturating_sub(height);
    let target = selected.or_else(|| rows.iter().filter_map(|row| row.task_id).max());
    let next = match target.and_then(|id| rows.iter().position(|row| row.task_id == Some(id))) {
        Some(index) if index < top => index,
        Some(index) if index >= top.saturating_add(height) => index + 1 - height,
        Some(_) => top,
        None => last,
    };
    next.min(last)
}
