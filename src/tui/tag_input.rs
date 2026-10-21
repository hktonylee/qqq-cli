use super::render::{PopupKind, PopupRow};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone)]
pub(super) struct Cursor {
    byte: usize,
    column: Option<usize>,
    completion: Option<Completion>,
}

#[derive(Clone)]
struct Completion {
    tags: Vec<String>,
    selected: usize,
}

impl Cursor {
    pub(super) fn byte(&self) -> usize {
        self.byte
    }

    pub(super) fn at_end(value: &str) -> Self {
        Self {
            byte: value.len(),
            column: None,
            completion: None,
        }
    }

    pub(super) fn with_tags(mut self, mut tags: Vec<String>) -> Self {
        tags.sort();
        tags.dedup();
        self.completion = Some(Completion { tags, selected: 0 });
        self
    }

    fn label_range(&self, value: &str) -> std::ops::Range<usize> {
        let start = value[..self.byte]
            .rfind(['\n', ','])
            .map_or(0, |byte| byte + 1);
        let end = self.byte
            + value[self.byte..]
                .find(['\n', ','])
                .unwrap_or(value.len() - self.byte);
        start..end
    }

    fn matches(&self, value: &str) -> Vec<&str> {
        let Some(completion) = &self.completion else {
            return Vec::new();
        };
        let range = self.label_range(value);
        let prefix = value[range.start..self.byte].trim_start().to_lowercase();
        if prefix.is_empty() {
            return Vec::new();
        }
        let used: std::collections::HashSet<_> = value[..range.start]
            .split(['\n', ','])
            .chain(value[range.end..].split(['\n', ',']))
            .map(str::trim)
            .collect();
        completion
            .tags
            .iter()
            .filter(|tag| {
                tag.to_lowercase().starts_with(&prefix)
                    && tag.as_str() != value[range.clone()].trim()
                    && !used.contains(tag.as_str())
            })
            .map(String::as_str)
            .collect()
    }

    pub(super) fn has_matches(&self, value: &str) -> bool {
        !self.matches(value).is_empty()
    }

    fn snap(&mut self, value: &str) {
        self.byte = value
            .grapheme_indices(true)
            .map(|(byte, _)| byte)
            .chain(std::iter::once(value.len()))
            .find(|byte| *byte >= self.byte)
            .unwrap_or(value.len());
        self.column = None;
        if let Some(completion) = &mut self.completion {
            completion.selected = 0;
        }
    }

    pub(super) fn insert(&mut self, value: &mut String, text: &str) {
        value.insert_str(self.byte, text);
        self.byte += text.len();
        self.snap(value);
    }

    pub(super) fn backspace(&mut self, value: &mut String) {
        if let Some((previous, _)) = value[..self.byte].grapheme_indices(true).next_back() {
            value.replace_range(previous..self.byte, "");
            self.byte = previous;
            self.snap(value);
        }
    }

    pub(super) fn delete(&mut self, value: &mut String) {
        if let Some(next) = value[self.byte..].graphemes(true).next() {
            value.replace_range(self.byte..self.byte + next.len(), "");
            self.snap(value);
        }
    }

    pub(super) fn left(&mut self, value: &str) {
        if let Some((previous, _)) = value[..self.byte].grapheme_indices(true).next_back() {
            self.byte = previous;
        }
        self.column = None;
    }

    pub(super) fn right(&mut self, value: &str) {
        if let Some(next) = value[self.byte..].graphemes(true).next() {
            self.byte += next.len();
        }
        self.column = None;
    }

    pub(super) fn home(&mut self, value: &str) {
        self.byte = value[..self.byte].rfind('\n').map_or(0, |byte| byte + 1);
        self.column = None;
    }

    pub(super) fn end(&mut self, value: &str) {
        self.byte += value[self.byte..]
            .find('\n')
            .unwrap_or(value.len() - self.byte);
        self.column = None;
    }

    fn delete_line(&mut self, value: &mut String) {
        let start = value[..self.byte].rfind('\n').map_or(0, |byte| byte + 1);
        let end = value[start..].find('\n').map(|byte| start + byte);
        let range = match end {
            Some(end) => start..end + 1,
            None if start > 0 => start - 1..value.len(),
            None => 0..value.len(),
        };
        self.byte = range.start;
        value.replace_range(range, "");
        self.snap(value);
    }

    fn previous_word(&mut self, value: &str) {
        let mut before = value[..self.byte].grapheme_indices(true).rev().peekable();
        while let Some(&(byte, grapheme)) = before.peek() {
            if !grapheme.chars().all(char::is_whitespace) {
                break;
            }
            self.byte = byte;
            before.next();
        }
        for (byte, grapheme) in before {
            if grapheme.chars().all(char::is_whitespace) {
                break;
            }
            self.byte = byte;
        }
        self.column = None;
    }

    fn next_word(&mut self, value: &str) {
        let mut after = value[self.byte..].graphemes(true).peekable();
        while let Some(grapheme) = after.peek() {
            if !grapheme.chars().all(char::is_whitespace) {
                break;
            }
            self.byte += grapheme.len();
            after.next();
        }
        for grapheme in after {
            if grapheme.chars().all(char::is_whitespace) {
                break;
            }
            self.byte += grapheme.len();
        }
        self.column = None;
    }

    fn delete_previous_word(&mut self, value: &mut String) {
        let end = self.byte;
        let start = value[..end].rfind('\n').map_or(0, |byte| byte + 1);
        self.byte -= start;
        self.previous_word(&value[start..end]);
        // Word deletion stops at current line; word navigation can cross lines.
        self.byte += start;
        value.replace_range(self.byte..end, "");
        self.snap(value);
    }

    pub(super) fn edit(&mut self, value: &mut String, error: &mut String, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::SUPER) {
            return false;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        if self.completion.is_some() && !control && !alt {
            let matches = self.matches(value);
            let count = matches.len();
            if key.code == KeyCode::Tab {
                if count > 0 {
                    let selected = self.completion.as_ref().unwrap().selected.min(count - 1);
                    let label = matches[selected].to_owned();
                    let range = self.label_range(value);
                    self.byte = range.start + label.len();
                    value.replace_range(range, &label);
                    self.snap(value);
                    error.clear();
                }
                return true;
            }
            if count > 0 && matches!(key.code, KeyCode::Up | KeyCode::Down) {
                let completion = self.completion.as_mut().unwrap();
                completion.selected = if key.code == KeyCode::Up {
                    completion.selected.saturating_sub(1)
                } else {
                    (completion.selected + 1).min(count - 1)
                };
                return true;
            }
        }
        let changed = match (key.code, control, alt) {
            (KeyCode::Char('u'), true, false) => {
                self.delete_line(value);
                true
            }
            (KeyCode::Char('w'), true, false) => {
                self.delete_previous_word(value);
                true
            }
            (KeyCode::Char('a'), true, false) | (KeyCode::Home, false, false) => {
                self.home(value);
                false
            }
            (KeyCode::Char('e'), true, false) | (KeyCode::End, false, false) => {
                self.end(value);
                false
            }
            (KeyCode::Left, false, true) => {
                self.previous_word(value);
                false
            }
            (KeyCode::Right, false, true) => {
                self.next_word(value);
                false
            }
            (KeyCode::Char('v'), true, false) => {
                match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
                    Ok(text) => {
                        self.insert(value, &text.replace("\r\n", "\n"));
                        error.clear();
                    }
                    Err(failure) => *error = format!("Clipboard text unavailable: {failure}"),
                }
                return true;
            }
            (KeyCode::Left, false, false) => {
                self.left(value);
                false
            }
            (KeyCode::Right, false, false) => {
                self.right(value);
                false
            }
            (KeyCode::Up, false, false) | (KeyCode::Down, false, false) => {
                self.vertical(value, key.code == KeyCode::Up);
                false
            }
            (KeyCode::Enter, false, false) => {
                self.insert(value, "\n");
                true
            }
            (KeyCode::Tab, false, false) => {
                self.insert(value, "\t");
                true
            }
            (KeyCode::Backspace, false, false) => {
                self.backspace(value);
                true
            }
            (KeyCode::Delete, false, false) => {
                self.delete(value);
                true
            }
            (KeyCode::Char(ch), false, false) if !ch.is_control() => {
                self.insert(value, &ch.to_string());
                true
            }
            _ => return false,
        };
        if changed {
            error.clear();
        }
        if let Some(completion) = &mut self.completion {
            completion.selected = 0;
        }
        true
    }

    pub(super) fn visual_vertical(
        &mut self,
        value: &str,
        layout: &super::render::Layout,
        up: bool,
    ) {
        let caret = value[..self.byte].graphemes(true).count();
        let (row, column) = layout.positions[caret];
        let preferred = self.column.unwrap_or(column);
        let target = if up { row.saturating_sub(1) } else { row + 1 };
        if let Some((index, _)) = layout
            .positions
            .iter()
            .enumerate()
            .rev()
            .find(|(_, (row, column))| *row == target && *column <= preferred)
            .or_else(|| {
                layout
                    .positions
                    .iter()
                    .enumerate()
                    .find(|(_, (row, _))| *row == target)
            })
        {
            self.byte = value
                .grapheme_indices(true)
                .nth(index)
                .map_or(value.len(), |(byte, _)| byte);
            self.column = Some(preferred);
        }
    }

    pub(super) fn vertical(&mut self, value: &str, up: bool) {
        let line = value[..self.byte]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        let starts: Vec<_> = std::iter::once(0)
            .chain(value.match_indices('\n').map(|(byte, _)| byte + 1))
            .collect();
        let target = if up {
            line.saturating_sub(1)
        } else {
            (line + 1).min(starts.len() - 1)
        };
        if target == line {
            return;
        }
        let start = starts[line];
        let column = self
            .column
            .unwrap_or_else(|| super::render::escape(&value[start..self.byte]).width());
        let target_start = starts[target];
        let target_end = starts.get(target + 1).map_or(value.len(), |byte| byte - 1);
        let mut used = 0;
        self.byte = target_start;
        for grapheme in value[target_start..target_end].graphemes(true) {
            let cells = super::render::escape(grapheme).width();
            if used + cells > column {
                break;
            }
            used += cells;
            self.byte += grapheme.len();
        }
        self.column = Some(column);
    }
}

fn focused_row(value: &str, byte: usize, width: usize) -> PopupRow {
    let safe = super::render::escape(value);
    let caret = super::render::escape(&value[..byte]).width();
    let mut start = 0;
    let mut skipped = 0;
    for (index, grapheme) in safe.grapheme_indices(true) {
        if caret.saturating_sub(skipped) <= width.saturating_sub(1) {
            break;
        }
        start = index + grapheme.len();
        skipped += grapheme.width();
    }
    let mut row = PopupRow::new(
        super::render::clipped(&safe[start..], width),
        PopupKind::Input,
    );
    row.cursor = Some(caret.saturating_sub(skipped).min(width.saturating_sub(1)));
    row
}

pub(super) fn rows(
    id: Option<i64>,
    value: &str,
    cursor: &Cursor,
    error: &str,
    width: usize,
    height: usize,
) -> Vec<PopupRow> {
    let matches = cursor.matches(value);
    let heading = usize::from(height >= 3);
    let guidance = usize::from(
        height >= if error.is_empty() { 4 } else { 5 } + usize::from(!matches.is_empty()),
    );
    let footer = usize::from(height >= 2);
    let body_height = height.saturating_sub(heading + guidance + footer);
    let errors: Vec<_> = super::wrap_modal(error, width)
        .into_iter()
        .filter(|_| !error.is_empty())
        .take(
            body_height
                .saturating_sub(1 + usize::from(!matches.is_empty()))
                .min(3),
        )
        .collect();
    let choices = matches
        .len()
        .min(3)
        .min(body_height.saturating_sub(errors.len() + 1));
    let input_height = body_height.saturating_sub(errors.len() + choices);
    let inputs: Vec<_> = value.split('\n').collect();
    let active = value[..cursor.byte]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let start = active
        .saturating_sub(input_height / 2)
        .min(inputs.len().saturating_sub(input_height));
    let active_start = value[..cursor.byte].rfind('\n').map_or(0, |byte| byte + 1);
    let mut rows = Vec::new();
    if heading > 0 {
        rows.push(PopupRow::new(
            id.map_or_else(
                || "Tags new task".to_owned(),
                |id| format!("Tags task #{id}"),
            ),
            PopupKind::Heading,
        ));
    }
    if guidance > 0 {
        rows.push(PopupRow::new(
            if choices == 0 {
                "One tag per line; blank clears"
            } else if width >= "Tab complete  ↑↓ pick".width() {
                "Tab complete  ↑↓ pick"
            } else {
                "Tab ↑↓"
            },
            PopupKind::Hint,
        ));
    }
    rows.extend(
        inputs
            .into_iter()
            .enumerate()
            .skip(start)
            .take(input_height)
            .map(|(index, line)| {
                if index == active {
                    focused_row(line, cursor.byte - active_start, width)
                } else {
                    PopupRow::new(line, PopupKind::Input)
                }
            }),
    );
    if choices > 0 {
        let selected = cursor
            .completion
            .as_ref()
            .unwrap()
            .selected
            .min(matches.len() - 1);
        let start = selected.saturating_sub(choices - 1);
        rows.extend(
            matches
                .iter()
                .enumerate()
                .skip(start)
                .take(choices)
                .map(|(index, label)| {
                    PopupRow::new(
                        *label,
                        if index == selected {
                            PopupKind::SelectedChoice
                        } else {
                            PopupKind::Choice
                        },
                    )
                }),
        );
    }
    rows.extend(
        errors
            .into_iter()
            .map(|line| PopupRow::new(line, PopupKind::Error)),
    );
    if footer > 0 {
        let full = "Enter line Ctrl-S apply Ctrl-U delete line Esc";
        let compact = "Enter Ctrl-S Ctrl-U Esc";
        let shortcuts = if full.len() <= width {
            full
        } else if compact.len() <= width {
            compact
        } else {
            "↵ ^S ^U Esc"
        };
        rows.push(PopupRow::new(shortcuts, PopupKind::Hint));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_matches_project_tags_without_repeating_other_labels() {
        let value = "frontend\nfr";
        let cursor = Cursor::at_end(value).with_tags(vec![
            "frontier".into(),
            "frontend".into(),
            "framework".into(),
            "framework".into(),
            "backend".into(),
        ]);
        assert_eq!(cursor.matches(value), ["framework", "frontier"]);
        let empty = Cursor::at_end("frontend\n").with_tags(vec!["frontend".into()]);
        assert!(empty.matches("frontend\n").is_empty());
        let cursor = Cursor::at_end("FR").with_tags(vec!["frontend".into()]);
        assert_eq!(cursor.matches("FR"), ["frontend"]);
    }

    #[test]
    fn completion_keeps_longer_prefix_matches_and_alternatives_inside_existing_labels() {
        let cursor = Cursor::at_end("bug").with_tags(vec!["bug".into(), "bugfix".into()]);
        assert_eq!(cursor.matches("bug"), ["bugfix"]);
        let mut cursor =
            Cursor::at_end("frontend").with_tags(vec!["frontend".into(), "framework".into()]);
        cursor.byte = 2;
        assert_eq!(cursor.matches("frontend"), ["framework"]);
    }

    #[test]
    fn completion_accepts_selected_label_at_caret_and_preserves_other_text() {
        let mut value = "keep, frsuffix\n界面".to_owned();
        let mut cursor =
            Cursor::at_end(&value).with_tags(vec!["framework".into(), "frontend".into()]);
        cursor.byte = "keep, fr".len();
        let mut error = "invalid".to_owned();
        cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        );
        assert_eq!(value, "keep, frsuffix\n界面");
        cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert_eq!(value, "keep,frontend\n界面");
        assert_eq!(cursor.byte, "keep,frontend".len());
        assert!(error.is_empty());
        cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(value, "keep,frontend\n\n界面");
    }

    #[test]
    fn completion_leaves_custom_tags_and_multiline_navigation_available() {
        let mut value = "frontend\ncustom".to_owned();
        let mut cursor = Cursor::at_end(&value).with_tags(vec!["frontend".into()]);
        let mut error = String::new();
        cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert_eq!(value, "frontend\ncustom");
        cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        );
        assert_eq!(cursor.byte, 6);
        cursor.end(&value);
        assert!(cursor.matches(&value).is_empty());
        cursor.insert(&mut value, ", new");
        assert_eq!(value, "frontend, new\ncustom");
        assert!(!cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
        ));
    }

    #[test]
    fn completion_keeps_unicode_caret_errors_and_choices_visible_when_resized() {
        let value = "keep\n界";
        let cursor = Cursor::at_end(value).with_tags(vec!["界面".into(), "界限".into()]);
        for width in [12, 28, 46] {
            for height in 1..=18 {
                let rows = rows(Some(1), value, &cursor, "Invalid label", width, height);
                assert!(rows.len() <= height);
                assert!(rows.iter().any(|row| row.cursor.is_some()));
                if height >= 5 {
                    assert!(rows.iter().any(|row| row.kind == PopupKind::SelectedChoice));
                    assert!(rows.iter().any(|row| row.kind == PopupKind::Error));
                }
            }
        }
        let mut value = value.to_owned();
        let mut cursor = cursor;
        cursor.edit(
            &mut value,
            &mut String::new(),
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        );
        assert_eq!(value, "keep\n界限");
        assert_eq!(cursor.byte, value.len());
    }

    #[test]
    fn ctrl_u_removes_only_current_row_at_every_line_boundary() {
        for (input, byte, expected, expected_byte) in [
            ("first\nmiddle\nlast", 2, "middle\nlast", 0),
            ("first\nmiddle\nlast", 8, "first\nlast", 6),
            ("first\nmiddle\nlast", 17, "first\nmiddle", 12),
            ("first\n", 6, "first", 5),
            ("first\n\nlast", 6, "first\nlast", 6),
            ("", 0, "", 0),
            ("e\u{301}👩‍💻", 3, "", 0),
            ("界\ne\u{301}👩‍💻\nlast", 7, "界\nlast", 4),
        ] {
            let mut value = input.to_owned();
            let mut cursor = Cursor::at_end(&value);
            cursor.byte = byte;
            let mut error = "invalid label".to_owned();
            assert!(cursor.edit(
                &mut value,
                &mut error,
                KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            ));
            assert_eq!(value, expected, "input={input:?} byte={byte}");
            assert_eq!(cursor.byte, expected_byte);
            assert!(error.is_empty());
        }
    }

    #[test]
    fn word_shortcuts_match_editor_navigation_and_stop_deletion_at_newline() {
        let text = "alpha  beta\n界 e\u{301}👩‍💻   ";
        let mut value = text.to_owned();
        let mut cursor = Cursor::at_end(&value);
        let mut draft = super::super::draft::Draft::new(text);
        let mut error = String::new();
        for _ in 0..6 {
            assert!(cursor.edit(
                &mut value,
                &mut error,
                KeyEvent::new(KeyCode::Left, KeyModifiers::ALT)
            ));
            draft.previous_word();
            assert_eq!(value[..cursor.byte].graphemes(true).count(), draft.cursor());
        }
        for _ in 0..6 {
            assert!(cursor.edit(
                &mut value,
                &mut error,
                KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)
            ));
            draft.next_word();
            assert_eq!(value[..cursor.byte].graphemes(true).count(), draft.cursor());
        }
        for _ in 0..6 {
            assert!(cursor.edit(
                &mut value,
                &mut error,
                KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)
            ));
            draft.delete_previous_word();
            assert_eq!(value, draft.finish().unwrap().description);
            assert!(value.starts_with("alpha  beta\n"));
        }
    }

    #[test]
    fn editor_keys_keep_caret_on_graphemes_and_leave_save_to_caller() {
        let mut value = "first\ne\u{301}👩‍💻".to_owned();
        let mut cursor = Cursor::at_end(&value);
        let mut error = String::new();
        assert!(cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)
        ));
        assert_eq!(cursor.byte, "first\n".len());
        assert!(cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)
        ));
        assert_eq!(value, "first\n👩‍💻");
        assert!(cursor.edit(
            &mut value,
            &mut error,
            KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL)
        ));
        assert_eq!(cursor.byte, value.len());
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            assert!(cursor.edit(
                &mut value,
                &mut error,
                KeyEvent::new(KeyCode::Enter, modifiers)
            ));
        }
        assert_eq!(value, "first\n👩‍💻\n\n");
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::CONTROL | KeyModifiers::ALT,
            KeyModifiers::CONTROL | KeyModifiers::SUPER,
        ] {
            assert!(!cursor.edit(
                &mut value,
                &mut error,
                KeyEvent::new(KeyCode::Char('s'), modifiers)
            ));
            assert_eq!(value, "first\n👩‍💻\n\n");
        }
    }

    #[test]
    fn vertical_navigation_keeps_preferred_column_across_short_unicode_rows() {
        let value = "abcdef\n界\nabcdef";
        let mut cursor = Cursor::at_end(value);
        cursor.vertical(value, true);
        assert_eq!(&value[..cursor.byte], "abcdef\n界");
        cursor.vertical(value, true);
        assert_eq!(&value[..cursor.byte], "abcdef");
        cursor.vertical(value, true);
        assert_eq!(&value[..cursor.byte], "abcdef");
        cursor.vertical(value, false);
        cursor.vertical(value, false);
        assert_eq!(cursor.byte, value.len());
        cursor.vertical(value, false);
        assert_eq!(cursor.byte, value.len());
    }

    #[test]
    fn selected_row_edits_insert_paste_newline_and_remove_whole_graphemes() {
        let mut value = "first\ne\u{301}👩‍💻\nlast".to_owned();
        let mut cursor = Cursor::at_end(&value);
        cursor.vertical(&value, true);
        cursor.end(&value);
        cursor.backspace(&mut value);
        assert_eq!(value, "first\ne\u{301}\nlast");
        cursor.home(&value);
        cursor.delete(&mut value);
        assert_eq!(value, "first\n\nlast");
        cursor.insert(&mut value, "middle");
        cursor.insert(&mut value, "\n");
        cursor.insert(&mut value, "added");
        assert_eq!(value, "first\nmiddle\nadded\nlast");
        cursor.home(&value);
        cursor.backspace(&mut value);
        assert_eq!(value, "first\nmiddleadded\nlast");
        cursor.left(&value);
        cursor.insert(&mut value, "-");
        assert_eq!(value, "first\nmiddl-eadded\nlast");
    }

    #[test]
    fn horizontal_navigation_and_rendering_keep_long_unicode_caret_visible() {
        let value = "界👩‍💻e\u{301}".repeat(20);
        let mut cursor = Cursor::at_end(&value);
        for _ in 0..3 {
            cursor.left(&value);
        }
        assert_eq!(cursor.byte, value.len() - "界👩‍💻e\u{301}".len());
        for _ in 0..3 {
            cursor.right(&value);
        }
        assert_eq!(cursor.byte, value.len());
        for width in [4, 12, 18, 46] {
            let tail = focused_row(&value, cursor.byte, width);
            assert!(tail.text.width() <= width);
            assert!(tail.cursor.unwrap() < width);
            cursor.home(&value);
            let head = focused_row(&value, cursor.byte, width);
            assert_eq!(head.cursor, Some(0));
            assert!(head.text.starts_with("界"));
            cursor.end(&value);
        }
        let indented = focused_row("  keep", 2, 46);
        assert_eq!(indented.text, "  keep");
        assert_eq!(indented.cursor, Some(2));
    }

    #[test]
    fn active_row_survives_scrolling_and_later_input_rows_do_not_take_cursor() {
        let value = (0..30)
            .map(|index| format!("tag{index:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut cursor = Cursor::at_end(&value);
        for _ in 0..29 {
            cursor.vertical(&value, true);
        }
        for (width, height) in [(72, 24), (24, 12), (18, 8)] {
            for color in [true, false] {
                let area = ratatui::layout::Rect::new(0, 0, width, height);
                let capacity = super::super::dashboard::popup_layout(area, usize::MAX).content;
                let lines = rows(
                    Some(7),
                    &value,
                    &cursor,
                    "",
                    capacity.width.into(),
                    capacity.height.into(),
                );
                let focused = lines.iter().position(|row| row.cursor.is_some()).unwrap();
                assert_eq!(lines[focused].text, "tag00");
                let mut terminal =
                    ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
                        .unwrap();
                terminal
                    .draw(|frame| super::super::dashboard::popup(frame, &lines, color, true))
                    .unwrap();
                let content = super::super::dashboard::popup_layout(area, lines.len()).content;
                assert_eq!(
                    terminal.get_cursor_position().unwrap(),
                    ratatui::layout::Position::new(content.x + 5, content.y + focused as u16)
                );
            }
        }
    }
}
