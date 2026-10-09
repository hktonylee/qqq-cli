# Parent Reference Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ctrl-P in a dashboard child draft inserts an atomic parent reference.

**Architecture:** Extend Draft atoms with TaskReference and reuse sparse undo
history. Saved-text seeding recognizes standalone #ID tokens; existing image and
paste parsing stays authoritative. Extend Layout highlights and dashboard Ctrl-P
context dispatch. Description persistence stays plain text.

**Tech Stack:** Rust, Crossterm, Python PTY fixtures, SQLite.

## 1. Reproduce shortcut behavior

- [ ] Add `child_reference` and `child_reference_no_color` scenarios in
  `tests/tui_dashboard_pty.py`, registered in `tests/tui.rs`. Select #2, Ctrl-P to
  open child, type `Before  after`, move caret between spaces, press Ctrl-P.
  Assert `Before [#2] after` and caret after chip; DB stays unchanged.
- [ ] Run `cargo test --offline --test tui tui_dashboard_child_parent_reference
  -- --nocapture`; expect failure because Ctrl-P currently reports no parent.

## 2. Add reference atoms and rendering

- [ ] Add model tests in `tests/tui_model.rs`: `Draft::task_reference(99)` inserts
  one atom, `finish().description == "#99"`, undo empties draft, redo restores
  chip, Backspace removes whole atom. Add saved-text boundary and paste/image
  coexistence cases using `Draft::from_saved`.
- [ ] Add renderer tests in `tests/tui_render.rs` with `Layout::with_references`
  for wrapped chip highlights, atom caret boundaries, and NO_COLOR output.
- [ ] Run model and render tests; confirm missing behavior before implementation.
- [ ] Extend `src/tui/draft.rs`: `Atom::TaskReference(i64)`, label `[#ID]`, raw
  contents `#ID`, content/word/history handling, `task_reference`,
  `reference_mask`, and saved-text seeding at current plain-text load sites.
  Recognize canonical i64 IDs only; do not change bytes or look up IDs.
- [ ] Extend `src/tui/render.rs`: TaskReference highlight, `with_references`
  constructor; existing constructors delegate with empty reference mask.
- [ ] Extend `src/tui/mod.rs`: pass reference mask, insert only for child draft
  with editor focus, preserve existing parent selection branch, set cursor
  following and clear stale message. Render child-specific Ctrl-P hint.
- [ ] Run model/render/new PTY tests; commit verified feature.

## 3. Verify lifecycle and finish

- [ ] Expand terminal scenario to undo/redo, deletion, retained child draft,
  failed save, save/reload, and filter/popup guards. Wait for final caret frame
  before comparing DB. Run targeted child and undo tests.
- [ ] Update README Ctrl-P row and docs/reference child-draft paragraph with
  chip display, exact storage, reload parsing, and undo semantics.
- [ ] Run `cargo fmt --check`, strict all-target Clippy, full serial tests,
  release build; obtain code review, fix concrete findings, commit changes.
- [ ] Rebase onto current master, fast-forward clean root, install checkout.
  Run installed reference PTY and historical compatibility checks. Record qqq
  evidence, complete #194, verify readback, remove branch/worktree, resume one
  persistent `qqq next --wait --local --json`.
