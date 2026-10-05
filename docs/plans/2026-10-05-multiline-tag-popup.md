# Multiline Tag Popup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show and edit tags on separate popup lines, preserving Enter apply.

**Architecture:** TUI-specific tag line parser reuses normalization. New
`src/tui/tag_input.rs` builds height-aware input rows and single shortcut footer.
Existing ActionUi value remains String; newline separates input rows.

**Tech Stack:** Rust, Crossterm, Ratatui, Python PTY tests.

## Task 1: Line parsing and popup rows

**Files:** `src/tags.rs`, new `src/tui/tag_input.rs`, `src/tui/mod.rs`.

- [ ] Add parser regression in `src/tags.rs` for newline and CRLF:

```rust
#[test]
fn popup_lines_preserve_order_dedupe_and_comma_compatibility() {
    assert_eq!(super::parse_lines("frontend\n界 面\nfrontend\n").unwrap(),
        vec!["frontend", "界 面"]);
    assert_eq!(super::parse_lines("frontend, bug\r\n\r\nops").unwrap(),
        vec!["frontend", "bug", "ops"]);
}
```

  Additional cases reject tabs, bare CR, escapes and brackets, including blank
  lines containing controls; accept blank/space-only lines and blank clearing.
- [ ] Run `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline
  --bin qqq popup_lines`; confirm missing API, then expected behavioral failure
  with temporary old comma parser delegation.
- [ ] Add `parse_lines(value: &str) -> Result<Vec<String>>`:
  iterate `value.lines()`, skip control-free blank lines, split remaining rows
  by commas, call `normalize`. Retain ordinary `parse` for CLI.
- [ ] Add popup regression via existing `action_lines`: multiple Input rows,
  empty final row retained, footer last, cursor-target input visible for
  heights1..18, errors visible when height allows. Add dedicated `tag_input::rows`
  helper; use heading/guidance when room remains, at most three error rows,
  retain tail of input rows. Footer adapts from full key labels to compact names
  and narrow `^U S-↵ ↵ Esc` aliases.
- [ ] Run `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline
  --bin qqq`; expect zero failures.

## Task 2: Event wiring and terminal behavior

**Files:** `src/tui/mod.rs`, `tests/tui_dashboard_pty.py`, `docs/reference.md`.

- [ ] Prefill `task.tags.join("\n")`; Tags input delegates rows and parser.
  Insert newline only for Shift+Enter Tags input, before ordinary Enter apply:

```rust
KeyCode::Enter if matches!(kind, ActionInputKind::Tags)
    && key.modifiers.contains(KeyModifiers::SHIFT) => {
    value.push('\n');
    error.clear();
    action_ui = Some(ActionUi::Input { id, kind, value, error });
}
```

  Preserve raw paste so non-newline controls remain validation errors.
- [ ] Update existing Tags PTY footer assertions for new shortcut text and
  narrow aliases; remove newline from invalid paste cases. Final valid paste
  uses newline-separated Unicode tags, including duplicate. Reopen verifies
  separate prefilled rows. Shift+Enter adds row without DB write, Enter applies.
  Large pasted list keeps final input and footer visible; cancel preserves DB.
  Exercise empty final row/backspace and legacy comma paste before clear.
- [ ] Run `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline
  --test tui tui_dashboard_tags`; expect four existing tests passing. Reference
  describes one tag per line, Shift+Enter and compact aliases.

## Task 3: Verification and delivery

- [ ] Run fmt/diff checks, full suite, Clippy `--all-targets -- -D warnings`,
  release build with `CARGO_TARGET_DIR=/app/qqq/target`, `--locked --offline`.
  Capture results in `/tmp/qqq-task-169-{full,clippy,release}.log`.
- [ ] Independent read-only review; resolve concrete findings.
- [ ] Commit `[Feat] Edit Popup Tags On Separate Lines`, refresh master,
  rebase, fast-forward merge and install qqq. Run five installed Tags PTY
  scenarios, verify installed/release SHA-256 equality. Record verification
  evidence in plan and queue. Complete #169, clean worktree, resume queue wait.

## Evidence

Baseline: four existing Tags tests passed; no source changes yet.
