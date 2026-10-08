# Atomic Bulk Task Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preview and atomically apply selected task metadata through CLI and TUI.

**Architecture:** Shared bulk core freezes direct selection, before/after metadata
and state fingerprints in typed version1 report. Readonly snapshot previews and
immediate apply transactions reuse shared archive/tag/priority checks. Dashboard
adds separate bulk modal/selection state without loading or saving editor drafts.

**Tech Stack:** Rust, Clap, serde JSON, SHA256, rusqlite, Ratatui/Crossterm,
existing subprocess and screen-aware Python PTY tests.

---

## File Map And Shared Types

- `src/bulk.rs`: typed preview/actions, snapshot selection, fingerprint checks,
  atomic application, file input validation and CLI arguments.
- `src/archive.rs`: single/bulk projected archive rules extracted from Db.
- `src/db.rs`: call archive helper, keep task query access crate-visible.
- `src/list_filter.rs`: expose direct matching helper, keep ancestor list behavior.
- `src/main.rs`: Bulk command/filter preparation, readonly preview and apply.
- `src/errors.rs`, `src/cli_error.rs`: BULK_CONFLICT and command identification.
- `src/output.rs`, `src/output/bulk.rs`: full safe human before/after preview.
- `src/tui/bulk.rs`: selection, menu/input/preview/error UI and scrolling.
- `src/tui/mod.rs`, `src/tui/dashboard.rs`, `src/tui/render.rs`: integrate modal,
  Ctrl-B/Ctrl-G and existing one-cell list marker without editor transitions.
- `tests/bulk.rs`: real CLI/DB acceptance; `tests/tui_bulk_pty.py`: reuse
  TerminalScreen from dashboard PTY support through isolated module extraction if
  needed; `tests/tui.rs`: named PTY entrypoints.
- Direct-source DB test crates: declare archive module alongside Db module.
- `README.md`, `docs/reference.md`: selector/action/preview/confirmation semantics.

Canonical core definitions (all public data derives Clone, Debug, Eq, PartialEq,
Deserialize, Serialize; each input struct denies unknown fields):

```rust
struct Actions {
    add_tags: Vec<String>,
    remove_tags: Vec<String>,
    set_tags: Option<Vec<String>>,
    priority: Option<i64>,
    archived: Option<bool>,
}
struct Metadata { tags: Vec<String>, priority: i64, archived: bool }
struct Row { id: i64, before: Metadata, after: Metadata, fingerprint: String }
struct Report {
    version: u32,
    applied: bool,
    database: String,
    actions: Actions,
    selected_ids: Vec<i64>,
    selected_count: usize,
    changed_count: usize,
    action_count: usize,
    tasks: Vec<Row>,
}
enum Selection<'a> {
    Ids(&'a [i64]),
    Filter { query: Option<&'a str>, statuses: &'a [ListStatus],
             include_archived: bool, filter: Option<&'a CompiledFilter> },
}
fn preview(db: &Db, selection: Selection<'_>, actions: Actions) -> Result<Report>;
fn apply(db: &mut Db, report: &Report, actor: &str) -> Result<Report>;
fn read(path: &Path) -> Result<Report>;
```

Preview uses unchecked deferred transaction on existing connection, then Db
selection queries participate in same transaction. Apply immediate transaction
uses crate-visible TASK_COLUMNS/task_row. Functions compute canonical DB path
from connection.path(); do not allocate data elsewhere.

## Task 1: CLI Frozen Preview And Basic Apply

- [x] Create TempDir CLI fixture removing agent/Herdr env, PATH isolated, init,
  DB bytes helper and command JSON parser. First regression:

```rust
f.ok(&["add", "First", "--tag", "old"]);
f.ok(&["add", "Second"]);
let before = f.bytes();
let preview = f.ok(&["bulk", "--id", "2", "--id", "1", "--id", "2",
    "--add-tag", "review", "--priority", "5"]);
assert_eq!(preview["selected_ids"], json!([1,2]));
assert_eq!(preview["selected_count"], 2);
assert_eq!(preview["changed_count"], 2);
assert_eq!(preview["action_count"], 4);
assert_eq!(preview["tasks"][0]["before"]["tags"], json!(["old"]));
assert_eq!(preview["tasks"][0]["after"]["tags"], json!(["old","review"]));
assert_eq!(f.bytes(), before);
f.write("preview.json", &preview);
let applied = f.ok(&["bulk", "--apply", "preview.json"]);
assert_eq!(applied["applied"], true);
assert_eq!(f.ok(&["show","1"])["task"]["priority"], 5);
```

- [x] Run `cargo test --locked --offline --test bulk -- --test-threads=1`;
  require expected unknown bulk subcommand failure before production changes.
- [x] Add Args and Commands::Bulk; require ID/selector/all selection and at least
  one action unless apply; enforce conflicts with Clap and pre-open validation.
  Normalize actions: trim/dedupe tags, reject add/remove overlap, bounds check,
  reject set+add/remove and archive+unarchive. No-op actions still valid.
- [x] Implement typed report and read file/stdin with strict JSON; direct selector
  function shares List query/status matches without expanding ancestors. Integrate
  Bulk tags/filter into existing run compile path before liveness preflight.
- [x] Implement basic preview/read/apply using transaction, exact IDs/metadata,
  fingerprint and structural report validation. Basic metadata writes update
  only tags/priority/archived/updated_at, preserve content_revision.
- [x] Add safe human renderer with every ID and before/after/count fields;
  human preview ends with explicit command instruction using saved JSON file.
  Output defaults still controlled centrally. Re-run focused plus CLI/list/filter
  suites; commit first verified checkpoint.

## Task 2: Shared Archive Rules And Transaction Acceptance

- [x] Add tests for tag operations, priority bounds, active/archive dependent rules
  and atomic projected archive/unarchive graph before moving validation:

```rust
f.ok(&["add","Parent"]);
f.ok(&["add","Child","--parent","1"]);
let preview = f.ok(&["bulk","--id","1","--id","2","--archive"]);
let applied = f.apply(&preview);
assert_eq!(applied["changed_count"], 2);
assert_eq!(f.ok(&["show","1"])["task"]["archived"], true);
assert_eq!(f.ok(&["show","2"])["task"]["archived"], true);
let unarchive = f.ok(&["bulk","--id","1","--id","2","--unarchive"]);
f.apply(&unarchive);
```

- [x] Shared archive helper reads parent/extra/dependent rows, substitutes final
  `BTreeMap<i64,bool>` archive values before checking. Same actor/nonempty,
  in_progress/unfinished-child/archived-prerequisite structured messages and
  no-op behavior as current Db::set_archived. Empty map preserves single behavior.
  Add module declarations to all direct-source Db test crates discovered via rg.
- [x] Guard fingerprint task JSON + private claim_key + MAX(event.id) in same
  snapshot; task prerequisite states included. Readiness-related archive failures
  on new related rows reject whole apply. Compare every selected row before SQL.
- [x] Add selector matrix: direct children omit context parents, query/status/tag/
  Luau AND, exact tags, repeated status OR, completed rows no display cap,
  explicit archived IDs, positive/duplicate/missing IDs, explicit --all and empty.
- [x] Add malformed CLI/JSON matrix before writable open: missing actions/selection,
  conflicts, unknown fields/versions/duplicates/types, counts/IDs/hash consistency,
  after-values mismatch, already-applied report and cross-project DB binding.
- [x] Assert preview DB bytes, claims/sequence/staging stay unchanged; older
  schema reports migration-required without normal writable open. Existing native
  preflight semantics tested separately; unknown live owner remains unchanged.
- [x] Freeze selection test: preview query, add newly matching row, apply affects
  original IDs only. Stale metadata/content/owner/dependency/delete cases ->
  BULK_CONFLICT and zero changes, including prior unchanged selected rows.
- [x] Inject late update and archive event failure; SQL transaction rolls back
  tasks and history. No-ops leave bytes/updated_at/events unchanged.
- [x] Run two CLI applies of same non-noop preview concurrently: exactly one
  complete success, other conflict; no partial graph or overwritten concurrent
  edit. Archive parent/child/extra prerequisite tests exercise both directions.
- [x] Verify full descriptions/images/revisions/owners/private records/links/
  dependencies/messages/prior history preserved. Run bulk, archive, priority,
  dependencies, list/filter/CLI, fmt and strict Clippy; commit checkpoint.

## Task 3: TUI Multi-Selection And Preview Flow

- [ ] Add PTY fixture with three tasks, one dirty draft and tag metadata. Capture
  real rendered screen for Ctrl-B marker and bulk menu before DB assertions:

```python
send(b"\x1b[A")                    # navigate existing draft caret safely
send(b"\x02")                      # Ctrl-B selects current saved task
wait_screen_contains("Bulk selected: 1")
send(b"\x1b[1;2A")                 # Shift-Up switches, parks dirty draft
send(b"\x02\x07")                 # select second task, Ctrl-G bulk menu
wait_screen_contains("Bulk actions (2)")
send(b"p5\r")                      # priority input -> frozen preview
wait_screen_contains("Bulk preview")
assert_saved_priorities([0,0,0])
send(b"n")                         # cancel keeps both selection + draft
wait_screen_contains("Task Editor")
```

  Actual navigation chooses existing saved tasks from initial new draft; test
  frames/caret positions and exact saved draft expectations. Add wrapper Rust
  test entry and run expected missing marker/menu RED.
- [ ] Add `src/tui/bulk.rs`: View stages Menu/Input/Preview/Error; key handlers
  return typed intents Preview(Actions), Apply(Report), Close/Clear/None. Input
  tags reuse tag_input::Cursor key/paste/rows; Enter newline, Ctrl-S preview.
  Numeric Enter previews; invalid input keeps text+cursor. Menu arrows/Enter
  and labeled shortcuts; no implicit confirm. Selection uses BTreeSet IDs.
- [ ] Add modal slot alongside jump/conflict/action in compose_inner. Bulk modal
  consumes paste/mouse/key input before editor. Add Mode::bulk_preview and
  bulk_apply operating on actual dashboard Db with actor tui, never run_action
  or load_target. Preserve all draft/top/filter/follow variables on every path.
- [ ] Ctrl-B toggles current saved ID, emits selected count in existing footer;
  empty target gives helpful message. Ctrl-G chooses bulk menu when marks exist;
  no marks leaves current single menu unchanged. Clear menu clears set only.
- [ ] dashboard View adds selected-ID reference for prefix marker; update
  standalone dashboard-render test initializers. Replace one-cell blank prefix
  with plus only on first row of marked task; continuation rows remain blank.
  Keep pane widths/hit targets and one-cell prefix contract unchanged.
- [ ] Scroll preview all rows within popup height, fixed heading/footer always
  visible, PgUp/PgDn/Up/Down, resize-safe top clamping. n/Esc/Enter cancel;
  Ctrl-C closes. y applies frozen report, no fresh selector call. Success clears
  marks and reports changed tasks; conflict/error preserves marks and drafts.
- [ ] Extend PTY checks: add/remove/replace tags, empty replace, priority,
  archive/unarchive and active rejection; hidden selection; cancellation;
  stale DB edit while preview open; dirty current/parked drafts, caret/paste
  atoms/attachments/filter/list+editor scroll restoration; compact/NO_COLOR
  and resize. Observe final frames before task assertions.
- [ ] Run focused bulk PTY, existing dirty-buffer/tag/navigation/action tests,
  model/render tests, fmt/Clippy; commit verified TUI checkpoint.

## Task 4: Documentation, Review And Delivery

- [ ] Document full selector/action syntax, preview JSON shape/path binding,
  counts/no-ops, conflict regeneration, final graph archive rules and no raw
  owner token. Add actual marked CLI example commands exercised by test from
  project root/subdir. Document Ctrl-B/Ctrl-G, marker, modal input and confirm,
  filter-hidden selection and retained drafts. Add BULK_CONFLICT reference row.
- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --locked --offline
  --all-targets -- -D warnings`, full serial `cargo test --locked --offline --
  --test-threads=1`, `cargo build --locked --offline --release`. Historical
  compatibility must confirm schema13 and immutable artifacts unchanged.
- [ ] Request focused read-only review through existing reviewer per requesting
  review skill. Fix important findings with regression tests and affected checks.
- [ ] Check current root master; clean rebase preserving concurrent task #184
  changes. Re-run affected/full gates if source changes. Fast-forward master,
  root install `cargo install --path . --locked --offline --force`.
- [ ] Root and installed checks: documented CLI preview/apply, frozen/conflict/
  rollback/noop, installed PTY multi-selection and dirty/caret/filter/scroll
  behavior, historical compatibility matrix, binary SHA match and root doctor.
- [ ] Record verification evidence, update plan, commit/integrate docs. Complete
  #185 through qqq, read back completion, remove owned merged worktree/branch,
  resume one persistent `qqq next --wait --local --json` process.

## Progress

Base6ba310c: fresh full combined suite761 tests/44 binaries already passed for
prior recipe delivery. New worktree build and fresh CLI/archive/priority/TUI DB
baseline111 tests/4 binaries pass. Design self-review complete; one shared bulk
transaction, no migration, no unresolved material choices. Implementation pending.

First checkpoint RED: bulk4 and archive graph1 rejected missing bulk subcommand.
GREEN: bulk5 plus CLI/archive/priority/TUI DB/list/filter suites pass132 tests
across7 binaries; fmt/diff checks and strict all-targets Clippy pass. Shared
archive extraction included early because graph/active-row test precedes core
archive exposure. Human renderer exists; dedicated human/default-output checks,
broader validation/rollback/concurrency and TUI implementation remain pending.

CLI acceptance checkpoint: bulk22 plus existing CLI/archive/priority/dependencies/
list/filter/tag-routing/TUI DB checks pass164 tests across9 binaries. Strict
all-targets Clippy and fmt pass. RED tests exposed unconditional metadata-column
writes, ignored update/audit inserts and early global-identity-validation bypass;
column-specific bound SQL, affected-row checks and normal identity-validation
ordering fix all four gaps. Full preserved claims/links/content/images/graph/
history, stale attachment/parent/prerequisite state/private claim changes,
concurrent notes and competing applies verified. Human and native-agent output,
stdin explicit confirmation, schema/staging purity and extra graph archives pass.
TUI implementation, docs/examples and full delivery checks remain pending.
