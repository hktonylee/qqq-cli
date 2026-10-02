# Guarded Task Deletion Plan

**Goal:** Delete archived leaf tasks and dependent data without stale DB/image
references; show read-only preview until explicit `--yes`.

**Architecture:** `src/delete.rs` owns preview, transaction, image staging,
rollback, and interrupted-operation recovery. CLI/output expose typed JSON and
human summaries. CLI recovers staged image dirs after DB open, before commands.

- [x] Write failing CLI tests for preview, guards, JSON/human output, attachment
  cleanup, and ID non-reuse.
- [x] Add delete preview/commit flow with under-lock validation and reversible
  attachment staging. Confirm focused tests.
- [x] Add rollback and interrupted-stage recovery tests; implement recovery and
  doctor diagnostic. Confirm focused tests.
- [x] Document backup guidance and permanent deletion behavior.
- [x] Run full tests, fmt, Clippy, diff check; request read-only review and fix
  findings.
- [ ] Fast-forward local master, rerun integrated checks, install CLI, complete
  queue task #69, remove owned worktree/branch, then call `qqq next --wait`
  once.
