# Completed Filter Toggle (#158)

Add a session-local completed-task visibility control at the right end of the TUI filter bar. Start checked (`[✓ Completed]`): current completed-task visibility stays enabled. Clicking the control toggles to `[× Completed]` and removes every completed task from the list and Shift-Up/Down navigation. Query matching still uses full saved descriptions. Noncompleted children of hidden completed parents remain visible as roots. Selected editor contents, cursor, draft buffers, task details and DB stay intact.

Preserve existing empty-filter collapse behavior. Show the bar while query is nonempty, filter is focused, or completed tasks are hidden, so an active visibility filter always has an accessible control. Ctrl+/ focuses query; Ctrl-T while filter is focused toggles completed visibility. Existing Tab/Enter, Escape and Ctrl-C behavior remains.

Place the button against the list pane's right edge, including wide split layouts. Reserve its cells before clipping query text; query caret never enters the button. Below 24 list-pane columns use compact `[✓]` / `[×]`; shorten filter label only when necessary to retain query text and caret. Reuse existing body/accent styles; tick/cross communicates state in plain output too. Mouse hit region must match rendered cells and ignore modal overlays.

Validate full/compact rendering, long Unicode query and caret boundaries, exact button hit region, status/query interaction, missing parent context, visible navigation, draft preservation, external status refresh, resizing and color/plain terminal behavior. No config, schema or CLI changes.
