# Task navigation in interactive `qqq add`

## Interaction

Only built-in terminal editor for `qqq add` gains Shift+Up and Shift+Down. Navigation includes tasks in every status, ordered by creation ID; deleted IDs are skipped. Shift+Up from new draft loads newest task. Repeated Shift+Up loads older tasks. Shift+Down loads newer tasks; from newest task it returns to blank new draft. At oldest task Shift+Up has no effect. At new draft Shift+Down has no effect. Footer advertises keys; header distinguishes new draft from task ID. Normal Up/Down, external `$EDITOR`, and `qqq edit` retain existing behavior.

Ctrl-S on loaded task edits its description through existing `Db::edit` transaction. Task status, owner, parent, messages, and existing image rows stay unchanged. Newly pasted/flagged images append through existing image path. Ctrl-S on new draft creates task through existing add transaction, applying `--parent` there; `--parent` does not change an existing task selected through navigation. No DB write occurs during navigation or confirmation. Concurrent deletion of selected task yields existing edit error on save; no replacement task is created.

## Dirty draft

Editor compares current expanded description and new image attachments with baseline loaded when draft was selected. Cursor movement and edits later undone do not make buffer dirty. A requested navigation with a valid destination prompts before replacing dirty content. Y discards and loads destination; N, Enter, or Esc keeps current draft. Ctrl-C cancels editor. Other input, including paste, is ignored while prompt is open. Boundary keys do not prompt. Reload resets cursor and scroll to top. Query errors keep draft intact and show footer error.

## Code and tests

`Db` supplies adjacent task ID/description with indexed ID-order queries. Built-in editor returns selected task ID alongside composition; external editor returns no selected task. `qqq add` chooses `Db::edit` or `Db::add` only after editor saves. PTY tests send Shift+Up/Down escape sequences, check header, dirty confirmation, cancel, task identity, new-draft return, and DB writes. Draft model tests cover reverted text and image dirtiness. Existing full tests, formatting, and Clippy must pass.
