# Upgrade compatibility gates implementation plan

> Execute inline with superpowers:executing-plans; independent review before merge.

**Goal:** Enforce historical DB upgrades and portable restores in CI/publication.

## Matrix regression tests

- [ ] Extend shared helper with command/output access, project file hashing and
  failure assertions for human/JSON mode; preserve existing run API.
- [ ] Expand migrated/restore checks for repeated normal opens, immutable bytes
  and valid claim constraints. Add six simultaneous openers for each source.
- [ ] Add all-old-schema late SQL rollback/retry, embedded-image write-conflict
  rollback/retry and invalid foreign-key rejection/recovery using copied fixtures.
- [ ] Cover future/zero/title/pending/corrupt DB rejection; read-only diagnostics
  and import dry-run before/after migration; next dry-run no-claim semantics.
- [ ] Cover portable-restore acceptance boundary and future/legacy/corrupt/hash/
  format failures, both output modes, clean/empty destination preservation.

## Required gate

- [ ] Add portable scripts/check-compatibility.sh using existing Cargo targets.
- [ ] Wire identical command into Ubuntu/macOS CI and publication before package
  verification and upload. Preserve current checks/actions/permissions.
- [ ] Document matrix, failure contracts, gate command and publication requirement.
- [ ] Validate script syntax and execution from another cwd; validate workflow
  syntax/order and crate-package fixture inclusion without publication.

## Verification and delivery

- [ ] Prove matrix detects temporary priority-default preservation fault and
  snapshot-publication fault. Restore runtime source exactly; fresh checks green.
- [ ] Run gate, focused regression suite, full cargo test --locked, fmt/diff checks,
  strict all-target Clippy and release build. Investigate any repeat PTY failure.
- [ ] Independent review of concurrency, rollback/isolation, negative contracts,
  immutable inventory and CI/publication enforcement.
- [ ] Commit/rebase/merge local master, verify integrated gate/installed binary,
  record queue evidence, complete171; clean owned worktree and resume wait.

## Evidence

Baseline compatibility+snapshot: 19 tests passed. Task170 full serial baseline:
651 tests / 40 binaries; parallel completed-toggle resize-click failure recorded.
Implementation not started.
