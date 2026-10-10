# Orchestrator Pane Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Task #211 records task runner terminal/pane as Orchestrator Session.

**Architecture:** Shared `Identity::for_claim` derives public terminal identity
from saved pane, independently of harness ownership and Herdr server routing.
No migration; explicit overrides and recorded history retain existing behavior.

**Tech Stack:** Rust, SQLite, serde, CLI integration tests.

## 1. Regressions

- [x] Update automatic expectations in `tests/autodetect.rs` from `default` or
  `named` to `terminal-1`. Assert named `herdr.server` remains `named`.
- [x] Cover reported harness, manual link and unique-CWD claims independently:
  `assert_eq!(task["orchestrator_session"], "terminal-1");`.
- [x] For absent, empty and whitespace `terminal_id`, retain reported harness
  identity and assert `orchestrator_session == "w1:p1"`. A failed server lookup
  must still record `terminal-1`, with `herdr.server == null`.
- [x] Assert CLI list filter and human show consume stored terminal identity.
- [x] Update dispatch pre-prompt DB check to use child terminal, falling back to
  child pane when `NO_TERMINAL`; add successful no-terminal dispatch regression.
  Assert child session differs from caller terminal; explicit overrides remain.
- [x] Run `CARGO_TARGET_DIR=/app/qqq/target/task203 cargo test --offline --locked --test autodetect --test dispatch -- --test-threads=1`.
  Expected RED: old server value differs from terminal/pane; no unrelated errors.

## 2. Fix and docs

- [x] Replace server-derived field in `src/identity.rs`:
  ```rust
  orchestrator_session: Some(
      link.pane.terminal_id.as_ref()
          .filter(|terminal| !terminal.trim().is_empty())
          .unwrap_or(&link.pane.pane_id).clone(),
  ),
  ```
- [x] Change `src/main.rs` help to terminal/pane wording. Update current
  `docs/reference.md` and `docs/filter-variables.md`; explain terminal fallback,
  separate saved server, recorded history and overrides.
- [x] Run `cargo fmt --all -- --check`, then focused tests above plus
  `--test identity`. Expected GREEN; baseline 57 existing cases stay passing.

## 3. Verify and deliver

- [x] Run full `CARGO_TARGET_DIR=/app/qqq/target/task203 cargo test --offline --locked -- --test-threads=1`.
- [x] Run `CARGO_TARGET_DIR=/app/qqq/target/task203 cargo clippy --offline --locked --all-targets -- -D warnings` and release build.
- [x] Request independent code review; resolve substantiated findings and rerun
  affected checks. Commit verified fix using bracketed Title Case message.
- [ ] Rebase onto current master; verify integration and fast-forward root.
- [ ] Install verified release; check artifact hash, help and controlled live
  pane metadata. Refresh owned task #211 explicitly, verify owner retained.
- [ ] Record qqq evidence, complete #211, read back completion.

Queue worker resumes one `qqq next --wait --local --json` after delivery, retaining
waiter until it yields work.

Verification checkpoint: 61 focused cases passed; full suite, strict Clippy,
release build, fmt and independent review passed on combined master base.
