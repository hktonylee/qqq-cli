# Two-Panel Task TUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `qqq tui` with list panel above continuous task editor.

**Architecture:** Reuse existing DB, continuous-save callback, editor state machine,
and human task-tree formatter. Add panel row/scroll model, then draw list and
editor in separate terminal rectangles. Keep `qqq add` layout and behavior.

**Tech Stack:** Rust 2024, Clap, Crossterm, SQLite, Python PTY tests.

---

### Task 1: CLI contract and failing PTY test

**Files:** `src/main.rs`, `src/output.rs`, `tests/tui.rs`,
`tests/tui_dashboard_pty.py`

- [ ] **Step 1: Add PTY scenario.** Launch `[binary, "--json", "tui"]` with
  stdin/stderr on PTY and stdout pipe. Seed `First`, `Second`. Wait for
  `qqq tasks`, `First`, `Second`, and `qqq task editor - new task` on screen.
  Send Shift-Up, expect `task #2`, append text, Ctrl-S, expect `Saved #2. New
  task`; verify DB update. Save `Third`, verify list redraw; Esc, assert empty
  stdout and restored terminal. Run second no-save scenario: Esc immediately,
  expect success and empty stdout. Repeat exit with global `--json`.

```rust
#[test]
fn tui_dashboard_edits_and_adds_without_leaving_screen() {
    dashboard_scenario("save");
}
#[test]
fn tui_dashboard_empty_exit_succeeds() {
    dashboard_scenario("empty");
}
```

- [ ] **Step 2: Run red.**

```sh
cargo test --locked --test tui tui_dashboard -- --nocapture
```

Expected: CLI rejects unknown `tui` subcommand.

- [ ] **Step 3: Add command and output routing.** Add `Commands::Tui` to Clap.
  Route command through `Db::open(false)`, `tui::compose_dashboard`, and
  `Db::save_composition` callback. `run()` returns `Ok(None)` after execution
  for this command, including global `--json`; `Format::from` gets exhaustive
  match arm but never renders TUI. Re-run focused PTY test; it remains red until
  panel mode exists.

```rust
Commands::Tui => {
    tui::compose_dashboard(&mut db, &mut |db, outcome| {
        let task = db.save_composition(outcome.target_id, None, &outcome.composition)?;
        Ok(task.id)
    })?;
    Value::Null
}
```

### Task 2: List rows and split renderer

**Files:** Create `src/tui/panel.rs`; modify `src/tui/render.rs`,
`src/tui/mod.rs`; test `tests/tui_render.rs`.

- [ ] **Step 1: Add failing row/geometry tests.** Given task-tree output,
  `panel::rows` tags task lines by leading ID; wrapped continuation lines keep
  preceding ID. Verify selected row scrolls into viewport, new draft scrolls
  to end, and tiny terminal shows resize prompt. Verify lower editor cursor
  lands below divider and stays within editor body after scroll/resize.

```rust
let rows = panel::rows("ID     STATUS       TASK\n1      New          One\n                    detail\n2      New          Two");
assert_eq!(rows[2].task_id, Some(1));
assert_eq!(rows[3].task_id, Some(2));
assert_eq!(panel::scroll_to(&rows, Some(1), 1, 2), 1);
```

- [ ] **Step 2: Run red.**

```sh
cargo test --locked --test tui_render
```

Expected: missing `panel` module and split renderer.

- [ ] **Step 3: Build panel model.** `ListRow { text, task_id }` keeps rows
  from shared `qqq list` formatter. Parse ID only when row starts with ASCII
  digit; continuation rows inherit previous ID. `scroll_to` clamps viewport.

```rust
pub struct ListRow {
    pub text: String,
    pub task_id: Option<i64>,
}
pub fn rows(tree: &str) -> Vec<ListRow> {
    let mut task_id = None;
    tree.lines()
        .map(|line| {
            if line.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                task_id = line.split_whitespace().next().and_then(|value| value.parse().ok());
            }
            ListRow { text: line.to_owned(), task_id }
        })
        .collect()
}
pub fn scroll_to(rows: &[ListRow], selected: Option<i64>, top: usize, height: usize) -> usize {
    if height == 0 { return 0; }
    let last = rows.len().saturating_sub(height);
    let next = match selected.and_then(|id| rows.iter().position(|row| row.task_id == Some(id))) {
        Some(index) if index < top => index,
        Some(index) if index >= top.saturating_add(height) => index + 1 - height,
        Some(_) => top,
        None => last,
    };
    next.min(last)
}
```

- [ ] **Step 4: Draw split screen.** Extract editor painting from existing
  `render::draw` into shared rectangle painter. `draw_dashboard` clears screen,
  paints top title, clipped list rows, selection marker, divider, then paints
  lower editor with local viewport. Use height 8, width 12 minimum; show resize
  prompt below that. Keep existing full-screen `draw` output unchanged. Define
  split geometry as below; pass `editor_start` and `editor_height` to shared
  painter instead of full-screen origin and height.

```rust
let panel_height = height / 2;
let list_height = panel_height - 2; // title and divider
let editor_start = panel_height;
let editor_height = height - panel_height;
let editor_body_height = editor_height - 2; // title and footer
```

- [ ] **Step 5: Run green.**

```sh
cargo test --locked --test tui_render
```

Expected: row mapping, geometry, and existing render tests pass.

### Task 3: Connect dashboard editor and finish

**Files:** `src/tui/mod.rs`, `src/main.rs`, `README.md`, `tests/tui.rs`,
`tests/tui_dashboard_pty.py`.

- [ ] **Step 1: Extend editor mode.** Add dashboard flag to continuous mode.
  Before each dashboard draw, query `db.list(None)`, render plain task tree at
  panel width, map rows, then call `draw_dashboard`. Keep Shift navigation,
  switch confirmation, save reset, and failed-save retention unchanged.
  `cancel` returns success for dashboard even before first save; `qqq add`
  continues to return cancellation error before first save.

```rust
pub fn compose_dashboard(
    db: &mut crate::db::Db,
    save: &mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
) -> Result<()> {
    compose_inner("", Mode::Continuous { db, save, dashboard: true }).map(|_| ())
}
```

- [ ] **Step 2: Run focused PTY green.** Add explicit checks for selection
  marker scrolling, dirty switch, failed save retaining draft, and terminal
  resize. Check empty stdout in plain and `--json` modes.

```sh
cargo test --locked --test tui tui_dashboard -- --nocapture
```

- [ ] **Step 3: Document command.** Add short `qqq tui` example after Quick
  start. Explain two panels, Shift-Up/Down, Ctrl-S reset, exit, and saved-task
  silence on stdout. Keep `qqq add` documentation intact.

- [ ] **Step 4: Verify complete branch.**

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo package --locked --list
git diff --check
```

- [ ] **Step 5: Integrate.** Rebase on current `master` in this worktree,
  resolve overlap with task 49 list formatter, rerun checks, fast-forward
  `master`, rerun full tests, then remove this worktree and branch. Mark task
  51 complete only after integration.
