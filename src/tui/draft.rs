use crate::images::{ImageInput, ImageReference};
use anyhow::{Result, ensure};
use std::collections::{HashMap, VecDeque};
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
    fn adopt_images(&mut self, saved: &HashMap<usize, (String, String)>) {
        if let Self::Image { id, .. } = self {
            if let Some((name, markdown)) = saved.get(id) {
                *self = Self::StoredImage {
                    id: *id,
                    name: name.clone(),
                    original: markdown.clone(),
                    markdown: markdown.clone(),
                };
            }
        } else if let Self::StoredImage {
            original, markdown, ..
        } = self
        {
            original.clone_from(markdown);
        }
    }

    fn retained_bytes(&self) -> usize {
        match self {
            Self::Text(text) => text.capacity(),
            Self::Paste { text, source, .. } => {
                text.capacity()
                    + match source {
                        PasteSource::New => 0,
                        PasteSource::StoredFence(original) => original.capacity(),
                    }
            }
            Self::Image { input, .. } => input.name.capacity() + input.data.capacity(),
            Self::StoredImage {
                name,
                original,
                markdown,
                ..
            } => name.capacity() + original.capacity() + markdown.capacity(),
        }
    }

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
const HISTORY_LIMIT: usize = 256;
const HISTORY_BYTES: usize = 64 * 1024 * 1024;

struct Change {
    start: usize,
    replace: usize,
    atoms: Vec<Atom>,
    cursor: usize,
    other_cursor: usize,
    next_image: usize,
    other_next_image: usize,
}

impl Change {
    fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.atoms.capacity() * std::mem::size_of::<Atom>()
            + self.atoms.iter().map(Atom::retained_bytes).sum::<usize>()
    }
}

#[derive(Default)]
struct History {
    undo: VecDeque<Change>,
    redo: VecDeque<Change>,
    bytes: usize,
}

impl History {
    fn record(&mut self, change: Change) {
        self.bytes -= self.redo.iter().map(Change::retained_bytes).sum::<usize>();
        self.redo.clear();
        self.push_undo(change);
    }

    fn push_undo(&mut self, change: Change) {
        self.bytes += change.retained_bytes();
        self.undo.push_back(change);
        self.trim();
    }

    fn push_redo(&mut self, change: Change) {
        self.bytes += change.retained_bytes();
        self.redo.push_back(change);
        self.trim();
    }

    fn trim(&mut self) {
        while self.undo.len() + self.redo.len() > 1
            && (self.undo.len() + self.redo.len() > HISTORY_LIMIT || self.bytes > HISTORY_BYTES)
        {
            let discarded = self
                .undo
                .pop_front()
                .or_else(|| self.redo.pop_front())
                .unwrap();
            self.bytes -= discarded.retained_bytes();
        }
    }

    fn recount(&mut self) {
        self.bytes = self
            .undo
            .iter()
            .chain(&self.redo)
            .map(Change::retained_bytes)
            .sum();
        self.trim();
    }
}

pub struct Draft {
    atoms: Vec<Atom>,
    cursor: usize,
    next_image: usize,
    history: History,
    pub(super) tags: Vec<String>,
}
impl Draft {
    pub fn new(description: &str) -> Self {
        let mut draft = Self {
            atoms: Vec::new(),
            cursor: 0,
            next_image: 1,
            history: History::default(),
            tags: Vec::new(),
        };
        draft.insert(description);
        draft.history = History::default();
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
        draft.history = History::default();
        Ok(draft)
    }

    fn edit(&mut self, range: Range<usize>, atoms: Vec<Atom>, cursor: usize, next_image: usize) {
        if range.is_empty() && atoms.is_empty() {
            return;
        }
        let start = range.start;
        let replace = atoms.len();
        let removed = self.atoms.splice(range, atoms).collect();
        self.history.record(Change {
            start,
            replace,
            atoms: removed,
            cursor: self.cursor,
            other_cursor: cursor,
            next_image: self.next_image,
            other_next_image: next_image,
        });
        self.cursor = cursor;
        self.next_image = next_image;
    }

    fn invert(&mut self, change: &mut Change) {
        let replacement = std::mem::take(&mut change.atoms);
        let replace = replacement.len();
        change.atoms = self
            .atoms
            .splice(change.start..change.start + change.replace, replacement)
            .collect();
        change.replace = replace;
        self.cursor = change.cursor;
        self.next_image = change.next_image;
        std::mem::swap(&mut change.cursor, &mut change.other_cursor);
        std::mem::swap(&mut change.next_image, &mut change.other_next_image);
    }

    pub fn undo(&mut self) -> bool {
        let Some(mut change) = self.history.undo.pop_back() else {
            return false;
        };
        self.history.bytes -= change.retained_bytes();
        self.invert(&mut change);
        self.history.push_redo(change);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(mut change) = self.history.redo.pop_back() else {
            return false;
        };
        self.history.bytes -= change.retained_bytes();
        self.invert(&mut change);
        self.history.push_undo(change);
        true
    }

    pub fn insert(&mut self, text: &str) {
        let mut start = self.cursor;
        let mut atoms = Vec::new();
        for grapheme in text.graphemes(true) {
            if let Some(Atom::Text(previous)) = atoms.last_mut() {
                let joined = format!("{previous}{grapheme}");
                if joined.graphemes(true).count() == 1 {
                    *previous = joined;
                    continue;
                }
            } else if atoms.is_empty() {
                if let Some(Atom::Text(previous)) = self
                    .cursor
                    .checked_sub(1)
                    .and_then(|index| self.atoms.get(index))
                {
                    let joined = format!("{previous}{grapheme}");
                    if joined.graphemes(true).count() == 1 {
                        start -= 1;
                        atoms.push(Atom::Text(joined));
                        continue;
                    }
                }
            }
            atoms.push(Atom::Text(grapheme.to_owned()));
        }
        let cursor = start + atoms.len();
        self.edit(start..self.cursor, atoms, cursor, self.next_image);
    }
    pub fn paste(&mut self, text: &str) {
        let chars = text.chars().count();
        if chars > 1000 {
            self.edit(
                self.cursor..self.cursor,
                vec![Atom::Paste {
                    text: text.to_owned(),
                    chars,
                    source: PasteSource::New,
                }],
                self.cursor + 1,
                self.next_image,
            );
        } else {
            self.insert(text);
        }
    }
    pub fn image(&mut self, input: ImageInput) -> Result<()> {
        input.media_type()?;
        self.edit(
            self.cursor..self.cursor,
            vec![Atom::Image {
                id: self.next_image,
                input,
            }],
            self.cursor + 1,
            self.next_image + 1,
        );
        Ok(())
    }
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.edit(
                self.cursor - 1..self.cursor,
                Vec::new(),
                self.cursor - 1,
                self.next_image,
            );
        }
    }
    pub fn delete_previous_word(&mut self) {
        let mut start = self.cursor;
        while start > 0
            && matches!(&self.atoms[start - 1], Atom::Text(text)
                if !matches!(text.as_str(), "\n" | "\r\n")
                    && text.chars().all(char::is_whitespace))
        {
            start -= 1;
        }
        let mut removed_word = false;
        while start > 0 {
            match &self.atoms[start - 1] {
                Atom::Text(text) if !text.chars().all(char::is_whitespace) => {
                    start -= 1;
                    removed_word = true;
                }
                Atom::Paste { .. } | Atom::Image { .. } | Atom::StoredImage { .. }
                    if !removed_word =>
                {
                    start -= 1;
                    break;
                }
                _ => break,
            }
        }
        self.edit(start..self.cursor, Vec::new(), start, self.next_image);
    }
    pub fn delete(&mut self) {
        if self.cursor < self.atoms.len() {
            self.edit(
                self.cursor..self.cursor + 1,
                Vec::new(),
                self.cursor,
                self.next_image,
            );
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
    pub fn adopt_saved(
        &mut self,
        description: &str,
        task_id: i64,
        references: &[ImageReference],
    ) -> Result<bool> {
        let pending = self
            .atoms
            .iter()
            .filter_map(|atom| match atom {
                Atom::Image { id, input } => Some((*id, input)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let Some(start) = references.len().checked_sub(pending.len()) else {
            return Ok(false);
        };
        let mut saved = HashMap::new();
        let mut candidate = self.contents(true);
        for ((id, input), (reference, span)) in pending
            .iter()
            .zip(references[start..].iter().zip(&candidate.image_spans))
            .rev()
        {
            if input.name != reference.name || input.media_type()? != reference.media_type {
                return Ok(false);
            }
            let markdown = reference.markdown(task_id)?;
            candidate.description.replace_range(span.clone(), &markdown);
            saved.insert(*id, (reference.name.clone(), markdown));
        }
        if candidate.description != description {
            return Ok(false);
        }
        for atom in self.atoms.iter_mut().chain(
            self.history
                .undo
                .iter_mut()
                .chain(&mut self.history.redo)
                .flat_map(|change| &mut change.atoms),
        ) {
            atom.adopt_images(&saved);
        }
        self.tags.clear();
        self.cursor = self.atoms.len();
        self.history.recount();
        Ok(true)
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
