# Upgrade Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox syntax for tracking.

**Goal:** Save durable original-schema recovery snapshot before automatic upgrades.

**Architecture:** Capture under existing SQLite write lock; shared archive validation
with distinct manifest2 metadata and explicit recovery restore. Keep ordinary portable
manifest1/schema9 lower bound. Read-only previews bypass migration and staging.

**Tech Stack:** Rust, rusqlite bundled SQLite, serde, tar, tempfile, SHA256.

## Files and responsibilities

- src/snapshot/upgrade.rs: lock-held original DB/image capture and durable publication.
- src/snapshot/format.rs: optional upgrade metadata, strict version1/version2 contracts.
- src/snapshot/backup.rs: shared version-aware DB validation and archive helpers.
- src/snapshot/restore.rs: shared safe extraction, explicit recovery validation/install.
- src/db.rs: call capture after lock/schema recheck, before migration writes.
- src/main.rs: restore recovery flag, next preview read-only early return.
- tests/compatibility/recovery.rs: historical original/upgrade/retry/concurrency matrix.
- tests/snapshot.rs: manifest constructor and malformed recovery archive contracts.
- docs/reference.md, fixture README: commands, location, recovery and preview boundaries.

## Task1: Pre-upgrade capture and original-schema recovery

- [ ] Add regression for every old fixture: normal list, exactly one archive outside
  .qqq; manifest source schema, SHA256/image hashes; recover into empty directory;
  compare all sqlite_master/table rows and allocation sequences before normal open;
  retry upgrade via existing assert_migrated helper. New/current projects save none.
  Example assertion: assert_eq!(recovered_state, original_state).
- [ ] Run cargo test --locked --offline --test compatibility recovery; expect missing
  archive failure, preserving genuine red before runtime edits.
- [ ] Add Upgrade metadata with source_schema,target_schema,project,created_at;
  optional #[serde(default,skip_serializing_if="Option::is_none")] Manifest.upgrade.
  validate(): version1 requires None; version2 requires supported metadata. Existing
  manifest1 serialized bytes/fixtures remain compatible.
- [ ] Share schema-aware validation (owner_session for1/2, assignee3/4, claim_key5+),
  task status/claims, integrity/FK/dependency checks. Portable min9, recovery min1.
- [ ] Capture with VACUUM INTO separate RO connection while BEGIN IMMEDIATE held;
  embedded DB images stay embedded, external image bytes copied from live or safe
  validated deletion stage. Verify archive via shared extraction before publication;
  fsync temp, no-clobber unique schema-prefix path, fsync recovery dir/project parent.
- [ ] Invoke capture only if rechecked schema>0 && schema<SCHEMA_VERSION, before
  PendingFiles/image SQL. Human stderr reports saved path via errors::json_output().
- [ ] Add restore recovery flag and explicit version2 route before any normal DB open.
- [ ] Run focused compatibility+snapshot; commit verified capture/recovery checkpoint.

## Task2: Read-only previews and failure/concurrency guarantees

- [ ] Replace old next preview migration regression with no-change/error contract;
  snapshot root also absent. Add early read-only next dry-run route returning
  json!(db.peek_next_filtered(filter)?); skip delete::recover/session discovery.
- [ ] Add migration SQL collision and image conflict failures: original DB/images
  unchanged, verified archives retained, recover originals, repair then retry.
- [ ] Add snapshot destination regular-file/symlink/missing attachment failures:
  abort before migration, original hash tree unchanged; remove blocker then retry.
- [ ] Add six concurrent openers: one archive/source, exact pre-state and upgraded
  state; committed writer before lock acquisition reflected in both DB+attachments.
- [ ] Add WAL snapshot with committed data, and held read-lock commit failure:
  snapshot retained, upgrade rollback, release lock, retry/recover valid.
- [ ] Add tampered recovery metadata/hash/state and populated-target rejection in
  human/JSON modes; empty target unchanged, original claim ownership preserved.
- [ ] Run focused matrix and leaf gate; commit verified failure/preview checkpoint.

## Task3: Documentation, review and delivery

- [ ] Document automatic path, durability/failure behavior, explicit recovery example,
  retained original schema, next preview read-only behavior and CI matrix expansion.
- [ ] Run gate/full cargo test --locked --offline; cargo fmt --check; cargo clippy
  --locked --offline --all-targets -- -D warnings; release. Record counts/log paths.
- [ ] Request independent read-only review using requesting-code-review skill.
- [ ] Commit/rebase/local ff merge, rerun integrated gate, install qqq and run installed
  recovery matrix; verify binary hashes and claim semantics against temp projects.
- [ ] Record evidence, complete172, finalize docs commit, remove owned worktree/branch,
  resume one qqq next --wait --local --json process with no repeated queue polling.
