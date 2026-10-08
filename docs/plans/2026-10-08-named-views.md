# Shared Named Views Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reuse versioned project views across CLI selection and TUI browsing without losing drafts.

**Architecture:** A shared selector engine compiles tag/filter/query/status/readiness criteria into bound SQLite predicates. Project-local view catalog stores typed definitions with locked atomic writes; consumers load read-only before preflight. TUI picker switches prepared selection while retaining editor and buffer state.

**Tech Stack:** Rust, clap Args, serde JSON, fs2, tempfile, rusqlite deterministic scalar functions, ratatui/Crossterm, CLI and Python PTY integration tests.

---

## Task 1: Shared Selectors

Files: create `src/selection.rs`, `tests/views.rs`; modify `src/main.rs`, `src/sql_filter/mod.rs`, `src/sql_filter/functions.rs`, `src/list_filter.rs`, `src/db.rs`, `Cargo.toml`.

- [ ] Add isolated integration harness in `tests/views.rs`, clearing ambient agent/Herdr sessions. Initial RED:

```rust
#[test]
fn list_and_next_share_unicode_query_and_readiness() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "ÉCOLE blocked", "--parent", "1", "--tag", "backend"]);
    ok(p, &["add", "ÉCOLE ready", "--tag", "backend"]);
    assert_eq!(ids(&ok(p, &["list", "--query", "école", "--readiness", "ready"])), [3]);
    assert_eq!(ok(p, &["next", "--dry-run", "--query", "école", "--tag", "backend"])["id"], 3);
    assert_eq!(ids(&ok(p, &["list", "--query", "école", "--readiness", "blocked"])), [1, 2]);
}
```

- [ ] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test views`; expect unsupported readiness/query flags.
- [ ] Define shared typed criteria and visibility:

```rust
#[derive(Clone, Debug, Default, clap::Args, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selectors {
    #[arg(long = "tag", value_name = "LABEL", allow_hyphen_values = true)]
    #[serde(default)]
    pub tags: Vec<String>,
    #[arg(long, value_name = "EXPR", allow_hyphen_values = true)]
    pub filter: Option<String>,
    #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
    pub query: Option<String>,
    #[arg(long = "status", value_enum, value_name = "STATUS")]
    #[serde(default)]
    pub statuses: Vec<crate::list_filter::ListStatus>,
    #[arg(long, value_enum)]
    pub readiness: Option<Readiness>,
}
#[derive(Clone, Copy, Debug, clap::ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness { Ready, Blocked }
#[derive(Clone, Copy, Default)]
pub struct Visibility {
    pub include_archived: Option<bool>,
    pub max_completed: Option<Option<i64>>,
    pub fallback_completed: Option<i64>,
}
#[derive(Clone)]
pub struct Prepared {
    pub filter: Option<crate::sql_filter::CompiledFilter>,
    pub include_archived: bool,
    pub max_completed: Option<i64>,
}
```

- [ ] Derive Debug/Serialize/Deserialize for ListStatus with snake_case; expose `as_str`. Derive Clone for CompiledFilter. Add private deterministic Unicode containment SQL function using rusqlite `functions` feature, register in both Db opens:

```rust
pub fn register(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function("qqq_contains", 2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let text = ctx.get::<String>(0)?;
            let query = ctx.get::<String>(1)?;
            Ok(text.to_lowercase().contains(&query.to_lowercase()))
        })
}
```

- [ ] Extend CompiledFilter with safe bound query/status helpers and static readiness predicate AND. Combine multiple Luau sources as `not not (\nSOURCE\n)` joined by `and`; compile once so parameter numbering remains valid, truthiness preserved. Compile errors retain INVALID_FILTER annotation; invalid tags retain INVALID_ARGUMENT `--tag`.
- [ ] `selection::prepare` receives optional saved Selectors, explicit Selectors, resolved Visibility; append saved/explicit tags, queries, status groups and readiness independently using AND. `selection::list(db, prepared, extra_query)` clones filter, adds live query, calls `Db::list_filtered`, then `list_filter::filter_tasks(tasks, None, &[], matches)` to retain context. Caller resolves saved versus explicit visibility before preparation; selection engine has no catalog dependency.
- [ ] Replace duplicated CLI List/Next fields with flattened Selectors; add selectors to TUI. Route main List/watch/Next through prepared filter and visibility. Keep ordinary human display limits, filtered empty strings, owned reuse and dispatch untouched.
- [ ] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test views --test filters --test tag_filters --test sql_filter --test watch --test cli`; fmt/diff check. Commit `[Refactor] Share Task Selection Across CLI Modes`.

## Task 2: Versioned View Catalog And CLI

Files: create `src/views.rs`; modify `src/main.rs`, `src/cli_error.rs`, `src/output.rs`, `tests/views.rs`, `tests/preflight.rs`.

- [ ] Add RED tests saving/listing/showing/removing views, nested project lookup, malformed definitions and missing view rejection before DB/owner recovery. Test command:

```rust
ok(p, &["view", "save", "Ready backend", "--tag", "backend", "--readiness", "ready", "--max-completed", "0"]);
assert_eq!(ok(p, &["view", "show", "Ready backend"])["criteria"]["tags"], serde_json::json!(["backend"]));
assert_eq!(ids(&ok(p, &["list", "--view", "Ready backend"])), [3]);
```

- [ ] Run views tests; expect unknown view command/flag.
- [ ] Define strict persisted format:

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedView {
    pub name: String,
    #[serde(default)]
    pub criteria: crate::selection::Selectors,
    #[serde(default)]
    pub include_archived: bool,
    pub max_completed: Option<i64>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub views: Vec<SavedView>,
}
```

- [ ] Expose existing `db::database_path(false)` for read-only project discovery. Catalog path is DB parent parent plus `.qqq-views.json`. Missing catalog -> version1/empty. Reject file >1MiB, non1 version, duplicate names, malformed/unknown/duplicate fields; validate every definition by shared normalization/compilation before use. Return ConfigError for bad persisted catalog, InvalidArgument for unknown requested name.
- [ ] Writer opens `.qqq/views.lock`, locks exclusively, rereads catalog, upserts/removes exact name, sorts names, writes pretty JSON plus newline to tempfile in project root, preserves old permissions, flushes/syncs, atomically persists. Concurrent writers must preserve each independent view.
- [ ] Add `ViewCommand::{Save, List, Show, Remove}` and `Commands::View`. Handle before DB preflight/open; output JSON full definition/array or readable names/criteria. Add `--view NAME` to List/Next/TUI, `--hide-archived` where archive flags exist, TUI completed visibility flags. Resolve view before preflight. Visibility overrides: explicit archive flag first; explicit all/limit first; saved defaults next; ordinary human config fallback last.
- [ ] Extend tests: status groups use saved OR AND explicit OR, both queries required, all tags required, Luau nonboolean truthiness preserved, blocked/ready use prerequisites, archive/completed overrides, versioned persistence and file permissions, save replaces exact name, removing unknown fails, read-only browsing DB/file bytes unchanged, concurrent saves, malformed catalog before active-owner recovery.
- [ ] Run focused views/preflight/output/alias tests, fmt/diff. Commit `[Feat] Add Project Named View Catalog`.

## Task 3: TUI Picker And Shared Matching

Files: create `src/tui/view_picker.rs`; modify `src/tui/mod.rs`, `src/tui/render.rs`, `src/main.rs`, `tests/tui.rs`, `tests/tui_dashboard_pty.py`, `tests/tui_db.rs`.

- [ ] Add PTY scenario `views` plus `views_no_color`, fixture views stored at project root. Initial RED opens Ctrl-B, expects picker heading and criteria; current TUI cannot show it.
- [ ] Add context type containing catalog path, startup Selectors, Visibility, active SavedView and Prepared selection; construct before DB/terminal entry. Continuous add carries no view context; dashboard receives context. Preserve existing non-dashboard behavior.
- [ ] Picker holds sorted definitions plus first no-view entry, selected index and scroll. Render selected name/criteria using existing PopupRow kinds and wrapping, bounded popup height, selected visible. Up/Down changes selection, Enter applies, Esc/Ctrl-C closes. Ctrl-B reloads/validates catalog when no other modal/confirmation active; errors use existing footer/modal, preserve active view and draft.
- [ ] Each dashboard snapshot uses shared selection::list for display, with live filter query added by AND. Keep full task snapshot for hidden current task details/status. Use resulting context_only metadata with existing tree rendering, dirty markers and visible IDs. Apply view by changing prepared selection/visibility only; retain target/baseline/draft/buffers, caret/editor scroll/details scroll, filter text/focus. Active name appears in title; default title remains unchanged.
- [ ] Completed toggle preserves existing behavior: saved zero limit starts hidden; toggling on reveals all; positive saved limit retained when initially visible. View switching reapplies defaults plus explicit startup overrides. Existing task-ID jump may reveal archived/completed target locally using current behavior; switching reasserts saved visibility.
- [ ] Add unit picker tests for empty catalog, selection boundaries, wrapping/scroll/no-color neutral PopupRow kinds. Add required source modules to tests/tui_db as dependencies require.
- [ ] Extend PTY checks: dirty saved task retains caret and editor scroll across hidden view switch; staged new tags and parked draft survive; live DB edits alter membership without replacing hidden dirty task; filter text/focus retained; resize to narrow/wide and no-color; picker cancellation retains state; CLI visible IDs equal TUI rows under equivalent selectors.
- [ ] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test tui --test tui_db -- --test-threads=4`; fmt/diff. Commit `[Feat] Pick Shared Views In TUI Without Losing Drafts`.

## Task 4: Worker Parity And Delivery

Files: modify `tests/views.rs`, `tests/watch.rs`, `tests/dispatch.rs`, `README.md`, `docs/reference.md`, `docs/filter.md`; create `docs/views.md`; update this plan.

- [ ] Add watch/wait/dry-run tests against named tag/ready views; definition loaded once per invocation, DB commits reevaluate. Concurrent named-view workers claim distinct matching tasks; changed views reuse owned task; unknown/malformed view creates no Herdr tab. Explicit extra selectors AND saved criteria for Herdr preflight and claim.
- [ ] Run focused views/watch/dispatch/preflight tests. Document complete version1 file, commands, exact lookup, AND/status/query/readiness semantics, visibility precedence, snapshots versus project source files, watch loading, Ctrl-B picker and retained drafts.
- [ ] Run `cargo fmt --check`, `git diff --check`, `CARGO_INCREMENTAL=0 cargo test --locked --offline -- --test-threads=4`, `CARGO_INCREMENTAL=0 cargo clippy --locked --offline --all-targets -- -D warnings`; require zero failures. Use process access required by native owner tests.
- [ ] Request independent review through requesting-code-review skill, fix validated findings, rerun affected/full checks as justified. Commit `[Docs] Document Shared Named Views And Verification`.
- [ ] Build release, run documented examples and views PTY scenarios against release binary. Rebase onto current master, resolve other worker main/docs edits preserving their behavior, fast-forward locally. Verify integrated suite; install with `CARGO_INCREMENTAL=0 cargo install --path . --locked --offline --force`. Run installed examples/PTYs, compare release/installed SHA256. Record actual evidence in plan, commit verified docs.

Queue-worker handoff: record progress using `qqq message 184`, complete only after delivery checks, read back status. Remove own merged worktree/branch, resume one persistent `qqq next --wait --local --json`.
