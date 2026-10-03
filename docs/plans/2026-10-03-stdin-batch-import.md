# Stdin and Batch Import Implementation

> **For agent:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to execute this plan task-by-task.

Source: [design](../specs/2026-10-03-stdin-batch-import-design.md), task #149.

## Task 1: Input and Atomic Core

**Files:** `src/main.rs`, new `src/import.rs`, `src/db.rs`, `src/output.rs`,
new `src/output/import.rs`, new `tests/cli/import.rs`, `tests/cli.rs`.

- [ ] Run CLI/priority/dependency baseline before edits.
- [ ] Add failing CLI tests for stdin literal content, conflicting inputs and
  encoding errors; batch file/stdin preview, forward refs and no writes.
- [ ] Add stdin flag with Clap conflicts; read UTF-8 and validate before DB open,
  feed ordinary direct-add flow so parent/priority/images stay compatible.
- [ ] Add Import path/dry-run command. Read/parse input and validate static schema
  before DB open; early read-only path for dry-run skips migration/recovery.
- [ ] Define strict versioned batch structs, parent ref variants and result types.
  Validate keys/descriptions/priorities, reference resolution and graph cycles.
  Stable topological order uses min input-index heap and indexed key lookup.
- [ ] Share DB parent validation; validate all existing parents in same transaction
  before inserting. Dry-run deferred read; import immediate transaction and one
  commit. Bound inserts allocate key mapping and resolved parent IDs atomically.
- [ ] Add human renderer with count, keys/IDs or preview fields; existing output
  sanitization prevents controls. JSON schema fields deterministic/documentable.

## Task 2: Regression Matrix

**Files:** `tests/cli/import.rs`; optional core tests in `src/import.rs`.

- [ ] Stdin Unicode/multiline/CRLF/literal shell text, blank/invalid UTF-8/conflicts;
  stdin add parent/priority/images, unchanged default interactive add.
- [ ] File and stdin batches, forward/local/existing parents, priority defaults,
  deterministic insertion/mapping, empty no-op and exact whitespace preservation.
- [ ] Malformed JSON, missing/unknown/duplicate fields, duplicate/blank keys,
  invalid priorities/descriptions/parent IDs, missing refs, self/multi-node cycles.
- [ ] Existing parent archived/status checks, dry-run readonly triggers/DB bytes,
  pending deletion staging, old-schema migration refusal and missing project.
- [ ] Trigger second-write failure, assert tasks/events/messages/images/sequence
  unchanged; retry succeeds without ID gaps caused by rolled-back batch.
- [ ] Concurrent batches/import-vs-add use correct references and contiguous batch
  allocations; external writers cannot change existing parent state within import.
- [ ] Human output counts/key IDs/control escaping/NO_COLOR; JSON fields/types.
- [ ] Run focused CLI/import/dependency/priority checks and full locked suite.

## Task 3: Documentation and Integration

**Files:** `docs/reference.md`, `README.md`, this plan.

- [ ] Document stdin conflicts and piping, version-1 schema, defaults/validation,
  forward refs/order, dry-run/current-schema behavior, atomic commit and JSON result.
- [ ] Run fmt/full tests/Clippy/diff checks; request existing reviewer read-only,
  fix findings and repeat affected checks. Record validation evidence.
- [ ] Commit, rebase/resolve concurrent master updates, verify combined changes,
  fast-forward, install locked/offline release and smoke installed stdin/import.
- [ ] Record integration before cleanup; remove merged worktree/branch, complete
  #149 and resume persistent `qqq next --wait --local --json` without polling.
