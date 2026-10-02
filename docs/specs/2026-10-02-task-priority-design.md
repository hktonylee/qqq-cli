# Task Priority and Claim Order Design

Every task has signed integer `priority` from `-100` through `100`, default `0`. Larger numbers claim first. `qqq next` keeps returning an already-owned in-progress task before considering queued work. For new ready work, it selects highest priority, then smallest task ID. Parent readiness rules stay unchanged; a blocked high-priority child cannot jump ahead of ready tasks. List order remains ID/dependency-tree order so browsing stays stable.

Schema version 7 adds `tasks.priority INTEGER NOT NULL DEFAULT 0 CHECK(priority BETWEEN -100 AND 100)` and a partial queue index on `(priority DESC,id)` for new tasks. Migration runs under the existing immediate transaction and rechecks `user_version` after acquiring its lock. Existing rows become priority 0, so their claim order remains FIFO. Only migrations that rebuild old tables disable foreign keys. Version 7 is accepted on open; newer versions are rejected without writes.

`qqq add --priority N` sets priority for created tasks, including interactive creation. Browsing/editing an existing task through `add` retains its priority. `qqq edit ID --priority N` changes priority in the same transaction as supplied description, parent, images, or status transition. A priority-only edit skips the editor and preserves status, claim ownership, and identity fields. Range validation occurs in Clap and DB methods; the SQLite CHECK also protects direct DB writes.

All task JSON responses, including `add`, `edit`, `next`, `list`, and `show`, contain numeric `priority`. Human `show` and task summaries show `Priority:`; CLI `list` gains a compact `PRI` column. TUI continues using its existing compact tree renderer; TUI priority controls and display belong to queued task 70. README documents range, default, ordering, list-vs-claim order, and examples.

Tests cover CLI bounds, default/edit behavior across statuses, ownership retention, JSON and human output, ready-parent gating, equal-priority ID ties, concurrent claims, version-6 migration and concurrent migration opens, and unchanged FIFO order for migrated tasks.
