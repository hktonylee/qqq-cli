# Luau Filters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Compile documented Luau task predicates to SQLite for list, watch, local claims, waiting claims, and Herdr dispatch.

**Architecture:** An AST compiler emits static SQL and bound rusqlite values. List snapshots return SQL matches alongside base task rows, then retain existing search/status/ancestor behavior. Claim predicates execute inside existing ownership transactions.

**Tech Stack:** Rust 1.85+, full_moon 3 with Luau, rusqlite, clap, existing CLI integration tests.

## Task 1: Compiler contract and literal decoder

Files: create `tests/sql_filter.rs`, `src/sql_filter/mod.rs`, `src/sql_filter/literals.rs`; modify `Cargo.toml`, `Cargo.lock`, `src/main.rs`.

- [x] Add compiler tests through the source module; first test proves SQL injection remains a bound value:

```rust
#[path = "../src/sql_filter/mod.rs"]
mod sql_filter;
use rusqlite::{Connection, params_from_iter};

fn evaluate(source: &str) -> bool {
    let filter = sql_filter::compile(source).unwrap();
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE tasks(id INTEGER,description TEXT,status TEXT,priority INTEGER,archived INTEGER,parent_id INTEGER); INSERT INTO tasks VALUES(1,'Fix auth','new',3,0,NULL)").unwrap();
    db.query_row(&format!("SELECT {} FROM tasks", filter.sql()), params_from_iter(filter.params()), |row| row.get(0)).unwrap()
}

#[test]
fn literals_are_bound() {
    let filter = sql_filter::compile("description == \"' OR 1=1 --\"").unwrap();
    assert!(!filter.sql().contains("OR 1=1"));
    assert_eq!(filter.params(), &[rusqlite::types::Value::Text("' OR 1=1 --".into())]);
    assert!(!evaluate("description == \"' OR 1=1 --\""));
}
```

- [x] Run `CARGO_TARGET_DIR=/Users/tonylee/Dropbox/Projects/qqq/target cargo test --test sql_filter`; expect missing compiler module.
- [x] Define compiler boundary, add `full_moon = { version = "=3.0.0", default-features = false, features = ["luau", "serde"] }`, retain Rust 1.85-compatible `smol_str 0.3.2` in lockfile. Implement literal parsing using TokenType rather than token display/trivia.

```rust
pub struct CompiledFilter {
    sql: String,
    params: Vec<rusqlite::types::Value>,
}
impl CompiledFilter {
    pub fn sql(&self) -> &str { &self.sql }
    pub fn params(&self) -> &[rusqlite::types::Value] { &self.params }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind { Boolean, Number, Text, Nil }
struct Expr { sql: String, kind: Kind }
```

- [x] Test quoted escape families, multiline normalization, finite decimal/hex/binary numbers, invalid UTF-8 and out-of-range integers; run same target, expect passing literals.

## Task 2: Typed expression/function compilation

Files: modify `src/sql_filter/mod.rs`, `tests/sql_filter.rs`; create `src/sql_filter/functions.rs`.

- [x] Add expression tests before implementation:

```rust
#[test]
fn luau_precedence_nil_and_truthiness() {
    for expression in ["priority + 2 * 3 == 9", "parent_id == nil", "not parent_id", "not false and not nil", "not not 0", "not not \"\"", "(parent_id or 0) == 0", "(if archived then 0 else priority) == 3", "#'é' == 2"] {
        assert!(evaluate(expression), "{expression}");
    }
    for expression in ["false == 0", "nil ~= parent_id", "parent_id > 0", "not 0", "not ''"] {
        assert!(!evaluate(expression), "{expression}");
    }
}
```

- [x] Run compiler target; expect missing operator/function behavior. Implement AST allowlist with recursive typed expressions; merge same-kind/nil branches, compile Luau truthiness by kind, logical/conditional values with CASE to preserve nil and short-circuiting. Reject unsupported kinds/forms. Limit tokens before parser, recursion/nodes/SQL size during compiler.

```rust
fn truthy(value: &Expr) -> String {
    match value.kind {
        Kind::Boolean => format!("COALESCE(({}),0)", value.sql),
        Kind::Nil | Kind::Number | Kind::Text => format!("({}) IS NOT NULL", value.sql),
    }
}
```

- [x] Add exact function signature table and SQL emitters. LIKE/GLOB swap SQLite function argument order by emitting SQL operators; LIKE escape emits ESCAPE. Validate arity/types; dates allow text/number first argument, text modifiers. Coalesce/min/max require compatible types. Test every function family, wrong arities, unknown names, injection, parser/program escape attempts, resource limits.
- [x] Run `cargo fmt --check` and compiler target; expect all compiler tests pass. Commit compiler checkpoint with verified scope.

## Task 3: List and watch integration

Files: modify `src/main.rs`, `src/db.rs`, `src/list_filter.rs`, `src/watch.rs`, `tests/tui_db.rs`; create `tests/filters.rs`; modify `tests/watch.rs`.

- [x] Add CLI tests for aliases, AND composition, parent context, completed/archive limits, invalid-filter errors before DB creation. Add watch test:

```rust
#[test]
fn watch_applies_luau_filter_to_each_commit() {
    let dir = project();
    let p = dir.path();
    let mut watch = Watcher::start(p, &["list", "--watch", "--json", "--filter", "priority > 0"]);
    assert_eq!(watch.snapshot(), serde_json::json!([]));
    ok(p, &["add", "Low"]);
    assert_eq!(watch.snapshot(), serde_json::json!([]));
    ok(p, &["edit", "1", "--priority", "5"]);
    assert_eq!(watch.snapshot()[0]["id"], 1);
    watch.idle();
}
```

- [x] Run `cargo test --test filters list_combines_luau_search_status_and_parent_context`; expect unsupported `--filter`.
- [x] Add both CLI flags with `EXPR` help and compile once in `run` before opening DB. Pass `Option<&CompiledFilter>` to `execute`. Add `Db::list_filtered(max_completed, include_archived, filter)` returning `(Vec<Task>, Option<HashSet<i64>>)`. Append static compiled predicate as column 14, bind filter params followed by completion/archive parameters; use shifted numbered placeholders. Read matches and rows from same SQL snapshot. Extend `filter_tasks` with optional ID set and existing AND/context traversal.

```rust
let limit_slot = filter.params().len() + 1;
let archive_slot = limit_slot + 1;
let mut values = filter.params().to_vec();
values.push(max_completed.map_or(rusqlite::types::Value::Null, rusqlite::types::Value::Integer));
values.push(rusqlite::types::Value::Integer(i64::from(include_archived)));
```

- [x] Replace watch's seven positional arguments with `WatchOptions`, including query/status/filter references. Preserve refresh/version/broken-pipe behavior. Import compiler module in direct DB test harness. Run compiler/list/watch targets, fmt, clippy; keep list/claim wiring in one verified integration checkpoint.

## Task 4: Atomic next and Herdr integration

Files: modify `src/db.rs`, `src/main.rs`, `src/dispatch.rs`, `tests/filters.rs`, `tests/dispatch.rs`.

- [x] Add tests for priority/order, parent readiness, archived exclusions, matching race/concurrent owners, existing claim returned even when filter false, wait ignoring unmatched changes then claiming matching row. Add dispatch test proving no tab for unmatched rows and matching claim persisted before mocked prompt.
- [x] Run focused targets; expect next to ignore predicate.
- [x] Extend DB claim internals with optional filter. Keep old method wrappers for existing callers. Existing owned-ID lookup remains first; only new selection changes. Bind compiled parameters in same immediate transaction:

```rust
let predicate = filter.map_or("1", sql_filter::CompiledFilter::sql);
let sql = format!("SELECT id FROM tasks WHERE status='new' AND archived=0 AND (parent_id IS NULL OR EXISTS(SELECT 1 FROM tasks parent WHERE parent.id=tasks.parent_id AND parent.status='completed')) AND ({predicate}) ORDER BY priority DESC,id ASC LIMIT 1");
let values = filter.map_or(&[][..], sql_filter::CompiledFilter::params);
```

- [x] Add filtered readiness preflight; pass same filter into Herdr atomic claim. Preserve link/identity/failure-release behavior. Run filters/dispatch/CLI/dependency targets plus clippy; commit verified claim checkpoint.

## Task 5: Public references and release verification

Files: create `docs/filter.md`, `docs/filter-variables.md`, `docs/filter-functions.md`; modify `README.md`, this plan.

- [x] Write exhaustive usage/variables/functions tables matching compiler mappings and restrictions; include quoted shell examples for list, next, watch/wait, nil checks, dates, LIKE escape, logic truthiness, bound limits, current-claim behavior. Link README and CLI help.
- [x] Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` with shared target directory. Expect zero failures; disclose unresolved failures instead of claiming pass.
- [x] Request read-only code review using existing reviewer under requesting-code-review skill; resolve findings, rerun affected checks. Mark completed checkboxes, commit final verified implementation/docs.
- [x] Rebase task-88 onto current master, fast-forward master, rerun full suite plus fmt/clippy on combined tree (master also gained task 87 during implementation). Install via `cargo install --path . --locked --force`. Run installed binary in temporary initialized project using `list --filter 'like(task_name, "%auth%")'` and `next --local --session smoke --filter 'priority >= 0'`; confirm matching rows only.
- [x] Prepare queue handoff after final docs commit: complete task 88 explicitly; remove worktree and merged branch; resume single blocking queue waiter. DB records completion; operational handoff follows this commit.

Self-review: all spec paths covered, public names/types aligned, ownership/archive/context unchanged, no schema migration. No unresolved design choices.

Validation: pre-rebase full suite 437 passing; added runtime-rollback regression passes. Combined-tree full suite after task 87 rebase: 442 passing; fmt/clippy clean. Reviewer found no remaining code blockers; 18 public filter examples and both CLI help pages pass smoke checks.

Main integration: 40 focused filter/dispatch/watch tests pass. Installed release smoke passes list/aliases, atomic claims, persistent ownership, dependencies/archive, invalid filters, waiting claims, watch refresh/idle behavior.
