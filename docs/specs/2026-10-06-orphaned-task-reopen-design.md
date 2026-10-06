# Reopen orphaned Herdr work

Task #179 allows `qqq reopen ID` and TUI Reopen to return an in-progress task to
`new` when its owning Herdr agent has disappeared.

## Ownership and liveness

Completed tasks retain existing reopen behavior. New and error tasks remain
invalid. Archived tasks and archived unfinished dependencies retain existing
guards.

In-progress recovery requires a saved Herdr link associated with current claim.
Record private claim association in existing `herdr_links.link_json` when saving
links; public Link serialization stays unchanged. Existing automatic links can
be associated by their serialized `(agent, kind, value)` claim identity. Legacy
links for opaque owners without association cannot prove ownership and reject.
Automatic, dispatched, and explicitly linked future claims all gain association.
Linking associates current claim with pane without transferring ownership.

Query saved server's `agent list`. Exact agent identity or same saved terminal
with same agent kind means owner remains live, including moved panes and changed
session reporting. No matching agent or terminal in a successful response proves
absence. Failed commands, malformed responses, missing links and mismatched
claim associations reject without changing task. A failed server lookup is
unknown liveness, not proof of absence.

## Atomic transition

Read current task, claim association and saved link under immediate SQLite
transaction. Validate availability, then query liveness before updating. Keep
write lock through validation and update so concurrent reassignment cannot
invalidate checked claim. Successful recovery clears claim and four assignment
fields, advances updated timestamp, preserves content revision, description,
tags, priority, dependencies, messages, images, creation time and saved link,
then appends one `reopen` event attributed to caller. New claim replaces old
link. Concurrent reopen attempts commit at most once.

No schema migration, new flag or background claim expiration. Existing
owner-authorized release and force-reset remain available.

## Validation

CLI integration tests exercise automatic terminal and session claims, changed
reporting, moved panes, successful orphan recovery, current and legacy claim
associations, live owners, failed/malformed lookup, missing/stale links,
concurrent attempts, archived dependencies and preservation/reclaim behavior.
TUI action coverage exercises same recovery path with rendered success before
DB assertions. Run full Rust/PTY suite, fmt, Clippy, compatibility checks and
release build; integrate locally, update installed CLI, record evidence and
complete task before waiting again.
