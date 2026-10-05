# Automatic Session Handoff Audit

> Execute inline with superpowers:executing-plans; regress behavior before prompt change, obtain independent read-only review.

**Task176:** Verify `qqq next` reads harness/orchestrator context automatically and saves metadata; downstream worker should not need session flags.

**Findings:** Explicit `--session` / inherited `QQQ_SESSION` takes precedence. Exact Herdr context reads pane and reports any harness identity, with terminal fallback; Codex display uses CODEX_SESSION_ID. Outside Herdr, CODEX_THREAD_ID then CODEX_SESSION_ID supplies native owner/display. Unique project-root Herdr pane is last fallback. No standalone env adapter exists for unspecified other harnesses. Four fields/link save during claim, retained on completion since task175. Dispatch creates synthetic stable claim key and supplies it through child tab's QQQ_SESSION, then saves child harness/link before prompting. Prompt nevertheless repeats --session arguments: redundant parameter dependency in instructions.

**Design:** Keep inherited dispatch owner token/precedence and saved child metadata. Change dispatched prompt to use inherited QQQ_SESSION for message/complete/error commands without session parameters; no provider-variable guesses or ownership transfer. Audit Herdr generic-harness id/terminal contexts and native Codex using existing tests plus installed CLI matrix. Optional clarification requested for standalone non-Codex harness names; continue with verified existing support.

## Steps

- [x] Add dispatch regression requiring flag-free prompt and env-only downstream
  message/completion commands, including unavailable Herdr and conflicting native
  Codex session. Expand other-Herdr-harness identity/terminal claim/completion
  assertions. Observe prompt regression red; context audit green.
- [x] Remove redundant session flags from worker prompt, preserve inherited
  QQQ_SESSION contract, child metadata and
  caller/child separation. Update ownership and dispatch docs with concise matrix.
- [x] Verify focused/full checks, compatibility, fmt/strict Clippy/release; obtain
  independent review, commit/rebase/fast-forward locally, install and verify
  env-only context matrix. Record evidence, queue-complete, cleanup, resume waiter.

## Evidence

Base master 83a64d1, installed qqq 0.5.0. Current queue tasks174–176 claimed through
`qqq next --wait --local --json` without session parameter. Memory warning about
older design-only native support verified against current src/session.rs and
current autodetect tests.

- Before prompt fix: dispatch regression failed on absent inherited-env contract;
  context audit: 27 tests passed, including 6 other-harness/identity combinations.
  Evidence: `/tmp/qqq-task-176-red.log`, `/tmp/qqq-task-176-context-audit.log`.
- Fresh focused dispatch/autodetect/identity: 55 tests/3 binaries passed, including
  env-only retrieval/message/completion/error with unavailable Herdr and native
  session conflict (`/tmp/qqq-task-176-focused-final.log`).
- Standalone non-Codex provider clarification remains optional/pending; current
  source only claims supported detection, no guessed provider environment vars.
- Independent read-only review clear; dispatch handoff and 6 generic Herdr
  harness contexts passed independently.
- Full locked/offline serial suite: 686 tests / 40 binaries passed
  (`/tmp/qqq-task-176-full.log`); compatibility 153/7 passed
  (`/tmp/qqq-task-176-gate.log`); fmt, strict Clippy all-targets and release passed
  (`/tmp/qqq-task-176-clippy.log`, `/tmp/qqq-task-176-release.log`). Serial full
  run uses previously documented unchanged Completed-toggle test flake workaround.
- Fix `e31910e` fast-forwarded locally; clean package rebuild passed focused 55/3
  and compatibility 153/7, release build and install passed. Evidence:
  `/tmp/qqq-task-176-integrated-focused.log`,
  `/tmp/qqq-task-176-integrated-gate.log`, `/tmp/qqq-task-176-install.log`.
- Installed qqq 0.5.0 passed 15 temporary-project automatic-context cases:
  native Codex thread/session/thread+display; inherited owner overriding native
  context; 4 Herdr harness names x id/terminal; unique root agent; env-only
  dispatch retrieval/message/completion/error. Matrix uses mock Herdr CLI; current
  real queue claims also verified exact live Herdr context without session flags.
  No live agent spawning performed. Evidence:
  `/tmp/qqq-task-176-installed-context.log`.
- Installed and fresh main release SHA256 match:
  `a0c0d116e6af57b44a7c433f732dfb5e2ffc72299f93232957396735947ec9d3`.
- No remote CI, push, tag or publication performed.

After verification record integrates: queue-complete task, retain metadata,
clean owned worktree/branch, resume one silent waiter.
