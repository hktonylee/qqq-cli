# Owner Liveness Preflight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Fail confirmed dead task owners synchronously before qqq executes commands.

**Architecture:** Focused process module owns portable process evidence; preflight module owns observation/cache and conditional failure transaction. Db captures fresh-claim process metadata and invokes preflight after opening current schema. Inspection paths scan existing current schema without migration.

**Tech Stack:** Rust, rusqlite, serde, existing Herdr CLI, macOS/Linux process tools.

---

### Task 1: Herdr Observation And Guarded Failure

Files: create `src/preflight.rs`, `tests/preflight.rs`; modify `src/db.rs`,
`src/herdr.rs`, `src/main.rs`.

- [x] Write subprocess fixture with saved named-server link. Change response to
  empty agents, run show; assert returned task already error, original data and
  link preserved, one qqq-preflight error event/reason message, empty stderr.
  Repeat show; assert no duplicate failure records. Test live/malformed/failed
  responses, stopped/missing server and claim mismatch.

```rust
assert_eq!(fixture.ok(&["show", "1"])["task"]["status"], "error");
assert_eq!(fixture.db().query_row(
    "SELECT count(*) FROM events WHERE action='error'", [],
    |row| row.get::<_, i64>(0)).unwrap(), 1);
```

- [x] Run `cargo test --locked --offline --test preflight -- --test-threads=1`;
  expect status assertion failure (in_progress).
- [x] Add preflight snapshots with task ID, claim key, latest claim event ID and
  validated saved link. Cache `herdr agent list` per saved server. On failure,
  query `herdr session list --json`; only absent/stopped server proves death.
  Reuse existing exact/terminal identity rules via `herdr::matches_owner`.
- [x] Apply confirmed death with immediate transaction and below predicate.

```sql
UPDATE tasks SET status='error',claim_key=NULL,harness_name=NULL,
 harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL,
 updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
WHERE id=?1 AND status='in_progress' AND claim_key=?2
 AND COALESCE((SELECT MAX(id) FROM events
 WHERE task_id=?1 AND action='claim'),0)=?3;
```

- [x] Insert error event and reason message only for affected row; commit.
  Add same-key reclaim/concurrent scan tests using real SQLite connections and
  delayed fake Herdr reply. No external probes inside transaction.
- [x] Run focused preflight tests; commit verified checkpoint.

### Task 2: Native Process Identity And Private Claim Storage

Files: create `src/process.rs`, `src/sql/migrate_v13.sql`; modify `src/db.rs`,
`src/main.rs`, `src/preflight.rs`; extend `tests/preflight.rs`.

- [x] Write native owner regression: actual codex-named parent claims task,
  remains alive -> unchanged; kill/wait parent -> next show sees error. Test
  unrelated live PID with changed start/executable, foreign machine, unreadable
  evidence and unbound manual owner.
- [x] Run focused test; expect no native owner failure.
- [x] Implement ProcessIdentity serde record (machine, PID, start timestamp,
  executable). Load complete `ps` snapshot with C locale; reject malformed or
  unsuccessful snapshot. macOS machine UUID from ioreg, Linux machine-id plus
  PID namespace. Walk caller ancestry for recognized actual harness executable;
  no arbitrary manual token inference. Match PID and full fingerprint, reject
  zombie as dead. Inspection failures return unknown.
- [x] Add private storage:

```sql
CREATE TABLE claim_processes (
 task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
 claim_key TEXT NOT NULL,
 claim_event_id INTEGER NOT NULL,
 process_json TEXT NOT NULL CHECK(json_valid(process_json))
);
PRAGMA user_version=13;
```

- [x] Set SCHEMA_VERSION13, migrate after12, delete stale binding on fresh claim,
  capture process with current claim event ID inside fresh-claim transaction.
  Existing assignment reuse leaves process record unchanged. Preflight process
  death has same conditional mutation guard as Herdr death.
- [x] Run process unit/native/preflight tests; commit verified checkpoint.

### Task 3: Command Startup, Recovery And Compatibility

Files: modify `src/main.rs`, `src/db.rs`, `src/watch.rs`,
`tests/reopen_orphan.rs`, `tests/tui_dashboard_pty.py`,
`tests/tui_pty.rs`, schema assertions and compatibility fixture inventory.

- [x] Add startup checks for status/doctor/dry-run and TUI plus no-write live
  inspection test. Run focused tests and observe old behavior.
- [x] Scan current project before run, using existing read-only opener; missing,
  unsafe or old-schema inspection never creates/migrates. Open R/W only when
  observed dead claim exists. Db::open scans after migration to handle legacy
  writable paths. Avoid second scan when startup already checked current schema.
- [x] Extend reopen admission to error whose newest event is qqq-preflight error.
  Preserve manual error rejection. Update orphan recovery expectations to account
  for prior automatic failure, retaining original messages/history/link.
- [x] Pin schema13 SQL from committed feature SHA/blob. Extend fixture generator
  and inventory through13; preserve historical DB/snapshot bytes. Update canonical
  expected schema values and current-schema assertions. Add claim_processes empty
  migration checks. Update support documentation/current schema table.
- [x] Run preflight/reopen/identity/compatibility/doctor/dry-run/PTY focused suites;
  commit verified checkpoint.

### Task 4: Verification, Review, Integration

Files: modify `docs/reference.md` and this plan with final evidence.

- [ ] Document synchronous preflight, proof/unknown cases, command inspection
  semantics, manual retry/orphan recovery, one scan per command/TUI startup.
- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --locked --offline
  --all-targets -- -D warnings`, `cargo test --locked --offline --
  --test-threads=1`, `cargo build --locked --offline --release`.
- [ ] Request precise-context code review; fix important findings with regression
  tests and rerun affected checks. Verify diff/checklist and commit evidence.
- [ ] Rebase/fast-forward locally using existing authorization, install qqq,
  verify installed native/Herdr dead/live/unknown smoke and TUI startup. Record
  evidence on #181, complete task, remove feature worktree/branch, resume one
  `qqq next --wait --local --json` waiter without polling/restarting silent wait.

## Progress

Baseline: 705 tests across41 binaries. Core preflight/native tests15 pass,
autodetection27 pass; focused compatibility/doctor/reopen/DB suite91 pass.
TUI startup/reopen scenarios pass. Clippy clean before final startup-scope patch.
Review found/fixed stale relink, unbounded probes, malformed owner metadata,
foreign machine under denied process access, archive recovery. Re-review found
no important issues. Historical DB/snapshot/image bytes unchanged (30 artifacts).
Full regression running; first attempt stopped after older autodetection
fixtures consulted live Herdr and stalled owned-task waiter. Fixtures now isolate
Herdr. Installed verification and integration pending.
