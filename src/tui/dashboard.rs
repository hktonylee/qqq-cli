use super::{panel, render};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ACCENT: Color = Color::Indexed(81);
const SELECTION_BG: Color = Color::Indexed(24);
const BODY_FG: Color = Color::Indexed(252);
const BODY_BG: Color = Color::Indexed(236);

pub struct DetailsView<'a> {
    pub rows: &'a [String],
    pub top: &'a mut usize,
}

pub struct View<'a> {
    pub query: &'a str,
    pub focused: bool,
    pub top: &'a mut usize,
    pub follow_selected: bool,
    pub modal_lines: Option<&'a [String]>,
    pub details: Option<DetailsView<'a>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WheelArea {
    List(usize),
    Details(usize),
    Editor(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClickTarget {
    Task(i64),
    Editor(usize),
}

pub struct Panes {
    pub list: Rect,
    pub details: Rect,
    pub editor: Rect,
}

pub fn panes(area: Rect) -> Panes {
    let list_height = ((u32::from(area.height) * 35 + 50) / 100) as u16;
    let list_height = list_height.max(4).min(area.height.saturating_sub(4));
    let details_height = ((u32::from(area.height) * 20 + 50) / 100) as u16;
    let details_height = details_height
        .max(1)
        .min(area.height.saturating_sub(list_height + 3));
    let editor_y = area.y + list_height + details_height;
    Panes {
        list: Rect::new(area.x, area.y, area.width, list_height),
        details: Rect::new(area.x, area.y + list_height, area.width, details_height),
        editor: Rect::new(
            area.x,
            editor_y,
            area.width,
            area.height - list_height - details_height,
        ),
    }
}

pub fn details_height(area: Rect) -> usize {
    usize::from(if area.height >= 3 {
        area.height - 1
    } else {
        area.height
    })
}

pub fn wheel_area(size: (u16, u16), column: u16, row: u16) -> Option<WheelArea> {
    if size.0 < 12 || size.1 < 8 || column >= size.0 || row >= size.1 {
        return None;
    }
    let Panes {
        list,
        details,
        editor,
    } = panes(Rect::new(0, 0, size.0, size.1));
    if row < list.y + list.height - 1 {
        Some(WheelArea::List(usize::from(list.height.saturating_sub(3))))
    } else if row >= details.y && usize::from(row - details.y) < details_height(details) {
        Some(WheelArea::Details(details_height(details)))
    } else if row >= editor.y && row < editor.y + editor.height - 1 {
        Some(WheelArea::Editor(usize::from(
            editor.height.saturating_sub(2),
        )))
    } else {
        None
    }
}

pub struct HitState<'a> {
    pub rows: &'a [panel::ListRow],
    pub list_top: usize,
    pub editor_top: usize,
    pub layout: &'a render::Layout,
}

pub fn click_target(
    size: (u16, u16),
    column: u16,
    row: u16,
    hit: HitState<'_>,
) -> Option<ClickTarget> {
    if size.0 < 12 || size.1 < 8 || column >= size.0 || row >= size.1 {
        return None;
    }
    let Panes { list, editor, .. } = panes(Rect::new(0, 0, size.0, size.1));
    if row >= list.y + 2 && row < list.y + list.height - 1 {
        let index = hit.list_top + usize::from(row - list.y - 2);
        return hit.rows.get(index)?.task_id.map(ClickTarget::Task);
    }
    if row > editor.y && row < editor.y + editor.height - 1 {
        let layout_row = hit.editor_top + usize::from(row - editor.y - 1);
        if layout_row < hit.layout.rows.len() {
            return Some(ClickTarget::Editor(
                hit.layout.nearest(layout_row, usize::from(column)),
            ));
        }
    }
    None
}

fn filter_text(query: &str, width: usize) -> (String, u16) {
    const LABEL: &str = "Filter: ";
    let available = width.saturating_sub(LABEL.len());
    let mut suffix = Vec::new();
    let mut used = 0;
    for grapheme in query.graphemes(true).rev() {
        let cells = grapheme.width();
        if used + cells > available {
            break;
        }
        suffix.push(grapheme);
        used += cells;
    }
    suffix.reverse();
    let text = format!("{LABEL}{}", suffix.concat());
    let cursor = (LABEL.len() + used).min(width.saturating_sub(1)) as u16;
    (text, cursor)
}

fn row_style(status: Option<&str>, selected: bool, color: bool) -> Style {
    if !color {
        return Style::default();
    }
    if selected {
        return Style::default().fg(BODY_FG).bg(SELECTION_BG);
    }
    match status {
        Some("in_progress") => Style::default().fg(ACCENT),
        Some("completed") => Style::default().fg(Color::DarkGray),
        Some("error") => Style::default().fg(Color::Red),
        _ => Style::default(),
    }
}

fn editor_line(layout: &render::Layout, index: usize, color: bool) -> Line<'static> {
    let line = &layout.rows[index];
    if !color {
        return Line::from(line.clone());
    }
    let mut spans = Vec::new();
    let mut offset = 0;
    for &(start, end, kind) in &layout.highlight_spans[index] {
        let foreground = match kind {
            render::HighlightKind::Image => ACCENT,
            render::HighlightKind::Paste => Color::Indexed(222),
        };
        spans.push(Span::raw(line[offset..start].to_owned()));
        spans.push(Span::styled(
            line[start..end].to_owned(),
            Style::default().fg(foreground),
        ));
        offset = end;
    }
    spans.push(Span::raw(line[offset..].to_owned()));
    Line::from(spans)
}

fn editor(frame: &mut Frame<'_>, area: Rect, editor: render::DashboardEditor<'_>, color: bool) {
    let body = Rect::new(area.x, area.y + 1, area.width, area.height - 2);
    let (row, column) = editor.layout.positions[editor.cursor];
    let body_height = usize::from(body.height);
    *editor.top = (*editor.top).min(editor.layout.rows.len().saturating_sub(body_height));
    if editor.follow_cursor {
        if row < *editor.top {
            *editor.top = row;
        }
        if row >= editor.top.saturating_add(body_height) {
            *editor.top = row + 1 - body_height;
        }
    }
    let heading_style = if color {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(render::clipped(editor.chrome.title, area.width.into()))
            .style(heading_style),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let body_style = if color {
        Style::default().fg(BODY_FG).bg(BODY_BG)
    } else {
        Style::default()
    };
    let lines: Vec<_> = (0..body_height)
        .filter_map(|offset| {
            let index = editor.top.saturating_add(offset);
            (index < editor.layout.rows.len()).then(|| editor_line(editor.layout, index, color))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).style(body_style), body);
    let footer = if editor.chrome.message.is_empty() {
        editor.chrome.keys
    } else {
        editor.chrome.message
    };
    let footer_style = if !color {
        Style::default()
    } else if editor.chrome.message.is_empty() {
        Style::default().fg(Color::Gray)
    } else if editor.message_is_error {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Yellow)
    };
    frame.render_widget(
        Paragraph::new(render::clipped(footer, area.width.into())).style(footer_style),
        Rect::new(area.x, area.y + area.height - 1, area.width, 1),
    );
    if row >= *editor.top && row < editor.top.saturating_add(body_height) {
        frame.set_cursor_position((
            body.x.saturating_add(column as u16),
            body.y.saturating_add((row - *editor.top) as u16),
        ));
    }
}

fn details(frame: &mut Frame<'_>, area: Rect, view: DetailsView<'_>, color: bool) {
    let height = details_height(area);
    *view.top = (*view.top).min(view.rows.len().saturating_sub(height));
    for (offset, row) in view.rows.iter().skip(*view.top).take(height).enumerate() {
        let style = if color && *view.top == 0 && offset == 0 {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        frame.render_widget(
            Paragraph::new(row.clone()).style(style),
            Rect::new(area.x, area.y + offset as u16, area.width, 1),
        );
    }
    if height < usize::from(area.height) {
        let style = if color {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };
        frame.render_widget(
            Paragraph::new("─".repeat(area.width.into())).style(style),
            Rect::new(area.x, area.y + area.height - 1, area.width, 1),
        );
    }
}

pub fn draw(
    frame: &mut Frame<'_>,
    rows: &[panel::ListRow],
    statuses: &HashMap<i64, &str>,
    selected: Option<i64>,
    list_view: View<'_>,
    editor_state: render::DashboardEditor<'_>,
    color: bool,
) {
    let area = frame.area();
    if area.width < 12 || area.height < 8 {
        frame.render_widget(
            Paragraph::new(render::clipped(
                "Resize terminal (min 12x8)",
                area.width.into(),
            )),
            area,
        );
        frame.set_cursor_position((area.x, area.y));
        return;
    }
    let Panes {
        list,
        details: details_area,
        editor: editor_area,
    } = panes(area);
    let list_height = usize::from(list.height.saturating_sub(3));
    *list_view.top = if list_view.follow_selected {
        panel::scroll_to(rows, selected, *list_view.top, list_height)
    } else {
        (*list_view.top).min(rows.len().saturating_sub(list_height))
    };
    let heading_style = if color {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new("qqq tasks").style(heading_style),
        Rect::new(list.x, list.y, list.width, 1),
    );
    let (filter_label, filter_cursor) = filter_text(list_view.query, usize::from(list.width));
    let filter_style = if color && list_view.focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else if color {
        Style::default().fg(Color::Gray)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(filter_label).style(filter_style),
        Rect::new(list.x, list.y + 1, list.width, 1),
    );
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching tasks."),
            Rect::new(list.x, list.y + 2, list.width, 1),
        );
    }
    for (offset, row) in rows
        .iter()
        .skip(*list_view.top)
        .take(list_height)
        .enumerate()
    {
        let is_selected = selected.is_some() && row.task_id == selected;
        let marker = if is_selected { "> " } else { "  " };
        let style = row_style(
            row.task_id.and_then(|id| statuses.get(&id).copied()),
            is_selected,
            color,
        );
        frame.render_widget(
            Paragraph::new(format!("{marker}{}", row.text)).style(style),
            Rect::new(list.x, list.y + 2 + offset as u16, list.width, 1),
        );
    }
    let separator_style = if color {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new("─".repeat(list.width.into())).style(separator_style),
        Rect::new(list.x, list.y + list.height - 1, list.width, 1),
    );
    let empty_details = ["Select task to view details.".to_owned()];
    let mut empty_top = 0;
    let details_view = list_view.details.unwrap_or(DetailsView {
        rows: &empty_details,
        top: &mut empty_top,
    });
    details(frame, details_area, details_view, color);
    editor(frame, editor_area, editor_state, color);
    if list_view.focused {
        frame.set_cursor_position((list.x + filter_cursor, list.y + 1));
    }
    if let Some(lines) = list_view.modal_lines {
        frame.render_widget(Clear, area);
        let heading_style = if color {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        for (index, line) in lines.iter().take(usize::from(area.height)).enumerate() {
            frame.render_widget(
                Paragraph::new(render::clipped(line, usize::from(area.width))).style(
                    if index == 0 {
                        heading_style
                    } else {
                        Style::default()
                    },
                ),
                Rect::new(area.x, area.y + index as u16, area.width, 1),
            );
        }
        frame.set_cursor_position((area.x, area.y));
    }
}
