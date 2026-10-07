# Orphaned Herdr Task Reopen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recover in-progress Herdr tasks after owner disappears through CLI and TUI Reopen.

**Architecture:** Store claim association privately in existing link JSON. Verify associated identity/terminal absence under reopen transaction; preserve existing completed-task behavior.

**Tech Stack:** Rust, rusqlite, serde, existing Herdr CLI and Python PTY harness.

### Task 1: CLI behavior and guard coverage

**Files:** Create `tests/reopen_orphan.rs`; existing `tests/reopen.rs` stays regression coverage.

- [x] Add fake Herdr executable with real qqq subprocesses and response files. Claim using `next --local` in exact pane context. Exercise terminal and reported-session identities.

```rust
let before = fixture.show();
fixture.respond(json!({"result":{"agents":[]}}));
let reopened = fixture.ok(&["reopen", "1", "--session", "reviewer"]);
assert_eq!(reopened["status"], "new");
assert_eq!(reopened["content_revision"], before["task"]["content_revision"]);
assert_eq!(fixture.show()["events"][1]["action"], "reopen");
```

- [x] Verify red with `cargo test --locked --offline --test reopen_orphan`; expected current `Task 1 must be completed to reopen` rejects recovery.
- [x] Cover live/moved terminals, changed reporting, same ID under another agent kind, missing/stale links, missing server metadata, legacy associations, malformed/failed lookup, concurrent recovery, dependency guard, preserved task fields and stale-owner completion rejection after reclaim.

### Task 2: Transactional recovery

**Files:** Modify `src/herdr.rs`, `src/db.rs`, `src/main.rs`.

- [x] Introduce `herdr::owner_is_live(link: &Link) -> Result<bool>` using typed `agent list` response. Require saved server. Match exact identity or same saved terminal and agent kind; propagate lookup errors.

```rust
pub fn owner_is_live(link: &Link) -> Result<bool> {
    let server = link.server.as_deref().context(
        Info::new(Code::DispatchError, "Cannot verify owning Herdr agent without saved server")
            .detail("reason", "missing_owner_server"),
    )?;
    let agents: Agents = call(Some(server), &["agent", "list"])?;
    Ok(agents.agents.iter().any(|pane| {
        identity_matches(pane, &link.identity)
            || link.pane.terminal_id.as_deref().is_some_and(|terminal| {
                pane.terminal_id.as_deref() == Some(terminal)
                    && (pane.agent.as_deref() == Some(&link.identity.agent)
                        || pane.agent_session.as_ref()
                            .is_some_and(|session| session.agent == link.identity.agent))
            })
    }))
}
```

- [x] Extract existing `find` matching into `identity_matches(pane: &Pane, identity: &AgentSession) -> bool`; keep `find` ambiguity handling unchanged.
- [x] Save current claim as private `claim_key` property in `herdr_links.link_json`. Continue deserializing public `Link` without that property; all link-writing paths share `Db::save_link`.

```rust
let claim_key: Option<String> = conn.query_row(
    "SELECT claim_key FROM tasks WHERE id=?", [id], |row| row.get(0),
)?;
let stored = StoredLink { link: link.clone(), claim_key };
let encoded = serde_json::to_string(&stored)?;
```

- [x] In `Db::reopen`, keep completed handling. For in-progress task, load current claim/link under immediate transaction, accept matching recorded claim or legacy saved link (fresh claims always replace links), reject absent/stale association and live owner. Existing update/event/content/dependency behavior remains shared.
- [x] Update CLI help to mention absent Herdr owner. Run focused CLI/ownership tests, `cargo test --locked --offline --test reopen_orphan --test reopen --test autodetect`, and TUI recovery scenarios; expect zero failures. Commit verified feature.

### Task 3: TUI, docs, verification and integration

**Files:** Modify `tests/tui_dashboard_pty.py`, `tests/tui.rs`, `docs/reference.md`, this plan.

- [x] Add rendered TUI recovery scenario using fake Herdr and same real claim setup. Select in-progress task, Ctrl-G then `o`, confirm `y`; wait for `Reopened #1` before checking DB and history. Add Rust PTY scenario entry.
- [x] Extend reference reopen section: absent owner condition, claim association, successful lookup requirement, terminal fallback and unchanged preservation/guards.
- [x] Run `cargo fmt --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`, full `cargo test --locked --offline -- --test-threads=1`, `scripts/check-compatibility-docs.sh`, `scripts/check-compatibility.sh`, `cargo build --locked --offline --release`, `git diff --check`. Inspect actual results before committing.
- [x] Use finishing-a-development-branch: rebase, fast-forward master, verify integration, install locked release, smoke-test installed recovery. Record evidence in task #179 and complete with current owner. Worker cleanup and next persistent wait follow completion.

## Verification

- Rebased onto `8dfe93a`, preserving concurrent task #180 tag changes.
- Full serial suite: 705 passed across 41 binaries, zero failures; 106 TUI tests.
- Focused ownership/CLI suite: 97 passed before review fixes; final recovery,
  reopen and autodetection suite: 47 passed, including 12 recovery tests.
- New PTY scenarios cover recovery with color/plain output, cancellation and
  owner returning before confirmation; old installed binary failed recovery
  scenario at completed-only guard.
- Review reproduced missing saved-server and optional agent-kind bugs. Both
  regressions failed before fixes, then passed; independent reviewer verified
  exact named-server recovery and unchanged DB on both rejected cases.
- Fmt, Clippy, compatibility docs (3 tests), compatibility gate (153 tests),
  release build and diff checks passed on rebased code. Logs use
  `/tmp/qqq-task-179-integrated-*.log`.
- Two parallel default suite runs hit PTY timing failures in Ctrl+/ cleanup
  (`KeyboardInterrupt`) and Completed toggle. Ctrl+/ focused rerun passed;
  complete serial run passed. CI was not run.
- Fast-forwarded master to `50b9dea`, verified source/test parity with tested
  worktree and clean checkout. Installed locked/offline release from master.
- Installed CLI verified live-owner rejection, orphan recovery, preservation,
  stale-link replacement and reclaim. Ten installed PTY scenarios passed,
  including three recovery scenarios and existing action/error/retry/tag/filter
  coverage; log: `/tmp/qqq-task-179-installed.log`.
- Installed and root release binaries share SHA256
  `4c9ea8f382ab1da1305109b56cfe177b07fe6e81425a10ad49c7a9a57c26a8d2`.
- Recorded final evidence as qqq message #103; task #179 completed through
  current owner at `2026-10-07T18:23:25.289Z`.
