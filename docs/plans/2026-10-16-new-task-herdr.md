# New Task Herdr Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Create a Herdr queue worker from a saved New task using an editable prompt.

**Architecture:** Add a small creation dialog alongside the existing modal views.
Extend `tui/handoff.rs` with workspace lookup and agent startup, retaining the
existing terminal handoff. Use the open DB path to resolve the queue root.

**Tech Stack:** Rust, Ratatui, Crossterm, Herdr CLI, Python PTY fixtures.

### Task 1: Specify observable creation behavior

Files: `tests/tui_dashboard_pty.py`, `tests/tui.rs`.

- [x] Add PTY cases for New-task creation, cancellation, prompt edits, project
  root from a nested cwd, workspace creation/reuse, server startup, failures,
  shell-started agents, compact/no-color UI, and unmodified task data.
  Use `send(b"\x08")` to open and `send(b"\x13")` to submit. Fake Herdr must
  record argument arrays and assert prompt text, cwd, target pane, and order.
- [x] Run `cargo test --locked --test tui tui_dashboard_ctrl_h_creates -- --nocapture`
  and confirm failure because Ctrl-H does not open the creation dialog.

### Task 2: Implement the creation dialog and integration

Files: `src/tui/agent_create.rs`, `src/tui/handoff.rs`, `src/tui/mod.rs`,
`src/tui/render.rs`, `README.md`.

- [x] Add `View::new`, `View::paste`, `View::key`, and `View::rows` for the prompt.
  Reuse `tag_input::Cursor` editing and rendering; reject whitespace-only input.
- [x] Add `handoff::create(db: &crate::db::Db, prompt: &str) -> Result<String>` to
  resolve the root, check/start Herdr, select/create a workspace, start Codex,
  submit the prompt, and focus its pane. Use typed JSON response structures and
  direct process arguments. Include the created pane in errors.
- [x] Route modal rendering, paste, keyboard input, and mouse blocking through
  the new view. Intercept Ctrl-H only for saved New tasks before linked handoff.
  Drop and restore the terminal guard around creation and client attachment.
- [x] Update the Ctrl-H hint and README. Update existing handoff fixtures to use
  linked tasks with a non-New status so they continue to cover the retained behavior.
- [x] Rerun new cases, existing Ctrl-H cases, and TUI unit/render tests.

### Task 3: Verify and deliver

- [x] Run `cargo fmt --check`, strict Clippy, `cargo test --locked`, compatibility
  checks, and `cargo build --locked --release`. Use the shared target directory
  and two build jobs to limit disk use.
- [x] Review the final diff against the spec, fix actionable findings, and commit
  as `[Feat] Create Herdr Agents From New Tasks`.
- [ ] Rebase and fast-forward into master, install the verified release, run
  installed-release PTY smoke tests, and complete task #235 with evidence.
- [ ] Remove the clean worktree and resume one blocking qqq waiter.


Verified on 2026-10-16: formatting and strict Clippy passed; the full locked
suite passed 897 tests across 47 binaries with two test threads; the optimized
release built and passed all 16 creation PTY scenarios. Herdr calls in these
PTY scenarios use a fake executable checked against installed Herdr 0.9.5 help
and its bundled response schema; no real worker was launched during tests.
Independent review findings for attachment recovery and wrapped Unicode cursor
navigation were reproduced, fixed, and verified. An existing undo PTY scenario
now waits until each popup has closed before continuing to the next action.
