# Upgrade compatibility gates implementation plan

> Execute inline with superpowers:executing-plans; independent review before merge.

**Goal:** Enforce historical DB upgrades and portable restores in CI/publication.

## Matrix regression tests

- [x] Extend shared helper with command/output access, project file hashing and
  failure assertions for human/JSON mode; preserve existing run API.
- [x] Expand migrated/restore checks for repeated normal opens, immutable bytes
  and valid claim constraints. Add six simultaneous openers for each source.
- [x] Add all-old-schema late SQL rollback/retry, embedded-image write-conflict
  rollback/retry and invalid foreign-key rejection/recovery using copied fixtures.
- [x] Cover future/zero/title/pending/corrupt DB rejection; read-only diagnostics
  and import dry-run before/after migration; next dry-run no-claim semantics.
- [x] Cover portable-restore acceptance boundary and future/legacy/corrupt/hash/
  format failures, both output modes, clean/empty destination preservation.

## Required gate

- [x] Add portable scripts/check-compatibility.sh using existing Cargo targets.
- [x] Wire identical command into Ubuntu/macOS CI and publication before package
  verification and upload. Preserve current checks/actions/permissions.
- [x] Document matrix, failure contracts, gate command and publication requirement.
- [x] Validate script syntax and execution from another cwd; validate workflow
  syntax/order and crate-package fixture inclusion without publication.

## Verification and delivery

- [x] Prove matrix detects temporary priority-default preservation fault and
  snapshot-publication fault. Restore runtime source exactly; fresh checks green.
- [x] Run gate, focused regression suite, full cargo test --locked, fmt/diff checks,
  strict all-target Clippy and release build. Investigate any repeat PTY failure.
- [x] Independent review of concurrency, rollback/isolation, negative contracts,
  immutable inventory and CI/publication enforcement.
- [ ] Commit/rebase/merge local master, verify integrated gate/installed binary,
  record queue evidence, complete171; clean owned worktree and resume wait.

## Evidence

Baseline compatibility+snapshot: 19 tests passed. Task170 full serial baseline:
651 tests / 40 binaries; parallel completed-toggle resize-click failure recorded.
- Expanded matrix: 16 tests passed; leaf gate: 133 tests / 7 binaries, invoked
  from /tmp. Same gate required on both CI OSes and before package/publication;
  both workflows parsed with Ruby YAML, wiring/order/mandatory steps validated.
- Rejection red: schema9/title snapshot restored successfully. Reused normal-open
  title guard before installation. Next red: injected pending row restored while
  bundled integrity_check returned ok; explicit snapshot status/claim validation
  rejects pending, invalid status, missing active owner and inactive owner.
- Temporary priority-default and restore-publication faults each detected by
  focused regression, exit101. Restored production bytes in finally; gate green.
- Independent review clear; reviewer ran all16 compatibility tests separately.
- Full default-parallel suite: 662 tests / 40 binaries passed, including previous
  completed-toggle PTY. Fresh fmt/diff, strict all-target Clippy, release passed.
- Crate package inventory includes all12 DBs,4 tar snapshots,new matrix modules
  and executable gate script. Offline publish dry-run requires registry HTTP;
  offline cargo package verification passed:319 files,3.2MiB. Normal-network
  publication dry-run also passed, explicitly aborted upload; registry cache
  write warning nonfatal. CLI regression runtime remained offline.
