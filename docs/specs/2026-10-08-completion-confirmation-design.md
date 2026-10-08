# Completion Confirmation Design

Task #187 replaces Ctrl-G → Complete's force checkbox with explicit keys.

- `y` always invokes owner-protected normal completion, including when popup
  preparation found no matching owner. `Y` always invokes existing atomic force
  completion. No fallback changes the meaning of lowercase `y`.
- Popup heading remains ownership-aware. Show `y confirm`, `Y force`, cancellation
  hint, existing ownership warning, dirty-draft warning, exact recoverable error.
  Compact hints retain both case-sensitive keys. No checkbox, selectable rows,
  Space toggle or mouse activation.
- Completion confirmation hides terminal cursor, including underlying editor or
  filter caret and undersized-terminal fallback. Closing restores previous editor
  caret. Action menus, text inputs and other confirmations retain cursor behavior.
- Discovery/transport failures and ownership races during normal confirmation
  retain popup for retry with `y` or explicit force with `Y`. Other DB/validation
  errors retain ordinary error handling. Force bypasses owner discovery only;
  unfinished/status/transaction/history checks remain in existing DB action.
- Existing cancellation keys and draft/content/image preservation remain intact.
  No schema, queue, process-preflight or CLI completion change.

Validation uses existing force PTY fixtures for legacy/native/sessionless owners,
initial/confirmation Herdr failures, ownership transfer, unrelated DB errors,
completed tasks, dirty drafts, color/no-color, compact/narrow layouts. Add explicit
cursor/key/noninteraction assertions before implementation; run focused then full
tests, fmt, strict Clippy, independent review and installed-binary checks.
