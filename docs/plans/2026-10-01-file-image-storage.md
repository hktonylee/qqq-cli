# File-Backed Task Images Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store image bytes under `.qqq/images` and remove image BLOBs from SQLite without losing existing attachments.

**Architecture:** `images.rs` owns typed paths, durable no-clobber writes, reads, and rollback cleanup. `db.rs` keeps image metadata transactional, migrates legacy blobs before dropping their column, and delegates byte operations to image storage. `show` remains metadata-based; export reads file bytes.

**Tech Stack:** Rust, rusqlite/SQLite, tempfile, filesystem sync, CLI integration tests.

---

### Task 1: File storage primitive

**Files:** Modify `src/images.rs`; create `tests/image_storage.rs`.

- [x] Write tests that import `src/images.rs`. Create PNG/JPEG/GIF/WebP payloads; assert `ImageStore::path(7, 12, media_type)` yields `images/7/12.<ext>`. Write through `PendingFiles`; assert bytes match, no-clobber reusing identical bytes works, conflicting bytes fail without changing file, and dropping an uncommitted guard removes only files that guard created. Verify `read` rejects missing/nonregular/size-mismatched files.
- [x] Run `CARGO_TARGET_DIR=/private/tmp/qqq-task53-target cargo test --locked --test image_storage`; expect missing storage API failure.
- [x] Add `ImageStore { root: PathBuf }` with `new`, `path`, `write`, `read`; add `PendingFiles { created: Vec<PathBuf>, active: bool }` with `new`, `keep`, and `Drop`. Map only `image/png -> png`, `image/jpeg -> jpg`, `image/gif -> gif`, `image/webp -> webp`; reject other media types. `write` rejects symlinked directories, writes `tempfile::NamedTempFile` in task directory, syncs it, calls `persist_noclobber`, syncs directory on Unix, and records newly created final paths. Existing final path is accepted only if bytes exactly match, then synced again. `read` rejects symlinks/nonregular files and checks expected byte count.

```rust
impl ImageStore {
    pub fn new(root: PathBuf) -> Self;
    pub fn path(&self, task_id: i64, image_id: i64, media_type: &str) -> Result<PathBuf>;
    pub fn write(&self, pending: &mut PendingFiles, task_id: i64, image_id: i64,
                 media_type: &str, data: &[u8]) -> Result<()>;
    pub fn read(&self, task_id: i64, image_id: i64, media_type: &str,
                expected_bytes: i64) -> Result<Vec<u8>>;
}
```
- [x] Rerun `cargo test --locked --test image_storage`; require pass. Commit storage primitive and tests.

### Task 2: Schema v6 and legacy migration

**Files:** Create `src/sql/migrate_v6.sql`; modify `src/db.rs`, `tests/dependencies.rs`, `tests/error_status.rs`, `tests/identity.rs`, `tests/tui_db.rs`; add focused migration tests in `tests/images.rs`.

- [x] Add failing tests for opening v5 database with multiple image IDs, including deleted high ID. Assert files at `images/<task>/<id>.<ext>` contain exact legacy bytes, `PRAGMA user_version=6`, `pragma_table_info('images')` lacks `data`, JSON byte counts match, image sequence stays above deleted ID, and repeat/concurrent opens preserve results. Add retry test: precreate a conflicting target file, opening fails with v5/data intact; remove conflict, rerun and succeed. Existing legacy v1/v3 tests must expect v6 plus file bytes instead of `SELECT data`.
- [x] Run focused migration tests; expect old schema/version assertions to fail.
- [x] Write `migrate_v6.sql`: create `images_v6(id INTEGER PRIMARY KEY AUTOINCREMENT, task_id ... REFERENCES tasks(id), name TEXT, media_type TEXT, bytes INTEGER NOT NULL CHECK(bytes>=0))`; copy `id,task_id,name,media_type,length(data)`; preserve `sqlite_sequence` high-water mark; drop old table; rename new table; recreate `images_task`; set `PRAGMA user_version=6`.

```sql
CREATE TABLE images_v6 (id INTEGER PRIMARY KEY AUTOINCREMENT,
 task_id INTEGER NOT NULL REFERENCES tasks(id), name TEXT NOT NULL,
 media_type TEXT NOT NULL, bytes INTEGER NOT NULL CHECK(bytes>=0));
INSERT INTO images_v6(id,task_id,name,media_type,bytes)
 SELECT id,task_id,name,media_type,length(data) FROM images;
UPDATE sqlite_sequence SET seq=MAX(seq,COALESCE((SELECT seq FROM sqlite_sequence WHERE name='images'),0)) WHERE name='images_v6';
DROP TABLE images;
ALTER TABLE images_v6 RENAME TO images;
CREATE INDEX images_task ON images(task_id,id);
PRAGMA user_version=6;
```
- [x] Extend `Db` with `image_store: ImageStore`, rooted at opened DB's parent plus `images`. Accept versions 1–6. Under existing immediate transaction, run prior migrations then stream old image rows through `ImageStore::write` and `PendingFiles`, execute v6 SQL, foreign-key check, clean files before rollback on failed commit, and best-effort `VACUUM` for legacy image rows with warning on failure. Recheck `user_version` after lock so concurrent openers skip repeated copy. Update `tests/tui_db.rs` fixture to retain a temp directory for its store.
- [x] Run migration tests, then full suite to locate old blob assertions. Commit schema migration after all migration tests pass.

### Task 3: New writes, reads, transaction failures

**Files:** Modify `src/db.rs`, `tests/images.rs`, `tests/tui_db.rs`, `tests/force_new.rs`, `tests/parent_edit.rs`, `tests/error_status.rs` as needed.

- [x] Add failing CLI test: `add --image` writes exact bytes under `.qqq/images/1/1.png` with metadata-only `images`; `edit --image` appends `2.jpg`; deleting original source files does not affect export; deleting stored file causes export error without creating output. Add rollback assertions: trigger failure on second image insertion leaves DB task/edit state and newly written files unchanged. Update in-memory `tui_db` tests for filesystem-backed bytes.
- [x] Run `cargo test --locked --test images` plus `--test tui_db`; expect failure from old DB-backed behavior.
- [x] In `Db::save_images`, insert `(task_id,name,media_type,bytes)` inside caller transaction, get inserted ID, write bytes through `ImageStore::write` and a caller-owned `PendingFiles`. In `add`/`edit`, retain guard across all SQL changes; keep files after successful commit or ambiguous failure, clean while transaction still holds write lock before rollback. Change `show` query to select `bytes`; change export query to select `media_type,bytes`, read through store, then create output with `create_new`.

```sql
INSERT INTO images(task_id,name,media_type,bytes) VALUES (?,?,?,?);
SELECT id,name,media_type,bytes FROM images WHERE task_id=? ORDER BY id;
SELECT media_type,bytes FROM images WHERE id=? AND task_id=?;
```
- [x] Run focused CLI/TUI/rollback tests; require pass. Run full suite and fix tests that directly insert/select `images.data` against current schema. Commit DB integration and tests.

### Task 4: Docs, review, integration

**Files:** Modify `README.md`; update this plan's checkboxes.

- [x] Change README storage/backup text: SQLite holds metadata; `.qqq/images` holds bytes; stop writers and copy whole `.qqq` for backup; schema versions 1–5 migrate to 6; `VACUUM` can reclaim old free pages if automatic compaction failed.
- [x] Run `cargo fmt --all -- --check`, strict `cargo clippy --locked --all-targets -- -D warnings`, full `cargo test --locked`, and `git diff --check`; require exit 0.
- [x] Request read-only code review. Fix verified Critical/Important findings and rerun affected checks.
- [x] Rebase onto current local `master`, fast-forward locally, run integrated full suite, remove owned worktree, delete merged branch. Complete task 53 with `qqq --json complete 53`; verify `qqq --json show 53` reports completed. Resume `qqq --json next --wait --local` once.
