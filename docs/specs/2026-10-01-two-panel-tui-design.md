# Two-Panel Task TUI

## Command and layout

`qqq tui` opens a terminal-only, alternate-screen view. Upper panel shows task
tree with IDs, statuses, dependencies, and descriptions in `qqq list` style.
It includes all tasks so every Shift-Up/Down target remains visible. Lower
panel is continuous task editor from `qqq add`. Panel split gives each half
usable height; narrow or short terminals show resize prompt. Terminal resize
redraws both panels without changing draft or selection.

Upper panel has its own scroll position. New-task draft shows latest tasks;
selecting existing task scrolls its row into view and marks it. Long lists
cannot displace editor. List rows come from shared task-list formatting, so
CLI list and TUI agree on order, status names, dependencies, and escaping.
Upper panel refreshes after saves, navigation, and redraw; no background DB
watch is required.

## Editing

Shift-Up selects newest task, then older IDs; Shift-Down moves toward newer
tasks, then blank new-task draft. Selection loads full description into lower
editor. Unsaved text or pasted images trigger existing switch confirmation.
Ctrl-S updates selected task or creates task from new draft, commits it, then
clears draft, images, cursor, and selection for next task. Invalid or failed
save keeps draft. Esc on nonempty draft confirms discard; Ctrl-C exits.

`qqq tui` differs from `qqq add`: exiting before first save succeeds because
viewing task list is valid use. Terminal UI writes to stderr; stdout stays
empty, including with global `--json`. Saves commit on Ctrl-S; no result is
printed on exit. Existing `qqq add` behavior stays unchanged.

## Verification

PTY tests cover upper list, lower editor, selection marker and scrolling,
editing existing task, two consecutive new saves, list refresh, dirty-switch
confirmation, resize, empty exit, silent stdout, and terminal restoration.
Unit tests cover panel geometry and clipping where PTY assertions cannot
isolate layout. Existing editor and list tests remain green.
