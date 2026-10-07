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
        text: String,
        chars: usize,
        source: PasteSource,
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
enum PasteSource {
    New,
    StoredFence(String),
}

struct PasteboardBlock<'a> {
    start: usize,
    end: usize,
    payload: &'a str,
}

fn fenced(text: &str) -> String {
    let longest = text.split(|ch| ch != '`').map(str::len).max().unwrap_or(0);
    let ticks = "`".repeat(3.max(longest + 1));
    format!("{ticks}pasteboard\n{text}\n{ticks}")
}

fn fence_marker(line: &str) -> Option<(u8, usize, &str, usize)> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let marker = *line.as_bytes().get(indent)?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let count = line[indent..]
        .bytes()
        .take_while(|byte| *byte == marker)
        .count();
    (count >= 3).then_some((marker, count, &line[indent + count..], indent))
}

fn fence_closes(line: &str, marker: u8, opening_count: usize) -> bool {
    fence_marker(line).is_some_and(|(closing, count, rest, _)| {
        closing == marker
            && count >= opening_count
            && rest.chars().all(|ch| matches!(ch, ' ' | '\t' | '\r'))
    })
}

fn find_pasteboard(description: &str, from: usize) -> Option<PasteboardBlock<'_>> {
    let mut start = from;
    let mut enclosing: Option<(u8, usize)> = None;
    while start < description.len() {
        let end = description[start..]
            .find('\n')
            .map_or(description.len(), |offset| start + offset);
        let line = &description[start..end];
        if let Some((marker, count)) = enclosing {
            if fence_closes(line, marker, count) {
                enclosing = None;
            }
        } else if let Some((marker, count, rest, indent)) = fence_marker(line) {
            if marker == b'~' || !rest.contains('`') {
                if marker == b'`'
                    && indent == 0
                    && rest.trim_end_matches([' ', '\t', '\r']) == "pasteboard"
                    && end < description.len()
                {
                    let payload_start = end + 1;
                    let mut line_start = payload_start;
                    loop {
                        let line_end = description[line_start..]
                            .find('\n')
                            .map_or(description.len(), |offset| line_start + offset);
                        if fence_closes(&description[line_start..line_end], marker, count) {
                            let payload_end = if line_start == payload_start {
                                line_start
                            } else {
                                line_start - 1
                            };
                            return Some(PasteboardBlock {
                                start,
                                end: line_end,
                                payload: &description[payload_start..payload_end],
                            });
                        }
                        if line_end == description.len() {
                            break;
                        }
                        line_start = line_end + 1;
                    }
                }
                enclosing = Some((marker, count));
            }
        }
        if end == description.len() {
            break;
        }
        start = end + 1;
    }
    None
}
impl Atom {
    fn is_whitespace(&self) -> bool {
        matches!(self, Self::Text(text) if text.chars().all(char::is_whitespace))
    }

    fn is_word_text(&self) -> bool {
        matches!(self, Self::Text(text) if !text.chars().all(char::is_whitespace))
    }

    fn starts_with_line_break(&self) -> bool {
        let text = match self {
            Self::Text(text) => text,
            Self::Paste {
                source: PasteSource::StoredFence(original),
                ..
            } => original,
            _ => return false,
        };
        text.starts_with('\n') || text.starts_with("\r\n")
    }

    fn label(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Paste { chars, .. } => format!("[Pasted Content {chars} chars]"),
            Self::Image { id, input } => format!("[Image #{id}: {}]", input.name),
            Self::StoredImage { id, name, .. } => format!("[Image #{id}: {name}]"),
        }
    }
}
pub struct Draft {
    atoms: Vec<Atom>,
    cursor: usize,
    next_image: usize,
    pub(super) tags: Vec<String>,
}
impl Draft {
    pub fn new(description: &str) -> Self {
        let mut draft = Self {
            atoms: Vec::new(),
            cursor: 0,
            next_image: 1,
            tags: Vec::new(),
        };
        draft.insert(description);
        draft
    }

    pub fn from_saved(
        description: &str,
        task_id: i64,
        references: &[ImageReference],
    ) -> Result<Self> {
        let mut draft = Self::new("");
        let mut fences = Vec::new();
        let mut scan = 0;
        while let Some(block) = find_pasteboard(description, scan) {
            scan = (block.end + 1).min(description.len());
            fences.push(block);
        }
        let markdown = references
            .iter()
            .map(|image| image.markdown(task_id))
            .collect::<Result<Vec<_>>>()?;
        let legacy = references
            .iter()
            .map(|image| format!("[Image: {}]", image.name))
            .collect::<Vec<_>>();
        let mut used_legacy = markdown
            .iter()
            .map(|reference| {
                description.match_indices(reference).any(|(position, _)| {
                    !fences
                        .iter()
                        .any(|block| position >= block.start && position < block.end)
                })
            })
            .collect::<Vec<_>>();
        let mut cursor = 0;
        while cursor < description.len() {
            let remaining = &description[cursor..];
            let mut found_image: Option<(usize, usize, bool)> = None;
            for index in 0..references.len() {
                for (token, is_legacy) in [(&markdown[index], false), (&legacy[index], true)] {
                    if is_legacy && used_legacy[index] {
                        continue;
                    }
                    if let Some(position) = remaining.find(token) {
                        let position = cursor + position;
                        if found_image.is_none_or(|(best, _, _)| position < best) {
                            found_image = Some((position, index, is_legacy));
                        }
                    }
                }
            }
            let next_block = fences.iter().find(|block| block.start >= cursor);
            if let Some(block) = next_block
                .filter(|block| found_image.is_none_or(|(position, _, _)| block.start <= position))
            {
                draft.insert(&description[cursor..block.start]);
                draft.atoms.push(Atom::Paste {
                    text: block.payload.to_owned(),
                    chars: block.payload.chars().count(),
                    source: PasteSource::StoredFence(
                        description[block.start..block.end].to_owned(),
                    ),
                });
                draft.cursor += 1;
                cursor = block.end;
                continue;
            }
            let Some((position, index, is_legacy)) = found_image else {
                draft.insert(remaining);
                break;
            };
            draft.insert(&description[cursor..position]);
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
            cursor = position + original.len();
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
                    text: text.to_owned(),
                    chars,
                    source: PasteSource::New,
                },
            );
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
        self.atoms.is_empty() && self.tags.is_empty()
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
    pub fn paste_mask(&self) -> Vec<bool> {
        self.atoms
            .iter()
            .map(|atom| matches!(atom, Atom::Paste { .. }))
            .collect()
    }
    pub fn is_dirty_against(&self, baseline: &str) -> bool {
        !self.tags.is_empty()
            || self
                .atoms
                .iter()
                .any(|atom| matches!(atom, Atom::Image { .. }))
            || self.contents(false).description != baseline
    }
    fn contents(&self, normalize_legacy: bool) -> Composition {
        let mut text = String::new();
        let mut images = Vec::new();
        let mut image_spans = Vec::new();
        for (index, atom) in self.atoms.iter().enumerate() {
            match atom {
                Atom::Text(value) => text.push_str(value),
                Atom::Paste {
                    text: value,
                    source,
                    ..
                } => match source {
                    PasteSource::New => {
                        if !text.is_empty() && !text.ends_with('\n') {
                            text.push('\n');
                        }
                        text.push_str(&fenced(value));
                        if self
                            .atoms
                            .get(index + 1)
                            .is_some_and(|next| !next.starts_with_line_break())
                        {
                            text.push('\n');
                        }
                    }
                    PasteSource::StoredFence(original) => {
                        if !text.is_empty() && !text.ends_with('\n') {
                            text.push('\n');
                        }
                        text.push_str(original);
                        if self
                            .atoms
                            .get(index + 1)
                            .is_some_and(|next| !next.starts_with_line_break())
                        {
                            text.push('\n');
                        }
                    }
                },
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
        let has_content = self.atoms.iter().any(|atom| match atom {
            Atom::Text(text) | Atom::Paste { text, .. } => !text.trim().is_empty(),
            Atom::Image { .. } | Atom::StoredImage { .. } => true,
        });
        ensure!(
            has_content && !contents.description.trim().is_empty(),
            "Task description cannot be empty; task not saved"
        );
        Ok(contents)
    }
}
