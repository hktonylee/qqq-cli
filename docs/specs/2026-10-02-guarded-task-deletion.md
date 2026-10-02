# Guarded Permanent Task Deletion

`qqq delete <id>` returns read-only preview: task identity, dependent data
counts, stored image paths, backup advice, and `deleted:false`. `--yes` repeats
all guards under `BEGIN IMMEDIATE`, deletes data, returns same summary with
`deleted:true`. Both JSON and human output distinguish preview from completed
deletion. A task must be archived and not in progress; any child, including
archived/completed child, blocks deletion. Errors name required archive,
release/complete, or child reparent/delete step.

Deletion removes messages, events, Herdr link, image rows, and task in one
SQLite transaction. Image paths come only from DB IDs and supported media
types. Before moving files, verify real image directories, regular files, and
that task image directory contains exactly those expected files. A missing or
unsafe attachment aborts with DB unchanged. Hold SQLite write lock while
renaming task image directory to `.qqq/.delete-staging/<id>`, deleting rows,
and committing. If SQL or commit fails, rename directory back before rolling
back, so DB references remain valid. Sync directory entries around rename.

Staged directory allows interrupted operation recovery. On next DB open,
recover staging under SQLite write lock: task row present -> rename images
back; task row absent -> finish file cleanup. Reject unsafe/conflicting stage
paths. `qqq doctor` reports pending staging and directs user to run qqq command
for recovery. SQLite `AUTOINCREMENT` retains deleted task identity, including
deleting highest ID.

Tests cover preview immutability, JSON/human summaries, guard failures, all
dependent data and image cleanup, no ID reuse, SQL-trigger rollback after
staging, missing/unsafe files, interrupted-stage recovery on both sides of DB
commit, and concurrent guard recheck. README recommends `qqq backup` before
permanent deletion.
