# Force Complete Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Offer explicit forced completion for dashboard tasks without a matching current-session claim.

**Architecture:** Resolve Complete versus Force Complete when the menu action activates. Reuse the existing confirmation flow; perform forced status change and audit insertion atomically in a separate DB method. Keep CLI completion and normal transactional ownership checks intact.

**Tech Stack:** Rust, rusqlite, Ratatui/Crossterm, Python PTY integration tests.

---

### Task 1: Reproduce Missing Force Prompt

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`.

- [x] Build isolated baseline at `5c97d0c`; full locked offline suite passes 593 tests across 38 targets.
- [ ] Add force-completion scenarios to existing PTY harness. Seed a foreign active task, new task, error task, own task, and completed task; launch with session `worker` except sessionless/native variants. Initial regression checks the actual visible prompt before any database mutation:

```python
click(5, task_row("Foreign item"))
wait_visible(lambda: "Task #1" in editor_title())
send(b"\x07c")
wait_visible(lambda: "Force complete task #1?" in visible.text())
assert cli("show", "1")["task"]["status"] == "in_progress"
send(b"n")
wait_visible(lambda: "Task actions" not in visible.text())
assert cli("show", "1")["task"]["status"] == "in_progress"
```

```rust
#[test]
fn tui_dashboard_force_completion_requires_explicit_confirmation() {
    dashboard_scenario("force_complete");
    dashboard_scenario("force_complete_no_color");
}
```

- [ ] Run `CARGO_TARGET_DIR=/private/tmp/qqq-155-target cargo test --locked --offline --test tui tui_dashboard_force_completion_requires_explicit_confirmation -- --exact`; expect failure because old prompt says Complete rather than Force complete.

### Task 2: Atomic Force Completion and Menu Routing

**Files:** `src/db.rs`, `src/tui/mod.rs`, `src/main.rs`, `tests/tui_db.rs`.

- [ ] Add real database coverage for preservation across `new`, `in_progress`, and `error`; missing/completed rejection; event-trigger failure rollback. Snapshot `db.show(id)` before failures and compare afterward. For successful cases compare description, content revision, priority, archived, parent, prerequisites, messages, images and Herdr; check all claim metadata clears and exactly one `complete` event records actor.
- [ ] Add DB operation beside ordinary complete:

```rust
pub fn force_complete(&mut self, id: i64, actor: &str) -> Result<Task> {
    nonempty(actor, "Actor")?;
    let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure!(
        tx.execute("UPDATE tasks SET status='completed',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status IN ('new','in_progress','error')", [id])? == 1,
        transition_error(&tx, id, &["new", "in_progress", "error"], None,
            format!("Task {id} must be unfinished to force complete"))?
    );
    tx.execute("INSERT INTO events(task_id,session,action) VALUES (?, ?, 'complete')", params![id, actor])?;
    let task = tx.query_row(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"), [id], task_row)?;
    tx.commit()?;
    Ok(task)
}
```

- [ ] Add `TaskAction::ForceComplete(i64)` to ID/success matches and use label `Force complete`. Add a separate completion resolver callback stored only for dashboard mode:

```rust
type CompletionHandler<'a> = dyn FnMut(&crate::db::Db, i64) -> Result<TaskAction> + 'a;
// Continuous mode field:
completion: Option<&'b mut CompletionHandler<'b>>,
```

Expose resolver through `compose_dashboard`, set `None` in continuous add, and call it on the selected `c` action. Propagate resolver errors through existing ActionUi error popup. Add force warning row before existing dirty warning and confirmation hint; wrap both with existing `wrap_modal` for compact terminals.

- [ ] Wire resolver and mutation in main:

```rust
&mut |db, id| {
    let owner = resolved_owner(session_input, project_dir, db).ok();
    let owned = owner.as_ref().map(|owner| {
        db.owned_with_name(&owner.key, overrides.harness_name.as_deref())
    }).transpose()?.flatten();
    Ok(if owned.is_some_and(|task| task.id == id) {
        tui::TaskAction::Complete(id)
    } else {
        tui::TaskAction::ForceComplete(id)
    })
},
// Existing mutation handler new arm:
tui::TaskAction::ForceComplete(id) => db.force_complete(id, session_input.unwrap_or("manual")),
```

- [ ] Run regression and DB suites; expect all pass. Commit verified core feature using `[Feat] Offer Force Complete In Task Menu`.

### Task 3: Ownership Races, Cancellation and Documentation

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`, `docs/reference.md`.

- [ ] Extend PTY coverage for explicit acceptance/cancellation, unsaved draft preservation, New/Error completion, owned normal completion, foreign owner transfer during normal prompt, sessionless/native alias operation, repeated completion rejection, and color/plain mode. A normal prompt with changed ownership must reject; reopen menu to receive force prompt.
- [ ] Update existing `actions_basic` unclaimed branch to cancel force prompt, retaining later priority/parent/archive coverage:

```python
action("c", "Force complete task #4?")
send(b"n")
wait_visible(lambda: editor_line().startswith("XFresh item"))
assert cli("show", "4")["task"]["status"] == "new"
```

- [ ] Extend Ctrl-G reference with force confirmation, unfinished-state scope and operator-attributed completion audit semantics; describe cancellation and normal ownership race protection.
- [ ] Run `cargo fmt --all -- --check`, `git diff --check`, focused TUI/DB/CLI tests, full `cargo test --locked --offline --no-fail-fast`, and `cargo clippy --locked --offline --all-targets -- -D warnings`, using private target directory. Request read-only review, resolve findings, commit.

### Task 4: Local Integration and Queue Handoff

- [ ] Rebase task checkout onto latest master. Rerun full checks if upstream source changed. Fast-forward only clean root master at expected base.
- [ ] Install verified stable checkout with `CARGO_TARGET_DIR=/private/tmp/qqq-155-target cargo install --path . --locked --offline --force`; run installed force-completion PTY scenarios and ordinary CLI ownership rejection smoke.
- [ ] Explicitly run `qqq complete 160 --json`, record actual evidence here, integrate docs, remove only clean merged task checkout/branch, resume one blocking `qqq next --wait --local --json`.
