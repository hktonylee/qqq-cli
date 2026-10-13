use super::render::{PopupKind, PopupRow};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;

const TEMPLATES: [(&str, &str); 6] = [
    ("Unarchived tasks", "not archived"),
    ("Active tasks", "not archived and status ~= \"completed\""),
    ("By tag", "not archived and has_tag(\"backend\")"),
    ("High priority", "not archived and priority >= 5"),
    (
        "Recent tasks",
        "not archived\nand datetime(created_time) >= datetime(\"now\", \"-7 days\")",
    ),
    ("All tasks", "true"),
];

pub(super) enum Action {
    Use(&'static str),
    Cancel,
}

pub(super) struct Picker {
    selected: usize,
}

impl Picker {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        let key = super::escape_alias(key);
        if key.code == KeyCode::Esc
            || (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c'))
        {
            return Some(Action::Cancel);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            return None;
        }
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(TEMPLATES.len() - 1),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = TEMPLATES.len() - 1,
            KeyCode::Enter => return Some(Action::Use(TEMPLATES[self.selected].1)),
            _ => (),
        }
        None
    }

    pub fn rows(&self, width: usize, height: usize) -> Vec<PopupRow> {
        if height == 0 {
            return Vec::new();
        }
        let heading = usize::from(height >= 3);
        let footer = usize::from(height >= 2);
        let available = height - heading - footer;
        let visible = (available / 2).max(1).min(TEMPLATES.len());
        let start = self
            .selected
            .saturating_sub(visible - 1)
            .min(TEMPLATES.len() - visible);
        let mut rows = Vec::new();
        if heading > 0 {
            rows.push(PopupRow::new("View templates", PopupKind::Heading));
        }
        rows.extend(TEMPLATES.iter().enumerate().skip(start).take(visible).map(
            |(index, (name, _))| {
                PopupRow::new(
                    *name,
                    if index == self.selected {
                        PopupKind::SelectedChoice
                    } else {
                        PopupKind::Choice
                    },
                )
            },
        ));
        let preview_height = height.saturating_sub(rows.len() + footer);
        rows.extend(
            TEMPLATES[self.selected]
                .1
                .lines()
                .flat_map(|line| super::wrap_modal(line, width))
                .take(preview_height)
                .map(|line| PopupRow::new(line, PopupKind::Body)),
        );
        if footer > 0 {
            let hint = ["↑↓ Pick Enter Use Esc Back", "↑↓ Enter Esc", "↑↓ ↵ Esc"]
                .into_iter()
                .find(|text| text.width() <= width)
                .unwrap_or("↑↓ ↵ Esc");
            rows.push(PopupRow::new(hint, PopupKind::Hint));
        }
        rows
    }
}
