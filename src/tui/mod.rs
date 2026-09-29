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
pub fn compose(description: &str) -> Result<Composition> {
    let terminal = TerminalGuard::enter()?;
    let mut draft = Draft::new(description);
    let mut top = 0;
    let mut message = String::new();
    let mut confirm_discard = false;
    loop {
        let size = terminal::size()?;
        let layout = render::Layout::new(&draft.fragments(), size.0 as usize);
        let footer = if confirm_discard {
            if usize::from(size.0) < "Discard draft? (y/N)".len() {
                "Discard? y/N"
            } else {
                "Discard draft? (y/N)"
            }
        } else {
            &message
        };
        render::draw(
            &mut io::stderr(),
            &layout,
            draft.cursor(),
            &mut top,
            size,
            footer,
            terminal.color,
        )?;
        match event::read()? {
            Event::Paste(text) if !confirm_discard => {
                message = paste(&mut draft, &text)
                    .err()
                    .map_or_else(String::new, |error| format!("{error:#}"));
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                if confirm_discard {
                    if control && key.code == KeyCode::Char('c') {
                        bail!("Editor cancelled; task not saved");
                    }
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    {
                        match key.code {
                            KeyCode::Char('y' | 'Y') => bail!("Editor cancelled; task not saved"),
                            KeyCode::Char('n' | 'N') | KeyCode::Enter | KeyCode::Esc => {
                                confirm_discard = false;
                                message.clear();
                            }
                            _ => (),
                        }
                    }
                    continue;
                }
                if control {
                    match key.code {
                        KeyCode::Char('s') => match draft.finish() {
                            Ok(composition) => return Ok(composition),
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
                        _ => (),
                    }
                    continue;
                }
                message.clear();
                match key.code {
                    KeyCode::Esc if draft.is_empty() => bail!("Editor cancelled; task not saved"),
                    KeyCode::Esc => confirm_discard = true,
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
