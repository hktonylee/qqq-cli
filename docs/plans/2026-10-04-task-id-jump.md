# Ctrl+L Task ID Jump Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Ctrl+L opens a task ID popup and navigates safely without saving or discarding drafts.

**Architecture:** A small `jump` module owns input validation and typed popup rows. Dashboard integration routes modal events before editor/filter cancellation, then reuses snapshot loading and retained draft restoration. Archived navigation enables the existing archived-list mode.

**Tech Stack:** Rust, Crossterm, Ratatui, SQLite, Python PTY regression tests.

---

## Task 1: Reproduce Missing Shortcut

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`.

- [x] Add `jump`, `jump_no_color`, `jump_filter`, `jump_drafts`, `jump_archived` and `jump_narrow` scenarios. Reuse the existing isolated DB, terminal emulator, visible-state waits and terminal cleanup checks. Core missing-feature assertion:

```python
send(b"\x0c")
wait_visible(lambda: "Go to task" in visible.text())
send(b"2\r")
wait_visible(lambda: "Task #2 (New)" in editor_title()
             and editor_line().startswith("Second")
             and (visible.x, visible.y) == (6, editor_row() + 1))
assert cli("list") == initial_tasks
```

Use separate Rust tests for basic/color input, filter/draft/archive navigation,
and compact-terminal behavior:

```rust
#[test]
fn tui_dashboard_task_id_jump_input_and_color() {
    for name in ["jump", "jump_no_color"] {
        dashboard_scenario(name);
    }
}
#[test]
fn tui_dashboard_task_id_jump_preserves_drafts_and_reveals_targets() {
    for name in ["jump_filter", "jump_drafts", "jump_archived"] {
        dashboard_scenario(name);
    }
}
#[test]
fn tui_dashboard_task_id_jump_works_in_compact_terminal() {
    dashboard_scenario("jump_narrow");
}
```

- [x] Run `cargo test --offline --test tui tui_dashboard_task_id_jump_input_and_color -- --exact`. Expect missing `Go to task` visible state before production changes.

## Task 2: Input Controller

**Create:** `src/tui/jump.rs`. **Modify:** module declaration in `src/tui/mod.rs`.

- [x] Create the controller below, with module-local tests covering valid IDs
  `1`, whitespace around `42`, `i64::MAX`; invalid empty/zero/negative/text/
  overflow/multiple IDs; Esc/Ctrl+C cancellation; Ctrl+U; Backspace; paste
  control replacement; Heading/Hint/Input/Error row roles.

```rust
use super::render::{PopupKind, PopupRow};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, PartialEq)]
pub(super) enum Action {
    Go(i64),
    Cancel,
}

#[derive(Default)]
pub(super) struct View {
    value: String,
    error: String,
}

impl View {
    pub fn paste(&mut self, text: &str) {
        self.value.extend(text.chars().map(|ch| if ch.is_control() { ' ' } else { ch }));
        self.error.clear();
    }
    pub fn set_error(&mut self, error: String) {
        self.error = error;
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        let control = key.modifiers == KeyModifiers::CONTROL;
        if key.code == KeyCode::Esc || (control && key.code == KeyCode::Char('c')) {
            return Some(Action::Cancel);
        }
        if control && key.code == KeyCode::Char('u') {
            self.value.clear();
            self.error.clear();
            return None;
        }
        if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
            return None;
        }
        match key.code {
            KeyCode::Enter => match self.value.trim().parse::<i64>() {
                Ok(id) if id > 0 => return Some(Action::Go(id)),
                _ => self.error = "Enter a positive task ID".into(),
            },
            KeyCode::Backspace => {
                if let Some((index, _)) = self.value.grapheme_indices(true).next_back() {
                    self.value.truncate(index);
                }
                self.error.clear();
            }
            KeyCode::Char(ch) if !ch.is_control() => {
                self.value.push(ch);
                self.error.clear();
            }
            _ => (),
        }
        None
    }
    pub fn rows(&self, width: usize) -> Vec<PopupRow> {
        let mut rows = vec![
            PopupRow::new("Go to task", PopupKind::Heading),
            PopupRow::new("Enter task ID", PopupKind::Hint),
            PopupRow::new(format!("> {}", self.value), PopupKind::Input),
        ];
        rows.extend(super::wrap_modal(&self.error, width).into_iter()
            .map(|text| PopupRow::new(text, PopupKind::Error)));
        rows.push(PopupRow::new("Enter go  Esc cancel", PopupKind::Hint));
        rows
    }
}
```

- [x] Add `mod jump;`; run `cargo test --offline --bin qqq tui::jump`.

Controller tests (inside `jump.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent { KeyEvent::new(code, KeyModifiers::NONE) }
    fn ctrl(ch: char) -> KeyEvent { KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL) }
    #[test]
    fn validates_positive_task_ids() {
        for (text, id) in [("1", 1), (" 42 ", 42), ("9223372036854775807", i64::MAX)] {
            let mut view = View::default();
            view.paste(text);
            assert_eq!(view.key(key(KeyCode::Enter)), Some(Action::Go(id)));
        }
        for text in ["", "0", "-1", "abc", "9223372036854775808", "1 2"] {
            let mut view = View::default();
            view.paste(text);
            assert_eq!(view.key(key(KeyCode::Enter)), None);
            assert_eq!(view.error, "Enter a positive task ID");
        }
    }
    #[test]
    fn edits_graphemes_and_keeps_modal_keys_and_paste_local() {
        let mut view = View::default();
        view.paste("7e\u{301}");
        view.key(key(KeyCode::Backspace));
        assert_eq!(view.value, "7");
        view.key(ctrl('u'));
        view.paste("1\n2\x1b\t");
        assert_eq!(view.value, "1 2  ");
        assert_eq!(view.key(ctrl('s')), None);
        assert_eq!(view.key(ctrl('c')), Some(Action::Cancel));
        assert_eq!(view.key(key(KeyCode::Esc)), Some(Action::Cancel));
        assert_eq!(view.value, "1 2  ");
    }
    #[test]
    fn exposes_existing_popup_roles_and_clears_errors_on_edit() {
        let mut view = View::default();
        view.key(key(KeyCode::Enter));
        assert!(view.rows(80).iter().map(|row| row.kind).eq([
            PopupKind::Heading, PopupKind::Hint, PopupKind::Input,
            PopupKind::Error, PopupKind::Hint,
        ]));
        view.key(key(KeyCode::Char('2')));
        assert!(view.error.is_empty());
        view.set_error("Task #2 not found".into());
        view.key(key(KeyCode::Backspace));
        assert!(view.error.is_empty());
        view.set_error("Load error".into());
        view.key(ctrl('u'));
        assert!(view.error.is_empty() && view.value.is_empty());
    }
}
```

## Task 3: Modal Routing And Navigation

**Modify:** `src/tui/mod.rs`, `src/tui/render.rs`.

- [x] Split snapshot loading into a helper returning archive state from the same
  snapshot; keep the existing `task_target` API as a wrapper:

```rust
fn task_target(db: &crate::db::Db, id: i64) -> Result<Target> {
    task_target_with_archived(db, id).map(|(target, _)| target)
}
fn task_target_with_archived(db: &crate::db::Db, id: i64) -> Result<(Target, bool)> {
    let snapshot = db.content_snapshot(id)?;
    let archived = snapshot.task.archived;
    let draft = Draft::from_saved(&snapshot.task.description, id, &snapshot.references)?;
    Ok((Target::Task {
        id,
        description: snapshot.task.description,
        status: snapshot.task.status,
        revision: snapshot.task.content_revision,
        draft,
    }, archived))
}
```

- [x] Make local `include_archived` mutable; add `let mut jump_ui: Option<jump::View> = None;`. Add popup rows between conflict and action rows:

```rust
.or_else(|| jump_ui.as_ref().map(|ui| ui.rows(usize::from(popup_content.width))))
```

Add `&& jump_ui.is_none()` to the active mouse guard. Route paste into the jump
view before action/filter/editor handling:

```rust
if let Some(ui) = jump_ui.as_mut() {
    ui.paste(&text);
} else if let Some(ActionUi::Input { value, error, .. }) = action_ui.as_mut() {
    value.extend(text.chars().filter(|ch| !ch.is_control()));
    error.clear();
}
```

- [x] Handle jump keys at the start of the key branch, before existing conflict,
  confirmation, filter and editor handling:

```rust
if let Some(mut ui) = jump_ui.take() {
    match ui.key(key) {
        Some(jump::Action::Go(id)) => match task_target_with_archived(
            mode.db().expect("dashboard has database"), id,
        ) {
            Ok((target, archived)) => {
                buffers.park(DraftKey::current(target_id, draft_parent_id), &mut draft,
                    &baseline, top, editor_follow_cursor);
                editor_follow_cursor = load_target(restore_target(target, &mut buffers),
                    &mut draft, &mut target_id, &mut target_status, &mut draft_parent_id,
                    &mut baseline, &mut top);
                include_archived |= archived;
                show_completed |= target_status.as_deref() == Some("completed");
                filter_query.clear();
                filter_focused = false;
                list_top = 0;
                list_follow_selected = true;
                message.clear();
                message_is_error = false;
            }
            Err(error) => {
                ui.set_error(format!("{error:#}"));
                jump_ui = Some(ui);
            }
        },
        Some(jump::Action::Cancel) => (),
        None => jump_ui = Some(ui),
    }
    continue;
}
```

After existing modal handling, before Ctrl+P and filter input, open the view:

```rust
if dashboard && control && key.code == KeyCode::Char('l')
    && !key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
{
    jump_ui = Some(jump::View::default());
    continue;
}
```

- [x] Add `Ctrl-L Go to Task` to dashboard and filter shortcut constants,
  including both Herdr variants. Keep non-dashboard editor shortcuts unchanged.

```rust
pub const DASHBOARD_KEYS: &str = "Ctrl-S Save  Ctrl-L Go to Task  Ctrl-P Create Child  Ctrl-G Menu  Shift-Up/Dn Switch Tasks  Ctrl+/ Filter";
pub const DASHBOARD_HERDR_KEYS: &str = "Ctrl-S Save  Ctrl-H Herdr  Ctrl-L Go to Task  Ctrl-P Create Child  Ctrl-G Menu  Shift-Up/Dn Switch Tasks  Ctrl+/ Filter";
pub const FILTER_KEYS: &str = "Type to Filter  Ctrl-L Go to Task  Ctrl-T Completed  Backspace Edit  Esc Clear/Close  Tab/Enter Editor";
pub const FILTER_HERDR_KEYS: &str = "Type to Filter  Ctrl-L Go to Task  Ctrl-T Completed  Ctrl-H Herdr  Backspace Edit  Esc Clear/Close  Tab/Enter Editor";
```
- [x] Run `cargo fmt --all`, controller tests and all new PTY tests. Verify
  invalid/missing IDs do not mutate DB, cancellation preserves filter/focus,
  dirty new/task/child drafts restore, same-ID jump preserves caret, offscreen
  list follows selection, archived task appears, narrow input clips safely.
- [x] Commit verified feature with `[Feat] Add Ctrl L Task ID Jump`.

## Task 4: Documentation And Integration

**Modify:** `README.md`, `docs/reference.md`, this plan.

- [x] Add keyboard row `| Ctrl+L | Go to task ID; retain unsaved drafts |`.
  Reference text explains Enter, Esc/Ctrl+C, missing/invalid errors, cleared
  filter on success and session-wide archived visibility after archived jump.
- [x] Run `cargo fmt --all --check`, `git diff --check`,
  `cargo test --offline --all-targets`,
  `cargo clippy --offline --all-targets -- -D warnings`.
- [x] Request read-only review with exact base/head diff. Resolve concrete
  findings, commit checks and documentation, rebase and fast-forward local master.
- [x] Verify combined source if parent production changed. Install with
  `cargo install --path . --locked --offline --force`. Run all seven new PTY
  scenarios plus existing filter, retained-buffer, child, actions, compact and
  wide-layout checks against the absolute installed binary path.
- [x] Record truthful evidence using `qqq message 157`, complete task from root
  workspace, remove only merged task-157 worktree/branch, resume one persistent
  `qqq next --wait --local --json` process.

Pre-integration checks: 587 tests across 38 binaries passed, including 88 TUI
tests. Formatting, diff checks and Clippy all targets with warnings denied
passed. Read-only review found no remaining concrete issues. Existing footer
expectations now include Ctrl+L; shortcut coloring checks include the new key.
The hint remains visible in normal and Herdr bars at 72 columns.

Concurrent narrow-terminal stress reproduced partial-frame timing failures in
7 of 24 runs. Waiting for complete startup, resize and popup frames before input
and cursor assertions passed 24 of 24 runs. No TUI behavior changes were needed.
Logs: `/tmp/qqq-task-157-full-frame.log`, `/tmp/qqq-task-157-clippy.log` and
`/tmp/qqq-task-157-narrow-frame-stress/`.

Integration must cover the completed-task visibility toggle added to master
during this task: a successful jump to a hidden completed task reveals its row;
cancellation and jumps to unfinished tasks preserve that visibility preference.

Rebase preserved both Ctrl+L and Ctrl-T hints, NO_COLOR scenarios, and fixture
exclusions. New `jump_completed` PTY regression failed before the integration
fix because editor loaded task #2 while its completed row stayed hidden. The
fix enables completed rows only when destination is completed. All four jump
tests, covering seven scenarios, passed afterward. Cancel and unfinished jumps
preserve the hidden-completed setting and retained new draft. Red/green logs:
`/tmp/qqq-task-157-completed-red.log`, `/tmp/qqq-task-157-integrated-jump.log`.


Final combined checks: 593 tests across 38 binaries passed with zero failures;
formatting, diff checks and Clippy all targets with warnings denied passed.
Final read-only review against master `874cb4d` found no concrete issues.
Local master fast-forwarded to `7a026ab`. Locked/offline optimized install passed.
Installed `/Users/tonylee/.cargo/bin/qqq` passed 19 PTY scenarios: seven jump
scenarios, filter shortcuts/query, retained navigation/manual scroll, dirty child,
actions, compact/wide color and NO_COLOR, completed-toggle color and NO_COLOR.
Installed SHA256 matched the verified release before and after checks:
`be37a10a9928f3eab6ba38e5529bca0dfadf972e35a6ac249654eda7f9b5a4d2`.

Evidence recorded in task message #84. Explicit `qqq complete 157 --json`
returned `completed` at `2026-10-04T08:40:10.892Z`. Final logs:
`/tmp/qqq-task-157-integrated-full.log`,
`/tmp/qqq-task-157-integrated-clippy.log`, `/tmp/qqq-task-157-install.log`,
`/tmp/qqq-task-157-installed-pty.log`. Owned checkout cleanup and one persistent
blocking queue wait follow this documentation checkpoint.
