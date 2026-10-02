# Portable Backup and Restore Design

`qqq backup DEST` writes one uncompressed tar snapshot. Relative paths resolve
from current directory. Archive contains `manifest.json`, `qqq.db`, and one
`images/<task-id>/<image-id>.<ext>` entry per database image row. Manifest
format version 1 records database size and SHA-256, plus each image path, size,
and SHA-256. Archive paths always use `/` across platforms. Backup refuses an
existing destination, destination inside project `.qqq`, symlinked destination
parent, missing or mismatched image
bytes, invalid database integrity, and invalid foreign keys. It creates archive
in a same-directory temporary file, syncs it, then installs with no-clobber
and syncs destination directory. Publish-sync failures remove destination.

Backup starts `BEGIN IMMEDIATE` on project database before snapshotting.
SQLite `VACUUM INTO` through a second connection creates a standalone database
copy while that write reservation blocks other qqq writers. Image rows and
files are copied to private staging while reservation remains held. Existing
`ImageStore::read` enforces real directories, regular files, and database byte
counts. Once staging is complete, release reservation; hash and package staged
files. Writers can resume while tar is built. Database copy receives
`integrity_check` and `foreign_key_check` before archive publish.

`qqq restore SOURCE` operates on current project directory without opening or
initializing its database. Reject if another `.qqq` exists in an ancestor or
current `.qqq` contains any file. Source must be a regular file. Parse tar
entry-by-entry without automatic extraction: require manifest first; allow
only regular files with exact manifest-listed paths; reject absolute paths,
parent traversal, links, duplicates, extra or missing files, unsupported
format, and oversized manifest. Copy only validated names into private staging
under current directory. Verify hashes, sizes, image count against database
rows, schema version, SQLite integrity, and foreign keys before installing
staged `.qqq`. Sync staged files/directories, rename staging into place, then
sync destination directory. Validation and install failures preserve
pre-existing empty `.qqq`; if final directory sync fails after rename, error
names installed path for inspection. Archive paths join destination only after
strict grammar and manifest membership pass.

Both commands return JSON with destination/source, task count, image count,
and byte count; human output gives same counts. Tests cover round-trip with
images/history, simultaneous writer, corrupt manifest/database, missing or
wrong-sized images, unsafe tar entries, failed staging/copy, refusal to
overwrite, and unchanged destination on failure. README replaces stop-writers
copy guidance with commands, archive format, and restore location rules.
