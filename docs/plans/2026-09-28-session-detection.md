# Native Session Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto-detect exact Herdr pane or native Codex session consistently across ownership commands.

**Architecture:** Typed caller resolver separates private key, native metadata and optional Herdr link. DB derives native metadata inside claim transaction only for fresh claims. Exact pane resolution stays in Herdr adapter; no schema changes.

**Tech Stack:** Rust, clap, rusqlite, serde, real CLI integration tests, fake Herdr executable.

## 1. Tests before implementation

Files: `tests/autodetect.rs`, `tests/wait.rs`, existing CLI command fixtures.

- [x] Clear inherited `CODEX_THREAD_ID`, `CODEX_SESSION_ID`, `HERDR_PANE_ID` in fixture commands (preserve deliberate Herdr test env).
- [x] Add native claim regression using existing `command`, `project`, `ok` helpers:

```rust
let dir = project();
let p = dir.path();
let task = ok(command(p).env("PATH", p).env("CODEX_THREAD_ID", "native").arg("next"));
assert_eq!(task["harness_name"], "codex");
assert_eq!(task["harness_session"], "native");
assert!(task["orchestrator_name"].is_null());
assert!(ok(command(p).args(["show", "1"]))["herdr"].is_null());
```

- [x] Cover thread-over-session precedence, session fallback, lifecycle (wait/release/error/complete), wrong thread, explicit CLI/env overrides, harness overrides, blank/non-UTF-8 selected values, native metadata retention, exact pane without HERDR_ENV, invalid exact context, manual link, configured dispatch existing claim and new-task refusal outside Herdr. Fake Herdr logs arguments; exact lookup must contain `--pane`.
- [x] Add incoming-task wait test with bounded child-process polling, isolated HOME/DB and no Herdr binary.
- [x] Run `cargo test --locked --test autodetect --test wait`; confirm native and pane-specific regressions fail for missing behavior.

## 2. Typed caller resolution

Files: create `src/session.rs`; modify `src/main.rs`, `src/herdr.rs`, `src/dispatch.rs`, `src/db.rs`.

- [x] Define `Owner { key: String, link: Option<Link>, metadata: Option<Identity> }`. Native helper returns `Result<Option<Owner>>`; selected `std::env::var` value rejects non-Unicode/blank. Skip native resolution when exact Herdr context exists. Encode private key with `serde_json::to_string(&("codex", "id", &value))`; native metadata fills only harness name/session.
- [x] `session::owner(explicit, project_dir, db)` validates explicit input first, then native helper (which defers exact Herdr context), then existing `herdr::owner` adapter. `session::dispatch_caller(db)` returns native key when selected, otherwise existing exact Herdr caller key.
- [x] Add `herdr::has_context()` checking pane env presence or HERDR_ENV=1. Share current-pane lookup used by `current` and `dispatch_caller`; validate pane then call `pane current --pane <id>`. `discover` uses `has_context`.
- [x] Route `local_owner`, `complete` and ownership-sensitive edit transitions through typed resolver. Keep manual `herdr link` using Herdr-only resolution. Apply explicit harness overrides after native metadata.
- [x] Extend DB claim API with optional derived metadata; choose it only for new claims inside transaction. Keep current identity for existing owned tasks; apply user overrides separately. Update main/dispatch callsites with native metadata or None.
- [x] Run focused suites; fix implementation until all pass.

## 3. Docs and final verification

Files: `README.md`, this plan.

- [x] Add regression for release between dispatch lookup and write lock; retrieve existing claims atomically without claiming fresh work. Preserve explicit harness-session local fallback under invalid automatic env.
- [x] Document precedence, variable names, exact pane lookup, public native fields, fallback/limitations and unchanged explicit override behavior.
- [x] Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --release`, `git diff --check`.
- [x] Request independent review required by requesting-code-review skill; resolve findings with regressions.
- [x] Commit verified change; rebase onto current master, fast-forward merge, rebuild root debug CLI, smoke-test isolated native and real current Herdr caller. Mark task #24 complete; remove clean merged worktree/branch.
- [x] Return to `qqq next --wait` loop; task #36 claimed. Thread goal now stops after five consecutive empty waits.

## Verified before integration

- 217 tests passed; formatting, clippy all targets with warnings denied, release build and diff check passed.
- Native and incoming-wait regressions observed red before implementation.
- Dispatch release race reproduced red, fixed with atomic existing-only retrieval; regression passed.
- Independent review clear; reviewer reran four focused regressions.
- Isolated native Codex, actual Herdr pane (without HERDR_ENV marker), and another Herdr agent kind claimed/completed successfully.

Integrated on master at `41c975c`; root CLI rebuilt, 33 focused tests and native claim/completion smoke passed. Task #24 completed via original ownership key.
