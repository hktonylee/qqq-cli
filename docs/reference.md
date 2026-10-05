# qqq reference

[← README](../README.md)

Detailed behavior and flags for qqq. For a first run, start with the
[quick start](../README.md#try-it).

- [Install from source](#install-from-source)
- [List and search](#list-and-show) · [Add and edit](#add-and-edit)
- [Archive](#archive-and-unarchive) · [Reopen](#reopen-completed-work)
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

## Reopen completed work

```sh
qqq reopen 12
qqq reopen -1 --session reviewer
```

`reopen` returns a completed task to `new` and records a reopen event. It keeps
description, priority, parent, prerequisites, messages, images, creation time, and prior
history. A task in any other status fails without changes; repeating `reopen`
also fails. Archived completed tasks require `unarchive` first. A completed
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
values; Created/Updated occupy separate rows. Messages follow creation order,
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
Ctrl-C in dirty new draft asks before exit. After active new draft is approved,
Ctrl-C or Esc exit prompts for every retained dirty draft, including hidden,
archived or deleted tasks and child drafts. Prompts identify draft and preview it;
`y` advances, `n`, Enter or Esc cancels whole exit and preserves all drafts,
including earlier approvals. App exits after every draft is approved. Blank draft
exits immediately only when no retained dirty drafts remain.
Cancelling a deleted task's discard prompt reopens its retained local draft;
Ctrl-S explains removal and keeps draft without recreating task.
Repeated Ctrl-C keeps confirmation open. Ctrl-V or terminal paste inserts text; pasting
image file path attaches image. Saves commit immediately. TUI needs terminal
and writes no stdout, including with `--json`.

Ctrl+P opens or restores child draft under selected task; header shows parent ID.
Current edits stay retained. Each parent has separate child draft; return to parent
and press Ctrl+P to continue it. Ctrl-S creates child with dependency;
Enter or Shift+Enter inserts newline. Navigating away retains child parent context.

Ctrl+L opens tags for the selected saved task from editor or filter. Current
labels are prefilled, one per line. Shift+Enter adds a line; Enter applies.
Blank lines are ignored; blank input clears tags. Newline/CRLF paste and legacy
comma-separated input are accepted. Ctrl-U clears input, Esc/Ctrl-C cancels.
Shortcuts share the popup footer; narrow popups use `^U` for Ctrl-U, `S-↵` for
Shift+Enter and `↵` for Enter. Final input line stays visible in short terminals.
List tags use yellow when color is enabled. Invalid labels stay in the popup. Tag saves
retain unsaved description drafts, caret, scroll, parked buffers and filter
focus. New unsaved tasks show “Select task to edit tags”; save first to tag.

Ctrl+K opens a task ID popup from editor or filter. Enter a positive ID, press
Enter to go; invalid or missing IDs keep popup open with an error. Backspace edits
input; Ctrl+U clears it. Esc or Ctrl+C closes popup and preserves selection,
draft, filter and focus. Successful jump clears filter and retains unsaved
task/new/child drafts, including caret and editor scroll. Jumping to archived
task enables archived rows for remaining dashboard session. Jumping to completed
task reveals completed rows when hidden; other jumps and cancellation preserve
that setting. Navigation saves no content and changes no task status or ownership.

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
returns list to selected task. Editor keys reveal caret after manual scroll.
Left-click task row to load it in editor, including indented or wrapped rows.
Left-click editor text to place caret. Task-row clicks retain unsaved drafts,
using same restoration behavior as Shift-Up/Down.

Ctrl+/ focuses filter and shows `Filter:` with editable caret, even when empty.
Filter bar stays visible while focused, query is nonempty, or completed tasks are
hidden. Empty unfocused query reserves no row when completed tasks are shown.
Right-edge `[✓ Completed]` button includes completed tasks; click to switch to
`[× Completed]` and hide them. Ctrl-T toggles the same state while filter is
focused. Narrow list panes show compact `[✓]` / `[×]`. Visibility resets when TUI
opens again. Toggle preserves query, opened task, unsaved drafts and DB; navigation
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
editor clearing flow; later Ctrl-C follows editor cancel flow. Dirty drafts ask
before discard.

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
`r` to retry error task, `o` to reopen completed task, `a` to archive or
unarchive, `p` to set priority (-100..100), or `d` to set parent by positive
task ID or `none`. Retry appears only for error tasks; non-error tasks ignore `r`.
Press `e` for Mark error, enter a nonempty reason, then confirm with `y`.
Only current owner of an in-progress task can mark it error; other states or
owners show an action error and keep draft. Reason is recorded as a task message
with an error event; claim and assignment clear while saved content stays intact.
Error tasks stay out of worker queue until explicitly retried. Reason prompt and
confirmation cancel without changing task. Confirmation previews reason and asks
before discarding an active dirty draft, matching other state actions.
Arrows skip hidden actions; Enter activates displayed selection.
Open menu refreshes Retry availability after external status changes, keeping
selected action when available.
Complete checks current session ownership when activated. Without a matching
claim, popup asks `Force complete task #ID?` and warns
`Complete without matching task owner.` Confirm with `y` to finish a New,
In progress or Error task regardless of owner. This clears claim and ownership
metadata, preserves task content and history, and records a standard `complete`
event attributed to explicit session or `manual`. Already-completed tasks fail.
Cancellation preserves draft and DB. An owned task whose claim changes while
ordinary confirmation is open fails; open menu again to request forced completion.
After filter clears, Esc closes menu or prompt. State changes ask for `y` before
running; priority and parent changes ask when draft has unsaved edits. Successful
action refreshes task and list; rejected action keeps draft and shows DB error.
Error view wraps long messages; Up/Down scrolls, Esc closes it.

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

```sh
qqq status
qqq status --include-archived --json
qqq next --explain
qqq next --explain --filter 'priority >= 5' --json
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
| `matches_filter` | Filter result, independent of readiness; true without a filter |
| `queue_rank` | One-based rank among all ready tasks by priority/ID; null otherwise |
| `reasons` | Zero or more eligibility/exclusion codes |
| `owner` | Recorded claim key for an active task; null otherwise |
| `blockers` | Unfinished/missing parent and extra prerequisites: ID, status and archived flag; missing refs have null status/flag |
| `latest_activity` | Latest task update, message or event: `at`, `source`, `id`, `session`, `action` |

Reason codes: `archived`, `parent_not_completed`, `prerequisite_not_completed`, `in_progress`, `error`,
`completed`, `filter_excluded`. A ready matching task has no reasons.
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

Without a queued candidate, human preview prints `No task available for pickup.`
Use `qqq status` to inspect blocked, active, and error tasks.
JSON preview returns `null` when no candidate is available.

```sh
qqq next --dry-run --json
```

`next --filter 'priority >= 5'` selects matching new candidates using same
priority/readiness rules. Without `--dry-run`, existing owned task still returns
regardless of filter.
Combine with `--wait` or Herdr dispatch; no matching ready candidate yields no claim.

Without `--session` or `QQQ_SESSION`, owner discovery uses:

1. Exact Herdr pane from `HERDR_PANE_ID` or `HERDR_ENV=1` (requires pane ID).
2. Native Codex `CODEX_THREAD_ID`, then `CODEX_SESSION_ID` fallback.
3. Unique Herdr agent at project root (`foreground_cwd` before `cwd`).

Explicit session IDs and native Codex ownership work without Herdr when dispatch is
disabled or `next --local` is used. Exact Herdr context takes precedence.
`next --local` skips dispatch, still resolves owner.

For automatic Codex claims, displayed `harness_session` uses `CODEX_SESSION_ID`
when set, including inside Herdr. Without it, resolved owner ID supplies display.
Claim keys and Herdr links keep resolved identity; `--harness-session` overrides
display. Repeated claims retain saved metadata unless explicit assignment flags
override it.

Completion, release and error marking require owner. Use original session token
or uniquely matching displayed `harness_session`; add `--harness-name` when
public sessions overlap. JSON assignment fields: `harness_name`,
`harness_session`, `orchestrator_name`, `orchestrator_session`.

Human `next` output omits these four assignment fields. JSON and `show` retain
them for ownership inspection.

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

Startup failure releases dispatch reservation; created tabs stay open. Prompt
errors keep claim and link because delivery may have happened. Inspect agent
before retry or forced release.

## Data

Keep `.qqq/` out of Git. Create portable snapshot while other local qqq
writers run:

```sh
qqq backup ../project-snapshot.tar
mkdir ../restored-project
cd ../restored-project
qqq restore ../project-snapshot.tar
qqq list
```

Backup contains consistent SQLite data plus every stored image. Tar archive
starts with `manifest.json`, then `qqq.db`, then `images/<task-id>/<image-id>.<ext>`.
Manifest records SHA-256 hashes and sizes; restore checks those, image rows,
SQLite integrity, and foreign keys before installing `.qqq`. Backup refuses to
overwrite destination. Restore works only from new project directory or one
with empty `.qqq`; existing data and nested projects are rejected. Invalid
archives leave project unchanged. Relative snapshot paths resolve from current
directory.

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

Compatible DBs at schema versions 1–11 migrate to version 12. Version 12 adds
tags; version 11 added extra prerequisite edges; version 10 added content
revisions. Upgrades preserve old IDs, ownership, history and readiness. Portable
snapshot format 1 accepts schema-9–12 DBs: restore preserves stored schema,
then next normal DB open migrates older versions. Version 6 moves
existing image blobs to `.qqq/images/` before SQLite drops its `data` column.
Migration tries `VACUUM` to reclaim old blob pages. If compaction warns, stop
writers and run `sqlite3 .qqq/qqq.db 'VACUUM;'` later. Upgrade other qqq workers
before migration; older binaries cannot open version 12. Legacy `title` or
`pending` schemas need manual conversion; newer unknown schemas fail. Back up
before conversion.

## Development

Tests require Python 3 for real-terminal coverage on macOS and Linux.
[Historical compatibility fixtures](../tests/fixtures/compatibility/README.md)
cover every normal-open DB schema and accepted portable snapshot schema. Tests
copy checked-in artifacts; regeneration uses frozen historical sources.
Snapshot validation rejects legacy title/pending layouts and invalid status/claim
relationships before installing DB/images. Human and JSON failures preserve
original projects. Diagnostics and import dry-run never migrate; next dry-run
uses normal DB-open upgrades, then previews queued candidate without claiming.

```sh
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
coverage for a new schema or snapshot format fails gate.

### Publish to crates.io

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
