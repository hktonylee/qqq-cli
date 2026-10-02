# TUI Task Actions Plan

**Goal:** Complete task state/dependency workflow from keyboard-only dashboard.

**Architecture:** Existing editor loop owns modal state; full-screen Ratatui
menu/prompt keeps actions visible at small sizes. Main CLI supplies action
callback that reuses DB transitions and session discovery.

- [x] Add failing PTY scenarios for menu, owned completion, retry, reopen,
  archive/unarchive, priority, parent, rejected transitions, dirty draft,
  narrow terminal, stdout/cleanup.
- [x] Add typed TUI action callback and CLI dispatch using existing DB methods.
- [x] Add menu/input/confirmation state, modal rendering, success refresh,
  failure preservation. Run focused PTY and model/render tests.
- [x] Update README key help and verify help/footer copy.
- [x] Run full suite, fmt, Clippy, diff check; request read-only review and fix
  findings.
- [ ] Fast-forward local master, rerun integrated checks, install CLI, complete
  queue task #70, remove owned worktree/branch, call `qqq next --wait` once.
