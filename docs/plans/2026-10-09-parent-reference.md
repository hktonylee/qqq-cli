# Parent Reference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ctrl-P in dashboard child draft inserts parent #ID as ordinary text.

**Architecture:** Reuse Draft.insert and existing undo/history/save paths.
Extend existing Ctrl-P handler with child-draft/editor-focus branch. Change hint
only for active child draft. No atom, renderer, storage, or schema changes.

**Tech Stack:** Rust, Crossterm, Python PTY fixtures.

## Reproduce and fix

- [ ] Add child_reference and child_reference_no_color PTY scenarios in
  tests/tui_dashboard_pty.py, register tests/tui.rs. Select #2, open child,
  move caret between words, Ctrl-P -> Before #2 after. Assert DB unchanged.
- [ ] Run cargo test --offline --test tui tui_dashboard_child_parent_reference
  -- --nocapture. Confirm current Ctrl-P misses insertion.
- [ ] In src/tui/mod.rs, before selected-parent branch, add:

```rust
if target_id.is_none() && draft_parent_id.is_some() {
    if !filter_focused {
        draft.insert(&format!("#{}", draft_parent_id.unwrap()));
        editor_follow_cursor = true;
        message.clear();
        message_is_error = false;
    }
} else if let Some(parent) = target_id {
    // Keep existing child-draft opening branch.
}
```

- [ ] Add child-specific Ctrl-P Parent Ref footer constant in src/tui/render.rs;
  select it in src/tui/mod.rs when draft_parent_id is Some and target_id is None.
- [ ] Run new tests; expand to repeat insertion, undo/redo, character deletion,
  dirty buffer restore, failed save, successful save/reload, filter/popup scope.
- [ ] Update README.md Ctrl-P row and docs/reference.md child-draft paragraph.
- [ ] Run fmt, strict Clippy, full tests, release build; review, commit feature.
- [ ] Rebase, fast-forward master, install, verify installed PTY/compatibility.
  Record qqq evidence, complete/readback #194, clean worktree, resume waiter.
