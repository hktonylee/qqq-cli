# Parent Dependencies

`qqq add [TITLE] --parent ID` creates a task that waits for the existing parent
to complete. Works with inline and editor input. One optional parent per task;
links are immutable, parents must already exist, so CLI cannot create cycles.
Reject missing parents before launching an editor or inserting a task.

Task JSON includes nullable `parent_id` in every response. `next` returns the
session's existing claim first, otherwise claims the oldest pending task whose
parent is absent or completed. Blocked tasks stay pending. No ready tasks means
JSON null, including queues containing only blocked tasks. Parent release keeps
children blocked. Completion unlocks direct children; chains unlock in order.

Schema version 2 adds nullable `parent_id INTEGER REFERENCES tasks(id)`.
Existing version 1 databases migrate inside an immediate transaction on open;
recheck version inside the transaction for concurrent opens. Fresh initialization
uses the existing version 1 schema followed by the same migration. Reject unknown
versions without modification. Existing data and ownership remain intact.

Integration tests cover parent persistence, validation, queue ordering, parent
release/completion, dependency chains, concurrent claims, editor composition,
version 1 migration preserving all task-related data, repeat/concurrent migration,
and unknown schema rejection. Run full tests, fmt, clippy and diff checks.
