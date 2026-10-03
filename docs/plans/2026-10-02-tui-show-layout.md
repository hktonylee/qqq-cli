# TUI Show Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Match TUI task-detail titles and layout to `qqq show`.

**Architecture:** Reuse existing human formatter on full local `Db::show` snapshot. Convert its trusted SGR output into typed spans before safe wrapping. Existing TUI geometry, refresh and independent scroll remain.

**Tech Stack:** Rust, rusqlite, serde_json, Ratatui, Python PTY fixtures.

## Task 1: Share Show Formatting

**Files:** `src/output.rs`, `src/tui/details.rs`, `src/tui/render.rs`, `src/tui/dashboard.rs`.

- [x] Change formatter regression to expect `#4 · In progress` header, Description/Details/Assignment/Messages/Images/History/Herdr titles and show field alignment. Run `cargo test --offline --bin qqq tui::details::tests` and observe old compact header failure.
- [x] Expose existing formatter without changing CLI output:

```rust
pub(crate) fn detail_text(value: &Value, color: bool) -> String {
    detail::render(value, color)
}
```

- [x] Replace task/message-only formatter with shared output adapter, retaining `wrap` and unavailable notice:

```rust
pub fn rows(value: &serde_json::Value, width: usize) -> Vec<DetailRow> {
    let content = crate::output::detail_text(value, true)
        .split('\n')
        .map(styled_line)
        .collect();
    wrap(content, width)
}

fn styled_line(line: &str) -> DetailRow {
    let mut parts = Vec::new();
    let mut text = line;
    let mut kind = DetailKind::Body;
    while let Some((before, sequence)) = text.split_once("\x1b[") {
        parts.push((before.to_owned(), kind));
        let Some((code, rest)) = sequence.split_once('m') else {
            parts.push((format!("\\u{{1b}}[{sequence}"), DetailKind::Body));
            return DetailRow::styled(parts);
        };
        kind = match code {
            "1" => DetailKind::Heading,
            "1;36" => DetailKind::Section,
            "2" => DetailKind::Muted,
            "36" => DetailKind::InProgress,
            "90" => DetailKind::Completed,
            "31" => DetailKind::Error,
            "33" => DetailKind::Warning,
            "32" => DetailKind::Success,
            _ => DetailKind::Body,
        };
        text = rest;
    }
    parts.push((text.to_owned(), kind));
    DetailRow::styled(parts)
}
```

- [x] Add `Section`, `Success` typed roles; Section uses Cyan+BOLD, Success Green. Keep ID bold, statuses and dim spans from #119. Avoid empty spans in `DetailRow::push`:

```rust
if text.is_empty() {
    return;
}
```

- [x] Convert formatter fixtures to full show JSON payload; assert unwrapped plaintext equals `detail_text(value, false)`, section titles and value column 25. Check narrow Unicode/control and typed span roles. Run formatter + render tests + `tests/show.rs`; commit verified formatter checkpoint.

## Task 2: Connect Complete Detail Snapshot

**Files:** `src/tui/mod.rs`, `tests/tui_dashboard_pty.py`, `README.md`.

- [x] Replace selected-task formatting call with full stored snapshot:

```rust
Some(task) => details::rows(&db.show_task(task)?, details_width),
```

- [x] Update details/details_no_color PTY checks: initial `#2 · New`, scroll to Description/Details/Assignment/Messages, field column alignment, separate Created/Updated rows, ascending message IDs, message indentation. Preserve editor cursor, draft save, new-task selection and stale-content checks.
- [x] Update details_scroll to locate message section before testing PageDown/wheel increments; selection reset returns to `#ID · Status`. Update refresh to scroll to messages before externally appending; assert show header status plus latest message appears after preceding messages. Keep deleted selected-task behavior and plain mode checks.
- [x] README describes show sections, field alignment, message order and empty collections; removes compact/latest-first/combined-timestamp text. Run `cargo test --offline --test tui`, focused show/formatter/render checks; commit verified integration checkpoint.

## Task 3: Verify And Integrate

- [x] Review found deletion between list and second task lookup could exit TUI.
  Factor existing `Db::show` section reads into `Db::show_task(&Task)`; checked
  `show(id)` delegates after task lookup, TUI passes listed Task. Extend existing
  `tests/tui_db.rs` regression: compare payloads, capture listed Task/version,
  delete through separate DB connection, assert checked show fails, captured-task
  detail succeeds with empty collections, version changes and next list empty.
  Focused DB/show/refresh-deletion checks pass.

- [x] `cargo fmt --all --check`; `git diff --check`.
- [x] `cargo test --offline --all-targets`; `cargo clippy --offline --all-targets -- -D warnings`.
- [x] Request read-only review through requesting-code-review skill; resolve findings.
- [x] Rebase onto current master; fast-forward master; verify any changed combined source.
- [x] Build/install release; run details, details_no_color, details_scroll, details_refresh, details_deleted, layout_new, dumb, color PTY scenarios.
After verification, record task evidence, complete #120, remove owned branch/worktree and return to blocking queue wait.

Final combined source checks after race fix and rebase: 473 tests across 37 binaries; fmt, diff check and clippy passed. Installed release passed 9 PTY scenarios, including handoff scroll. Review found no remaining issues. Source commit: 174af99; master matched tested source.
