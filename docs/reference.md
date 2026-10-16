# qqq reference

[← README](../README.md) · [Quick start](../README.md#try-it)

Command behavior for the current source checkout. Published releases may lag;
run `qqq COMMAND --help` for the flags in your installed version.

- [Install](#install-from-source) · [List and show](#list-and-show)
- [Add and edit](#add-and-edit) · [Import](#atomic-batch-import) · [Recipes](#task-recipes)
- [Complete](#complete-work) · [Reopen](#reopen-work) · [Return or retry](#return-or-retry-work)
- [Archive](#archive-and-unarchive) · [Bulk actions](#bulk-task-actions)
- [Editor](#built-in-editor) · [TUI](#task-tui) · [Dependencies and images](#dependencies-and-images)
- [Workers and ownership](#agents-and-recovery) · [Diagnostics](#queue-diagnostics)
- [Config](#config-and-aliases) · [Herdr](#herdr) · [JSON errors](#json-error-contract)
- [Backups and recovery](#data) · [Development](#development) · [Publishing](#publish-to-cratesio)

## Install from source

Requires Rust 1.89+ and a C compiler.

```sh
git clone https://github.com/hktonylee/qqq-cli.git
cd qqq-cli
cargo install --path . --locked --force
```

Run `qqq init` in a project before adding tasks. Commands find the nearest
`.qqq/` in the current directory or its parents and use its `qqq.db`.
A missing database there is an error. `qqq config` needs no project database.

## List and show

```sh
qqq list
qqq list --query "parser error" --all
qqq list --tag frontend --status new --readiness ready
qqq list --filter 'like(task_name, "%auth%") and priority > 0'
qqq list --view "Ready frontend"
qqq list --watch
qqq show 12
qqq show -1                         # newest task; -2 is second newest
```

`list` shows tasks in dependency-tree order, with roots and siblings ordered by
creation ID. Terminal output wraps descriptions; piped output uses first-line
previews. `show` includes full text, messages, images, history and the Herdr link.
Negative creation indexes work with `show` and `edit`.

| List option | Effect |
| --- | --- |
| `--oneline` | Show only each description's first line; JSON and search retain full text |
| `--max-completed N` | Keep the N most recent completions plus unfinished tasks; `0` hides completions |
| `--all` | Bypass the completed-task limit; conflicts with `--max-completed` |
| `--include-archived` | Include archived rows; `--all` alone does not include them |
| `--hide-archived` | Hide archived rows, overriding a saved view |
| `--watch` | Refresh after database commits; Ctrl-C stops |

Archived tasks are hidden by default. Hidden parents leave visible children as
roots. Hidden archived completions do not consume the completed-task limit.

### Selectors and saved views

`list`, `next` and `tui` share these selectors:

| Selector | Match |
| --- | --- |
| `--query TEXT` | Substring anywhere in the full description, using Unicode lowercase comparison |
| `--status STATUS` | `new`, `in_progress`, `completed` or `error`; repeat for OR |
| `--tag LABEL` | Whole stored label, with exact case and Unicode; repeat to require every tag |
| `--filter EXPR` | Typed Luau expression compiled to SQLite with bound values |
| `--readiness ready\|blocked` | Shared dependency eligibility; blocked means new, unarchived and not ready |
| `--view NAME` | Saved project selectors and visibility |

Different selectors combine with AND. List filtering follows the completed-task
limit; use `--all` to search all completions. Matching tasks retain visible
ancestors as `[context]` rows (`context_only: true` in JSON).

Save and inspect selectors with `qqq view save NAME`, `view list`, `view show NAME`
and `view remove NAME`. Extra flags narrow a saved view; explicit archive and
completed-limit flags override its visibility. See [named views](views.md),
[filter syntax](filter.md), [variables](filter-variables.md) and
[functions](filter-functions.md). `has_tag("LABEL")` tests one exact valid label.

Tag selectors trim boundary whitespace and reject invalid labels before project
preflight. Changing a worker's selectors never releases its existing claim.

### Output

Global `--json` and `--human` work before or after a command and conflict with
each other. Without either flag, qqq uses JSON for recognized agent callers:
nonblank UTF-8 `CODEX_THREAD_ID`/`CODEX_SESSION_ID`, or an exact current Herdr pane
with agent metadata. Pipes, `QQQ_SESSION` and harness overrides alone do not
select JSON. Failed optional caller discovery keeps human output; explicit flags
skip that lookup.

JSON lists are flat and preserve full descriptions, `parent_id` and tags.
Empty lists return `[]`; an empty `next` returns `null`. Watch emits one array
per line. Human terminal watch redraws; pipes and `TERM=dumb` append snapshots.
`NO_COLOR=1` or `TERM=dumb` disables color. Help/version remain text, and editors
remain interactive. Failures follow the [JSON error contract](#json-error-contract).

## Add and edit

```sh
qqq add "Fix login" --priority 8 --tag backend
qqq add --description "Fix layout" --tag frontend --tag bug
qqq add --stdin < description.txt
qqq edit 12 --description "Updated details" --priority -5
qqq edit 12 --set-tags "frontend, bug"
qqq edit 12 --set-tags ""             # clear tags
qqq edit -1                          # edit newest task interactively
```

Descriptions preserve whitespace and newlines; blank-only text fails. `--stdin`
reads UTF-8 to EOF without opening an editor and conflicts with positional text,
`--description` and `--edit`. Parent, prerequisite, priority, tag and image flags
can accompany it.

Priority is an integer from `-100` to `100`, default `0`. Ready workers claim
higher priority first, then oldest ID. Editing priority does not reorder lists
or replace existing ownership.

Tags are ordered labels displayed as `[frontend] [bug]` before the description.
`--tag` adds one label per flag; `--set-tags` replaces the comma-separated list
and conflicts with `--edit`. Boundary spaces are trimmed and exact duplicates
removed; case, Unicode, internal spaces and order are preserved. Empty labels,
controls, commas and square brackets are invalid. JSON keeps raw description
separate from `tags`, which is always an array.

Direct update flags skip the editor unless explicitly supported with `--edit`;
omitted fields stay unchanged. Combined content, metadata, dependency and
lifecycle edits commit atomically. Metadata edits also work on completed or
archived tasks, subject to the operation's own guards.

### Concurrent edits

Task JSON has a positive `content_revision`. Only description or attachment
changes advance it; an identical save does not. Messages, metadata, lifecycle
and dependency changes leave it unchanged.

```sh
qqq show 12 --json                   # task.content_revision
qqq edit 12 --description "Reviewed text" --expected-revision 3
```

A stale `--expected-revision` rejects the whole operation, including accompanying
metadata and images. Without the flag, direct edits save unconditionally.
Interactive editors guard the revision they loaded automatically; see
[editor conflicts](#content-conflicts).

## Atomic batch import

```sh
qqq import plan.json --dry-run --json
qqq import plan.json
cat plan.json | qqq import - --json
```

Version-1 input:

```json
{
  "version": 1,
  "tasks": [
    {"key": "tests", "description": "Run regression checks", "parent": {"key": "feature"}},
    {"key": "feature", "description": "Build feature", "priority": 10, "tags": ["backend"]}
  ]
}
```

| Task field | Rule |
| --- | --- |
| `key` | Required, unique, nonblank string; references match its exact whitespace |
| `description` | Required nonblank string; preserves whitespace and newlines |
| `priority` | Integer -100..100; defaults to `0` |
| `tags` | Valid normalized labels; defaults to `[]` |
| `parent` | `{"key":"feature"}`, `{"id":12}`, or null/omitted |
| `depends_on` | Array of the same reference objects; defaults to `[]` |

Key references may point forward within the batch. ID references must name
existing positive task IDs. Each reference has exactly one type. Archived
unfinished dependencies are rejected; archived completed dependencies are valid.
Version 1 has no image fields. An empty task array is valid.

The whole batch validates and commits in one transaction. Missing references,
cycles across both dependency types, duplicate prerequisites, parent/prerequisite
overlap, invalid values, unknown/duplicate fields and unsupported versions fail
without partial tasks or consumed IDs. Normal project migration and deletion
recovery may run before the import transaction.

Dependencies are inserted first; ties use the lowest original input index.
For the example, an empty project creates `feature` as ID 1 and `tests` as ID 2.
New tasks start `new`, unarchived and unassigned.

`--dry-run` validates through a read-only snapshot without migration, deletion
recovery, writes or reserved IDs. Upgrade older supported databases with
`qqq list` first. Real import rechecks existing references in its write transaction.

JSON returns `version`, `dry_run`, `count`, `mapping`, `creation_order` and
normalized `tasks`. Mapping keys are sorted; tasks retain input order and include
original references, `id`, `parent_id` and `prerequisite_ids`. Preview mapping is
`{}` and new IDs are null; existing dependency IDs remain known.

## Task recipes

Store reusable version-1 batches in `<project-root>/.qqq-recipes/NAME.json`.
The project must be initialized. Discovery uses the nearest project, including
from subdirectories; names allow ASCII letters, digits, underscores and hyphens.
Pass the name without a path or `.json`. Recipe files can be committed to Git.

Copy the bundled [bug](../examples/recipes/bug.json) and
[release](../examples/recipes/release.json) recipes into that directory:

<!-- recipe-cli-examples -->
```sh
qqq add --template bug --var component=auth --dry-run --json
qqq add --template bug --var component=auth --json
qqq add --template release --var version=0.5.1 --dry-run --json
qqq add --template release --var version=0.5.1 --json
```
<!-- /recipe-cli-examples -->

Each application creates new queue records. The release recipe creates
`prepare`, `tests` and `publish` with dependency links; it does not run commands.

```json
{
  "version": 1,
  "parameters": [{"name": "component"}, {"name": "severity", "default": "normal"}],
  "tasks": [{
    "key": "bug",
    "description": "Fix ${component} bug\nSeverity: ${severity}",
    "tags": ["bug", "${component}"],
    "priority": 5
  }]
}
```

`version` and `tasks` are required; `parameters` defaults to `[]`. Parameter names
are case-sensitive ASCII identifiers. A string `default` makes a parameter
optional; every other declaration requires a value, even if unused in task text.
Task fields and graph validation follow [import](#atomic-batch-import).

Repeat `--var NAME=VALUE`. Split occurs at the first `=`; quote shell-sensitive
values, such as `--var 'component=auth = account login'`. Empty values count as
provided but expanded descriptions and tags still must validate.

`${name}` substitutes only description and tag strings, once. `$$` emits `$`;
`$${name}` emits literal `${name}`. Other dollar forms remain literal. No shell,
environment lookup or code evaluation occurs. Use normal JSON string escaping.
Unknown/malformed placeholders, missing/duplicate/unknown assignments, duplicate
parameters, invalid fields and invalid graphs fail before task creation.

`--template` owns task data and conflicts with inline descriptions, `--stdin`,
`--edit`, `--parent`, `--depends-on`, explicit `--priority`, `--image` and `--tag`.
`--var` and add's `--dry-run` require `--template`.

Preview validates expanded values and dependencies without reserving IDs or
migrating. Command-start owner preflight still applies. Real application inserts
the whole graph atomically. Human preview shows expanded text/tags; JSON uses
the import report, mapping recipe keys to new IDs after commit.

## Archive and unarchive

```sh
qqq edit 12 --archive
qqq list --include-archived
qqq edit 12 --unarchive
```

Archive changes visibility, preserving status, content, history, attachments and
dependencies. Access by ID still works. Actual changes record archive/unarchive
events; repeating a flag is a no-op. The actor is `--session`, or `cli` if absent.

The two flags conflict with each other and `--edit`, skip the editor, and can
combine with direct edits and `--expected-revision` in one transaction.
In-progress tasks cannot be archived. An unfinished dependency cannot be archived
while a visible unfinished task depends on it. Adding links or unarchiving an
unfinished dependent under an archived unfinished dependency also fails.
Archived completed dependencies still satisfy readiness.

## Bulk task actions

Preview metadata changes, review the exact IDs and values, then apply the saved
JSON. Generating or discarding a preview changes no selected task.

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

Use real task IDs in the examples. `--apply -` reads a saved preview from stdin;
`--human` prints readable results.

| Selection | Rule |
| --- | --- |
| Repeated `--id ID` | Positive absolute IDs, sorted/deduplicated; includes archived tasks |
| `--query`, `--status`, `--tag`, `--filter` | Same matching rules as list; only direct matches enter the batch |
| `--all` | Explicitly select all visible tasks |
| `--include-archived` | Expand selector visibility; not a selection on its own |

IDs conflict with selectors. Different selectors combine with AND; completed
display limits never apply. Empty selections are valid.

| Action | Effect |
| --- | --- |
| `--add-tag LABEL` / `--remove-tag LABEL` | Add/remove labels; repeat flags |
| `--set-tags LABELS` | Replace comma-separated tags; `""` clears |
| `--priority N` | Set priority from -100 through 100 |
| `--archive` / `--unarchive` | Set visibility using final-batch dependency guards |

At least one action is required. Set-tags conflicts with add/remove; the same
label cannot be added and removed. Archive/unarchive conflict. Tags, priority
and visibility may combine. Use individual commands for status and ownership.

Preview JSON contains `version: 1`, `applied: false`, canonical `database` path,
normalized `actions`, `selected_ids`, `selected_count`, `changed_count`,
`action_count` and `tasks`. Rows contain `id`, `before`, `after` and opaque
SHA-256 `fingerprint`; metadata is `tags`, `priority`, `archived`.
`changed_count` counts changed tasks; `action_count` counts changed fields.
No-op rows write no task data or events. Apply returns `applied: true`.

Apply uses frozen IDs, not a fresh selector query. The preview must belong to
the same canonical database path. Selected-task content, attachments, assignment
or relevant metadata changes invalidate it with `BULK_CONFLICT`; messages alone
do not. Regenerate and review after a conflict. Invalid/inconsistent previews
use `INVALID_ARGUMENT`; an applied report cannot be reused.

All writes and archive events commit together or roll back together. Active
tasks cannot be archived; parent/dependent visibility is checked against the
final batch state. Other content and claims stay intact.

## Complete work

```sh
qqq edit 12 --complete --session worker
qqq edit 12 --complete --session worker --description "Finished" --archive
```

Completion requires an in-progress task owned by the supplied/discovered session.
It ends the claim, records a `complete` event and retains public assignment
metadata and the Herdr link for log lookup.

`--complete` and `--reopen` skip the editor and combine atomically with direct
edits. They conflict with each other, `--edit`, `--set-status` and `--set-pending`.
`--expected-revision` guards the whole operation. The standalone `complete` and
`reopen` commands have been removed.

## Reopen work

```sh
qqq edit 12 --reopen
qqq edit 12 --unarchive --reopen
```

Reopen returns completed tasks and preflight-generated errors to `new`, clears
assignment metadata and records a reopen event. Ordinary manual errors use
[retry](#return-or-retry-work). Content, dependencies, attachments and prior
history remain; the saved Herdr link stays until the next claim replaces it.

An active task can reopen only when a successful lookup on its saved Herdr
server establishes that its owner is absent. Exact session identity or the same
terminal and agent kind counts as live, including moved panes. Missing links,
failed/malformed lookups and mismatched claim associations cannot prove absence.

New tasks cannot reopen. Archived tasks require unarchive first or in the same
edit. Archived unfinished dependencies must be completed, unarchived or unlinked.
Completed descendants stay completed; new dependents wait for the reopened task.

The TUI also offers explicit force reopen; see [action confirmations](#action-confirmations).

## Built-in editor

On a terminal, `add` without text and `edit` without update flags open the built-in
editor. UI goes to stderr; stdout receives the result after exit.

| Key | Action |
| --- | --- |
| Ctrl-S | Save |
| Enter / Shift-Enter | Insert newline |
| Ctrl-V | Paste clipboard text or image; requires desktop clipboard support |
| Ctrl-W | Delete previous word within the current line |
| Alt/Option-Left/Right | Move by word |
| Ctrl-Z / Ctrl-Y | Undo / redo description edits |
| Shift-Up/Down | Browse tasks in interactive add |
| Esc | Exit blank draft or confirm discard |
| Ctrl-C | Exit; saved tasks remain committed |

Interactive `add` keeps accepting tasks after each save. Shift-Up loads newest
then older unarchived tasks; Shift-Down returns toward a blank draft. Switching
a changed draft asks before discard. New-task parent, prerequisite and priority
flags apply to every creation; `--image` files attach only to the first successful
save, including an existing-task update. Inline add, external edit and `edit`
save once. JSON interactive add returns an array of saved tasks; no save is an error.

Undo treats each input, deletion, word deletion or paste as one step, including
image/pasteboard items. It restores content and caret. Movement/failed edits keep
redo; editing after undo clears it. History keeps up to 256 changes and 64 MiB of
removed payloads, retaining one oversized change if necessary. It changes only
the local description; filters and popup inputs have no description undo.

Pastes over 1,000 characters collapse into a gold `[Pasted Content N chars]` item.
Saving stores the full payload in a fenced `pasteboard` block; backtick-containing
payloads use longer fences. Reload restores complete blocks as atomic editable
items. Backspace/Delete removes the whole item. Other text/fences remain ordinary
text; unchanged content preserves its original bytes.

### External editor

```sh
export EDITOR='vim'                  # must wait until editing finishes
qqq add --edit
qqq edit 12 --edit --description "Prefilled draft"
```

`--edit` (`-e`) forces `$EDITOR`. Without terminal input and stderr, interactive
add/edit uses it automatically. Direct edits preserve omitted fields and images.

### Content conflicts

Interactive saves check the loaded revision. A concurrent content change opens
a conflict view instead of overwriting it. Message/status changes do not conflict.

| Conflict view | Action |
| --- | --- |
| Tab | Switch complete current DB text / local draft |
| Arrows, PgUp/PgDn, Home/End | Scroll |
| `r`, then `y` | Reload latest DB text, discarding local text and pending images |
| `o`, then `y` | Overwrite against the displayed revision; another edit rejects it again |
| Esc / Ctrl-C | Keep draft, caret, paste blocks and pending images |
| `n`, Enter, Esc in confirmation | Cancel the proposed reload/overwrite |

A removed task is never recreated by saving. Retained TUI drafts keep their
original loaded revisions; resolving one does not change the others.

External editor conflicts preserve complete local/current text and pending image
files beside an `attachments.json` recovery manifest. With terminal input, `r`
reloads after confirmation, `o` overwrites after confirmation, and `k` keeps files
and exits with failure. Without terminal input, qqq keeps files and fails.
Review them before retrying with `--expected-revision` and any recovered `--image`
paths. JSON failures may include these paths in `details.recovery`.

## Task TUI

```sh
qqq tui
qqq tui --view "Ready frontend" --include-archived
```

The TUI needs a terminal and writes no stdout, including with `--json`. The task
list and details refresh after external commits without replacing local draft
text or manual scroll. Lists and filters use saved descriptions.

### Navigation and drafts

| Key / gesture | Action |
| --- | --- |
| Shift-Up/Down or Ctrl-Up/Down | Select tasks in visible tree order, then the general new draft |
| Click task / editor text | Load the task / place the caret |
| Mouse wheel | Scroll the pane under the pointer |
| PgUp/PgDn | Scroll selected-task details from the editor |
| Ctrl-S | Save only the active draft |
| Ctrl-P | Open/restore a child draft; inside it, insert the parent's plain `#ID` |
| Ctrl-K | Jump to a positive task ID |
| Ctrl-B | Open saved views; `n` creates one |
| Ctrl-L | Edit task or draft tags |
| Ctrl-D | Toggle a saved task's bulk-selection mark |
| Ctrl-G | Open task actions, or bulk actions while tasks are marked |
| Ctrl-H | Open the linked live Herdr agent; detach to return |
| Ctrl+/ | Focus the description filter |

Task previews show up to three wrapped lines; the editor loads the full text.
Selection follows the full preview when it fits. Manual scrolling can leave it
partly visible; editing reveals the caret again. Selecting another task resets
its details scroll. A blank draft keeps the details pane in place.

Switching tasks retains unsaved general, task and per-parent child drafts,
including staged tags, image/paste items, undo history, caret and editor scroll.
Dirty drafts show `[*]`. Saved tasks stay open by default; set
`tui.after_save_new = "open_new"` to return to a blank draft after creation.
Existing-task edits stay open. Ctrl-P parent references are ordinary text and
add no dependency; child creation itself retains its parent link.

Undo history survives saving while the same task stays open. Accepted reload,
discard, fresh task load or clearing to a new draft starts a fresh history.

Ctrl-K retains drafts and clears the live query on a successful jump. Archived
jumps enable archived visibility; completed jumps reveal hidden completions.
Saved selectors and positive completion limits still apply, so an opened task
may have no visible list row. Invalid/missing IDs keep the popup open.

Ctrl-B applies a saved view with Enter; Esc/Ctrl-C cancels and PgUp/PgDn scrolls
criteria. In the new-view form, Tab/Shift-Tab switches fields, Ctrl-F opens
filter templates and Ctrl-S saves. View changes preserve the opened task, drafts,
query and focus. The picker reloads the catalog when opened; the active view
stays fixed until another selection. See [views](views.md).

### Tag input

Ctrl-L prefills one tag per line. Enter adds a line; Ctrl-S applies; Ctrl-U
deletes the current line; Esc/Ctrl-C cancels. Blank lines are ignored and blank
input clears tags. CRLF/newline paste and comma-separated input are accepted.
Invalid labels stay in the popup for correction.

Type a prefix to suggest existing project tags, including tags outside the
current view. Up/Down selects a suggestion; Tab completes the label at the caret.
Already-entered tags are excluded. Custom labels remain available. Without
suggestions, Up/Down moves between input lines. Left/Right, Home/End, Ctrl-A/E,
Alt-Left/Right and Ctrl-W retain normal caret/word editing. Long inputs scroll
to keep the active row visible. Compact hints use `^S`, `^U` and `↵`.

Applying saved-task tags leaves unsaved description drafts, scroll and filters
intact. On general/child drafts it stages tags locally; no task is created until
Ctrl-S saves a nonempty description. Tags survive navigation and count as dirty
work for discard prompts. Bulk add/replace tag inputs use the same completion.

### Bulk selection

Ctrl-D marks tasks with `+`; marks survive filtering and refresh. Ctrl-G opens
add/remove/replace tags, priority and archive/unarchive actions for the exact
marked IDs. `x` clears marks.

Remove-tags offers the sorted union of saved labels on all marked tasks,
including hidden ones. Up/Down picks; Space toggles; Enter/Ctrl-S previews.
Add/replace inputs retain multiline editing and Ctrl-S preview.

Preview shows changed values. Only `y` applies; `n`, Enter or Esc cancels.
Success clears marks; conflicts retain them for a new preview. These operations
use saved tasks and preserve local drafts, caret, scroll and filter focus.

### Filtering and visibility

Ctrl+/ opens `Filter:`. Type a case-insensitive description query; Backspace edits,
Tab/Enter returns to the editor, and Shift-Up/Down navigates matching tasks.
`/` and Alt+/ remain literal text. Matching tasks retain visible ancestors.

| While filtering | Action |
| --- | --- |
| Ctrl-T / click Completed | Toggle completed tasks |
| Ctrl-A / click Archived | Toggle archived tasks |
| Esc | Clear query and close filter focus |
| Ctrl-C with a query | Clear query, keeping focus |
| Ctrl-C with an empty focused filter | Return to editor |

Visibility toggles preserve drafts and reset to startup settings next session.
The filter bar remains visible while focused, nonempty, hiding completions or
including archives. Narrow panes abbreviate the visibility buttons.

### Action confirmations

Ctrl-G's task menu supports arrows/Enter or letter shortcuts:

| Key | Action |
| --- | --- |
| `c` | Complete; hidden on completed tasks |
| `e` | Mark error with a nonblank reason; hidden on error/completed tasks |
| `r` | Retry an error task |
| `o` | Reopen a non-error task |
| `p` | Set priority (-100..100) |
| `d` | Set parent to a positive ID or `none` |
| `a` | Archive/unarchive |

Menus skip unavailable actions and refresh availability after status changes.
Status changes require confirmation; priority/parent changes ask when the draft
is dirty. Rejected actions keep the draft and display the error.

| Confirmation | Lowercase `y` | Uppercase `Y` |
| --- | --- | --- |
| Complete | Require the current owner of an in-progress task | Force a new, active or error task to completed |
| Reopen | Apply normal completed/orphaned-owner checks | Force a completed or active task to new |
| Mark error | Require the current owner of an in-progress task | Force a new or active task to error |

Force is always explicit and skips owner/Herdr lookup. Completion retains
assignment metadata and the saved link; reopen/error clear assignment. Error
marking records the reason message and error event. Content and dependency
edges stay intact; archive/dependency guards still apply where required.

`n`, Enter or Esc cancels confirmation. Reopen/Mark-error confirmations hide the
caret, ignore editing/navigation and Ctrl-C, and preserve filters on Esc.
Completion errors retain normal/force choices after lookup or ownership failures;
unrelated database/input errors keep ordinary error handling. Force never happens
automatically. Action-error text scrolls with Up/Down or `j`/`k`; Enter closes it.
In other menus/errors, Esc clears an active filter before closing the popup.

### Clearing and exit

From the editor, Esc clears the active editor before a later press exits; dirty
drafts ask first. Ctrl-C returns a selected task to a blank draft, asking before
discarding saved-task edits. In a nonempty new/child draft, Ctrl-C immediately
clears text, tags, images, paste items and undo history; a child retains its parent.
From an empty new draft it starts exit.

Exit asks about every retained dirty draft, including hidden/deleted tasks.
`y` advances; `n`, Enter or Esc cancels the whole exit and preserves every draft,
including earlier approvals. Repeated Ctrl-C keeps the prompt open. Deleted-task
drafts cannot recreate their task by saving.

### Layout

Below 60 columns, list/details/editor stack in roughly equal heights. From
60–149 columns their heights are 35%/20%/45%. At 150+ columns, list and details
share the upper area at 60%/40% width; the editor stays full-width below.
Details show the same content as `show`, with independent scrolling. Completed
rows remain gray when selected; `NO_COLOR` preserves text markers and controls.

## Dependencies and images

A task has at most one tree parent and any number of extra prerequisites. It is
ready only when new, unarchived and every dependency is completed. Extra edges
do not change tree placement. Workers, previews and diagnostics use
this same rule.

```sh
qqq add "Integrate" --parent 1 --depends-on 2 --depends-on 3
qqq edit 4 --depends-on 5
qqq edit 4 --remove-depends-on 2
qqq edit 4 --clear-depends-on --depends-on 6
qqq edit 4 --set-parent none
```

Extra-prerequisite flags skip the editor and conflict with `--edit`.
`--set-parent` also skips it unless `--edit` is supplied. Dependency changes
can combine with direct edits. Clear removes extras only and is idempotent. IDs must
exist. Self-links, cycles across either edge type, duplicate additions, missing
removals and parent/prerequisite overlap fail atomically. Add/remove of the same
ID and clear/remove together also fail. The final graph is validated, allowing
an edge to move between parent and prerequisite in one edit.

Dependency changes affect future claims, preserving active owners, completed
descendants and content revisions. Archived completed dependencies satisfy
readiness; archived unfinished ones follow the archive guards.

Task JSON exposes ordered `prerequisites` entries with `id`, `status`, `archived`;
missing references have null status/archive. Human/TUI details mark unfinished
entries as `blocks claim` and refresh after external changes.

```sh
qqq edit 12 --image ./before.png --image ./after.jpg
qqq show 12
qqq show 12 --export-image 1 --output ./exported.png
```

Repeat `--image` to attach files. PNG, JPEG, GIF and WebP signatures are accepted,
up to 20 MiB each; this is a signature check, not full validation. Bytes are copied
to `.qqq/images/<task-id>/<image-id>.<ext>`. Export uses the selected task's image
ID from `show` and requires a new output path.

Pasted image paths or clipboard images become Markdown links. Reloading shows
linked images as atomic editor items. Removing a link leaves its attachment
available through `show`. Content and attachment changes save atomically.

## Queue diagnostics

```sh
qqq status --json
qqq status --include-archived
qqq next --explain --tag frontend --json
qqq next --explain --session worker-1
```

`status` reports counts, owners, dependency blockers and latest activity.
`next --explain` adds selection diagnostics without claiming.
It needs no session and skips automatic identity discovery; explicit `--session`,
`QQQ_SESSION` or `--harness-session` can inspect owned-task reuse. Add
`--harness-name` for ambiguous public sessions. Owned tasks precede filters.
Explanation conflicts with `--wait`/`--dry-run`; next's `--include-archived`
requires `--explain`.

Diagnostics use one read-only SQLite snapshot and do not create/migrate projects
or recover pending deletion files. Older schemas need `qqq list` first.
Command-start owner preflight still runs. Reports reserve no work and activity
age never expires claims.

### Diagnostic JSON

| Field | Meaning |
| --- | --- |
| `include_archived` | Requested scope |
| `counts` | `total`, `new`, `ready`, `blocked`, `matching_ready`, `in_progress`, `error`, `completed`, `archived` |
| `state` | `empty`, `ready`, `no_matching_ready`, `blocked`, `error`, `in_progress`, `no_ready` |
| `tasks` | Diagnostic rows in ascending task ID order |
| `explanation` | Selection detail for explain; null for status |

`ready + blocked = new`; archived new tasks count as blocked when included.
`matching_ready` applies selectors to ready tasks. Archived is a subset, not a
status. Mixed unavailable queues and completed-only queues use `no_ready`.

Each row contains `task`, `ready`, `matches_filter`, `queue_rank` (one-based among
all ready tasks, otherwise null), `reasons`, `owner` (active claim key or null),
`blockers` and `latest_activity`. Blockers include unfinished/missing parent and
extra prerequisites, regardless of archive scope.

Reason codes are `archived`, `parent_not_completed`, `prerequisite_not_completed`,
`in_progress`, `error`, `completed`, `filter_excluded`, `tag_excluded`. A ready
matching task has no reasons. Tag exclusion can coexist with dependency blockers;
failed predicates inside Luau use `filter_excluded`.

Latest activity has `at`, `source`, `id`, `session`, `action`. It compares task
updates with messages/events; timestamp ties prefer message, then event, then
task, with higher IDs breaking same-source ties. Task activity has null
ID/session/action; messages have null action.

Explanation has `owner_input`, `resolved_owner`,
`ordering: ["priority_desc","id_asc"]`, matching `eligible_ids`, `outcome` and
`selection` (null or `{"kind":"owned"|"queued","task":...}`). Outcomes are
`owned_task_reuse`, `ready_candidate`, `empty_queue`, `no_matching_ready_candidate`,
`blocked_queue`, `error_queue`, `in_progress_queue`, `no_ready_tasks`.
An archived owned task can be selected even when absent from scoped rows.

## Agents and recovery

Use one stable, unique session per worker:

```sh
export QQQ_SESSION='worker-1'
qqq next
qqq message 12 "Running tests"
qqq edit 12 --complete
qqq next --wait
```

Each owner gets one active task; repeated `next` returns it before considering
new work or filters. Claims never expire. Ready tasks are ordered by priority
descending, then ID ascending. Error/blocked tasks are not claimable.
`--wait` blocks until eligible work appears; without it an empty queue succeeds
with human `No task available for pickup.` or JSON `null`.

`next --dry-run` previews queued work without reusing claims, discovering an owner,
reserving IDs or changing queued tasks. It accepts selectors,
`--wait` and `--json`. Another worker may claim the previewed task.
The preview does not migrate or recover deletion staging; older schemas return
`DATABASE_ERROR` / `migration_required` until a normal open upgrades them.

Strict read-only operations refuse WAL mode or sidecars (`unsafe_read_only`)
because SQLite could create WAL/shm files. Stop writers and run
`sqlite3 .qqq/qqq.db 'PRAGMA journal_mode=DELETE;'` before retrying.

### Owner identity

Resolution order:

1. `--session` / `QQQ_SESSION`, then explicit `--harness-session`.
2. Exact Herdr pane (`HERDR_PANE_ID`, or `HERDR_ENV=1` with a pane ID).
3. Native Codex `CODEX_THREAD_ID`, then `CODEX_SESSION_ID`.
4. Unique Herdr agent at the project root (`foreground_cwd` before `cwd`).

`next` always claims for the calling worker and never creates an agent.
`--local` remains accepted as a compatibility flag with no effect. Explicit/native
identity works without Herdr. Other harnesses
use Herdr identity or explicit flags; qqq does not guess unrelated environment
variables. Missing agent-session hooks fall back to terminal ID plus agent kind.

Automatic claims save detected identity and a Herdr link when available.
Assignment JSON has `harness_name`, `harness_session`, `orchestrator_name`,
`orchestrator_session`; human `next` omits these fields, while JSON/show retains them.
For Codex, displayed session prefers `CODEX_SESSION_ID`; an explicit
`--harness-session` overrides it. Herdr orchestrator session is the worker's pane
ID, not the separately saved server name. Dispatch records the child pane.
Repeated claims retain metadata unless assignment flags override it.

Completion/release/error marking accepts the original owner key or a uniquely
matching public harness session; add `--harness-name` when ambiguous.
Completion keeps assignment for log lookup; release/error/reopen clears it.
Saved links remain until replaced. Session IDs coordinate local agents and do
not authenticate users; local callers may edit content or add messages.

### Owner liveness before commands

qqq checks active project owners synchronously before commands and TUI/watch
startup. Confirmed dead owners become `error`: assignment clears, a reason
message and `qqq-preflight` event are recorded, and content/history/link remain.
This also precedes inspection/preview commands; their read-only phase does not
itself clean up owners. Preflight never creates/migrates projects; writable opens
scan after migration. Idle TUI/watch/wait runs no periodic liveness scan.

Native Codex claims privately record the actual ancestor process and machine
identity. A missing/reused PID or zombie on the same machine proves death;
foreign or unavailable evidence stays unknown. Linux identity includes the PID
namespace and PID 1 boot/start fingerprint, avoiding recycled namespace matches;
older inode-only records remain unknown. Manual tokens alone identify no process.

Herdr absence requires a successful valid agent lookup on the saved server.
Missing registry entries, stopped-server reports and failed/malformed/timed-out
probes remain unknown. Probes time out after two seconds; claim/link/process
guards prevent a stale check from failing reassigned work.

### Return or retry work

```sh
qqq edit 12 --set-pending --session worker-1
qqq edit 12 --set-status error --reason "Missing credentials" --session worker-1
qqq edit 12 --set-status new          # retry an error; no owner needed
```

`--set-pending` equals `--set-status new`. Returning an active task or marking
it error requires its owner. Error marking requires a nonblank reason, records
it as a message, and clears assignment. These flags skip the editor.
Preflight-generated errors can also use `--reopen`.

Inspect abandoned work before forcing it back to the queue:

```sh
qqq show 12
qqq edit 12 --set-status new --force
```

`--force` skips ownership/Herdr discovery and requires literal `--set-status new`;
it conflicts with `--set-pending` and `--edit`. Return/retry preserves content,
dependencies, messages, images and the saved link.

## Config and aliases

Config lives at `~/.config/qqq/config.toml` under `HOME`.

```sh
qqq config --list
qqq config alias.ls 'list --watch'
qqq config --get alias.ls             # or: qqq config alias.ls
qqq config display.max-completed 10
qqq config tui.after_save_new open_new
qqq config herdr.initial-prompt '/goal Work through the queue'
qqq config --unset alias.ls
```

| Setting | Values / default |
| --- | --- |
| `display.max-completed` | Nonnegative integer; absent means unlimited; applies to human lists |
| `tui.after_save_new` | `open_saved` (default) or `open_new`; read at TUI startup |
| `herdr.initial-prompt` | Literal string prefilled when Ctrl+H opens agent creation; absent uses the built-in queue-worker prompt |
| `alias.NAME` | Command string |

Keys use TOML dotted syntax; quote literal dots, such as `'alias."with.dot"'`.
Reads create no files; `--get` can inspect invalid settings. Writes validate known
settings and preserve comments/unknown keys. Other values parse as TOML literals,
falling back to strings. JSON returns typed values; unset restores defaults.
Explicit `--max-completed` affects both human and JSON output.

`herdr.initial-prompt` is read each time the creation dialog opens. It supports
multiline text; an empty string opens a blank prompt that must be filled before
creating the agent. Unset the setting to restore the built-in prompt.

```toml
[display]
max-completed = 10

[tui]
after_save_new = "open_saved"

[alias]
ls = "list"
n = "next"
done = "edit --complete"
bug = "add"
```

`qqq bug "Fix login"` expands to `qqq add "Fix login"`. Aliases support quoting
and chains, with no shell expansion. Built-ins take precedence. Cycles, invalid
quoting and `!` aliases fail.

## Herdr

```sh
qqq herdr link 12
qqq herdr find 12
qqq herdr link 12 --agent codex --agent-session session-123 --server work
```

Herdr is optional. `link` stores a live-pane association without changing the
owner; `find` locates the saved identity on its saved server. Both require Herdr
CLI/server. Discovery uses the exact caller pane or unique project-root agent;
ambiguity fails before claim. Terminal-plus-agent identity survives missing hooks.
Local explicit-session claims do not auto-link.

Ctrl+H on a saved New task opens the editable agent-creation prompt. The dialog
uses `herdr.initial-prompt` when configured. `qqq next` claims work for its caller;
the removed `herdr.next-to-new-agent` setting has no effect.

## JSON error contract

JSON failures write one object plus newline to stderr. Stdout stays available
for success data. Runtime failures exit 1; argument-parser failures exit 2.

```sh
qqq --json show 999 >result.json 2>error.json
```

```json
{"code":"TASK_NOT_FOUND","message":"Task 999 not found","details":{"task_id":999,"command":"show"}}
```

Branch on stable `code` and relevant `details`; message wording can change.
`details` is always an object, may gain fields, and includes `command` when known.
Omitted fields mean unavailable; explicit null means an absent optional value.

| Code | Meaning / useful details |
| --- | --- |
| `TASK_NOT_FOUND` | Missing task; `task_id` or `task_reference` |
| `OWNERSHIP_MISMATCH` | Wrong active owner; `task_id`, `actual_status`, `expected_statuses` |
| `INVALID_TRANSITION` | Status/dependency/archive guard; `actual_status`, `expected_statuses`, `reason`, `blocking_child_id`, `actual_archived`, `expected_archived` |
| `INVALID_ARGUMENT` | Invalid flag/value, alias or owner input; `argument`, `reason`, sometimes `alias` |
| `INVALID_FILTER` | Bad Luau filter; `argument: "--filter"`, runtime `reason: "evaluation_failed"` |
| `DB_BUSY` | SQLite lock; `sqlite_code` (5/6), `sqlite_extended_code`; retry after competing transaction |
| `CONTENT_CONFLICT` | Stale/removed edited task; `task_id`, `expected_revision`, `current_revision`, `reason` |
| `BULK_CONFLICT` | Stale preview or batch write failure; `conflict_ids`, `reason`, sometimes `selected_ids`; stale previews use `stale_bulk_preview` |
| `PROJECT_NOT_FOUND` | No project database |
| `CONFIG_ERROR` | Config operation failed; `path` |
| `IO_ERROR` | Filesystem/I/O failure; `io_kind`, `os_code`, sometimes path/task/reason |
| `EDITOR_ERROR` | Missing/failed external editor; sometimes `exit_code` |
| `DISPATCH_ERROR` | Herdr command or lookup failure; `reason`, sometimes `exit_code` |
| `DATABASE_ERROR` | Other SQLite/schema failure; SQLite codes or `schema_version`, `expected_schema_version`, `reason` |
| `COMMAND_ERROR` | No narrower classification |

Error envelopes omit private ownership values, full task text, raw editor/Herdr
output and arbitrary SQLite trigger text. Human mode retains diagnostic context.
Success/diagnostic payloads keep their own identity/content fields.

For content conflicts, inspect current text before retrying with its reviewed
revision. `reason` is `revision_changed` or `task_removed`; removal has
`current_revision: null` and must not trigger automatic task recreation.
A task missing before the editor opens remains `TASK_NOT_FOUND`.

Help/version exit 0 with text, even under `--json`. An unhealthy `doctor --json`
prints its diagnostic report on stdout and exits 1 without an error envelope.
Watch may emit successful snapshots before a later error. Interactive sessions
use stderr for UI, then emit a final error after terminal restoration; use direct
fields or nonterminal stdin when parsing all stderr as JSON. Nonterminal JSON
external-editor calls suppress editor output/recovery prose. Expanded aliases
determine output mode; a literal `--json` consumed as data or after `--` does not.

## Data

Keep `.qqq/` and `.qqq-upgrades/` out of Git. Package and database schema versions
are separate. The fixture matrix and runtime checks enforce this support table:

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

Supported versions also need a valid `description`/`new` layout. Legacy `title`
columns or `pending` statuses need manual conversion regardless of user_version.
Schema 0 initializes an empty database; existing schema-0 projects do not upgrade.
Unknown newer schemas and archive formats are rejected.

### Portable backup

A backup captures consistent SQLite data and every stored image while other
local qqq writers run. Destinations must not already exist.

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

Archives contain `manifest.json`, `qqq.db` and images. The manifest records SHA-256
hashes and sizes. Restore validates data, images, integrity and foreign keys
before installing `.qqq`; it requires a new project directory or empty `.qqq`.
Existing/nested projects and invalid archives are rejected without replacement.
Relative archive paths resolve from the current directory.

### Automatic upgrades and recovery

Before upgrading a supported old schema, qqq saves a verified original-schema
archive in sibling `.qqq-upgrades/`. Human stderr reports its path; JSON shapes
stay unchanged. Names are `schema-<source>-to-<target>-.upgrade-<unique>.tar`.
The manifest records source/target schema, original project path, time, hashes
and sizes. Snapshot failure aborts with the source intact. Successful and failed
migrations retain the archive; retries never overwrite it. Concurrent upgrades
coordinate under a write lock and produce one archive per successful upgrade.
Fresh/current-schema opens, diagnostics and dry runs create none.

Stop workers before recovery. Use the printed absolute path or archive inventory.
This example starts in the original project and creates a fresh target; it needs
`sqlite3` for inspection before another automatic upgrade:

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

`restore --recovery` accepts upgrade archives and preserves their original schema,
including embedded-image schemas 1–5. It verifies manifest/project metadata,
hashes, integrity, foreign keys, claims and attachments before installation.
Pending deletion images restore to canonical paths. Ordinary restore accepts
only portable formats/schemas from the support table.

Inspect with SQLite before `list`, `show`, backup or TUI: normal database opens
upgrade old schemas. Recovery never overwrites the original project. Retain the
original and archive until recovered data and a subsequent upgrade are verified;
archives have no automatic cleanup.

Upgrades preserve IDs/reserved sequences, text, timestamps, graph, history,
images and active claim keys. Older records gain these defaults:

| Schema | Addition / older-data default |
| --- | --- |
| 2 | Parent: null |
| 3, 5 | Identity: retain owner key; derive active harness/orchestrator metadata from saved claim/link |
| 4 | Error status support; existing statuses/history unchanged |
| 6 | Move embedded image bytes to `.qqq/images/` |
| 7 | Priority: `0` |
| 8 | Archived: `false` |
| 9 | Reopen events; existing history unchanged |
| 10 | Content revision: `1` |
| 11 | Extra prerequisites: empty |
| 12 | Tags: `[]` |
| 13 | Private claim process identity: empty; legacy Herdr links remain usable |

If migration's compaction warns, stop writers and run
`sqlite3 .qqq/qqq.db 'VACUUM;'` later. Portable restore also preserves stored
schema; the next normal open upgrades it.

For shared projects, stop old workers/waiters, upgrade all binaries, run
`qqq list` then `qqq doctor`, and restart workers. Older binaries reject newer
schemas. Migration never expires/transfers/releases a claim; the owner must
complete or return it explicitly.

### Permanent deletion

```sh
qqq backup ../before-delete.tar
qqq delete 42                       # preview
qqq delete 42 --yes                  # permanently apply
```

Only archived tasks without children or incoming prerequisite references can be
deleted. Use a positive absolute ID. Release/complete active work before archive;
reparent/delete children and remove references first.

Preview reports affected messages, events, links, images and paths without
changing them (`deleted: false` in JSON). `--yes` removes them permanently and
returns `deleted: true`; IDs stay reserved. Legacy schemas/pending recovery
block preview until a normal command migrates/recovers the project. Interrupted
image staging recovers according to committed database state on the next normal
open. Keep a backup until `doctor` passes.

### Health checks

```sh
qqq doctor
qqq --json doctor
```

Doctor checks SQLite integrity/foreign keys/schema, dependency cycles/duplicate
edges/missing references, image paths/sizes/signatures and orphan files. Healthy
projects exit 0; issues include recovery actions and exit 1. JSON has `ok`, counts
and `issues` containing code, path, message and action. Doctor does not migrate.

WAL/sidecars defer inspection (`DB_READ_ONLY_UNSAFE`), including persisted WAL
mode without sidecars. Stop writers, recover/checkpoint SQLite and switch to
`journal_mode=DELETE` before retrying. For `DELETE_RECOVERY_PENDING`, fix reported
paths/access problems, run a normal command such as `list` to recover, then
rerun doctor. Restore damaged data into a new directory and verify it before
replacing the live project; keep the damaged copy until recovery succeeds.

### Legacy root-level database

The CLI no longer discovers root-level `qqq.db`. Stop writers and verify
`.qqq/qqq.db` is absent before copying; `qqq init` would create a separate empty DB.

```sh
mkdir -p .qqq
sqlite3 qqq.db ".backup '.qqq/qqq.db'"
qqq list
```

Check the migrated data before removing the old database.

## Development

Tests need Rust, Python 3 for PTY coverage, a POSIX shell and `sqlite3`.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
./scripts/check-compatibility-docs.sh
./scripts/check-compatibility.sh
cargo build --locked --release
```

[CI](../.github/workflows/ci.yml) checks Ubuntu 24.04/macOS 14, builds release
binaries and retains `qqq-<OS>-<ARCH>` archives for 14 days.

[Historical fixtures](../tests/fixtures/compatibility/README.md) cover every
supported database/snapshot schema. Compatibility checks verify upgrades,
restores, rollback/retry, images, identities, dependencies, defaults and reserved
IDs. Documentation checks compare the marked support table with fixtures/runtime
and execute recovery examples in temporary historical projects. New schemas or
snapshot formats require matching fixtures and published facts; tests use
checked-in artifacts, not regenerated current databases.

### Publish to crates.io

Follow the [release checklist](release-checklist.md). The
[publish workflow](../.github/workflows/publish.yml) verifies a `v<version>` tag
against `Cargo.toml`, runs checks/package dry-run and publishes `qqq-cli`.

1. Verify the crates.io account email and create a token allowed to publish `qqq-cli`.
2. Store it as the repository's `CARGO_REGISTRY_TOKEN` Actions secret.
3. Choose an unused version, update `Cargo.toml`/`Cargo.lock`, and commit.
4. Push the commit and matching tag, for example:

```sh
git push origin master
git tag v0.10.0
git push origin v0.10.0
```

The manual workflow accepts an existing `tag`; `dry_run=true` validates and
`dry_run=false` publishes. Published versions cannot be overwritten.

License: [MIT](../LICENSE).
