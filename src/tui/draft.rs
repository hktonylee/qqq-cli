use crate::images::ImageInput;
use anyhow::{Result, ensure};
use unicode_segmentation::UnicodeSegmentation;

pub struct Composition {
    pub title: String,
    pub description: String,
    pub images: Vec<ImageInput>,
}
enum Atom {
    Text(String),
    Paste { id: usize, text: String },
    Image { id: usize, input: ImageInput },
}
impl Atom {
    fn label(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Paste { id, text } => {
                format!("[Pasted text #{id}: {} chars]", text.chars().count())
            }
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
    pub fn new(title: &str, description: &str) -> Self {
        let mut draft = Self {
            atoms: Vec::new(),
            cursor: 0,
            next_paste: 1,
            next_image: 1,
        };
        draft.insert(&format!("{title}\n\n{description}"));
        if title.is_empty() {
            draft.cursor = 0;
        }
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
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.chars().count() > 1000 {
            self.atoms.insert(
                self.cursor,
                Atom::Paste {
                    id: self.next_paste,
                    text,
                },
            );
            self.next_paste += 1;
            self.cursor += 1;
        } else {
            self.insert(&text);
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
    pub fn home(&mut self) {
        while self.cursor > 0
            && !matches!(&self.atoms[self.cursor-1], Atom::Text(text) if text == "\n")
        {
            self.cursor -= 1;
        }
    }
    pub fn end(&mut self) {
        while self.cursor < self.atoms.len()
            && !matches!(&self.atoms[self.cursor], Atom::Text(text) if text == "\n")
        {
            self.cursor += 1;
        }
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
    pub fn visible(&self) -> String {
        self.fragments().concat()
    }
    pub fn finish(&self) -> Result<Composition> {
        let mut text = String::new();
        let mut images = Vec::new();
        for atom in &self.atoms {
            match atom {
                Atom::Text(value) | Atom::Paste { text: value, .. } => text.push_str(value),
                Atom::Image { input, .. } => {
                    ensure!(
                        text.contains('\n'),
                        "Place images below the first title line"
                    );
                    input.media_type()?;
                    text.push_str(&format!("[Image: {}]", input.name));
                    images.push(input.clone());
                }
            }
        }
        let (title, description) = text.split_once('\n').unwrap_or((&text, ""));
        ensure!(
            !title.trim().is_empty(),
            "Task title cannot be empty; task not saved"
        );
        Ok(Composition {
            title: title.trim().to_owned(),
            description: description.trim().to_owned(),
            images,
        })
    }
}
