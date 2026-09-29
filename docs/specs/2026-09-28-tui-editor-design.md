# Interactive Task Editor

Task #22 adds a default terminal editor to interactive `qqq add` and `qqq edit`.
User selected default TUI with `$EDITOR` available through `--edit`.

## Input and UI

One draft buffer uses first line as title, remaining lines as description. Inline
field commands remain direct updates. `add` without title and `edit` without field
updates open TUI when stdin/stderr are terminals. `--edit` always opens `$EDITOR`;
nonterminal interactive invocations keep existing `$EDITOR` fallback.

Draw editor to stderr in alternate screen, keeping stdout reserved for final text
or JSON result. Header explains draft format. Footer lists Ctrl-S save, Esc/Ctrl-C
cancel, Ctrl-V clipboard paste, Enter newline, arrows/Home/End navigation,
Backspace/Delete removal. Resize keeps cursor visible in a scrolling viewport.
Terminal guard restores raw mode, cursor, bracketed paste and alternate screen
on normal return or error. Paste cannot trigger save or cancel key bindings.

## Paste Model

Codex composer uses a 1,000 character threshold, atomic text elements and expanded
text at submission. Its clipboard reader obtains image data and encodes PNG:
- https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/chat_composer.rs
- https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/chat_composer/paste_input.rs
- https://github.com/openai/codex/blob/main/codex-rs/tui/src/clipboard_paste.rs

Implement same concept independently. Large terminal/clipboard pastes (>1,000
Unicode characters) display `[Pasted text #N: X chars]`; original text stays in
memory and expands on save. Short pastes remain editable text. Image clipboard
paste (Ctrl-V) and pasted local image paths display `[Image #N: filename]`.
Arrow navigation crosses placeholders as one unit; deleting placeholder removes
its payload. Literal placeholder-looking text never acquires special meaning.
Image placeholders must appear below title line. Saved description records image
filename marker; full image data is attached to task. No temporary image paths
are persisted. Existing attachments remain unchanged during edit.

Use crossterm bracketed paste events. Terminals without bracketed paste insert
ordinary text; Ctrl-V provides explicit clipboard paste and placeholder support.
Native clipboard failures are shown in footer; draft remains editable. Image
errors never discard draft. Newline normalization matches task draft format.
Control bytes are never emitted as terminal commands; display escapes them while
saved text preserves pasted content (apart from newline normalization/trimming).

## Data and Errors

Separate draft model, terminal loop/rendering, clipboard/path acquisition and DB
save. Clipboard uses arboard; PNG encoding uses image. Pin image version with
MSRV compatible with project's Rust 1.85 declaration. Unicode grapheme and width
libraries support cursor editing and viewport measurement.

Task plus newly pasted images commit in one SQLite transaction after successful
save. Enforce existing PNG/JPEG/GIF/WebP signatures and 20 MiB per-image limit.
No DB write transaction spans interactive input. Cancel, empty title, invalid
image, terminal failure or SQL failure creates no partial task/image changes.
Negative edit references resolve before editor opens. Preserve task status,
assignee, parent, messages and existing images.

## Verification

Test model expansion, literal labels, duplicate paste payloads, atomic deletion,
Unicode edits, control escaping, image placement and limits. Test atomic saves
with SQLite failure injection. PTY tests exercise bracketed paste, save/cancel,
JSON stdout isolation, terminal cleanup, editing and default entry. Existing
external-editor tests remain valid; add explicit `edit --edit` prefill coverage.
Run fmt, clippy, full test suite, release build and code review before integration.
