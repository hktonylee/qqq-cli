# Concurrent Task Content Saves

Task 146 prevents editors from replacing descriptions or adding attachments against stale loaded content. Task 144 owns retaining per-task dirty buffers; this change carries each buffer's content revision with its baseline.

## Content revision and transaction

Schema v10 adds positive integer `tasks.content_revision`, initially 1. SQLite triggers advance it when description changes or an image row is inserted, updated, or deleted. Identical description saves do not advance it. Messages, status, ownership, priority, dependencies, archival, and Herdr metadata do not advance it. Task JSON includes this revision.

A content snapshot reads task text, revision, and image references in one read transaction. A guarded save starts an immediate write transaction, compares its expected revision, then applies every requested field and attachment. A mismatch or removed task rejects before any mutation or image-file creation. Existing unguarded direct edits remain supported. `edit --expected-revision N` enables guarded direct edits.

## Editor recovery

Loaded TUI buffers retain their revision alongside original text and draft atoms. Save conflicts preserve text, cursor, collapsed paste payloads, stored image references, and unsaved image bytes. A scrollable conflict view shows current DB text and local draft text. Reload requires confirmation before dropping local edits. Overwrite requires a separate explicit confirmation and still checks the revision shown in the conflict view, so another concurrent save causes another conflict. Closing the view keeps the draft. Removed tasks retain local drafts and report removal without recreating the task.

Interactive single-task edits use the same guarded save loop. External-editor saves capture the baseline before opening `$EDITOR`. On conflict, recovery files retain local and current text; a terminal prompt offers reload, overwrite with confirmation, or keep draft and exit. With noninteractive input, report paths and revision for an explicit guarded CLI retry, exit nonzero, leave DB untouched. Pending `--image` attachments remain available through prompt retries; noninteractive recovery records their source paths.

## Compatibility and verification

Keep existing save wrappers for callers that deliberately request unguarded edits. Guarding does not change owner checks or transition semantics. Migration preserves task/image IDs and existing file bytes; concurrent opens serialize migration.

Checks cover two loaded editors, direct CLI revision guards, external editor vs CLI, status/message-only changes, compound-edit rollback, paste/image conflicts, removed tasks, recovery files, reload, confirmed overwrite, and a second edit during conflict confirmation. Full Rust/PTY suite, fmt, Clippy, review, and installed CLI smoke checks precede completion. Integrate task 144 before final TUI checks and merge.
