# Stdin and Batch Import Implementation

> **For agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to execute this plan task-by-task.

Source: [design](../specs/2026-10-03-stdin-batch-import-design.md), task #149.

## Task 1: Input and Atomic Core

**Files:** `src/main.rs`, new `src/import.rs`, `src/db.rs`, `src/output.rs`,
new `src/output/import.rs`, new `tests/cli/import.rs`, `tests/cli.rs`.

- [x] Run CLI/priority/dependency baseline before edits.
- [x] Add failing CLI tests for stdin literal content, conflicting inputs and
  encoding errors; batch file/stdin preview, forward refs and no writes.
- [x] Add stdin flag with Clap conflicts; read UTF-8 and validate before DB open,
  feed ordinary direct-add flow so parent/priority/images stay compatible.
- [x] Add Import path/dry-run command. Read/parse input and validate static schema
  before DB open; early read-only path for dry-run skips migration/recovery.
- [x] Define strict versioned batch structs, parent ref variants and result types.
  Validate keys/descriptions/priorities, reference resolution and graph cycles.
  Stable topological order uses min input-index heap and indexed key lookup.
- [x] Share DB parent validation; validate all existing parents in same transaction
  before inserting. Dry-run deferred read; import immediate transaction and one
  commit. Bound inserts allocate key mapping and resolved parent IDs atomically.
- [x] Add human renderer with count, keys/IDs or preview fields; existing output
  sanitization prevents controls. JSON schema fields deterministic/documentable.

## Task 2: Regression Matrix

**Files:** `tests/cli/import.rs`; optional core tests in `src/import.rs`.

- [x] Stdin Unicode/multiline/CRLF/literal shell text, blank/invalid UTF-8/conflicts;
  stdin add parent/priority/images, unchanged default interactive add.
- [x] File and stdin batches, forward/local/existing parents, priority defaults,
  deterministic insertion/mapping, empty no-op and exact whitespace preservation.
- [x] Malformed JSON, missing/unknown/duplicate fields, duplicate/blank keys,
  invalid priorities/descriptions/parent IDs, missing refs, self/multi-node cycles.
- [x] Existing parent archived/status checks, dry-run readonly triggers/DB bytes,
  pending deletion staging, old-schema migration refusal and missing project.
- [x] Trigger second-write failure, assert tasks/events/messages/images/sequence
  unchanged; retry succeeds without ID gaps caused by rolled-back batch.
- [x] Concurrent batches/import-vs-add use correct references and contiguous batch
  allocations; external writers cannot change existing parent state within import.
- [x] Human output counts/key IDs/control escaping/NO_COLOR; JSON fields/types.
- [x] Run focused CLI/import/dependency/priority checks and full locked suite.

## Task 3: Documentation and Integration

**Files:** `docs/reference.md`, `README.md`, this plan.

- [x] Document stdin conflicts and piping, version-1 schema, defaults/validation,
  forward refs/order, dry-run/current-schema behavior, atomic commit and JSON result.
- [x] Run fmt/full tests/Clippy/diff checks; request existing reviewer read-only,
  fix findings and repeat affected checks. Record validation evidence.
- [ ] Commit, rebase/resolve concurrent master updates, verify combined changes,
  fast-forward, install locked/offline release and smoke installed stdin/import.
- [ ] Record integration before cleanup; remove merged worktree/branch, complete
  #149 and resume persistent `qqq next --wait --local --json` without polling.


## Verification Checkpoints

- Baseline CLI/priority/dependency suites passed before edits.
- Initial seven regressions failed on missing `--stdin` / `import` command.
- Eleven focused stdin/import tests passed, including malformed/strict schema
  cases, forward/existing refs, byte-preserving dry-run, pending staging,
  rollback with unchanged sequence, concurrent batches/adds and parent archival.
- SQLite CHECK rejects text with only spaces before leading NUL; preview now
  applies same validation. New case failed before fix, passed after.
- Initial full locked suite: 540 tests passed across 37 test binaries.
- Formatting, Clippy with denied warnings and diff whitespace check passed.
- Review found stdin needed same SQL blank-text validation before DB open. Shared
  validator now serves stdin, batch parsing and add persistence. Legacy-schema/
  pending-staging regression failed before fix and passed afterward. Valid NUL
  suffix remains preserved. Reviewer approved with no remaining findings.
- Final focused suite: 12 tests passed; full locked suite: 541 tests passed.
  Formatting, Clippy with denied warnings and diff check passed again.
- Local integration and installed CLI smoke follow.
