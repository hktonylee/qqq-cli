use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::io::{self, Write};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const KEYS: &str = "Ctrl-S save  Esc cancel  Ctrl-V paste  Ctrl-W word";
pub const NAV_KEYS: &str = "Ctrl-S save  Shift-Up/Down tasks  Esc cancel  Ctrl-W word";
pub const ADD_KEYS: &str = "Ctrl-S save  Shift-Up/Down  Esc/Ctrl-C exit  Ctrl-W word";
const BACKGROUND: Color = Color::AnsiValue(236);
const FOREGROUND: Color = Color::AnsiValue(252);
const IMAGE_FOREGROUND: Color = Color::AnsiValue(81);

pub struct Chrome<'a> {
    pub title: &'a str,
    pub keys: &'a str,
    pub message: &'a str,
}

pub struct Layout {
    pub rows: Vec<String>,
    pub positions: Vec<(usize, usize)>,
    image_spans: Vec<Vec<(usize, usize)>>,
}
impl Layout {
    pub fn new(fragments: &[String], image_mask: &[bool], width: usize) -> Self {
        let width = width.max(1);
        let mut layout = Self {
            rows: vec![String::new()],
            positions: vec![(0, 0)],
            image_spans: vec![Vec::new()],
        };
        let mut column = 0;
        for (index, fragment) in fragments.iter().enumerate() {
            let image = image_mask.get(index).copied().unwrap_or(false);
            let safe = escape(fragment);
            for grapheme in safe.graphemes(true) {
                if grapheme == "\n" {
                    layout.rows.push(String::new());
                    layout.image_spans.push(Vec::new());
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
                    layout.image_spans.push(Vec::new());
                    column = 0;
                }
                let line = layout.rows.last_mut().expect("layout always has a row");
                let start = line.len();
                line.push_str(grapheme);
                if image {
                    let spans = layout
                        .image_spans
                        .last_mut()
                        .expect("layout always has image spans");
                    match spans.last_mut() {
                        Some((_, end)) if *end == start => *end = line.len(),
                        _ => spans.push((start, line.len())),
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
            layout.image_spans.push(Vec::new());
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
fn escape(text: &str) -> String {
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
fn clipped(text: &str, width: usize) -> String {
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
    let body_height = height as usize - 2;
    let (row, column) = layout.positions[cursor];
    if row < *top {
        *top = row;
    }
    if row >= *top + body_height {
        *top = row + 1 - body_height;
    }
    queue!(output, Print(clipped(chrome.title, width as usize)))?;
    if color {
        queue!(
            output,
            SetBackgroundColor(BACKGROUND),
            SetForegroundColor(FOREGROUND)
        )?;
        // Paint every editor cell, including blank rows; keep terminal chrome default.
        let blank = " ".repeat(width as usize);
        for row in 1..height - 1 {
            queue!(output, MoveTo(0, row), Print(&blank))?;
        }
    }
    for (index, line) in layout.rows.iter().skip(*top).take(body_height).enumerate() {
        queue!(output, MoveTo(0, index as u16 + 1))?;
        if color {
            let mut offset = 0;
            for &(start, end) in &layout.image_spans[*top + index] {
                queue!(
                    output,
                    Print(&line[offset..start]),
                    SetForegroundColor(IMAGE_FOREGROUND),
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
        MoveTo(0, height - 1),
        Print(clipped(footer, width as usize)),
        MoveTo(column as u16, (row - *top) as u16 + 1)
    )?;
    output.flush()
}
