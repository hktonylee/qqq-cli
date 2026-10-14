# Complete And Reopen Edit Flags

Task #228 follows #227: replace standalone `complete` and `reopen` commands
with `qqq edit ID --complete` and `qqq edit ID --reopen`. No aliases retain
removed commands. Negative creation references follow `edit` resolution.

Both flags skip the editor. They conflict with each other, `--edit`,
`--set-status`, and `--set-pending`; existing `--reason` and `--force` rules
remain unchanged. Direct description, image, priority, tags, parent,
dependency, and archive edits combine with a lifecycle flag in one transaction.
`--expected-revision` guards the entire change. Validation and late-write
failures leave status, claims, assignment, content, attachments, and history
unchanged.

Completion retains owner resolution and exact ownership checks, clears the
private active claim, preserves public assignment and saved links, and records
one `complete` event. It can combine with `--archive` after completion.

Reopen retains completed/preflight-error recovery, verified absent Herdr owner
checks for active tasks, archive/dependency guards, assignment clearing, saved
links, and one `reopen` event attributed to the supplied session or `cli`.
`--unarchive --reopen` validates and applies the final unarchived state together.
Ordinary errors still use retry; `--reopen` adds no force-recovery shortcut.

Shared connection-level lifecycle helpers execute inside the existing edit
transaction and are reused by the existing DB/TUI methods. No schema changes.
Update current documentation, dispatch prompts, and CLI/PTY callers. Historical
compatibility fixture sources and historical plans retain their old syntax.

Validation covers editor skipping, flag conflicts and removed subcommands,
atomic combined edits, ownership and revision failures, late event rollback,
existing recovery/identity/dispatch and terminal behavior, full tests,
formatting, strict Clippy, code review, integration and installed CLI smoke.
