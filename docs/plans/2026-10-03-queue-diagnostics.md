# Queue Diagnostics Implementation

> **For agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to execute this plan task-by-task.

Source: [design](../specs/2026-10-03-queue-diagnostics-design.md), task #147.

## Task 1: Read-only DB and Shared Rules

**Files:** `src/db.rs`, new `src/queue.rs`, `src/main.rs`, `src/output.rs`,
new `tests/cli/queue.rs`, `src/output/queue.rs`.

- [x] Run priority/dependency/CLI/filter baseline before changes.
- [x] Add CLI regressions for missing command/flag, empty report and no directory
  creation. Add legacy schema test requiring error without migration or file changes.
- [x] Extract database-path discovery into helper used by existing open and new
  `Db::open_read_only`. Read-only path verifies current schema and description
  schema with SELECT; construct ImageStore path without filesystem writes.
  Introduce shared schema-version constant for existing validation and readonly.
- [x] Extract `READY_TASK_PREDICATE` and make candidate/owner-key helpers crate-visible.
  Have dispatch availability methods use candidate helper; claims/preview unchanged.
- [x] Add typed diagnostic structs/enums in queue module, serde snake_case names.
  Implement one deferred read transaction. Read task rows with ready predicate and
  bound compiled filter; retain all rows for blockers, then scope output.
  Counts partition task statuses; ready/blocked partition new. Compute ranks with
  safe comparisons (no priority negation). Diagnostic selection reuses first evaluated eligible ID to keep time-sensitive filters coherent.
- [x] Read latest activity in same transaction using one combined task/message/event
  query and timestamp/source/ID order. Resolve explicit owner with shared helper,
  never session discovery. Owned lookup precedes queued candidate and ignores filter.
- [x] Wire Status and Next explain flags. Reject wait/dry-run combinations and require
  explain for next archive flag. Handle diagnostics before normal DB open/recovery
  and exclude explain from next_owner discovery. Ordinary routes unchanged.
- [x] Wire output formats and renderer with escaped strings. Status human omits
  completed rows; JSON full scoped report. Explanation human includes every row.
  Render counts, state/outcome, candidate order, reasons, blockers, owner and activity.

## Task 2: Behavioral Regression Matrix

**Files:** `tests/cli/queue.rs`; optional internal tests in `src/queue.rs`.

- [x] Empty, completed-only, ready priority ties and blocked-chain fixtures. Compare
  explain selection to dry-run; claim real candidate, verify count transition.
- [x] Error-only, mixed unavailable, active-only and archived cases. Default hides
  archived rows, include exposes archive reason; archived tasks never selected.
- [x] Filter matching/exclusions and no matching ready distinguish from no ready.
  Invalid filters fail before mutation. Owned reuse ignores filter and archived scope.
- [x] Explicit owner/harness lookup including exact key, metadata alias and ambiguity.
  Identity overrides never update metadata. Anonymous explanation ignores native
  session and cannot launch Herdr (failing/logged executable stub).
- [x] SQL mutation-denying triggers on tasks/messages/events/links; report leaves all
  rows/claims/activity unchanged. Explain with configured dispatch avoids calls.
- [x] Latest update/message/event fixtures with deterministic timestamps and ties;
  stale activity keeps ownership. Human status includes counts, owners/blockers and
  meaningful queue-state messages, escapes controls, no ANSI in pipe/NO_COLOR.
- [x] Concurrent writer transitions parent/task while repeated reports assert
  counts, selected task and blocker statuses agree within each snapshot.
- [x] Run focused queue plus existing next/dry-run/wait/dependency/priority/filter/
  identity/dispatch suites, then full locked test suite.

## Task 3: Documentation and Integration

**Files:** `docs/reference.md`, `README.md` if command overview benefits, this plan.

- [x] Document flags, snapshot/read-only/schema behavior, JSON fields/reason codes,
  default human scope, owner context, ordering and no ownership expiry.
- [x] Run fmt, full locked tests, Clippy warnings denied, diff whitespace check.
  Request read-only review through existing reviewer; resolve findings and repeat
  affected checks. Record focused/full evidence.
- [ ] Commit, rebase onto current master, verify combined code if changed,
  fast-forward, install locked/offline release and exercise installed diagnostics,
  queued-only dry-run, ordinary owned reuse, scope/filter and no mutation.
- [ ] Record integration, remove merged worktree/branch, complete #147, resume
  persistent `qqq next --wait --local --json` without model polling.


## Verification Checkpoints

- Baseline priority/dependency/CLI/filter suites passed before implementation.
- Initial diagnostic tests failed on missing `status` / `--explain`, as expected.
- Twelve diagnostic tests passed, including mutation-denying SQL triggers,
  old-schema byte preservation, native/Herdr bypass and concurrent snapshot checks.
- Initial full suite: 515 tests passed across 37 test binaries.
- Integration must adopt schema 10 and content revision fields from task #146,
  which landed on master during this task. Combined code needs fresh full checks.

- Rebasing onto task #146 retained schema 10 migration and `content_revision` in
  all diagnostic task objects; readiness/filter column offsets updated.
- Review found time-sensitive filters reevaluated after activity scan. Added
  failing regression that crosses a date cutoff, then reused evaluated eligible
  IDs. Regression passed; reviewer approved with no remaining findings.
- Old-schema diagnostics test covers versions 1 and 9 without migration or byte changes.
- First combined full run hit pre-existing TUI Escape/menu input race. Focused
  replay passed; test used fixed 100 ms pause between staged cancellation keys.
  Replaced pause with rendered filter-clear/menu-open state before second Escape.
  Production TUI behavior unchanged. Fresh combined full run required.

- Final combined schema-10 suite: 529 tests passed across 37 test binaries.
- `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`
  and `git diff --check` passed. Review approved both diagnostics fix and TUI
  harness synchronization, with no remaining findings.
