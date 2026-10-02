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
