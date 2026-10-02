mod clipboard;
pub mod draft;
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
use std::io::{self, IsTerminal};

pub struct Outcome {
    pub composition: Composition,
    pub target_id: Option<i64>,
}

enum Target {
    New,
    Task { id: i64, description: String },
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
    Ok(match db.adjacent_description(current, older)? {
        Some((id, description)) => Some(Target::Task { id, description }),
        None if !older => Some(Target::New),
        None => None,
    })
}

fn load_target(
    target: Target,
    draft: &mut Draft,
    target_id: &mut Option<i64>,
    baseline: &mut String,
    top: &mut usize,
) {
    match target {
        Target::New => {
            *target_id = None;
            baseline.clear();
        }
        Target::Task { id, description } => {
            *target_id = Some(id);
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
pub fn compose(description: &str, navigation: Option<&crate::db::Db>) -> Result<Outcome> {
    let terminal = TerminalGuard::enter()?;
    let mut draft = Draft::new(description);
    let mut baseline = description.to_owned();
    let mut target_id = None;
    let mut top = 0;
    let mut message = String::new();
    let mut confirmation: Option<Confirmation> = None;
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
        let title = match (navigation.is_some(), target_id) {
            (true, Some(id)) => format!("qqq task editor - task #{id}"),
            (true, None) => "qqq task editor - new task".to_owned(),
            (false, _) => "qqq task editor".to_owned(),
        };
        render::draw(
            &mut io::stderr(),
            &layout,
            draft.cursor(),
            &mut top,
            size,
            &render::Chrome {
                title: &title,
                keys: if navigation.is_some() {
                    render::NAV_KEYS
                } else {
                    render::KEYS
                },
                message: footer,
            },
            terminal.color,
        )?;
        match event::read()? {
            Event::Paste(text) if confirmation.is_none() => {
                message = paste(&mut draft, &text)
                    .err()
                    .map_or_else(String::new, |error| format!("{error:#}"));
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                if let Some(pending) = confirmation.take() {
                    if control && key.code == KeyCode::Char('c') {
                        bail!("Editor cancelled; task not saved");
                    }
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    {
                        match key.code {
                            KeyCode::Char('y' | 'Y') => match pending {
                                Confirmation::Exit => bail!("Editor cancelled; task not saved"),
                                Confirmation::Switch(target) => {
                                    load_target(
                                        target,
                                        &mut draft,
                                        &mut target_id,
                                        &mut baseline,
                                        &mut top,
                                    );
                                    message.clear();
                                }
                            },
                            KeyCode::Char('n' | 'N') | KeyCode::Enter | KeyCode::Esc => {
                                message.clear();
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
                    if let Some(db) = navigation {
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
                                    &mut baseline,
                                    &mut top,
                                );
                                message.clear();
                            }
                            Ok(None) => {
                                message = if older {
                                    "No older task".to_owned()
                                } else {
                                    "Already at new task".to_owned()
                                };
                            }
                            Err(error) => message = format!("{error:#}"),
                        }
                        continue;
                    }
                }
                if control {
                    match key.code {
                        KeyCode::Char('s') => match draft.finish() {
                            Ok(composition) => {
                                return Ok(Outcome {
                                    composition,
                                    target_id,
                                });
                            }
                            Err(error) => message = error.to_string(),
                        },
                        KeyCode::Char('c') => bail!("Editor cancelled; task not saved"),
                        KeyCode::Char('v') => {
                            let result = clipboard::read().and_then(|value| match value {
                                clipboard::Paste::Text(text) => paste(&mut draft, &text),
                                clipboard::Paste::Image(image) => draft.image(image),
                            });
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
                match key.code {
                    KeyCode::Esc if draft.is_empty() => bail!("Editor cancelled; task not saved"),
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
