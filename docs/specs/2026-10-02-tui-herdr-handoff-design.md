# TUI Herdr Handoff

Task #102: Ctrl-H focuses selected task's agent, opens Herdr client.

Resolve stored task link with existing `herdr::find`, using exact agent/session
identity and stored server. Never trust cached pane ID: agents can move. Focus
resolved pane with `herdr --session <server> agent focus <pane>` before attaching
`herdr --session <server>`.

Drop TUI terminal guard before client starts. Inherit terminal stdin/stderr,
route client stdout to terminal stderr so `qqq --json tui` stdout stays clean.
On client exit, re-enter terminal modes, clear Ratatui buffer, redraw unchanged
draft, selection, filter, cursor and scroll state. Neither focus nor attachment
saves or discards edits. Ctrl-H works from editor/filter; confirmation dialogs
retain their current handling.

No selected task, missing/stale/ambiguous identity, focus failure, client launch
failure or nonzero client exit produces visible error. Failed lookup/focus never
opens client. No task mutation or new agent creation.

Real PTY tests use fake Herdr executable to record command order and validate
cooked terminal flags during attachment. Cover moved pane, named server,
terminal identity, dirty editor/filter preservation, failure paths, Backspace,
JSON stdout and final terminal restoration. Run full Rust tests, fmt, Clippy;
review, commit, rebase/merge locally, install, complete queue item.
