# Herdr Dispatch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Config-driven new-agent dispatch with persistent Herdr association.
**Architecture:** Shared typed config; reserve task via existing atomic next; typed Herdr tab/start/get/prompt adapter; record link before prompt. Retain ambiguous-delivery claims.
**Tech Stack:** Rust, clap, serde/toml, rusqlite, fake Herdr integration tests.

## Steps
- [x] Add `tests/dispatch.rs`: temporary config/project, fake Herdr command log and tab/agent envelopes. Verify next owner differs from caller, saved link, QQQ_SESSION env, unfocused tab/cwd, fixed-task prompt, terminal fallback. Cover no ready tasks, disabled/local, failures before/after prompt, concurrent dispatch, parent dependency.
- [x] Run `cargo test --locked --test dispatch`; expect missing dispatch behavior.
- [x] Extract `src/config.rs` from alias reader. Add `Herdr { #[serde(rename="next-to-new-agent", default)] next_to_new_agent: bool }`; preserve alias tests. Change Next to `Next { #[arg(long)] local: bool }`, dispatch only when `!local && config::load()?.herdr.next_to_new_agent`.
- [x] Implement `src/dispatch.rs`: return caller existing claim first; otherwise generate unique session/name, reserve with `db.next`, validate context, create/start/get, `db.set_link`, prompt. Before prompt errors call `db.edit(id,None,None,Some(owner))`; prompt errors keep ownership/link and give recovery detail.
- [x] Extend `src/herdr.rs`: typed tab/agent envelopes and subprocess methods; optional terminal_id/agent fields on Pane, terminal fallback identity and exact find matching. Spawn uses `tab create --workspace W --cwd C --no-focus --env QQQ_SESSION=S`, `agent start NAME --kind codex --pane P`, `agent get P`, `agent prompt P TEXT`.
- [x] Document config, --local, ownership, terminal fallback, and recovery semantics. Run tests, fmt, clippy, diff checks. Request review.
- [x] Commit implementation; rebase preserved newer assignee, new-status, and --wait changes. Verified 71 tests, fmt, clippy, diff checks; review found no blockers. Installed Herdr schema confirms required tab/root-pane and terminal identity fields.

Integration handoff: merge master, verify merged checkout, complete task 13 via CLI. Live spawning was not exercised.
