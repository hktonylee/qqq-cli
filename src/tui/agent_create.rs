use super::{
    render::{self, PopupKind, PopupRow},
    tag_input,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation;

pub(super) const DEFAULT_PROMPT: &str = "/goal Use the `qqq cli --wait`, to look for next task, and work on that. Repeat this forever. Please don't wake up repeatedly to poll the status when running waiter.";

pub(super) enum Action {
    Create(String),
    Cancel,
}

pub(super) struct View {
    root: PathBuf,
    value: String,
    cursor: tag_input::Cursor,
    error: String,
}

impl View {
    pub fn new(root: PathBuf, prompt: Option<String>) -> Self {
        let value = prompt.unwrap_or_else(|| DEFAULT_PROMPT.into());
        let cursor = tag_input::Cursor::at_end(&value);
        Self {
            root,
            value,
            cursor,
            error: String::new(),
        }
    }

    pub fn paste(&mut self, text: &str) {
        self.cursor
            .insert(&mut self.value, &text.replace("\r\n", "\n"));
        self.error.clear();
    }

    pub fn key(&mut self, key: KeyEvent, width: usize) -> Option<Action> {
        if key.code == KeyCode::Esc
            || (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c'))
        {
            return Some(Action::Cancel);
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('s') {
            if self.value.trim().is_empty() {
                self.error = "Enter a prompt".into();
            } else {
                return Some(Action::Create(self.value.clone()));
            }
        } else if matches!(key.code, KeyCode::Up | KeyCode::Down)
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            let layout = self.layout(width);
            self.cursor
                .visual_vertical(&self.value, &layout, key.code == KeyCode::Up);
        } else {
            self.cursor.edit(&mut self.value, &mut self.error, key);
        }
        None
    }

    fn layout(&self, width: usize) -> render::Layout {
        let fragments: Vec<_> = self.value.graphemes(true).map(str::to_owned).collect();
        // Match the popup's two-cell padding and one-cell caret reservation.
        render::Layout::new(&fragments, &[], width.saturating_sub(3))
    }

    pub fn rows(&self, width: usize, height: usize) -> Vec<PopupRow> {
        let heading = usize::from(height >= 3);
        let guidance = usize::from(height >= if self.error.is_empty() { 4 } else { 5 });
        let footer = usize::from(height >= 2);
        let body = height.saturating_sub(heading + guidance + footer);
        let errors: Vec<_> = super::wrap_modal(&self.error, width)
            .into_iter()
            .filter(|_| !self.error.is_empty())
            .take(body.saturating_sub(1).min(3))
            .collect();
        let input_height = body.saturating_sub(errors.len());
        let layout = self.layout(width);
        let caret = self.value[..self.cursor.byte()].graphemes(true).count();
        let (active, column) = layout.positions[caret];
        let start = active
            .saturating_sub(input_height / 2)
            .min(layout.rows.len().saturating_sub(input_height));
        let mut rows = Vec::new();
        if heading > 0 {
            rows.push(PopupRow::new("Create Herdr agent", PopupKind::Heading));
        }
        if guidance > 0 {
            rows.push(PopupRow::new(
                format!("Directory: {}", self.root.display()),
                PopupKind::Hint,
            ));
        }
        rows.extend(
            layout
                .rows
                .iter()
                .enumerate()
                .skip(start)
                .take(input_height)
                .map(|(index, text)| {
                    let mut row = PopupRow::new(format!("  {text}"), PopupKind::Input);
                    if index == active {
                        row.cursor = Some((2 + column).min(width.saturating_sub(1)));
                    }
                    row
                }),
        );
        rows.extend(
            errors
                .into_iter()
                .map(|text| PopupRow::new(text, PopupKind::Error)),
        );
        if footer > 0 {
            let hint = if width >= 37 {
                "Enter line  Ctrl-S create  Esc cancel"
            } else if width >= 21 {
                "Enter  Ctrl-S  Esc"
            } else {
                "↵ ^S Esc"
            };
            rows.push(PopupRow::new(hint, PopupKind::Hint));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn down_reaches_wrapped_row_starting_with_wide_grapheme() {
        let mut view = View::new(PathBuf::from("/project"), None);
        view.value = format!("{}界bbb", "a".repeat(43));
        view.cursor = tag_input::Cursor::at_end(&view.value);
        view.key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE), 46);
        view.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 46);
        let layout = view.layout(46);
        let caret = view.value[..view.cursor.byte()].graphemes(true).count();
        assert_eq!(layout.positions[caret].0, 1);
        assert!(view.value.is_char_boundary(view.cursor.byte()));
    }

    #[test]
    fn arrows_move_within_wrapped_prompt_before_editing() {
        let mut view = View::new(PathBuf::from("/project"), None);
        let end = view.cursor.byte();
        view.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), 36);
        assert!(view.cursor.byte() < end);
        let insertion = view.cursor.byte();
        view.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), 36);
        assert_eq!(view.cursor.byte(), end);
        view.key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), 36);
        assert_eq!(view.cursor.byte(), insertion);
        view.key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE), 36);
        let mut expected = DEFAULT_PROMPT.to_owned();
        expected.insert(insertion, 'X');
        assert_eq!(view.value, expected);
    }

    #[test]
    fn default_prompt_is_readable_without_horizontal_scrolling() {
        let view = View::new(PathBuf::from("/project"), None);
        for (width, height) in [(64, 10), (36, 12)] {
            let rows = view.rows(width, height);
            let text: String = rows
                .iter()
                .filter(|row| row.kind == PopupKind::Input)
                .map(|row| row.text.strip_prefix("  ").unwrap())
                .collect();
            assert_eq!(text, DEFAULT_PROMPT);
            assert!(rows.iter().any(|row| row.cursor.is_some()));
            assert!(rows.len() <= height);
        }
    }
}
