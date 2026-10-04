# Mark Error Task Action

Task #161 adds `e Mark error` to the existing Ctrl-G actions menu for saved
tasks. Append it after Parent so existing shortcut and arrow order remains
stable. Like Complete, the item stays visible and the DB enforces eligibility.

Selecting it opens `Error task #ID` with a required reason. Enter rejects empty
or whitespace-only input inline. Backspace edits graphemes; paste replaces
control characters with spaces for this reason input. Other action inputs keep
their current paste behavior. Esc cancels using existing modal/filter rules.

A valid trimmed reason always opens `Mark error task #ID?` confirmation. Show
a wrapped reason preview, capped at three rows, and the existing dirty-draft
warning. Only `y` applies the transition; `n`, Esc or Enter cancels. Cancel and
DB rejection preserve the editor draft, caret and persistent task data.
Confirmed success follows existing action behavior, including the confirmed
discard of an active dirty draft, and refreshes the task title/list/details.

Reuse resolved owner identity and `EditTransition::Error`: only the owner of an
in-progress task can mark it error. Atomically clear the claim and assignment,
record one error event and one reason message, preserve saved description,
images, priority, parent and content revision. No force/reset or new DB API.
The task stays out of the queue until explicitly retried. Existing Retry then
appears for the error task.

Tests cover shortcut and arrow activation, required reason/edit/paste, clean
and dirty confirmation/cancel, stale or foreign ownership, invalid task state,
full reason persistence in a narrow terminal, NO_COLOR and terminal cleanup.
Full suite, formatting, Clippy, read-only review, local integration and installed
binary checks gate task completion.
