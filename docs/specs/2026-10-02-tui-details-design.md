# TUI Details Pane

Task #89 requests three horizontal regions. Task list always occupies top third.
New drafts use remaining two thirds as editor. Selecting task exposes details and
messages in middle third; editor occupies bottom third.

## Layout

For terminal height `h >= 12`, list height is `h / 3`. Selected-task details
height is also `h / 3`; editor gets remainder. Integer rounding gives editor at
most two extra rows. New drafts have no details pane.

Keep existing 12x8 minimum. At heights 8 through 11, reserve four rows for list,
three for editor; selected-task details use available remainder. Resize hint
handles smaller terminals. List, details, editor hit tests use same rectangles.

## Content and State

Details start with task ID, status, priority, parent. Latest messages follow,
including author and timestamp. Show explicit empty-message state. Created,
updated, archive and public harness/orchestrator fields follow messages.
Description remains in editor. Terminal controls are escaped; Unicode wraps by
display cells.

Details have independent scroll offset. PgUp/PgDn scroll by visible page when
task selected, outside filter/action/confirmation modes. Mouse wheel scrolls pane
under pointer. Details clicks never move editor caret or mutate draft.

Selection changes reset details scroll; same-task external commits refresh
metadata/messages without replacing dirty editor content. Returning to new draft
hides details immediately. Deleted selected task shows unavailable state while
preserving editor draft. Query/read failures retain existing DB-error behavior.

## Implementation Boundaries

- `src/tui/dashboard.rs`: shared pane geometry, rendering and hit tests.
- `src/tui/details.rs`: escaped/wrapped details rows, no DB access.
- `src/db.rs`: typed message reads, existing `show` output preserved.
- `src/tui/mod.rs`: selected details snapshot, scroll state and input routing.
- Tests: geometry, real terminal workflows, refresh, empty/deleted states,
  keyboard/mouse behavior, resize, Unicode and plain-color mode.

Existing terminal fixtures should locate editor by visible heading instead of
assuming fixed row. Keep navigation, dirty-draft confirmations, child-task
creation, filtering, attachment handling and task actions covered.
