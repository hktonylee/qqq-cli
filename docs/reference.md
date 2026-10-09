# qqq reference

[← README](../README.md)

Detailed behavior and flags for qqq. For a first run, start with the
[quick start](../README.md#try-it).

- [Install from source](#install-from-source)
- [List and search](#list-and-show) · [Add and edit](#add-and-edit)
- [Named project views](views.md)
- [Dependency graph and completion impact](graph.md)
- [Archive](#archive-and-unarchive) · [Reopen](#reopen-work)
- [Bulk metadata actions](#bulk-task-actions)
- [Editor](#built-in-editor) · [TUI](#task-tui)
- [Dependencies and images](#dependencies-and-images)
- [Ownership and recovery](#agents-and-recovery)
- [Config and aliases](#config-and-aliases) · [Herdr](#herdr)
- [JSON error contract](#json-error-contract)
- [Backup, restore, deletion, health](#data)
- [Development](#development) · [Publishing](#publish-to-cratesio)

## Install from source

Requires Rust 1.85+ and a C compiler. This guide follows the development branch;
published releases may lag features described here.

```sh
git clone https://github.com/hktonylee/qqq-cli.git
cd qqq-cli
cargo install --path . --locked --force
```

Task commands use the nearest `.qqq` directory in the current directory or its
parents. A missing `qqq.db` there causes an error. `qqq config` needs no project DB.

## List and show

```sh
qqq list
qqq list --oneline
qqq show 1
qqq show -1                         # newest task; -2 is second newest
qqq list --max-completed 10
qqq list --all                      # bypass configured completion limit
qqq list --query "parser error"     # search full descriptions, ignoring case
qqq list --status new --status error
qqq list --query "parser" --status in_progress --all
qqq list --filter 'like(task_name, "%auth%") and priority > 0'
qqq list --tag frontend --tag "UI review"
qqq list --readiness blocked
qqq list --view "Ready frontend"
qqq list --watch
qqq list --watch --json
qqq list --include-archived
```

`list` shows status, priority, and dependency tree; roots and siblings follow
creation ID order. In a terminal with known width, it shows full descriptions
with wrapped, aligned continuation lines; piped output keeps first-line previews.
`show` adds messages, images, ownership history and Herdr link. Negative indexes
work with `show`, `edit`, `archive`, `unarchive`, and `reopen`; `-1` selects newest task.

`--oneline` limits terminal and watch views to each description's first line.
JSON and search still use full descriptions.

`--max-completed N` keeps N most recent completions plus unfinished tasks;
`0` hides completed tasks. Hidden parents display children as roots. `--all`
bypasses configured limit; it conflicts with `--max-completed`.

`--query TEXT` matches any part of a full description, including later lines,
using Unicode lowercase comparison. Repeat `--status` to match any listed
status: `new`, `in_progress`, `completed`, or `error`. Query and statuses combine
with AND. Search runs after completed-display limit; use `--all` to search all
completed tasks. Visible ancestors of matches appear as `[context]` rows even
when they do not match filters. JSON stays flat and marks those ancestors with
`context_only: true`; direct matches keep existing task fields. Empty filtered
results return `[]` in JSON or `No matching tasks.` in human output.

`list --filter EXPR` and `next --filter EXPR` compile a typed Luau expression
to SQLite with bound values. List filters combine with query/status and retain
visible ancestor context. See [filter guide](filter.md),
[default variables](filter-variables.md),
[SQLite functions](filter-functions.md).

`list --tag LABEL` and `next --tag LABEL` match whole stored labels with exact
case/Unicode. Repeat to require every label; tags combine with other filters
using AND. Boundary whitespace is trimmed. Invalid labels fail before project
preflight or DB access. `has_tag("LABEL")` offers the same membership test in
Luau expressions; exactly one valid string literal is required. See
[tag selectors and named views](filter.md#list-behavior).

`list`, `next`, and `tui` share tags, Luau filter, Unicode query, status groups and
`--readiness ready|blocked`. Ready uses existing parent/prerequisite eligibility;
blocked includes only new unarchived tasks failing that predicate. Save these
selectors with `qqq view save NAME`, inspect using `view list`/`view show NAME`,
remove using `view remove NAME`. `--view NAME` applies the same project definition
across CLI/TUI. Extra selectors combine using AND; explicit archive/completed
flags override saved visibility. See [versioned file, precedence and picker](views.md).

`--watch` prints initial list, then refreshes after DB commits. Terminal output
redraws; pipes, `TERM=dumb` and JSON append snapshots. Ctrl-C stops watching.

Default list, watch, TUI, and claim queue hide archived tasks. Use
`list --include-archived`, `list --watch --include-archived`, or
`tui --include-archived` to browse them. Human lists label archived rows
`[archived]`; JSON includes `archived: true` without changing description.
`--all` changes completed-task limit only; hidden archived completions do not
consume that limit.

Global `--json` and `--human` work before or after commands; they conflict.
Without either flag, recognized agent callers receive JSON: a nonblank UTF-8
`CODEX_THREAD_ID` or `CODEX_SESSION_ID`, or an exact current Herdr pane reporting
an agent name/session. Detection does not discover other agents at project cwd
or infer agents from pipes, `QQQ_SESSION`, or harness overrides. Missing Herdr,
failed lookup, and panes without agent metadata keep readable output. Explicit
output flags supplied directly skip optional caller lookup. Use `--human` to override automatic
JSON. Help/version remain text; TUI/editor rendering remains interactive.

JSON lists stay flat, preserve
full descriptions and `parent_id`; watch prints one array per line. Empty lists
return `[]`; no ready task returns `null`. Errors use stderr (exit 1 for runtime,
2 for arguments). JSON failures follow [error contract](#json-error-contract).
`NO_COLOR=1` or `TERM=dumb` disables color.

## JSON error contract

Failed commands in JSON mode (global `--json` or recognized agent default)
write one JSON object plus newline to
stderr. Error prose and terminal UI never appear on stdout. Runtime failures
exit 1; argument-parser failures exit 2. Existing success payloads stay unchanged.

```sh
qqq --json show 999 >result.json 2>error.json
```

```json
{"code":"TASK_NOT_FOUND","message":"Task 999 not found","details":{"task_id":999,"command":"show"}}
```

`code` is stable machine classification; branch on code and details.
`message` is readable diagnostic text; wording can change.
`details` is always an object; fields depend on failure and can grow over time.
Runtime errors include `command` when known. Omitted fields mean unavailable;
explicit `null` indicates absent current content or optional recovery file.
Parser and alias failures can occur before command resolution.

| Code | Meaning | Relevant details |
| --- | --- | --- |
| `TASK_NOT_FOUND` | Task ID or creation-order reference unavailable | `task_id` or `task_reference` |
| `OWNERSHIP_MISMATCH` | Active task belongs to different owner | `task_id`, `actual_status`, `expected_statuses` |
| `INVALID_TRANSITION` | State or dependency blocks operation | `task_id`, `actual_status`, `expected_statuses`; archive guards add `actual_archived`, `expected_archived`; dependency guards add `reason`, `blocking_child_id` |
| `INVALID_ARGUMENT` | Flag/value, alias or owner-resolution input invalid | `argument`, `reason`; alias errors add `alias` |
| `INVALID_FILTER` | Luau filter invalid or failed during evaluation | `argument: "--filter"`; runtime failure adds `reason: "evaluation_failed"` |
| `DB_BUSY` | SQLite busy/locked; retry after competing transaction finishes | `sqlite_code` (5 or 6), `sqlite_extended_code` |
| `CONTENT_CONFLICT` | Loaded content changed or task removed during edit | `task_id`, `expected_revision`, `current_revision`, `reason` (`revision_changed` or `task_removed`) |
| `BULK_CONFLICT` | Frozen bulk selection changed or DB rejected a batch write; whole batch rolls back | `conflict_ids`, `reason`; stale previews include `selected_ids` and `reason: "stale_bulk_preview"` |
| `PROJECT_NOT_FOUND` | Project DB unavailable | `command` when resolved |
| `CONFIG_ERROR` | Config cannot load or requested config operation failed | `path` |
| `IO_ERROR` | Filesystem or input/output operation failed | `io_kind`, `os_code` when available; explicit guards may add `path`, `task_id`, `reason` |
| `EDITOR_ERROR` | EDITOR missing, failed to start or exited unsuccessfully | `exit_code` when available |
| `DISPATCH_ERROR` | Herdr lookup, transport or dispatch failed | `reason`, `exit_code` when available; dispatch failures add `task_id`, `actual_status`, `delivery_possible` |
| `DATABASE_ERROR` | Other SQLite or schema failure | `sqlite_code`, `sqlite_extended_code`, or `schema_version`, `expected_schema_version`, `reason` |
| `COMMAND_ERROR` | Failure without narrower public classification | `command`; explicit guards may add `reason` |

Ownership details omit claim keys and owner identity values. Error envelopes
omit full task text, raw Herdr/editor output and arbitrary SQLite trigger text.
Human mode keeps underlying diagnostic context. Existing successful task and
diagnostic payloads keep their own identity/content fields.

Stale-save example:

```json
{"code":"CONTENT_CONFLICT","message":"Content conflict for task 1: expected revision 1, current revision 2; save rejected","details":{"command":"edit","task_id":1,"expected_revision":1,"current_revision":2,"reason":"revision_changed"}}
```

Reload current task, review changes, then explicitly retry with reviewed revision
via `edit --expected-revision N`. `task_removed` has `current_revision: null`;
do not recreate task automatically. Missing task before editor opens remains
`TASK_NOT_FOUND`. `DB_BUSY` permits retry; other codes need caller-specific recovery.

External editor save failures can add `details.recovery` with `local_draft`,
`current_text` and `attachments_manifest` paths. Draft and pending image bytes
remain available for recovery. Nonterminal JSON editor calls suppress editor
terminal output and recovery prose so whole stderr stays parseable.

Help/version still print plain help/version to stdout with exit 0, including
with `--json`. Empty `next` returns successful `null`. Unhealthy `doctor --json`
keeps diagnostic report on stdout with exit 1 and no error envelope.
JSON watch can emit successful snapshots before later failure.

Explicit interactive terminal sessions keep UI/prompts on stderr, followed by
one final JSON error line after terminal restoration if command fails. For
whole-stderr JSON parsing, supply inline fields or nonterminal stdin.
Literal `--json` after `--` or consumed as option value does not enable JSON;
fully expanded aliases determine output mode.

## Add and edit

```sh
qqq add "Fix login"
qqq add "Fix urgent login" --priority 8
qqq add "Fix layout" --tag frontend --tag bug
qqq add --description "Fix login"   # same input, alternative flag
qqq edit 1 --description "Updated details"
qqq edit 1 --priority -5             # lower future claim order
qqq edit 1 --set-tags "frontend, bug" # replace tags; skips editor
qqq edit 1 --set-tags ""              # clear tags
qqq edit -1                         # edit newest task interactively
```

Tags are ordered metadata, displayed before task names as `[frontend] [bug]`.
Repeated `--tag` flags accept one label each; `--set-tags` accepts a comma-separated
replacement list and conflicts with `--edit`. Surrounding spaces are trimmed;
exact duplicates are removed while case/order remain. Labels support Unicode
and internal spaces; empty labels, control characters, commas and square
brackets are rejected. Blank replacement input clears tags. JSON always includes
`tags: []` or a string array and keeps raw description unchanged. Metadata edits
preserve content revision, attachments, status and ownership, including on
completed/archived tasks. Combined edits commit atomically. List `--query` and
TUI filtering continue matching descriptions.

Descriptions preserve whitespace and newlines; blank-only text fails.
Priority defaults to `0` and accepts integers from `-100` through `100`.
Higher priority claims first among ready new tasks; equal priority uses oldest
ID. `list` stays in ID/dependency order. Editing priority on active, completed,
or error tasks keeps status and ownership metadata; an already-owned task still
returns to its owner before new claims.

Task JSON exposes positive `content_revision`. Description or image changes
advance it; messages, status, ownership, priority and dependency changes do not.
Use loaded revision to reject stale direct edits:

```sh
qqq show 1 --json                    # task.content_revision
qqq edit 1 --description "Updated details" --expected-revision 3
```

A stale revision rejects the whole edit, including supplied metadata and images.
Direct edits without `--expected-revision` retain unconditional-save behavior.
An identical description save leaves content revision unchanged.

Read a full description from stdin without opening an editor:

```sh
qqq add --stdin < description.txt
printf '%s\n' 'Fix login' 'Check Unicode: λ' | qqq add --stdin --priority 8
qqq add --stdin --parent 12 --image screenshot.png < description.txt
```

`--stdin` reads UTF-8 to EOF and preserves whitespace, line endings and literal
shell-looking text. Invalid UTF-8 and blank-only descriptions fail. It conflicts
with positional text, `--description` and `--edit`; parent, prerequisite, priority and image
options keep their usual direct-add behavior. Existing add/editor modes stay
available without `--stdin`.

## Atomic batch import

```sh
qqq import plan.json --dry-run --json
qqq import plan.json
cat plan.json | qqq import - --json
```

Version-1 batch schema:

```json
{
  "version": 1,
  "tasks": [
    {
      "key": "tests",
      "description": "Run regression checks\nInclude Unicode: λ",
      "priority": 5,
      "parent": {"key": "feature"}
    },
    {
      "key": "feature",
      "description": "Build feature",
      "priority": 10
    }
  ]
}
```

Each task requires a nonblank unique `key` and nonblank `description`. Keys and
descriptions retain exact whitespace; references match exact keys. Priority
defaults to 0 and accepts integer -100 through 100. Omitted/null parent means
no tree parent. Omitted `tags` defaults to `[]`; supplied tag labels use the same normalization
and validation as add. Import preview/results include normalized tags.
Omitted `depends_on` defaults to `[]`. Each prerequisite uses
the same reference shape as parent; for example `"depends_on": [{"key":"api"},
{"id":12}]`. `{"key":"feature"}` references a task in the same batch,
including one listed later. `{"id":12}` references an existing positive DB task
ID. Reference objects require exactly one reference type. Existing archived
unfinished parents/prerequisites are rejected; archived completed references are allowed, as
with ordinary add.

Missing refs, cycles across parent and prerequisite links, duplicate prerequisites,
parent/prerequisite overlap, duplicate/blank keys, invalid priorities/descriptions,
unknown or duplicate fields, malformed JSON and unsupported versions fail.
An empty `tasks` array is valid and imports nothing. Version 1 has no image
attachment fields; use single-task add's image options for attachments.

Import validates the whole batch before insertion, then commits all tasks in
one transaction. Failed validation or writes leave no partial imported tasks,
messages, events, attachments or consumed IDs. Existing project schema migration
and pending deletion recovery still follow normal command behavior before the
import transaction. Imported tasks start new and unarchived, with ordinary
timestamps/content revisions and no assignment metadata.

Parents and batch-local prerequisites are inserted before their dependents. At each step, the lowest original
input index among available tasks is chosen. This keeps order deterministic for
forward refs and independent tasks. Other writers cannot interleave ID allocation
inside a batch or change an existing dependency's state before its commit.

`--dry-run` validates schema, local links and existing dependencies through a read-only
DB snapshot. It does not create a project, migrate schema, run deletion recovery,
write tasks or allocate/reserve IDs. Older supported DB schemas need a normal
command such as `qqq list` first. Parent state can change after preview; real
import validates it again within its write transaction.

Human output summarizes imported/validated count and keys with IDs or preview
details. JSON returns:

| Field | Meaning |
| --- | --- |
| `version` | Result schema version, currently 1 |
| `dry_run` | Whether this is a validation preview |
| `count` | Number of tasks in the batch |
| `mapping` | Key to committed task ID, in lexicographic key order; empty object for dry-run |
| `creation_order` | Keys in deterministic insertion order |
| `tasks` | Normalized entries in original input order |

Each result entry has `key`, `description`, defaulted `priority`, original
`parent` reference (or null), `depends_on` references (default `[]`), `id`,
resolved `parent_id` and `prerequisite_ids` in source reference order. Dry-run task IDs
are null; existing dependency IDs are known, batch-local dependency IDs remain null. After
commit, allocated task IDs and every resolved dependency ID are returned. For the
example above in an empty project, mapping is `{"feature":1,"tests":2}` and
creation order is `["feature","tests"]`.

## Task recipes

Store reusable workflows in `<project-root>/.qqq-recipes/NAME.json`. Recipe files
are ordinary project files suitable for Git, separate from ignored `.qqq/`
runtime state. Discovery uses nearest `.qqq` project, including from subdirs;
it never searches global folders, uses cwd shadow files or falls back to an outer
project's recipes. Project must already be initialized. Names contain ASCII
letters/digits/underscores/hyphens; pass name without `.json` or path separators.

Copy bundled [bug](../examples/recipes/bug.json) and
[release](../examples/recipes/release.json) examples from qqq source checkout into
your project's `.qqq-recipes/`, or create files using format below.

<!-- recipe-cli-examples -->
```sh
qqq add --template bug --var component=auth --dry-run --json
qqq add --template bug --var component=auth --json
qqq add --template release --var version=0.5.1 --dry-run --json
qqq add --template release --var version=0.5.1 --json
```
<!-- /recipe-cli-examples -->

Each application creates new tasks. Bug example creates one tagged task with
whole description and acceptance criteria. Release example creates `prepare`,
`tests`, `publish`: tests has prepare as parent; publish has prepare as parent
and tests as extra prerequisite. Recipes create queue records; applying release
recipe does not execute tests or publish software.

Version-1 recipe format:

```json
{
  "version": 1,
  "parameters": [
    {"name": "component"},
    {"name": "severity", "default": "normal"}
  ],
  "tasks": [
    {
      "key": "bug",
      "description": "Fix ${component} bug\n\nAcceptance criteria\n- Add regression test\nSeverity: ${severity}",
      "tags": ["bug", "${component}"],
      "priority": 5
    }
  ]
}
```

`version` and `tasks` are required. `parameters` defaults to `[]`. Parameter
names follow ASCII identifier syntax: letter/underscore first, then
letters/digits/underscores. Omitted `default` makes parameter required; supplied
default must be string. Names are case-sensitive. Every required declaration
must receive value, including declarations unused by task text.

Task fields, defaults, parent/prerequisite reference forms, tag validation and
ordering use [atomic import schema](#atomic-batch-import). Acceptance criteria
belong in whole description. Local refs such as `{"key":"prepare"}` resolve to
new IDs from same application; explicitly declared existing refs such as
`"depends_on":[{"id":12}]` retain ID12 and must exist in target project. No image
attachment fields in version1. Empty task arrays are valid no-ops.

Repeat `--var NAME=VALUE`; split occurs at first equals sign. Quote assignments
when values contain spaces or shell metacharacters, for example
`--var 'component=auth = account login'`. Values preserve whitespace, additional
equals signs, quotes, Unicode and newlines. Empty value counts as provided;
expanded descriptions/tags still must pass ordinary task validation.

`${name}` substitutes only descriptions and individual tags. Keys, integer
priorities and reference objects remain literal. Values/defaults are inserted
once without further expansion: value containing `${other}` stays literal text.
`$$` emits `$`; `$${name}` emits literal `${name}`. Other dollar forms such as
`$HOME` or `$(...)` stay literal. Loading recipes never reads environment values,
executes shell commands or evaluates code. Unknown/malformed/unclosed placeholders
fail. JSON itself must use normal JSON string escaping (`\n`, `\"`, `\\`).

Required/duplicate/unknown assignments, duplicate declarations/JSON fields/task
keys, invalid fields/versions, missing refs and graph cycles fail before any task
creation. Expansion happens on parsed strings before shared import validation;
it cannot inject JSON fields. Recipe fields own task data, so `--template`
conflicts with positional text, `--description`, `--stdin`, `--edit`, `--parent`,
`--depends-on`, explicit `--priority`, `--image` and `--tag`. Add's `--var` and
`--dry-run` require `--template`; ordinary add/import behavior stays unchanged.

`--dry-run` validates expanded descriptions, normalized tags and graph against
current read-only DB snapshot. No new tasks, claims, IDs, migration or deletion
recovery. Command-start owner preflight remains independent and may fail a
confirmed dead owner, as with other inspection commands. Real application
rechecks existing references and inserts whole graph in one import transaction;
validation/insertion failures leave batch absent and consume no IDs. Concurrent
applications retain separate complete mappings.

JSON uses import report fields `version`, `dry_run`, `count`, `mapping`,
`creation_order`, `tasks`. Committed mapping is recipe key -> new task ID.
Preview mapping is `{}`; new IDs remain null while original reference objects
and creation_order show graph without reserving IDs. Expanded descriptions/tags
are exact JSON data. Human preview adds full descriptions/tags to graph summary;
terminal controls are escaped. Native agent callers retain automatic JSON,
ordinary shells human output, with existing `--json`/`--human` overrides.

## Archive and unarchive

```sh
qqq archive 12
qqq list --include-archived
qqq show 12
qqq unarchive 12
```

Archive keeps task status, description, priority, messages, images, ownership
history, and dependencies. `show`, `edit`, messages, and image export still work
by ID. Archive/unarchive add history events; repeated commands change nothing.
Events record supplied `--session` or `cli` when absent. In-progress tasks
cannot be archived. An unfinished parent or prerequisite cannot be archived
while a visible unfinished task depends on it. Adding links or unarchiving
unfinished dependents under archived unfinished dependencies also fails.
Archived completed dependencies still release tasks.

## Bulk task actions

`qqq bulk` previews metadata changes. Review exact IDs and before/after values,
save JSON, then explicitly confirm with `--apply PATH`. Commands below run from
project root or subdirectories:

<!-- bulk-cli-examples -->
```sh
qqq bulk --tag backend --status new --add-tag review --priority 5 --json > preview.json
qqq bulk --apply preview.json --json
qqq bulk --id 1 --id 2 --set-tags 'release,verified' --json > preview.json
qqq bulk --apply preview.json --json
qqq bulk --all --archive --json > preview.json
qqq bulk --apply preview.json --json
```
<!-- /bulk-cli-examples -->

Use IDs returned by `add`; example IDs `1` and `2` must exist. Human output shows
every selected ID and its values. `qqq bulk --apply preview.json --human` prints
readable results. `qqq bulk --apply - --json` reads a saved preview from stdin.
Generating a preview alone changes no selected task; discarding preview cancels.

Selection uses either repeated positive absolute `--id ID`, or selectors:

| Selector | Behavior |
| --- | --- |
| `--query TEXT` | Case-insensitive substring in full saved description |
| `--status STATUS` | Repeat for OR; `new`, `in_progress`, `completed`, `error` |
| `--filter EXPR` | Existing typed Luau filter |
| `--tag LABEL` | Repeat to require every exact normalized label |
| `--all` | Explicitly select all visible tasks without completion display limit |
| `--include-archived` | Include archived tasks with selectors |

Query, status, tag and Luau selectors combine with AND. Only directly matching
rows enter batch; ancestor context rows stay out. Completed display limits never
apply. Explicit IDs include archived tasks, deduplicate and sort ascending; they
conflict with selectors. Archive visibility alone is not a selection. Empty
selection produces zero counts, empty IDs and empty task rows.

| Action | Behavior |
| --- | --- |
| `--add-tag LABEL` | Add label; repeat |
| `--remove-tag LABEL` | Remove label; repeat |
| `--set-tags LABELS` | Replace with comma-separated labels; `""` clears |
| `--priority N` | Set integer priority from -100 through 100 |
| `--archive` / `--unarchive` | Set archive visibility |

At least one action is required. Tags use single-task validation. Replacing tags
conflicts with adding/removing; same normalized label cannot be added and removed.
Archive/unarchive conflict. Priority, tags and archive visibility can combine.
Deletion, status changes and ownership use individual task commands.

Preview JSON has `version: 1`, `applied: false`, canonical `database` path,
normalized `actions`, `selected_ids`, `selected_count`, `changed_count`,
`action_count` and `tasks`. Each task row has `id`, `before`, `after` and opaque
SHA256 `fingerprint`; metadata contains `tags`, `priority`, `archived`.
`changed_count` counts tasks with changed values; `action_count` counts changed
metadata fields across tasks. Adding several tags to one task counts as one
field change. No-op rows contribute zero; empty/no-op application writes no task
rows or history. Apply returns same report with `applied: true`.

Apply uses only frozen IDs, never reruns selectors. Newly matching tasks stay
unchanged. Saved preview belongs to its canonical project DB path. Relevant
selected-task changes, removal, assignment changes or content/attachment revisions
cause `BULK_CONFLICT`; no task in batch changes. Messages alone do not invalidate
preview. Regenerate and review after conflict. Malformed/inconsistent previews
fail with `INVALID_ARGUMENT`; applied reports cannot be applied again.

All rows validate before one transaction commits. DB failures roll back every
change and archive event. Archive guards use final batch state: active tasks
cannot be archived; unfinished dependencies cannot be archived while visible
unfinished tasks depend on them. Archiving/unarchiving parent and dependent
together works when final graph satisfies guards. Other task fields, claims,
descriptions, images, dependencies and existing history stay intact. Actual
archive changes add normal archive/unarchive events. Standard synchronous owner
liveness preflight still runs before command, as described below.

## Reopen work

```sh
qqq reopen 12
qqq reopen -1 --session reviewer
```

`reopen` returns a completed task to `new` and records a reopen event. An
in-progress task can also reopen when its owning Herdr agent is absent from a
successful lookup on its saved server. Exact session identity or same terminal
with same agent kind counts as live, including moved panes and changed session
reports. Missing links or saved servers, mismatched claim associations, failed commands and
invalid responses cannot establish absence; task stays unchanged. Legacy links
remain usable because fresh claims replace saved links. New links privately
record current claim association. CLI and lowercase `y` in TUI Reopen use same checks.

TUI Reopen confirmation offers `y` to reopen normally or uppercase `Y` to force
reopen a completed, in-progress or error task. Force reopen skips owner-liveness
checks and permits manual errors; it clears the task's recorded assignment even
when an owner remains live. Archive and dependency availability checks still
apply, and a new task cannot reopen. `n`, Enter or Esc cancels. Confirmation hides
the cursor and ignores editing/navigation input; Ctrl-C leaves it open.
Cancellation preserves the active filter, dirty draft and caret. Successful
reopening records the actor in a reopen event.

Reopening keeps
description, priority, parent, prerequisites, messages, images, creation time, and prior
history. Reopening clears recorded assignment metadata; saved Herdr link stays
until next claim replaces it. Preflight-generated errors also reopen; ordinary
manual errors use retry. New tasks fail without changes; repeating
`reopen` also fails. Archived tasks require `unarchive` first. A
task with an archived unfinished parent or prerequisite cannot reopen until
dependency is unarchived or completed, or its link removed. Completed descendants
stay completed; new dependents wait for reopened prerequisite to complete again. Reopened tasks return to default
lists, including `--max-completed 0`, and become claimable when dependencies
permit.

## Built-in editor

On a terminal, `add` without text or `edit` without update flags opens built-in
editor. Ctrl-S saves, Esc exits blank draft or confirms discard of nonempty
draft, Ctrl-C exits, Ctrl-W deletes previous word on same line, Ctrl-V pastes
clipboard text or image. Option+Left/Right moves cursor by word. Clipboard paste
needs desktop clipboard support.

Ctrl-Z undoes description edits; Ctrl-Y redoes them. Each text input, deletion,
Ctrl-W word deletion or paste is one step, including image/pasteboard items.
Undo restores content and caret; cursor movement and failed edits keep redo.
Editing after undo clears redo. History holds up to 256 changes and 64 MiB of
removed payloads, retaining one oversized change when necessary. Undo/redo
changes only local description; Ctrl-S commits it. Filter and popup inputs have
no description undo/redo.

Built-in editors collapse pastes over 1,000 characters into one gold
`[Pasted Content N chars]` item. Saving stores full payload in Markdown:

````markdown
```pasteboard
pasted text
```
````

Shift-Up/Down and `qqq edit` restore complete fences as one editable item;
Backspace or Delete removes entire item. Payload containing backticks gets a
longer fence. `NO_COLOR=1` and `TERM=dumb` keep label plain. Existing task text
outside complete `pasteboard` fences stays visible and editable character by
character, regardless of length. Other code fences remain ordinary text. Loading
or saving unchanged text preserves original bytes.

In built-in `qqq add`, Ctrl-S creates or updates task, clears editor, then waits
for next task. Saves commit immediately; exit keeps prior saves. Shift+Up loads
newest task, then older tasks; Shift+Down moves toward newer tasks, then blank
draft. Loaded task places cursor at description end. Header shows selected ID
and status. JSON output returns saved tasks as array on exit. Exit without any
save returns error.

Switching from changed draft asks before discard; N, Enter, or Esc keeps it.
Navigation includes all unarchived tasks, regardless of status; deleted IDs are
skipped. `--parent`, `--depends-on` and `--priority` apply to each new task. `--image` files attach
only to first successful save, including when updating existing task. Inline add,
external editor and `qqq edit` save once.

Use `--edit` (`-e`) to force `$EDITOR`. Without terminal input and stderr,
interactive add/edit uses `$EDITOR` automatically. Editor command must wait
until editing finishes.

```sh
export EDITOR='vim'
qqq add --edit
qqq edit 1 --edit --description "Prefilled draft"
```

Built-in add/edit editor uses stderr; stdout holds result on exit. Cancelling
draft keeps earlier saves. Direct edits preserve omitted fields, ownership and
attachments.

External edits automatically guard the revision loaded before `$EDITOR` opens.
On conflict, qqq shows paths to complete local text and current DB text, plus
current revision. Pending image bytes are copied beside local draft; an
`attachments.json` file lists their names and recovery paths. Newer DB content
and stored images stay intact.

With terminal input, `r` reloads DB text after discard confirmation, `o` saves
local text after overwrite confirmation, and `k` keeps recovery files and exits
with failure. Reload discards pending local images. Overwrite checks the displayed
revision again; another concurrent edit causes another conflict. Without terminal
input, qqq exits with failure and keeps recovery files. Review local/current
files, then retry with `--description` and `--expected-revision` for the current
revision; use `--image` with recovered attachment paths if needed. A removed
task keeps local draft and is never recreated by save.

Built-in edits and TUI saves guard loaded content revision automatically. A
conflict view opens instead of saving. `Tab` switches between complete current
DB text and local draft; arrows, Page-Up/Down and Home/End scroll text. Line breaks
and whitespace remain visible. `Esc` or Ctrl-C closes view while keeping draft,
cursor, collapsed paste blocks and pending image bytes.

`r` offers reload, then `y` confirms discarding local draft; `o` offers overwrite,
then `y` confirms saving local draft against displayed DB revision. `n`, Enter
or Esc cancels confirmation. Further content changes reject overwrite again.
Reload reads latest complete content. A removed task offers local text and keep
only. Retained dirty buffers keep their original loaded revision when switching
tasks; saving or reloading one leaves other drafts intact. Message/status updates
do not cause content conflicts.

## Task TUI

```sh
qqq tui
```

Below 60 columns, compact layout hides task status labels and details/messages.
Task list and editor each use half terminal height, rounded to rows.
From 60 through 149 columns, task list, details/messages and editor stay visible at 35%,
20% and 45% of terminal height, rounded to rows. At 150 columns or wider, list
and details share upper rows: list uses 60% of width, details 40%. Editor spans
full width below them, retaining its height and position between stacked and wide layouts.
Task list shows unarchived tasks by default.
Visible details pane shows `Select task to view details.` for blank drafts; selecting or
saving task keeps pane positions fixed. Small terminals retain usable list and
editor rows.
Details use a double-line box with two spaces inside each side; short panes
retain padding without borders. Text wraps to the inner width.
Details share `show` header and layout: `#ID · Status`, Description, Messages,
Details, Assignment, Images, History and Herdr sections. Fields align their
values; Created/Updated occupy separate rows. Assignment lists Harness and
Orchestrator Session; session values include stored names such as `default (herdr)`.
Messages follow creation order,
with indented headers and bodies; empty collections show `None`. IDs are bold,
statuses colored, labels/timestamps dim. Styles survive wrapping and scroll.
Each task preview shows at most three wrapped lines; clipped previews end with
`...`. Editor loads full description, including hidden lines.
Selection fills whole row with muted blue background, including wrapped lines;
light text stays readable. Task rows have one leading space. Editor header shows
selected task ID in color and plain modes.
Shift-Up/Down selects tasks in displayed tree order, then new-task draft.
Switching retains unsaved drafts in memory for this TUI session. Returning restores
text, image/paste items, caret and manual editor scroll. Dirty tasks show gold
`[*]` only before the first description preview line (text marker in plain mode).
ID/status columns, wrapping and continuation rows stay fixed when marker toggles;
first-line preview clips at pane edge. Dirty new/child drafts show `[*]` in title.
Reverting edits clears marker. Task list/filter still use saved DB descriptions.
Ctrl-S saves only selected draft; failed save keeps draft and marker. By default, saved task stays open for
further edits or task actions. Freshly loaded and saved tasks place cursor at description
end; editor scrolls to keep cursor visible. Set `tui.after_save_new` to `open_new`
to clear editor after creating task; existing-task edits stay open. Shift-Down past last
displayed task restores general new draft, or opens blank draft when none exists. Esc closes focused filter and clears query,
returning focus to editor while preserving draft. From editor, Esc clears editor
before exiting on next press. Selected tasks and dirty new drafts ask before
discarding unsaved content; `y` clears editor, `n` keeps draft for editing or Ctrl-S save.
Ctrl-C returns selected task to blank draft, asking before discarding edits.
Ctrl-C immediately clears a nonempty new or child draft, including text, tags,
images, paste items and undo history; child parent stays selected. Next Ctrl-C
exits only once active new draft is empty. Ctrl-C or Esc exit prompts for every retained dirty draft, including hidden,
archived or deleted tasks and child drafts. Prompts identify draft and preview it;
`y` advances, `n`, Enter or Esc cancels whole exit and preserves all drafts,
including earlier approvals. App exits after every draft is approved. Blank draft
exits immediately only when no retained dirty drafts remain.
Cancelling a deleted task's discard prompt reopens its retained local draft;
Ctrl-S explains removal and keeps draft without recreating task.
Repeated Ctrl-C keeps confirmation open. Ctrl-V or terminal paste inserts text; pasting
image file path attaches image. Saves commit immediately. TUI needs terminal
and writes no stdout, including with `--json`.

Undo/redo history survives Ctrl-S while same task stays open, including saved
image references without another upload. Retained unsaved drafts carry their own
history across navigation. Failed saves and cancelled conflicts keep history;
accepted reload, discard, fresh task load or clearing to new draft starts fresh.
Navigating away from a clean saved draft uses existing fresh-load behavior.

Ctrl+P opens or restores child draft under selected task; header shows parent ID.
Current edits stay retained. Each parent has separate child draft; return to parent
and press Ctrl+P to continue it. Ctrl-S creates child with dependency;
Enter or Shift+Enter inserts newline. Navigating away retains child parent context.
Inside child draft editor, Ctrl+P inserts its parent's plain `#ID` at caret,
without added spaces. One Ctrl+Z undoes insertion; cursor movement and deletion
use normal text characters. Save stores exact text; references add no dependency
or special display behavior. Filter input and popups leave draft unchanged.

Ctrl+B opens saved project views from editor or filter. Arrows select, Enter
applies, Esc/Ctrl+C cancels; PgUp/PgDn scrolls criteria. `tui --view NAME` opens
the same definition as CLI lists/workers. Switching retains opened task,
dirty/parked drafts, staged tags, caret/scroll and live query/focus, including when
the task disappears from the view. Picker reloads `.qqq-views.json` when opened;
active definition stays fixed until another selection. See [named views](views.md).

Ctrl+O opens selected saved task's dependency graph from editor or filter.
Up/Down/PgUp/PgDn scroll, Home/End jump, `u`/`d`/`b` select direction, `+`/`-`
adjust depth. Esc/Ctrl+C closes without changing drafts, tags, caret/scroll,
saved view or query/focus. DB commits refresh cached report. See [graph](graph.md).

Ctrl+L opens tags for the selected task or new draft from editor or filter. Current
labels are prefilled, one per line. Enter or Shift+Enter adds a line; Ctrl-S applies.
Up/Down move caret between tag rows, preserving preferred display column across
short rows. Left/Right move between characters; Home/End jump within current row.
Ctrl-A/Ctrl-E move to line start/end; Alt-Left/Alt-Right move by word. Ctrl-W deletes
the previous word without crossing a line. Typing, terminal paste, Ctrl-V text
paste, Backspace/Delete and Enter edit at caret. Active row and
caret stay visible when tags exceed popup height or width.
Blank lines are ignored; blank input clears tags. Newline/CRLF paste and legacy
comma-separated input are accepted. Ctrl-U deletes the current line while keeping
other labels; delete each line to clear all tags. Esc/Ctrl-C cancels.
Shortcuts share the popup footer; narrow popups use `^S` for Ctrl-S, `^U` for
Ctrl-U and `↵` for Enter. Active input row stays visible in short terminals.
List tags use yellow when color is enabled. Invalid labels stay in the popup. Tag saves
retain unsaved description drafts, caret, scroll, parked buffers and filter
focus. On a new general or child draft, applying tags only changes the draft;
no task enters the DB or queue until Ctrl-S creates it with description, parent,
images and tags in one transaction. Draft tags survive navigation independently
for each buffer. A draft containing only tags is unsaved work and requires discard
confirmation; Ctrl-S still requires a nonempty description. Fresh drafts start
without tags after a save.

Ctrl+K opens a task ID popup from editor or filter. Enter a positive ID, press
Enter to go; invalid or missing IDs keep popup open with an error. Backspace edits
input; Ctrl+U clears it. Esc or Ctrl+C closes popup and preserves selection,
draft, filter and focus. Successful jump clears filter and retains unsaved
task/new/child drafts, including caret and editor scroll. Jumping to archived
task enables archived rows for remaining dashboard session. Jumping to completed
task reveals completed rows when hidden by the completion toggle or zero limit;
other jumps and cancellation preserve that setting. Saved selectors and positive
completed limits remain in effect: jump opens a task even if its list row stays
hidden. Navigation saves no content and changes no task status or ownership.

Ctrl+D toggles current saved task in bulk selection. Selected tasks show `+` in
existing one-cell row prefix; dirty `[*]` and focused row remain independent.
Marks survive filtering and refresh, including hidden tasks. Ctrl+G opens bulk
menu while any task is marked; menu and preview show count and exact IDs.
`x Clear selection` removes marks. With no marks, Ctrl+G opens individual actions.

Bulk menu offers add/remove/replace tags, priority and archive/unarchive. Tag
input uses one label per line: Enter/Shift+Enter adds newline, Ctrl+U deletes
current line, Ctrl+S generates preview. Empty replacement clears tags. Priority
input previews on Enter. Invalid input retains text for correction.
Preview shows full before/after metadata; Up/Down, PgUp/PgDn and Home/End scroll.
Heading and `y apply` / `n/Esc cancel` stay visible across resize. Enter or Ctrl+C
also cancels. Only `y` applies frozen preview. Success clears marks; conflicts
retain marks for fresh preview. Cancelling or applying preserves unsaved drafts,
paste/image items, caret, manual pane scroll, filter and focus. Bulk actions
operate on saved tasks; Ctrl+S in editor saves description separately.

Ctrl-H focuses selected task's live Herdr agent and opens Herdr client using
task's linked server. `Ctrl-H Herdr` appears in the shortcut bar only when the
selected task has a saved Herdr link, including while filtering. Detach from client
to return to TUI with draft, selection, filter, cursor and scroll positions preserved. No task is saved or changed.
Missing links, unavailable agents and client errors appear in status bar.
Link task with `qqq herdr link` first if needed. Backspace still edits text.

Task list opens scrolled to bottom. List and details refresh after external DB
commits without keyboard input. Refresh keeps current editor draft, filter query,
and manual scroll positions. Selected task status updates in editor title; task
text stays in editor until reloaded or saved.

Mouse wheel scrolls list, details or editor under pointer. Panes keep separate
scroll positions; scrolling never edits or saves text. PgUp/PgDn scroll selected
task details while editor has focus. Selection changes reset details scroll.
Details clicks leave editor caret and draft unchanged. Shift-Up/Down
returns list to selected task, revealing its full preview when it fits. Automatic
selection follow also keeps all preview rows visible; smaller viewports keep
preview's first row visible. Mouse scrolling retains manual row offsets and can
leave previews partially visible. Editor keys reveal caret after manual scroll.
Left-click task row to load it in editor, including indented or wrapped rows.
Left-click editor text to place caret. Task-row clicks retain unsaved drafts,
using same restoration behavior as Shift-Up/Down.

Ctrl+/ focuses filter and shows `Filter:` with editable caret, even when empty.
Filter bar stays visible while focused, query is nonempty, completed tasks are
hidden, or archived tasks are included. Empty unfocused query reserves no row
when completed tasks are shown and archived tasks are hidden.
Right-edge `[✓ Completed] [× Archived]` buttons control visibility independently.
Click Completed or press Ctrl-T while filter is focused to hide/show completed
tasks. Archived defaults off; click Archived or press Ctrl-A while filter is
focused to include/hide archived tasks. `--include-archived` and saved views can
start with Archived enabled. Narrow list panes show `[✓C] [×A]` with the same
click targets. Visibility resets to startup settings when TUI opens again.
Toggle preserves query, opened task, unsaved drafts and DB; navigation
uses visible tasks. Unfinished children remain visible when completed parents are
hidden. Type to match any part of full description, ignoring case.
Matching tasks retain visible parent chain;
no matches shows `No matching tasks.`. Backspace edits query. Tab or Enter returns to
editor. `/` or Alt+/ inserts literal `/` into editor. Shift-Up/Down navigates visible
tasks while filter is active. Query changes do not change selected task, unsaved
draft, or DB.
With focused filter or nonempty query, Esc clears query and closes filter focus. Ctrl-C clears
nonempty query, preserving current focus. Selected task and editor draft stay intact, including while
popup or confirmation is open. With empty focused filter, next Ctrl-C closes filter
and returns focus to editor without changing opened task or draft. Next Esc follows
editor clearing flow; later Ctrl-C follows editor cancel flow. Esc asks before
discarding dirty drafts; Ctrl-C asks for saved-task edits, clears a nonempty new
draft immediately, and exits from an empty new draft after retained-draft approvals.

Ctrl-G opens centered actions popup for selected task; dashboard stays visible
around it. Prompts, confirmations and errors share popup; small terminals use
compact view, scrolling menu to keep selected action visible.
Actions form three groups separated by empty rows: status (`Complete`,
`Mark error`, optional `Retry error`, `Reopen`), settings (`Priority`,
`Set parent`), and visibility (`Archive`/`Unarchive`). Empty rows are never
selectable. Cyan titles/shortcuts, muted help, gold prompts/warnings and soft-red
errors distinguish popup states. Up/Down selects action, wrapping at ends;
Enter activates selected row. Dark accent tint and `>` mark selection; plain
mode keeps marker. Letter shortcuts activate actions directly. Press `c` to complete owned task,
`r` to retry error task, `o` to open Reopen confirmation (`y` normal, `Y` force), `a` to archive or
unarchive, `p` to set priority (-100..100), or `d` to set parent by positive
task ID or `none`. Retry appears only for error tasks; non-error tasks ignore `r`.
Press `e` for Mark error and enter a nonempty reason. Lowercase `y` keeps current
owner checks: only an in-progress task owned by the current session can fail.
Uppercase `Y` forces a New, In progress or Completed task to Error without owner
or Herdr lookup; already-Error tasks reject either action. Archive state, saved
content, dependency edges and Herdr links stay intact. Reason is recorded as a task message
with an error event; claim and assignment clear while saved content stays intact.
Error tasks stay out of worker queue until explicitly retried. Reason prompt and
confirmation cancel without changing task. Confirmation previews reason and asks
before discarding an active dirty draft, including at minimum terminal size.
Reason input keeps its caret; confirmation hides it and ignores editing,
navigation and Ctrl-C. `n`, Enter or Esc cancels, preserving filter and dirty caret.
Arrows skip hidden actions; Enter activates displayed selection.
Open menu refreshes Retry availability after external status changes, keeping
selected action when available.
Complete checks current session ownership when activated. Without a matching
claim, popup asks `Force complete task #ID?` and warns
`Complete without matching task owner.` In every completion popup, `y` confirms
normal completion with current ownership checks; uppercase `Y` explicitly confirms
force completion of a New, In progress or Error task regardless of owner. Popup
has no checkbox or caret; Space, arrows and mouse clicks do not select a mode.
`n`, Enter or Esc cancel. Matching owners can also force explicitly with `Y`.
This clears active claim, retains recorded harness/orchestrator metadata and Herdr
link for log lookup, preserves task content and history, and records a standard `complete`
event attributed to explicit session or `manual`. Already-completed tasks fail.
Cancellation preserves draft and DB. If Herdr fails or ownership changes during
normal confirmation, popup keeps exact error plus both confirmation keys.
Retry normally with `y`, or force explicitly with `Y`. Force path bypasses
Herdr/owner lookup; it never happens automatically after failure. DB corruption,
invalid input and other unrelated failures keep ordinary error handling.
Reopen and Mark error confirmations preserve filters on Esc. Other menus/prompts
close with Esc after the active filter clears. State changes
require confirmation before running; priority and parent changes ask when draft has unsaved edits. Successful
action refreshes task and list; rejected action keeps draft and shows DB error.
Action error popup hides caret and ignores Up/Down. `j`/`k` scroll long messages;
Esc or Enter closes popup and restores editor caret.

## Dependencies and images

Each task has at most one tree parent and any number of extra prerequisites.
A new, unarchived task becomes ready only when its parent (if set) and every
extra prerequisite are completed. `next`, filtered/waiting next, dry-run,
Herdr dispatch and queue diagnostics use the same readiness rule. Extra edges
do not change tree placement or duplicate task rows.

```sh
qqq add "Integrate" --parent 1 --depends-on 2 --depends-on 3
qqq edit 4 --depends-on 5                 # add prerequisite
qqq edit 4 --remove-depends-on 2          # remove one; repeat flag to remove more
qqq edit 4 --clear-depends-on             # clear extras, retain tree parent
qqq edit 4 --clear-depends-on --depends-on 6  # replace extras atomically
qqq edit 4 --set-parent none              # clear tree parent, retain extras
```

IDs must exist. Self links, duplicate additions (including an existing edge),
missing removals, parent/prerequisite overlap, and cycles across both link types
fail atomically. Add/remove of the same ID and clear/remove together fail.
Clear is idempotent. Edits validate the final graph: remove an extra while
making it the parent, or clear the parent while making it an extra, in one
command. Dependency edit flags skip the editor and conflict with `--edit`.
Direct description/status/priority/parent/image changes can share the operation.

Dependency edits and reopening prerequisites affect future claims; active owners
remain assigned, completed descendants keep their status/history, and content
revisions remain unchanged. Completed archived prerequisites satisfy readiness.
Archived unfinished prerequisites follow parent safety: unfinished tasks cannot
link to them, unarchive/reopen validates them, and an unfinished prerequisite
cannot be archived with visible unfinished dependents. Remove such edges before
unarchiving/reopening, or unarchive and finish the prerequisite first.

`show`, Task JSON and TUI details include ordered `prerequisites` entries:
`{"id":2,"status":"new","archived":false}`. Corrupt missing references have
null status/archive values. Human/TUI details mark unfinished entries with
`blocks claim`. Queue explanations include their IDs, status and archived state.
TUI details refresh when prerequisite status changes, retaining unsaved text.

Attach files through `add` or `edit`; repeat `--image` for multiple files:

```sh
qqq edit 2 --image ./before.png --image ./after.jpg
qqq show 2
qqq show 2 --export-image 1 --output ./exported.png
```

Image IDs come from `show`. Export needs selected task's image ID and new output
path. PNG, JPEG, GIF and WebP signatures supported, up to 20 MiB each; signature
check does not fully validate file. Bytes copy into
`.qqq/images/<task_id>/<image_id>.<ext>`. Built-in editor also accepts pasted
image paths or clipboard images. Pasted images become Markdown links in the
description, such as `![before.png](.qqq/images/2/4.png)`. Reloading a task
shows each linked image as one editable item; deleting that item removes its
description link while keeping the attachment available through `show`.

Description, status, parent, prerequisites, priority and image updates save atomically.

## Queue diagnostics

For task-focused upstream blocker chains and unique downstream impact, use
`qqq graph ID [--direction both|upstream|downstream] [--depth N] --json`.
Graph is strictly read-only and skips owner recovery. See [graph flags,
count semantics and JSON format](graph.md).

```sh
qqq status
qqq status --include-archived --json
qqq next --explain
qqq next --explain --filter 'priority >= 5' --json
qqq next --explain --tag frontend --tag bug
qqq next --explain --session worker-1
```

`status` summarizes ready, blocked, in-progress, error and completed counts,
then shows unfinished tasks with active owners, immediate parent/prerequisite blockers and
latest activity. JSON includes every task in scope, including completed tasks.
Archived tasks are excluded by default; `--include-archived` includes their rows
and counts. Archived tasks never become ready.

`next --explain` reports selection and every scoped task's eligibility without
claiming, dispatching an agent or changing assignment metadata. It distinguishes
an empty queue, blocked/error/owned queues, mixed unavailable work and ready tasks
excluded by filters. Candidates use priority descending, then ID ascending on
ties. Readiness uses exactly the same rules as real next: new, unarchived, with
a completed parent if set and all extra prerequisites completed.

Global explanation needs no session. Native Codex/Herdr identity discovery and
dispatch config are ignored. Supply `--session`, `QQQ_SESSION` or
`--harness-session` to inspect owned-task reuse; add `--harness-name` when a
public harness session is ambiguous. Exact claim keys take precedence over public
session matching. Existing owned tasks return before queue/filter selection, as
with ordinary next. This differs from `--dry-run`, which always previews queued
candidates. Explanation accepts `--local`, conflicts with `--wait` and
`--dry-run`; next's `--include-archived` requires `--explain`.

Diagnostics open the existing DB read-only, without creating a project, migrating
schema or recovering pending deletion files. Older supported schemas require a
normal command such as `qqq list` before diagnostics. Counts, task rows, blockers,
activity, owner lookup and selection share one SQLite transaction snapshot.
Filter results are evaluated once per task row and reused for counts and selection,
including date functions whose clock values can change between statements.
Reports do not reserve work; subsequent claims may see newer state. Activity age
never expires or reassigns ownership. Owners still explicitly complete or release
their claims.

### Diagnostic JSON

Both commands return an object with these fields:

| Field | Meaning |
| --- | --- |
| `include_archived` | Requested archive scope |
| `counts.total` | Number of scoped tasks |
| `counts.new` | Scoped tasks with `new` status |
| `counts.ready` | Unfiltered ready tasks |
| `counts.blocked` | New tasks that are not ready; includes archived new tasks when included |
| `counts.matching_ready` | Ready tasks matching supplied filter; same as ready without a filter |
| `counts.in_progress`, `.error`, `.completed` | Scoped task counts for each status |
| `counts.archived` | Archived subset of total; not an additional status category |
| `state` | `empty`, `ready`, `no_matching_ready`, `blocked`, `error`, `in_progress`, or `no_ready` |
| `tasks` | Scoped diagnostic rows in ascending creation ID order |
| `explanation` | Selection diagnostics for `--explain`; null for status |

`ready + blocked = new`. `blocked`, `error` and `in_progress` states describe
queues containing only that unfinished class; completed history does not hide
those states. Mixed unavailable or completed-only queues use `no_ready`.

Each row contains the normal `task` object plus:

| Field | Meaning |
| --- | --- |
| `ready` | Shared readiness predicate, independent of filter |
| `matches_filter` | Combined Luau/tag selector result, independent of readiness; true without selectors |
| `queue_rank` | One-based rank among all ready tasks by priority/ID; null otherwise |
| `reasons` | Zero or more eligibility/exclusion codes |
| `owner` | Recorded claim key for an active task; null otherwise |
| `blockers` | Unfinished/missing parent and extra prerequisites: ID, status and archived flag; missing refs have null status/flag |
| `latest_activity` | Latest task update, message or event: `at`, `source`, `id`, `session`, `action` |

Reason codes: `archived`, `parent_not_completed`, `prerequisite_not_completed`, `in_progress`, `error`,
`completed`, `filter_excluded`, `tag_excluded`. Explicit tag exclusion may appear
alongside dependency blockers. When tags match but Luau fails, reason remains
`filter_excluded`; predicates inside arbitrary Luau expressions use that reason
too. A ready matching task has no reasons.
Latest activity compares task `updated_at` with message/event `created_at`.
Timestamp ties prefer message, then event, then task; higher IDs break ties
within one source. Task activity has null ID/session/action; messages have null
action; events include their action. Updates cover creation and edits as well as
other task changes. Blocker resolution includes archived dependencies regardless of
output scope.

Explanation contains `owner_input`, `resolved_owner` (both null without explicit
context), `ordering: ["priority_desc", "id_asc"]`, `eligible_ids` (matching ready
IDs in selection order), `outcome` and `selection`. Outcome is
`owned_task_reuse`, `ready_candidate`, `empty_queue`,
`no_matching_ready_candidate`, `blocked_queue`, `error_queue`,
`in_progress_queue` or `no_ready_tasks`. Selection is null or
`{"kind":"owned"|"queued","task":...}`. An archived active owned task can appear
in selection while absent from default scoped rows, matching ordinary next.
Task objects retain the existing task JSON format; raw claim keys are exposed
only in diagnostic row `owner`.

## Agents and recovery

Use stable, unique session ID per worker:

```sh
export QQQ_SESSION='worker-1'
qqq next
qqq message 1 "Running tests"
qqq complete 1
qqq next --wait
```

`--session` overrides `QQQ_SESSION`. Each owner gets one active task; repeated
`next` returns it. Claims never expire. `--wait` blocks until work is ready;
without it, empty queue prints `No task available for pickup.` and exits successfully. Error
tasks and blocked children stay out of queue.

`next --dry-run` previews queued candidate without claiming or dispatching an
agent. Existing claims are skipped; no session or Herdr identity required.
Preview retains stored `new` status and metadata. Combine with `--filter`,
`--wait`, `--local`, and `--json`. Preview does not reserve task; another worker
can claim it afterward.

Preview opens DB read-only, leaves deletion staging untouched. Older schemas
return `DATABASE_ERROR` with reason `migration_required`; run `qqq list` to
upgrade before previewing. `--wait` keeps preview waiting for queued candidate
without reserving it or creating upgrade snapshots. Strict read-only commands
refuse WAL mode or existing WAL sidecars with reason `unsafe_read_only`,
because SQLite read-only opens can create WAL/shm files. Stop project writers,
use `sqlite3 .qqq/qqq.db 'PRAGMA journal_mode=DELETE;'`, then retry. Waiting
preview also rechecks journaling state before each read transaction.

Without a queued candidate, human preview prints `No task available for pickup.`
Use `qqq status` to inspect blocked, active, and error tasks.
JSON preview returns `null` when no candidate is available.

```sh
qqq next --dry-run --json
```

`next --filter 'priority >= 5'` selects matching new candidates using same
priority/readiness rules. Without `--dry-run`, existing owned task still returns
regardless of filter.
`--tag LABEL` uses the same selection rules; repeated labels require all tags.
Changed selectors never release or transfer existing ownership.
Combine with `--wait` or Herdr dispatch; no matching ready candidate yields no claim.

Without `--session` or `QQQ_SESSION`, owner discovery uses:

1. Exact Herdr pane from `HERDR_PANE_ID` or `HERDR_ENV=1` (requires pane ID).
2. Native Codex `CODEX_THREAD_ID`, then `CODEX_SESSION_ID` fallback.
3. Unique Herdr agent at project root (`foreground_cwd` before `cwd`).

Explicit session IDs and native Codex ownership work without Herdr when dispatch is
disabled or `next --local` is used. Exact Herdr context takes precedence.
`next --local` skips dispatch, still resolves owner.

Automatic claims save detected harness/orchestrator fields and Herdr link when
available. Session flags are optional in these contexts:

| Context | Automatic identity | Saved metadata |
| --- | --- | --- |
| Herdr pane, any reported harness | Pane agent-session identity, or terminal ID plus agent kind | Reported harness name/session, Herdr server name, saved pane link |
| Standalone Codex | `CODEX_THREAD_ID`, then `CODEX_SESSION_ID` | `codex`, displayed Codex session; no orchestrator or Herdr link |
| Unique Herdr agent at project root | Matching pane identity | Reported harness name/session, Herdr server name, saved pane link |
| Dispatched child | Inherited `QQQ_SESSION` resolves reserved claim | Child harness and Herdr link saved before prompt |

Outside Herdr, built-in native environment discovery currently covers Codex.
Other harnesses use reported Herdr pane identity. qqq does not guess sessions
from unrelated environment variables. Explicit session flags remain overrides.

For automatic Codex claims, displayed `harness_session` uses `CODEX_SESSION_ID`
when set, including inside Herdr. Without it, resolved owner ID supplies display.
Claim keys and Herdr links keep resolved identity; `--harness-session` overrides
display. Repeated claims retain saved metadata unless explicit assignment flags
override it.

Completion, release and error marking require owner. Use original session token
or uniquely matching displayed `harness_session`; add `--harness-name` when
public sessions overlap. JSON assignment fields: `harness_name`,
`harness_session`, `orchestrator_name`, `orchestrator_session`.

Normal and force completion retain these four recorded fields and saved Herdr
link. Completion response, JSON lists, `show` and TUI details keep worker/session
information available for log inspection; `qqq herdr find ID` still uses saved
link to locate agent. Active claim ends, so same worker can claim next task even
when completed tasks keep same public session. Release, error marking and reopen
clear current assignment metadata; fresh claim records new worker identity.

Human `next` output omits these four assignment fields. JSON and `show` retain
them for ownership inspection.

### Owner liveness before commands

qqq checks current project’s active owners synchronously before command execution
or TUI/watch startup. No background process or periodic scan. Confirmed dead
harness/orchestrator changes task to `error`, clears assignment, preserves task
contents/history/link, records reason message and `qqq-preflight` error event.
Inspection commands (`status`, `doctor`, `next --explain`, `--dry-run`) also run
cleanup first; preview itself does not claim or edit queued work. When no dead
owner is confirmed, preflight makes no DB writes. It never creates or migrates a
project; writable opens scan after migration.

`graph` is the read-only exception: it skips owner recovery and identity probes,
preserving stored owner/status exactly. TUI graph popup keeps existing TUI startup
preflight, then performs read-only graph snapshots.

Native Codex claims bind actual harness ancestor PID, start time, executable and
machine identity privately to current claim. Missing/reused PID or zombie proves
death on same machine; another machine or unavailable process data stays unknown.
Manual session tokens alone do not identify process. Saved Herdr server and agent
identity/terminal establish liveness; stopped/missing saved server or successful
agent list without owner proves death. Failed/malformed/timeout probes stay
unknown unless separate evidence proves stopped server. Probes time out after
two seconds. Claim, link and process guards prevent stale checks from failing
reassigned work.

Use `qqq edit ID --set-status new` to retry errors. `qqq reopen ID` also recovers
auto-failed owners; archive/unarchive preserves that option. TUI actions apply
same rules. Idle TUI/watch/wait does not rescan until another qqq command runs.

### Return or retry work

Return active task owned by `worker-1`:

```sh
qqq edit 1 --set-pending --session worker-1
```

Alternatively, mark active task failed, then retry:

```sh
qqq edit 1 --set-status error --reason "Missing credentials" --session worker-1
qqq edit 1 --set-status new          # retry error task; no session required
```

`--set-pending` equals `--set-status new`. Active tasks require owner; error
tasks can be retried without one. Error marking requires nonblank reason,
clears assignment, records reason as message. Status updates skip editor.

For abandoned active work, inspect task before forcing return:

```sh
qqq show 1
qqq edit 1 --set-status new --force
```

`--force` skips ownership matching and Herdr discovery. It requires literal
`--set-status new`; cannot combine with `--set-pending` or `--edit`. Return/retry
preserves content, dependencies, messages, images and Herdr link.

Session IDs coordinate local agents; they do not authenticate users. Any local
caller can edit task content or append messages regardless of ownership.

## Config and aliases

Config: `~/.config/qqq/config.toml` (under `HOME`).

```sh
qqq config --list
qqq config alias.ls 'list --watch'
qqq config --get alias.ls
qqq config alias.ls                 # same as --get
qqq config display.max-completed 10
qqq config tui.after_save_new open_new
qqq config --unset alias.ls
```

Keys use TOML dotted syntax; quote literal dots: `'alias."with.dot"'`.
Alias values are strings; `herdr.next-to-new-agent` is boolean;
`display.max-completed` is non-negative integer. `tui.after_save_new` accepts
`open_saved` (default) or `open_new`, loaded when `qqq tui` starts. Other values
accept TOML literals, falling back to strings.

Reads do not create files. `--list` shows stored values; `--get` can inspect
invalid settings. Writes validate known settings, preserve comments and unknown
keys. `--json` returns typed values; unset restores default.

Completion limit config affects human lists only; explicit `--max-completed`
affects text and JSON. Missing limit means unlimited.

Example config:

```toml
[display]
max-completed = 10

[tui]
after_save_new = "open_saved"

[alias]
ls = "list"
n = "next"
done = "complete"
bug = "add"
```

`qqq bug "Fix login"` expands to `qqq add "Fix login"`. Aliases support quoting
and chains; built-in commands take precedence. No shell expansion. Cycles,
invalid quoting and `!` aliases fail.

## Herdr

Optional Herdr integration links tasks to live agent panes:

```sh
qqq herdr link 1
qqq herdr find 1
qqq herdr link 1 --agent codex --agent-session session-123 --server work
```

Discovery selects exact caller pane or unique agent at project root. Ambiguous
identity fails before claim. Without agent-session hooks, terminal ID plus agent
kind supplies identity. Active claims retain identity as hooks change.

`link` stores association without changing owner; `find` searches live panes for
saved link identity. Both require Herdr CLI and server. Local explicit `--session`
claims do not auto-link; new-agent dispatch links spawned agent even when caller
uses explicit session.

To dispatch ready work into new Codex agent tab:

```sh
qqq config herdr.next-to-new-agent true
qqq next
```

Dispatch needs `HERDR_ENV=1` and `HERDR_WORKSPACE_ID`. It reuses caller's active
task; otherwise reserves ready task, opens sibling tab, starts agent and submits
prompt. Empty or blocked queues create no tab. `next --local` skips dispatch.
Default: disabled.

Dispatched child inherits `QQQ_SESSION` for reserved claim. Worker commands
`qqq message ID ...`, `qqq complete ID` and `qqq edit ID --set-status error
--reason ...` use that env automatically; generated prompt requires no session
parameter. Keep inherited variable set. Child's saved harness/orchestrator fields
identify spawned agent, separate from dispatch ownership token.

Startup failure releases dispatch reservation; created tabs stay open. Prompt
errors keep claim and link because delivery may have happened. Inspect agent
before retry or forced release.

## Data

Compatibility facts below describe current source checkout. Fixture matrix and
runtime output enforce this table; package version and DB schema are separate.

<!-- compatibility-support:start -->
| Contract | Supported value |
| --- | --- |
| Current DB schema | `13` |
| Normal-open DB schemas | `1–13` |
| Automatic upgrade source schemas | `1–12` |
| Portable snapshot format | `1` |
| Portable snapshot DB schemas | `9–13` |
| Upgrade recovery format | `2` |
| Upgrade recovery source schemas | `1–12` |
| Initialization-only schema | `0` |
| Manual-conversion layouts | `title`, `pending` |
<!-- compatibility-support:end -->

Supported versions require compatible `description`/`new` layout and valid data.
Legacy `title` columns or `pending` status layouts need manual conversion even
when their `user_version` falls inside listed range. Schema 0 initializes an
empty DB; existing schema-0 projects do not qualify for automatic upgrade.
Unknown newer schemas and archive formats are rejected.

Keep `.qqq/` and `.qqq-upgrades/` out of Git. Create portable snapshot while other local qqq
writers run:

<!-- compatibility-portable-example:start -->
```sh
qqq backup ../project-snapshot.tar
mkdir ../restored-project
cd ../restored-project
qqq restore ../project-snapshot.tar
qqq list
qqq doctor
```
<!-- compatibility-portable-example:end -->

Backup contains consistent SQLite data plus every stored image. Tar archive
starts with `manifest.json`, then `qqq.db`, then `images/<task-id>/<image-id>.<ext>`.
Manifest records SHA-256 hashes and sizes; restore checks those, image rows,
SQLite integrity, and foreign keys before installing `.qqq`. Backup refuses to
overwrite destination. Restore works only from new project directory or one
with empty `.qqq`; existing data and nested projects are rejected. Invalid
archives leave project unchanged. Relative snapshot paths resolve from current
directory.

Before automatically upgrading an existing supported DB, qqq saves original
schema and required attachments in sibling `.qqq-upgrades/` directory. Human
stderr prints `Saved pre-upgrade snapshot: <path>` before migration. JSON
result/error shapes stay unchanged; discover archives in that directory.
Names follow `schema-<source>-to-<target>-.upgrade-<unique>.tar`; retries never
overwrite previous records. Manifest records source/target schemas, original
absolute project path, creation time and SHA-256 hashes/byte counts. SQLite
write lock coordinates DB copy and required attachments before migration;
waiting openers recheck schema after acquiring lock. One successful concurrent
upgrade produces one archive. Snapshot is verified, synced and atomically
published before migration changes DB/images. Snapshot failure aborts upgrade
with source intact. Successful
and failed migrations both retain verified snapshots. Current-schema opens,
fresh initialization, diagnostics and dry runs create none.

Upgrade recovery uses distinct manifest format listed in support table. Stop qqq workers, choose
original archive from printed absolute path or `.qqq-upgrades/` inventory.
Example starts in original project, prompts for that path, creates fresh target.
SQLite inspection requires `sqlite3` CLI:

<!-- compatibility-recovery-example:start -->
```sh
printf 'Saved snapshot absolute path: '
IFS= read -r recovery_archive
test -f "$recovery_archive"
recovery_project=$(mktemp -d "${TMPDIR:-/tmp}/qqq-recovered.XXXXXX")
cd "$recovery_project"
printf 'Recovered project: %s\n' "$PWD"
qqq restore --recovery "$recovery_archive"
# Recovery preserves original schema. Inspect data before retrying upgrade.
sqlite3 .qqq/qqq.db 'PRAGMA user_version; PRAGMA integrity_check; PRAGMA foreign_key_check;'
qqq list # saves new pre-upgrade snapshot, retries normal upgrade
qqq doctor
```
<!-- compatibility-recovery-example:end -->

Recovery validates manifest schema/project metadata, SHA256 hashes, SQLite
integrity/foreign keys, task status/claims and attachments before installation.
It accepts original source schemas listed in support table, including
embedded-image schemas 1–5, without
migrating during restore. Pending deletion attachments are captured without
altering live staging, then restored at canonical image paths. Populated target
is rejected. Ordinary `restore` accepts portable format/schema range listed above;
`--recovery` is required for automatic upgrade archives. Retain archives until
recovered data and subsequent upgrade are verified; no automatic cleanup runs.
Use SQLite inspection before `qqq list`, `qqq show`, backup, TUI or another
normal DB-backed command: those commands automatically upgrade restored old DB.
Recovery does not overwrite original project; inspect and verify fresh target
before replacing any live project files.

Preview permanent deletion before confirming it:

```sh
qqq backup ../before-delete.tar
qqq delete 42
qqq delete 42 --yes
```

Only archived tasks without children can be deleted. Use positive task ID;
relative creation indexes can change before confirmation. Preview lists messages,
events, Herdr link, stored images, and paths without changing project data;
`--json` returns same counts with `deleted:false`. `--yes` removes task and
dependent data permanently, including stored image files, then returns
`deleted:true`. Release or complete an active task before archiving it; delete
or reparent every child and remove all prerequisite references before deleting
a task. IDs remain reserved after deletion. Preview refuses legacy DB versions and pending recovery without
changing files; run `qqq list` to migrate or recover before previewing again.
If deletion stops while images are staged, next DB-backed qqq command recovers
them according to committed DB state. Keep backup until recovered project
passes `qqq doctor`.

Check project health without changing DB or attachments:

```sh
qqq doctor
qqq --json doctor
```

Doctor checks SQLite integrity, foreign keys, schema version, combined dependency
graph cycles/duplicate edges/missing refs, image paths, byte counts,
signatures, and orphan files/directories. Healthy project exits 0. Issues print recovery
actions and exit 1; JSON includes `ok`, counts, and `issues` with code, path,
message, and action. Doctor never migrates DB. If SQLite journal/WAL sidecars
exist, stop writers and recover or checkpoint SQLite before rerunning doctor.
Persisted WAL mode without sidecars also defers check (`DB_READ_ONLY_UNSAFE`),
leaving source files untouched. Stop writers, switch to journal_mode=DELETE
through SQLite before retrying.
For damaged DB or images, restore verified snapshot into new directory first;
inspect recovered data before replacing damaged project files. Keep damaged
copy until recovery is verified. For `DELETE_RECOVERY_PENDING`, follow returned
action: fix unsafe staging paths or access errors first. Valid interrupted
deletions recover through normal DB-backed command, such as `qqq list`;
rerun doctor afterward.

To move old root-level `qqq.db`, stop DB writers. Before `qqq init`, check that
`.qqq/qqq.db` does not exist. From project root:

```sh
mkdir -p .qqq
sqlite3 qqq.db ".backup '.qqq/qqq.db'"
qqq list # run updated CLI
```

Check task data before removing old DB. New CLI does not discover root-level
`qqq.db`; `qqq init` without migration creates separate empty DB.

Compatible source schemas listed above migrate to current DB schema after saving
verified original-schema recovery archive outside live store. Upgrade preserves
task/message/event/image IDs, reserved SQLite ID sequences, descriptions,
timestamps, parent/dependency edges, history, stored images and active claim
keys. Existing queue readiness remains consistent with existing dependencies;
new fields use defaults when source schema predates them:

| Added in schema | Field/behavior | Value for older data |
| --- | --- | --- |
| 2 | Parent | `parent_id=NULL` |
| 3, 5 | Ownership/identity | Owner key preserved through renames; active harness identity derives from saved Herdr link or prior claim key; orchestrator identity derives from saved link |
| 4 | Error status | Existing status/history unchanged; no error tasks invented |
| 7 | Priority | `0` |
| 8 | Archived | `false` |
| 9 | Reopen event | Existing events unchanged |
| 10 | Content revision | `1` |
| 11 | Extra prerequisites | Empty; existing parent edges retained |
| 12 | Tags | Empty array `[]` |
| 13 | Private claim process identity | Empty; legacy Herdr links still support liveness checks |

Version 6 moves existing image blobs to `.qqq/images/` before SQLite drops its
`data` column, preserving IDs, metadata and bytes. Portable restore preserves
stored schema; next normal DB open upgrades older versions.
Migration tries `VACUUM` to reclaim old blob pages. If compaction warns, stop
writers and run `sqlite3 .qqq/qqq.db 'VACUUM;'` later.

Coordinate workers sharing project: stop older workers and waiters, upgrade all
binaries, run one normal command such as `qqq list`, verify `qqq doctor`, then
restart workers. Older binaries reject schemas newer than their own supported
version; do not run mixed versions after migration. Claims remain explicit:
migration does not expire, transfer, release or replace active ownership. Original
owner still completes/releases work explicitly; elapsed time does not free claim.
Back up before manual conversion of unsupported legacy layouts.

## Development

Tests require Python 3 for real-terminal coverage, POSIX shell and `sqlite3` CLI
for executable recovery documentation on macOS and Linux.
[Historical compatibility fixtures](../tests/fixtures/compatibility/README.md)
cover every normal-open DB schema and accepted portable snapshot schema. Tests
copy checked-in artifacts; regeneration uses frozen historical sources.
Snapshot validation rejects legacy title/pending layouts and invalid status/claim
relationships before installing DB/images. Human and JSON failures preserve
original projects. Diagnostics and all dry runs never migrate; next dry-run
requires current schema, previews queued candidate without claiming.
Recovery matrix checks original-schema equality, embedded/external images, WAL
and concurrent writer consistency, one archive per concurrent upgrade, retained
records after SQL/image/commit failures, snapshot verification failure and
metadata/claim/checksum rejection before installing restored projects.

```sh
./scripts/check-compatibility-docs.sh
./scripts/check-compatibility.sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

[CI](../.github/workflows/ci.yml) checks Ubuntu 24.04 and macOS 14, builds release
binary, uploads `qqq-<OS>-<ARCH>` archives (14-day retention). Both CI OSes and
publication require `scripts/check-compatibility.sh`: immutable historical DB
upgrade/restore matrix plus identity, image-storage, dependency/tag CLI and
snapshot regressions. Matrix verifies repeated/concurrent opens, rollback/retry,
field defaults, ownership/readiness and reserved ID sequences. Missing fixture
coverage for a new schema or snapshot format fails gate. Focused
`scripts/check-compatibility-docs.sh` checks marked support table against fixture
metadata, initialized schema and runtime archive metadata (`SCHEMA_VERSION`
sets upgrade target). It executes documented portable/recovery examples in
temporary historical projects. New schemas require matrix fixtures and published
facts to change together; unrelated prose is outside contract check.

### Publish to crates.io

[Release checklist](release-checklist.md) covers compatibility fixtures, recovery,
user-visible schema notes, package/binary versions and existing publication gates.

[Publish workflow](../.github/workflows/publish.yml) checks `v<version>` tag against
`Cargo.toml`, runs checks and package dry run, then publishes `qqq-cli`. Setup:

1. Sign in to crates.io, verify email, create token allowed to publish `qqq-cli`.
2. Add GitHub Actions repo secret `CARGO_REGISTRY_TOKEN`.
3. Choose unused version, update `Cargo.toml` and `Cargo.lock`, commit changes.
4. Push commit and matching tag. Example for version `0.3.0`:

```sh
git push origin master
git tag v0.3.0
git push origin v0.3.0
```

For manual validation, run **Publish to crates.io** with existing `tag` and
default `dry_run=true`. Set `dry_run=false` to publish new version. Published
versions cannot be overwritten.

License: [MIT](../LICENSE).
