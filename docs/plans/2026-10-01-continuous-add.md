# Continuous Interactive Add Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Save multiple tasks from one built-in `qqq add` editor session; clear draft after each Ctrl-S.

**Architecture:** Keep existing one-shot composer for edit and external input. Add continuous save mode to same TUI event loop; CLI supplies database callback, collects saved tasks, prints results after editor closes. Reuse existing task renderer for human batch output.

**Tech Stack:** Rust 2024, Crossterm, SQLite/rusqlite, Python PTY integration tests.

---

### Task 1: Prove continuous add fails before implementation

**Files:**
- Modify: `tests/tui.rs`
- Modify: `tests/tui_pty.py`

- [ ] **Step 1: Add PTY scenario.** In `tests/tui.rs`, add:

```rust
#[test]
fn tui_add_saves_multiple_tasks_without_leaving_editor() {
    scenario("continuous");
}
```

In `tests/tui_pty.py`, add `continuous` branch after initial editor draw:

```python
elif scenario == "continuous":
    send(b"First\x13")
    read_until(b"Saved #1. New task")
    assert child.poll() is None
    assert cli("show", "1")["task"]["description"] == "First"
    screen.clear()
    send(b"Second\x13")
    read_until(b"Saved #2. New task")
    assert cli("show", "2")["task"]["description"] == "Second"
    send(b"\x1b")
```

Expect JSON array with IDs `[1, 2]`, descriptions `["First", "Second"]`, successful exit, restored terminal. Existing single-save add scenarios must press Esc after saved footer and read first array element.

- [ ] **Step 2: Run red test.**

```sh
cargo test --locked --test tui tui_add_saves_multiple_tasks_without_leaving_editor -- --nocapture
```

Expected: old editor exits after first Ctrl-S; `Saved #1. New task` never appears.

### Task 2: Keep TUI open and reset draft after successful save

**Files:**
- Modify: `src/tui/mod.rs`
- Modify: `src/editor.rs`
- Test: `tests/tui.rs`, `tests/tui_pty.py`

- [ ] **Step 1: Refactor composer into single and continuous modes.** Add private mode and shared loop:

```rust
enum SaveMode<'a> {
    Single,
    Continuous(&'a mut dyn FnMut(Composition) -> Result<i64>),
}

pub fn compose(description: &str) -> Result<Composition> {
    compose_inner(description, SaveMode::Single)
        .map(|saved| saved.expect("single editor returns saved composition"))
}

pub fn compose_continuously(
    save: &mut dyn FnMut(Composition) -> Result<i64>,
) -> Result<()> {
    compose_inner("", SaveMode::Continuous(save)).map(|_| ())
}
```

Move current `compose` event loop to `compose_inner(description, mode) -> Result<Option<Composition>>`. On Ctrl-S, preserve current validation. `Single` returns composition. `Continuous` calls callback; on success set `saved_any = true`, `draft = Draft::new("")`, `top = 0`, `message = format!("Saved #{id}. New task")`; on callback error show error and keep draft. Route Ctrl-C, empty Esc, and confirmed discard through helper below so prior saves exit success while zero-save cancellation remains error:

```rust
fn cancel(saved_any: bool) -> Result<Option<Composition>> {
    if saved_any {
        Ok(None)
    } else {
        bail!("Editor cancelled; task not saved")
    }
}
```

- [ ] **Step 2: Expose editor selection.** In `src/editor.rs`, use same predicate for one-shot composer and CLI routing:

```rust
pub fn uses_builtin(external: bool) -> bool {
    !external && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}
```

- [ ] **Step 3: Run focused test.**

```sh
cargo test --locked --test tui tui_add_saves_multiple_tasks_without_leaving_editor -- --nocapture
```

Expected: still fails until CLI invokes continuous composer.

### Task 3: Save tasks, return batch output, preserve flags

**Files:**
- Modify: `src/main.rs`
- Modify: `src/output.rs`
- Modify: `tests/tui.rs`, `tests/tui_pty.py`

- [ ] **Step 1: Route built-in `add` through callback.** In `Commands::Add`, keep inline and external branches. For no inline description with `editor::uses_builtin(edit)`, read command-line images once, then:

```rust
let mut saved = Vec::new();
let mut first_images = images;
tui::compose_continuously(&mut |mut draft| {
    draft.images.extend(first_images.iter().cloned());
    let task = db.save_composition(None, parent, &draft)?;
    first_images.clear();
    let id = task.id;
    saved.push(task);
    Ok(id)
})?;
json!(saved)
```

Parent remains fixed. Clear command-line images only after successful save; pasted images disappear with draft reset.

- [ ] **Step 2: Render human batch.** In `Format::AddedTask` branch of `src/output.rs`, render array elements with existing `task(item, false, false)`, joined by blank lines. Keep object behavior for inline and external one-shot add. JSON serializer already emits array.

- [ ] **Step 3: Adapt old PTY scenarios.** For successful built-in add, wait for `Saved #1. New task`, assert process still alive, press Esc, then inspect first element of JSON array. Leave edit, external, cancellation behavior unchanged. Add separate `continuous_flags` scenario with `--parent` and `--image`, proving parent on both saved tasks and image only on first.

- [ ] **Step 4: Run focused tests.**

```sh
cargo test --locked --test tui
cargo test --locked --test editor
```

Expected: all pass.

### Task 4: Docs, full verification, integration

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document interaction.** State built-in `qqq add` saves each Ctrl-S and clears draft; Esc/Ctrl-C ends session, prior saves remain. Document `--parent` applies to each task, supplied `--image` applies to first, and interactive JSON result is array.

- [ ] **Step 2: Run gates.**

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
git diff --check
```

Expected: all pass. Review diff for saved-task duplication, terminal restoration, and task 46 overlap.

- [ ] **Step 3: Commit, rebase, integrate.** Commit focused feature with bracketed Title Case subject. Rebase worktree onto current master after task 46 merges; resolve shared TUI edits without losing either feature. Fast-forward master, rerun relevant gates, mark task 47 complete through `qqq` from project root, verify status, remove owned worktree.
