# File-backed task images

## Outcome

Image bytes live in `.qqq/images/<task_id>/<image_id>.<ext>`; SQLite stores only image ID, task ID, original name, media type, and byte count. Extensions follow validated media types: `png`, `jpg`, `gif`, `webp`. `qqq show` keeps its current JSON image metadata and `--export-image` behavior. Task IDs, image IDs, attachment order, and ownership behavior stay stable.

## New writes

`Db` retains its `.qqq/images` path. `add` and `edit` validate all image inputs before mutation, insert image metadata inside their existing immediate SQLite transaction, and write each image to a temporary file in its task directory. Storage rejects symlinked parent directories. Flush and sync bytes, then persist without replacing an existing final path. A final path left by an interrupted attempt may be reused only when its bytes match; sync reused files and directories before accepting them. A conflict aborts without overwriting. A guard removes newly persisted files when later work or a confirmed rollback fails. Release the guard after a successful commit; retain files when commit outcome cannot be proven. Files may remain orphaned after an ambiguous commit failure or process crash; referenced DB rows must never point to missing files after a successful operation.

`show` reads stored byte count from SQLite. Export looks up image by task and ID, reads its derived path, verifies byte count, then creates output only if absent. Missing or altered stored files cause an error before output creation.

## Existing databases

Opening schema versions 1–5 upgrades to version 6 under an immediate SQLite transaction. Earlier migrations run first. For each legacy image row, copy `data` to derived file path using same verified, no-overwrite writer. Preserve malformed legacy payload bytes without revalidating signatures; reject unknown media types safely. Once all files are durable, rebuild `images` table without `data`, preserve IDs and AUTOINCREMENT sequence, then commit version 6. A confirmed rollback leaves previous database schema and payloads intact, removes newly created files, and permits retry. A matching file left by a crash is reusable; a conflicting file stops migration without data loss. Concurrent openers serialize through SQLite's write lock and recheck schema version after acquiring it.

Old blob bytes can remain in SQLite free pages until compaction. Run `VACUUM` after successful migration to reclaim space; failure to compact does not invalidate migrated rows or files. Document manual `VACUUM` retry and backup of full `.qqq` directory. Backups made from SQLite alone no longer include images; stop writers before copying `.qqq` for a consistent backup.

## Verification

CLI tests prove on-disk paths and bytes, absence of `data` column, unchanged show/export JSON, scoped export, and missing-file failure. Migration tests cover v1/v3/v5 images, IDs/sequence, repeat/concurrent opens, preexisting matching and conflicting files, and retry after failure. Rollback tests confirm failed add/edit leave DB state and newly created image files unchanged. Full Rust suite, fmt, Clippy, and integrated tests must pass.
