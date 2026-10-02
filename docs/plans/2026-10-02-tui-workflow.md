# Complete task TUI workflow implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Verify complete `qqq tui` edit-and-create session plus terminal cleanup and state coverage.

**Architecture:** Extend existing Python PTY harness, retaining its screen emulator and isolated SQLite DB. Add scenarios rather than duplicating harness. Fix production TUI only when an assertion reproduces a fault.

**Tech Stack:** Rust CLI tests, Python PTY, SQLite, Crossterm.

---

### Task 1: Terminal and state coverage

**Files:** `tests/tui_dashboard_pty.py`, `tests/tui.rs`

- [x] Add scenarios `workflow_empty` and `workflow_status`. Skip seed inserts for empty; assert `No tasks yet.` and blank editor. For status, seed three tasks, claim/complete first, claim/fail second using `qqq edit 2 --set-status error --reason Failed --session worker`, and assert Completed, Error, New list labels plus status in selected editor heading.
- [x] Assert startup emits `\x1b[?2004h` and exit emits `\x1b[?2004l`; retain existing raw mode, alternate-screen, mouse capture, and stdout assertions. On induced `wheel_error`, assert all restoration sequences and raw mode recovery.
- [x] Run `cargo test --locked --test tui tui_dashboard_workflow_states_and_cleanup -- --nocapture`; fix any failing state assertions. Commit.

### Task 2: Combined edit and create session

**Files:** `tests/tui_dashboard_pty.py`, `tests/tui.rs`, `README.md`, production TUI file only if regression appears

- [x] Seed at least 12 matching tasks plus a nonmatching task. In PTY scenario `workflow`, type `/target`, wheel list up, click visible `Target` row, click editor, edit and Ctrl-S. Assert DB update and cleared new-task editor while child remains alive with empty stdout.
- [x] Type new task body, send bracketed text paste (`\x1b[200~...\x1b[201~`), then paste path of tiny PNG fixture from `tests/tui_pty.py`. Ctrl-S; assert new task body and image metadata in `qqq show`, plus visible save footer. Verify blank-draft Ctrl-S error remains recoverable before creating new task.
- [x] Add README TUI key/mouse help covering Shift-Up/Down, `/` and Alt+/, Tab/Enter, Ctrl-S, Esc/Ctrl-C, wheel, click, dirty confirmation, bracketed paste/image. Run focused PTY test and commit.

### Task 3: Verify and integrate

**Files:** all changed files

- [x] Run focused PTY/model checks, full `cargo test --locked`, `cargo fmt --all -- --check`, strict `cargo clippy --locked --all-targets -- -D warnings`, and `git diff --check`. Request read-only code review; fix Critical/Important findings and rerun affected checks.
- [x] Rebase onto local master, fast-forward, rerun integrated full suite, remove owned worktree/branch, install CLI, complete task 62, verify queue status, then call `qqq --json next --wait --local` once.
