# Grouped Task Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Related Ctrl-G actions form groups separated by empty rows; Parent becomes Set parent.

**Architecture:** Static groups define one action order. Selection remains indexed over actions; rendered rows include nonselectable separators. Action-row generation fits a window around selected item in short terminals.

**Tech Stack:** Rust, Crossterm, Ratatui, Python PTY scenarios.

## Regression And Implementation

- [x] Add tests in `src/tui/mod.rs` for exact grouped labels and action-only selection with/without Retry, Archive/Unarchive, and small heights. Run `cargo test --locked --offline --bin qqq grouped_action_menu`; observe expected failures.
- [x] Replace flat menu definition with status/settings/visibility groups. Render empty `PopupKind::Body` rows between groups, map selected action independently of separator row indices, fit selected row plus heading/hint into available height.
- [x] Update `tests/tui_dashboard_pty.py` popup bounds, expected action order, retry navigation, error navigation, and narrow-menu assertions. Assert actual empty rows between visible groups. Run action-popup, menu-arrows, menu-retry, task-actions, and mark-error PTY tests.
- [x] Update README/reference menu description. Run full `cargo test --locked --offline`, `cargo fmt --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`, and locked offline release build.
- [x] Request independent review, address concrete findings with regression checks, commit verified implementation.
- [x] Rebase, integrate locally, install, run installed-menu PTY checks. Record evidence with `qqq message 166`, complete task.

After delivery, clean worktree and resume persistent queue wait.

## Verification

- Baseline render suites: 52 tests passed.
- Both new grouped-menu tests failed against old labels/layout, then passed with groups and selection-preserving small-height window.
- Focused PTY checks passed for popup/background/resize, arrows/wrap/Enter, Retry live changes, Mark error, and task actions.
- Full suite: 635 tests across 39 binaries, zero failures. Formatting, Clippy all-targets with warnings denied, and locked offline release build passed.
- Independent review found no concrete issues; two grouped-menu tests passed independently.
- Logs: `/tmp/qqq-task-166-full.log`, `/tmp/qqq-task-166-clippy.log`, `/tmp/qqq-task-166-release.log`.
- Integrated locally at `893c87f`; installed qqq 0.5.0 passed 17 menu PTY scenarios. Installed/release SHA256 match: `116b45f9cb0a23b027094d112412c5607a2fca1e39b2e53abb580267c6bc5a8b`. Task #166 completed through qqq. Installed log: `/tmp/qqq-task-166-installed-pty.log`.
