# Editor Undo/Redo Design

Task #189 adds Ctrl-Z undo and Ctrl-Y redo to built-in task-description editors.
User explicitly chose to keep history after Ctrl-S while same task stays open.

## Interaction

- Each mutating input is one step: typed character/newline/tab, Backspace/Delete,
  Ctrl-W word deletion, bracketed paste or clipboard/image paste. Cursor motion,
  empty edits and failed image validation create no step and preserve redo.
- Undo/redo restores grapheme atoms, complete paste/image tokens, caret and image
  numbering. Text, raw paste bytes, image bytes and stored image identity stay exact.
  A changed undo/redo follows restored caret; exhausted history leaves caret/scroll.
- A new edit after undo clears redo. History belongs to draft, follows existing
  parked unsaved buffers. Fresh load, accepted discard and new blank draft start
  fresh history; existing clean-buffer navigation policy remains unchanged.
- Save keeps history when editor keeps saved task open. Normalize newly stored
  image refs in active draft and affected history, preserving atom positions.
  Require snapshot revision to match actual revision returned by save transaction,
  then verify candidate content exactly matches saved snapshot before carrying history;
  unexpected external reload starts fresh history. Undo itself never changes DB,
  revision, tags, ownership or task status. Failed saves/conflict cancellation keep
  history; accepted conflict reload starts fresh. Saving undone text uses existing
  revision guard. Saved pending tags become metadata; undo concerns editor content.
- Ctrl-Z/Y are inactive while filter or any popup owns keyboard input. Existing
  action confirmation `y`/`Y` remains unchanged. All built-in add/edit/TUI modes,
  Unicode, NO_COLOR and resizing remain supported.

## Storage

Use reversible splices retaining removed atoms, replacement length, previous
caret/image counter. Applying undo/redo swaps removed/replacement atoms; no whole
draft snapshot or image/paste payload cloning for each keystroke. Cap history at
256 changes and 64 MiB of retained removed content, keeping newest single change
if oversized. Bounds affect oldest undo/farthest redo only. Initialization is not
an edit. Existing save/switch/dirty/transaction semantics remain in their owners.

## Validation

Model tests cover Unicode joining, atomic deletes/pastes/images, no-op redo,
branch invalidation, history bounds, initial/reloaded seeds, saved-image identity.
Real PTYs cover both bindings, dirty/caret/frame restoration, save then undo/redo,
saved and pending image/paste bytes, parked drafts, modal/filter isolation,
failed-save/conflict lifecycle, standalone add/edit, no-color and compact resize.
Run full suite, fmt, strict Clippy/release, independent review, local integration,
installed PTYs/compatibility and doctor before completing task.
