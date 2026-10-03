# Retained Dirty Drafts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Keep dashboard drafts across navigation, show dirty task markers, confirm every pending draft before exit.

**Architecture:** Move complete dirty `Draft` into ordered UI cache keyed by task or new-draft parent. Decorate loaded targets with retained state; active buffer remains outside cache. Add row metadata for styled dirty column. Exit confirmations track approvals without changing draft storage.

**Tech Stack:** Rust, Crossterm, Ratatui, SQLite, Python PTY tests.

---

### Task 1: Ordered Draft Storage

**Files:** Create `src/tui/buffers.rs`; modify `src/tui/mod.rs`, `src/tui/draft.rs`.

- [x] Baseline: `cargo test --locked` passed 491 tests on `91697f1`.
- [x] Add failing cache tests before methods: park task #2 with changed text and caret/scroll, restore and compare original baseline and caret; park task #1 plus general and child drafts, assert ordered independent keys; park image and 1001-character paste, restore and compare composition/image bytes and masks; clean/reverted draft is not parked.
- [x] Use this storage API:

  ```rust
  use super::draft::Draft;
  use std::collections::BTreeMap;

  #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
  pub(super) enum DraftKey {
      Task(i64),
      New(Option<i64>),
  }

  impl DraftKey {
      pub fn current(id: Option<i64>, parent: Option<i64>) -> Self {
          id.map_or(Self::New(parent), Self::Task)
      }

      pub fn label(self) -> String {
          match self {
              Self::Task(id) => format!("task #{id}"),
              Self::New(None) => "new draft".into(),
              Self::New(Some(parent)) => format!("child draft (parent #{parent})"),
          }
      }
  }

  pub(super) struct ParkedDraft {
      pub draft: Draft,
      pub baseline: String,
      pub top: usize,
      pub follow_cursor: bool,
  }

  #[derive(Default)]
  pub(super) struct DraftBuffers {
      entries: BTreeMap<DraftKey, ParkedDraft>,
  }

  impl DraftBuffers {
      pub fn park(&mut self, key: DraftKey, draft: &mut Draft,
                  baseline: &str, top: usize, follow_cursor: bool) {
          if draft.is_dirty_against(baseline) {
              self.entries.insert(key, ParkedDraft {
                  draft: std::mem::replace(draft, Draft::new("")),
                  baseline: baseline.to_owned(), top, follow_cursor,
              });
          }
      }

      pub fn take(&mut self, key: DraftKey) -> Option<ParkedDraft> {
          self.entries.remove(&key)
      }

      pub fn get(&self, key: DraftKey) -> Option<&ParkedDraft> {
          self.entries.get(&key)
      }

      pub fn keys(&self) -> Vec<DraftKey> {
          self.entries.keys().copied().collect()
      }

      pub fn task_ids(&self) -> impl Iterator<Item = i64> + '_ {
          self.entries.keys().filter_map(|key| match key {
              DraftKey::Task(id) => Some(*id),
              DraftKey::New(_) => None,
          })
      }
  }
  ```
- [x] Register `mod buffers`. Add `Target::Retained { key: DraftKey, status: Option<String>, saved: ParkedDraft }` and `restore_target(target, &mut buffers)`: derive key from fresh target, take matching parked state, carry fresh task status. Extend `load_target` to restore retained baseline/draft/parent/top; return cursor-follow mode (`true` for fresh targets). Assign that return at every call site instead of overriding with `true`.
- [x] Avoid cloning pending image bytes during per-frame dirty checks: return `true` immediately when `Draft.atoms` contains `Atom::Image`; otherwise compare serialized description with baseline. Run existing `cargo test --locked --test tui_model dirty_check`.
- [x] Run `cargo test --locked --bin qqq buffers::tests`; require all storage regressions pass. Commit verified storage checkpoint.

### Task 2: Navigation and Dirty Rendering

**Files:** Modify `src/tui/mod.rs`, `src/tui/panel.rs`, `src/tui/dashboard.rs`, `tests/tui.rs`, `tests/tui_dashboard_pty.py`, `tests/tui_dashboard_render.rs`.

- [x] Add failing PTY `buffers_navigation`: edit #2, Shift-Up to #1 without confirmation, edit #1, Shift-Down restore #2, save #2 only, return #1 and verify draft remains and DB #1 stays original. Capture caret before switching, verify restored caret. Assert `[*]` while active/background dirty and removal after independent save/revert.
- [x] Add PTY `buffers_new_child`: dirty general new draft -> #2 -> child draft -> #1 -> #2 -> Ctrl-P restores child; Shift-Down restores general new draft; save child and general independently, verify saved child parent ID.
- [x] Park active dirty state only after validated mouse/keyboard target resolves. Dashboard Shift-Up/Down, task-row clicks and Ctrl-P retain without switch confirmation. Non-dashboard navigation retains existing confirmation branch. Decorate every `load_target` input with `restore_target` so explicit return-to-new and successful actions/saves can restore distinct parked new drafts without duplicate keys.
- [x] Add `dirty: bool` to `panel::ListRow`, initialized `false`. In compose loop, collect parked task IDs plus active dirty task ID. Determine whether displayed tasks include dirty ID; subtract four extra columns from list width when needed before calling existing formatter/`panel::rows`. Set each row's flag by task ID.
- [x] Render marker column between selection marker and row text:

  ```rust
  let dirty_column = rows.iter().any(|row| row.dirty);
  let mut spans = vec![Span::raw(marker)];
  if dirty_column {
      let flag_style = if color && row.dirty {
          Style::default().fg(POPUP_PROMPT_FG)
              .bg(if is_selected { SELECTION_BG } else { BODY_BG })
      } else {
          Style::default()
      };
      spans.push(Span::styled(if row.dirty { "[*]" } else { "   " }, flag_style));
      spans.push(Span::raw(" "));
  }
  spans.push(Span::raw(row.text.as_str()));
  ```

  Use `Paragraph::new(Line::from(spans)).style(style)`. Append textual `[*]` to dirty new/child draft title. Stored task content/filter matching remains unchanged.
- [x] Add renderer regressions for dirty/clean/selected/wrapped rows, gold marker backing, NO_COLOR style resets, literal `[*]` in user text, width clipping and click mapping. Gold index 222 against explicit dark backing must meet 4.5:1 contrast.
- [x] Update existing dashboard dirty-navigation tests (`dirty`, `child_dirty`, `click_filter`, `wheel`, filtered confirmation setup, `workflow`) to retention behavior; keep explicit Ctrl-C/Esc/action discard assertions. Replace blind cleanup keystroke batches with state-aware Ctrl-C/confirmation handling so any number of retained drafts can exit without typing accidental text into blank editor.
- [x] Run focused storage/model/render tests and new PTY navigation scenarios. Commit navigation/markers only after checks pass.

### Task 3: Exit Approval Queue

**Files:** Modify `src/tui/mod.rs`, `tests/tui.rs`, `tests/tui_dashboard_pty.py`.

- [x] Add failing `buffers_exit`: retain dirty #2 and #1, reach clean new draft, Ctrl-C prompts #1 then #2. `y` on #1, `n` on #2 cancels exit; navigate back and verify both drafts preserved. Repeat exit with active dirty new draft; approve active draft, iterate retained keys, repeated Ctrl-C keeps current prompt, Esc/Enter cancel. Final approvals exit and DB remains unchanged.
- [x] Add `Confirmation::ExitBuffers { keys: Vec<DraftKey>, index: usize }`. Create pending queue from `buffers.keys()` only when nonempty. Footer identifies current key; compact footer abbreviates while retaining key context. Popup identifies draft and previews text/image/paste labels with existing escaping/styling.
- [x] On `Confirmation::Exit` approval, enter parked queue instead of exiting when dashboard cache is nonempty. Clean-new Ctrl-C and blank-editor Esc enter same queue before exit. Repeated Ctrl-C on queue must `continue` before active-draft guards or generic confirmation Ctrl-C exit.
- [x] Approval transition:

  ```rust
  Confirmation::ExitBuffers { keys, index } => {
      if index + 1 == keys.len() {
          return cancel(saved_any, dashboard);
      }
      confirmation = Some(Confirmation::ExitBuffers { keys, index: index + 1 });
  }
  ```

  Do not remove/cache-discard anything while advancing. Existing `n`/Enter/Esc cancellation clears confirmation only, preserving active and parked drafts. Clear action popup when starting exit queue.
- [x] Add deleted-background-task scenario, image/paste retention through declined exit, and NO_COLOR/minimum-width/wide resize coverage. Verify current #143 clear-query/close-filter/active-task/new-task sequence still precedes background prompts.
- [x] Run `cargo test --locked --test tui buffers -- --nocapture`, existing Ctrl-C/Esc/action scenarios, then full suite. Commit exit behavior after all regression updates pass.

### Task 4: Documentation and Integration

**Files:** Modify `docs/reference.md`, this plan; keep CLI/DB schema unchanged.

- [x] Replace dashboard discard-on-navigation docs with retained keyboard/mouse/child behavior, dirty marker, independent save, new/child entry points, per-buffer exit approval and cancellation preserving all drafts. Keep single-editor docs unchanged.
- [x] Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `git diff --check`. Request read-only review through existing reviewer; resolve findings, repeat affected checks.
- [x] Record focused/full/review evidence. Rebase task branch onto current master; resolve conflicts and verify combined code when changed. Fast-forward master, install `cargo install --path . --locked --offline --force`.
- [x] Exercise installed CLI retention/independent-save/exit-cancel/NO_COLOR paths, remove merged worktree/branch, `qqq complete 144 --json`, resume persistent `qqq next --wait --local --json`.

Verification checkpoint: all 500 tests pass, including five cache/restoration unit tests,
31 dashboard renderer tests and 79 TUI tests. New buffer PTY cases cover keyboard/mouse,
independent saves/revert, new/child drafts, caret/manual scroll, image export bytes,
paste atoms, exit approval cancellation, deleted tasks, filter priority, plain mode,
and resizing through 12x8, 150x36 and 72x24. Focused atom/scroll checks also pass
after extending declined-exit coverage. Formatting, Clippy with warnings denied,
and diff whitespace checks pass. Read-only review approved after replacing let-chain
syntax incompatible with declared Rust 1.85 and inheriting row style for clean marker
spacing. Navigation/storage/marker/exit tests observed expected failures before fixes.
Navigation and exit changes share final feature commit.


Integration checkpoint: master fast-forwarded to `2aa9050` without conflicts or
additional source changes. Installed qqq 0.3.0 from reviewed branch with locked,
offline release build. Seven installed CLI PTY scenarios passed: task navigation
and independent saves, new/child restoration, image/paste atoms through declined
exit, sequential exit cancellation, minimum/wide resize, plain mode, child exit.
Root working tree clean before integration; cleanup and queue completion follow
this documentation checkpoint.
