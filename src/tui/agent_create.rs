use super::{
    render::{PopupKind, PopupRow},
    tag_input,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

pub(super) const DEFAULT_PROMPT: &str = "/goal Use the `qqq cli --wait`, to look for next task, and work on that. Repeat this forever. Please don't wake up repeatedly to poll the status when running waiter.";

pub(super) enum Action {
    Create(String),
    Cancel,
}

pub(super) struct View {
    root: PathBuf,
    value: String,
    cursor: tag_input::Cursor,
    error: String,
}

impl View {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            value: DEFAULT_PROMPT.into(),
            cursor: tag_input::Cursor::at_end(DEFAULT_PROMPT),
            error: String::new(),
        }
    }

    pub fn paste(&mut self, text: &str) {
        self.cursor
            .insert(&mut self.value, &text.replace("\r\n", "\n"));
        self.error.clear();
    }

    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.code == KeyCode::Esc
            || (key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c'))
        {
            return Some(Action::Cancel);
        }
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('s') {
            if self.value.trim().is_empty() {
                self.error = "Enter a prompt".into();
            } else {
                return Some(Action::Create(self.value.clone()));
            }
        } else {
            self.cursor.edit(&mut self.value, &mut self.error, key);
        }
        None
    }

    pub fn rows(&self, width: usize, height: usize) -> Vec<PopupRow> {
        let mut rows = tag_input::rows(None, &self.value, &self.cursor, &self.error, width, height);
        let mut guidance = true;
        for row in &mut rows {
            match row.kind {
                PopupKind::Heading => row.text = "Create Herdr agent".into(),
                PopupKind::Hint
                    if guidance && height >= if self.error.is_empty() { 4 } else { 5 } =>
                {
                    row.text = format!("Directory: {}", self.root.display());
                    guidance = false;
                }
                PopupKind::Hint => {
                    row.text = if width >= 37 {
                        "Enter line  Ctrl-S create  Esc cancel"
                    } else {
                        "Enter line  Ctrl-S create  Esc"
                    }
                    .into();
                }
                _ => (),
            }
        }
        rows
    }
}
