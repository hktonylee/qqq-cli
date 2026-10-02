# TUI Herdr Handoff Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ctrl-H focuses selected task's live Herdr agent, opens client, returns to preserved TUI.

**Architecture:** Existing DB link and exact Herdr identity resolution supply live pane. Small `tui::handoff` module runs focus/client commands; compose loop drops/recreates terminal guard and clears Ratatui screen.

**Tech Stack:** Rust, Crossterm, Ratatui, SQLite, Herdr CLI, Python PTY tests.

## Task 1: PTY regression

**Files:** `tests/tui.rs`, `tests/tui_dashboard_pty.py`.

- [ ] Add executable fake Herdr fixture for `handoff_*` scenarios. Log argv as JSON lines. Return moved pane from agent list; supply fixture task link with cached old pane. Focus returns JSON envelope. Bare client validates `ICANON`/`ECHO`, stdin/stdout/stderr TTY status, emits terminal marker, exits.
- [ ] Add regression entry:

```rust
#[test]
fn tui_dashboard_ctrl_h_opens_selected_herdr_agent_and_preserves_editor() {
    dashboard_scenario("handoff_success");
    dashboard_scenario("handoff_terminal");
    dashboard_scenario("handoff_json");
    dashboard_scenario("handoff_filter");
}

#[test]
fn tui_dashboard_ctrl_h_errors_preserve_editor() {
    for scenario in ["handoff_no_selection", "handoff_missing", "handoff_stale",
        "handoff_ambiguous", "handoff_focus_error", "handoff_client_error"] {
        dashboard_scenario(scenario);
    }
}
```

- [ ] Scenarios select #2, modify `Second` to `Changed Second`, send legacy Ctrl-H (`b"\x08"`). Assert calls exactly `[["--session", "named", "agent", "list"], ["--session", "named", "agent", "focus", "live:p2"], ["--session", "named"]]`. After return, assert editor title #2, edited text, filter/cursor unchanged; save and assert DB description. Enhanced Ctrl-H (`b"\x1b[104;5u"`) also attaches. `b"\x7f"` deletes text without calling Herdr. Error scenarios assert error footer, unchanged draft and expected partial command list. JSON stdout remains empty.
- [ ] Run `cargo test --locked --test tui tui_dashboard_ctrl_h -- --nocapture`; expect failure because Ctrl-H does nothing.

## Task 2: Handoff implementation

**Files:** create `src/tui/handoff.rs`; modify `src/tui/mod.rs`, `README.md`.

- [ ] Add module with complete focus/client implementation:

```rust
use anyhow::{Context, Result, ensure};
use std::{io, process::Command};

pub(super) fn focus(db: &crate::db::Db, id: i64) -> Result<Option<String>> {
    let link = db.link(id)?.context("Task has no Herdr link; run qqq herdr link")?;
    let pane = crate::herdr::find(&link.identity, link.server.as_deref())?;
    let _: serde_json::Value = crate::herdr::call(
        link.server.as_deref(), &["agent", "focus", &pane.pane_id],
    )?;
    Ok(link.server)
}

pub(super) fn attach(server: Option<&str>) -> Result<()> {
    let mut command = Command::new("herdr");
    if let Some(server) = server {
        crate::db::nonempty(server, "Herdr server")?;
        command.args(["--session", server]);
    }
    let status = command.stdout(io::stderr()).status().context("Cannot open Herdr client")?;
    ensure!(status.success(), "Herdr client failed: {status}");
    Ok(())
}
```

- [ ] Declare `mod handoff`, make terminal guard mutable. After confirmation handling, before action/filter handling, intercept dashboard Control + Char('h'). Missing selection becomes visible error. Successful focus drops guard, runs attach, recreates guard, clears dashboard terminal; result updates footer. Always continue event loop without changing draft/filter/selection.
- [ ] Document Ctrl-H behavior in README TUI section.
- [ ] Run focused regression until green. Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` in private worktree Cargo target; shared targets allow concurrent workers to replace binaries.
- [ ] Review diff, commit `[Feat] Open Task Agent In Herdr From TUI`, rebase current master, fast-forward master. Run merged focused tests, install via `cargo install --path . --locked --offline --force`, remove clean worktree/merged branch, `qqq complete 102 --json`. Resume `qqq next --wait --local --json`.
