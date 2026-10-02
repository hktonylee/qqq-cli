# Portable Backup and Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create consistent portable snapshots and safely restore them into empty project locations.

**Architecture:** `snapshot` module owns manifest/tar format, backup writer, and restore validator. Backup holds SQLite immediate transaction while copying DB and images into staging, then packages staged data atomically. Restore parses archive into private staging, verifies manifest and SQLite against image rows, then installs `.qqq` by rename.

**Tech Stack:** Rust 2024, rusqlite, tar, sha2, serde_json, tempfile, CLI integration tests.

---

### Task 1: Format and validation primitives

**Files:** `Cargo.toml`, `Cargo.lock`, `src/snapshot/mod.rs`, `src/snapshot/format.rs`, `tests/snapshot.rs`.

- [x] Add `tar = "0.4"` and `sha2 = "0.10"` with Cargo, record lockfile. Define versioned manifest with DB size/hash and image `{path,bytes,sha256}` rows. Use `serde::{Serialize, Deserialize}` with `#[serde(deny_unknown_fields)]`.
- [x] Write tests for accepted paths `images/7/3.png`, rejected absolute/`..`/noncanonical IDs/extensions, duplicate image entries, unsupported manifest version. Run `cargo test --locked --test snapshot`; expect missing commands/module or format validation failures.
- [x] Implement exact archive path grammar and hash helper. Require safe relative path components, positive canonical decimal IDs, known extension, unique image paths. Use streaming digest:

```rust
let mut hash = sha2::Sha256::new();
let mut chunk = [0_u8; 64 * 1024];
loop {
    let count = reader.read(&mut chunk)?;
    if count == 0 { break; }
    hash.update(&chunk[..count]);
}
let digest = format!("{:x}", hash.finalize());
```

Adapt helper to `Read` without buffering entire database. Run focused tests, commit.

### Task 2: Consistent backup

**Files:** `src/snapshot/backup.rs`, `src/main.rs`, `src/output.rs`, `tests/snapshot.rs`.

- [x] Write failing round-trip-source tests: `qqq backup snapshot.tar` returns counts, archive contains manifest/DB/image, refuses existing destination, missing or mismatched image leaves no destination, snapshot retains task/messages/events/images. Run focused tests and observe missing command.
- [x] Add `Backup { destination: PathBuf }` command and output format. Pass open `Db` and DB path to `snapshot::backup`. Validate destination parent and no-clobber; stage in same parent; acquire `TransactionBehavior::Immediate`, run second connection `VACUUM INTO ?1`, copy every DB image via `ImageStore::read` while locked, release transaction, verify staged SQLite, hash and tar files, `sync_all`, `persist_noclobber`. Core sequence:

```rust
let tx = db.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
copy_sqlite_snapshot(db_path, staged_db)?;
copy_image_rows(&tx, &db.image_store, stage)?;
tx.commit()?;
validate_database(staged_db)?;
```

- [x] Add concurrent-writer test: writer holds write reservation while adding task/image; backup waits, then captures matching DB/image state. Missing source image fails without creating destination. Run focused tests, commit.

### Task 3: Safe restore

**Files:** `src/snapshot/restore.rs`, `src/snapshot/format.rs`, `src/main.rs`, `src/output.rs`, `tests/snapshot.rs`.

- [x] Write failing tests: restore into fresh and existing-empty `.qqq`; reject nonempty `.qqq`, ancestor project, malformed tar paths/link/duplicate/extra entry, corrupt manifest/DB, missing/wrong-size/wrong-hash image; verify no target mutation on every failure. Run focused tests.
- [x] Add `Restore { source: PathBuf }` command handled before `Db::open`. Check source regular file, destination/ancestor rules, and staging path. Require manifest first and bounded length; for every tar entry, reject non-regular type and path outside exact manifest set; never call `unpack`. Extract to private stage, verify hash/size, DB schema/integrity/FK, and exact DB image metadata; rename staged `.qqq` into place, preserving existing empty target on failure. Representative guard:

```rust
ensure!(entry.header().entry_type().is_file(), "Archive entry is not a regular file");
let relative = validate_archive_path(entry.path()?.as_ref())?;
ensure!(expected.remove(&relative), "Unexpected or duplicate archive entry");
```

- [x] Run focused tests and `qqq backup --help` / `qqq restore --help`; commit.

### Task 4: Docs, review, integration

**Files:** `README.md`, this plan.

- [x] Replace stop-writers copy guidance with backup/restore examples, format, no-overwrite and empty-location behavior. Run focused tests and commit.
- [x] Run `cargo test --locked --quiet`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`; read-only review found Windows path and directory-sync issues, now fixed and reverified.
- [x] Rebase on local master, fast-forward, rerun integrated checks, install CLI, `qqq --json complete 67`, mark plan complete, remove owned worktree/branch, call `qqq --json next --wait --local` once.
