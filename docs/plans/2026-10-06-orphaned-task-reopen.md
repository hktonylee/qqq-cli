# Orphaned Herdr Task Reopen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recover in-progress Herdr tasks after owner disappears through CLI and TUI Reopen.

**Architecture:** Store claim association privately in existing link JSON. Verify associated identity/terminal absence under reopen transaction; preserve existing completed-task behavior.

**Tech Stack:** Rust, rusqlite, serde, existing Herdr CLI and Python PTY harness.

### Task 1: CLI behavior and guard coverage

**Files:** Create `tests/reopen_orphan.rs`; existing `tests/reopen.rs` stays regression coverage.

- [ ] Add fake Herdr executable with real qqq subprocesses and response files. Claim using `next --local` in exact pane context. Exercise terminal and reported-session identities.

```rust
let before = fixture.show();
fixture.respond(json!({"result":{"agents":[]}}));
let reopened = fixture.ok(&["reopen", "1", "--session", "reviewer"]);
assert_eq!(reopened["status"], "new");
assert_eq!(reopened["content_revision"], before["task"]["content_revision"]);
assert_eq!(fixture.show()["events"][1]["action"], "reopen");
```

- [ ] Verify red with `cargo test --locked --offline --test reopen_orphan`; expected current `Task 1 must be completed to reopen` rejects recovery.
- [ ] Cover live/moved terminals, changed reporting, same ID under another agent kind, missing/stale/unassociated links, malformed/failed lookup, concurrent recovery, dependency guard, preserved task fields and stale-owner completion rejection after reclaim.

### Task 2: Transactional recovery

**Files:** Modify `src/herdr.rs`, `src/db.rs`, `src/main.rs`.

- [ ] Introduce `herdr::owner_is_live(link: &Link) -> Result<bool>` using typed `agent list` response. Match exact identity or same saved terminal and agent kind; propagate lookup errors.

```rust
pub fn owner_is_live(link: &Link) -> Result<bool> {
    let agents: Agents = call(link.server.as_deref(), &["agent", "list"])?;
    Ok(agents.agents.iter().any(|pane| {
        identity_matches(pane, &link.identity)
            || link.pane.terminal_id.as_deref().is_some_and(|terminal| {
                pane.terminal_id.as_deref() == Some(terminal)
                    && pane.agent.as_deref() == Some(&link.identity.agent)
            })
    }))
}
```

- [ ] Extract existing `find` matching into `identity_matches(pane: &Pane, identity: &AgentSession) -> bool`; keep `find` ambiguity handling unchanged.
- [ ] Save current claim as private `claim_key` property in `herdr_links.link_json`. Continue deserializing public `Link` without that property; all link-writing paths share `Db::save_link`.

```rust
let claim: Option<String> = conn.query_row(
    "SELECT claim_key FROM tasks WHERE id=?", [id], |row| row.get(0),
)?;
let mut encoded = serde_json::to_value(link)?;
encoded["claim_key"] = serde_json::to_value(claim)?;
```

- [ ] In `Db::reopen`, keep completed handling. For in-progress task, load current claim/link under immediate transaction, accept matching recorded claim or legacy automatic tuple, reject absent/stale association and live owner. Existing update/event/content/dependency behavior remains shared.
- [ ] Update CLI help to mention absent Herdr owner. Run focused tests with `cargo test --locked --offline --test reopen_orphan --test reopen --test autodetect --test dispatch --test identity --test json_errors --test dependencies --test tui`; expect zero failures. Commit verified feature.

### Task 3: TUI, docs, verification and integration

**Files:** Modify `tests/tui_dashboard_pty.py`, `tests/tui.rs`, `docs/reference.md`, this plan.

- [ ] Add rendered TUI recovery scenario using fake Herdr and same real claim setup. Select in-progress task, Ctrl-G then `o`, confirm `y`; wait for `Reopened #1` before checking DB and history. Add Rust PTY scenario entry.
- [ ] Extend reference reopen section: absent owner condition, claim association, successful lookup requirement, terminal fallback and unchanged preservation/guards.
- [ ] Run `cargo fmt --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`, full `cargo test --locked --offline`, `scripts/check-compatibility-docs.sh`, `scripts/check-compatibility.sh`, `cargo build --locked --offline --release`, `git diff --check`. Inspect actual results before committing.
- [ ] Use finishing-a-development-branch: rebase, fast-forward master, verify integration, install locked release, smoke-test installed recovery. Record evidence in task #179, complete with current owner, remove worktree/branch, resume one `qqq next --wait --local --json` process.
