# Ctrl+L Task ID Jump

Ctrl+L opens a centered task ID input popup in the dashboard, including while
filtering or editing a new/child draft. Existing confirmations, action menus and
content-conflict dialogs keep their current keyboard handling.

Enter accepts a positive `i64` task ID after trimming whitespace. Empty, zero,
negative, nonnumeric and overflowing values produce an inline error. Missing
tasks and snapshot/image loading failures keep the popup open with the input
intact. Backspace edits one grapheme; Ctrl+U clears input. Terminal paste replaces
control characters with spaces so separate pasted IDs cannot concatenate.

Successful navigation parks the current dirty draft, restores the destination's
retained draft or loads its current content snapshot, clears the list filter,
focuses the editor and follows the selected row. Caret, baseline, pending images,
child context and manual editor scroll remain part of retained drafts. Jumping
to an archived task enables archived rows for the rest of this dashboard session.
Jumping to a completed task enables completed rows when hidden. Other jumps and
cancellation preserve the completed visibility preference.
Navigation writes no task content, status or ownership metadata.

Esc or Ctrl+C closes only the jump popup. Draft, selection, filter, focus and
scroll state remain intact. Mouse input cannot reach the dashboard behind it.
The popup reuses existing typed popup rows, color gates, clipping and cursor
placement. Normal, filtered and Herdr shortcut bars advertise Ctrl+L.

Tests cover validation, editing/paste, typed popup roles, existing/missing IDs,
cancel from filtered and dirty states, retained task/new/child drafts, offscreen
and archived targets, compact terminals and NO_COLOR. Full suite, formatting,
Clippy, read-only review and installed-binary terminal checks gate completion.
