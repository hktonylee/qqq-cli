# Queue Diagnostics

Task #147 adds `qqq status` and `qqq next --explain` without changing claim,
preview, wait, owner discovery, dispatch or transition behavior.

## Commands

- `qqq status [--include-archived]`: global overview; no owner discovery.
- `qqq next --explain [--filter EXPR] [--include-archived]`: read-only selection
  explanation. No session/Herdr discovery or dispatch, including configured dispatch.
- Explicit `--session`/`QQQ_SESSION` or `--harness-session` supplies optional owner
  context for explanation. Resolve using same exact-key/unique-harness rules as
  real next; `--harness-name` disambiguates. No assignment metadata updates.
- Explanation conflicts with `--wait` and `--dry-run`. Next's
  `--include-archived` requires `--explain`; archived tasks never become candidates.
  `--local` remains accepted with explanation but performs no claim/dispatch.
- Regular next still reuses owned task before filtering. Dry-run still previews
  queued candidate only, even with owner context.

## Snapshot and Readiness

Open diagnostic DB with SQLite read-only flag. Reuse existing directory discovery
without creation, migrations, deletion recovery, image reads or application file writes.
SQLite may maintain WAL coordination sidecars even with a read-only DB connection.
Require current schema; older versions receive migration guidance through normal
command. No historical ownership lease or activity expiry is introduced.

Use one deferred transaction for rows, readiness/filter evaluation, latest activity,
owner resolution and selected task. No locks span waits. Shared SQL readiness
predicate is used by claims, preview, dispatch availability
and diagnostics: `new`, unarchived, no parent or completed parent. Candidate order
is priority descending then ID ascending. Parent blockers identify immediate
parent with status/archive state; blocked chains remain visible through each row.
Missing parent is reported rather than causing readiness to diverge.
Diagnostic selection reuses evaluated matching ready rows so time-sensitive
filters cannot drift between eligibility counts and selected candidate.

All tasks are read to resolve blockers, including archived parents. Default output
rows/counts exclude archived tasks; explicit include exposes them but never makes
them ready. `blocked` means new but not ready, including archived new tasks when
included. `archived` count is subset of total, not extra category. Filter matching
is separate from readiness and does not change unfiltered ready/blocked counts.

Latest activity compares task `updated_at`, latest message and latest event.
Expose timestamp, source, source ID, session and event action. Timestamp ties
prefer message then event then task update; source IDs break ties within source.
Task update covers creation, edits and other task field updates. Activity is
informational and never changes or expires claims.

## JSON Contract

Shared report fields:

- `include_archived`: requested archive scope.
- `counts`: total, new, ready, blocked, matching_ready, in_progress, error,
  completed, archived. Ready + blocked = new; matching_ready is ready subset.
- `state`: empty, ready, no_matching_ready, blocked, error, in_progress, no_ready.
  Blocked/error/in_progress classify queues containing only that unfinished class;
  mixed unavailable or completed-only queues use no_ready. Historical completed
  rows do not hide blocked/error/in-progress queue states.
- `tasks`: creation-ID order; each row has normal task object, ready flag,
  matches_filter flag, readiness rank (unfiltered priority/ID order), reason codes,
  owner key for active claim, blockers and latest_activity.
- `explanation`: null for status; selection diagnostics for --explain.

Explanation fields: owner_input, resolved_owner, outcome, ordering
(`priority_desc`, `id_asc`), eligible_ids and selection. Outcome is owned_task_reuse,
ready_candidate, empty_queue, no_matching_ready_candidate, blocked_queue,
error_queue, in_progress_queue or no_ready_tasks. Selection is null or
`{kind: owned|queued, task: normal_task_object}`. Owned reuse ignores filters and
archive exclusion exactly as normal next; selection may identify archived owned
task absent from scoped rows. All fields describe same transaction snapshot.

Reason codes: archived, parent_not_completed, in_progress, error, completed,
filter_excluded. Ready matching rows have no reasons. Filter exclusion is reported
separately even if another reason already prevents claiming.

## Human Output

Print counts and clear queue-state sentence. Status defaults to unfinished details
plus completed count; JSON includes all scoped tasks. Explanation includes all
scoped rows so filtering and status exclusions are inspectable. Each row shows
ID/status/readiness, priority, first description line, active owner/public identity,
parent blockers and latest activity. Escape control characters using existing
output sanitization. Do not imply ownership expiry or automatically reassign work.

## Verification

Cover empty, priorities/ID ties, blocked chains, error-only/mixed queues, completed
and archived scopes, filters, explicit owner reuse despite filter, exact/unique
harness owner matching, no native/Herdr discovery/dispatch, read-only SQL triggers,
old-schema non-migration, no project creation, latest edits/messages/events, and
coherent results during concurrent transitions. Compare queued selection with
real dry-run and owner selection with normal next. Preserve existing next/wait,
identity, dispatch, priority, dependency and filter checks. Full checks, review,
installed CLI smoke and local integration precede task completion.
