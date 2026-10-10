# Orchestrator Pane Identity

Task #211: Herdr assignments currently expose server name (`default`) as
`orchestrator_session`. Users need terminal/pane running assigned task, such as
`term_65d8208adfc4145`.

## Contract

- Fresh Herdr claims and active link updates record nonblank `pane.terminal_id`
  as `orchestrator_session`; missing or blank terminal ID falls back to `pane_id`.
- Reported harness session remains independent from terminal/pane identity.
  Codex display overrides and private claim ownership retain current behavior.
- Dispatched assignments record child terminal/pane before sending prompt.
- Explicit assignment flags override detected values.
- Saved `Link.server` still routes Herdr lookup and liveness requests.
- Existing assignments retain recorded metadata until explicit override or active
  link update; completed history remains unchanged. No schema migration.
- Standalone assignments retain absent orchestrator fields.

## Implementation

Change shared `Identity::for_claim` constructor. Direct discovery, manual linking
and dispatched children already use it. Update CLI help and current docs.
Historical migration docs and migration fixtures keep original semantics.

## Verification

CLI regressions cover native/reported harness identity, missing/blank terminal,
failed server lookup, manual link, named server routing, explicit overrides,
filter/display consumers, completion retention and child dispatch. Existing
migration and standalone identity tests protect unchanged contracts. Run full
suite, fmt, strict Clippy, release build, independent review and installed CLI
smoke before completing task.
