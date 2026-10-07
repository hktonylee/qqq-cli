use super::render::{PopupKind, PopupRow};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy)]
pub(super) struct Cursor {
    byte: usize,
    column: Option<usize>,
}

impl Cursor {
    pub(super) fn at_end(value: &str) -> Self {
        Self {
            byte: value.len(),
            column: None,
        }
    }

    fn snap(&mut self, value: &str) {
        self.byte = value
            .grapheme_indices(true)
            .map(|(byte, _)| byte)
            .chain(std::iter::once(value.len()))
            .find(|byte| *byte >= self.byte)
            .unwrap_or(value.len());
        self.column = None;
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
        if caret.saturating_sub(skipped) <= width.saturating_sub(3) {
            break;
        }
        start = index + grapheme.len();
        skipped += grapheme.width();
    }
    let mut row = PopupRow::new(
        format!(
            "> {}",
            super::render::clipped(&safe[start..], width.saturating_sub(2))
        ),
        PopupKind::Input,
    );
    row.cursor = Some((2 + caret.saturating_sub(skipped)).min(width.saturating_sub(1)));
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
            "One tag per line; blank clears",
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
                    PopupRow::new(format!("> {line}"), PopupKind::Input)
                }
            }),
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

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(head.cursor, Some(2));
            assert!(head.text.starts_with("> "));
            cursor.end(&value);
        }
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
                assert_eq!(lines[focused].text, "> tag00");
                let mut terminal =
                    ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
                        .unwrap();
                terminal
                    .draw(|frame| super::super::dashboard::popup(frame, &lines, color))
                    .unwrap();
                let content = super::super::dashboard::popup_layout(area, lines.len()).content;
                assert_eq!(
                    terminal.get_cursor_position().unwrap(),
                    ratatui::layout::Position::new(content.x + 7, content.y + focused as u16)
                );
            }
        }
    }
}
