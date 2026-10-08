# Dependency Graph And Completion Impact

Task #186. Base `a0a166f`; named views and recipe support retained.

## Commands And Read-Only Contract

`qqq graph ID [--direction both|upstream|downstream] [--depth N]
[--max-nodes N] [--max-edges N] [--json|--human]` inspects one task.
Positive IDs and existing negative creation-order references use current lookup
rules. Missing tasks return TASK_NOT_FOUND; invalid flags keep parser errors.
Defaults: both directions, depth 8, 1000 displayed nodes, 2000 displayed edges.
Depth accepts 0..128; node cap 1..10000, edge cap 1..20000.

CLI uses `Db::open_read_only`, skipping owner-recovery preflight, claim/session
resolution, migrations and Herdr dispatch. Existing read-only DB safety/schema
checks apply. One deferred read transaction covers ID resolution, minimal task
rows, readiness and dependency edges. No task/priority/content/edge/file writes.
TUI graph inspection uses the dashboard connection inside a read transaction;
it adds no recovery or mutation. Existing dashboard startup behavior remains.

## Snapshot And Indexed Traversal

Create `src/graph.rs` with typed options, indexed graph, report and renderer.
Bulk-select minimal task metadata plus existing READY_TASK_PREDICATE; separately
select all extra dependency edges. Parent IDs become parent-kind edges. Direction
always stores dependent -> prerequisite, independent of traversal direction.
Maintain incoming prerequisites and reverse dependents by task ID. Missing
references become explicit missing-node records; report warns about missing
references and cycles. No repair writes, recursion or repeated DB queries.

Upstream and downstream breadth-first searches start independently at focus;
never traverse upstream then expand unrelated siblings downstream. Visit each
node at most once per traversal, retaining minimum hop depths. Diamonds/shared
descendants appear once. A blocker traversal follows only unfinished/missing
prerequisites, stopping at completed tasks so historical unfinished ancestors
behind a completion are not described as current blockers. Cycle detection uses
linear Kahn traversal, reports a project-wide cycle flag, terminates on corrupt
graphs without claiming residual nodes are all cycle members.

Preprocessing and analysis are O(V + E), bounded by snapshot size. Bounded output
sorting costs O(R log R), with R displayed nodes. Depth/node/edge flags bound
presentation; exact impact totals still analyze full graph once. No path
enumeration or subtree duplication. Parent edges precede extra edges in stable
input order; displayed node summaries sort by ID, edges use stable source order.

## Stable Report

JSON format_version 1 includes task_id, direction, depth, max_nodes, max_edges,
focus summary, nodes, edges, impact and limits. Node summaries contain ID,
first-line task name, optional status/archive fields (null for missing nodes),
ready flag from shared SQL, direct unfinished dependency count, optional
upstream/downstream hop depths, blocks_focus flag and optional direct-impact
classification. Edges contain dependent ID, prerequisite ID, kind
(`parent`/`prerequisite`) and whether prerequisite is unfinished/missing.

Impact counts refer to real tasks, exclude focus and deduplicate shared nodes:

- direct_dependents: unique tasks with an edge to focus.
- transitive_dependents: all reachable downstream tasks, including direct ones.
- indirect_dependents: reachable tasks outside direct set.
- immediately_ready: currently blocked new/unarchived direct tasks whose only
  unfinished dependency is focus.
- still_blocked: new/unarchived direct tasks retaining other unfinished/missing
  prerequisites after hypothetical completion of focus.
- already_ready: direct tasks already ready in current snapshot.
- inactive_dependents: direct completed/error/in-progress/archived tasks, which
  do not become queued-ready from dependency completion alone.

Hypothesis changes only focus status to completed; all other statuses and edges
stay fixed. It does not predict recursive completion of newly ready tasks or
claim permission to complete focus. Completing an already completed focus makes
zero new tasks ready. Archived completed prerequisites satisfy readiness;
archived unfinished and error prerequisites remain blockers. Missing references
remain unfinished. Predict readiness using indexed prerequisite status lookup
under the same conditions as READY_TASK_PREDICATE; verify against real updates.

Limits report available/shown nodes, omissions by depth/node cap, eligible/shown
edges and edge omissions; impact counts remain exact regardless of direction or
display limits. Displayed-node impact annotations carry remaining unfinished
dependency counts after focus completion, so unrelated blockers stay clear.
Project-wide missing-reference count and cycle flag are diagnostics, distinct
from selected task's blocker reach. No unbounded lists of paths or hidden IDs.

## Human Output

Render focus state, hypothetical impact counts, upstream and downstream node
summaries and typed edge references. Shared nodes get one summary; edges refer
to IDs rather than reproducing subtrees. Include explicit truncation notice and
explain total counts are independent of display limits. Sanitize human controls;
JSON preserves text without terminal formatting. Respect NO_COLOR/TERM=dumb.
Renderer may use plain text in every environment for stable accessible output.

## TUI Inspector

Ctrl-O opens selected task's graph from editor/filter; no selection gives a
footer hint. Inspector uses existing popup rows, heading and scrolling. Up/Down,
PgUp/PgDn, Home/End scroll. `u`/`d`/`b` switch direction; `+`/`-` adjust depth
within 0..128. Esc/Ctrl-C closes only inspector. Explain read-only hypothetical
impact in heading/body. Narrow/resized terminals retain useful heading and
selected viewport, no-color uses existing neutral styles.

Cache report by selected ID/options/data_version. Refresh on committed DB
changes, retaining inspector scroll where possible. Missing/deleted task or
read failure displays an inspector error while retaining opened editor/draft.
No background graph recomputation on idle frames. Mouse input while inspector
is open does not edit background panes.

Opening, navigating, closing and refreshing inspector never switches editor
target or changes description/tag drafts, parked buffers, caret, editor/details
scroll, live filter text/focus or active named view. Stored graph data is never
written to project views/config/DB.

## Verification And Delivery

Meaningful RED CLI tests cover graph absence, typed edges, two unfinished
prerequisites, direct versus transitive impact, read-only byte preservation.
Add roots/leaves, diamonds, deep/wide graphs, completed/error/archived states,
dangling/cyclic fixtures, negative/missing references, depth/node/edge limits,
determinism, readiness parity and snapshot coherence tests. Deep/layered shared
graphs demonstrate no recursive stack growth or exponential path rendering.

PTY RED for Ctrl-O then verify dirty selected/parked drafts, caret/manual scroll,
filter and active-view retention, graph refresh after dependency updates,
direction/depth controls, delete/error display, no-color and resize. Run focused
and full suites, fmt, strict Clippy, independent read-only review, release and
installed graph CLI/TUI probes. Document two-prerequisite example and JSON
contract. Rebase/merge locally, install, verify DB health, complete #186, remove
own worktree/branch, resume exactly one persistent queue waiter.
