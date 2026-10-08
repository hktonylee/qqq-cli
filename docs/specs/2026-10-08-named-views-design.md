# Shared Named Views Design

Task #184 adds reusable project views shared by CLI and TUI.

## Persistence And Commands

Nearest ancestor containing `.qqq/qqq.db` defines project root, matching existing
DB discovery. Definitions live at project-root `.qqq-views.json`, suitable for
version control. Missing file means empty catalog; browsing never creates it.
DB schema and database snapshot format remain unchanged. Project source/config
files, including views, travel with the project separately from DB snapshots.

Version 1 format:

```json
{
  "version": 1,
  "views": [
    {
      "name": "Ready backend",
      "criteria": {
        "tags": ["backend"],
        "filter": "priority >= 5",
        "query": null,
        "statuses": ["new"],
        "readiness": "ready"
      },
      "include_archived": false,
      "max_completed": 0
    }
  ]
}
```

Names are nonempty exact strings without control characters or boundary
whitespace; spaces and Unicode work. Reject unsupported versions, duplicate
names, duplicate JSON struct fields, unknown fields, invalid labels/statuses,
negative limits and invalid Luau expressions. Cap file size at 1 MiB. Validate
catalog before use; unknown/malformed views fail before project preflight,
claims or dispatch. Errors use existing typed envelopes.

Commands: `qqq view save NAME` accepts selection/visibility flags, replacing
same-name definition; `view list`, `view show NAME`, `view remove NAME` manage
catalog without DB open. Save/remove use `.qqq/views.lock`, exclusive writer
lock, temporary file and atomic rename. Preserve existing file permissions.
Catalog output is sorted by name. Missing removal/lookup reports typed error.

`list`, `next` and `tui` accept `--view NAME` plus shared `--tag`, `--filter`,
`--query`, `--status`, `--readiness ready|blocked` selectors. Save captures one
query/filter, status OR group, all tags, optional readiness, archive visibility
and completed limit. Save defaults: hide archived, unlimited completed tasks.

## Matching And Precedence

Saved selectors AND explicit selectors. Each status group uses OR internally;
saved and explicit groups use AND. Every tag must match. Both text queries must
match full description using existing Unicode lowercase substring semantics.
Luau filters preserve truthiness and typing by booleanizing each expression
before AND composition. Existing expression bounds apply to combined filters.

`ready` reuses `READY_TASK_PREDICATE`; `blocked` means new, unarchived and not
ready under that same predicate. Archived/error/completed tasks are not blocked.
Selection remains parameterized SQL inside current atomic claim transaction.
Next preserves priority/oldest-ID ordering, owned reuse, wait/dry-run and Herdr
behavior. Next always requires ready unarchived candidates, regardless of saved
visibility; blocked/error views consequently offer no new candidate.

Explicit `--include-archived` or new `--hide-archived` overrides saved archive
visibility. List/TUI `--all` or `--max-completed N` overrides saved completed
limit. Saved defaults override display config; ordinary list retains current
human display config and JSON defaults. Next archive flags remain diagnostic
visibility controls, not permission to claim archived work.

Shared selection module builds compiled filters and visible ancestor context
for CLI list/watch and TUI. Private deterministic SQLite Unicode containment
function preserves current `--query` behavior in transactional claim selection;
it is not exposed as arbitrary Luau function. No dynamic SQL from user strings.
All browsing respects archive/completed visibility before ancestor retention.

## TUI

Ctrl-B opens view picker from editor/filter. Up/Down selects a definition;
criteria preview supports inspection before Enter applies. First entry removes
named view; Esc/Ctrl-C cancels picker only. Picker is scrollable, wraps criteria,
retains selected entry at narrow sizes, supports no-color and resize.
Active view name remains visible in dashboard title. Ctrl-L/K/T and existing
editor/filter controls retain meaning.

Switching changes only view selection and visibility. Retain opened task,
unsaved description/tag drafts, parked drafts, editor caret/scroll, details
scroll, filter text and focus. Live filter text adds another AND query.
Hidden opened tasks remain editable; task DB refresh updates membership and
details without replacing drafts. Ctrl-T toggles completed visibility for
current session; toggling a saved zero limit on reveals all completed tasks.
Switching again reapplies definition plus explicit startup visibility flags.

Views load once for list/watch/next invocation. Watch keeps loaded definition
while reevaluating matches on DB commits. TUI picker reloads catalog when opened;
active selection stays stable until user applies another definition. Loading or
switching views performs no persistence writes.

## Verification

TDD covers versioned catalog persistence, project discovery, malformed/missing
views, normalized tags, Unicode query parity, status groups, readiness and
visibility overrides, ancestor context, pre-claim rejection, ownership reuse,
watch/wait/dry-run, concurrent claims and Herdr no-match behavior. Verify normal
view browsing leaves definition and DB bytes unchanged when no existing owner
preflight action is needed; save/remove affect only view files.

PTY tests cover picker criteria, startup views, dirty/parked drafts and staged
tags, caret/scroll, hidden task refresh, filter focus, resize and no-color.
Compare TUI visible IDs against CLI for equivalent selectors. Run focused and
full Rust suites, fmt, strict Clippy, independent review, release/installed
CLI/TUI checks. Integrate locally, record evidence, complete #184, resume one
persistent queue waiter.
