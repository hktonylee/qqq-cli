# New Task Herdr Agent

Task #235 adds a Ctrl-H creation dialog for saved tasks with status `new` and
the unsaved New Task editor. Other task statuses keep their existing linked
agent handoff. The creation dialog shows the queue project directory and an
editable prompt initialized to the exact requested `/goal` text. Enter inserts
a line, Ctrl-S creates, and Escape/Ctrl-C cancels. Opening, editing, cancelling,
and launching never save, claim, or change the selected task or its draft.

Resolve the project root from the open database's `.qqq/qqq.db` path, including
when the TUI runs from a subdirectory. Follow `+herdr_here`: start the default
Herdr server if needed, wait up to ten seconds for readiness, and find the
workspace whose pane launch directory matches that root. Reuse that workspace
by creating a fresh tab; otherwise create the workspace and use its root pane.
Keep creation unfocused until startup and prompt submission succeed.

Start a Codex agent in the created pane. If shell startup has already launched
Codex, wait for that agent to become idle instead of launching another one.
Deliver the entered prompt as one argument through `herdr agent prompt`, focus
the created agent, and reuse the existing cooked-terminal client handoff.
Preserve the editor, filter, cursor, and selection when returning. Reject blank
prompts before any Herdr commands. Show failures with any created pane's ID;
never automatically repeat creation or uncertain prompt submission.

Reuse existing popup styles and multiline input editing. Verify with unit tests
and real PTY tests using a fake Herdr executable: cancellation, exact/default and
edited multiline prompts, root discovery, workspace reuse/creation, server
startup, automatic Codex startup, failures, no-color and compact terminals,
unchanged database/drafts, terminal restoration, and existing linked handoff.
