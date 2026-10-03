# Harness Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Replace task assignee with harness/orchestrator identity, preserving claims.

**Architecture:** Keep opaque ownership in private claim_key. Four nullable public fields carry metadata; overrides and Herdr links supply values. Atomic v5 migration and existing DB transactions preserve lifecycle invariants.

**Tech Stack:** Rust, clap, rusqlite, serde, Herdr CLI, integration tests.

---

### Task 1: Schema and public contract

Files: create `src/sql/migrate_v5.sql`, `src/identity.rs`; modify `src/db.rs`, `src/output.rs`; replace `tests/assignee.rs` with `tests/identity.rs`, update existing contract assertions.

- [x] Write failing integration assertions:
```rust
assert!(task.get("assignee").is_none());
assert!(task.get("claim_key").is_none());
assert_eq!(task["harness_session"], "worker");
assert_eq!(task["harness_name"], "codex");
```
- [x] Run `cargo test --test identity` -> field/flag failures.
- [x] Add typed identity contract, flattened in Task JSON:
```rust
#[derive(Clone, Default, serde::Serialize)]
pub struct Identity {
    pub harness_name: Option<String>,
    pub harness_session: Option<String>,
    pub orchestrator_name: Option<String>,
    pub orchestrator_session: Option<String>,
}
```
- [x] Migration renames assignee to claim_key; adds four TEXT columns; backfills active rows from herdr_links JSON with fallback claim_key. Advance open version checks to 5. Queries append metadata columns; human task display prints four fields.
```sql
ALTER TABLE tasks RENAME COLUMN assignee TO claim_key;
ALTER TABLE tasks ADD COLUMN harness_name TEXT;
ALTER TABLE tasks ADD COLUMN harness_session TEXT;
ALTER TABLE tasks ADD COLUMN orchestrator_name TEXT;
ALTER TABLE tasks ADD COLUMN orchestrator_session TEXT;
PRAGMA user_version=5;
```
- [x] Extend migration tests to preserve data, active ownership/constraints, error rows, concurrent opens and reject schema 6 without changes. Run identity/dependencies/error tests -> pass after Task 2.

### Task 2: Claim metadata and overrides

Files: modify `src/main.rs`, `src/db.rs`, `src/aliases.rs`, `src/identity.rs`, `src/herdr.rs`, `src/dispatch.rs`; test `tests/identity.rs`, `tests/autodetect.rs`, `tests/dispatch.rs`.

- [x] Add failing CLI tests for all four overrides, raw session ownership, ambiguous raw sessions and name narrowing; mock named Herdr discovery and assert child metadata exists before prompt.
```rust
let task = ok(&d, &["next", "--session", "key", "--harness-name", "codex",
    "--harness-session", "public", "--orchestrator-name", "herdr",
    "--orchestrator-session", "named"]);
assert_eq!(task["harness_session"], "public");
assert_eq!(ok(&d, &["next", "--local", "--session", "public"])["id"], 1);
```
Current CLI uses alias `owned` only when configured, so direct test retrieval uses `next --local` instead of `owned`.
- [x] Typed Identity validates nonempty supplied fields; derives defaults from claim/link, overlays explicit flags. Add four global clap flags, matching alias parser global-option recognition.
```rust
pub fn next_with_identity(&mut self, session: &str, link: Option<&crate::herdr::Link>, overrides: &crate::identity::Identity) -> anyhow::Result<Option<Task>>;
```
- [x] Resolve exact claim key first, then active harness session/name; multiple matches error before claiming new work. Resolve ownership inside write transactions for release/error/complete. Clear four fields when claim ends; content edits leave them untouched. New and existing claims apply overrides atomically.
- [x] Best-effort Herdr server-name lookup occurs outside transactions. Dispatch gets real child metadata, applies overrides, persists link+fields before prompt, returns freshly loaded Task. Existing generated token remains in env/prompt and private DB key.
- [x] Run `cargo test --test identity --test autodetect --test dispatch` -> pass; commit feature plus contract updates.

### Task 3: Integration and verification

Files: `README.md`, existing field-related tests, spec/plan progress.

- [x] Update README examples for JSON fields, explicit override flags and legacy ownership recovery.
- [x] Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release`, `git diff --check` -> all pass.
- [x] Dispatch independent code review; fix material findings with regression tests, rerun affected checks.
- [x] Rebase latest master, rerun checks after relevant concurrent changes, merge fast-forward, rebuild root debug binary; smoke queue show/next with original task #26 session.
- [x] Record qqq progress, complete task #26 using original owner token, remove isolated branch/worktree. Continue `next --wait --local` with stable session.
