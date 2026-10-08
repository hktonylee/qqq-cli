//! Bulk modal state stays independent from active and parked editor drafts.
use super::{
    render::{PopupKind, PopupRow},
    tag_input,
};
use crate::bulk::{Actions, Report};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ITEMS: [(char, &str); 7] = [
    ('t', "Add tags"),
    ('r', "Remove tags"),
    ('s', "Replace tags"),
    ('p', "Priority"),
    ('a', "Archive"),
    ('u', "Unarchive"),
    ('x', "Clear selection"),
];

#[derive(Clone, Copy)]
enum InputKind {
    Add,
    Remove,
    Replace,
    Priority,
}
enum Stage {
    Menu {
        selected: usize,
    },
    Input {
        kind: InputKind,
        value: String,
        cursor: tag_input::Cursor,
        error: String,
    },
    Preview(Report),
    Error(String),
}
pub(super) struct View {
    ids: Vec<i64>,
    stage: Stage,
    top: usize,
    page: usize,
}
pub(super) enum Intent {
    None,
    Close,
    Clear,
    Preview(Actions),
    Apply(Report),
}

impl View {
    pub fn menu(ids: Vec<i64>) -> Self {
        Self {
            ids,
            stage: Stage::Menu { selected: 0 },
            top: 0,
            page: 1,
        }
    }
    pub fn show_preview(&mut self, report: Report) {
        self.stage = Stage::Preview(report);
        self.top = 0;
    }
    pub fn show_error(&mut self, text: String) {
        self.stage = Stage::Error(text);
        self.top = 0;
    }
    pub fn ids(&self) -> &[i64] {
        &self.ids
    }
    pub fn paste(&mut self, text: &str) {
        if let Stage::Input {
            value,
            cursor,
            error,
            ..
        } = &mut self.stage
        {
            cursor.insert(value, &text.replace("\r\n", "\n"));
            error.clear();
        }
    }
    pub fn key(&mut self, key: KeyEvent) -> Intent {
        if key.code == KeyCode::Esc
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
        {
            return Intent::Close;
        }
        if key.modifiers.contains(KeyModifiers::SUPER) {
            return Intent::None;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            if let Stage::Input {
                value,
                cursor,
                error,
                ..
            } = &mut self.stage
            {
                cursor.edit(value, error, key);
            }
            return Intent::None;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match &mut self.stage {
            Stage::Menu { selected } => {
                if control {
                    return Intent::None;
                }
                match key.code {
                    KeyCode::Up => {
                        *selected = selected.saturating_sub(1);
                        return Intent::None;
                    }
                    KeyCode::Down => {
                        *selected = (*selected + 1).min(ITEMS.len() - 1);
                        return Intent::None;
                    }
                    _ => {}
                }
                let activation = if key.code == KeyCode::Enter {
                    KeyCode::Char(ITEMS[*selected].0)
                } else {
                    key.code
                };
                let kind = match activation {
                    KeyCode::Char('x') => return Intent::Clear,
                    KeyCode::Char('a') => {
                        return Intent::Preview(Actions {
                            archived: Some(true),
                            ..Default::default()
                        });
                    }
                    KeyCode::Char('u') => {
                        return Intent::Preview(Actions {
                            archived: Some(false),
                            ..Default::default()
                        });
                    }
                    KeyCode::Char('t') => InputKind::Add,
                    KeyCode::Char('r') => InputKind::Remove,
                    KeyCode::Char('s') => InputKind::Replace,
                    KeyCode::Char('p') => InputKind::Priority,
                    _ => return Intent::None,
                };
                self.stage = Stage::Input {
                    kind,
                    value: String::new(),
                    cursor: tag_input::Cursor::at_end(""),
                    error: String::new(),
                };
            }
            Stage::Input {
                kind,
                value,
                cursor,
                error,
            } => {
                let preview = (control && key.code == KeyCode::Char('s'))
                    || (matches!(kind, InputKind::Priority)
                        && !control
                        && key.code == KeyCode::Enter);
                if preview {
                    let actions = match kind {
                        InputKind::Priority => value
                            .trim()
                            .parse::<i64>()
                            .map(|priority| Actions {
                                priority: Some(priority),
                                ..Default::default()
                            })
                            .map_err(|_| "Priority must be -100..100".to_owned()),
                        InputKind::Add | InputKind::Remove | InputKind::Replace => {
                            crate::tags::parse_lines(value)
                                .map(|tags| match kind {
                                    InputKind::Add => Actions {
                                        add_tags: tags,
                                        ..Default::default()
                                    },
                                    InputKind::Remove => Actions {
                                        remove_tags: tags,
                                        ..Default::default()
                                    },
                                    InputKind::Replace => Actions {
                                        set_tags: Some(tags),
                                        ..Default::default()
                                    },
                                    InputKind::Priority => unreachable!(),
                                })
                                .map_err(|failure| failure.to_string())
                        }
                    }
                    .and_then(|actions| actions.normalize().map_err(|failure| failure.to_string()));
                    match actions {
                        Ok(actions) => return Intent::Preview(actions),
                        Err(failure) => *error = failure,
                    }
                } else {
                    cursor.edit(value, error, key);
                }
            }
            Stage::Preview(report) => {
                if !control {
                    match key.code {
                        KeyCode::Char('y' | 'Y') => return Intent::Apply(report.clone()),
                        KeyCode::Char('n' | 'N') | KeyCode::Enter => return Intent::Close,
                        _ => self.scroll(key.code),
                    }
                }
            }
            Stage::Error(_) => {
                if !control && matches!(key.code, KeyCode::Enter | KeyCode::Char('n' | 'N')) {
                    return Intent::Close;
                }
                if !control {
                    self.scroll(key.code);
                }
            }
        }
        Intent::None
    }
    fn scroll(&mut self, key: KeyCode) {
        self.top = match key {
            KeyCode::Up => self.top.saturating_sub(1),
            KeyCode::Down => self.top.saturating_add(1),
            KeyCode::PageUp => self.top.saturating_sub(self.page),
            KeyCode::PageDown => self.top.saturating_add(self.page),
            KeyCode::Home => 0,
            KeyCode::End => usize::MAX,
            _ => self.top,
        };
    }
    pub fn rows(&mut self, width: usize, height: usize) -> Vec<PopupRow> {
        if let Stage::Input {
            kind,
            value,
            cursor,
            error,
        } = &self.stage
        {
            let title = match kind {
                InputKind::Priority => "Bulk priority",
                InputKind::Add => "Bulk add tags",
                InputKind::Remove => "Bulk remove tags",
                InputKind::Replace => "Bulk replace tags",
            };
            let mut rows = tag_input::rows(None, value, cursor, error, width, height);
            if let Some(first) = rows
                .first_mut()
                .filter(|row| row.kind == PopupKind::Heading)
            {
                first.text = format!("{title} ({})", self.ids.len());
            }
            let last = rows.len().saturating_sub(1);
            for (index, row) in rows.iter_mut().enumerate() {
                if row.kind == PopupKind::Hint {
                    row.text = if index != last {
                        if matches!(kind, InputKind::Priority) {
                            "Priority -100..100"
                        } else {
                            "One tag per line"
                        }
                    } else if matches!(kind, InputKind::Priority) {
                        if width >= 18 {
                            "Enter preview Esc"
                        } else {
                            "↵ preview"
                        }
                    } else if width >= 45 {
                        "Enter newline Ctrl-S preview Ctrl-U delete Esc"
                    } else if width >= 30 {
                        "↵ line ^S preview ^U delete Esc"
                    } else if width >= 21 {
                        "^S preview ^U del Esc"
                    } else {
                        "^S preview"
                    }
                    .to_owned();
                }
            }
            return rows;
        }
        let (heading, body, hint) = match &self.stage {
            Stage::Menu { selected } => {
                let mut rows: Vec<_> = wrap(&format!("IDs: {}", ids(&self.ids)), width)
                    .into_iter()
                    .map(|text| PopupRow::new(text, PopupKind::Body))
                    .collect();
                rows.extend(ITEMS.iter().enumerate().map(|(index, (key, label))| {
                    PopupRow::new(
                        format!("{key} {label}"),
                        if index == *selected {
                            PopupKind::SelectedAction
                        } else {
                            PopupKind::Action
                        },
                    )
                }));
                (
                    format!("Bulk actions ({})", self.ids.len()),
                    rows,
                    "Up/Down Enter  Esc cancel",
                )
            }
            Stage::Preview(report) => {
                let mut lines = vec![
                    format!("IDs: {}", ids(&report.selected_ids)),
                    format!(
                        "{} changed, {} actions",
                        report.changed_count, report.action_count
                    ),
                ];
                for row in &report.tasks {
                    lines.push(format!("#{}", row.id));
                    lines.push(format!(
                        "Tags: {} -> {}",
                        serde_json::to_string(&row.before.tags).expect("tags serializable"),
                        serde_json::to_string(&row.after.tags).expect("tags serializable")
                    ));
                    lines.push(format!(
                        "Priority: {} -> {}",
                        row.before.priority, row.after.priority
                    ));
                    lines.push(format!(
                        "Archived: {} -> {}",
                        row.before.archived, row.after.archived
                    ));
                }
                (
                    format!("Bulk preview ({})", self.ids.len()),
                    lines
                        .into_iter()
                        .flat_map(|text| wrap(&text, width))
                        .map(|text| PopupRow::new(text, PopupKind::Body))
                        .collect(),
                    if width >= 40 {
                        "y apply  n/Esc cancel  Up/Down PgUp/PgDn"
                    } else if width >= 21 {
                        "y apply n/Esc cancel"
                    } else if width >= 16 {
                        "y apply n cancel"
                    } else {
                        "y apply n no"
                    },
                )
            }
            Stage::Error(text) => (
                "Bulk error".to_owned(),
                wrap(text, width)
                    .into_iter()
                    .map(|text| PopupRow::new(text, PopupKind::Error))
                    .collect(),
                "Regenerate preview  Esc close  Up/Down",
            ),
            Stage::Input { .. } => unreachable!(),
        };
        let visible = height.saturating_sub(2);
        self.page = visible.max(1);
        let mut top = self.top.min(body.len().saturating_sub(visible));
        if visible > 0 && matches!(self.stage, Stage::Menu { .. }) {
            let row = body
                .iter()
                .position(|row| row.kind == PopupKind::SelectedAction)
                .expect("menu has selected action");
            if row < top {
                top = row;
            } else if row >= top + visible {
                top = row + 1 - visible;
            }
        }
        self.top = top;
        let mut rows = Vec::new();
        if height >= 2 {
            rows.push(PopupRow::new(heading, PopupKind::Heading));
            rows.extend(body.into_iter().skip(top).take(visible));
        }
        if height > 0 {
            rows.push(PopupRow::new(hint, PopupKind::Hint));
        }
        rows
    }
}
fn ids(ids: &[i64]) -> String {
    ids.iter()
        .map(|id| format!("#{id}"))
        .collect::<Vec<_>>()
        .join(", ")
}
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let cells = grapheme.width();
        if !line.is_empty() && used + cells > width.max(1) {
            rows.push(std::mem::take(&mut line));
            used = 0;
        }
        line.push_str(grapheme);
        used += cells;
    }
    rows.push(line);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_input_popups_reserve_confirmation_footer_in_short_screens() {
        let report: Report = serde_json::from_value(serde_json::json!({
            "version": 1, "applied": false, "database": "/test.db",
            "actions": {"add_tags": [], "remove_tags": [], "set_tags": null,
                "priority": 1, "archived": null},
            "selected_ids": [1], "selected_count": 1, "changed_count": 1,
            "action_count": 1,
            "tasks": [{"id": 1, "fingerprint": "0".repeat(64),
                "before": {"tags": [], "priority": 0, "archived": false},
                "after": {"tags": [], "priority": 1, "archived": false}}]
        }))
        .unwrap();
        for width in [12, 18, 28, 38, 39, 40, 46] {
            for height in (2..=18).chain(0..=1) {
                for stage in [0, 1, 2] {
                    let mut ui = View::menu(vec![1]);
                    match stage {
                        1 => ui.show_preview(report.clone()),
                        2 => ui.show_error("Conflict; regenerate preview".into()),
                        _ => (),
                    }
                    let rows = ui.rows(width, height);
                    assert!(rows.len() <= height, "stage={stage} height={height}");
                    if height >= 2 {
                        assert_eq!(rows[0].kind, PopupKind::Heading);
                        assert_eq!(rows.last().unwrap().kind, PopupKind::Hint);
                    }
                    if stage == 1 && height > 0 {
                        assert!(rows.last().unwrap().text.contains("y apply"));
                        assert!(rows.last().unwrap().text.width() <= width);
                    }
                }
            }
        }
    }

    #[test]
    fn short_input_popups_keep_value_cursor_and_preview_hint() {
        for width in [12, 18, 28, 46] {
            for height in 1..=18 {
                let mut ui = View::menu(vec![1, 2]);
                ui.key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
                ui.paste("first\n界面");
                let rows = ui.rows(width, height);
                let input = rows
                    .iter()
                    .rfind(|row| row.kind == PopupKind::Input)
                    .unwrap();
                assert_eq!(input.text, "> 界面", "height={height}");
                assert_eq!(input.cursor, Some(6));
                if height >= 2 {
                    let hint = &rows.last().unwrap().text;
                    assert!(hint.contains("preview"));
                    assert!(hint.width() <= width);
                }
            }
        }
    }

    #[test]
    fn tag_input_keeps_alt_word_navigation_from_single_task_editor() {
        let mut ui = View::menu(vec![1]);
        ui.key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        ui.paste("alpha beta");
        ui.key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));
        ui.key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE));
        match ui.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)) {
            Intent::Preview(actions) => assert_eq!(actions.add_tags, ["alpha Xbeta"]),
            _ => panic!("tag input should preview edited label"),
        }
    }
}
