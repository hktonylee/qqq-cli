# Structured JSON Errors Implementation Plan

> Execute inline with superpowers:executing-plans. Preserve existing success payloads and use typed classifications, never message matching.

**Goal:** Give JSON CLI callers stable error codes and safe structured recovery details.

**Architecture:** A small shared domain-error module carries explicit code, safe public message, human diagnostic text and object details. CLI error boundary renders domain errors, content conflicts, SQLite/IO types and generic failures. Parsing uses fallible Clap; output mode is known before alias/parser failures. Editor recovery keeps source errors and attaches structured file paths.

**Baseline:** Source matches task 146's verified 516-test baseline; task 148 worktree starts from `d056f4d`. Run isolated build before edits; no production differences from baseline.

## 1. Contract regressions

**Files:** new `tests/json_errors.rs`.

- [x] Add real CLI helpers asserting entire nonterminal stderr parses as exactly one `{code,message,details}` object, stdout empty, nonzero exit. Cover missing task/reference/project, CLI flags/values, blank fields, invalid filters, owner mismatch and transition details, content conflict/removal, contention, aliases/config, editor and IO errors.
- [x] Include private owner/claim strings in ownership fixtures and assert absent from JSON envelope. Check DB rollback after rejected compound edits.
- [x] Assert JSON flag position/alias expansion/literal values, human errors, help/version, successful payloads, empty queue and unhealthy doctor behavior.
- [x] Run new focused tests; record expected old plain-stderr/parser failure.

## 2. Shared errors and CLI boundary

**Files:** new `src/errors.rs`, new `src/cli_error.rs`, `src/main.rs`, `src/aliases.rs` as needed.

- [x] Add explicit code enum and typed error details object; separate safe public wording from existing human context. No identity/claim values in JSON.
- [x] Add renderer inspecting typed errors, task 146 `ContentConflict`, SQLite busy/locked/error codes and IO errors. Document generic fallback; include command details.
- [x] Replace terminating parse with fallible parse; discover JSON request before alias/parser errors, respect literal `--` and hyphen-allowing values. Preserve Clap help/version and parser exit status.
- [x] Keep doctor reports and successful JSON/human/watch payloads intact. Add output-mode access only where existing diagnostics/editor behavior require it.
- [x] Run parser/basic contract tests, plus CLI/aliases/config/doctor suites.

## 3. Domain classifications

**Files:** `src/db.rs`, `src/delete.rs`, `src/sql_filter/mod.rs`, validation/config/session/dispatch boundaries, standalone test module imports.

- [x] Replace missing-task contexts with typed errors; include real ID or negative reference.
- [x] On failed guarded transition, read status/owner in existing transaction; distinguish missing task, wrong state and owner mismatch without leaking claim keys. Preserve readable human text and rollback behavior.
- [x] Tag argument/filter errors at validation boundaries; type project lookup and config/session/dispatch failures where meaningful. Preserve underlying SQLite causes so contention remains classifiable.
- [x] Run domain contract, DB/owner/transition/dependency/archive/delete/filter suites. Ensure codes don't depend on wording; test error wrapping.

## 4. Editor error/recovery output

**Files:** `src/editor.rs`, `src/editor/recovery.rs`, `tests/editor.rs`, new JSON contract tests.

- [x] Preserve typed conflict/IO errors through recovery exits; attach local/current/attachment recovery paths.
- [x] Suppress prose recovery prelude and arbitrary editor terminal output for nonterminal JSON calls. Keep terminal UI/prompts and human editor diagnostics.
- [x] Add editor exit status details; preserve pending image bytes and local recovery files. Avoid printing duplicate error objects.
- [x] Run external editor JSON recovery checks and existing editor/TUI PTY checks.

## 5. Docs, verification, integration

**Files:** `docs/reference.md`, spec/plan status.

- [x] Add envelope examples, code/details reference, parser/runtime exits, contention retry and revision-conflict retry. State doctor/help/version/empty-queue exceptions and live terminal stderr behavior.
- [x] Run full `cargo test --locked`, fmt, Clippy all targets, diff check; record actual results.
- [x] Request read-only review via existing reviewer, address supported findings, run affected checks.
- [x] Rebase current master, rerun relevant checks for upstream code changes, clean-root fast-forward merge, install, verify real installed contract.
- [x] Explicitly complete task 148 after installed checks; record evidence.

After documentation integration: remove own worktree/branch; resume one blocking queue wait.

## Verification evidence

- Baseline isolated `cargo build --locked` passed. New missing-task contract initially failed on old plain stderr. Alias literal-JSON regression also failed before fix.
- Real JSON contract suite: 16 tests passed, including busy DB lock, private SQLite trigger text, Herdr subprocess privacy, ownership/state rollback, editor conflict/removal and retained pending image bytes. Renderer unit checks cover busy/locked errors through context and private human wrapping.
- Full `cargo test --locked --no-fail-fast`: 559 tests passed across 38 targets, 0 failures or ignored; includes 82 TUI tests. `/private/tmp/qqq-148-full2.log`.
- `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --all -- --check`, `git diff --check` passed.
- Rebased onto task 147 queue diagnostics and task 149 stdin/batch import source `e879cc0`; master later gained doc-only integration evidence `6672510`. New commands share contract.
- Raw SQLite trigger messages, subprocess output and owner tokens remain human-only. Image/snapshot rejection suites retain human diagnostic coverage; machine contract uses codes/details. Dispatch failures distinguish released startup claims from uncertain prompt delivery without session identities.

- Final read-only review clear after two reproduced alias fixes: literal option values stay human; alias introducing only `--json` still returns JSON on missing-command expansion failure. Second regression failed before prospective-expansion fix.
- Post-review run passed all 17 JSON tests and Clippy, but wide-layout PTY padding assertion failed once under full-suite load. Focused wide-layout test passed both color modes on unchanged source. Final full-suite rerun passed on unchanged production source.

- Final `cargo test --locked --no-fail-fast`: 560 tests passed across 38 targets, 0 failures or ignored; includes 17 JSON contract tests and 82 TUI tests. `/private/tmp/qqq-148-full4.log`. Final Clippy passed (`/private/tmp/qqq-148-clippy3.log`); fmt and diff checks passed.

## Installed integration

- Feature commit `85461df` fast-forwarded into clean master after final rebase onto `6672510`. Source/tests/dependencies matched freshly verified pre-rebase commit `da127c6`; only upstream documentation changed.
- `cargo install --path . --locked --force` passed; installed `/Users/tonylee/.cargo/bin/qqq` replaced task 149 build with merged master source.
- Installed CLI passed 12 smoke groups: success/error streams, parser exit 2, missing IDs, invalid filters and explanation, owner/state privacy, guarded stale saves, alias JSON modes, filesystem/config errors, actual DB lock and private trigger text, editor exit isolation, retained editor conflict text/images, import/status, help/version/doctor/empty queue. `/private/tmp/qqq-148-installed-check.log`.
- Installed real PTY checks passed: single-task overwrite and removal, dashboard content-conflict recovery in color and NO_COLOR. `/private/tmp/qqq-148-installed-pty.log`.
- Explicit `qqq complete 148 --json` returned `status: completed` at `2026-10-03T19:22:36.861Z`, after all installed gates passed.
