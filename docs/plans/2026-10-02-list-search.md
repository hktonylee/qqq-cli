# CLI List Search and Filters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add structured description search and repeatable status filters to `qqq list` and `list --watch`, preserving useful dependency context.

**Architecture:** Keep `Db::list` as source of completion-limited structured rows. New `src/list_filter.rs` applies query/status matching and adds visible ancestors as context-only rows. CLI and watch use same function; output renderer labels context rows.

**Tech Stack:** Rust 2024, Clap, Serde, SQLite-backed CLI integration tests.

---

### Task 1: Matching and dependency context

**Files:** `src/list_filter.rs`, `src/db.rs`, `src/main.rs`, `tests/list.rs`

- [x] Add failing CLI tests in `tests/list.rs`: full-description multiline query, Unicode case-insensitive match (`CAFÉ` finds `café`), literal SQL-like query, repeated statuses as OR, query/status as AND, and child-only match including parent with `context_only: true`. Verify direct matches retain their existing JSON shape and hidden completed parent stays hidden under `--max-completed 0`. Run `cargo test --locked --test list -- --nocapture`; expect failure for unrecognized `--query`/`--status`.
- [x] Define `ListStatus` in `src/list_filter.rs` with Clap `ValueEnum` for `new`, `in_progress`, `completed`, `error`. Add pure `filter_tasks(tasks: Vec<Task>, query: Option<&str>, statuses: &[ListStatus]) -> Vec<Task>`. Match `description.to_lowercase().contains(&query.to_lowercase())` and status set; use ID-to-index map plus visited ancestor IDs to include visible parents. Mark only nonmatching ancestors `context_only = true`. No filters return original rows unchanged.
- [x] Add `context_only: bool` to `Task` in `src/db.rs`, initialized false in `task_row`; serialize only true with a small `is_false` helper. Wire `List { query: Option<String>, statuses: Vec<ListStatus> }` into Clap and one-shot `execute` in `src/main.rs`; status flag is repeatable, accepted value `in_progress` matches DB spelling. Run focused tests; commit.

### Task 2: Human output and completed-limit behavior

**Files:** `src/output.rs`, `src/main.rs`, `tests/list.rs`, `README.md`

- [x] Add failing human CLI assertions: parent prints `[context]`, child stays in tree, filtered empty prints `No matching tasks.`, JSON filtered empty is `[]`, and `--status completed --max-completed 0` returns empty while `--all` can show completed matches. Test invalid status exits 2 with empty stdout and README-style examples.
- [x] Prefix context-only task descriptions in `task_tree` without changing unfiltered rows. In `run`, choose `No matching tasks.` for empty filtered human output before existing display-limit empty handling. Document `--query`, repeatable `--status`, query AND status, ancestor context, and completion-limit order in README. Run `cargo test --locked --test list -- --nocapture`; commit.

### Task 3: Watch integration and verification

**Files:** `src/watch.rs`, `src/main.rs`, `tests/watch.rs`

- [x] Add failing watch test that starts `list --watch --json --query needle --status new`, sees `[]`, adds matching/nonmatching tasks, and verifies every refreshed snapshot is filtered and still flat JSON. Use existing `Watcher` harness; no timing sleeps beyond its bounded channel wait.
- [x] Pass borrowed query/status filters from CLI to `watch::run`; apply `filter_tasks` after each `Db::list` call. Make filtered empty human watch snapshots say `No matching tasks.`. Run focused watch test and full `tests/watch.rs`; commit.
- [ ] Run focused `tests/list.rs`, `tests/watch.rs`, full `cargo test --locked`, `cargo fmt --all -- --check`, strict `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`. Request read-only code review; fix Critical/Important findings, rerun affected checks. Rebase on local master, fast-forward, rerun integrated full suite, remove owned worktree/branch, install CLI, complete task 63, verify queue status, then call `qqq --json next --wait --local` once.
