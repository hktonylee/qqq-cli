# Ctrl-G actions by task state

Review proposal for #198. Based on master `7586966`, qqq 0.7.0.
Document only; runtime menu behavior unchanged.

## Recommendation

Hide actions invalid for current status. Keep existing explicit force paths.
Keep useful actions visible when temporary blockers exist; show blocker reason.
Status, archived flag, ownership, dependencies are separate inputs.

Current menu always shows Complete, Mark error, Reopen, Priority, Set parent,
Archive/Unarchive. Only Retry is status-filtered. Result: Completed still offers
Complete, Error still offers Mark error, New still offers Reopen, In progress
still offers Archive. These operations reject at DB layer.

## Single-task menu

Applies when saved task selected, no bulk marks. Existing shortcut letters stay.
`Force only` means normal `y` cannot apply; explicit uppercase `Y` required.
Opening menu, selecting row, pressing Enter never silently chooses force.

| Row | New | In progress | Error | Completed |
| --- | --- | --- | --- | --- |
| `c Complete` | Show; force only | Show; normal owner or explicit force | Show; force only | Hide |
| `e Mark error` | Show; force only | Show; normal owner or explicit force | Hide | Show; force only |
| `r Retry` | Hide | Hide | Show; normal, session-free | Hide |
| `o Reopen` | Hide | Show; verified orphan recovery or explicit force | Hide; Retry covers return to New | Show; normal |
| `p Priority` | Show | Show | Show | Show |
| `d Set parent` | Show | Show | Show | Show |
| `a Archive` | Show, subject to blockers | Hide; active tasks cannot archive | Show, subject to blockers | Show |

Suggested lifecycle order: New `c,e`; In progress `c,e,o`; Error `r,c`;
Completed `o,e`. Then metadata `p,d`; finally Archive/Unarchive. Error opens on
Retry; Completed opens on Reopen. Archived task opens on Unarchive, regardless
of status. Keep blank separator only between nonempty
groups. No new actions, status values, or shortcuts.

Error menu deliberately uses one return-to-New action. Current Reopen also has
special handling for auto-failed owners and explicit force on Error; those paths
remain available through CLI. Retry works for any Error and does not require
owner discovery. Unlike Reopen, Retry does not validate dependency availability;
task may return to New while blocked. This distinction is explicit review choice.
Alternative: retain `o Reopen` on Error alongside Retry if both audit/recovery
paths need menu access. Recommended default: Retry only.

## Context modifiers

| Context | Menu behavior |
| --- | --- |
| Archived task | Replace `a Archive` with `a Unarchive`. Keep status matrix for other rows. Disable Reopen with `Unarchive first`. |
| Archived Error + Retry | Keep Retry available; confirmation says `Returns to New; stays archived`. Retry does not unarchive or make task ready. |
| Unfinished task has visible unfinished child/dependent | Archive remains visible, disabled: `Finish or archive dependent #ID first`. |
| Unarchive/Reopen blocked by missing or archived unfinished parent/prerequisite | Keep row visible, disabled when blocker is known; show task ID and fix needed. |
| Ownership belongs to current session | Normal Complete/Mark error available only for In progress. |
| Foreign or unknown owner | Keep applicable Complete/Mark error/Reopen rows. Resolve/check on activation; explicit `Y` force stays available. Unknown never means owner dead. |
| Unsaved general/child draft, no bulk marks | No task action menu: keep `Select task for actions`. Ctrl-L edits draft tags; Ctrl-P inserts parent ref in child editor. |
| Task removed while menu open | Close stale menu, show `Task removed; draft kept`; no fallback mutation. |

Unfinished unarchived prerequisites may block claiming without blocking
Unarchive/Reopen. Those actions require available dependencies, not completed
dependencies. Archived completed prerequisites remain valid.

Disabled row stays focusable so reason can be read. Enter/letter shows reason,
opens no mutation confirmation. Hidden row has no active shortcut. Plain mode
prints `Unavailable: reason`; color alone never signals availability.

## Confirmation and draft behavior

Keep current `y` normal / `Y` explicit force flows for Complete, Mark error,
Reopen. Force never overrides archived Reopen guard, dependency availability,
input validation, or unrelated DB errors. Already-Completed Complete and
already-Error Mark error remain invalid even with force.

Mark error requires nonempty trimmed reason. Force-only contexts show this
before reason entry and again in confirmation. In progress Reopen checks
orphaned ownership when `y` chosen; `Y` bypasses owner check explicitly.

Menu opening/navigation never saves or discards draft. Existing dirty-draft
confirmation stays before actions that reload active editor. Cancel/rejection
keeps text, tags, caret, scroll, retained buffers and undo history. Successful
single-task action retains existing draft reload policy; this proposal changes
availability, not save behavior.

Refresh open menu after committed status/archive changes. Retain selected action
by action identity; if removed, select nearest surviving row. Revalidate latest
DB state at activation/commit; stale menu never authorizes invalid transition.
Use cached local state for menu display; no new watchdog, per-frame process scan,
or owner discovery. Unknown external guards are checked by existing action path.

## Bulk menu

Any Ctrl-D marks -> Ctrl-G still opens bulk menu, including from unsaved draft.
Keep current Add tags, Remove tags, Replace tags, Priority, Archive, Unarchive,
Clear selection. Do not introduce bulk Complete, Mark error, Retry or Reopen.

For mixed states, retain whole-selection validation before preview. First blocker
rejects preview; TUI shows error popup. Valid selection gets preview and
whole-batch atomic apply. Archiving an unarchived In progress task rejects batch.
Existing no-op archive/unarchive rows remain represented in valid preview.
Blocker failure keeps selection, active draft and parked drafts intact.
No silent partial apply or new per-task blocker report.

## Examples

```text
Task actions #42 · Error
> r Retry
  c Complete       Y force only

  p Priority
  d Set parent

  a Archive
```

```text
Task actions #42 · Completed · Archived
  o Reopen         Unavailable: unarchive first
  e Mark error     Y force only

  p Priority
  d Set parent

> a Unarchive
```

Narrow terminals wrap selected row reason, retain selected action and cancel
hint. Status/archived text in heading may clip before action labels do.

## Acceptance checks for later implementation

Verify four statuses × archived flag; owner/foreign/unknown cases; live/orphan
Reopen; blockers, archived completed dependencies, missing tasks. Check hidden
shortcuts, focusable disabled reasons, direct keys and Enter equivalence, live
menu refresh, minimum-size terminal and NO_COLOR. Verify dirty cancel/failure
preserves draft/caret/history; force requires uppercase Y; bulk all-or-nothing
behavior stays intact. These are future checks, not tests added for this doc.

## Current source evidence

- [Menu groups, filtering and input dispatch](../../src/tui/mod.rs):
  `ACTION_MENU_GROUPS`, `action_menu_items`, `ActionUi::Menu`.
- [Action callbacks](../../src/main.rs): `TaskAction::Complete`, `Retry`,
  `Reopen`, force and metadata branches.
- [Status/ownership/retry/reopen guards](../../src/db.rs): `complete`,
  `force_complete`, `force_error`, `reopen_inner`, `EditTransition::RetryError`.
- [Archive guards](../../src/archive.rs): `validate`.
- [Dependency availability](../../src/dependencies.rs): `ensure_all_available`.
- [Bulk menu](../../src/tui/bulk.rs), [bulk transactions](../../src/bulk.rs).
- [Existing user contracts](../reference.md#task-tui): current force
  confirmations, drafts, filters, task actions.

Review focus: status matrix, force-only rows on New/Completed, Retry-only Error
menu, focusable blocked rows. Implementation remains separate from this doc.
