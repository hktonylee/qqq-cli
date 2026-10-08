# Synchronous Owner Liveness Preflight

Task #181 originally requested globally singleton debounced worker. User replaced
that design: synchronously check owner before executing qqq command. No worker,
registry, timer, debounce, or background thread.

## Execution

After parsing/validation, before project command or TUI starts, scan current
project's in-progress claims. Every command includes inspection/dry-run commands:
preflight may mark dead owners error; command's own preview remains nonmutating.
Help/parser failures have no command execution. Config/restore without existing
project remain usable. Preflight never creates projects or migrates DBs; normal
project opens run preflight after any required migration. Older-schema inspection
retains migration-required behavior. Current-schema preflight with no confirmed
dead claims makes no DB writes. TUI/watch/next-wait run scan once before start;
no periodic scan while idle.

## Proof Of Death

Local harness claims capture PID, start timestamp, executable, machine identity
when actual harness ancestor can be identified. Private schema13 claim_processes
table binds record to task, private claim key, exact claim event ID. Save only
fresh claim; retrieving claim cannot transfer or overwrite original process.
Missing process, changed fingerprint (PID reuse), or zombie proves death only on
same machine/PID namespace. Different machine, unsupported platform, failed
process inspection, absent process binding, malformed stored metadata remain
unknown. Manual ownership strings alone never prove process identity.

Herdr claims use saved server and saved identity/terminal rules from orphan reopen.
Successful agent list without owning identity proves absent harness. Failed agent
query may establish dead orchestrator through successful session list showing
saved server absent/stopped. Transport errors, malformed/incomplete session data,
unknown server, mismatched link binding remain unknown. Probe each server once
per scan, including many tasks on same server. Do not use display overrides to
infer ownership. Native harness process death may prove failure even if Herdr
still reports terminal. Process records travel in snapshots; machine guard keeps
restored foreign processes unknown.

## Mutation

Observe without write transaction. For each confirmed dead owner, acquire short
immediate transaction and conditionally update exact in-progress claim, guarded
by claim key and newest claim event ID. Completion/release/reassignment, including
same owner reclaim, wins over stale observation. Set existing status error, clear
claim/assignment fields, update timestamp; retain task contents, revision, images,
tags, relationships, messages and Herdr link. Append one error event authored
qqq-preflight and one reason message. Concurrent/repeated scans produce one pair.
DB failure aborts command; unavailable liveness evidence does not.

Reopen accepts preflight-generated errors as explicit orphan recovery, using
latest event's error action and qqq-preflight author. Ordinary manual error still
uses edit --set-status new. This preserves existing orphan reopen flow after
automatic failure has cleared assignment.

## Verification

RED/GREEN integration tests cover saved-server lookup, dead harness/server before
requested output, live/unknown evidence, repeated/concurrent scans, stale and
same-key reclaims, retained data, orphan reopen, native ancestor binding, PID
reuse and foreign machine. Process parsing tests cover complete snapshots and
malformed evidence. Preserve JSON stdout/stderr. TUI startup must render error
state before interaction. Schema13 gets pinned SQL, immutable DB/snapshot fixture
and updated support inventory. Run focused suites, full serial Rust/PTY suite,
fmt, Clippy, release build, installed smoke and review before integration.
