# Read-Only Project Doctor Design

`qqq doctor` checks nearest project `.qqq` without calling `Db::open`, so it
never initializes or migrates database. It opens existing database with SQLite
read-only flags, runs `PRAGMA integrity_check`, `foreign_key_check`, and schema
version check (current version 9). Missing database, unreadable/corrupt SQLite,
unsupported schema, or active SQLite journal/WAL sidecars become diagnostics.
Sidecars are rejected before opening database to avoid creating or changing
shared-memory files. Run doctor when other writers have stopped if sidecars
are present. SQLite reads run in one pinned snapshot through filesystem scan;
sidecars are checked again before report. A writer that stages image files
during scan therefore yields only a deferred sidecar diagnostic, not false
image damage guidance.

For supported database, read all image rows. For each, derive expected path
from task/image IDs and media type, reject unsafe path components, inspect
regular file without following symlinks, compare file size with DB bytes, and
compare header signature with DB media type (PNG/JPEG/GIF/WebP). Scan
`.qqq/images` without following links; report files absent from image rows,
unreferenced or nested directories, symlinks, unreadable paths, and unsupported image
metadata. Missing image root is healthy only when DB has no image rows.

Report is stable JSON:

```json
{"ok":false,"database":"/project/.qqq/qqq.db","schema_version":9,"tasks":2,"images":1,"issues":[{"code":"IMAGE_MISSING","path":"/project/.qqq/images/2/1.png","message":"Stored image is missing","action":"Restore image from a verified backup."}]}
```

Healthy report has `ok:true`, empty `issues`, exit 0. Any issue yields JSON
report on stdout with exit 1; human output lists counts, each issue/path, and
concrete recovery action. Fatal CLI argument errors retain existing stderr
behavior. Diagnosis never writes project data or files. Tests compare DB bytes,
mtime, and directory tree before/after healthy and unhealthy checks; cover
missing/corrupt DB, wrong schema, FK damage, missing/changed/orphan/unsafe
images, JSON and human exit behavior, and recovery docs.
