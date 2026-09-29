# Editable Dependencies Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Change/clear task dependency safely through edit.

**Architecture:** Typed parent update extends existing atomic edit. Validate proposed ancestry with recursive SQL under immediate write transaction; use current queue query unchanged so new parent is authoritative for claims.

**Tech Stack:** Rust, Clap, SQLite/rusqlite, CLI integration tests.

---

### Task 1: Regression Tests

**Files:** Create `tests/parent_edit.rs`; extend `tests/wait.rs`.

- [x] Write lifecycle/queue test:

```rust
assert_eq!(ok(p, &["edit", "1", "--set-parent", "2"])["parent_id"], 2);
assert_eq!(ok(p, &["next", "--local", "--session", "worker"])["id"], 2);
assert!(ok(p, &["next", "--local", "--session", "other"]).is_null());
assert!(ok(p, &["edit", "1", "--set-parent", "none"])["parent_id"].is_null());
assert_eq!(ok(p, &["next", "--local", "--session", "other"])["id"], 1);
```

- [x] Cover field/image/status atomicity with wrong owner, image-insert abort trigger, invalid title/ID, self and descendant cycles. Verify history/messages/claim/link/created_at stay intact. Cover explicit editor plus parent, negative references, concurrent reciprocal edits and waiting until parent is cleared.
- [x] Run `cargo test --locked --test parent_edit`; expect failures for missing `--set-parent`.

### Task 2: Atomic Parent Update

**Files:** Modify `src/db.rs`, `src/main.rs`, `src/dispatch.rs`.

- [x] Define `ParentChange { Set(i64), Clear }`, derive Clone/Copy/Debug, implement FromStr accepting `none` or positive i64; other values error `Parent must be a positive task ID or none`.
- [x] Extend `Db::edit` with `parent: Option<ParentChange>`, existing callers supply None. Inside transaction, validate parent exists and `id != parent`; detect cycles:

```sql
WITH RECURSIVE ancestors(id,parent_id) AS (
 SELECT id,parent_id FROM tasks WHERE id=?1
 UNION
 SELECT tasks.id,tasks.parent_id FROM tasks JOIN ancestors ON tasks.id=ancestors.parent_id
)
SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)
 OR NOT EXISTS(SELECT 1 FROM ancestors WHERE parent_id IS NULL)
```

Reject true result. Set/clear with `UPDATE tasks SET parent_id=? WHERE id=?`; validate affected row. Existing transaction owns rollback for later field/status/image failures.
- [x] Add `set_parent: Option<db::ParentChange>` flagged `#[arg(long, value_name="ID|none")]`. Parent-only edit bypasses compose. Forced compose calls Db.edit with draft fields/images and parent so all saves remain atomic.
- [x] Run `cargo test --locked --test parent_edit --test dependencies --test images --test editor --test wait --test tui_db`; require all pass.

### Task 3: Docs And Completion

**Files:** Modify `README.md`.

- [x] Document `qqq edit 2 --set-parent 1`, `qqq edit 2 --set-parent none`, cycle/parent validation, atomic combinations, ownership preservation and existing claim behavior.
- [x] Run full `cargo test --locked`, `cargo fmt --check`, strict Clippy, release build, diff check; require exit 0.
- [ ] Review, commit `[Feat] Allow Editing Task Dependencies`, rebase current master, fast-forward merge, rebuild CLI, record checks and complete #29. Clean worktree; resume `next --wait --local --json`.
