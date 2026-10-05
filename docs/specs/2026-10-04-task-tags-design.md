# Task Tags

Task 162 adds ordered tag metadata without changing task descriptions. User
shortcut decision: Ctrl-L edits tags, Ctrl-K opens task ID jump, Ctrl-T retains
Completed visibility in the filter.

## Data contract

- Schema 12 adds `tasks.tags TEXT NOT NULL DEFAULT '[]'`, constrained to a JSON
  array. Existing rows migrate to empty tags. `Task.tags` always serializes as
  an array of strings.
- Normalize by trimming surrounding whitespace, retaining the first occurrence
  of each exact label. Case and order remain significant. Unicode and internal
  spaces are supported. Empty labels, control characters, commas, square
  brackets are invalid. No arbitrary label count or length limits.
- Metadata changes preserve content revision, description, images, ownership,
  state, relationships, messages, events. Combined CLI changes commit atomically.
- Import version 1 accepts optional `tags`; omitted tags default to empty.
  Validate all labels before writing any batch rows. Backup/restore retains tags.

## CLI and display

`qqq add "Fix layout" --tag frontend --tag bug` creates tagged tasks.
`qqq edit 1 --set-tags "frontend, bug"` replaces tags; `--set-tags ""` clears.
Direct tag edits skip the editor. `--set-tags` conflicts with `--edit`.

List and TUI previews prefix the first description line with
`[frontend] [bug] `. Wrapping, tree context, plain output and Unicode remain
supported. Detail/show and next human output expose tag metadata. JSON keeps
the raw description intact. Tagless output remains unchanged. Existing filter
semantics remain description based; tag query syntax is outside this change.

## TUI

Ctrl-L opens a tag popup for the selected saved task from editor or filter.
Prefill current labels joined by comma and space. Enter applies; empty text
clears; Esc/Ctrl-C cancels. Invalid input stays in the popup with an error.
Typing/pasting edits popup text, never the description. A blank new-task editor
shows a select-task hint. Metadata saves preserve dirty drafts, caret, scroll,
selection, filter focus, parked buffers and content conflict baseline.

Ctrl-K retains task jump behavior, including completed/archived destinations and
draft preservation. Current shortcut hints, README/reference and jump spec
reflect new bindings. Historical jump implementation plan retains original
evidence with a note pointing to this change.

## Verification

CLI regressions cover normalization, malformed labels, atomic rollback,
metadata-only edits on owned tasks, import dry run/rollback, migration from 11,
backup/restore and wrapped tagged output. PTY scenarios cover Ctrl-L editing,
clear/cancel/validation, paste, dirty draft/caret preservation, filter focus,
new-task guard, Unicode, plain/color rendering; existing jump regressions move
to Ctrl-K. Run focused suites, complete suite, fmt, strict Clippy, read-only
review, local integration and installed-binary smoke before queue completion.
