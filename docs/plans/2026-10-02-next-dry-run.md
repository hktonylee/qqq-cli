# Next Dry Run Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Preview next task without claiming or dispatching it.

**Architecture:** Share ready-candidate selector between atomic claim and deferred read snapshot. Dry-run skips existing claims and owner/Herdr discovery. Preserve regular next ownership, filters, wait loop, stored task output.

**Tech Stack:** Rust, Clap, rusqlite/SQLite, existing Luau filter compiler, CLI integration tests.

---

### Task 1: Prove preview skips owned tasks and preserves queue

**Files:** Modify `tests/cli.rs`.

- [x] Add failing CLI regressions using existing `project`, `ok`, `run` helpers:

```rust
#[test]
fn next_dry_run_previews_priority_filter_and_readiness_without_claim() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Blocked", "--parent", "1", "--priority", "100"]);
    ok(p, &["add", "Archived", "--priority", "100"]);
    ok(p, &["archive", "3"]);
    ok(p, &["add", "Match first", "--priority", "5"]);
    ok(p, &["add", "Match second", "--priority", "5"]);
    let before = ok(p, &["list", "--include-archived"]);
    let preview = ok(p, &["next", "--dry-run", "--local", "--session", "preview"]);
    assert_eq!(preview["id"], 4);
    assert_eq!(preview["status"], "new");
    assert!(preview["harness_session"].is_null());
    assert_eq!(ok(p, &["list", "--include-archived"]), before);
    assert!(ok(p, &["show", "4"])["events"].as_array().unwrap().is_empty());
    assert_eq!(ok(p, &["next", "--dry-run", "--local", "--session", "preview", "--filter", "id == 5"])["id"], 5);
    assert!(ok(p, &["next", "--dry-run", "--local", "--session", "preview", "--filter", "false"]).is_null());
    assert_eq!(ok(p, &["next", "--local", "--session", "worker"])["id"], 4);
}

#[test]
fn next_dry_run_skips_owned_task_and_keeps_queue_without_writes() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Owned"]);
    ok(p, &["next", "--local", "--session", "owner"]);
    let queued = ok(p, &["add", "Queued"]);
    let before_owned = ok(p, &["show", "1"]);
    let before_queued = ok(p, &["show", "2"]);
    let conn = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER preview_no_task_updates BEFORE UPDATE ON tasks BEGIN SELECT RAISE(ABORT, 'preview wrote task'); END;").unwrap();
    let preview = ok(p, &["next", "--dry-run", "--local", "--session", "owner", "--harness-name", "changed", "--harness-session", "changed", "--orchestrator-name", "changed"]);
    assert_eq!(preview, queued);
    assert_eq!(ok(p, &["show", "1"]), before_owned);
    assert_eq!(ok(p, &["show", "2"]), before_queued);
    assert_eq!(ok(p, &["next", "--dry-run"]), queued);
    assert!(ok(p, &["next", "--dry-run", "--filter", "false"]).is_null());
}
```

- [x] Run `cargo test --locked --test cli next_dry_run`; expect Clap rejection of unknown `--dry-run`.

### Task 2: Share ready selection, add read-only DB path and CLI routing

**Files:** Modify `src/db.rs`, `src/main.rs`.

- [x] Extract ready-candidate query into DB helper:

```rust
fn ready_task_id(conn: &Connection, filter: Option<&CompiledFilter>) -> Result<Option<i64>> {
    let predicate = filter.map_or("1", CompiledFilter::sql);
    let sql = format!("SELECT id FROM tasks WHERE status='new' AND archived=0
        AND (parent_id IS NULL OR EXISTS (SELECT 1 FROM tasks parent WHERE parent.id=tasks.parent_id AND parent.status='completed'))
        AND ({predicate}) ORDER BY priority DESC,id ASC LIMIT 1");
    Ok(conn.query_row(&sql, params_from_iter(filter.map_or(&[][..], CompiledFilter::params)), |row| row.get(0)).optional()?)
}

pub fn peek_next_filtered(&mut self, filter: Option<&CompiledFilter>) -> Result<Option<Task>> {
    let tx = self.conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let id = Self::ready_task_id(&tx, filter)?;
    let task = id.map(|id| tx.query_row(
        "SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session,priority,archived FROM tasks WHERE id=?",
        [id], task_row,
    )).transpose()?;
    tx.commit()?;
    Ok(task)
}
```

Replace only `None if allow_new` candidate-query branch in `claim` with `Self::ready_task_id(&tx, filter)?`. Keep existing owned query, immediate transaction, all mutation/identity/link handling unchanged.

- [x] Add Clap flag inside `Commands::Next`:

```rust
/// Preview queued candidate without claiming or returning an owned task.
#[arg(long)]
dry_run: bool,
```

- [x] Make existing `next_owner` match apply only to normal next:

```rust
Commands::Next { local, dry_run: false, .. }
    if *local || !config::load()?.herdr.next_to_new_agent => {
        Some(local_owner(&cli, project_dir, &db)?)
    }
```

- [x] Bind `dry_run` in next execution and wrap current claim/dispatch choice:

```rust
let task = if dry_run {
    db.peek_next_filtered(filter)?
} else {
    match &next_owner {
        Some(owner) => db.next_with_identity_filtered(&owner.key, owner.link.as_ref(), owner.metadata.as_ref(), &overrides, filter)?,
        None => dispatch::next(&mut db, session_input, &overrides, filter)?,
    }
};
```

Retain return condition and 250ms wait after transaction ends. Update wait comment to include read snapshots.

- [x] Run `cargo test --locked --test cli next_dry_run`, `cargo fmt`, existing priority/filter/ownership/dispatch tests. Commit `[Feat] Preview Queued Task Without Claiming` after checks pass.

### Task 3: Cover wait, dispatch and empty/human output

**Files:** Modify `tests/wait.rs`, `tests/dispatch.rs`, `tests/output.rs`, `README.md`, `docs/filter.md`.

- [x] Add waiting preview using existing `Waiter` cleanup and test helpers:

```rust
#[test]
fn wait_dry_run_returns_unclaimed_incoming_task() {
    let dir = project();
    let p = dir.path();
    let mut waiter = Waiter(Some(command(p).arg("--json")
        .args(["next", "--wait", "--dry-run"])
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap()));
    waiter.assert_waiting();
    ok(p, &["add", "Incoming"]);
    let preview = waiter.task();
    assert_eq!(preview["status"], "new");
    assert!(preview["harness_session"].is_null());
    assert!(ok(p, &["show", "1"])["events"].as_array().unwrap().is_empty());
    assert_eq!(ok(p, &["next", "--local", "--session", "worker"])["id"], preview["id"]);
}
```

- [x] Add configured-dispatch bypass regression using existing fake Herdr fixture:

```rust
#[test]
fn dispatch_dry_run_never_starts_agent_or_returns_owned_task() {
    let p = Project::new();
    p.ok(&["add", "Owned"]);
    p.ok(&["next", "--local", "--session", "caller"]);
    let queued = p.ok(&["add", "Queued"]);
    let before = p.ok(&["show", "1"]);
    let output = p.command().env_remove("HERDR_ENV").env_remove("HERDR_PANE_ID")
        .args(["next", "--dry-run"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let preview: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview, queued);
    assert_eq!(p.ok(&["next", "--dry-run", "--session", "caller", "--harness-session", "changed"]), queued);
    assert_eq!(p.ok(&["show", "1"]), before);
    assert!(p.calls().is_empty());
    assert!(p.ok(&["show", "2"])["events"].as_array().unwrap().is_empty());
}
```

- [x] Add human/JSON output test using existing output helpers:

```rust
#[test]
fn next_dry_run_reports_stored_task_and_empty_queue() {
    let dir = project();
    let p = dir.path();
    assert_eq!(text(p, &["next", "--dry-run"]), "No ready tasks.\n");
    assert_eq!(text(p, &["next", "--dry-run", "--json"]), "null\n");
    text(p, &["add", "Preview"]);
    let preview = text(p, &["next", "--dry-run"]);
    assert!(preview.contains("#1"));
    assert!(preview.contains("Status: New"));
    assert!(preview.contains("Harness session: -"));
    assert!(!preview.contains('\u{1b}'));
    assert_eq!(text(p, &["next", "--dry-run"]), preview);
    text(p, &["next", "--local", "--session", "owner"]);
    assert_eq!(text(p, &["next", "--dry-run", "--session", "owner"]), "No ready tasks.\n");
}
```

- [x] Strengthen queue-state test before preview: seed completed/error tasks with higher priority through CLI transitions, then snapshot all tasks:

```rust
ok(p, &["add", "Done", "--priority", "100"]);
ok(p, &["next", "--local", "--session", "finished", "--filter", "id == 6"]);
ok(p, &["complete", "6", "--session", "finished"]);
ok(p, &["add", "Failed", "--priority", "100"]);
ok(p, &["next", "--local", "--session", "failed", "--filter", "id == 7"]);
ok(p, &["edit", "7", "--set-status", "error", "--reason", "Retry needed", "--session", "failed"]);
```

- [x] Strengthen read-only regression before capturing snapshots: save valid stale link for queued task and reject all task/event/link mutations:

```rust
let link = serde_json::json!({
    "server": "saved", "identity": {"agent": "codex", "kind": "id", "value": "saved"},
    "pane": {"pane_id": "saved", "workspace_id": "saved", "tab_id": "saved", "terminal_id": "saved", "agent": "codex"}
});
conn.execute("INSERT INTO herdr_links(task_id,link_json) VALUES(2,?)", [link.to_string()]).unwrap();
for table in ["tasks", "events", "herdr_links"] {
    for operation in ["INSERT", "UPDATE", "DELETE"] {
        conn.execute_batch(&format!("CREATE TRIGGER preview_no_{table}_{operation} BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT, 'preview mutated queue'); END;")).unwrap();
    }
}
```

Capture `before_queued` after link insertion; compare both show snapshots after preview. This replaces single update trigger from initial regression.

- [x] Add README usage immediately after worker loop:

````markdown
`next --dry-run` previews queued candidate without claiming or dispatching an agent.
Existing claims are skipped. No session/Herdr identity required. Preview retains
`new` status and stored metadata. Combine with `--filter`, `--wait`, `--local`, and `--json`. Preview does
not reserve task; another worker can claim it afterward.

```sh
qqq next --dry-run --json
```
````

Add `next --dry-run --filter EXPR` sentence in `docs/filter.md`: same filter/readiness ordering, read-only snapshot; `--wait` waits without claiming.

- [x] Run `cargo test --locked --test cli --test filters --test dispatch --test wait --test output`. Commit `[Test] Cover Next Preview Wait And Dispatch` after checks pass.

Additional coverage: two concurrent previews wait for blocked child while caller owns parent, then both return same unclaimed child after parent completes.

### Task 4: Verify, review, integrate and resume queue

- [x] Run `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings` in worktree's private Cargo target directory. Record totals and exit codes.
- [x] Request read-only review through required review skill; resolve concrete findings, repeat affected checks.
- [ ] Rebase on current master; preserve concurrent work, verify affected integration paths. Fast-forward local master, install with `cargo install --path . --locked --offline --force`.
- [ ] Exercise installed `next --dry-run --json`; current owned task #118 must never return. Compare `qqq show 118 --json` before/after to prove unchanged ownership.
- [ ] Remove merged worktree/branch, complete #118 with qqq, resume one persistent `qqq next --wait --local --json` waiter.

Verification: focused CLI/filter/dispatch/wait/output suites passed 68 tests;
full suite passed 470 tests. Formatting and Clippy passed with exit code 0.
Read-only review found no issues. Help wording subsequently clarified claim vs
preview; CLI/output suites and formatting/Clippy passed again.
