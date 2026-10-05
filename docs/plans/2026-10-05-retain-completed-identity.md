# Retain Completed Task Identity

> Execute inline with superpowers:executing-plans. Use regressions before changing runtime behavior; obtain independent read-only review before local integration.

**Goal:** Keep recorded harness/orchestrator name and session on completed tasks so logs and saved Herdr association remain inspectable.

**Design:** Public identity metadata differs from active ownership. Normal and force completion clear `claim_key`, set completed status, append existing audit event; leave four identity fields and saved Herdr link intact. Existing JSON/human/TUI readers already show metadata. Active owner queries restrict to in-progress tasks. No schema or JSON field changes. Release/error/reopen retain existing reset semantics; new claim replaces prior identity/link through existing lifecycle.

**Cause:** Both completion UPDATE statements explicitly null four identity columns. Saved Herdr link already survives completion.

## Steps

- [x] Add/adjust identity, automatic-Herdr, DB-force and real-terminal expectations.
  Cover preserved four fields across completion response/show/list/human output,
  exact saved Herdr link, completed-task moved-pane lookup, released claim and
  same public session reused for later tasks without ambiguity. Update legacy
  migration expectations to retain saved public session on completion. Observe red.
- [x] Remove identity clearing only from normal/force completion. Update reference
  wording, verify focused cases, owner guards, force checkbox and reopen/reset.
- [ ] Run full locked/offline suite, compatibility gate, fmt/strict Clippy/release;
  obtain independent review. Commit, rebase/fast-forward locally, install and verify
  installed CLI/force PTY. Record evidence before queue completion and cleanup.

## Evidence

Starting source: master `8b8c633`; prior task174 full serial685 tests/40 binaries,
compatibility152/7 and installed11 force PTY passed with old identity-clearing
behavior.

- Before runtime fix: identity5 passed/6 failed, force DB2 passed/1 failed;
  assertions exposed erased metadata. Evidence: `/tmp/qqq-task-175-identity-red.log`,
  `/tmp/qqq-task-175-force-red.log`.
- Focused identity/autodetect/DB/reopen: 66 tests / 4 binaries passed; force PTY: 4 Rust tests / 11
  scenarios passed (`/tmp/qqq-task-175-focused.log`,
  `/tmp/qqq-task-175-force-pty.log`).
- Full suite runs serially because unchanged Completed-toggle resize/click test
  showed known default-parallel flake during task174; no unrelated test/UI change.
- Independent read-only review: no concrete findings; reviewer passed 58 tests
  across identity/autodetect/DB checks.
- Full locked/offline serial suite: 686 tests / 40 binaries passed
  (`/tmp/qqq-task-175-full.log`). Compatibility gate: 153 tests / 7 binaries passed
  (`/tmp/qqq-task-175-gate.log`). Fmt, diff check, strict all-target Clippy and
  release build passed (`/tmp/qqq-task-175-clippy.log`,
  `/tmp/qqq-task-175-release.log`).
- Local integration and installed verification pending.
