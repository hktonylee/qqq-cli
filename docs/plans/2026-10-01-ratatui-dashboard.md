# Ratatui Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render `qqq tui` split dashboard through Ratatui with useful color while preserving task editing and terminal behavior.

**Architecture:** Keep existing task-tree formatting, panel scroll model, and grapheme-aware editor layout. Add a Ratatui frame painter for dashboard; hold one persistent `Terminal<CrosstermBackend<Stderr>>` in dashboard session. Keep standalone editor's Crossterm renderer. Ratatui 0.29 preserves Rust 1.85 support.

**Tech Stack:** Rust 2024, Ratatui 0.29, Crossterm 0.29 input, Ratatui `TestBackend`, Python PTY tests.

---

### Task 1: Dependency and frame behavior tests

**Files:** Modify `Cargo.toml`, `Cargo.lock`, `src/tui/render.rs`; create `tests/tui_dashboard_render.rs`.

- [x] Add `ratatui = "=0.29.0"` to dependencies; run `cargo update -p ratatui --precise 0.29.0` or `cargo check` to lock it. Confirm `cargo tree -i ratatui` and `cargo check --locked` pass without raising `rust-version = "1.85"`.
- [x] Add `pub(super)` access to `render::Layout::image_spans` for dashboard painting. Keep layout construction and standalone drawing unchanged.
- [x] Add failing `TestBackend` test for `dashboard::draw`: create 72x16 terminal; pass rows from `panel::rows("ID     STATUS       TASK\n1      New          First")`, `render::Layout::new(&["Draft".into()], &[], 72)`, selected task 1; assert row 0 begins `qqq tasks`, row 2 begins `> 1` after task-tree header, row 8 begins `qqq task editor`, row 9 begins `Draft`, row 15 begins `Ctrl-S`, cursor equals `(0,9)`.
- [x] Add 10x7 test asserting clipped `Resize ter` hint and no colored cells; add 72x16 no-color test asserting default foreground/background across dashboard cells. Run `cargo test --locked --test tui_dashboard_render`; expect missing dashboard module/function failure. Commit tests and dependency as `[Test] Define Ratatui Dashboard Frames`.

### Task 2: Dashboard painter and semantic color

**Files:** Create `src/tui/dashboard.rs`; modify `src/tui/mod.rs`, `tests/tui_dashboard_render.rs`.

- [x] Implement `dashboard::draw(frame, rows, statuses, selected, list_top, editor, color)`. Use `ratatui::layout::Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])` for top/bottom. For min-size failure render `Paragraph::new("Resize terminal (min 12x8)")` clipped to frame. Top reserves first row for `qqq tasks`, last row for separator; `panel::scroll_to(rows, selected, list_top, top.height.saturating_sub(2))` chooses visible rows. Render each task line with `> ` marker, clipping through Ratatui area width.
- [x] Paint editor heading and footer as `Paragraph`s; body area is bottom rectangle excluding those rows. Update `editor.top` until `render::Layout::positions[editor.cursor]` is visible. Build `Line`s from each visible `layout.rows` entry and `image_spans`, with accent `Span` only for image bytes. Fill body rectangle with neutral style, render text, set frame cursor to `body.x + column`, `body.y + row - editor.top`.
- [x] Add color tests using `TestBackend::buffer().get(x,y).style()`: selected row highlight, in-progress cyan, completed muted, error red, editor dark body/light text, image accent, message footer. Use task ID -> status lookup from `Vec<Task>` in `compose_inner`; continuation rows inherit task ID. Assert no-color cells retain default style. Run focused render tests; commit as `[Feat] Render Split Dashboard With Ratatui`.

### Task 3: Persistent terminal and PTY regression

**Files:** Modify `src/tui/mod.rs`, `tests/tui_dashboard_pty.py`, `tests/tui.rs`.

- [x] Create `Terminal<CrosstermBackend<Stderr>>` once after `TerminalGuard::enter()` for dashboard mode; call `terminal.draw(|frame| dashboard::draw(...))?` each loop. Keep `event::read`, save, navigation, confirmation, and `TerminalGuard` restoration as they are. Continue to send all TUI bytes to stderr.
- [x] Update dashboard PTY assertions that rely on old byte-by-byte Crossterm repaint sequence to inspect rendered semantic content. Add color-on, `NO_COLOR`, and `TERM=dumb` scenarios asserting status/selection accents only when enabled, while stdout remains clean.
- [x] Run `cargo test --locked --test tui_dashboard_render --test tui --test tui_render`; fix any resize/cursor/selection regressions. Commit as `[Feat] Use Persistent Ratatui Dashboard Terminal`.

### Task 4: Verification, review, integration

**Files:** Update this plan's checkboxes; modify `README.md` only if TUI documentation needs a behavior correction.

- [x] Run `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `git diff --check`; require exit 0.
- [x] Request read-only code review. Fix confirmed Critical/Important findings and rerun affected checks.
- [x] Rebase branch onto current local master; fast-forward local master; run integrated full suite. Remove owned worktree and merged branch. Complete task 56 with `qqq --json complete 56`, verify `qqq --json show 56`, then resume one blocking `qqq --json next --wait --local` call.
