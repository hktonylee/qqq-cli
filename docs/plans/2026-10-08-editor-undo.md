# Editor Undo/Redo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Ctrl-Z undo/Ctrl-Y redo with exact draft atoms/caret and saved-task history.

**Architecture:** Reversible atom splices live inside Draft. Input dispatch calls
undo/redo only after popup/filter guards. Saved-image normalization preserves
existing atom positions, history and DB identity when snapshot matches.

**Tech Stack:** Rust, Crossterm, SQLite, Python PTYs and shared TerminalScreen.

### 1. RED behavior

Files: `tests/tui_undo_pty.py`, `tests/tui.rs`, `tests/tui_model.rs`.

- [x] Run baseline `cargo test --locked --offline --test tui_model -- --test-threads=1`;
  32 passed, `/tmp/qqq-task-189-baseline.log`.
- [ ] Add real-frame PTY helpers using shared decoder, separate stdout and temp DB.
  Seed saved task, paste one edit, Ctrl-Z then Ctrl-Y:

```python
paste(" changed")
wait_editor("Seed changed", len("Seed changed"))
send(b"\x1a")
wait_editor("Seed", len("Seed"))
assert cli("show", "1") == before
send(b"\x19")
wait_editor("Seed changed", len("Seed changed"))
```

- [ ] Run focused PTY test; expect missing undo screen, not compile/setup failure.
- [ ] Add model expectations for graphemes, atomic word/delete/paste/image,
  no-op redo, new-edit branch, seeds, bounds and saved image identity.

### 2. Reversible draft changes

Files: `src/tui/draft.rs`, `tests/tui_model.rs`.

- [ ] Add private edit record/history, bounded count/retained bytes. Apply inverse:

```rust
let replacement_len = change.atoms.len();
let replacement = std::mem::take(&mut change.atoms);
change.atoms = self.atoms.splice(change.start..change.start + change.replace, replacement).collect();
change.replace = replacement_len;
std::mem::swap(&mut self.cursor, &mut change.cursor);
std::mem::swap(&mut self.next_image, &mut change.next_image);
```

- [ ] Route insert/paste/image/Backspace/Delete/word deletion through one splice;
  calculate word range once. Keep old grapheme joining and atomic token behavior.
  Clear constructor/from_saved history after seed initialization.
- [ ] `adopt_saved` verifies rendered candidate against saved description using
  pending image spans/new sorted refs. Convert current/history image atoms to
  stored refs, normalize stored legacy refs, clear staged tags, keep history.
  Snapshot mismatch returns false without mutation; caller loads fresh snapshot.
- [ ] Run model suite; commit scoped verified implementation checkpoint.

### 3. Input/save integration and PTYs

Files: `src/tui/mod.rs`, `tests/tui_undo_pty.py`, `tests/tui.rs`.

- [ ] Handle Ctrl-Z/Y after all popup/filter guards. Set follow-caret only when
  change succeeds; exhausted history reports `Nothing to undo/redo`.
- [ ] When dashboard keeps saved task, pass previous draft through verified
  saved normalization before load_target. Clearing/new/accepted reload reset;
  parked unsaved drafts retain history by existing move ownership.
- [ ] Cover text/Unicode, caret/delete/word, branch, save, image/paste bytes,
  buffers, modal/filter, failed save/conflict, add/edit, no-color/compact/resize.
  Expect current DB unchanged until explicit Ctrl-S; saved images never duplicate.
- [ ] Run focused PTYs/model, existing TUI/render/DB checks; commit feature.

### 4. Documentation and completion

Files: `docs/reference.md`, `README.md`, this plan.

- [ ] Document keys, one-input edit steps, save/history/reset scope and bounds.
- [ ] Run full locked/offline serial tests, fmt, strict all-target Clippy, release.
  Request independent read-only review; fix important findings with focused tests.
- [ ] Rebase current master, fast-forward clean root, install current qqq.
  Run installed undo PTYs/compatibility; compare release/install SHA; doctor.
- [ ] Record evidence, qqq message/complete/read-back #189, clean worktree/branch,
  resume one persistent queue waiter. No unrelated changes or remote publication.

## Evidence

Implementation pending. User selected keeping history after save.
