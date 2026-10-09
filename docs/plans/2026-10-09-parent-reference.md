# Parent Reference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Ctrl-P in dashboard child draft inserts parent #ID as ordinary text.

**Architecture:** Reuse Draft.insert and existing undo/history/save paths.
Extend existing Ctrl-P handler with child-draft/editor-focus branch. Change hint
only for active child draft. No atom, renderer, storage, or schema changes.

**Tech Stack:** Rust, Crossterm, Python PTY fixtures.

## Reproduce and fix

- [x] Add child_reference and child_reference_no_color PTY scenarios in
  tests/tui_dashboard_pty.py, register tests/tui.rs. Select #2, open child,
  move caret between words, Ctrl-P -> Before #2 after. Assert DB unchanged.
- [x] Run cargo test --offline --test tui tui_dashboard_child_parent_reference
  -- --nocapture. Confirm current Ctrl-P misses insertion.
- [x] In src/tui/mod.rs, before selected-parent branch, add:

```rust
if let Some(parent) = draft_parent_id.filter(|_| target_id.is_none()) {
    if !filter_focused {
        draft.insert(&format!("#{parent}"));
        editor_follow_cursor = true;
        message.clear();
        message_is_error = false;
    }
} else if let Some(parent) = target_id {
    // Keep existing child-draft opening branch.
}
```

- [x] Add child-specific Ctrl-P Parent Ref footer constant in src/tui/render.rs;
  select it in src/tui/mod.rs when draft_parent_id is Some and target_id is None.
- [x] Run new tests; expand to repeat insertion, undo/redo, character deletion,
  dirty buffer restore, failed save, successful save/reload, filter/popup scope.
- [x] Update README.md Ctrl-P row and docs/reference.md child-draft paragraph.
- [x] Run fmt, strict Clippy, full tests, release build; review, commit feature.
- [x] Rebase, fast-forward master, install, verify installed PTY/compatibility.
  Record qqq evidence, complete/readback #194. Normal worktree cleanup and
  continuation use one persistent queue waiter.

## Verification

- Baseline: 191 model/render/TUI tests passed.
- Shortcut RED: Ctrl-P retained text and reported Select parent task first.
- Final three PTY paths passed: color, NO_COLOR, retained bulk marks.
- Full serial suite: 864 tests across 47 binaries passed.
- Formatting, strict all-target Clippy, release build/help/version passed.
- Review found stale child hint with bulk marks; regression reproduced RED,
  fix passed all three PTY paths; follow-up review found no further issues.
- Plain text only: no reference atoms, chips, parsing, or schema changes.

Installed delivery: qqq 0.7.0, three reference PTY paths and 35 historical
compatibility tests passed. Installed and both release binaries match SHA256
f1532791d97964cf7647b334893a4643a3369c3f379e2aae33d8cf68f3c66d46.
Doctor reported no issues at schema 13. Task #194 completed and read back;
claim/complete session matched. Queue evidence message #130.
