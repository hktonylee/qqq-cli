# Assignee Rename

Task #10 renames `owner_session` to `assignee` in task JSON, Rust models, live
SQLite schema, and documentation. Human output uses `Assignee:`. Values remain
nullable session identity strings; `--session`, claim/release/completion events,
and authorization matching retain existing semantics. No legacy JSON field alias.

Schema version 3 renames the existing column with SQLite ALTER TABLE, preserving
values, CHECK constraints, and the unique active-session index. Keep historical
schema.sql (version 1) and migrate_v2.sql unchanged. Fresh databases apply both
migrations; existing versions 1 and 2 migrate in the same immediate transaction.
Recheck version after locking for concurrent opens. Reject newer schemas without
writes. Migration never changes task timestamps, IDs, links, attachments, or history.

Tests cover renamed output in all task responses; version 1 and 2 upgrades;
repeat and concurrent opens; retained claims and related data; index and CHECK
constraint enforcement; pending/completed null assignment; wrong-session denial.
Run full tests, fmt, Clippy, release build, and diff checks before integration.
