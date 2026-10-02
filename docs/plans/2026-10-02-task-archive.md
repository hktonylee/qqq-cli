# Reversible Task Archive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Archive and restore tasks without losing data, blocking live dependencies, or exposing archived work in default queues.

**Architecture:** Version-8 SQLite migration adds an archived flag, event actions, and an unarchived queue index. DB methods enforce dependency rules in immediate transactions. CLI, watch, and TUI use visibility-aware reads; explicit flags reveal archived tasks.

**Tech Stack:** Rust 2024, Clap, rusqlite, SQLite migrations, CLI/PTY integration tests.

---

### Task 1: Schema and task model

**Files:** `src/migrate_v8.sql`, `src/db.rs`, `tests/archive.rs`, `tests/tui_db.rs`, existing migration tests.

- [x] Add failing v7 fixture test in `tests/archive.rs`: apply `schema.sql` and migrations 2–7, insert task/event/image metadata, run CLI `list`; expect `archived:false`, schema version 8, unchanged IDs/timestamps/events, and preserved next autoincrement IDs. Run `cargo test --locked --test archive`; expect failure.
- [x] Add `src/migrate_v8.sql` with `ALTER TABLE tasks ADD COLUMN archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1));`. Replace `new_queue_priority` with `ON tasks(priority DESC,id) WHERE status='new' AND archived=0`. Rebuild `events` with action CHECK containing prior four actions plus `archive` and `unarchive`; copy rows, preserve `sqlite_sequence`, rename, recreate `events_task`, set `user_version=8`.
- [x] In `src/db.rs`, accept versions 1–8, run migration 8 after migration 7 under existing immediate transaction, require final version 8. Add `Task.archived: bool`; append `archived` to every `task_row` SELECT and decode final column. Apply `migrate_v8.sql` to direct DB fixture in `tests/tui_db.rs`; update old tests expecting schema 7 or rejecting version 8 to expect 8/reject 9. Run focused migration/DB tests; commit.

### Task 2: Archive state transitions and dependency invariant

**Files:** `src/db.rs`, `tests/archive.rs`.

- [x] Add failing tests: archive/unarchive preserve status, content, priority, images, messages, previous events, IDs, and identity; repeated ops add no event and leave `updated_at` unchanged; archiving active task fails; unfinished parent with visible unfinished child fails, while completed parent or only completed/archived children succeeds; unarchiving unfinished child under archived unfinished parent fails. Run `cargo test --locked --test archive`; expect failure.
- [x] Add `Db::set_archived(id, archived, actor) -> Result<Task>` (or two thin wrappers). Start `BEGIN IMMEDIATE`, load row, return unchanged if already in requested state, reject active archive, query non-archived unfinished children before archiving unfinished parent, query archived unfinished parent before unarchiving unfinished child. Update flag/time, insert event action, return task, commit. Use explicit actor or `cli` fallback from CLI. Run focused tests; commit.
- [x] Add failing tests for creating/reparenting unfinished child under archived unfinished parent and for nested dependencies. Move add parent check into its insert transaction; add shared SQL predicate/check for blocked parent to add, edit parent change, and unarchive. Run focused dependency/archive tests; commit.

### Task 3: Queue and visibility reads

**Files:** `src/db.rs`, `src/main.rs`, `src/watch.rs`, `src/tui/mod.rs`, `tests/archive.rs`, TUI PTY tests.

- [x] Add failing tests: default list hides archived rows; explicit include flag shows them; hidden archived completions do not consume completed display limit; `next` skips archived high-priority work; archived completed parent still releases child; simultaneous sessions claim distinct visible ready tasks. Run focused tests; expect failure.
- [x] Add `Db::list_with_archived(max_completed, include_archived)` and keep `list` wrapper for existing callers. Filter archived rows in outer list and completed subquery. Add `archived=0` to `has_ready` and new-task claim query. Keep completed-parent predicate based on status alone. Run focused tests; commit.
- [x] Add Clap `--include-archived` to `list` and `tui`; pass flag through watch and dashboard. Default continuous editor navigation excludes archived tasks; dashboard uses visibility-aware list and existing visible-ID navigation. Add focused PTY visibility test using preseeded archived row, plus direct navigation tests where practical. Run focused TUI/watch tests; commit.

### Task 4: CLI commands, output, docs

**Files:** `src/main.rs`, `src/output.rs`, `src/output/detail.rs`, `README.md`, `tests/archive.rs`, `tests/output.rs`.

- [x] Add failing CLI tests for `archive ID`, `unarchive ID`, negative index, JSON `archived`, event history, human summary/show label, `[archived]` list marker, and repeat operations. Run `cargo test --locked --test archive`; expect unrecognized commands.
- [x] Add `Archive { id: i64 }` and `Unarchive { id: i64 }` Clap variants. Resolve negative IDs via `resolve_task_id`, call DB transition with `session_input.unwrap_or("cli")`, return task JSON. Map both variants to task summary format. Add `Archived: yes/no` in human task/show and `[archived]` prefix only in list/TUI description rendering. Run focused CLI/output tests; commit.
- [x] Document commands, default/explicit visibility, dependency guard, event history, and preserved state in `README.md`; add examples for `list --include-archived`, `tui --include-archived`, archive/unarchive. Run docs-linked help/output checks; commit.

### Task 5: Review and integration

- [ ] Run full `cargo test --locked --quiet`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`. Request read-only code review, fix Critical/Important findings, rerun affected checks. Rebase on local master, fast-forward, rerun integrated full suite, install CLI, complete task 65, update this plan status, remove owned worktree/branch, then call `qqq --json next --wait --local` once.
