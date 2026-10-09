# Parent references in child drafts

Task #194: pressing Ctrl-P inside an existing dashboard child draft inserts its
parent reference at the editor caret. Ctrl-P on an existing task still opens or
restores that task's child draft. General new drafts retain the parent-selection
error. Filter input and popups cannot insert a reference into the editor.

Reference display is `[#99]`, matching existing bracketed editor tokens. Storage
is plain `#99`, with no added spaces, dependency mutation, DB lookup, or schema
change. The child parent/dependency continues to come from draft parent context.
Each press inserts one atom and one undo record. Arrow keys cross the atom;
Backspace/Delete/Ctrl-W remove it as one unit. Save adoption and retained draft
buffers preserve reference atoms and undo history.

Saved descriptions recognize standalone canonical positive i64 references as
chips, including manually typed references. A reference must have no leading
zero, fit i64, and have no adjacent alphanumeric, underscore, hash, or preceding
backslash. Stored bytes remain exact. Pasteboard payloads and image tokens are
already separate atoms and are excluded from reference parsing. This is display
and editing only; unresolved task IDs are allowed.

Use the existing token layout and highlighting paths, with brackets visible in
NO_COLOR mode and narrow terminals. Keep stored CLI/JSON description plain.
Add a child-specific footer hint for Ctrl-P's current action.

Verify real terminal behavior before DB assertions: caret insertion, repeated
presses, undo/redo, atomic deletion, child buffers, failed save, successful save
and fresh reload, filter/popup scope, and color/plain output. Model tests cover
canonical boundaries, exact bytes, Unicode, image/paste coexistence, and history.
