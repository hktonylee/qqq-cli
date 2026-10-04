use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::io::{self, Write};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const KEYS: &str = "Ctrl-S Save  Esc Cancel  Ctrl-V Paste";
pub const NAV_KEYS: &str = "Ctrl-S Save  Shift-Up/Dn Switch Tasks  Esc Cancel";
pub const ADD_KEYS: &str = "Ctrl-S Save  Shift-Up/Dn Switch Tasks  Esc/Ctrl-C Exit";
pub const DASHBOARD_KEYS: &str = "Ctrl-S Save  Ctrl-L Go to Task  Ctrl-P Create Child  Ctrl-G Menu  Shift-Up/Dn Switch Tasks  Ctrl+/ Filter";
pub const DASHBOARD_HERDR_KEYS: &str = "Ctrl-S Save  Ctrl-H Herdr  Ctrl-L Go to Task  Ctrl-P Create Child  Ctrl-G Menu  Shift-Up/Dn Switch Tasks  Ctrl+/ Filter";
pub const FILTER_KEYS: &str = "Type to Filter  Ctrl-L Go to Task  Ctrl-T Completed  Backspace Edit  Esc Clear/Close  Tab/Enter Editor";
pub const FILTER_HERDR_KEYS: &str = "Type to Filter  Ctrl-L Go to Task  Ctrl-T Completed  Ctrl-H Herdr  Backspace Edit  Esc Clear/Close  Tab/Enter Editor";
const BACKGROUND: Color = Color::AnsiValue(236);
const FOREGROUND: Color = Color::AnsiValue(252);
const IMAGE_FOREGROUND: Color = Color::AnsiValue(81);
const PASTE_FOREGROUND: Color = Color::AnsiValue(222);

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum HighlightKind {
    Image,
    Paste,
}

pub struct Chrome<'a> {
    pub title: &'a str,
    pub title_status_color: Option<(usize, &'static str)>,
    pub keys: &'a str,
    pub message: &'a str,
}

pub struct DashboardEditor<'a> {
    pub layout: &'a Layout,
    pub cursor: usize,
    pub top: &'a mut usize,
    pub chrome: &'a Chrome<'a>,
    pub message_is_error: bool,
    pub follow_cursor: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetailKind {
    Heading,
    Section,
    InProgress,
    Completed,
    Error,
    Success,
    Body,
    Muted,
    Warning,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailRow {
    pub text: String,
    pub kind: DetailKind,
    pub spans: Vec<(usize, usize, DetailKind)>,
}

impl DetailRow {
    pub fn new(text: impl Into<String>, kind: DetailKind) -> Self {
        Self {
            text: text.into(),
            kind,
            spans: Vec::new(),
        }
    }

    pub fn styled(parts: impl IntoIterator<Item = (String, DetailKind)>) -> Self {
        let mut row = Self::new("", DetailKind::Body);
        for (text, kind) in parts {
            row.push(&text, kind);
        }
        row
    }

    pub(super) fn push(&mut self, text: &str, kind: DetailKind) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        match self.spans.last_mut() {
            Some((_, end, previous)) if *previous == kind => *end = self.text.len(),
            _ => self.spans.push((start, self.text.len(), kind)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopupKind {
    Heading,
    Body,
    Action,
    SelectedAction,
    Hint,
    Input,
    Error,
    Warning,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PopupRow {
    pub text: String,
    pub kind: PopupKind,
}

impl PopupRow {
    pub fn new(text: impl Into<String>, kind: PopupKind) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }
}

pub struct Layout {
    pub rows: Vec<String>,
    pub positions: Vec<(usize, usize)>,
    pub(super) highlight_spans: Vec<Vec<(usize, usize, HighlightKind)>>,
}
impl Layout {
    pub fn new(fragments: &[String], image_mask: &[bool], width: usize) -> Self {
        Self::with_paste(fragments, image_mask, &[], width)
    }

    pub fn with_paste(
        fragments: &[String],
        image_mask: &[bool],
        paste_mask: &[bool],
        width: usize,
    ) -> Self {
        let width = width.max(1);
        let mut layout = Self {
            rows: vec![String::new()],
            positions: vec![(0, 0)],
            highlight_spans: vec![Vec::new()],
        };
        let mut column = 0;
        for (index, fragment) in fragments.iter().enumerate() {
            let highlight = if image_mask.get(index).copied().unwrap_or(false) {
                Some(HighlightKind::Image)
            } else if paste_mask.get(index).copied().unwrap_or(false) {
                Some(HighlightKind::Paste)
            } else {
                None
            };
            let safe = escape(fragment);
            for grapheme in safe.graphemes(true) {
                if grapheme == "\n" {
                    layout.rows.push(String::new());
                    layout.highlight_spans.push(Vec::new());
                    column = 0;
                    continue;
                }
                let (grapheme, cells) = if grapheme.width() > width {
                    ("?", 1)
                } else {
                    (grapheme, grapheme.width())
                };
                if column + cells > width {
                    layout.rows.push(String::new());
                    layout.highlight_spans.push(Vec::new());
                    column = 0;
                }
                let line = layout.rows.last_mut().expect("layout always has a row");
                let start = line.len();
                line.push_str(grapheme);
                if let Some(kind) = highlight {
                    let spans = layout
                        .highlight_spans
                        .last_mut()
                        .expect("layout always has highlight spans");
                    match spans.last_mut() {
                        Some((_, end, previous)) if *end == start && *previous == kind => {
                            *end = line.len()
                        }
                        _ => spans.push((start, line.len(), kind)),
                    }
                }
                column += cells;
            }
            layout.positions.push(if column == width {
                (layout.rows.len(), 0)
            } else {
                (layout.rows.len() - 1, column)
            });
        }
        if layout
            .positions
            .last()
            .is_some_and(|(row, _)| *row == layout.rows.len())
        {
            layout.rows.push(String::new());
            layout.highlight_spans.push(Vec::new());
        }
        layout
    }
    pub fn nearest(&self, row: usize, column: usize) -> usize {
        self.positions
            .iter()
            .enumerate()
            .min_by_key(|(index, (r, c))| {
                (
                    r.abs_diff(row),
                    c.abs_diff(column),
                    std::cmp::Reverse(*index),
                )
            })
            .map_or(0, |(index, _)| index)
    }
}
pub(super) fn escape(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .flat_map(|c| {
            if c.is_control() && c != '\n' {
                c.escape_unicode().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}
pub(super) fn clipped(text: &str, width: usize) -> String {
    let safe = escape(text).replace('\n', " ");
    let mut result = String::new();
    let mut cells = 0;
    for grapheme in safe.graphemes(true) {
        cells += grapheme.width();
        if cells > width {
            break;
        }
        result.push_str(grapheme);
    }
    result
}
fn paint_editor(
    output: &mut impl Write,
    layout: &Layout,
    cursor: usize,
    top: &mut usize,
    area: (u16, u16, u16),
    chrome: &Chrome<'_>,
    color: bool,
) -> io::Result<()> {
    let (width, start, height) = area;
    let body_height = height as usize - 2;
    let (row, column) = layout.positions[cursor];
    if row < *top {
        *top = row;
    }
    if row >= *top + body_height {
        *top = row + 1 - body_height;
    }
    queue!(output, MoveTo(0, start))?;
    if let Some((status_start, code)) = chrome.title_status_color.filter(|_| color) {
        let (prefix, status) = chrome.title.split_at(status_start);
        let prefix = clipped(prefix, width as usize);
        let status = clipped(status, (width as usize).saturating_sub(prefix.width()));
        queue!(output, Print(prefix))?;
        if !status.is_empty() {
            queue!(
                output,
                Print(format!("\x1b[{code}m")),
                Print(status),
                ResetColor
            )?;
        }
    } else {
        queue!(output, Print(clipped(chrome.title, width as usize)))?;
    }
    if color {
        queue!(
            output,
            SetBackgroundColor(BACKGROUND),
            SetForegroundColor(FOREGROUND)
        )?;
        // Paint every editor cell, including blank rows; keep terminal chrome default.
        let blank = " ".repeat(width as usize);
        for row in start + 1..start + height - 1 {
            queue!(output, MoveTo(0, row), Print(&blank))?;
        }
    }
    for (index, line) in layout.rows.iter().skip(*top).take(body_height).enumerate() {
        queue!(output, MoveTo(0, start + index as u16 + 1))?;
        if color {
            let mut offset = 0;
            for &(start, end, kind) in &layout.highlight_spans[*top + index] {
                let foreground = match kind {
                    HighlightKind::Image => IMAGE_FOREGROUND,
                    HighlightKind::Paste => PASTE_FOREGROUND,
                };
                queue!(
                    output,
                    Print(&line[offset..start]),
                    SetForegroundColor(foreground),
                    Print(&line[start..end]),
                    SetForegroundColor(FOREGROUND)
                )?;
                offset = end;
            }
            queue!(output, Print(&line[offset..]))?;
        } else {
            queue!(output, Print(line))?;
        }
    }
    if color {
        queue!(output, ResetColor)?;
    }
    let footer = if chrome.message.is_empty() {
        chrome.keys
    } else {
        chrome.message
    };
    queue!(
        output,
        MoveTo(0, start + height - 1),
        Print(clipped(footer, width as usize)),
        MoveTo(column as u16, start + (row - *top) as u16 + 1)
    )?;
    Ok(())
}

pub fn draw(
    output: &mut impl Write,
    layout: &Layout,
    cursor: usize,
    top: &mut usize,
    size: (u16, u16),
    chrome: &Chrome<'_>,
    color: bool,
) -> io::Result<()> {
    let (width, height) = size;
    if color {
        queue!(output, ResetColor)?;
    }
    queue!(output, MoveTo(0, 0), Clear(ClearType::All))?;
    if width < 12 || height < 4 {
        queue!(
            output,
            Print(clipped("Resize terminal (min 12x4)", width as usize)),
            MoveTo(0, 0)
        )?;
        return output.flush();
    }
    paint_editor(
        output,
        layout,
        cursor,
        top,
        (width, 0, height),
        chrome,
        color,
    )?;
    output.flush()
}
