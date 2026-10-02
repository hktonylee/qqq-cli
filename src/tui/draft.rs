use crate::images::ImageInput;
use anyhow::{Result, ensure};
use unicode_segmentation::UnicodeSegmentation;

pub struct Composition {
    pub description: String,
    pub images: Vec<ImageInput>,
}
enum Atom {
    Text(String),
    Paste {
        id: usize,
        text: String,
        chars: usize,
    },
    Image {
        id: usize,
        input: ImageInput,
    },
}
impl Atom {
    fn is_whitespace(&self) -> bool {
        matches!(self, Self::Text(text) if text.chars().all(char::is_whitespace))
    }

    fn is_word_text(&self) -> bool {
        matches!(self, Self::Text(text) if !text.chars().all(char::is_whitespace))
    }

    fn label(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Paste { id, chars, .. } => format!("[Pasted text #{id}: {chars} chars]"),
            Self::Image { id, input } => format!("[Image #{id}: {}]", input.name),
        }
    }
}
pub struct Draft {
    atoms: Vec<Atom>,
    cursor: usize,
    next_paste: usize,
    next_image: usize,
}
impl Draft {
    pub fn new(description: &str) -> Self {
        let mut draft = Self {
            atoms: Vec::new(),
            cursor: 0,
            next_paste: 1,
            next_image: 1,
        };
        draft.paste(description);
        draft
    }
    pub fn insert(&mut self, text: &str) {
        for grapheme in text.graphemes(true) {
            if let Some(Atom::Text(previous)) = self
                .cursor
                .checked_sub(1)
                .and_then(|index| self.atoms.get_mut(index))
            {
                let joined = format!("{previous}{grapheme}");
                if joined.graphemes(true).count() == 1 {
                    *previous = joined;
                    continue;
                }
            }
            self.atoms
                .insert(self.cursor, Atom::Text(grapheme.to_owned()));
            self.cursor += 1;
        }
    }
    pub fn paste(&mut self, text: &str) {
        let chars = text.chars().count();
        if chars > 1000 {
            self.atoms.insert(
                self.cursor,
                Atom::Paste {
                    id: self.next_paste,
                    text: text.to_owned(),
                    chars,
                },
            );
            self.next_paste += 1;
            self.cursor += 1;
        } else {
            self.insert(text);
        }
    }
    pub fn image(&mut self, input: ImageInput) -> Result<()> {
        input.media_type()?;
        self.atoms.insert(
            self.cursor,
            Atom::Image {
                id: self.next_image,
                input,
            },
        );
        self.next_image += 1;
        self.cursor += 1;
        Ok(())
    }
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.atoms.remove(self.cursor);
        }
    }
    pub fn delete_previous_word(&mut self) {
        while self.cursor > 0
            && matches!(&self.atoms[self.cursor - 1], Atom::Text(text)
                if !matches!(text.as_str(), "\n" | "\r\n")
                    && text.chars().all(char::is_whitespace))
        {
            self.backspace();
        }
        let mut removed_word = false;
        while self.cursor > 0 {
            match &self.atoms[self.cursor - 1] {
                Atom::Text(text) if !text.chars().all(char::is_whitespace) => {
                    self.backspace();
                    removed_word = true;
                }
                Atom::Paste { .. } | Atom::Image { .. } if !removed_word => {
                    self.backspace();
                    break;
                }
                _ => break,
            }
        }
    }
    pub fn delete(&mut self) {
        if self.cursor < self.atoms.len() {
            self.atoms.remove(self.cursor);
        }
    }
    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.atoms.len());
    }
    pub fn previous_word(&mut self) {
        while self.cursor > 0 && self.atoms[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        if self.cursor > 0 && !self.atoms[self.cursor - 1].is_word_text() {
            self.cursor -= 1;
            return;
        }
        while self.cursor > 0 && self.atoms[self.cursor - 1].is_word_text() {
            self.cursor -= 1;
        }
    }
    pub fn next_word(&mut self) {
        while self.cursor < self.atoms.len() && self.atoms[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
        if self.cursor < self.atoms.len() && !self.atoms[self.cursor].is_word_text() {
            self.cursor += 1;
            return;
        }
        while self.cursor < self.atoms.len() && self.atoms[self.cursor].is_word_text() {
            self.cursor += 1;
        }
    }
    pub fn home(&mut self) {
        while self.cursor > 0
            && !matches!(&self.atoms[self.cursor-1], Atom::Text(text) if matches!(text.as_str(), "\n" | "\r\n"))
        {
            self.cursor -= 1;
        }
    }
    pub fn end(&mut self) {
        while self.cursor < self.atoms.len()
            && !matches!(&self.atoms[self.cursor], Atom::Text(text) if matches!(text.as_str(), "\n" | "\r\n"))
        {
            self.cursor += 1;
        }
    }
    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.atoms.len());
    }
    pub fn fragments(&self) -> Vec<String> {
        self.atoms.iter().map(Atom::label).collect()
    }
    pub fn image_mask(&self) -> Vec<bool> {
        self.atoms
            .iter()
            .map(|atom| matches!(atom, Atom::Image { .. }))
            .collect()
    }
    pub fn is_dirty_against(&self, baseline: &str) -> bool {
        let contents = self.contents();
        contents.description != baseline || !contents.images.is_empty()
    }
    fn contents(&self) -> Composition {
        let mut text = String::new();
        let mut images = Vec::new();
        for atom in &self.atoms {
            match atom {
                Atom::Text(value) | Atom::Paste { text: value, .. } => text.push_str(value),
                Atom::Image { input, .. } => {
                    text.push_str(&format!("[Image: {}]", input.name));
                    images.push(input.clone());
                }
            }
        }
        Composition {
            description: text,
            images,
        }
    }
    pub fn finish(&self) -> Result<Composition> {
        let contents = self.contents();
        for image in &contents.images {
            image.media_type()?;
        }
        ensure!(
            !contents.description.trim().is_empty(),
            "Task description cannot be empty; task not saved"
        );
        Ok(contents)
    }
}
