# Interactive Add Editor Task Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Shift+Up/Down in built-in `qqq add` editor loads adjacent tasks, checks dirty content, saves loaded task or new draft.

**Architecture:** `Db` queries ID neighbors; `tui` owns navigation/confirmation state, returning selected task ID with composition. Existing `save_composition` handles update versus create. Render shows active target.

**Tech Stack:** Rust, rusqlite, crossterm, Python PTY integration tests.

---

### Task 1: Compare draft against loaded baseline

**Files:** Modify `src/tui/draft.rs`; test `tests/tui_model.rs`.

- [ ] Add draft-model test before implementation:

```rust
#[test]
fn dirty_check_ignores_cursor_motion_and_reverted_edits() {
    let mut draft = Draft::new("Saved");
    assert!(!draft.is_dirty_against("Saved"));
    draft.left();
    assert!(!draft.is_dirty_against("Saved"));
    draft.insert("!");
    assert!(draft.is_dirty_against("Saved"));
    draft.backspace();
    assert!(!draft.is_dirty_against("Saved"));
}
```

Add image test using `ImageInput { name: "x.png".into(), data: b"\x89PNG\r\n\x1a\nbytes".to_vec() }`: insertion makes dirty, backspace restores clean.

- [ ] Run `CARGO_TARGET_DIR=/private/tmp/qqq-task46-target cargo test --locked --test tui_model dirty_check`; expect compile failure because method is absent.
- [ ] Extract non-validating composition construction from `Draft::finish` into private `contents() -> Composition`; implement `is_dirty_against(&self, baseline: &str) -> bool` as `contents.description != baseline || !contents.images.is_empty()`. `finish` retains blank-body and image validation.
- [ ] Run `cargo test --locked --test tui_model` with task target dir; expect pass.

### Task 2: Prove interactive navigation through real terminal

**Files:** Create `tests/tui_history_pty.py`; modify `tests/tui.rs`.

- [ ] Add PTY scenarios that send `\x1b[1;2A` for Shift+Up and `\x1b[1;2B` for Shift+Down. Create fixture DB using `qqq --json init`, then seed tasks through CLI. Assertions:

```python
# After seeding First, Second, Third, launch `qqq --json add` in PTY.
send(b"\x1b[1;2A")  # Third
read_until(b"task #3")
send(b"\x1b[1;2A")  # Second
read_until(b"task #2")
send(b"\x1b[1;2B")  # Third
read_until(b"task #3")
send(b" updated\x13")
assert json.loads(stdout)["id"] == 3
assert cli("list")[2]["description"] == "Third updated"
assert len(cli("list")) == 3
```

Additional scenarios: Shift+Down from newest returns new draft and creates task; dirty new draft prompts, N/Esc keep it, Y discards; dirty loaded task prompts before older navigation; empty queue and oldest boundary keep draft; normal Up/Down and `qqq edit` remain cursor-only. Check JSON stdout, terminal restoration, task status/owner/parent unchanged.

- [ ] Run `CARGO_TARGET_DIR=/private/tmp/qqq-task46-target cargo test --locked --test tui tui_add_history`; expect red because Shift+Up moves cursor and add creates new task.

### Task 3: Implement target selection and save

**Files:** Modify `src/db.rs`, `src/tui/mod.rs`, `src/tui/render.rs`, `src/editor.rs`, `src/main.rs`, `tests/tui_render.rs`.

- [ ] Add `Db::adjacent_description(current: Option<i64>, older: bool) -> Result<Option<(i64, String)>>`. Older query:

```sql
SELECT id,description FROM tasks
WHERE (?1 IS NULL OR id < ?1)
ORDER BY id DESC LIMIT 1
```

Newer query uses `WHERE id > ?1 ORDER BY id ASC LIMIT 1`. `OptionalExtension` turns no row into `None`.

- [ ] Add `tui::Outcome { composition: Composition, target_id: Option<i64> }`; change `editor::compose(description, external, navigation: Option<&Db>) -> Result<Outcome>`. External editor returns target `None`; `qqq edit` passes `None`. Built-in `qqq add` passes `Some(&db)` and calls `db.save_composition(outcome.target_id, parent, &outcome.composition)`.
- [ ] In TUI, retain `target_id`, loaded `baseline`, pending destination, and confirmation mode. On Shift+Up/Down with navigation enabled, query destination first. No destination means footer boundary message and no prompt. If dirty, show `Discard changes and switch? (y/N)`; Y loads destination, N/Enter/Esc keeps draft, Ctrl-C cancels. Loading sets `Draft::new(description)`, target ID, baseline, cursor, and scroll top. Ctrl-S returns `Outcome` with current target ID. DB query errors show footer and keep draft.
- [ ] Render target in header: `qqq task editor - new task` or `qqq task editor - task #ID`. Add navigation keys to footer for `qqq add`; retain original header/keys for `qqq edit`. Update render tests for revised draw signature.
- [ ] Run PTY navigation tests; expect pass. Run existing editor, TUI model, render, and PTY suites; expect pass.

### Task 4: Docs, checks, integration

**Files:** Modify `README.md` and CLI help in `src/main.rs` if needed.

- [ ] Document Shift+Up/Down order, new-draft return, confirmation, and save semantics in README.
- [ ] Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --quiet` using task target dir; expect pass.
- [ ] Commit code, tests, docs with `[Feat] Navigate Tasks In Add Editor`.
- [ ] Request read-only code review; address verified issues and rerun relevant checks.
- [ ] Rebase task branch onto current master, fast-forward master, run full tests on integrated tree with isolated target dir, remove task worktree, delete merged branch.
- [ ] Complete queue task 46 with `qqq --json complete 46`; verify `qqq show 46 --json` reports completed.
