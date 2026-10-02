# Task Priority Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist bounded task priority and claim highest-priority ready task with stable ID ties.

**Architecture:** Version-7 SQLite migration adds checked priority column and queue index. DB keeps priority in every `Task` projection; CLI add/edit pass validated values into existing atomic transactions. Claim query orders ready new tasks by priority descending, ID ascending. Output adds CLI list/show priority while preserving compact TUI rows.

**Tech Stack:** Rust 2024, Clap, rusqlite, SQLite migrations, CLI integration tests.

---

### Task 1: Schema migration and task model

**Files:** `src/migrate_v7.sql`, `src/db.rs`, `tests/priority.rs`, `tests/dependencies.rs`

- [x] Add failing migration test. Build version-6 fixture by applying `schema.sql` then `migrate_v2.sql` through `migrate_v6.sql`, seed two new tasks, record timestamps and IDs. Run new CLI `list`; assert numeric priority 0, schema version 7, unchanged IDs/timestamps, first `next` claims oldest ID. Also open same version-6 DB through two CLI processes to verify lock/recheck. Run `cargo test --locked --test priority -- --nocapture`; expect failure from missing priority/version 7.
- [x] Add migration SQL:

```sql
ALTER TABLE tasks ADD COLUMN priority INTEGER NOT NULL DEFAULT 0
 CHECK(priority BETWEEN -100 AND 100);
DROP INDEX new_queue;
CREATE INDEX new_queue_priority ON tasks(priority DESC,id) WHERE status='new';
PRAGMA user_version=7;
```

- [x] Extend `Db::open` version guards and immediate migration path to version 7. Turn `foreign_keys` OFF only when starting below version 6; rerun version check inside write transaction. Add `priority: i64` to `Task` and every SQL projection feeding `task_row`; preserve all existing field offsets, read priority at final index. Update unsupported-version test to reject version 8. Run focused migration and existing DB tests; commit.

### Task 2: CLI add/edit priority, bounds, ownership

**Files:** `src/main.rs`, `src/db.rs`, `tests/priority.rs`

- [x] Add failing tests for `add --priority 8`, default 0, negative priority, `edit ID --priority -3`, combined description/priority, priority-only edits on active/completed/error tasks, and unchanged status, claim key, identity, event count. Reject `-101`, `101`, and non-integer values without writes; test `--priority -5` as separate argument. Run focused test; expect unrecognized flag.
- [x] Add Clap `--priority` to `Add` (default 0) and `Edit` (optional), using `i64` parser range `-100..=100` and allowing negative values. Add `Db::add_with_priority` and `save_composition_with_priority` for creation while retaining existing zero-priority wrappers for TUI/test callers. Add `Db::edit_with_priority` and thread optional priority into `edit_with_spans`; update priority in existing edit transaction. Priority-only `edit` must bypass editor. Validate bounds in DB methods before writes. Run focused tests; commit.

### Task 3: Claim order and dependency readiness

**Files:** `src/db.rs`, `tests/priority.rs`

- [ ] Add failing tests: high-priority task claims before older low-priority task; equal priority ties use smallest ID; high-priority child remains blocked until parent completion; existing owned task returns before higher-priority new task; changing active task priority preserves claim. Spawn concurrent distinct sessions against same DB and assert unique claims from highest ready priorities. Run focused test; expect FIFO mismatch.
- [ ] Change new-task selection in `Db::claim` to `ORDER BY priority DESC,id ASC LIMIT 1`; leave owned-task query and parent predicate unchanged. Run focused priority, dependency, ownership, and concurrency tests; commit.

### Task 4: Human output, docs, and integration

**Files:** `src/output.rs`, `src/output/detail.rs`, `src/tui/mod.rs`, `tests/priority.rs`, `README.md`

- [ ] Add failing assertions that CLI `list` includes `PRI` column with signed values, `show` includes `Priority:`, and `add`/`edit` human summaries include priority. Confirm TUI compact list stays unchanged with existing TUI suite.
- [ ] Add a CLI task-tree format variant with priority column, retaining current `Format::Tasks` for TUI. Wire CLI `list` and `watch` to priority variant. Add priority row to human show and task summary. Document examples, range, claim ordering, and stable list order in README. Run focused output/TUI tests; commit.
- [ ] Run full `cargo test --locked`, `cargo fmt --all -- --check`, strict `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`. Request read-only review, fix Critical/Important findings, and rerun affected tests. Rebase onto local master, fast-forward, rerun integrated full suite, remove owned worktree/branch, install CLI, complete task 64, verify queue status, then call `qqq --json next --wait --local` once.
