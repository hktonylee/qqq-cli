# Atomic Bulk Task Actions Design

Task #185 adds shared metadata transactions, frozen CLI previews, and TUI
multi-selection. No deletion, ownership/status transitions or DB migration.

## CLI Contract

`qqq bulk` previews by default. Selection uses repeated positive `--id ID`, or
existing `--query TEXT`, repeated `--status STATUS`, `--filter LUAU`, repeated
`--tag LABEL`, and `--include-archived`. IDs conflict with selectors. `--all`
explicitly selects all visible rows when no filter is wanted; archive visibility
alone does not count as a selector. Filters compose with AND, statuses with OR,
query matches whole description case-insensitively, tags require all exact labels.
Selection ignores human display completed limits and excludes context-only
ancestor rows. Explicit IDs include archived tasks; duplicate IDs are deduped,
sorted ascending. Zero/negative/missing IDs reject before writes.

Actions: repeated `--add-tag LABEL`, repeated `--remove-tag LABEL`,
`--set-tags LABELS` (existing comma parser, empty clears), `--priority N`,
`--archive`, `--unarchive`. Require at least one action. Replace tags conflicts
with add/remove; adding/removing same normalized label rejects. Priority and
archive actions may compose with tags. Archive/unarchive conflict. Reuse tag
normalization and priority bounds -100..100.

Examples:

```sh
qqq bulk --tag backend --status new --add-tag review --priority 5 --json > preview.json
qqq bulk --apply preview.json --json
qqq bulk --id 12 --id 19 --set-tags 'release,verified' --json > preview.json
qqq bulk --apply preview.json --human
qqq bulk --all --include-archived --unarchive --json
```

`--apply PATH` (including `-` for stdin) is explicit confirmation. It consumes
exact versioned JSON preview from first command, conflicts with all selectors
and action flags, never reruns selectors. Human output is readable preview;
`--json` saves applyable preview. Cancellation means not applying file. Empty
selection reports zero counts and no-op apply creates no writes/events. Applied
result cannot be reapplied as preview. Old valid preview may apply only while
all guarded state still matches; no-op preview remains harmless.

## Frozen Preview And Errors

Version1 typed report contains `version`, `applied`, canonical `database` path,
normalized `actions`, `selected_ids`, `selected_count`, `changed_count`,
`action_count`, and sorted `tasks`. Each row has `id`, `before` and `after`
metadata (`tags`, `priority`, `archived`), plus SHA256 `fingerprint` of relevant
DB state. Action count counts changed metadata fields, not repeated tags or IDs.
Human preview displays every selected ID and before/after value, including no-ops.

Fingerprint uses canonical serialized task fields, raw internal claim key hashed
as data, latest task event ID and related dependency state. Content revision
guards description/attachments; tags/priority/archive/status/assignment/parent/
prerequisites/creation/update data guard concurrent metadata and ownership edits.
Never output raw claim key. Messages alone need not invalidate metadata preview.
Canonical DB path prevents applying file in unrelated project. No project secret,
reserved IDs, persisted preview record or new table required.

Strict deserialization rejects unknown/duplicate JSON fields, wrong types,
unsupported version, malformed hashes, unsorted/duplicate IDs, inconsistent
counts and before/after values. Recompute after-values from actions and before
metadata; inconsistent preview rejects. Imported preview is data only.
Missing task or fingerprint mismatch yields structured `BULK_CONFLICT` with
reason, selected IDs and conflict IDs; no description/private claim disclosure.
Bad CLI/preview yields `INVALID_ARGUMENT`, malformed filters `INVALID_FILTER`,
DB contention existing `DB_BUSY`, archive rule failures `INVALID_TRANSITION`.
Conflict directs user to regenerate preview and review again.

## Transaction And Archive Validation

Preview opens existing current-schema DB read-only, starts deferred read
transaction, resolves selection and expands before/after in one snapshot.
No task/claim/ID/schema/deletion-recovery writes. Existing synchronous owner
liveness preflight remains separate, runs before bulk command as for other
commands, and may mark proven dead owners before snapshot.

Apply parses/validates file before writable DB open. Shared core starts immediate
transaction, checks project binding and every selected fingerprint, computes
whole projected metadata set and validates every selected task before SQL writes.
Then updates only changed metadata fields and `updated_at`, inserts ordinary
archive/unarchive events only for actual visibility changes, commits once.
Any validation/insertion/event/commit failure rolls back complete batch.
Owners, native claim records, links, dependencies, descriptions, content
revisions, attachments, messages and prior history remain intact.

Extract existing archive validation into shared helper used by single-task and
bulk calls. With no overrides, preserve single-task behavior/messages. Bulk checks
projected final archive values: in_progress rows cannot become archived; archiving
unfinished row requires every unfinished child/dependent to be archived in final
state; unarchiving unfinished row requires parent/extra prerequisites available
or completed in final state. No-op archive requests preserve current behavior.
Related row changes between preview/apply that affect archive readiness are
conflicts or fail current validation; transaction cannot leave invalid partial
graph. Batch parent/child archive/unarchive works regardless of ID order.

## TUI Selection And Preview

Ctrl-D toggles current saved task in bulk selection, using existing one-cell blank
list prefix as `+` marker. Dirty marker `[*]` and focused-row background stay
distinct. Shift-Up/Down and mouse navigation retain existing parked draft flow.
Ctrl-D on new task reports instruction rather than assigning invented ID.
Selection survives filter/status toggles and refresh, so hidden selections remain
explicitly visible in bulk menu/preview list. Ctrl-G opens existing single-task
menu when selection empty, bulk menu when selection nonempty. Bulk menu shows
selected count/IDs, tag add/remove/replace, priority, archive, unarchive and clear
selection. Clear selection changes no task.

New `src/tui/bulk.rs` owns typed menu/input/preview/error state and rendering.
Tag input retains existing multiline/grapheme cursor behavior: Enter newline,
Ctrl-S preview, Ctrl-U current-line delete. Numeric input Enter previews. Preview
shows full selected IDs, all before/after changes and counts, supports scroll,
provides explicit `y apply`, `n/Esc cancel`. Enter defaults cancel; Ctrl-C closes
popup. Text labels and markers work under NO_COLOR, small terminals, resize.
Reuse existing popup layout; rows scroll rather than clipping unreviewable data.

Shared bulk preview/apply core operates directly on dashboard DB connection.
No reload/save of active editor or parked buffers on apply/cancel/conflict.
Preserve caret, editor/list/details scroll, filter text/focus, dirty descriptions,
attachments and unsaved tags. Bulk metadata updates on dirty tasks stay separate
from saved content; archive may hide task while draft stays available in buffer.
Successful apply clears selection and refreshes rows/details; conflict retains
selection and draft, asks for fresh preview. Errors never auto-confirm.

## Validation And Delivery

CLI subprocess tests: direct selectors/context ancestors, multiple/duplicate IDs,
empty selection, tag add/remove/replace, bounds, malformed selectors/preview,
human/agent defaults, readonly bytes/staging/old-schema guards, exact frozen IDs,
new matching rows, stale tag/priority/content/owner/dependency edits, delete/reuse,
cross-project preview, active/related archive restrictions, projected graph
archive/unarchive, no-ops, full field/history preservation, late SQL/event rollback
and concurrent competing applies.

PTY tests observe rendered frames before DB assertions. Cover multi-select,
scrollable preview/cancel/confirm, all actions, dirty parked/current descriptions,
caret/paste/attachments/filter/scroll preservation, hidden selection, stale
preview, invalid active archive, no color and compact resize. Run RED before
impl, focused/full suites, fmt, strict Clippy, release, focused review. Rebase,
integrate, install; verify installed CLI and PTY plus historical compatibility,
record qqq evidence, complete #185 and resume one persistent queue waiter.
