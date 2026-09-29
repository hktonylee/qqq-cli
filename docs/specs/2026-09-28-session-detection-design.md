# Automatic Session Detection

Task #24: infer caller identity without qqq-specific session configuration.
Task #26 is complete; use existing private claim keys and public harness metadata.

## Resolution

Explicit `--session` / `QQQ_SESSION` override automatic sources. Exact Herdr
context means `HERDR_PANE_ID` is present, or `HERDR_ENV=1`; it takes priority over
native Codex environment. Query `herdr pane current --pane <HERDR_PANE_ID>`.
Without explicit ownership input, missing, blank or invalid selected context
fails before any claim; never fall through from an exact pane to another client
or cwd match. Explicit `--harness-session` retains task #26 behavior: recover
an existing public session, otherwise attempt auto-fill with local explicit
ownership fallback when discovery fails.

Outside exact Herdr context, prefer `CODEX_THREAD_ID`, then `CODEX_SESSION_ID`.
Both exist in current environment and installed Codex binary. Selected values
must be UTF-8 and nonblank; preserve exact nonblank values. Automatic ownership
uses collision-safe JSON tuple `["codex","id","<value>"]`. Native claims need
no Herdr installation, invocation or link. Public `harness_name=codex` and
`harness_session=<raw value>`; orchestrator fields remain null unless overridden.
No guessed environment names for other clients: use Herdr agent reports or
explicit ownership inputs.

When no native or exact source exists, retain unique Herdr agent discovery by
canonical database directory. Retain existing active-claim identity across
changes in Herdr session reporting. `next`, `next --wait`, `complete`, returning
tasks to new and setting error use same caller resolution. Explicit harness
metadata overrides retain current semantics; existing claims keep prior metadata
unless new overrides are supplied. Derive native metadata only for fresh claims,
inside DB write transaction. No schema migration.

## Boundaries

Add `src/session.rs`: typed owner key, optional Herdr link, optional native
metadata; resolution and dispatch caller helper. Herdr adapter stays responsible
for pane/cwd discovery, links and active Herdr identity retention. Manual
`herdr link` discovers Herdr independently of native Codex environment.

Configured dispatch first returns caller's existing native claim without Herdr
lookup or agent creation. Existing-claim retrieval applies overrides atomically
and never creates a fresh local claim if ownership disappeared while waiting
for DB lock. Creating a new agent still requires valid Herdr
workspace context; `--local` handles native callers outside Herdr.

## Verification

Real CLI tests cover native thread/fallback identity and metadata, overrides,
wrong-owner rejection, lifecycle, incoming wait, invalid environment, exact pane
without HERDR_ENV, Herdr precedence, dispatch existing claim, manual linking and
metadata preservation. Clear inherited caller environment in all test fixtures.
Run full suite, formatting, clippy with warnings denied, release build and
independent review. Rebase, integrate locally, rebuild CLI and smoke-test isolated
native and real Herdr caller contexts. Complete task only after verification.
