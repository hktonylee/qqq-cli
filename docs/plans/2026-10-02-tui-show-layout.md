# TUI Show Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Match TUI task-detail titles and layout to `qqq show`.

**Architecture:** Reuse existing human formatter on full local `Db::show` snapshot. Convert its trusted SGR output into typed spans before safe wrapping. Existing TUI geometry, refresh and independent scroll remain.

**Tech Stack:** Rust, rusqlite, serde_json, Ratatui, Python PTY fixtures.

## Task 1: Share Show Formatting

**Files:** `src/output.rs`, `src/tui/details.rs`, `src/tui/render.rs`, `src/tui/dashboard.rs`.

- [ ] Change formatter regression to expect `#4 · In progress` header, Description/Details/Assignment/Messages/Images/History/Herdr titles and show field alignment. Run `cargo test --offline --bin qqq tui::details::tests` and observe old compact header failure.
- [ ] Expose existing formatter without changing CLI output:

```rust
pub(crate) fn detail_text(value: &Value, color: bool) -> String {
    detail::render(value, color)
}
```

- [ ] Replace task/message-only formatter with shared output adapter, retaining `wrap` and unavailable notice:

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

- [ ] Add `Section`, `Success` typed roles; Section uses Cyan+BOLD, Success Green. Keep ID bold, statuses and dim spans from #119. Avoid empty spans in `DetailRow::push`:

```rust
if text.is_empty() {
    return;
}
```

- [ ] Convert formatter fixtures to full show JSON payload; assert unwrapped plaintext equals `detail_text(value, false)`, section titles and value column 25. Check narrow Unicode/control and typed span roles. Run formatter + render tests + `tests/show.rs`; commit verified formatter checkpoint.

## Task 2: Connect Complete Detail Snapshot

**Files:** `src/tui/mod.rs`, `tests/tui_dashboard_pty.py`, `README.md`.

- [ ] Replace selected-task formatting call with full stored snapshot:

```rust
Some(_) => details::rows(&db.show(id)?, details_width),
```

- [ ] Update details/details_no_color PTY checks: initial `#2 · New`, scroll to Description/Details/Assignment/Messages, field column alignment, separate Created/Updated rows, ascending message IDs, message indentation. Preserve editor cursor, draft save, new-task selection and stale-content checks.
- [ ] Update details_scroll to locate message section before testing PageDown/wheel increments; selection reset returns to `#ID · Status`. Update refresh to scroll to messages before externally appending; assert show header status plus latest message appears after preceding messages. Keep deleted selected-task behavior and plain mode checks.
- [ ] README describes show sections, field alignment, message order and empty collections; removes compact/latest-first/combined-timestamp text. Run `cargo test --offline --test tui`, focused show/formatter/render checks; commit verified integration checkpoint.

## Task 3: Verify And Integrate

- [ ] `cargo fmt --all --check`; `git diff --check`.
- [ ] `cargo test --offline --all-targets`; `cargo clippy --offline --all-targets -- -D warnings`.
- [ ] Request read-only review through requesting-code-review skill; resolve findings.
- [ ] Rebase onto current master; fast-forward master; verify any changed combined source.
- [ ] Build/install release; run details, details_no_color, details_scroll, details_refresh, details_deleted, layout_new, dumb, color PTY scenarios.
- [ ] Record evidence, complete #120, remove owned branch/worktree; return to blocking queue wait.
