# Completed Task Reopen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reopen completed tasks as new without losing history or stranding visible dependents.

**Architecture:** Version-9 migration admits `reopen` events. A DB transaction verifies completed, unarchived state and parent availability, updates status/time, appends event, returns task. CLI exposes explicit command; existing list/watch/claim paths observe status transition.

**Tech Stack:** Rust 2024, Clap, rusqlite, SQLite, CLI integration tests.

---

### Task 1: Schema version 9 and event history

**Files:** `src/migrate_v9.sql`, `src/db.rs`, `tests/reopen.rs`, `tests/tui_db.rs`, existing schema-version tests.

- [x] Write v8 fixture test: apply `schema.sql` and migrations 2–8, insert a completed task with event ID 8, insert/delete event ID 20, run CLI `show`; assert version 9, preserved task/event rows, and next `reopen` event ID 21. Run `cargo test --locked --test reopen`; expect missing version/action failure.
- [x] Add `migrate_v9.sql` with event-table rebuild. New action CHECK includes `claim`, `release`, `complete`, `error`, `archive`, `unarchive`, `reopen`; copy IDs and timestamps, preserve `sqlite_sequence`, recreate `events_task`, set `PRAGMA user_version=9`.
- [x] Update `Db::open` outer/inner version guards and final assertion to 9; apply migration 9 after 8. Apply migration to `tests/tui_db.rs` direct fixture; change successful-version assertions from 8 to 9 and unsupported-version fixture from 9 to 10. Run focused migration and DB tests; commit.

### Task 2: Reopen transaction and CLI

**Files:** `src/db.rs`, `src/main.rs`, `src/output.rs`, `tests/reopen.rs`.

- [x] Write failing tests for `qqq reopen ID` on completed task: status becomes new, timestamp changes, one event appended, description/priority/parent/messages/images/previous events remain. Repeated reopen and new/active/error targets fail with no mutation. `--json` returns task; human output says `Status: New`; negative index works. Run `cargo test --locked --test reopen`; expect unrecognized command.
- [x] Add `Db::reopen(id: i64, actor: &str) -> Result<Task>`: start immediate transaction, read task, require `status='completed'` and `archived=0`, call `ensure_parent_available` when parent exists, update `status='new'`, null ownership fields, set timestamp, insert `reopen` event, return task, commit. Add `Reopen { id: i64 }` to Clap; resolve ID; use `session_input.unwrap_or("cli")`; map output to `Format::Task`. Run focused tests; commit.

### Task 3: Dependency, archive, watcher, docs

**Files:** `tests/reopen.rs`, `tests/watch.rs`, `README.md`.

- [x] Test reopening completed parent: completed descendants stay complete, new descendants wait until parent recompletes; claim priority applies. Test archived target and child beneath archived unfinished parent reject, then unarchive permits reopen. Existing DB parent guard passed these checks.
- [x] Test watcher: completed→reopen snapshot changes to new, rejected reopen emits no snapshot. Run `cargo test --locked --test reopen --test watch`; commit.
- [x] Document `qqq reopen ID`, archived prerequisite, rejection, history preservation, and descendant readiness in `README.md`. Check `qqq reopen --help` and focused CLI tests; commit.

### Task 4: Review and integration

- [ ] Run `cargo test --locked --quiet`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`. Request read-only review; resolve Critical/Important findings and rerun affected checks. Rebase branch on local master, fast-forward, rerun integrated full suite, install CLI, complete task 66, update plan status, remove owned worktree/branch, call `qqq --json next --wait --local` once.
