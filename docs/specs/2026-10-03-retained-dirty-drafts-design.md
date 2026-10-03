# Retained Dirty Drafts

Task #144 extends staged cancellation from #143 with session-local draft retention.

## Behavior

- Shift-Up/Down and task-row clicks switch immediately, retaining dirty drafts.
- Returning restores text, image/paste atoms, caret, editor scroll, and original
  saved-content baseline. Task status still refreshes from DB.
- Ctrl-P retains current task draft and opens/restores child draft for that parent.
- General new draft restores when navigation reaches new-task view. Child drafts
  restore through Ctrl-P on the same parent. Distinct parents have distinct drafts.
- Ctrl-S saves only active draft. Successful save clears its dirty marker; failed
  save keeps draft and marker. Reverting edits to saved contents clears marker.
- Explicit active discard keeps existing confirmations and discards only active
  draft. Task actions keep existing dirty-draft confirmation/error behavior.
- Retention lasts for current TUI process. Navigation/markers do not write DB.

## Dirty Indicators

Existing task rows show `[*]` while their active or retained draft differs from
saved baseline, including pending images. Gold marker uses explicit dark backing;
plain mode retains text marker. Marker column appears when a displayed task is
dirty; reserve four columns across displayed rows before formatting/wrapping so
task text, parent indentation, preview limits and click mapping remain correct.
Wrapped rows share owning task's dirty state. Literal `[*]` in task text receives
normal text styling. New/child draft title shows `[*]` while active and dirty.

Filtering and task descriptions in list continue to use stored DB content.
Hidden/archived/deleted task drafts remain retained and participate in exit checks.

## Cancellation and Exit

One Ctrl-C press handles one stage:

1. Clear nonempty query, keeping filter focus.
2. Close empty focused filter, keeping active task/draft.
3. Close opened task, confirming discard if active draft is dirty. Load retained
   general new draft when present.
4. In new-task view, confirm active dirty draft before exit.
5. Before exiting, prompt for every retained dirty draft in deterministic order.

Background prompts identify task ID or new/child context and preview draft.
`y` approves current draft and advances; `n`, Enter or Esc cancels whole exit.
Approval does not remove drafts while prompts remain. Cancelling any prompt keeps
all drafts, including previously approved drafts and active new draft. Repeated
Ctrl-C keeps current prompt open. Exit occurs after every prompt is approved.
Esc retains its editor-clearing behavior, then applies background checks before
app exit so another exit key cannot silently lose retained drafts.

## State and Boundaries

`src/tui/buffers.rs` owns a typed ordered map keyed by existing task ID or
new-draft parent context. Each parked draft owns `Draft`, baseline, scroll offset,
and cursor-follow mode. Move draft atoms into map; preserve image data without
copying/re-serializing it. Only dirty drafts are parked.

Active draft stays in existing compose-loop state. A retained-target variant
allows `load_target` to restore parked state while retaining fresh task status.
Restoration removes entry from map, preventing duplicate active/background state.
Navigation parks after target validation succeeds. Failed target load leaves
active draft and parked drafts unchanged.

Exit confirmation stores ordered keys and current prompt position. It only tracks
approvals; drafts remain untouched until process exits. Cache remains independent
of DB schema and CLI output. Single-task editor and non-dashboard continuous add
keep existing behavior.

## Verification

- Model tests: task/new/child isolation, original baseline, caret/scroll restoration,
  deterministic ordering, reverted clean drafts, image/paste preservation.
- Render tests: gold/plain marker, wrapped ownership, clean rows, selected tint,
  literal marker text, width reservation and mouse row mapping.
- PTY tests: multiple dirty tasks, keyboard/mouse/child navigation, independent
  saves, new/child restoration, active discard, sequential exit prompts, declined
  later prompt preserving earlier edits, repeated Ctrl-C, empty-filter priority,
  deleted background task, NO_COLOR, compact/wide resizing and terminal cleanup.
- Update old dashboard navigation tests to retention contract while retaining
  explicit-discard/action coverage; single-editor dirty-switch checks remain.
- Run full suite, formatting, Clippy, read-only review, installed CLI scenarios.
