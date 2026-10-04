# Force Complete From Task Menu

Task 160 asks the dashboard to offer forced completion when the selected task is not claimed by the current session. Scope includes every unfinished status (`new`, `in_progress`, `error`); already-completed tasks remain invalid.

## Interaction

Keep the existing Ctrl-G menu and `c Complete` row, including arrow/Enter activation. When activated, resolve current ownership using the same session discovery and harness-name matching as ordinary completion. A matching active claim selects the existing Complete action. Otherwise select Force Complete and show `Force complete task #ID?` plus `Complete without matching task owner.` The existing `y confirm  n/Esc cancel` hint and dirty-draft warning remain. No mutation happens before `y`.

Use the existing popup styles, wrapping, keyboard handling and color opt-outs. Cancellation retains selected task, draft, caret and buffers. Successful completion refreshes saved task content and status as other state actions do. Rejected transitions preserve drafts and show the existing error popup.

## Data and ownership

Add a separate database force-completion operation. In one immediate transaction, require an existing unfinished task, mark it completed, clear claim and harness/orchestrator ownership fields, update the timestamp, and append a standard `complete` event attributed to explicit session or `manual`. Preserve description, content revision, priority, archive state, parent/prerequisites, messages, images and Herdr link. Already-completed and missing tasks fail without writes. No CLI force-complete flag or schema change is needed.

Pass a completion-action resolver into the dashboard beside the existing mutation handler. Resolve ownership at action activation, not from displayed harness metadata. Missing session discovery means no matching claim and offers the explicit force prompt. Database owner-resolution errors still surface. Ordinary completion continues checking ownership transactionally: ownership changes while its prompt is open must fail instead of silently escalating to force.

## Verification

Use real PTY scenarios for owned/foreign/unclaimed/failed tasks, both shortcut and Enter activation, cancellation with a dirty draft, sessionless operation, native session aliases, plain color mode, and claim changes while a normal confirmation is open. Database tests verify metadata/content/history preservation, rejection of missing/completed tasks, and transaction rollback if event insertion fails. Update the old unclaimed-completion rejection assertion to cancel the new force prompt. Run focused tests, full suite, formatting, strict Clippy, read-only review, and installed CLI smoke before explicit task completion and cleanup.
