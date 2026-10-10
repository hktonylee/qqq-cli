use super::render::{PopupKind, PopupRow};
use crate::views::SavedView;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;

fn footer_hints(width: usize) -> Vec<PopupRow> {
    let first = [
        "Up/Down Pick  Enter Apply",
        "↑↓ Pick  Enter Apply",
        "↑↓ Pick  Enter",
        "↑↓  Enter",
    ]
    .into_iter()
    .find(|text| text.width() <= width)
    .unwrap_or("↑↓  Enter");
    let second = [
        "PgUp/PgDn Details  Esc Cancel",
        "Pg↑↓ Scroll  Esc",
        "Pg↑↓  Esc",
    ]
    .into_iter()
    .find(|text| text.width() <= width)
    .unwrap_or("Pg↑↓  Esc");
    let combined = format!("{first}  {second}");
    if combined.width() <= width {
        vec![PopupRow::new(combined, PopupKind::Hint)]
    } else {
        vec![
            PopupRow::new(first, PopupKind::Hint),
            PopupRow::new(second, PopupKind::Hint),
        ]
    }
}

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
        let hints = footer_hints(width);
        let heading_height = usize::from(height > hints.len() + 1);
        let count = self.views.len() + 1;
        let visible = (height / 3)
            .max(1)
            .min(count)
            .min(height - hints.len() - heading_height);
        let start = self
            .selected
            .saturating_sub(visible - 1)
            .min(count - visible);
        let mut rows = Vec::new();
        if heading_height > 0 {
            rows.push(PopupRow::new("Choose view", PopupKind::Heading));
        }
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
        let available = height.saturating_sub(rows.len() + hints.len());
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
        rows.extend(hints);
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
    fn footer_hints_fit_width_keep_all_keys_and_leave_selection_visible() {
        use unicode_width::UnicodeWidthStr;
        for width in [10, 12, 16, 20, 28, 29, 44, 45, 46, 54, 72] {
            for height in [3, 4, 8, 18] {
                for error in ["", "Load failed"] {
                    let mut picker = Picker::new(definitions(), Some("View 29"));
                    picker.set_error(error.into());
                    let rows = picker.rows(width, height);
                    assert!(rows.len() <= height);
                    assert_eq!(
                        rows.iter()
                            .filter(|row| row.kind == PopupKind::SelectedAction)
                            .count(),
                        1
                    );
                    let hints: Vec<_> = rows
                        .iter()
                        .filter(|row| row.kind == PopupKind::Hint)
                        .collect();
                    assert!((1..=2).contains(&hints.len()));
                    for hint in &hints {
                        assert!(hint.text.width() <= width, "width {width}: {}", hint.text);
                    }
                    assert!(
                        rows[rows.len() - hints.len()..]
                            .iter()
                            .all(|row| row.kind == PopupKind::Hint)
                    );
                    let text = hints
                        .iter()
                        .map(|row| row.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    assert!(text.contains("Up/Down") || text.contains("↑↓"));
                    assert!(text.contains("PgUp/PgDn") || text.contains("Pg↑↓"));
                    assert!(text.contains("Enter") && text.contains("Esc"));
                    if height >= 5 {
                        assert!(rows.iter().any(|row| row.kind
                            == if error.is_empty() {
                                PopupKind::Body
                            } else {
                                PopupKind::Error
                            }));
                    }
                }
            }
        }
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
