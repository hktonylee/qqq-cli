# Ctrl-C Filter Stages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Ctrl-C handles one stage per press: clear query, close empty focused filter, clear opened task with discard confirmation, exit new draft with discard confirmation.

**Architecture:** Extend existing early dashboard filter cancellation guard to include empty focused filters. Decide whether to close focus before clearing query; keep existing task/draft confirmation flows and single-task editor behavior.

**Tech Stack:** Rust, Crossterm, Ratatui, SQLite, Python PTY integration tests.

---

### Task 1: Regress empty-filter cancellation

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`.

- [x] Run baseline: `cargo test --locked --test tui tui_dashboard_escape_and_ctrl_c_return_to_new_before_exit`. Existing eight scenarios pass.
- [x] Extend existing `filter_escape_empty` PTY branch to accept `filter_ctrl_c_empty` names. Use this cancellation key:

  ```python
  send(b"\x03" if scenario.startswith("filter_ctrl_c_empty") else b"\x1b")
  ```

  Reuse assertions that bar disappears, original task title/draft/caret remain, process stays alive, DB rows stay unchanged, literal `/` still enters editor. Enable existing NO_COLOR setup for `filter_ctrl_c_empty_no_color`.
- [x] Add regression matrix:

  ```rust
  #[test]
  fn tui_dashboard_ctrl_c_empty_filter_returns_to_editor() {
      for name in [
          "filter_ctrl_c_empty",
          "filter_ctrl_c_empty_dirty",
          "filter_ctrl_c_empty_selected",
          "filter_ctrl_c_empty_selected_dirty",
          "filter_ctrl_c_empty_child",
          "filter_ctrl_c_empty_no_color",
          "filter_ctrl_c_empty_cleared",
          "filter_ctrl_c_empty_backspace",
      ] {
          dashboard_scenario(name);
      }
  }
  ```
- [x] Run `cargo test --locked --test tui tui_dashboard_ctrl_c_empty_filter_returns_to_editor -- --nocapture`; expect failure because empty-filter Ctrl-C exits or changes editor instead of closing filter.

### Task 2: Apply filter priority

**Files:** `src/tui/mod.rs`, `tests/tui_dashboard_pty.py`.

- [x] Replace filter cancellation guard/body prefix:

  ```rust
  if dashboard
      && (cancel_key || key.code == KeyCode::Esc)
      && (!filter_query.is_empty() || filter_focused)
  {
      if key.code == KeyCode::Esc || filter_query.is_empty() {
          filter_focused = false;
      }
      filter_query.clear();
      list_top = 0;
      list_follow_selected = true;
      continue;
  }
  ```

  Keep existing list reset and `continue`; remove later filter-focused Ctrl-C exit branch now handled by this guard.
- [x] Update existing `ctrl_c_filter_empty` scenario: second Ctrl-C hides bar and stays in blank editor; third exits. Update `ctrl_c_new_filter`: after query clear use Ctrl-C to close filter, assert dirty draft unchanged and no confirmation, then Ctrl-C opens exit confirmation. Update `ctrl_c_dirty_selected_filter_focused`: after query clear use Ctrl-C to close filter, assert selected dirty task unchanged and no confirmation, then Ctrl-C opens switch confirmation.
- [x] Run `cargo test --locked --test tui ctrl_c -- --nocapture`; expect all existing/new Ctrl-C paths pass, including denied/accepted discard, repeated Ctrl-C, images, whitespace, scrolled drafts, and terminal restoration.

### Task 3: Docs, review, integration

**Files:** `docs/reference.md`, this plan.

- [x] Describe filter stages in reference: `Ctrl-C clears nonempty query, preserving current focus. With empty focused filter, next Ctrl-C closes filter and returns focus to editor without changing opened task or draft. Later Ctrl-C follows editor cancel flow.` Keep Esc and popup/confirmation preservation semantics.
- [x] Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`; require zero failures. Request read-only review through existing reviewer.
- [ ] Record verification; commit `[Fix] Close Empty Filter Before Ctrl C Cancels Editor`. Rebase on current master, resolve conflicts, verify changed integration if needed; fast-forward master.
- [ ] Install with `cargo install --path . --locked --offline --force`. Run installed CLI PTY scenarios `filter_ctrl_c_empty_selected_dirty`, `ctrl_c_new_filter`, `filter_ctrl_c_empty_no_color`. Remove merged worktree/branch, complete task #143, resume one persistent `qqq next --wait --local --json` waiter.

Verification: existing eight cancellation scenarios passed at baseline. New empty-filter regression failed with `Editor exited before visible state`; after fix, three Ctrl-C tests covering 22 PTY scenarios passed. Full suite passed 490 tests; formatting, Clippy, diff check passed. Read-only review approved with no findings.
