# Dependency graph and completion impact

Inspect prerequisites, unfinished blocker chains and downstream dependents:

```sh
qqq graph 12
qqq graph -1 --direction upstream --depth 3
qqq graph 12 --direction downstream --max-nodes 100 --max-edges 150 --json
```

Positive IDs identify tasks; negative indexes use creation order (`-1` newest).
Zero is invalid. Missing IDs return `TASK_NOT_FOUND`. A task with no relationships
still produces a report containing its focus node and zero impact totals.

| Flag | Default | Values |
| --- | --- | --- |
| `--direction` | `both` | `both`, `upstream`, `downstream` |
| `--depth` | `8` | `0` through `128` |
| `--max-nodes` | `1000` | `1` through `10000` |
| `--max-edges` | `2000` | `1` through `20000` |

Upstream follows requirements of the focus task. Downstream follows tasks that
require it. Each direction starts independently from the focus; inspecting an
upstream prerequisite does not pull its unrelated children into the report.
Depth is minimum hop distance. Depth zero displays only the focus. Parent edges
are labeled `parent`; extra `--depends-on` links are labeled `prerequisite`.
Human edges read `dependent <- prerequisite`.

## Blockers and immediate impact

Readiness matches worker pickup: task must be new, unarchived, with completed
parent and every extra prerequisite completed. Error tasks require retry;
in-progress, completed and archived tasks are not queue candidates. An archived
completed prerequisite still satisfies its requirement. Missing prerequisites
remain blockers. Status and archive state are both shown.

`blocks_focus` marks reachable unfinished requirements. Traversal stops at a
completed prerequisite: older unfinished ancestors behind that completed task
remain historical upstream relationships, but do not block the focus. Inspection
does not reopen or retry tasks.

Impact assumes **only the focus task becomes completed**. Other requirements
keep their current states. Newly ready tasks are never assumed completed.

```sh
qqq add "API"                       # #1
qqq add "UI"                        # #2
qqq add "Integration" --depends-on 1 --depends-on 2   # #3
qqq add "API checks" --parent 1      # #4
qqq add "Publish checks" --parent 4  # #5
qqq graph 1
```

Completing API immediately releases API checks. Integration remains blocked by
UI. Publish checks is reachable downstream, but still needs API checks completed.
Result: two direct dependents, three transitive dependents including those two,
one indirect dependent, one immediately ready, one still blocked.

Direct real tasks are partitioned into `immediately_ready`, `still_blocked`,
`already_ready`, and `inactive_dependents`. Already-ready tasks do not count as
newly ready. Completed focus yields zero newly ready tasks. Transitive totals
count each real task once, including direct dependents; indirect is transitive
minus direct. Diamonds and duplicate parent/extra links do not inflate counts.
Missing placeholder tasks are excluded from impact totals.

## Bounds, diagnostics and read-only access

Display caps bound node and edge summaries. Focus is retained first, then nearby
nodes are discovered in minimum-hop order; retained nodes are emitted by ID.
Edges have stable parent-then-extra ordering. Exact impact totals analyze the
full project snapshot regardless of direction, depth or display caps. `limits`
separates nodes omitted by depth, eligible nodes omitted by node cap, and edges
between displayed nodes omitted by edge cap. A small displayed graph can therefore
have a large exact impact count.

Snapshot uses two bulk reads, indexed iterative traversals and cycle detection.
In-memory graph work is linear in project nodes/edges, plus sorting retained
display nodes; indexed SQLite readiness checks reuse worker eligibility.
reports do not enumerate paths or repeat shared descendant trees.
`diagnostics.missing_references` counts project edges with a missing endpoint;
`cycle_detected` reports any project cycle, including outside displayed region.
These warnings do not repair corrupt links. Missing displayed endpoints have null
name/status/archive state and `missing_task` reason.

CLI `graph` opens the existing DB read-only. It skips owner-recovery preflight,
native/Herdr probes, claims and dispatch. Browsing cannot change task status,
priority, assignment, edges, messages, config or view definitions. Schema
migration is refused with `DATABASE_ERROR`, reason `migration_required`; run a
normal command such as `qqq list` first. Existing WAL/SHM safeguard is retained:
unsafe WAL state returns `DATABASE_ERROR`, reason `unsafe_read_only`.
Ordinary list/status/next inspection retains its existing owner recovery.

Human output uses plain text, sanitizes controls in descriptions and clips at
terminal cell width. `NO_COLOR`, `TERM=dumb` and JSON emit no ANSI styling.
JSON retains exact description title strings through normal JSON escaping.

## JSON format 1

Top-level fields: `format_version`, `task_id`, `direction`, `depth`, `max_nodes`,
`max_edges`, `focus`, `nodes`, `edges`, `impact`, `limits`, `diagnostics`.
`focus` duplicates its entry in the node array for direct access.

| Object | Fields |
| --- | --- |
| Node | `id`, `task_name` (first description line), `status`, `archived`, `ready`, `reasons`, `unfinished_dependencies`, `upstream_depth`, `downstream_depth`, `blocks_focus`, `impact`, `remaining_after_completion` |
| Edge | `dependent`, `prerequisite`, `kind`, `unfinished` |
| Impact | `direct_dependents`, `transitive_dependents`, `indirect_dependents`, `immediately_ready`, `still_blocked`, `already_ready`, `inactive_dependents` |
| Limits | `available_nodes` (within requested depth), `shown_nodes`, `omitted_by_depth`, `omitted_by_node_limit`, `available_edges` (between displayed nodes), `shown_edges`, `omitted_edges` |
| Diagnostics | `missing_references`, `cycle_detected` |

Node reasons: `archived`, `in_progress`, `error`, `completed`,
`parent_not_completed`, `prerequisite_not_completed`, `missing_task`,
`unknown_status`. Ready nodes have no reasons. `unfinished_dependencies` counts
unique immediate requirements, including missing tasks. Direction not requested
or relationship not present yields null depth. Focus has depth zero in each
requested direction. Direct real dependents have `impact` classification and
`remaining_after_completion` count; other nodes have null values for those fields.

## TUI inspector

Ctrl-O opens graph for the opened saved task from editor or filter. A new unsaved
task has no persisted graph. Up/Down and PgUp/PgDn scroll; Home/End jump to start
or end. `u`, `d`, `b` select upstream, downstream or both; `+`/`-` adjust depth
within 0–128. Esc or Ctrl-C closes only the inspector.

Graph report is cached until DB commit or graph options change. External
prerequisite completion updates displayed blockers/readiness while inspector is
open. Deleted task or query error appears inside inspector without discarding
drafts. TUI startup keeps its ordinary owner preflight; graph browsing adds no
mutation. Paste, background clicks and save shortcuts do not edit through popup.

Opened/parked drafts, staged new-task tags, selected task, caret, manual editor
and details scroll, active saved view and live query/focus survive opening,
refresh and closing. Inspector supports narrow/resized terminals and `NO_COLOR`.
