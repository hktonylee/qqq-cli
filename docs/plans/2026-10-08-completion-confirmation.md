# Completion Confirmation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Replace completion checkbox with `y` normal / `Y` force and hide popup caret.

**Architecture:** Keep existing preparation/error classification and DB handlers.
Choose completion action from confirmation key, never initial ownership classification.
Add explicit dashboard cursor visibility so completion frames never request a caret.

**Tech Stack:** Rust, Crossterm, Ratatui, SQLite, Python PTY harness.

### 1. Baseline and regressions

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`

- [x] Run unchanged baseline: `cargo test --locked --offline --test tui tui_dashboard_force_completion -- --test-threads=1` (4 tests / 11 scenarios).
- [x] Replace checkbox interaction assertions with direct key assertions. Example:

```python
wait_visible(lambda: "Y force" in visible.text() and not visible.cursor_visible)
assert "[ ] Force complete" not in visible.text()
send(b"y")  # foreign owner must fail normally without changing task/history
wait_visible(lambda: "not claimed" in visible.text() and not visible.cursor_visible)
assert cli("show", "1") == before_force
send(b"Y")  # explicit force completes through existing guarded DB action
```

- [x] Check inert Space/arrows/mouse, cancellation and restored dirty caret;
  owner/session/Herdr/race/DB-error/status fixtures retain their existing guards.
- [x] Run same command; observe failures for old checkbox/cursor or blocked `Y`.

### 2. Confirmation and rendering

**Files:** `src/tui/mod.rs`, `src/tui/dashboard.rs`, `src/tui/completion.rs`,
`tests/tui_dashboard_render.rs`

- [x] Remove `Confirmation::Action.force`, `toggle_force`, checkbox hit handler,
  checkbox rows and obsolete `FORCE_REQUIRED`. Keep error string/recovery whitelist.
- [x] Select action only at confirmation:

```rust
let selected = if action.is_completion() {
    if key.code == KeyCode::Char('Y') {
        TaskAction::ForceComplete(action.id())
    } else {
        TaskAction::Complete(action.id())
    }
} else {
    action.clone()
};
```

- [x] Use full hint `y confirm  Y force  n/Esc cancel`, compact `y Y force Esc`;
  retain warnings/errors and reserve last popup row for hints.
- [x] Add `View.hide_cursor: bool`, set only for completion confirmation. Guard
  dashboard/filter/editor/minimum-size cursor setters; popup receives cursor flag.
  Existing render fixtures set false. Input/menu popup calls request cursor.
- [x] Run focused force PTY and complete render suite; expect all pass. Commit
  scoped verified change with `[Feat] Simplify Completion Confirmation Keys`.

### 3. Documentation, integration and installation

**Files:** `docs/reference.md`, this plan

- [x] Replace checkbox documentation with explicit `y`/`Y`, passive popup and
  normal error retry. Run full locked/offline serial suite, fmt check, strict
  all-target Clippy, release build. Obtain independent read-only code review.
- [ ] Rebase current master, resolve any concurrent-worker changes in worktree,
  fast-forward clean master. Install with `cargo install --path . --locked --offline --force`.
- [ ] Run force PTY scenarios against installed binary, compatibility suite and
  doctor; compare release/install SHA256. Record evidence, complete #187 through
  qqq, read back, remove own worktree/branch, resume one persistent queue waiter.

## Evidence

- Baseline passed: 4 Rust tests / 11 PTY scenarios, `/tmp/qqq-task-187-baseline.log`.
- RED: 4 tests failed for old checkbox/cursor behavior, `/tmp/qqq-task-187-red.log`.
- GREEN: 4 tests / 11 scenarios passed, `/tmp/qqq-task-187-green.log`. Includes
  guarded lowercase normal completion, explicit uppercase force, inert Space/
  arrows/mouse/modified keys, dirty caret restoration, Herdr retry/no-discovery
  force path, owner race, unrelated DB error, completed-task rejection, no-color,
  narrow/compact, 12x8 and undersized resize. A test indentation error was corrected;
  foreign-owner waits now require changed error frame, avoiding stale modal reads.
- Binary unit tests and 36 dashboard render tests passed, `/tmp/qqq-task-187-render.log`.
- After clean rebase onto #186 (`28a4164`), full serial locked/offline suite passed:
  **832 tests / 47 binaries**, `/tmp/qqq-task-187-full.log`.
- `cargo fmt --check`, strict all-target Clippy and release build passed:
  `/tmp/qqq-task-187-clippy.log`, `/tmp/qqq-task-187-release.log`.
- Independent read-only review of `28a4164..accd78d` found no actionable issues;
  reviewer inspected source/PTY waits, ran no tests. Current full suite is fresh
  combined-source evidence. No DB/schema/preflight changes or remote CI/publish.
- Integration/installed-binary verification pending.
