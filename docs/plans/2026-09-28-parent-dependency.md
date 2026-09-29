# Parent Dependency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add parent-first task dependencies through `qqq add --parent ID`.
**Architecture:** Nullable foreign key on tasks, transactional version 1 migration, ready-task filtering inside existing atomic claim transaction. CLI validates parent before editor invocation.
**Tech Stack:** Rust, clap, rusqlite, serde, tempfile integration tests.

## Task 1: Behavioral Tests

- [x] Add tests in `tests/dependencies.rs` using the real binary in temporary projects. Assert `add Child --parent 1` returns `parent_id: 1`, then `next --session a` claims parent, `next --session b` skips child for unrelated task, parent completion unlocks child. Verify chain order, release, empty ready queue, rejected unknown/self IDs, editor mode, and concurrent claims.
- [x] Build version 1 fixtures with `include_str!("../src/schema.sql")`; populate task/message/image/event/link rows with rusqlite, open via CLI and assert data preserved, parent null, `PRAGMA user_version` equals 2. Repeat and race migration; version 3 must fail unchanged.
- [x] Run `cargo test --locked --test dependencies`; expect failures from missing `--parent` and unchanged version 1.

## Task 2: Implement

- [x] Create `src/migrate_v2.sql`:
  ```sql
  ALTER TABLE tasks ADD COLUMN parent_id INTEGER REFERENCES tasks(id);
  PRAGMA user_version=2;
  ```
- [x] In `src/db.rs`, migrate versions 0 (init only) and 1 under immediate transaction, rechecking version; reject others except 2. Add `pub parent_id: Option<i64>` to Task, map column 7, append parent_id to every task SELECT.
- [x] Extend `Db::add` with `parent_id: Option<i64>`, validate using `self.task(id)?`, insert with `INSERT INTO tasks(title,description,parent_id) VALUES (?,?,?)`.
- [x] Use ready query inside existing next transaction:
  ```sql
  SELECT id FROM tasks WHERE status='pending'
  AND (parent_id IS NULL OR EXISTS
    (SELECT 1 FROM tasks parent WHERE parent.id=tasks.parent_id AND parent.status='completed'))
  ORDER BY id LIMIT 1
  ```
- [x] In `src/main.rs` Add variant, add `#[arg(long)] parent: Option<i64>`, validate before editor, forward to `db.add(&title, &description, parent)`.
- [x] Run `cargo test --locked --test dependencies`; expect pass.

## Task 3: Verify And Ship

- [x] Document `--parent`, nullable JSON field, blocked pending tasks and automatic migration in README.md.
- [x] Run `cargo test --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`. Request code review, fix blockers.
- [x] Prepare verified implementation for commit `[Feat] Add Parent Task Dependencies`. Full suite: 21 passed; fmt, clippy, diff checks passed; review found no blockers.

Integration handoff: rebase onto master, fast-forward master, verify merged tests, remove worktree/branch. Record commit/check results and complete task 2 through qqq CLI.
