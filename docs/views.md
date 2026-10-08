# Named project views

Save reusable selectors for CLI lists, workers and TUI in one project file:

```sh
qqq view save "Ready frontend" --tag frontend --status new --readiness ready --max-completed 0
qqq view list
qqq view show "Ready frontend"
qqq list --view "Ready frontend"
qqq list --view "Ready frontend" --watch --json
qqq next --view "Ready frontend" --local --wait --json
qqq next --view "Ready frontend" --dry-run --tag bug
qqq tui --view "Ready frontend"
qqq view remove "Ready frontend"
```

`save` creates or replaces the exact name. Names are case-sensitive; spaces and
Unicode are allowed, surrounding whitespace and controls are rejected. `list`
sorts names. JSON `list` returns complete definitions; `save`, `show`, and
`remove` return the complete saved or removed definition. A missing name fails
with `INVALID_ARGUMENT`, `details.reason: "unknown_view"`.

## Persistence and lookup

Views live in `.qqq-views.json` beside the nearest project `.qqq/qqq.db`, including
when commands run from a nested directory. A missing views file means an empty
catalog. Browsing and applying views do not rewrite this file or user config.
Normal task commands retain their existing owner-recovery preflight; view
management reads project metadata without opening the DB or recovering owners.

Version 1 format:

```json
{
  "version": 1,
  "views": [
    {
      "name": "Ready frontend",
      "criteria": {
        "tags": ["frontend"],
        "filter": "priority >= 5",
        "query": "layout",
        "statuses": ["new"],
        "readiness": "ready"
      },
      "include_archived": false,
      "max_completed": 0
    }
  ]
}
```

`criteria` may be omitted. Omitted tags/statuses mean empty lists; omitted or
null filter/query/readiness adds no restriction. `include_archived` defaults to
false. Omitted or null `max_completed` means unlimited; zero hides completions,
positive values retain that many most recent completions. Supported statuses:
`new`, `in_progress`, `completed`, `error`; readiness: `ready` or `blocked`.

The whole catalog is validated before use. Unsupported versions, duplicate
names or JSON fields, unknown fields, invalid selectors, negative limits and
files over 1 MiB fail with `CONFIG_ERROR`, `details.reason: "invalid_views"`.
Unknown or malformed views fail before claim, owner recovery or Herdr dispatch.
Invalid explicit selectors are checked before project lookup.

Save/remove lock `.qqq/views.lock`, reread current definitions, then replace the
catalog atomically. Concurrent saves preserve independent views; updates retain
existing file permissions. Keep `.qqq/` ignored. Commit `.qqq-views.json` to share
views with collaborators. Portable DB backups and upgrade snapshots omit this
project source file; retain or copy it separately. DB schema and portable
snapshot format are unchanged by named views.

## Matching and precedence

`list`, `next`, and `tui` share `--tag`, `--filter`, `--query`, `--status`, and
`--readiness`. Explicit selectors combine with saved selectors using AND:

```sh
qqq list --view "Ready frontend" --tag bug --query resize
qqq next --view "Ready frontend" --filter 'priority >= 10' --dry-run
```

Every saved and explicit tag must match an exact stored label. Tags use existing
normalization and validation. Both full-description queries must match, using
Unicode lowercase comparison. Statuses within each group combine with OR;
the saved group and explicit group combine with AND. Both Luau expressions
must be truthy, preserving Luau behavior for numbers, text, false and nil.
Combined expressions retain existing compiler limits.

`ready` uses the same eligibility predicate as worker claims: new, unarchived,
completed parent and completed prerequisites. `blocked` means a new, unarchived
task that fails that predicate. Archived, completed and error tasks are never
ready or blocked. Visible ancestors remain `[context]` rows even when they fail
selectors; JSON marks them `context_only: true`. Existing tree order is retained.

Visibility is resolved separately. Explicit `--include-archived` or
`--hide-archived` overrides the saved archive default. On list/TUI,
`--max-completed N` or `--all` overrides the saved completed limit. Without
overrides, saved defaults apply, including unlimited completions. Ordinary human
lists without a view retain their configured display default. Archive flags
on `next` require `--explain`; archived rows can appear in diagnostics but never
become candidates. `--all` affects completions only.

Workers retain atomic claims, highest-priority/oldest-ID ordering, wait,
read-only dry-run and Herdr dispatch. Existing owned tasks return even after
view or selectors change; switching views never releases or transfers ownership.
A blocked/completed-only view has no new candidates. No match returns JSON
`null`; waiting workers remain asleep and empty queues create no Herdr tab.

List watch loads its view once at startup, then reevaluates tasks after DB
commits. Editing the views file alone does not refresh watch or change its
loaded definition; restart watch to use new definitions.

## TUI picker

Ctrl-B opens saved views from editor or filter. Up/Down selects a name, with
criteria shown below; Home/End selects first/last entry. PgUp/PgDn scrolls long
criteria. Enter applies; Esc or Ctrl-C cancels. First entry, `All tasks (no view)`,
keeps explicit startup selectors and visibility. Active view appears in editor
title. Picker supports narrow/resized terminals and `NO_COLOR`.

Opening picker reloads and validates the catalog. Applying a view retains
opened task, unsaved descriptions, staged new-task tags, parked drafts, caret,
editor/detail scroll and live filter text/focus. A hidden opened task remains
editable. DB refresh updates list membership without replacing dirty content.
Invalid catalog or failed selection keeps current view and drafts intact.
Switching reapplies saved defaults plus explicit startup visibility overrides.
Ctrl-T toggles completion visibility; turning on a saved zero limit reveals
all completions. A positive active limit remains in effect while visible.
Ctrl-K still opens a task hidden by saved selectors or a positive completion
limit; its list row stays governed by those criteria.
