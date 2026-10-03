# Multiple Prerequisites (#150)

**Goal:** Gate ready tasks on their tree parent plus independent prerequisites without changing tree placement, existing ownership, content revisions, or completed descendants.

**Required skill:** superpowers:executing-plans.

## Design

`add --depends-on ID` repeats. `edit --depends-on ID` adds edges; `--remove-depends-on ID` removes specific edges; `--clear-depends-on` removes all extras. Clear plus add replaces extras atomically. Clear plus remove, overlapping add/remove, duplicate additions, missing removals, self links, missing references, parent/extra overlap, and cycles across both edge types fail without partial changes. Dependency flags on edit skip the editor and conflict with explicit `--edit`; direct description/status/parent/priority/image edits can share the transaction. Parent-to-extra conversion works when final graph has no duplicate edge.

Schema v11 adds `task_dependencies(task_id, prerequisite_id)` with foreign keys, unique edges, a self-link check, and reverse index. All readiness queries use one predicate: new, unarchived, parent completed if present, every extra prerequisite completed. Edits and claims use immediate write transactions. Existing owner reuse remains unchanged; dependency changes and reopening prerequisites affect future claims only.

Archived unfinished prerequisites follow parent safety: reject linking unfinished tasks to them; reject archiving unfinished prerequisites with unarchived unfinished dependents; validate edges when unarchiving or reopening unfinished tasks. Completed archived prerequisites satisfy readiness. Deletion refuses any referenced prerequisite, including references from completed/archived tasks.

Normal Task JSON includes ordered `prerequisites` entries with ID, status, and archived state. Shared show/TUI detail rendering lists prerequisites and marks blockers. Queue diagnostics report `prerequisite_not_completed` and every unfinished extra blocker. Tree rendering continues using parent_id only.

Import v1 gains optional `depends_on`, an array of existing `{id}` or local `{key}` references. Parent plus prerequisites determine stable topological creation order. Validate combined local cycles and duplicate references before DB writes. Dry run resolves existing IDs and leaves local IDs null; result rows include source references and resolved prerequisite IDs. All inserted tasks and edges share one transaction.

Backup/restore accept schema 9 through 11. Current-schema snapshots validate the combined graph. Doctor checks schema 11 and graph references/cycles. Migration preserves IDs, status, ownership, history, priority, revisions and old readiness.

## Plan

- [x] Add CLI regressions in tests/cli/prerequisites.rs for mixed readiness, editing, ownership, archive/reopen, queue output, graph validation, concurrency and import; confirm failures.
- [x] Add src/sql/migrate_v11.sql and shared src/dependencies.rs graph/edge operations; update src/db.rs readiness and atomic add/edit/archive/reopen paths.
- [x] Add typed prerequisite metadata to Task projections; update src/main.rs flags, src/queue.rs explanations and src/output/detail.rs shared human/TUI details.
- [x] Extend src/import.rs validation, deterministic topology, preview results and transactional edge insertion.
- [x] Update src/delete.rs, src/doctor.rs, src/snapshot/backup.rs; verify migrations and snapshot/doctor round trips and corruption handling.
- [x] Update CLI and dependency docs, fmt, focused tests, full locked suite and strict Clippy; obtain independent review.
- [ ] Rebase on current master, rerun relevant checks, fast-forward locally, install CLI, verify installed behavior, complete #150 and resume persistent waiter.

## Validation before integration

- Four original CLI regressions failed on missing flags; import forward-ref regression failed on missing field; archived-prerequisite reopen regression exposed missing validation, then passed after shared validation fix.
- Twelve prerequisite CLI checks pass, including edge-trigger rollback, 12 claim/edit races, combined cycles and wait readiness. Herdr dispatch gating and color/no-color PTY refresh preserve dirty text.
- Full locked suite: 556 passed. Strict all-target Clippy, fmt and diff checks passed. Existing reviewer approved after restoring Messages immediately below Description and fixing a test qualifier typo.
- Concurrent task #148 landed structured JSON errors on master; integration must retain typed errors and rerun combined checks.
