# Force Complete Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Offer explicit forced completion for dashboard tasks without a matching current-session claim.

**Architecture:** Resolve Complete versus Force Complete when the menu action activates. Reuse the existing confirmation flow; perform forced status change and audit insertion atomically in a separate DB method. Keep CLI completion and normal transactional ownership checks intact.

**Tech Stack:** Rust, rusqlite, Ratatui/Crossterm, Python PTY integration tests.

---

### Task 1: Reproduce Missing Force Prompt

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`.

- [x] Build isolated baseline at `5c97d0c`; full locked offline suite passes 593 tests across 38 targets.
- [x] Add force-completion scenarios to existing PTY harness. Seed a foreign active task, new task, error task, own task, and completed task; launch with session `worker` except sessionless/native variants. Initial regression checks the actual visible prompt before any database mutation:

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

- [x] Run `CARGO_TARGET_DIR=/private/tmp/qqq-155-target cargo test --locked --offline --test tui tui_dashboard_force_completion_requires_explicit_confirmation -- --exact`; expect failure because old prompt says Complete rather than Force complete.

### Task 2: Atomic Force Completion and Menu Routing

**Files:** `src/db.rs`, `src/tui/mod.rs`, `src/main.rs`, `tests/tui_db.rs`.

- [x] Add real database coverage for preservation across `new`, `in_progress`, and `error`; missing/completed rejection; event-trigger failure rollback. Snapshot `db.show(id)` before failures and compare afterward. For successful cases compare description, content revision, priority, archived, parent, prerequisites, messages, images and Herdr; check all claim metadata clears and exactly one `complete` event records actor.
- [x] Add DB operation beside ordinary complete:

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

- [x] Add `TaskAction::ForceComplete(i64)` to ID/success matches and use label `Force complete`. Add a separate completion resolver callback stored only for dashboard mode:

```rust
type CompletionHandler<'a> = dyn FnMut(&crate::db::Db, i64) -> Result<TaskAction> + 'a;
// Continuous mode field:
completion: Option<&'b mut CompletionHandler<'b>>,
```

Expose resolver through `compose_dashboard`, set `None` in continuous add, and call it on the selected `c` action. Propagate resolver errors through existing ActionUi error popup. Add force warning row before existing dirty warning and confirmation hint; wrap both with existing `wrap_modal` for compact terminals.

- [x] Wire resolver and mutation in main:

```rust
&mut |db, id| {
    let owner = match resolved_owner(session_input, project_dir, db) {
        Ok(owner) => Some(owner),
        Err(error)
            if error.downcast_ref::<errors::Info>().is_some_and(|info| {
                matches!(info.code, errors::Code::DispatchError)
                    || info
                        .details
                        .get("reason")
                        .and_then(Value::as_str)
                        .is_some_and(|reason| {
                            matches!(
                                reason,
                                "missing_project_agent"
                                    | "ambiguous_project_agent"
                                    | "missing_herdr_context"
                                    | "missing_herdr_pane"
                                    | "missing_agent_identity"
                                    | "missing_agent_kind"
                            )
                        })
            }) =>
        {
            None
        }
        Err(error) => return Err(error),
    };
    let owned = owner
        .as_ref()
        .map(|owner| {
            db.owned_with_name(&owner.key, overrides.harness_name.as_deref())
        })
        .transpose()?
        .flatten();
    Ok(if owned.is_some_and(|task| task.id == id) {
        tui::TaskAction::Complete(id)
    } else {
        tui::TaskAction::ForceComplete(id)
    })
},
// Existing mutation handler new arm:
tui::TaskAction::ForceComplete(id) => db.force_complete(id, session_input.unwrap_or("manual")),
```

- [x] Run regression and DB suites; expect all pass. Commit verified core feature using `[Feat] Offer Force Complete In Task Menu`.

### Task 3: Ownership Races, Cancellation and Documentation

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`, `docs/reference.md`.

- [x] Extend PTY coverage for explicit acceptance/cancellation, unsaved draft preservation, New/Error completion, owned normal completion, foreign owner transfer during normal prompt, sessionless/native alias operation, repeated completion rejection, and color/plain mode. A normal prompt with changed ownership must reject; reopen menu to receive force prompt.
- [x] Update existing `actions_basic` unclaimed branch to cancel force prompt, retaining later priority/parent/archive coverage:

```python
action("c", "Force complete task #4?")
send(b"n")
wait_visible(lambda: editor_line().startswith("XFresh item"))
assert cli("show", "4")["task"]["status"] == "new"
```

- [x] Extend Ctrl-G reference with force confirmation, unfinished-state scope and operator-attributed completion audit semantics; describe cancellation and normal ownership race protection.
- [x] Run `cargo fmt --all -- --check`, `git diff --check`, focused TUI/DB/CLI tests, full `cargo test --locked --offline --no-fail-fast`, and `cargo clippy --locked --offline --all-targets -- -D warnings`, using private target directory. Request read-only review, resolve findings, commit.

### Task 4: Local Integration and Queue Handoff

- [x] Rebase task checkout onto latest master. Rerun full checks if upstream source changed. Fast-forward only clean root master at expected base.
- [x] Install verified stable checkout with `CARGO_TARGET_DIR=/private/tmp/qqq-155-target cargo install --path . --locked --offline --force`; run installed force-completion PTY scenarios and ordinary CLI ownership rejection smoke.
- [x] Explicitly run `qqq complete 160 --json` and record actual evidence here.

After evidence integration, remove only the clean merged task checkout/branch and resume one blocking `qqq next --wait --local --json`.


## Verification Evidence

- Baseline at `5c97d0c`: build and 593 tests across 38 targets passed.
- Initial real PTY regression failed on ordinary Complete prompt for a foreign claim. New force behavior passed 169 focused CLI/TUI/DB tests. Standalone source `a74db6a` passed 598 full tests across 38 targets and strict all-target Clippy.
- Force completion uses the existing `complete` event, preserving schema constraints and completion ordering. Database tests cover every unfinished status, archived/content/revision/priority/parent/prerequisite/message/image/Herdr preservation, cleared ownership, missing/completed/blank-actor rejection, and rollback if audit insertion fails.
- Read-only review found owner-discovery errors were suppressed. A malformed unrelated active Herdr link reproduced the incorrect force prompt; discovery fallback now permits only known missing/unavailable identity cases. Regression verifies Action error with local draft and DB unchanged. Both force test groups then passed.
- Rebased onto concurrent Mark Error source `bcdb158` and evidence `acb71cc`, preserving its action, Clone enum, reason input, ownership checks and PTY scenarios. Force warning shares its confirmation helper. Combined source `9a0e1c7` passed 602 tests across 38 targets, zero failed or ignored (`/private/tmp/qqq-160-integrated-full.log`). Formatting, diff checks and strict all-target Clippy passed; combined read-only review found no issues.
- Clean local master fast-forwarded to the combined source. Install from stable task checkout succeeded (`/private/tmp/qqq-160-install.log`). Installed CLI passed six force scenarios (color/plain, sessionless/native alias, ownership transfer, owner-discovery DB error), both color/plain Mark Error success scenarios, and a direct CLI wrong-owner rejection followed by proper-owner completion (`/private/tmp/qqq-160-installed.log`). Installed version remains `qqq 0.4.0`.
- `qqq complete 160 --json` explicitly completed task at `2026-10-05T06:05:37.186Z`.
