# Dependency Graph Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Inspect typed upstream blockers and unique downstream impact through read-only CLI/TUI.

**Architecture:** Two bulk reads inside one deferred snapshot build prerequisite/reverse indexes. Iterative traversals deduplicate nodes; shared readiness SQL and indexed hypothetical completion distinguish immediate readiness from reachability. Display limits bound output; cached TUI popup preserves editor state.

**Tech Stack:** Rust, rusqlite, clap, serde JSON, ratatui/Crossterm, CLI and Python PTY tests. No dependencies/schema changes.

Spec: [resolved design](../specs/2026-10-08-dependency-graph-design.md).

## Task 1: Read-Only Graph Report And CLI

Files: create `src/graph.rs`, `tests/graph.rs`; modify `src/db.rs`, `src/main.rs`, `src/cli_error.rs`, `src/output.rs`.

- [x] Add isolated CLI helpers clearing HOME/QQQ/CODEX/Herdr identities. Initial RED:

```rust
#[test]
fn graph_separates_two_blockers_and_immediate_versus_transitive_impact() {
    let dir = project(); let p = dir.path();
    ok(p, &["add", "A"]);
    ok(p, &["add", "B"]);
    ok(p, &["add", "Integration", "--depends-on", "1", "--depends-on", "2"]);
    ok(p, &["add", "Direct", "--parent", "1"]);
    ok(p, &["add", "Later", "--parent", "4"]);
    let graph = ok(p, &["graph", "1"]);
    assert_eq!(graph["format_version"], 1);
    assert_eq!(graph["impact"]["direct_dependents"], 2);
    assert_eq!(graph["impact"]["transitive_dependents"], 3);
    assert_eq!(graph["impact"]["indirect_dependents"], 1);
    assert_eq!(graph["impact"]["immediately_ready"], 1);
    assert_eq!(graph["impact"]["still_blocked"], 1);
    assert!(graph["edges"].as_array().unwrap().iter().any(|edge|
        edge["dependent"] == 4 && edge["prerequisite"] == 1 && edge["kind"] == "parent"));
    assert!(graph["edges"].as_array().unwrap().iter().any(|edge|
        edge["dependent"] == 3 && edge["prerequisite"] == 1 && edge["kind"] == "prerequisite"));
    let integration = ok(p, &["graph", "3", "--direction", "upstream"]);
    assert_eq!(integration["nodes"].as_array().unwrap().len(), 3);
}
```

- [x] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test graph`; expect unknown graph command, not fixture errors.
- [x] Define complete report model and CLI options:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction { Both, Upstream, Downstream }
#[derive(Clone, Copy, Debug, clap::Args)]
pub struct Options {
    #[arg(long, value_enum, default_value="both")]
    pub direction: Direction,
    #[arg(long, default_value_t=8, value_parser=clap::value_parser!(u16).range(0..=128))]
    pub depth: u16,
    #[arg(long, default_value_t=1000, value_parser=clap::value_parser!(u32).range(1..=10000))]
    pub max_nodes: u32,
    #[arg(long, default_value_t=2000, value_parser=clap::value_parser!(u32).range(1..=20000))]
    pub max_edges: u32,
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Kind { Parent, Prerequisite }
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Reason { Archived, InProgress, Error, Completed, ParentNotCompleted, PrerequisiteNotCompleted, MissingTask, UnknownStatus }
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all="snake_case")]
pub enum ImpactState { ImmediatelyReady, StillBlocked, AlreadyReady, Inactive }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub id: i64, pub task_name: Option<String>, pub status: Option<String>,
    pub archived: Option<bool>, pub ready: bool, pub reasons: Vec<Reason>,
    pub unfinished_dependencies: usize, pub upstream_depth: Option<usize>,
    pub downstream_depth: Option<usize>, pub blocks_focus: bool,
    pub impact: Option<ImpactState>, pub remaining_after_completion: Option<usize>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Edge { pub dependent: i64, pub prerequisite: i64, pub kind: Kind, pub unfinished: bool }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Impact {
    pub direct_dependents: usize, pub transitive_dependents: usize,
    pub indirect_dependents: usize, pub immediately_ready: usize,
    pub still_blocked: usize, pub already_ready: usize, pub inactive_dependents: usize,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Limits {
    pub available_nodes: usize, pub shown_nodes: usize, pub omitted_by_depth: usize,
    pub omitted_by_node_limit: usize, pub available_edges: usize, pub shown_edges: usize,
    pub omitted_edges: usize,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Diagnostics { pub missing_references: usize, pub cycle_detected: bool }
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Report {
    pub format_version: u32, pub task_id: i64, pub direction: Direction, pub depth: u16,
    pub max_nodes: u32, pub max_edges: u32, pub focus: Node, pub nodes: Vec<Node>,
    pub edges: Vec<Edge>, pub impact: Impact, pub limits: Limits, pub diagnostics: Diagnostics,
}
```

- [x] Internally store minimal metadata and parent IDs, edges, `HashMap<i64, Vec<usize>>` prerequisite/dependent edge indexes. Bulk SELECT `id,description,status,archived,parent_id,({READY_TASK_PREDICATE})` ordered by ID, then extra edges ordered by task/prerequisite. Missing IDs receive placeholder metadata. Build indexes in one edge pass; do not use TASK_COLUMNS' unrelated images/identity/prerequisite JSON. Deduplicate dependency IDs when counting blockers/direct impact; preserve parent/extra edge kinds in output.
- [x] Implement iterative BFS depth map and unfinished-blocker traversal:

```rust
fn distances(start: i64, adjacency: &std::collections::HashMap<i64, Vec<i64>>)
    -> std::collections::HashMap<i64, usize> {
    let mut found = std::collections::HashMap::from([(start, 0)]);
    let mut pending = std::collections::VecDeque::from([start]);
    while let Some(id) = pending.pop_front() {
        let depth = found[&id] + 1;
        for next in adjacency.get(&id).into_iter().flatten() {
            if let std::collections::hash_map::Entry::Vacant(entry) = found.entry(*next) {
                entry.insert(depth); pending.push_back(*next);
            }
        }
    }
    found
}
```

Build unfinished-only adjacency in same edge pass; a completed prerequisite stops blocker reach. Kahn indegrees/reverse edges detect cycles without recursion. BFS both directions independently; remove root from impact sets even if corrupt cycle returns to root.
- [x] Compute unique direct and transitive real-task sets. For each direct real task, use current SQL ready flag; new/unarchived candidate future-ready iff every prerequisite is focus or completed. Count currently-false/future-true as immediate, other unfinished dependencies as still-blocked, ready tasks as already-ready, other states inactive. Root already completed yields zero new readiness. Keep exact totals independent of display direction/depth.
- [x] Presentation selects focus first, then minimum-hop discovered candidates matching requested direction and depth until node cap. Sort selected IDs only; include each node once with both distances when relevant. Scan stable edge vector once to count edges between selected nodes and retain first max_edges. Limits expose omissions; never construct path lists or exponentially duplicated trees.
- [x] Add `Db::graph(reference, options) -> Result<Report>` wrapping `unchecked_transaction()`, existing `resolve_task_id` inside transaction, `graph::report(&tx, id, options)`, commit read transaction. CLI command handles before writable DB/owner resolution. Add Graph command name/output format, skip preflight in run. No change to existing List/Status/Next behavior.
- [x] `graph::lines(&Report) -> Vec<String>` renders focus status/reasons, exact impact counts and hypothetical explanation, upstream/downstream ID summaries, typed edge references, missing/cycle diagnostics and limit notices. `graph::render(&Value, Option<usize>)` deserializes typed report, sanitizes controls, clips human lines by grapheme cell width when terminal width exists. Plain text respects all color modes.
- [x] Add read-only tests comparing DB/config/sidecars before/after valid/missing graph under active stale owner and mutation-denying triggers. Old schema rejects migration without bytes change. Assert native/Herdr probes are skipped, snapshot fields coherent, JSON streams/errors preserved. Run graph/cli/output/dependencies/preflight focused tests; fmt/diff. Commit `[Feat] Inspect Dependency Graphs And Completion Impact`.

## Task 2: Bounds, Corruption And Readiness Parity

Files: `tests/graph.rs`, `src/graph.rs` unit tests.

- [ ] Add root/leaf and diamond fixtures: parent1 -> tasks2/3 -> shared4 -> leaf5; assert direct2, transitive4, indirect2, immediate2, shared nodes listed once. Completed focus produces zero new-ready count. Extra two-prerequisite integration retains remaining blocker when only first completes; recompute after actual completion and compare with list `--readiness ready` IDs.
- [ ] Mixed states include errored blocker, archived unfinished blocker (inject archived flag after creating edges), archived completed prerequisite, in-progress and completed dependents. Assert reasons and immediate/still-blocked/inactive partitions match shared readiness. Completed nodes terminate blocker chains; historical ancestors remain ordinary upstream relationships.
- [ ] Inject dangling parent/extra edges with foreign_keys off and a cycle via SQL in isolated DB. Report missing placeholders and cycle flag, does not panic/loop/repair; source bytes unchanged. Missing dependent rows count as diagnostics, not real tasks in impact totals. Duplicate parent/extra links retain kinds but count one dependent/blocker identity.
- [ ] Deep chain (at least 1500) and wide/shared layered graph (many theoretical paths) use batched SQLite fixture construction after CLI init. Assert exact downstream unique count and output below caps, no recursion overflow; depth0 emits only focus, direction doesn't expand unrelated siblings, truncation counts accurate, same bytes across repeated JSON calls. Max-node and max-edge parser boundaries reject invalid limits.
- [ ] Unit traversal tests prove minimum hop distance in diamonds, completed blocker stop, cycle termination, and at most one node summary per ID. Graph human output strips control/ANSI from hostile titles, includes counts and truncation, valid Unicode widths under narrow clipping. Run focused graph + shared readiness/dependency/queue suites; commit `[Test] Bound Graph Traversal And Verify Readiness Impact`.

## Task 3: Cached TUI Graph Inspector

Files: create `src/tui/graph_inspector.rs`; modify `src/tui/mod.rs`, `src/tui/render.rs`, `tests/tui.rs`, `tests/tui_dashboard_pty.py`, `tests/tui_dashboard_render.rs`.

- [ ] Add RED graph PTY fixture with two blockers and selected dirty integration task. Ctrl-O must show `Dependency graph #3`, blocker IDs and hypothetical impact; current UI has no inspector. Test command `CARGO_INCREMENTAL=0 cargo test --locked --offline --test tui tui_dashboard_graph_inspector_preserves_drafts -- --exact`.
- [ ] Controller owns `id: i64`, `options: graph::Options`, `version: Option<i64>`, `report: Option<graph::Report>`, `error: String`, `top: usize`. Methods: `new(id)`, `refresh(&Db, version)` only when version/options change, `key(KeyEvent)` returning Close or Refresh/None, `rows(width,height)` wrapped popup content. Error clears fresh report display, keeps inspector/editor alive. Arrows/Pg keys/Home/End affect top; u/d/b direction, +/- bounded depth invalidate version; Esc/Ctrl-C close.
- [ ] In compose_inner add inspector option alongside view/jump/conflict UI. Handle inspector keys/paste before generic editor/filter cancel; Ctrl-O opens only selected task and when no modal/confirmation active. Render inspector before other popup fallback; refresh using captured DB data_version each frame. Mouse background input disabled while inspector open. Modify only inspector state; retain target, buffers, draft/tag state, caret, editor/details top, filter focus/query and view context.
- [ ] Add Ctrl-O hint after existing prioritized shortcuts; update footer style shortcut expectations. Existing key ordering/hints stay visible at prior widths.
- [ ] PTY tests cover editor and focused filter entry, dirty selected and parked new/tag drafts, caret/manual scroll, active saved view, direction/depth/scroll, committed prerequisite updates producing refreshed readiness while no keyboard input, task deletion/error display, Escape/Ctrl-C cancel, narrow/wide resize and NO_COLOR. Always wait final rendered cursor/frame before DB/state assertions.
- [ ] Run `CARGO_INCREMENTAL=0 cargo test --locked --offline --test tui --test tui_db --test tui_dashboard_render --test tui_filter -- --test-threads=4`, fmt/diff; commit `[Feat] Inspect Task Blockers And Impact In TUI`.

## Task 4: Documentation, Review And Delivery

Files: create `docs/graph.md`; modify README/reference, this plan.

- [ ] Document exact graph flags/defaults, format_version1 fields/reasons, parent versus extra directions, hypothetical only-root completion, count scopes, completed blocker cutoff, corrupt references, global diagnostics, display bounds versus full exact totals and read-only schema policy. Two-prerequisite example A/B -> Integration shows completing A leaves B blocker. Ctrl-O controls and preserved drafts documented.
- [ ] Fresh fmt/diff, full locked/offline suite with process access, strict all-targets Clippy. Use CARGO_INCREMENTAL=0 for every cargo invocation. Record actual log/count evidence; no remote CI claim without a run.
- [ ] Request independent read-only reviewer via requesting-code-review skill with exact base/head/spec/plan. Fix verified findings and rerun affected/full checks. Build release; isolated graph CLI probes and graph PTYs against release. Commit `[Docs] Document Dependency Graphs And Verification`.
- [ ] Rebase onto current master, preserve other worker readiness/task185 changes, fast-forward after green checks. Verify integrated suite, install locked/offline/force, repeat installed graph CLI/PTys and compare root release/installed SHA256. Doctor healthy. Record final evidence, verification-only doc commit/merge.
- [ ] Queue message/complete/readback #186 only after delivery. Prove branch ancestor, remove own worktree/upstream/branch; preserve other workers. Resume exactly one `qqq next --wait --local --json`, retain confirmed silent waiter without polling/replacement.

## Evidence

- Task184 integrated baseline passed 779 tests/45 binaries at a0a166f. Fresh isolated task186 baseline passed 779 tests/45 binaries; `/tmp/qqq-task-186-baseline.log`.
- Spec self-review: no placeholders; exact totals and presentation limits separated; completed prerequisites stop blocker reach; immediate-ready never means transitive automatic completion; CLI skip-preflight is explicit read-only exception; TUI retains existing startup behavior. Rust graph analysis uses direct indexing and bounded-work tests; bundled complexity scanner does not analyze Rust.

- Task1 RED: two CLI tests failed only for unknown graph subcommand. GREEN: graph/CLI/output/dependencies/preflight passed 109 tests across five binaries; `/tmp/qqq-task-186-cli-focused.log`. Offline Herdr evidence intentionally stays unknown; confirmed gone-owner probe still recovers through ordinary show. Graph invokes neither probe nor writes. Db graph extension impl lives in graph.rs to keep database-only fixture modules independent of presentation.
