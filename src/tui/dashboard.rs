use super::{panel, render};
use ratatui::{
    Frame,
    layout::{Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ACCENT: Color = Color::Indexed(81);
// Accent hue with lower lightness/chroma: OKLCH(30% 0.045 223.18).
const SELECTION_BG: Color = Color::Rgb(15, 51, 62);
const BODY_FG: Color = Color::Indexed(252);
const BODY_BG: Color = Color::Indexed(236);
const POPUP_ERROR_FG: Color = Color::Indexed(210);
const POPUP_PROMPT_FG: Color = Color::Indexed(222);

pub struct DetailsView<'a> {
    pub rows: &'a [render::DetailRow],
    pub top: &'a mut usize,
}

pub struct View<'a> {
    pub query: &'a str,
    pub focused: bool,
    pub top: &'a mut usize,
    pub follow_selected: bool,
    pub modal_lines: Option<&'a [render::PopupRow]>,
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

pub struct PopupLayout {
    pub outer: Rect,
    pub content: Rect,
    bordered: bool,
}

pub fn popup_layout(area: Rect, rows: usize) -> PopupLayout {
    let bordered = area.width >= 20 && area.height >= 14;
    let border = usize::from(bordered) * 2;
    let width = if bordered {
        area.width.saturating_sub(4).min(48)
    } else {
        area.width.min(48)
    };
    let max_height = if bordered {
        area.height.saturating_sub(4)
    } else {
        area.height
    };
    let height = rows.saturating_add(border).min(usize::from(max_height)) as u16;
    let outer = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    let inset = u16::from(bordered);
    PopupLayout {
        outer,
        content: outer.inner(Margin::new(inset, inset)),
        bordered,
    }
}

fn popup_row_style(kind: render::PopupKind, title: bool, color: bool) -> Style {
    use render::PopupKind;
    if !color {
        return Style::default();
    }
    let body = Style::default().fg(BODY_FG).bg(BODY_BG);
    match kind {
        PopupKind::Heading => body.fg(ACCENT).add_modifier(Modifier::BOLD),
        PopupKind::Hint => body.fg(Color::Gray),
        PopupKind::Warning => body.fg(POPUP_PROMPT_FG),
        PopupKind::Error => {
            let style = body.fg(POPUP_ERROR_FG);
            if title {
                style.add_modifier(Modifier::BOLD)
            } else {
                style
            }
        }
        PopupKind::Action | PopupKind::Input => body,
        PopupKind::SelectedAction => body.bg(SELECTION_BG),
    }
}

fn popup_row_line(text: String, kind: render::PopupKind, color: bool) -> Line<'static> {
    use render::PopupKind;
    let marker = match kind {
        PopupKind::SelectedAction => "> ",
        PopupKind::Action => "  ",
        _ => "",
    };
    if !color {
        return Line::from(format!("{marker}{text}"));
    }
    match kind {
        PopupKind::Action | PopupKind::SelectedAction => {
            let key_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
            match text.split_once(' ') {
                Some((key, label)) => Line::from(vec![
                    Span::raw(marker.to_owned()),
                    Span::styled(key.to_owned(), key_style),
                    Span::raw(format!(" {label}")),
                ]),
                None => Line::from(vec![
                    Span::raw(marker.to_owned()),
                    Span::styled(text, key_style),
                ]),
            }
        }
        PopupKind::Hint => hotkey_line(&text, true),
        PopupKind::Input => Line::from(vec![
            Span::styled(
                text.chars().take(2).collect::<String>(),
                Style::default().fg(POPUP_PROMPT_FG),
            ),
            Span::raw(text.chars().skip(2).collect::<String>()),
        ]),
        PopupKind::Heading | PopupKind::Error | PopupKind::Warning => Line::from(text),
    }
}

fn popup(frame: &mut Frame<'_>, lines: &[render::PopupRow], color: bool) {
    use render::PopupKind;
    let PopupLayout {
        outer,
        content,
        bordered,
    } = popup_layout(frame.area(), lines.len());
    frame.render_widget(Clear, outer);
    let body_style = popup_row_style(PopupKind::Action, false, color);
    let border_style = if color {
        body_style.fg(Color::DarkGray)
    } else {
        body_style
    };
    let block = Block::default().style(body_style);
    let block = if bordered {
        block.borders(Borders::ALL).border_style(border_style)
    } else {
        block
    };
    frame.render_widget(block, outer);
    let mut cursor = (content.x, content.y);
    for (index, row) in lines.iter().take(usize::from(content.height)).enumerate() {
        let width = usize::from(content.width);
        let text = if row.kind == PopupKind::Input {
            let value = row.text.strip_prefix("> ").unwrap_or(&row.text);
            let safe = render::clipped(value, usize::MAX);
            let (tail, _) = text_tail(&safe, width.saturating_sub(3));
            render::clipped(&format!("> {tail}"), width)
        } else {
            let marker_width = usize::from(matches!(
                row.kind,
                PopupKind::Action | PopupKind::SelectedAction
            )) * 2;
            render::clipped(&row.text, width.saturating_sub(marker_width))
        };
        if row.kind == PopupKind::Input {
            cursor = (
                content.x + (text.width() as u16).min(content.width.saturating_sub(1)),
                content.y + index as u16,
            );
        } else if row.kind == PopupKind::SelectedAction {
            cursor = (content.x, content.y + index as u16);
        }
        let style = popup_row_style(row.kind, index == 0, color);
        let line = popup_row_line(text, row.kind, color);
        frame.render_widget(
            Paragraph::new(line).style(style),
            Rect::new(content.x, content.y + index as u16, content.width, 1),
        );
    }
    frame.set_cursor_position(cursor);
}

pub fn panes(area: Rect) -> Panes {
    if area.width < 50 {
        let list_height = area.height.div_ceil(2);
        let editor_y = area.y + list_height;
        return Panes {
            list: Rect::new(area.x, area.y, area.width, list_height),
            details: Rect::new(area.x, editor_y, area.width, 0),
            editor: Rect::new(area.x, editor_y, area.width, area.height - list_height),
        };
    }
    let list_height = ((u32::from(area.height) * 35 + 50) / 100) as u16;
    let list_height = list_height.max(4).min(area.height.saturating_sub(4));
    let details_height = ((u32::from(area.height) * 20 + 50) / 100) as u16;
    let details_height = details_height
        .max(1)
        .min(area.height.saturating_sub(list_height + 3));
    let editor_y = area.y + list_height + details_height;
    if area.width >= 150 {
        let details_width = ((u32::from(area.width) * 40 + 50) / 100) as u16;
        let list_width = area.width - details_width;
        let upper_height = list_height + details_height;
        return Panes {
            list: Rect::new(area.x, area.y, list_width, upper_height),
            details: Rect::new(area.x + list_width, area.y, details_width, upper_height),
            editor: Rect::new(area.x, editor_y, area.width, area.height - upper_height),
        };
    }
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

fn details_block(area: Rect) -> Block<'static> {
    let block = Block::default().padding(Padding::horizontal(2));
    if area.height >= 3 && area.width >= 8 {
        block.borders(Borders::ALL).border_type(BorderType::Double)
    } else {
        block
    }
}

pub fn details_content(area: Rect) -> Rect {
    details_block(area).inner(area)
}

pub fn details_height(area: Rect) -> usize {
    usize::from(details_content(area).height)
}

fn list_content(area: Rect, filter_visible: bool) -> Rect {
    let filter_height = u16::from(filter_visible);
    Rect::new(
        area.x,
        area.y + filter_height,
        area.width,
        area.height.saturating_sub(filter_height),
    )
}

pub fn wheel_area(
    size: (u16, u16),
    column: u16,
    row: u16,
    filter_visible: bool,
) -> Option<WheelArea> {
    if size.0 < 12 || size.1 < 8 || column >= size.0 || row >= size.1 {
        return None;
    }
    let Panes {
        list,
        details,
        editor,
    } = panes(Rect::new(0, 0, size.0, size.1));
    if list.contains(Position::new(column, row)) {
        Some(WheelArea::List(usize::from(
            list_content(list, filter_visible).height,
        )))
    } else if details_content(details).contains(Position::new(column, row)) {
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
    pub filter_visible: bool,
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
    let content = list_content(list, hit.filter_visible);
    if content.contains(Position::new(column, row)) {
        let index = hit.list_top + usize::from(row - content.y);
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

fn text_tail(text: &str, available: usize) -> (String, usize) {
    let mut suffix = Vec::new();
    let mut used = 0;
    for grapheme in text.graphemes(true).rev() {
        let cells = grapheme.width();
        if used + cells > available {
            break;
        }
        suffix.push(grapheme);
        used += cells;
    }
    suffix.reverse();
    (suffix.concat(), used)
}

fn filter_line(query: &str, width: usize, focused: bool, color: bool) -> (Line<'static>, u16) {
    let label = "Filter: ";
    let (tail, used) = text_tail(query, width.saturating_sub(label.len()));
    let label_style = if color && focused {
        Style::default()
            .fg(ACCENT)
            .bg(Color::Reset)
            .add_modifier(Modifier::BOLD)
    } else if color {
        Style::default().fg(Color::Gray).bg(Color::Reset)
    } else {
        Style::default()
    };
    let line = Line::from(vec![Span::styled(label, label_style), Span::raw(tail)]);
    let cursor = (label.len() + used).min(width.saturating_sub(1)) as u16;
    (line, cursor)
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

fn hotkey_line(keys: &str, color: bool) -> Line<'static> {
    let safe = render::clipped(keys, usize::MAX);
    if !color {
        return Line::from(safe);
    }
    let key_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    for part in safe.split_inclusive(' ') {
        let key = part.trim_end_matches(' ');
        let shortcut = key.starts_with("Ctrl-")
            || key.starts_with("Shift-")
            || matches!(
                key,
                "Ctrl+/"
                    | "Esc"
                    | "Esc/Ctrl-C"
                    | "Backspace"
                    | "Tab/Enter"
                    | "Enter"
                    | "Up/Down"
                    | "Up/Dn"
                    | "y"
                    | "n/Esc"
            );
        spans.push(Span::styled(
            key.to_owned(),
            if shortcut {
                key_style
            } else {
                Style::default()
            },
        ));
        spans.push(Span::raw(part[key.len()..].to_owned()));
    }
    Line::from(spans)
}

fn editor(
    frame: &mut Frame<'_>,
    area: Rect,
    editor: render::DashboardEditor<'_>,
    color: bool,
) -> Option<Position> {
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
        hotkey_line(editor.chrome.keys, color)
    } else {
        Line::from(render::clipped(editor.chrome.message, area.width.into()))
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
        Paragraph::new(footer).style(footer_style),
        Rect::new(area.x, area.y + area.height - 1, area.width, 1),
    );
    if row >= *editor.top && row < editor.top.saturating_add(body_height) {
        Some(Position::new(
            body.x.saturating_add(column as u16),
            body.y.saturating_add((row - *editor.top) as u16),
        ))
    } else {
        None
    }
}

fn detail_style(kind: render::DetailKind, color: bool) -> Style {
    if !color {
        return Style::default();
    }
    match kind {
        render::DetailKind::Section => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        render::DetailKind::Heading => Style::default().add_modifier(Modifier::BOLD),
        render::DetailKind::InProgress => Style::default().fg(Color::Cyan),
        render::DetailKind::Completed => Style::default().fg(Color::DarkGray),
        render::DetailKind::Error => Style::default().fg(Color::Red),
        render::DetailKind::Success => Style::default().fg(Color::Green),
        render::DetailKind::Muted => Style::default().add_modifier(Modifier::DIM),
        render::DetailKind::Warning => Style::default().fg(Color::Yellow),
        render::DetailKind::Body => Style::default(),
    }
}

fn details(frame: &mut Frame<'_>, area: Rect, view: DetailsView<'_>, color: bool) {
    let block = details_block(area).border_style(if color {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    });
    let content = block.inner(area);
    frame.render_widget(block, area);
    let height = usize::from(content.height);
    *view.top = (*view.top).min(view.rows.len().saturating_sub(height));
    for (offset, row) in view.rows.iter().skip(*view.top).take(height).enumerate() {
        let spans = row
            .spans
            .iter()
            .map(|&(start, end, kind)| {
                Span::styled(row.text[start..end].to_owned(), detail_style(kind, color))
            })
            .collect::<Vec<_>>();
        let line = if spans.is_empty() {
            Line::from(row.text.clone())
        } else {
            Line::from(spans)
        };
        frame.render_widget(
            Paragraph::new(line).style(detail_style(row.kind, color)),
            Rect::new(content.x, content.y + offset as u16, content.width, 1),
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
    let filter_visible = list_view.focused || !list_view.query.is_empty();
    let content = list_content(list, filter_visible);
    let list_height = usize::from(content.height);
    *list_view.top = if list_view.follow_selected {
        panel::scroll_to(rows, selected, *list_view.top, list_height)
    } else {
        (*list_view.top).min(rows.len().saturating_sub(list_height))
    };
    if filter_visible {
        let (filter, filter_cursor) = filter_line(
            list_view.query,
            usize::from(list.width),
            list_view.focused,
            color,
        );
        let filter_style = if color {
            Style::default().fg(BODY_FG).bg(BODY_BG)
        } else {
            Style::default()
        };
        frame.render_widget(
            Paragraph::new(filter).style(filter_style),
            Rect::new(list.x, list.y, list.width, 1),
        );
        if list_view.focused {
            frame.set_cursor_position((list.x + filter_cursor, list.y));
        }
    }
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching tasks."),
            Rect::new(content.x, content.y, content.width, 1),
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
            Rect::new(content.x, content.y + offset as u16, content.width, 1),
        );
    }
    let empty_details = [render::DetailRow::new(
        "Select task to view details.",
        render::DetailKind::Muted,
    )];
    let mut empty_top = 0;
    let details_view = list_view.details.unwrap_or(DetailsView {
        rows: &empty_details,
        top: &mut empty_top,
    });
    if details_area.height > 0 {
        details(frame, details_area, details_view, color);
    }
    let editor_cursor = editor(frame, editor_area, editor_state, color);
    if let Some(cursor) = editor_cursor.filter(|_| !list_view.focused) {
        frame.set_cursor_position(cursor);
    }
    if let Some(lines) = list_view.modal_lines {
        popup(frame, lines, color);
    }
}
