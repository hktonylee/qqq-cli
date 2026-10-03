use super::render::{PopupKind, PopupRow};
use crate::db::{ContentConflict, CurrentContent};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
enum Resolution {
    Reload,
    Overwrite,
}

pub enum Action {
    Keep,
    Reload,
    Overwrite(i64),
}

pub struct View {
    id: i64,
    current: Option<CurrentContent>,
    local: String,
    local_page: bool,
    top: usize,
    confirmation: Option<Resolution>,
}

impl View {
    pub fn new(conflict: &ContentConflict, local: String) -> Self {
        Self {
            id: conflict.task_id,
            current: conflict.current.as_ref().map(|current| CurrentContent {
                description: current.description.clone(),
                revision: current.revision,
            }),
            local,
            local_page: false,
            top: 0,
            confirmation: None,
        }
    }

    pub fn mark_removed(&mut self) {
        self.current = None;
        self.local_page = false;
        self.top = 0;
        self.confirmation = None;
    }

    fn text(&self) -> &str {
        if self.local_page {
            &self.local
        } else {
            self.current
                .as_ref()
                .map_or("Task removed. Local draft kept.", |current| {
                    current.description.as_str()
                })
        }
    }

    fn wrapped(&self, width: usize) -> Vec<String> {
        let fragments: Vec<_> = self.text().graphemes(true).map(str::to_owned).collect();
        super::render::Layout::new(&fragments, &[], width.max(1)).rows
    }

    pub fn rows(&self, width: usize, height: usize) -> Vec<PopupRow> {
        if let Some(resolution) = self.confirmation {
            return vec![
                PopupRow::new(
                    match resolution {
                        Resolution::Reload => "Reload current DB text?",
                        Resolution::Overwrite => "Overwrite DB text?",
                    },
                    PopupKind::Heading,
                ),
                PopupRow::new(
                    match resolution {
                        Resolution::Reload => "Discard local text and pending images",
                        Resolution::Overwrite => "Replace DB text with local draft",
                    },
                    PopupKind::Warning,
                ),
                PopupRow::new("y confirm  n/Esc cancel", PopupKind::Hint),
            ];
        }
        let wrapped = self.wrapped(width);
        let available = height.saturating_sub(3).max(1);
        let start = self.top.min(wrapped.len().saturating_sub(available));
        let title = if self.local_page {
            "Local draft".to_owned()
        } else {
            self.current
                .as_ref()
                .map_or("Current DB: task removed".to_owned(), |current| {
                    format!("Current DB revision {}", current.revision)
                })
        };
        let mut rows = vec![
            PopupRow::new(format!("Content conflict #{}", self.id), PopupKind::Error),
            PopupRow::new(title, PopupKind::Heading),
        ];
        rows.extend(
            wrapped
                .into_iter()
                .skip(start)
                .take(available)
                .map(|text| PopupRow::new(text, PopupKind::Body)),
        );
        rows.push(PopupRow::new(
            if self.current.is_some() {
                "Tab text  r reload  o overwrite  Esc keep"
            } else {
                "Tab text  Esc keep"
            },
            PopupKind::Hint,
        ));
        rows
    }

    pub fn key(&mut self, key: KeyEvent, width: usize, height: usize) -> Option<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.confirmation = None;
            return Some(Action::Keep);
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
        {
            return None;
        }
        if let Some(resolution) = self.confirmation {
            match key.code {
                KeyCode::Char('y' | 'Y') => {
                    self.confirmation = None;
                    return match resolution {
                        Resolution::Reload => Some(Action::Reload),
                        Resolution::Overwrite => self
                            .current
                            .as_ref()
                            .map(|current| Action::Overwrite(current.revision)),
                    };
                }
                KeyCode::Char('n' | 'N') | KeyCode::Enter | KeyCode::Esc => {
                    self.confirmation = None
                }
                _ => (),
            }
            return None;
        }
        let page = height.saturating_sub(3).max(1);
        let max_top = self.wrapped(width).len().saturating_sub(page);
        match key.code {
            KeyCode::Esc => return Some(Action::Keep),
            KeyCode::Tab | KeyCode::BackTab => {
                self.local_page = !self.local_page;
                self.top = 0;
            }
            KeyCode::Up => self.top = self.top.saturating_sub(1),
            KeyCode::Down => self.top = (self.top + 1).min(max_top),
            KeyCode::PageUp => self.top = self.top.saturating_sub(page),
            KeyCode::PageDown => self.top = self.top.saturating_add(page).min(max_top),
            KeyCode::Home => self.top = 0,
            KeyCode::End => self.top = max_top,
            KeyCode::Char('r' | 'R') if self.current.is_some() => {
                self.confirmation = Some(Resolution::Reload)
            }
            KeyCode::Char('o' | 'O') if self.current.is_some() => {
                self.confirmation = Some(Resolution::Overwrite)
            }
            _ => (),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::View;
    use crate::db::{ContentConflict, CurrentContent};
    use crate::tui::dashboard::{popup, popup_layout};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    #[test]
    fn current_and_local_wrap_edges_remain_visible_in_popup() {
        for width in [12, 20, 59, 72] {
            let area = Rect::new(0, 0, width, 24);
            let columns = usize::from(popup_layout(area, usize::MAX).content.width);
            let edge = format!("   {}XY", ".".repeat(columns - 5));
            let text = format!("{edge}Z\nFull tail");
            let mut view = View::new(
                &ContentConflict {
                    task_id: 2,
                    expected_revision: 1,
                    current: Some(CurrentContent {
                        description: text.clone(),
                        revision: 2,
                    }),
                },
                text,
            );
            for local in [false, true] {
                view.local_page = local;
                let lines = view.rows(columns, 18);
                let content = popup_layout(area, lines.len()).content;
                for color in [false, true] {
                    let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
                    terminal.draw(|frame| popup(frame, &lines, color)).unwrap();
                    let buffer = terminal.backend().buffer();
                    let row: String = (content.x..content.x + content.width)
                        .map(|x| buffer[(x, content.y + 2)].symbol())
                        .collect();
                    assert_eq!(row, edge, "width={width}, local={local}, color={color}");
                    assert_eq!(buffer[(content.x, content.y + 3)].symbol(), "Z");
                }
            }
        }
    }
}
