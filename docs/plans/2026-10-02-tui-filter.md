# TUI filter implementation plan

> **For agentic workers:** Use superpowers:executing-plans task by task. Mark completed checks.

**Goal:** Add visible task-list filter to `qqq tui` without changing selected task or draft while query changes.

**Architecture:** Pure panel filter model selects matching tasks plus ancestors. Dashboard draws bar, rows, empty result. Compose loop owns filter text/focus; Shift navigation uses visible IDs.

### Task 1: Filter model

- [ ] Add failing `panel` model tests for case-insensitive full description, parent chain, empty/no-match query, and visible-ID navigation.
- [ ] Add pure filter function over task views and adjacent visible ID selection. Run focused tests and commit.

### Task 2: Dashboard layout

- [ ] Add failing Ratatui render tests for visible filter bar, focus cursor, empty state, hidden selection, resize, and no-color cells.
- [ ] Draw filter bar under heading; reserve row in list scrolling; show empty message. Run focused tests and commit.

### Task 3: Keyboard flow and PTY

- [ ] Add failing PTY scenarios for `/`, typing, backspace/Esc clear, Tab/Enter blur, full-description match, parent context, filtered Shift navigation, dirty draft, DB immutability, resize, and plain mode.
- [ ] Wire filter state through compose loop. Preserve editor draft and selected target while filtering; navigate visible IDs with existing confirmation behavior. Update README. Run focused PTY/model/render tests and commit.

### Task 4: Verify and integrate

- [ ] Run full `cargo test --locked`, strict Clippy, fmt check, diff check. Request read-only code review; fix Important/Critical findings.
- [ ] Rebase local master, fast-forward, run integrated full suite, remove owned worktree/branch, install CLI, complete task 59, verify status, resume one blocking `qqq --json next --wait --local` call.
