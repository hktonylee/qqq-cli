# Agent JSON Defaults Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recognized agent callers receive JSON by default; `--human` preserves an explicit human mode.

**Architecture:** Best-effort caller detection uses native Codex markers and exact Herdr pane metadata. Existing output/error paths share one resolved boolean; argument scanning respects option values and positional literals.

**Tech Stack:** Rust, Clap, serde_json, existing subprocess integration/PTY tests.

## Task 1: Regression Coverage

**Files:** Create `tests/agent_output.rs`.

- [x] Add isolated subprocess tests removing inherited `QQQ_SESSION`, `CODEX_THREAD_ID`, `CODEX_SESSION_ID`, `HERDR_ENV`, and `HERDR_PANE_ID`. Native marker tests use `.env("CODEX_THREAD_ID", "agent-thread")`; parse stdout/stderr with `serde_json::from_slice::<Value>`.
- [x] Cover init/add/list/show/next/complete success, runtime failure `TASK_NOT_FOUND`, parser failure `INVALID_ARGUMENT`, broken aliases, full completed lists under configured display limit, and newline-delimited watch snapshots.
- [x] Cover `--human` before/after commands and aliases; explicit `--json` with no agent; conflicting flags; literal `--human`/`--json` as option values or following `--`; empty markers; text help/version.
- [x] Unix Herdr fixture supplies `pane current --pane ...` JSON through an executable shell script. Assert agent metadata enables JSON, ordinary panes keep prose, failed lookups stay usable, and caller detection never runs `agent list`.
- [x] Run `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline --test agent_output`. Confirm failures because default output is prose or `--human` is absent.

## Task 2: Output Resolution

**Files:** Modify `src/main.rs`, `src/cli_error.rs`, `src/aliases.rs`, `src/session.rs`, `src/herdr.rs`.

- [x] Add global field:

```rust
#[arg(long, global = true, conflicts_with = "json")]
human: bool,
```

- [x] In `session.rs`, add `is_agent_caller() -> bool`: nonblank UTF-8 native markers return true; otherwise call best-effort exact Herdr detection. In `herdr.rs`, add caller predicate using `current_pane()` only when `has_context()`; nonblank `agent_session.agent` or `agent` means agent.
- [x] Extend `cli_error::requests_json(arguments, default_json)` to track real `--human` while preserving existing value-skipping and `--` behavior; explicit JSON wins for structured flag-conflict errors. Return `default_json && !human` when no real JSON flag appears.
- [x] Resolve detection lazily when arguments require automatic mode; cache caller result through alias expansion and both early error scans. Recompute real overrides after expansion, since aliases can consume apparent flags as literal option values. After successful parse, set `cli.json = cli.json || (requested_json && !cli.human)` before existing `run`/error paths.
- [x] Extend root alias scanning to skip `--human` like `--json`.
- [x] Run `cargo fmt`, focused agent-output tests, existing `output`, `json_errors`, `aliases`, `display`, `watch`, `autodetect`, `identity`, and CLI suites. Fix failures within feature scope.
- [x] Commit verified behavior with `[Feat] Default Agent Caller Output To JSON`.

## Task 3: Documentation And Delivery

**Files:** Modify `README.md`, `docs/reference.md`.

- [x] Document supported detection, explicit `--json`/`--human`, unchanged help/version, best-effort Herdr fallback, and JSON error contract for detected agents.
- [x] Run `cargo fmt --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`, `cargo test --locked --offline`, and `cargo build --locked --offline --release`.
- [x] Review diff against spec, obtain independent review before merging, fix actionable issues with focused verification.
- [ ] Commit docs, rebase onto updated master, fast-forward integrate, verify installed CLI defaults and overrides. Record evidence through `qqq message 165`, complete task, clean merged worktree, resume one persistent queue waiter.

## Verified Results

- Baseline: 617 tests across 38 binaries, zero failures.
- Final: 633 tests across 39 binaries, zero failures; formatting, Clippy, and locked offline release build passed.
- Independent review found one alias/literal mode bug; failing native and Herdr regressions proved it, lazy cached resolution fixed it, re-review found no remaining concrete issues.
- Full suite exposed inherited agent context in intentional human-format tests; those subprocesses now request `--human` explicitly.
- Logs: `/tmp/qqq-task-165-final-full.log`, `/tmp/qqq-task-165-final-clippy.log`, `/tmp/qqq-task-165-final-release.log`.
