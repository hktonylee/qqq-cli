# Grouped Task Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Related Ctrl-G actions form groups separated by empty rows; Parent becomes Set parent.

**Architecture:** Static groups define one action order. Selection remains indexed over actions; rendered rows include nonselectable separators. Action-row generation fits a window around selected item in short terminals.

**Tech Stack:** Rust, Crossterm, Ratatui, Python PTY scenarios.

## Regression And Implementation

- [ ] Add tests in `src/tui/mod.rs` for exact grouped labels and action-only selection with/without Retry, Archive/Unarchive, and small heights. Run `cargo test --locked --offline --bin qqq grouped_action_menu`; observe expected failures.
- [ ] Replace flat menu definition with status/settings/visibility groups. Render empty `PopupKind::Body` rows between groups, map selected action independently of separator row indices, fit selected row plus heading/hint into available height.
- [ ] Update `tests/tui_dashboard_pty.py` popup bounds, expected action order, retry navigation, error navigation, and narrow-menu assertions. Assert actual empty rows between visible groups. Run action-popup, menu-arrows, menu-retry, task-actions, and mark-error PTY tests.
- [ ] Update README/reference menu description. Run full `cargo test --locked --offline`, `cargo fmt --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`, and locked offline release build.
- [ ] Request independent review, address concrete findings with regression checks, commit verified implementation.
- [ ] Rebase, integrate locally, install, run installed-menu PTY checks. Record evidence with `qqq message 166`, complete task. Clean worktree and resume persistent queue wait after delivery.
