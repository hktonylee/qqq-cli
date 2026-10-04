# Completed Filter Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Toggle completed-task visibility using a stateful button at the TUI filter bar's right edge.

**Architecture:** Keep visibility in dashboard session state. Shared pane geometry defines button rendering and mouse hits. Apply status exclusion before query/ancestor matching, so list rendering and navigation use identical IDs. Preserve editor/draft state throughout.

**Tech Stack:** Rust, Ratatui, Crossterm, existing model/render tests and Python PTY harness.

---

### Task 1: Capture missing button

**Files:** `tests/tui_dashboard_render.rs`.

- [x] Add focused-filter render regression using existing `View`, with long Unicode query. Assert right-edge checked button and caret before button.

```rust
let button = line(terminal.backend().buffer(), 0);
assert!(button.ends_with("[✓ Completed]"), "{button}");
assert!(terminal.get_cursor_position().unwrap().x < 58);
```

- [x] Run `cargo test --locked --test tui_dashboard_render completed_filter_button`; confirm assertion fails because no button exists.

### Task 2: State, filtering and geometry

**Files:** `src/tui/panel.rs`, `src/tui/dashboard.rs`, `src/tui/mod.rs`, `src/tui/render.rs`; existing view/model fixtures in `tests/tui_dashboard_render.rs`, `tests/tui_filter.rs`.

- [x] Extend `FilterTask` with `status: &str`; extend `filter_tasks` with `show_completed: bool`. Exclude completed tasks from matching and ancestor positions when unchecked. Keep eligible children whose parent is excluded. Update existing fixtures to `status: "new"` and current calls to enabled visibility.

```rust
let positions: HashMap<_, _> = tasks.iter().enumerate()
    .filter(|(_, task)| show_completed || task.status != "completed")
    .map(|(index, task)| (task.id, index)).collect();
for task in tasks.iter().filter(|task| show_completed || task.status != "completed") {
    // Existing description matching and parent walk use these eligible positions.
}
```

- [x] Share bar/button geometry in dashboard, add `View.show_completed` and `ClickTarget::ToggleCompleted`. Existing view fixtures use `true`.

```rust
pub fn filter_visible(query: &str, focused: bool, show_completed: bool) -> bool {
    focused || !query.is_empty() || !show_completed
}
pub fn completed_button(list: Rect) -> Rect {
    let width = if list.width >= 24 { 13 } else { 3 };
    Rect::new(list.x + list.width.saturating_sub(width), list.y, width, 1)
}
```

- [x] Reserve button width plus one gap before drawing filter query. Use `[✓ Completed]` / `[× Completed]`, compact `[✓]` / `[×]` below 24 columns. `filter_line` uses `F: ` when query area is below 10 cells; reserve one query caret cell. Keep button text readable in color/plain mode.

```rust
let label = if width >= 10 { "Filter: " } else { "F: " };
let (tail, used) = text_tail(query, width.saturating_sub(label.len() + 1));
let cursor = (label.len() + used).min(width.saturating_sub(1)) as u16;
```

- [x] Start `show_completed = true` in editor session. Pass status/visibility into panel filtering and draw state. Use shared `filter_visible` result for wheel/click routing. Mouse button and focused Ctrl-T flip visibility, reset list scroll/follow, preserve all editor/draft/query state. Add `Ctrl-T Completed` to filter shortcut hints. Empty status-filter result uses `No matching tasks.`.

```rust
show_completed = !show_completed;
list_top = 0;
list_follow_selected = true;
```

- [x] Run `cargo test --locked --test tui_filter --test tui_dashboard_render`; verify baseline and new regression pass. Adapt old exact query/style expectations for reserved button area, without weakening caret/geometry checks.

### Task 3: Product regressions and integration

**Files:** `tests/tui_filter.rs`, `tests/tui_dashboard_render.rs`, `tests/tui_dashboard_pty.py`, `tests/tui.rs`, `docs/reference.md`.

- [x] Cover checked/unchecked, query interaction, completed ancestor exclusion, active child retention, navigation, exact hit cells, collapsed/active bar, 12/23/24/50/72/150-column layouts and Unicode query caret.
- [x] Add color/plain PTY scenarios: create completed parent and unfinished child; open/dirty completed task; click toggle; assert completed rows absent while draft/selection survive; query and keyboard toggle; switch visible tasks and restore dirty draft; external completion respects hidden state; resize and toggle back; DB remains unchanged except explicit fixture mutations. Wait for complete visible states before assertions.
- [x] Update old PTY query assertions to inspect query segment before button. Keep existing filter shortcuts, draft navigation, mouse routing and Escape/Ctrl-C checks.
- [x] Document click state, Ctrl-T, collapse rule, compact button, session-only behavior and retained drafts.
- [x] Run `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check`, `git diff --check`; request read-only review.
- [ ] Commit `[UI] Add Completed Visibility Toggle`, rebase current master, verify combined tree, fast-forward locally, install using this worktree's own target, run installed color/plain scenarios. Record evidence, complete #158, clean worktree, resume `qqq next --wait --local --json`.

## Validation before integration

- Baseline: 37 model/render checks passed. New checked-button regression failed because no button was rendered; now passes.
- Model/render checks: 41 passed, including strict completed-ancestor exclusion, unfinished-child retention, visible navigation, query interaction, Unicode caret boundaries, exact hit regions, compact/full buttons and color/plain styles at 12/13/23/24/50/72/150 columns.
- Color/plain terminal scenarios passed: click and Ctrl-T, active-filter collapse, retained new/task drafts, hidden completed selection, external child completion, resizing and unchanged saved descriptions. Assertions wait for expected final caret/frame and Escape processing.
- Existing wide-layout query-only expectation now reads query segment before button; focused resize scenario passed.
- Full locked suite: 586 passed. Strict all-target Clippy, fmt and diff checks passed. Read-only review approved with no findings.
