# Editable Dependencies

Task #29: `qqq edit <id> --set-parent <id>` changes dependency after creation;
`--set-parent none` clears it. A parent-only edit skips editor. Forced `--edit`
can combine composed content/images with parent update in one transaction.

Use typed `ParentChange` with `Set(i64)` and `Clear`. Positive IDs only; omitted
flag preserves current dependency. Validate parent existence, self-reference and
cycle formation under existing immediate edit transaction. Recursive SQL follows
parent chain with UNION so traversal terminates even for corrupted cyclic data.
Reject proposed parent ancestry containing target or lacking a root. Parent,
content, attachment and optional status writes commit together or all roll back.

Existing ownership, messages, history, attachments, creation time and Herdr link
stay intact. No ownership change unless explicitly requested through status flag.
Existing active claims remain assigned after reparenting; eligibility for fresh
claims follows current dependency. Completed parents unblock children; new,
active and error parents block. Clearing dependency makes queued task ready.

Tests: set/change/clear, parent-only editor bypass, invalid IDs, self/descendant
cycles, concurrent reciprocal edits, updated queue/wait eligibility, metadata
preservation, atomic status/content/images failures, forced editor combination,
negative task references. Docs list commands and status/ownership behavior.
No schema change required.
