# Completion Force Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let explicit force completion recover from Herdr/owner failures in Ctrl+G Complete, with unchecked opt-in checkbox.

**Architecture:** Preserve owner-protected normal completion and existing atomic `Db::force_complete`. Share existing discovery-error classification between preparation and confirmed-action failure handling. Completion confirmation holds force opt-in and recoverable error text; mouse/Space toggle never writes DB, `y` confirms selected mode, cancellation preserves draft. Unrelated DB/validation failures retain ordinary error popup.

**Tech Stack:** Rust/Crossterm/Ratatui, existing SQLite actions, Python real-terminal test harness.

---

## Findings and interaction contract

Baseline force-completion tests passed (2 Rust tests, 6 PTY scenarios). Current
preparation already converts typed Herdr failures into force confirmation. Gap:
preparation resolves matching owner; confirmation resolves again; Herdr can fail
on second call, which closes confirmation into generic Action error with no force
option. Reproduced with functioning fake exact Herdr pane then transport failure
immediately before `y`; DB/history/draft stayed untouched but no force option.
Evidence: `/tmp/qqq-task-174-preflight-probe.log`.

- Every completion dialog shows `[ ] Force complete`, reset unchecked on opening.
- Space or click checkbox/label toggles; `y` confirms. Existing Enter/n/Esc cancel
  behavior stays. Ordinary matching-owner completion works without selecting force.
- Existing Force complete dialog requires checkbox selection before `y` can write.
- Herdr discovery/transport failure or ownership mismatch during normal confirm
  keeps dialog with exact failure and unchecked force option. Retry normal or
  explicitly select force. Force path never invokes Herdr.
- Corrupt DB, invalid explicit session and other errors stay errors; no automatic
  fallback or bypass. Force completion keeps unfinished-only/atomic event guards.
- Checkbox/hint stay usable in compact/no-color layouts. Dirty-draft warning and
  cancellation preserve current text/revision/images until confirmed completion.

## Task 1: Regressions before fix

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`

- [x] Run unchanged force baseline and standalone real-terminal failure probe.
- [x] Add fake Herdr scenarios: initial failure and normal-confirm failure after
  matching-owner preparation. Seed native claim, expose exact matching pane,
  switch failure via temporary marker before confirming.
- [x] Add PTY assertions for unchecked state, blocked forced `y`, Space toggling,
  mouse toggling, cancel/reopen reset, error retention, no DB/event/draft change,
  explicit force success without further Herdr calls, ownership/identity clearing.
- [x] Run `cargo test --locked --offline --test tui tui_dashboard_force_completion`
  and observe missing force recovery/checkbox failures.

## Task 2: Repair confirmation handling

**Files:** `src/tui/completion.rs`, `src/tui/mod.rs`, `src/main.rs`

- [x] Extract existing typed dispatch/missing-identity discovery whitelist into
  `tui::completion::discovery_unavailable(&anyhow::Error)`. Add
  `can_force_after_error` permitting same errors plus OwnershipMismatch; reject
  DB/untyped/invalid explicit argument errors. Reuse discovery helper in main
  preparation; do not swallow DB owner-lookup errors.
- [x] Extend Action confirmation with `force: bool` and `error: String`, initialize
  through helper with `false`/empty. Render completion checkbox early, selected
  owner warning, dirty warning, bounded exact error text, final compact hint.
- [x] Toggle checkbox with Space and popup-coordinate mouse hit test. Ignore
  unrelated mouse clicks while modal open. Forced unchecked `y` preserves modal;
  selected force executes `TaskAction::ForceComplete`, normal executes Complete.
- [x] On recoverable normal failure retain confirmation/error with force=false.
  Other failures use existing Action error popup. Never automatically force.
- [x] Update existing force/race/dirty-draft scenarios to select checkbox only at
  explicitly forced confirmations. Add narrow/no-color behavior checks; preserve
  existing owner-DB-error rejection scenario.
- [x] Run focused TUI/action/DB checks, format and strict all-targets Clippy;
  commit verified fix with bracketed Title Case message.

## Task 3: Docs, review and completion

**Files:** `docs/reference.md`, this plan

- [x] Explain checkbox, `Space`/click + `y`, normal retry after Herdr failure,
  explicit force bypass and preserved draft/claim guards.
- [ ] Run focused TUI suite, DB/render checks, full locked/offline suite,
  compatibility gate, fmt/Clippy/release build. Verify installed binary with
  focused force scenarios after local integration/install.
- [x] Obtain independent read-only review through requesting-code-review skill;
  fix concrete findings and rerun affected checks.
- [ ] Record fresh evidence, locally rebase/fast-forward, clean owned worktree,
  complete task through qqq, resume exactly one silent queue waiter.

## Validation evidence

- Initial regression run: existing 2 tests passed, new 2 failed for absent checkbox
  and lost confirmation recovery (`/tmp/qqq-task-174-red.log`).
- Focused final force coverage: 4 Rust tests / 11 PTY scenarios passed, including
  initial and confirmation-time Herdr failure, color/no-color, keyboard/mouse,
  cancel/reset, owner race, unrelated DB failure and 24x14 compact terminal
  (`/tmp/qqq-task-174-force-compact.log`).
- Error classification: 2 passed; action/DB and render checks: 36 + 20 passed.
- First full TUI run exposed a test resize barrier missing final frame; fixed new
  narrow scenario to wait for full footer/cursor before sending next key.
- Next default-parallel TUI run passed new force cases but failed unchanged
  Completed-toggle resize/click scenario; isolated old test passed. No unrelated
  UI change. Evidence: `/tmp/qqq-task-174-tui-final.log` and
  `/tmp/qqq-task-174-existing-toggle.log`.
- Full suite serial: 685 tests / 40 binaries passed; compatibility gate: 152 / 7;
  strict all-target Clippy, format and release build passed before footer change.
- Independent reviewer found compact hint needed 20 columns in an 18-column
  popup; new 24x14 PTY reproduced clipping (exit101), then passed with width-aware
  `Space y Esc`. Evidence: `/tmp/qqq-task-174-compact-red.log`.
- Independent final review: compact finding resolved, 3 initial-failure PTY
  scenarios passed independently, no remaining concrete issues.
- Fresh final checks including compact hint: full suite serial 685 tests / 40
  binaries passed (`/tmp/qqq-task-174-full-final.log`); compatibility gate 152 / 7
  (`/tmp/qqq-task-174-gate-final.log`); fmt, strict all-target Clippy and release
  build passed (`/tmp/qqq-task-174-clippy-compact.log`,
  `/tmp/qqq-task-174-release-final.log`).
- Integration and installed verification pending.

Version evidence: local `v0.4.0` tag (commit `5c97d0c`) predates original
force-complete menu commit `2ad9770`. Other machine version remains unverified;
requested `qqq --version` asynchronously. Current fix addresses confirmed gap in
current code; do not infer other machine's installed version.
