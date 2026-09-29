# Error Status

Task #25 needs visible failures requiring manual handling. Current statuses only
cover queued, active, and completed tasks.

## Flow

- Owner runs `qqq edit <id> --set-status error --reason "Failure details"`.
- Only an in-progress task owned by the supplied/discovered session can fail.
- Failure clears assignee, records an `error` event and reason as a message in
  the same transaction as optional title/description edits.
- Error tasks remain in `list`, including when completed rows are hidden. Human
  output says `Error`; terminal list rows use red. JSON status is `error`.
- `next`, including `--wait`, skips error tasks. A failed parent keeps children
  blocked. Workers can claim another ready task after reporting failure.
- User handles task, then runs `qqq edit <id> --set-status new`. Error tasks have
  no owner, so any local caller may retry; no Herdr session is required. Supplied
  session records retry author; otherwise history author is `manual`.
- Existing in-progress release still requires its owner. New/completed tasks
  cannot be released. Retrying preserves messages, attachments, dependencies,
  and latest Herdr link; a subsequent claim clears/replaces link as today.
- `--reason` is valid only with error status and cannot be blank. Status edits
  skip editor. Wrong owner/status or invalid content leaves all data unchanged.
- Generic CLI errors do not change task status. Dispatch startup rollback and
  ambiguous prompt-delivery ownership retain their established behavior. Agent
  dispatch instructions explain how to report failed work.

## Persistence

Schema version 4 rebuilds tasks/events to extend their status/action CHECK
constraints. Disable foreign keys before the immediate migration transaction;
re-enable after success. Copy every column, ID, timestamp, owner, and dependency,
preserve AUTOINCREMENT high-water marks, recreate existing indexes, check foreign
keys before commit. Versions 1/2 pass through existing migrations; concurrent
opens recheck version after obtaining write lock. Newer versions remain rejected.

## Verification

Regression tests cover error/retry lifecycle, dependency blocking, owner rejection,
atomic combined edits, required reason, session-free manual retry, preserved task
details, human/JSON rendering, waiting until explicit retry, legacy migration
including deleted IDs, concurrent migration, DB constraints and foreign keys.
Run full Rust tests, formatting, strict Clippy, release build before integration.
