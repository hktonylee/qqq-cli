# Stdin Descriptions and Atomic Batch Import

Task #149 adds `qqq add --stdin` and `qqq import PATH` / `qqq import -`.
Existing add/editor, parent, priority and image behavior stays compatible.

## Single Description

`add --stdin` reads stdin to EOF as UTF-8, preserving full whitespace, Unicode,
line endings and shell-looking text. Blank-only descriptions retain existing
validation. Reject combination with positional description, `--description` or
`--edit` before reading stdin. Parent, priority and repeated image flags remain
available through ordinary direct-add path. Read/validate stdin before normal DB
open so invalid encoding/blank input cannot migrate or recover project data.

## Version-1 Schema

```json
{
  "version": 1,
  "tasks": [
    {"key": "child", "description": "Run checks", "priority": 5,
     "parent": {"key": "parent"}},
    {"key": "parent", "description": "Build feature"},
    {"key": "follow-up", "description": "Review existing task",
     "parent": {"id": 12}}
  ]
}
```

Require version 1, task array, nonblank unique keys and nonblank descriptions.
Priority defaults to 0, accepts integer -100 through 100. Parent defaults to
none/null; exactly one positive existing ID or nonblank local key. Unknown fields,
invalid field types, duplicate fields/keys, unsupported versions, missing local
refs and cycles fail. Keep key spellings exact; do not trim description or key.
Empty batches are valid no-ops. Version 1 accepts no image files; existing task
attachments stay untouched. Single-description stdin still supports image flags.

Read JSON file/stdin and validate schema/local graph before opening project DB.
Use typed serde structs with unknown-field rejection. Compute stable topological
order: repeatedly choose lowest original input index among tasks whose batch
parent is already ordered. This inserts parents before children, supports forward
references, detects cycles in O(n log n), and makes created ID mapping deterministic
relative to concurrent writers. Existing DB parent must exist and satisfy ordinary
add rule (archived unfinished parents rejected; archived completed allowed).

## Transactions and Dry-run

`import --dry-run` uses current-schema `Db::open_read_only` with one deferred
transaction for existing-parent validation. No init, migration, recovery, agent
discovery, ID allocation or task/event/image writes. Older supported schema needs
normal migration command first. Preview never reserves IDs or parent state.

Real import uses normal DB discovery/migration/recovery. Within one immediate
transaction, validate all existing parents first, then insert in stable topological
order using bound values. Reuse existing parent/priority/description validation.
Resolve local keys to IDs inserted inside same transaction. A validation/write/
commit failure rolls back every imported task and AUTOINCREMENT allocation;
no import messages, events or image files are created. Other writers cannot
interleave allocations or change parent state before commit.

## Result

Both modes return `version: 1`, `dry_run`, `count`, `mapping`, `creation_order`
and `tasks`. Mapping is key -> committed positive task ID in lexicographic key
order; empty object for dry-run. Creation order lists keys in stable insertion
order. Tasks remain original input order, exposing key, description, priority,
original parent reference, nullable ID and nullable resolved parent ID. Dry-run
IDs are null; direct existing parents have known parent ID, local parent IDs
remain null. Real import exposes allocated IDs and resolved parents.

Human output reports imported/validated count, then keys and IDs (or preview
descriptions and parent refs), sanitizing control characters through existing
output helpers. JSON retains exact descriptions/keys.

## Verification

Cover stdin multiline/Unicode/CRLF/shell text/invalid encoding/blank/conflicts,
parent/priority/images and unchanged editor flows. Cover file/stdin JSON,
forward refs, existing parents, stable order, defaults/empty batches, malformed
JSON/types/unknown fields, invalid keys/priority/descriptions/missing refs/cycles.
Verify readonly dry-run bytes/tables/staging unchanged and no project creation.
Inject second-write failure to prove rollback of tasks/events/images/sequence.
Concurrent imports must allocate complete contiguous batches with correct links.
Run focused plus full suites, fmt/Clippy, review, local integration and installed
CLI piping/import checks before completing task and resuming queue loop.
