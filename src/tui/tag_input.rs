use super::render::{PopupKind, PopupRow};

pub(super) fn rows(
    id: i64,
    value: &str,
    error: &str,
    width: usize,
    height: usize,
) -> Vec<PopupRow> {
    let heading = usize::from(height >= 3);
    let guidance = usize::from(height >= if error.is_empty() { 4 } else { 5 });
    let footer = usize::from(height >= 2);
    let body_height = height.saturating_sub(heading + guidance + footer);
    let errors: Vec<_> = super::wrap_modal(error, width)
        .into_iter()
        .filter(|_| !error.is_empty())
        .take(body_height.saturating_sub(1).min(3))
        .collect();
    let input_height = body_height.saturating_sub(errors.len());
    let mut inputs: Vec<_> = value.split('\n').rev().take(input_height).collect();
    inputs.reverse();
    let mut rows = Vec::new();
    if heading > 0 {
        rows.push(PopupRow::new(
            format!("Tags task #{id}"),
            PopupKind::Heading,
        ));
    }
    if guidance > 0 {
        rows.push(PopupRow::new(
            "One tag per line; blank clears",
            PopupKind::Hint,
        ));
    }
    rows.extend(
        inputs
            .into_iter()
            .map(|line| PopupRow::new(format!("> {line}"), PopupKind::Input)),
    );
    rows.extend(
        errors
            .into_iter()
            .map(|line| PopupRow::new(line, PopupKind::Error)),
    );
    if footer > 0 {
        let full = "Ctrl-U clear Shift-Enter line Enter apply Esc";
        let compact = "Ctrl-U Shift-Enter Enter Esc";
        let shortcuts = if full.len() <= width {
            full
        } else if compact.len() <= width {
            compact
        } else {
            "^U S-↵ ↵ Esc"
        };
        rows.push(PopupRow::new(shortcuts, PopupKind::Hint));
    }
    rows
}
