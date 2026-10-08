use super::render::{PopupKind, PopupRow};
use crate::views::SavedView;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(super) enum Action {
    Apply(Option<SavedView>),
    Cancel,
}

pub(super) struct Picker {
    views: Vec<SavedView>,
    selected: usize,
    preview_top: usize,
    error: String,
}

impl Picker {
    pub fn new(views: Vec<SavedView>, active: Option<&str>) -> Self {
        let selected = active
            .and_then(|name| views.iter().position(|view| view.name == name))
            .map_or(0, |index| index + 1);
        Self {
            views,
            selected,
            preview_top: 0,
            error: String::new(),
        }
    }
    pub fn set_error(&mut self, error: String) {
        self.error = error;
        self.preview_top = 0;
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
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
        let previous = self.selected;
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(self.views.len()),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.views.len(),
            KeyCode::PageUp => self.preview_top = self.preview_top.saturating_sub(5),
            KeyCode::PageDown => self.preview_top = self.preview_top.saturating_add(5),
            KeyCode::Enter => {
                return Some(Action::Apply(
                    self.selected
                        .checked_sub(1)
                        .map(|index| self.views[index].clone()),
                ));
            }
            _ => (),
        }
        if previous != self.selected {
            self.preview_top = 0;
            self.error.clear();
        }
        None
    }
    pub fn rows(&mut self, width: usize, height: usize) -> Vec<PopupRow> {
        let height = height.max(3);
        let count = self.views.len() + 1;
        let visible = (height / 3).max(1).min(count);
        let start = self
            .selected
            .saturating_sub(visible - 1)
            .min(count - visible);
        let mut rows = vec![PopupRow::new("Choose view", PopupKind::Heading)];
        for index in start..start + visible {
            let name = if index == 0 {
                "All tasks (no view)"
            } else {
                &self.views[index - 1].name
            };
            rows.push(PopupRow::new(
                name,
                if index == self.selected {
                    PopupKind::SelectedAction
                } else {
                    PopupKind::Action
                },
            ));
        }
        let preview = if !self.error.is_empty() {
            vec![self.error.clone()]
        } else if self.selected == 0 {
            vec!["Use startup selectors and visibility.".to_owned()]
        } else {
            crate::views::describe(&self.views[self.selected - 1])
        };
        let preview: Vec<_> = preview
            .iter()
            .flat_map(|line| super::wrap_modal(line, width))
            .collect();
        let available = height.saturating_sub(rows.len() + 1);
        self.preview_top = self
            .preview_top
            .min(preview.len().saturating_sub(available.max(1)));
        let kind = if self.error.is_empty() {
            PopupKind::Body
        } else {
            PopupKind::Error
        };
        rows.extend(
            preview
                .into_iter()
                .skip(self.preview_top)
                .take(available)
                .map(|line| PopupRow::new(line, kind)),
        );
        rows.push(PopupRow::new(
            if width >= 44 {
                "Up/Down pick Enter apply PgUp/PgDn details Esc"
            } else {
                "↑↓ Enter Esc Pg↑↓"
            },
            PopupKind::Hint,
        ));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn definitions() -> Vec<SavedView> {
        (0..30)
            .map(|index| SavedView {
                name: format!("View {index:02}"),
                criteria: crate::selection::Selectors {
                    tags: vec!["界 面".into()],
                    filter: Some("priority > 5".into()),
                    ..Default::default()
                },
                include_archived: false,
                max_completed: None,
            })
            .collect()
    }
    #[test]
    fn selection_boundaries_and_cancel_stay_local() {
        let mut picker = Picker::new(Vec::new(), None);
        picker.key(key(KeyCode::Down));
        assert!(matches!(
            picker.key(key(KeyCode::Enter)),
            Some(Action::Apply(None))
        ));
        assert!(matches!(
            picker.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Cancel)
        ));
        let mut picker = Picker::new(definitions(), Some("View 29"));
        picker.key(key(KeyCode::Down));
        assert!(
            matches!(picker.key(key(KeyCode::Enter)), Some(Action::Apply(Some(view))) if view.name == "View 29")
        );
        picker.key(key(KeyCode::Home));
        assert!(matches!(
            picker.key(key(KeyCode::Enter)),
            Some(Action::Apply(None))
        ));
    }
    #[test]
    fn narrow_rows_keep_selected_and_preview_scroll_bounded() {
        let mut picker = Picker::new(definitions(), Some("View 29"));
        let rows = picker.rows(12, 7);
        assert!(rows.len() <= 7);
        assert!(
            rows.iter()
                .any(|row| row.text == "View 29" && row.kind == PopupKind::SelectedAction)
        );
        picker.key(key(KeyCode::PageDown));
        assert!(
            picker
                .rows(12, 7)
                .iter()
                .any(|row| row.text.contains("Filter") || row.text.contains("priority"))
        );
        picker.set_error("Load failed".into());
        assert!(
            picker
                .rows(20, 12)
                .iter()
                .any(|row| row.kind == PopupKind::Error)
        );
        picker.key(key(KeyCode::Up));
        assert!(picker.error.is_empty());
    }
}
