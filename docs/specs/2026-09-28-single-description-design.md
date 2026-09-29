# Single Task Description

Task #27 uses one description rather than splitting a first-line title from a
body. User's earlier instruction remains: no new migration, manually update local
SQLite with backup.

## Contract

`add TEXT` and `add --description TEXT` supply the same whole body; mutually
exclusive. Omit both to compose. `--edit` opens prefilled $EDITOR. `edit ID
--description TEXT` replaces whole body; omitted body preserves it during image
or status edits. Remove `--title`, Task.title and Composition.title. Description
must contain non-whitespace text; preserve leading/trailing whitespace, blank
lines and final newline. External editor writes/reads entire buffer without added
separator or trimming. TUI seeds one buffer, collapses large pastes and expands
losslessly on save; pasted image may appear anywhere, including first line.

Human list/tree/watch show first description line only, escaped for terminal.
Task details show full description, escaped line by line. JSON always carries
full description without title or derived preview fields. No stored preview.

## Persistence

Drop title from task schema, row queries and field updates. Existing dependencies,
ownership, statuses, timestamps, events, messages and attachments retain behavior.
Existing validation/atomic add/edit paths operate on one body. Keep schema version
and existing historical migration files; create no new migration. Databases that
still contain title fail with clear manual-update instruction before any writes.

After code verification and integration, back up live SQLite with backup API.
Within immediate transaction rebuild tasks from current fresh task schema,
copying all non-title fields and IDs; description becomes title when old body is
empty, otherwise title + blank line + old body. Preserve AUTOINCREMENT high-water
mark and indexes. Preserve related tables verbatim. Validate row counts, all
non-text task fields, foreign keys and integrity before commit. Rebuild binary
before switching live schema. Concurrent task #26 ownership fields must be
included by rebasing and deriving schema from current build at update time.

## Verification

Focused tests: no title in schema/JSON/help, positional/flag full body, exact
editor round-trip and prefill, leading blank line, whole-body blank validation,
list preview versus show, legacy-title rejection without mutation. Update existing
workflow, editor, DB fixture, attachment and PTY tests to same single-body API;
retain atomic failures, identity transitions, recent indexes and paste behavior.
Full suite, fmt, Clippy, release build, read-only review before local merge.
