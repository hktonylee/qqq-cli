# Harness and orchestrator identity

Task #26 replaces public `assignee` with nullable `harness_name`, `harness_session`, `orchestrator_name`, and `orchestrator_session`. User selected automatic Herdr metadata with explicit overrides.

## Identity and ownership

Harness identifies coding agent: Herdr agent kind plus reported agent session, or terminal ID when hooks have no session. Orchestrator identifies Herdr plus named server session. Explicit local claims use supplied session; unknown names remain null. Global flags override each field. `--session`/`QQQ_SESSION` remain stable ownership inputs; `--harness-session` supplies ownership input when no legacy session is supplied, and otherwise overrides displayed metadata. Exact internal ownership keys take priority over public-session lookup. Public sessions may recover active claims; multiple matches fail, with `--harness-name` available to narrow lookup.

Internal `claim_key` replaces live DB `assignee`, preserving opaque tokens, tuple encoding, active-session uniqueness, and status/ownership CHECK. It is absent from task JSON. Existing events/messages retain session strings. Completion, release, and error clear all current identity fields. Description, dependency and attachment edits preserve fields. Repeated next returns existing claim; explicit overrides may update its metadata.

## Auto-fill and migration

Schema v5 renames column, adds four fields, backfills active claims. Migration accepts compatible single-description schemas v1-v4. Legacy title schemas keep task #27 manual-update guard and remain untouched. Linked claims derive agent/session from stored Herdr identity and orchestrator name `herdr`; named server is copied when recorded. Unlinked claims retain old token as harness session; unknown metadata stays null. Historical server null remains unknown. No row IDs, timestamps, relations, attachments, events, or links change. Migration stays atomic and rechecks version after acquiring write lock.

Live auto-fill uses stored/current Herdr identity. Named server comes from explicit link server, otherwise matching `HERDR_SOCKET_PATH` against read-only `herdr session list --json`; absent socket means default server. Failed optional discovery leaves unknown metadata. External processes run before write transactions. Explicit overrides change metadata, never Herdr lookup identity. Dispatch stores real child identity before prompt while keeping generated owner token usable. Metadata/link update checks exact generated claim key; failed startup cleanup also uses exact key, preventing a stale dispatch from mutating a replacement claim. Lifecycle alias resolution happens once inside write transaction. Link updates on active tasks populate identity atomically; inactive task links do not recreate assignment metadata.

## Verification

Test public shape, field overrides, raw and legacy ownership, ambiguous sessions, atomic lifecycle, v1-v4 migration/concurrent opens, timestamps/history/images, unknown versions, Herdr auto/terminal identity, dispatch metadata before prompt, and TUI edit preservation. Run full tests, fmt, clippy with warnings denied, release build, independent review. Rebase concurrent changes before local master integration; build root binary before queue completion.

## Verification evidence

Rebased dependency-edit and task-detail color changes. Prior integration suite: 179 tests passed; final single-description integration checks recorded below. `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --release`, `git diff --check` pass. Regression coverage includes stale aliases during write-lock waits for complete/release/error, conflicting named socket with stored default server, replacement claim during child startup, and strict dispatch cleanup. Independent review covers migration, metadata contract, flags, server lookup and claim guards.
