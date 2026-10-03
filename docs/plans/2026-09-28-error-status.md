# Task Error Status Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Failed tasks remain visible until explicit manual retry.

**Architecture:** Extend SQLite status/event constraints through version 4 migration. Add typed edit transitions carrying owner and failure reason; persist status, message, history and field edits atomically. Keep queue eligibility restricted to new tasks with completed parents.

**Tech Stack:** Rust, Clap, rusqlite, SQLite, integration tests.

---

### Task 1: Regression Contract

**Files:** Create `tests/error_status.rs`; update schema-version expectations in `tests/assignee.rs` and `tests/dependencies.rs`.

- [x] Add CLI lifecycle test using existing TempDir/Command style:

```rust
let failed = ok(&d, &["edit", "1", "--set-status", "error", "--reason", "Missing API key", "--session", "worker"]);
assert_eq!(failed["status"], "error");
assert!(failed["assignee"].is_null());
assert!(ok(&d, &["next", "--local", "--session", "worker"]).is_null());
let retried = ok(&d, &["edit", "1", "--set-status", "new"]);
assert_eq!(retried["status"], "new");
assert_eq!(ok(&d, &["next", "--local", "--session", "worker"])["id"], 1);
```

- [x] Add tests for invalid/missing reason, wrong owner, new/completed error rejection, atomic field edits, failed-parent blocking, preserved attachments/messages/history/link, visible error rows with completed limit, human output, waiting until manual retry.
- [x] Add version-3 fixture from existing SQL migrations; insert parent, child, note, event, image and link; create/delete high IDs. Test repeat/concurrent upgrades, unchanged data, high-water IDs, constraints, foreign-key integrity.
- [x] Run `cargo test --locked --test error_status`: expect unsupported `error` status failures before implementation.

### Task 2: Persistence And Edit Flow

**Files:** Create `src/sql/migrate_v4.sql`; modify `src/db.rs`, `src/main.rs`, `src/dispatch.rs`.

- [x] Rebuild `tasks` and `events` using existing column definitions with expanded constraints:

```sql
CHECK(status IN ('new','in_progress','completed','error'))
CHECK(action IN ('claim','release','complete','error'))
```

Copy explicit columns/IDs, preserve sequence with `UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='tasks'),0)) WHERE name='tasks_v4'` before dropping old table; same for events. Recreate `active_session`, `new_queue`, `events_task`; set `PRAGMA user_version=4`.
- [x] Extend `Db::open` version range/target to 4, disable foreign keys before migration transaction, retain locked version recheck, run v4 after v3, require no rows from `PRAGMA foreign_key_check`, commit and re-enable foreign keys.
- [x] Introduce typed transition:

```rust
pub enum EditTransition<'a> {
    New(&'a str),
    RetryError(&'a str),
    Error { session: &'a str, reason: &'a str },
}
```

`Db::edit` accepts optional transition. `New` releases only owned in-progress tasks; `RetryError` retries only tasks still in error inside the transaction. Both record release events. `Error` validates reason and owned in-progress task, sets status/clears assignee, records error event and reason message. Existing final field UPDATE and transaction commit remain.
- [x] Expand `EditStatus` with `Error`; add `reason: Option<String>` flagged `#[arg(long, requires = "set_status")]`. Validate reason only with error status and require nonempty reason. Resolve owner for error/in-progress release; use explicit session or `manual` for error retry without Herdr discovery.
- [x] Change dispatch startup-release call to `Some(EditTransition::New(&name))`; update agent prompt with concrete error-reporting command.
- [x] Run focused tests; require all pass. Run relevant legacy tests, update target-version assertions from 3 to 4 and unknown-version fixture from 4 to 5.

### Task 3: Output, Docs, Final Verification

**Files:** Modify `src/output.rs`, `README.md`.

- [x] Map JSON status `error` to human `Error`; add red terminal list color:

```rust
Some("error") => "Error",
Some("error") if color => Some("31"),
```

- [x] Document failure command, required reason, owner rules, manual retry without session, queue/dependency behavior, schema version 4 preservation, and red rows.
- [x] Run `cargo test --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --release`, `git diff --check`. Require exit 0.
- [x] Review ownership/migration/queue invariants. Commit with `[Feat] Add Task Error Status And Manual Retry`.
- [x] Use finishing-a-development-branch: rebase onto current master, fast-forward clean base, verify integrated build/tests, rebuild CLI used by worker, record evidence and complete #25. Resume `qqq next --wait --local --json`.

## Verification

Integrated code commit: `e6b8fee`. Full combined suite: **153 passed**, zero failures.
Formatting, strict Clippy, release build, release lifecycle smoke passed. Independent
review found concurrent session-free retry race; separate error-only retry predicate
fixed it, regression reproduced failure before fix. Image-trigger test proves rollback
of status, claim, reason, history, content and attachments. Final independent error
suite: 10 passed, no remaining review findings. Live DB migrated to version 4; task
#25 completed. Pre-upgrade backup: `/private/tmp/qqq-pre-error-v4-or_mqv75/qqq.db`.
