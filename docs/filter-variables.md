# Filter variables

All default variables for `qqq list --filter` and `qqq next --filter`. Names are case-sensitive, global within expression; no `task.` prefix. See [usage and semantics](filter.md).

| Variable | Type | Meaning |
| --- | --- | --- |
| `id` | number | Positive task ID; relative CLI indexes such as `-1` are not substituted. |
| `description` | string | Whole stored description, including title, later lines. |
| `task_name` | string | Alias for `description`, including later lines. |
| `status` | string | `"new"`, `"in_progress"`, `"completed"`, or `"error"`. |
| `priority` | number | Claim priority, -100 through 100; default 0. |
| `archived` | boolean | True when archived. `list --include-archived` required to see those rows. |
| `created_at` | string | Stored UTC creation timestamp, such as `2026-10-02T12:34:56.789Z`. |
| `created_time` | string | Alias for `created_at`. |
| `updated_at` | string | Stored UTC latest task-update timestamp. |
| `updated_time` | string | Alias for `updated_at`. |
| `parent_id` | number or nil | Parent dependency ID; nil for root task. |
| `harness_name` | string or nil | Recorded coding harness, such as `"codex"`. |
| `harness_session` | string or nil | Recorded public harness session. |
| `orchestrator_name` | string or nil | Recorded orchestrator, such as `"herdr"`. |
| `orchestrator_session` | string or nil | Recorded orchestrator server session. |

Ownership variables describe current row before any new claim. New tasks usually have nil ownership fields; filters on those fields mostly help `list`. Existing ownership persists independently of `next --filter` matching. Completing/releasing tasks clears current assignment fields.

```sh
qqq list --filter 'parent_id == nil and priority >= 0'
qqq list --filter 'harness_name == "codex" and harness_session ~= nil'
qqq list --filter 'coalesce(orchestrator_name, "") == "herdr"'
qqq list --filter 'date(created_time) == date("now")'
qqq list --include-archived --filter 'archived and status == "completed"'
```

`claim_key` is internal, unavailable. `context_only` is derived after filtering, unavailable. Parent rows, events, messages, images, readiness helpers, SQL tables are not expression globals. Unknown names produce `Invalid --filter`.
