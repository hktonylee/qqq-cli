# Complete And Reopen Edit Flags Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move complete/reopen commands to atomic direct-edit flags for task #228.

**Architecture:** Extend the existing edit transition enum and reuse lifecycle
SQL/guards through helpers accepting the current transaction connection. Keep
public DB methods for unchanged TUI behavior; route CLI lifecycle flags through
`edit_guarded`, including its content-revision and attachment rollback guards.

**Tech Stack:** Rust, Clap, rusqlite, existing subprocess and PTY suites.

---

### Task 1: Establish Regression Evidence

**Files:** Modify `tests/cli.rs`; use `tests/reopen.rs`, `tests/reopen_orphan.rs`,
`tests/identity.rs`, and `tests/json_errors.rs` for existing guard coverage.

- [ ] Run the full baseline with `cargo test --locked` and retain its exit status.
- [ ] Add command-level cases for completing with description/tags/priority and
archiving, reopening with unarchiving and metadata, ownership/revision/late-event
rollback, skipped editor, removed commands, and conflicting lifecycle options.
Use the existing `project`, `ok`, `run`, and `command` helpers, snapshot `show`
before rejected edits, and compare the entire result afterward. Completion
setup still uses the old syntax until the caller migration in Task 3.
- [ ] Run `cargo test --locked --test cli edit_lifecycle`; expect parser failures
for unimplemented `--complete`/`--reopen`, rather than setup errors.

### Task 2: Add Flags And Shared Transaction Helpers

**Files:** Modify `src/main.rs`, `src/db.rs`, `src/output.rs`, `src/cli_error.rs`.

- [ ] Add these direct-edit flag definitions and include them in the editor-skip
condition and edit destructuring:

```rust
#[arg(long, conflicts_with_all = ["reopen", "set_status", "set_pending", "edit"])]
complete: bool,
#[arg(long, conflicts_with_all = ["set_status", "set_pending", "edit"])]
reopen: bool,
```

- [ ] Add `EditTransition::Complete { session, harness_name }` and
`EditTransition::Reopen { actor, force }`. Resolve completion owner with the
existing `resolved_owner`; reopening actor uses `session_input.unwrap_or("cli")`.
Existing new/error transition resolution remains unchanged.
- [ ] Extract the existing complete SQL and event into `complete_claim(conn,
id, session, harness_name) -> Result<()>`. Extract reopen validation/SQL/event
into `reopen_task(conn, id, actor, force, unarchive) -> Result<()>`. Both take
`&Connection`; public methods retain their transaction/query/commit wrappers.
The edit match calls these helpers in its transaction; `unarchive` is true
only when pending archive metadata equals `Some(false)`. Reopen archive guard
is `!task.archived || unarchive`; later normal archive validation still runs.
- [ ] Remove `Commands::Complete` and `Commands::Reopen`, execution arms,
output-format alternatives and error-command-name alternatives.
- [ ] Run `cargo test --locked --test cli edit_lifecycle`; inspect actual state
and errors, correct implementation issues, then run formatting checks.

### Task 3: Migrate Current Callers And Document Contract

**Files:** Modify current `tests/**/*.rs`, `tests/*.py` command invocations;
`src/dispatch.rs`, `tests/dispatch.rs`, `README.md`, and `docs/reference.md`.

- [ ] Migrate command argument arrays from `["complete", ID, ...]` to
`["edit", "--complete", ID, ...]`, and reopen analogously. Migrate chained
`.arg("reopen")` calls and Python `cli("complete", ...)`/`cli("reopen", ...)`.
Keep event action strings and historical compatibility fixture sources intact.
- [ ] Change the dispatch instruction from `{bin} complete {id}` to
`{bin} edit {id} --complete`; update the corresponding prompt assertion.
- [ ] Replace current documented shell invocations and the `done` alias with
`edit --complete`; document conflicts, editor skipping, transaction rollback,
revision guards, completion plus archive, and reopen plus unarchive.
- [ ] Run `cargo test --locked`; all existing suites must pass. Run
`cargo fmt --check`, `git diff --check`, and
`cargo clippy --locked --all-targets -- -D warnings`.
- [ ] Review the full diff, request read-only code review, and resolve actionable
findings. Commit with `[Refactor] Move Completion And Reopen To Edit Flags`.

### Task 4: Integrate And Verify Installed CLI

- [ ] Rebase onto current `master`, resolve conflicts in the worktree, rerun
relevant checks if upstream changes warrant it, and fast-forward integrate.
- [ ] Run `cargo install --path . --locked --force` from integrated checkout.
- [ ] In a temporary project, claim, `edit --complete`, `edit --reopen`, verify
combined metadata/archive behavior, and reject both removed commands.
- [ ] Record commit and verification in task #228, complete it with
`qqq edit 228 --complete` using its exact current owner, then resume a single
persistent `qqq next --wait --json` process.
