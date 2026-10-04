# Mark Error Menu Action Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mark an owned in-progress task error from Ctrl-G with a required reason and confirmation.

**Architecture:** Extend existing action/input enums and popup rows. Carry the reason in a cloned action, reuse resolved owner identity and atomic DB error transition. Preserve existing guards and action discard behavior.

**Tech Stack:** Rust, Crossterm, Ratatui, SQLite, Python PTY tests.

---

## Task 1: Missing Action Regression

**Modify:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`.

- [ ] Add PTY scenarios `menu_error_success`, `menu_error_success_no_color`,
  `menu_error_dirty`, `menu_error_rejected`, `menu_error_live`,
  `menu_error_new` and `menu_error_narrow`. Reuse owner fixtures and launch
  the TUI with explicit worker session; rejected scenario uses another session.
  Core assertion:

```python
click(5, task_row("Owned item"))
wait_visible(lambda: "Task #1 (In progress)" in editor_title())
send(b"\x07")
wait_visible(lambda: "e Mark error" in visible.text())
send(b"e")
wait_visible(lambda: "Error task #1" in visible.text())
send(b"Worker failed\r")
wait_visible(lambda: "Mark error task #1?" in visible.text())
assert cli("show", "1")["task"]["status"] == "in_progress"
send(b"y")
wait_visible(lambda: "Task #1 (Error)" in editor_title()
             and visible.text().splitlines()[-1].startswith("Marked error #1"))
detail = cli("show", "1")
assert detail["messages"][-1]["body"] == "Worker failed"
assert detail["events"][-1]["action"] == "error"
```

- [ ] Add Rust tests calling those scenarios; run
  `cargo test --offline --test tui tui_dashboard_mark_error_requires_reason_and_confirmation -- --exact`.
  Expect missing `e Mark error` before production changes.

## Task 2: Action And Reason Input

**Modify:** `src/tui/mod.rs`, `src/main.rs`.

- [ ] Replace `TaskAction` Copy with Clone, add `MarkError(i64, String)`.
  Make `id`, `label`, `success` borrow `&self`; dereference returned ID.
  Add labels `Mark error` and `Marked error #ID`. Append menu row:

```rust
('e', "Mark error"),
```

- [ ] Add `ActionInputKind::ErrorReason`, heading `Error task #ID`, hint
  `Enter error reason`. Add parser branch:

```rust
ActionInputKind::ErrorReason => {
    let reason = value.trim();
    if reason.is_empty() {
        return Err("Error reason cannot be empty".into());
    }
    Ok(TaskAction::MarkError(id, reason.to_owned()))
}
```

- [ ] `e` opens the existing input view with empty value/error. In pasted
  action input, map controls to spaces only for ErrorReason:

```rust
value.extend(text.chars().filter_map(|ch| {
    if ch.is_control() {
        matches!(kind, ActionInputKind::ErrorReason).then_some(' ')
    } else {
        Some(ch)
    }
}));
```

- [ ] Valid MarkError input always creates `Confirmation::Action`; other
  inputs keep current dirty-only confirmation guard:

```rust
Ok(action)
    if matches!(action, TaskAction::MarkError(..))
        || draft.is_dirty_against(&baseline.description) => {
    confirmation = Some(Confirmation::Action {
        action,
        dirty: draft.is_dirty_against(&baseline.description),
    });
}
```

- [ ] Preserve existing confirmation rows. Insert up to three wrapped
  `Reason: ...` Hint rows for MarkError before warning/footer. Pass an action
  clone into both `run_action` call sites so success text still has its reason
  carrier available. No editor/draft writes while prompting or rejected.

- [ ] Add main action handler branch:

```rust
tui::TaskAction::MarkError(id, reason) => {
    let owner = resolved_owner(session_input, project_dir, db)?;
    db.edit_with_priority(
        id,
        None,
        Some(db::EditTransition::Error {
            session: &owner.key,
            reason: &reason,
            harness_name: overrides.harness_name.as_deref(),
        }),
        &[],
        None,
        None,
    )
}
```

- [ ] Run `cargo fmt --all`, all new mark-error PTY tests and existing
  menu/retry/popup tests. Update intended row-count geometry and arrow wrap
  expectations for appended action while retaining background checks.
- [ ] Commit verified feature with `[Feat] Add Mark Error Menu Action`.

## Task 3: Docs And Completion

**Modify:** `README.md`, `docs/reference.md`, this plan.

- [ ] Document `e` reason prompt, required confirmation, ownership/state
  guard, reason history and existing dirty-draft discard warning.
- [ ] Run `cargo fmt --all --check`, `git diff --check`,
  `cargo test --offline --all-targets --no-fail-fast`,
  `cargo clippy --offline --all-targets -- -D warnings`.
- [ ] Request read-only review; resolve concrete findings, rebase current
  master, verify combined source if production changed, fast-forward locally.
- [ ] Install `cargo install --path . --locked --offline --force`. Run seven
  new scenarios plus retry, existing menu/action, jump and compact/wide flows
  against absolute installed binary path.
- [ ] Record evidence with `qqq message 161`, explicitly complete from root
  workspace, commit final check record, clean owned worktree/branch, resume one
  persistent `qqq next --wait --local --json`.
