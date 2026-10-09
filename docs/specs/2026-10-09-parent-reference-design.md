# Parent references in child drafts

Task #194: Ctrl-P inside an existing dashboard child draft inserts plain `#ID`
for its parent at editor caret. No spaces are added. Ctrl-P on an existing task
continues to open/restore its child draft. General new drafts keep parent-selection
error; filter input and popups cannot insert into editor.

User clarification: plain text only. No chips, new atom types, highlighting,
reference parsing, DB lookup, or schema changes. Draft.insert supplies one undo
record for insertion. Arrow keys and deletion operate on normal text characters.
Child parent/dependency continues to come from existing draft parent context.

Retained draft buffers and same-task save preserve existing undo history. Reload
uses ordinary plain description text. Show Ctrl-P Parent Ref in child-draft hint.
Verify caret placement, repeat insertion, undo/redo, normal text deletion,
draft retention, save/failure, reload, input scope, color/plain terminal output.
