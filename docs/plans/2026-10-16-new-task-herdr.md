# New Task Herdr Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a Herdr queue worker from the TUI's New task flow using an editable prompt.

**Architecture:** Add a small creation dialog alongside the existing modal views.
Extend `tui/handoff.rs` with workspace lookup and agent startup, retaining the
existing terminal handoff. Use the open DB path to resolve the queue root.

**Tech Stack:** Rust, Ratatui, Crossterm, Herdr CLI, Python PTY fixtures.

### Task 1: Specify observable creation behavior

Files: `tests/tui_dashboard_pty.py`, `tests/tui.rs`.

- [ ] Add PTY cases for New-task creation, cancellation, prompt edits, project
  root from a nested cwd, workspace creation/reuse, server startup, failures,
  shell-started agents, compact/no-color UI, and unmodified task data.
  Use `send(b"\x08")` to open and `send(b"\x13")` to submit. Fake Herdr must
  record argument arrays and assert prompt text, cwd, target pane, and order.
- [ ] Run `cargo test --locked --test tui tui_dashboard_ctrl_h_creates -- --nocapture`
  and confirm failure because Ctrl-H does not open the creation dialog.

### Task 2: Implement the creation dialog and integration

Files: `src/tui/agent_create.rs`, `src/tui/handoff.rs`, `src/tui/mod.rs`,
`src/tui/render.rs`, `README.md`.

- [ ] Add `View::new`, `View::paste`, `View::key`, and `View::rows` for the prompt.
  Reuse `tag_input::Cursor` editing and rendering; reject whitespace-only input.
- [ ] Add `handoff::create(db: &crate::db::Db, prompt: &str) -> Result<()>` to
  resolve the root, check/start Herdr, select/create a workspace, start Codex,
  submit the prompt, and focus its pane. Use typed JSON response structures and
  direct process arguments. Include the created pane in errors.
- [ ] Route modal rendering, paste, keyboard input, and mouse blocking through
  the new view. Intercept Ctrl-H for New tasks/drafts before linked handoff.
  Drop and restore the terminal guard around creation and client attachment.
- [ ] Update the Ctrl-H hint and README. Update existing handoff fixtures to use
  in-progress linked tasks so they continue to cover the retained behavior.
- [ ] Rerun new cases, existing Ctrl-H cases, and TUI unit/render tests.

### Task 3: Verify and deliver

- [ ] Run `cargo fmt --check`, strict Clippy, `cargo test --locked`, compatibility
  checks, and `cargo build --locked --release`. Use the shared target directory
  and two build jobs to limit disk use.
- [ ] Review the final diff against the spec, fix actionable findings, and commit
  as `[Feat] Create Herdr Agents From New Tasks`.
- [ ] Rebase and fast-forward into master, install the verified release, run
  installed-release PTY smoke tests, and complete task #235 with evidence.
- [ ] Remove the clean worktree and resume one blocking qqq waiter.
