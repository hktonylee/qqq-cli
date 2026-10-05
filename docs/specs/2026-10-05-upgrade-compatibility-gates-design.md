# Upgrade compatibility gates

Task171 extends immutable task170 fixtures into explicit local, CI and publication
gate. Normal-open DB schemas1–12 remain distinct from portable format1 restore
schemas9–12. Existing expected state covers IDs, sequences, descriptions, status
and claims, public identities, parents/history, priority/archive/revisions,
prerequisites/readiness, tags and blob/file image bytes.

## Expanded matrix

- Reopen every migrated DB and restored snapshot: canonical rows and image/file
  hashes remain stable. Exercise DB CHECK/unique constraints on migrated copies.
- Six concurrent CLI openers per source schema share a locked temporary project.
  All succeed; one migration preserves canonical state, sequences and images.
- Inject late migration SQL collision into every older schema. Whole transaction
  rolls back, source DB bytes and image bytes survive, new image files disappear;
  remove collision and retry to exact expected state. Separate earlier schemas
  exercise deterministic second-image write conflict and invalid foreign keys.
- Reject future/zero normal-open versions, legacy title/pending layouts and
  corrupt/unusable DBs in both human and JSON modes without changing project
  files. Future/legacy/unsupported pre9/corrupt snapshots fail before publishing
  destination. Tampered hashes and format versions are covered separately.
- On old sources, status, next --explain and import --dry-run reject migration
  without touching DB/images/deletion staging. On upgraded copies these commands
  and next --dry-run preserve canonical state and bytes. Normal next --dry-run
  retains existing normal-open upgrade semantics while never claiming/dispatching.

All mutations target TempDir copies. Golden artifacts, provenance and expected
JSON remain unchanged. Failure injection uses portable SQLite/schema/path
collisions, avoiding filesystem permission assumptions and runtime network.

## Gate wiring

`scripts/check-compatibility.sh` resolves repository root and runs locked Cargo
compatibility, identity, images/image_storage, dependencies, snapshot and CLI
integration targets, including dependency/tag regressions. Same command runs
as named step in existing Ubuntu/macOS
CI and Ubuntu publication job before package verification/upload. Existing full
tests, fmt, strict Clippy and build checks stay required. Reuse normal-open legacy-title guard and explicitly validate snapshot status/claim
relationships before installation. Bundled SQLite integrity_check returned ok for
injected pending rows; explicit data check rejects pending and invalid owners.
No new dependencies,
coverage/maintainability policies or hooks in this focused task.

Inventory probes current schema and emitted format. No schema version is skipped
or inferred away when runtime changes; missing fixture/expectation fails gate.
Prove enforcement with temporary priority-default and restore-publication faults,
then restore production bytes and run fresh green checks.

## Verification constraint

Task170 observed unchanged completed-toggle resize-click timeout in default
parallel PTY runs; isolated and serial full runs passed. Task171 must investigate
repeat failures if full default run fails. Any fix needs concrete root-cause
evidence and independent review; never weaken compatibility assertions or skip
old fixtures to obtain green CI.
