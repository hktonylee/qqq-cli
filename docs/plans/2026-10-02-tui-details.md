# TUI Details Pane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Give selected tasks separate details/messages pane; keep new-draft editor twice list height.

**Architecture:** Shared pane rectangles drive rendering and mouse routing. Pure details formatter consumes selected task plus typed DB messages. TUI owns snapshot and independent scroll offset; existing DB version refresh keeps detail content current without replacing drafts.

**Tech Stack:** Rust, SQLite/rusqlite, Ratatui, Crossterm, existing Unicode layout helper, Python PTY fixtures.

---

### Task 1: Geometry and Details Data

**Files:** `src/tui/dashboard.rs`, `src/tui/details.rs`, `src/db.rs`, `tests/tui_dashboard_render.rs`, `tests/tui_db.rs`.

- [x] Add failing geometry tests. At 72x18, expect list `(0,0,72,6)`; new editor `(0,6,72,12)`; selected details `(0,6,72,6)`, editor `(0,12,72,6)`. At 12x8, each interactive region has usable content. Assert borders and details clicks excluded from editor hit tests.

```rust
let panes = dashboard::panes(Rect::new(0, 0, 72, 18), true);
assert_eq!(panes.list.height, 6);
assert_eq!(panes.details.unwrap().height, 6);
assert_eq!(panes.editor.y, 12);
assert_eq!(panes.editor.height, 6);
```

- [x] Run `cargo test --locked --test tui_dashboard_render`; confirm missing geometry behavior.
- [x] Introduce `Panes { list: Rect, details: Option<Rect>, editor: Rect }`; replace 50/50 geometry. Use following height contract, shared by drawing and hit tests:

```rust
let list_height = (height / 3).max(4).min(height.saturating_sub(3));
let details_height = if selected {
    (height / 3).min(height.saturating_sub(list_height + 3))
} else { 0 };
```

- [x] Add typed DB message read test with multiline body, author and ordering. Define `TaskMessage { id: i64, body: String, session: Option<String>, created_at: String }`, deriving `Serialize`; `Db::task_messages(id)` reads existing message rows ordered by ID. Have `show` reuse method while retaining JSON contract.
- [x] Add formatter tests: selected status/priority/parent, latest-message-first ordering, author/time, no messages, public metadata, escaped terminal controls, Unicode wrapping. Formatter `details::rows(task: &Task, messages: &[TaskMessage], width: usize) -> Vec<String>` uses existing `render::Layout` for safe wrapping.
- [x] Run `cargo test --locked --test tui_db --test tui_render --test tui_dashboard_render`; confirm relevant tests pass. Commit `[Feat] Add TUI Details Data And Pane Geometry`.

### Task 2: Rendering and Input Routing

**Files:** `src/tui/dashboard.rs`, `src/tui/mod.rs`, `src/tui/render.rs`, `tests/tui_dashboard_render.rs`.

- [x] Add buffer-render tests for new-draft 1/3+2/3 layout and selected-task thirds. Assert details content and bottom editor cursor/style, no details in new draft, plain-color behavior, small/resize states and scroll clamping.
- [x] Keep draw argument count manageable: selection remains a draw argument; `View` groups pane state; introduce `DetailsView { rows: &[String], top: &mut usize }`. Pass optional details view to dashboard drawing. Hit tests use shared `Panes` plus selected-state input; group offsets/editor layout into hit-test state if signature exceeds seven args.
- [x] Read selected task/messages during dashboard snapshot. Generate unavailable rows when selected ID absent; DB read failures propagate. Track `details_id`, `details_top`, row count; reset only when selected ID changes. Render details before editor and preserve modal/filter cursor behavior.
- [x] Route details mouse wheel to its own offset. Add PgUp/PgDn after confirmation/action handling, before normal editor keys; consume only when task selected and filter unfocused. Use pane content height for page movement and clamp to available rows. Details clicks stay read-only.
- [x] Run focused render/data tests and `cargo clippy --locked --all-targets -- -D warnings`. Commit `[Feat] Render Selected Task Details And Messages`.

### Task 3: Real Terminal Verification and Delivery

**Files:** `tests/tui_dashboard_pty.py`, `tests/tui.rs`, `README.md`.

- [x] Introduce terminal-fixture helpers that locate editor heading/body dynamically. Adapt existing mouse/editor coordinates and resize assertions to shared layout behavior; keep assertions on actual terminal contents and DB data.
- [x] Add failing PTY scenarios before connecting missing behaviors: selected details/messages, new-draft expanded editor, independent details scroll, live message/status refresh with dirty draft, selection reset/no stale details, deleted selected task and no messages. Verify Unicode/control safety through focused formatter tests where screen emulator supports ASCII only.
- [x] Run `cargo test --locked --test tui`; investigate failures, preserving assertions rather than bypassing scenarios.
- [x] Update README task-TUI section with thirds, new-draft expansion, details content and PgUp/PgDn/wheel scrolling.
- [ ] Run `cargo fmt --check`, Clippy and full `cargo test --locked`; review diff through requesting-code-review skill. Report any intermittent PTY failures separately from fresh passing evidence.
- [ ] Commit completed work; use finishing-a-development-branch workflow to rebase onto current `master`, resolve conflicts in worktree, fast-forward local master and verify integrated result. Refresh installed CLI, smoke test, complete qqq task #89, clean owned worktree/branch, resume blocking `qqq next --wait --local --json`.

## Verification Record

Geometry/data checkpoint committed as `ff8a361`. Focused formatter, DB, render
and geometry checks: 55 passed. Full TUI suite: 63 passed. Clippy all targets
with warnings denied and formatting check passed. Initial new PTY assertions
observed partial redraws; predicates now wait for stale details to clear. Full
suite: 435 passed across 35 test binaries. Independent review pending.
