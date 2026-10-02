# Pasteboard blocks for large text

## Outcome

Pasting more than 1000 characters in the built-in editor creates one atomic `[Pasted Content N chars]` item. It uses a gold foreground distinct from image cyan in both single editor and dashboard; `NO_COLOR` and `TERM=dumb` keep plain text. Backspace, Delete, cursor motion, word motion, and discard confirmation treat the item as one edit unit. Short pastes remain ordinary text.

Saving a new large paste writes a Markdown fenced code block with info string `pasteboard`, for example:

````text
```pasteboard
content
```
````

The fence uses at least three backticks and one more than the longest backtick run in the payload. A line break before the opener and after the closer is inserted only when neighboring description text needs it for a valid block. The payload's exact characters, including CRLF and trailing newlines, round-trip. `Draft::from_saved` recognizes complete, line-start `pasteboard` fences outside other fenced code blocks and restores one atomic paste item; incomplete or differently tagged fences receive no pasteboard interpretation. It preserves the original fence bytes when saving an untouched loaded item. Ordinary existing long descriptions still display collapsed as before and save without a new fence unless the user pastes new content. Blank-only paste payload cannot bypass task description validation.

## Components

`Draft` distinguishes newly pasted text, seeded long text, and restored fenced text. New paste serialization adds fence and required separators; seeded text serializes raw; restored text serializes its original fence. The parser scans image references and pasteboard blocks in source order so Markdown image links inside a pasteboard payload stay payload text. Composition image byte spans continue to point at image placeholders after any new fences are inserted.

`Layout` gains paste-highlight spans alongside image highlights. Both Crossterm single-editor painter and Ratatui dashboard painter use the same semantic span positions, with distinct foreground colors and no terminal color escapes when color is disabled.

## Verification

Model tests cover threshold, exact payload round-trip, inline separators, CRLF, trailing newline, embedded triple backticks, multiple blocks, invalid fences, atomic deletion, and image-span offsets. Render tests cover both editor modes, wrapping and no-color behavior. PTY tests save, reload, edit, and verify Markdown storage and color, including a payload containing a fence. Run focused tests, full Rust suite, fmt, strict Clippy, review, then integrated verification after local merge.
