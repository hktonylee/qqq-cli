mod clipboard;
mod dashboard;
pub mod draft;
mod panel;
mod render;

use anyhow::{Result, bail, ensure};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
    style::ResetColor,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use draft::{Composition, Draft};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::collections::HashMap;
use std::io::{self, IsTerminal};

pub struct Outcome {
    pub composition: Composition,
    pub target_id: Option<i64>,
}

enum Target {
    New,
    Task {
        id: i64,
        description: String,
        status: String,
    },
}

enum Confirmation {
    Exit,
    Switch(Target),
}

fn adjacent_target(
    db: &crate::db::Db,
    current: Option<i64>,
    older: bool,
) -> Result<Option<Target>> {
    if !older && current.is_none() {
        return Ok(None);
    }
    Ok(match db.adjacent_task(current, older)? {
        Some((id, description, status)) => Some(Target::Task {
            id,
            description,
            status,
        }),
        None if !older => Some(Target::New),
        None => None,
    })
}

fn load_target(
    target: Target,
    draft: &mut Draft,
    target_id: &mut Option<i64>,
    target_status: &mut Option<String>,
    baseline: &mut String,
    top: &mut usize,
) {
    match target {
        Target::New => {
            *target_id = None;
            *target_status = None;
            baseline.clear();
        }
        Target::Task {
            id,
            description,
            status,
        } => {
            *target_id = Some(id);
            *target_status = Some(status);
            *baseline = description;
        }
    }
    *draft = Draft::new(baseline);
    draft.set_cursor(0);
    *top = 0;
}

struct TerminalGuard {
    color: bool,
}
impl TerminalGuard {
    fn enter() -> Result<Self> {
        ensure!(
            io::stdin().is_terminal() && io::stderr().is_terminal(),
            "Interactive editor requires terminal input and stderr"
        );
        terminal::enable_raw_mode()?;
        let guard = Self {
            color: crate::output::color_enabled(io::stderr().is_terminal()),
        };
        execute!(
            io::stderr(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            Show
        )?;
        Ok(guard)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.color {
            let _ = execute!(io::stderr(), ResetColor);
        }
        let _ = execute!(
            io::stderr(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            Show
        );
        let _ = terminal::disable_raw_mode();
    }
}
fn paste(draft: &mut Draft, text: &str) -> Result<()> {
    if let Some(image) = clipboard::path_image(text)? {
        draft.image(image)?;
    } else {
        draft.paste(text);
    }
    Ok(())
}
enum Mode<'a, 'b> {
    Single(Option<&'a crate::db::Db>),
    Continuous {
        db: &'a mut crate::db::Db,
        save: &'b mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
        dashboard: bool,
    },
}
impl Mode<'_, '_> {
    fn db(&self) -> Option<&crate::db::Db> {
        match self {
            Self::Single(db) => *db,
            Self::Continuous { db, .. } => Some(db),
        }
    }
}
fn cancel(saved_any: bool, dashboard: bool) -> Result<Option<Outcome>> {
    if saved_any || dashboard {
        Ok(None)
    } else {
        bail!("Editor cancelled; task not saved")
    }
}
pub fn compose(description: &str, navigation: Option<&crate::db::Db>) -> Result<Outcome> {
    compose_inner(description, Mode::Single(navigation))
        .map(|saved| saved.expect("single editor returns saved composition"))
}
pub fn compose_continuously(
    db: &mut crate::db::Db,
    save: &mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
) -> Result<()> {
    compose_inner(
        "",
        Mode::Continuous {
            db,
            save,
            dashboard: false,
        },
    )
    .map(|_| ())
}
pub fn compose_dashboard(
    db: &mut crate::db::Db,
    save: &mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
) -> Result<()> {
    compose_inner(
        "",
        Mode::Continuous {
            db,
            save,
            dashboard: true,
        },
    )
    .map(|_| ())
}
fn compose_inner(description: &str, mut mode: Mode<'_, '_>) -> Result<Option<Outcome>> {
    let terminal = TerminalGuard::enter()?;
    let dashboard = matches!(
        mode,
        Mode::Continuous {
            dashboard: true,
            ..
        }
    );
    let mut dashboard_terminal = if dashboard {
        Some(Terminal::new(CrosstermBackend::new(io::stderr()))?)
    } else {
        None
    };
    let mut draft = Draft::new(description);
    let mut baseline = description.to_owned();
    let mut target_id = None;
    let mut target_status = None;
    let mut top = 0;
    let mut list_top = 0;
    let mut message = String::new();
    let mut message_is_error = false;
    let mut confirmation: Option<Confirmation> = None;
    let mut saved_any = false;
    loop {
        let size = terminal::size()?;
        let layout = render::Layout::new(&draft.fragments(), &draft.image_mask(), size.0 as usize);
        let footer = match &confirmation {
            Some(Confirmation::Exit) if usize::from(size.0) < "Discard draft? (y/N)".len() => {
                "Discard? y/N"
            }
            Some(Confirmation::Exit) => "Discard draft? (y/N)",
            Some(Confirmation::Switch(_))
                if usize::from(size.0) < "Discard changes and switch? (y/N)".len() =>
            {
                "Switch? y/N"
            }
            Some(Confirmation::Switch(_)) => "Discard changes and switch? (y/N)",
            None => &message,
        };
        let title = match (mode.db().is_some(), target_id) {
            (true, Some(id)) => format!(
                "qqq task editor - task #{id} ({})",
                crate::output::status_label(
                    target_status.as_deref().expect("selected task has status")
                )
            ),
            (true, None) => "qqq task editor - new task".to_owned(),
            (false, _) => "qqq task editor".to_owned(),
        };
        let chrome = render::Chrome {
            title: &title,
            keys: match &mode {
                Mode::Continuous { .. } => render::ADD_KEYS,
                Mode::Single(Some(_)) => render::NAV_KEYS,
                Mode::Single(None) => render::KEYS,
            },
            message: footer,
        };
        if dashboard {
            let tasks = mode.db().expect("dashboard has database").list(None)?;
            let statuses: HashMap<_, _> = tasks
                .iter()
                .map(|task| (task.id, task.status.as_str()))
                .collect();
            let tree = crate::output::render(
                crate::output::Format::Tasks,
                &serde_json::json!(tasks),
                false,
                Some(usize::from(size.0).saturating_sub(2)),
            );
            let rows = panel::rows(&tree);
            dashboard_terminal
                .as_mut()
                .expect("dashboard has Ratatui terminal")
                .draw(|frame| {
                    dashboard::draw(
                        frame,
                        &rows,
                        &statuses,
                        target_id,
                        &mut list_top,
                        render::DashboardEditor {
                            layout: &layout,
                            cursor: draft.cursor(),
                            top: &mut top,
                            chrome: &chrome,
                            message_is_error: confirmation.is_none() && message_is_error,
                        },
                        terminal.color,
                    );
                })?;
        } else {
            render::draw(
                &mut io::stderr(),
                &layout,
                draft.cursor(),
                &mut top,
                size,
                &chrome,
                terminal.color,
            )?;
        }
        match event::read()? {
            Event::Paste(text) if confirmation.is_none() => {
                let result = paste(&mut draft, &text);
                message_is_error = result.is_err();
                message = result
                    .err()
                    .map_or_else(String::new, |error| format!("{error:#}"));
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                if let Some(pending) = confirmation.take() {
                    if control && key.code == KeyCode::Char('c') {
                        return cancel(saved_any, dashboard);
                    }
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    {
                        match key.code {
                            KeyCode::Char('y' | 'Y') => match pending {
                                Confirmation::Exit => return cancel(saved_any, dashboard),
                                Confirmation::Switch(target) => {
                                    load_target(
                                        target,
                                        &mut draft,
                                        &mut target_id,
                                        &mut target_status,
                                        &mut baseline,
                                        &mut top,
                                    );
                                    message.clear();
                                    message_is_error = false;
                                }
                            },
                            KeyCode::Char('n' | 'N') | KeyCode::Enter | KeyCode::Esc => {
                                message.clear();
                                message_is_error = false;
                            }
                            _ => confirmation = Some(pending),
                        }
                    } else {
                        confirmation = Some(pending);
                    }
                    continue;
                }
                if key.modifiers.contains(KeyModifiers::SHIFT)
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    && matches!(key.code, KeyCode::Up | KeyCode::Down)
                {
                    if let Some(db) = mode.db() {
                        let older = key.code == KeyCode::Up;
                        match adjacent_target(db, target_id, older) {
                            Ok(Some(target)) if draft.is_dirty_against(&baseline) => {
                                confirmation = Some(Confirmation::Switch(target));
                            }
                            Ok(Some(target)) => {
                                load_target(
                                    target,
                                    &mut draft,
                                    &mut target_id,
                                    &mut target_status,
                                    &mut baseline,
                                    &mut top,
                                );
                                message.clear();
                                message_is_error = false;
                            }
                            Ok(None) => {
                                message_is_error = false;
                                message = if older {
                                    "No older task".to_owned()
                                } else {
                                    "Already at new task".to_owned()
                                };
                            }
                            Err(error) => {
                                message_is_error = true;
                                message = format!("{error:#}");
                            }
                        }
                        continue;
                    }
                }
                if control {
                    match key.code {
                        KeyCode::Char('s') => match draft.finish() {
                            Ok(composition) => {
                                let outcome = Outcome {
                                    composition,
                                    target_id,
                                };
                                match &mut mode {
                                    Mode::Single(_) => return Ok(Some(outcome)),
                                    Mode::Continuous { db, save, .. } => match save(db, outcome) {
                                        Ok(id) => {
                                            saved_any = true;
                                            load_target(
                                                Target::New,
                                                &mut draft,
                                                &mut target_id,
                                                &mut target_status,
                                                &mut baseline,
                                                &mut top,
                                            );
                                            message = format!("Saved #{id}. New task");
                                            message_is_error = false;
                                        }
                                        Err(error) => {
                                            message = format!("{error:#}");
                                            message_is_error = true;
                                        }
                                    },
                                }
                            }
                            Err(error) => {
                                message = error.to_string();
                                message_is_error = true;
                            }
                        },
                        KeyCode::Char('c') => return cancel(saved_any, dashboard),
                        KeyCode::Char('v') => {
                            let result = clipboard::read().and_then(|value| match value {
                                clipboard::Paste::Text(text) => paste(&mut draft, &text),
                                clipboard::Paste::Image(image) => draft.image(image),
                            });
                            message_is_error = result.is_err();
                            message = result
                                .err()
                                .map_or_else(String::new, |error| format!("{error:#}"));
                        }
                        KeyCode::Char('a') => draft.home(),
                        KeyCode::Char('e') => draft.end(),
                        KeyCode::Char('w') => draft.delete_previous_word(),
                        _ => (),
                    }
                    continue;
                }
                message.clear();
                message_is_error = false;
                match key.code {
                    KeyCode::Esc if draft.is_empty() => return cancel(saved_any, dashboard),
                    KeyCode::Esc => confirmation = Some(Confirmation::Exit),
                    KeyCode::Char(character)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER) =>
                    {
                        draft.insert(&character.to_string())
                    }
                    KeyCode::Enter => draft.insert("\n"),
                    KeyCode::Tab => draft.insert("\t"),
                    KeyCode::Backspace => draft.backspace(),
                    KeyCode::Delete => draft.delete(),
                    KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                        draft.previous_word()
                    }
                    KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                        draft.next_word()
                    }
                    KeyCode::Left => draft.left(),
                    KeyCode::Right => draft.right(),
                    KeyCode::Home => draft.home(),
                    KeyCode::End => draft.end(),
                    KeyCode::Up | KeyCode::Down => {
                        let (row, column) = layout.positions[draft.cursor()];
                        let target = if key.code == KeyCode::Up {
                            row.saturating_sub(1)
                        } else {
                            row + 1
                        };
                        let cursor = layout.nearest(target, column);
                        if cursor != draft.cursor() {
                            draft.set_cursor(cursor);
                        } else if key.code == KeyCode::Down {
                            draft.right();
                        } else {
                            draft.left();
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }
}
