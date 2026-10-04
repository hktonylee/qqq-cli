use super::render::{PopupKind, PopupRow};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, PartialEq)]
pub(super) enum Action {
    Go(i64),
    Cancel,
}

#[derive(Default)]
pub(super) struct View {
    value: String,
    error: String,
}

impl View {
    pub fn paste(&mut self, text: &str) {
        self.value.extend(
            text.chars()
                .map(|ch| if ch.is_control() { ' ' } else { ch }),
        );
        self.error.clear();
    }
    pub fn set_error(&mut self, error: String) {
        self.error = error;
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        let control = key.modifiers == KeyModifiers::CONTROL;
        if key.code == KeyCode::Esc || (control && key.code == KeyCode::Char('c')) {
            return Some(Action::Cancel);
        }
        if control && key.code == KeyCode::Char('u') {
            self.value.clear();
            self.error.clear();
            return None;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            return None;
        }
        match key.code {
            KeyCode::Enter => match self.value.trim().parse::<i64>() {
                Ok(id) if id > 0 => return Some(Action::Go(id)),
                _ => self.error = "Enter a positive task ID".into(),
            },
            KeyCode::Backspace => {
                if let Some((index, _)) = self.value.grapheme_indices(true).next_back() {
                    self.value.truncate(index);
                }
                self.error.clear();
            }
            KeyCode::Char(ch) if !ch.is_control() => {
                self.value.push(ch);
                self.error.clear();
            }
            _ => (),
        }
        None
    }
    pub fn rows(&self, width: usize) -> Vec<PopupRow> {
        let mut rows = vec![
            PopupRow::new("Go to task", PopupKind::Heading),
            PopupRow::new("Enter task ID", PopupKind::Hint),
            PopupRow::new(format!("> {}", self.value), PopupKind::Input),
        ];
        rows.extend(
            super::wrap_modal(&self.error, width)
                .into_iter()
                .map(|text| PopupRow::new(text, PopupKind::Error)),
        );
        rows.push(PopupRow::new("Enter go  Esc cancel", PopupKind::Hint));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ctrl(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }
    #[test]
    fn validates_positive_task_ids() {
        for (text, id) in [("1", 1), (" 42 ", 42), ("9223372036854775807", i64::MAX)] {
            let mut view = View::default();
            view.paste(text);
            assert_eq!(view.key(key(KeyCode::Enter)), Some(Action::Go(id)));
        }
        for text in ["", "0", "-1", "abc", "9223372036854775808", "1 2"] {
            let mut view = View::default();
            view.paste(text);
            assert_eq!(view.key(key(KeyCode::Enter)), None);
            assert_eq!(view.error, "Enter a positive task ID");
        }
    }
    #[test]
    fn edits_graphemes_and_keeps_modal_keys_and_paste_local() {
        let mut view = View::default();
        view.paste("7e\u{301}");
        view.key(key(KeyCode::Backspace));
        assert_eq!(view.value, "7");
        view.key(ctrl('u'));
        view.paste("1\n2\x1b\t");
        assert_eq!(view.value, "1 2  ");
        assert_eq!(view.key(ctrl('s')), None);
        assert_eq!(view.key(ctrl('c')), Some(Action::Cancel));
        assert_eq!(view.key(key(KeyCode::Esc)), Some(Action::Cancel));
        assert_eq!(view.value, "1 2  ");
    }
    #[test]
    fn exposes_existing_popup_roles_and_clears_errors_on_edit() {
        let mut view = View::default();
        view.key(key(KeyCode::Enter));
        assert!(view.rows(80).iter().map(|row| row.kind).eq([
            PopupKind::Heading,
            PopupKind::Hint,
            PopupKind::Input,
            PopupKind::Error,
            PopupKind::Hint,
        ]));
        view.key(key(KeyCode::Char('2')));
        assert!(view.error.is_empty());
        view.set_error("Task #2 not found".into());
        view.key(key(KeyCode::Backspace));
        assert!(view.error.is_empty());
        view.set_error("Load error".into());
        view.key(ctrl('u'));
        assert!(view.error.is_empty() && view.value.is_empty());
    }
}
