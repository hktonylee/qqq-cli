# Content Conflict Protection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Reject stale task description/attachment saves atomically while retaining recoverable local drafts.

**Architecture:** Separate monotonic content revision from operational task metadata. Check expected revision under the existing immediate write lock; reuse image rollback. Keep conflict recovery in editor/TUI modules, with baseline revision retained per task buffer from task 144.

**Tech Stack:** Rust, rusqlite, SQLite triggers, ratatui/crossterm, Python PTY tests.

## 1. Guarded DB content saves

**Files:** `src/db.rs`, new `src/sql/migrate_v10.sql`, `src/doctor.rs`, `tests/editor.rs`, `tests/tui_db.rs`, schema-version assertions under `tests/`.

- [x] Add CLI regression asserting JSON has positive content revision; an old revision cannot replace newer description or create an image file. Use real commands in an isolated `TempDir`, compare complete `show` response and stored image bytes before/after rejection.
- [x] Run `CARGO_TARGET_DIR=/Users/tonylee/Dropbox/Projects/qqq/target cargo test --locked --test editor guarded_content_save -- --nocapture`; expect missing revision/guard failure.
- [x] Add v10 migration and update migration runner/doctor. Trigger contract:

```sql
ALTER TABLE tasks ADD COLUMN content_revision INTEGER NOT NULL DEFAULT 1 CHECK(content_revision>0);
CREATE TRIGGER task_description_revision AFTER UPDATE OF description ON tasks
WHEN OLD.description IS NOT NEW.description
BEGIN
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=NEW.id;
END;
CREATE TRIGGER task_image_insert_revision AFTER INSERT ON images
BEGIN
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=NEW.task_id;
END;
CREATE TRIGGER task_image_delete_revision AFTER DELETE ON images
BEGIN
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=OLD.task_id;
END;
CREATE TRIGGER task_image_update_revision AFTER UPDATE ON images
WHEN OLD.task_id IS NOT NEW.task_id OR OLD.name IS NOT NEW.name
 OR OLD.media_type IS NOT NEW.media_type OR OLD.bytes IS NOT NEW.bytes
BEGIN
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=OLD.task_id;
 UPDATE tasks SET content_revision=content_revision+1 WHERE id=NEW.task_id AND NEW.task_id!=OLD.task_id;
END;
PRAGMA user_version=10;
```

- [x] Include `content_revision` in task row SELECTs/JSON; move filter predicate row index to 14. Add `ContentSnapshot { task: Task, references: Vec<ImageReference> }`, fetched in a read transaction.
- [x] Add typed `ContentConflict` containing task ID, expected revision, optional current description/revision (none means removed). Add guard to `EditChanges`; retain existing unguarded wrappers. Public guarded methods take optional expected revision. At beginning of write transaction compare revision before parent/status/priority/image mutations.
- [x] Add `edit --expected-revision` positive integer flag, wire direct edit through guarded method. Existing direct edits without flag remain compatible.
- [x] Add DB tests for two snapshots, successful guarded image/paste composition, stale compound edit rollback, image-only revision change, no-op text save, message/status/priority/dependency changes, deletion while editing, and read-snapshot consistency. Update tests constructing v9 DBs to migrate v10; use 11 for unsupported future-version fixtures.
- [x] Run `cargo test --locked --test editor --test tui_db --test doctor --test dependencies --test images --test archive --test delete --test reopen --test body --test identity --test priority --test error_status` with shared target. Expect all pass. Commit verified DB/direct CLI checkpoint.

## 2. External editor recovery

**Files:** `src/editor.rs`, `src/main.rs`, `tests/editor.rs`.

- [x] Add real external editor script that invokes inline CLI edit while its draft is open; assert stale outer save fails, newer DB wins, local/current recovery files exist with exact complete text. Run focused test; expect old implementation overwrites newer content.
- [x] Keep edit draft directory through save attempt. Capture content snapshot before editor opens; use its revision for save, optionally require caller-supplied expected revision first. On conflict preserve local/current files, print their paths and current revision.
- [x] For terminal input offer `r` reload (confirm local discard), `o` overwrite (confirm), `k` keep and exit. Reload updates snapshot and reopens editor; overwrite guards revision displayed. Nonterminal input fails with recovery paths and guarded retry guidance. Keep pending image inputs on all retries; record image source paths for file-based recovery. Removed task keeps local file, never recreates DB row.
- [x] Add regression checks for script-driven reload/confirmed overwrite, second concurrent change, removed task, attachment preservation, and successful edit cleanup. Run editor suite; commit verified recovery checkpoint.

## 3. TUI conflict recovery and task 144 integration

**Files:** `src/tui/mod.rs`, new focused `src/tui/conflict.rs`, task 144 buffer module if present, `src/main.rs`, `tests/tui_dashboard_pty.py`, `tests/tui.rs`, `tests/tui_db.rs`.

- [ ] Implement guarded conflict handling on the current editor baseline; rebase onto completed task 144 before final per-task buffer integration. Retain original per-task revision wherever task 144 stores baseline/draft/cursor; switching away/back never refreshes a dirty buffer's baseline.
- [ ] Add PTY case: load task, edit draft, another CLI replaces text; Ctrl-S opens conflict view, leaves DB/local draft intact. Switch tasks and back, retry still conflicts. Include multiline paste and unsaved PNG data. Run new case; expect old save overwrites.
- [ ] Carry `expected_revision: Option<i64>` in `Outcome`. Load snapshots for targets, retain revision per buffer, use guarded callbacks from dashboard/continuous edit. Single-task builtin edit must save inside guarded loop rather than returning draft before save.
- [ ] Conflict view shows full current/local text through scrollable modal. `Esc` keeps draft; `r` opens discard/reload confirmation; `o` opens overwrite confirmation. Confirmed overwrite submits current view revision; a newer save reopens conflict with new revision. Successful save refreshes baseline from DB, clears only saved buffer. Reload replaces only selected buffer after confirmation. Missing task offers close/keep draft; no overwrite.
- [ ] Extend PTY checks for color/plain, narrow layout, two editors, removal, reload cancel/confirm, overwrite cancel/confirm, further edits during confirmation, status/message-only changes, and cached dirty-buffer preservation. Run focused TUI/DB suites; commit verified TUI checkpoint.

## 4. Docs, review, integration

**Files:** `docs/reference.md`, `README.md` only if quick-start behavior needs a concise hint, plan/spec status.

- [ ] Document content_revision, --expected-revision, conflict view keys, explicit confirmations, external recovery files, unguarded compatibility, and removal behavior.
- [ ] Run full `cargo test --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`; verify actual counts/exit codes. Update plan checkboxes with completed results.
- [ ] Request read-only review via existing reviewer, address supported findings, run affected checks. Rebase current master; rerun relevant tests if upstream code changes.
- [ ] With clean root, fast-forward merge. Install via `cargo install --path . --locked --force`. Run installed real CLI revision/image conflict and TUI conflict flows.
- [ ] After all gates pass, explicitly complete task 146, remove worktree/merged branch, resume one blocking `qqq next --wait --local --json`.

DB/external checkpoint: focused 13-target suite passed; snapshot race check passed; external reload/overwrite PTY flows passed; Clippy all targets passed. TUI conflict recovery remains pending.
