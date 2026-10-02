use crate::images::{ImageInput, ImageReference};
use anyhow::{Result, ensure};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub struct Composition {
    pub description: String,
    pub images: Vec<ImageInput>,
    pub image_spans: Vec<Range<usize>>,
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
    StoredImage {
        id: usize,
        name: String,
        original: String,
        markdown: String,
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
            Self::StoredImage { id, name, .. } => format!("[Image #{id}: {name}]"),
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

    pub fn from_saved(
        description: &str,
        task_id: i64,
        references: &[ImageReference],
    ) -> Result<Self> {
        let mut draft = Self::new("");
        let markdown = references
            .iter()
            .map(|image| image.markdown(task_id))
            .collect::<Result<Vec<_>>>()?;
        let legacy = references
            .iter()
            .map(|image| format!("[Image: {}]", image.name))
            .collect::<Vec<_>>();
        let mut used_legacy = vec![false; references.len()];
        let mut remaining = description;
        while !remaining.is_empty() {
            let mut found: Option<(usize, usize, bool)> = None;
            for index in 0..references.len() {
                for (token, is_legacy) in [(&markdown[index], false), (&legacy[index], true)] {
                    if is_legacy && used_legacy[index] {
                        continue;
                    }
                    if let Some(position) = remaining.find(token) {
                        if found.is_none_or(|(best, _, _)| position < best) {
                            found = Some((position, index, is_legacy));
                        }
                    }
                }
            }
            let Some((position, index, is_legacy)) = found else {
                draft.paste(remaining);
                break;
            };
            draft.paste(&remaining[..position]);
            let original = if is_legacy {
                used_legacy[index] = true;
                &legacy[index]
            } else {
                &markdown[index]
            };
            draft.atoms.push(Atom::StoredImage {
                id: draft.next_image,
                name: references[index].name.clone(),
                original: original.clone(),
                markdown: markdown[index].clone(),
            });
            draft.next_image += 1;
            draft.cursor += 1;
            remaining = &remaining[position + original.len()..];
        }
        Ok(draft)
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
                Atom::Paste { .. } | Atom::Image { .. } | Atom::StoredImage { .. }
                    if !removed_word =>
                {
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
            .map(|atom| matches!(atom, Atom::Image { .. } | Atom::StoredImage { .. }))
            .collect()
    }
    pub fn is_dirty_against(&self, baseline: &str) -> bool {
        let contents = self.contents(false);
        contents.description != baseline || !contents.images.is_empty()
    }
    fn contents(&self, normalize_legacy: bool) -> Composition {
        let mut text = String::new();
        let mut images = Vec::new();
        let mut image_spans = Vec::new();
        for atom in &self.atoms {
            match atom {
                Atom::Text(value) | Atom::Paste { text: value, .. } => text.push_str(value),
                Atom::Image { input, .. } => {
                    let start = text.len();
                    text.push_str(&format!("[Image: {}]", input.name));
                    image_spans.push(start..text.len());
                    images.push(input.clone());
                }
                Atom::StoredImage {
                    original, markdown, ..
                } => text.push_str(if normalize_legacy { markdown } else { original }),
            }
        }
        Composition {
            description: text,
            images,
            image_spans,
        }
    }
    pub fn finish(&self) -> Result<Composition> {
        let contents = self.contents(true);
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
