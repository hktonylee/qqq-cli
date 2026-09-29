use crossterm::{
    cursor::MoveTo,
    queue,
    style::Print,
    terminal::{Clear, ClearType},
};
use std::io::{self, Write};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const KEYS: &str = "Ctrl-S save  Esc cancel  Ctrl-V paste";

pub struct Layout {
    pub rows: Vec<String>,
    pub positions: Vec<(usize, usize)>,
}
impl Layout {
    pub fn new(fragments: &[String], width: usize) -> Self {
        let width = width.max(1);
        let mut layout = Self {
            rows: vec![String::new()],
            positions: vec![(0, 0)],
        };
        let mut column = 0;
        for fragment in fragments {
            let safe = escape(fragment);
            for grapheme in safe.graphemes(true) {
                if grapheme == "\n" {
                    layout.rows.push(String::new());
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
                    column = 0;
                }
                layout
                    .rows
                    .last_mut()
                    .expect("layout always has a row")
                    .push_str(grapheme);
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
    message: &str,
) -> io::Result<()> {
    let (width, height) = size;
    queue!(output, MoveTo(0, 0), Clear(ClearType::All))?;
    if width < 12 || height < 4 {
        queue!(
            output,
            Print(clipped("Resize terminal (min 12x4)", width as usize)),
            MoveTo(0, 0)
        )?;
        return output.flush();
    }
    let body_height = height as usize - 3;
    let (row, column) = layout.positions[cursor];
    if row < *top {
        *top = row;
    }
    if row >= *top + body_height {
        *top = row + 1 - body_height;
    }
    queue!(
        output,
        Print(clipped("qqq task editor", width as usize)),
        MoveTo(0, 1),
        Print(clipped("Whole buffer = task description", width as usize))
    )?;
    for (index, line) in layout.rows.iter().skip(*top).take(body_height).enumerate() {
        queue!(output, MoveTo(0, index as u16 + 2), Print(line))?;
    }
    let footer = if message.is_empty() { KEYS } else { message };
    queue!(
        output,
        MoveTo(0, height - 1),
        Print(clipped(footer, width as usize)),
        MoveTo(column as u16, (row - *top) as u16 + 2)
    )?;
    output.flush()
}
