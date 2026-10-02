# Pasteboard Blocks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Save large editor pastes as Markdown `pasteboard` fences and restore colored atomic paste items on task reload.

**Architecture:** Draft tracks new, seeded, and restored paste sources separately. New pastes serialize as collision-safe fenced blocks; source-order parser restores complete fences. Layout carries semantic highlight spans to both Crossterm and Ratatui painters.

**Tech Stack:** Rust, Unicode grapheme layout, Crossterm, Ratatui, Python PTY tests. No new dependency.

---

### Task 1: Draft pasteboard round-trip

**Files:** `src/tui/draft.rs`, `tests/tui_model.rs`, `tests/tui_db.rs`

- [ ] Write failing model tests: `Draft::paste` of 1001 characters yields label `[Pasted Content 1001 chars]` and description containing ` ```pasteboard` fence; `Draft::new` of same text remains raw and clean; `Draft::from_saved` restores complete fence as one paste item. Assert inline before/after text, trailing LF and CRLF payload, embedded triple backticks requiring four-backtick fence, multiple blocks, malformed/wrong-tag fences, atomic delete, and image-span byte offsets after a fence. In DB test, save composition and assert stored description fence and unchanged payload.

```rust
let mut draft = Draft::new("Before ");
draft.paste(&"x".repeat(1001));
draft.insert(" after");
assert_eq!(draft.fragments()[7], "[Pasted Content 1001 chars]");
assert_eq!(draft.finish().unwrap().description,
    format!("Before \n```pasteboard\n{}\n```\n after", "x".repeat(1001)));
```
- [ ] Run `cargo test --locked --test tui_model --test tui_db`; require expected missing-behavior failures.
- [ ] Replace `Atom::Paste` storage with `New`, `SeededPlain`, and `StoredFence(String)` sources. `Draft::new` seeds plain text; `Draft::paste` creates new atom only above existing 1000-character threshold. Generate fence with `max(3, longest_backtick_run + 1)`; insert boundary LF only when neighboring text lacks one. Parse complete, line-start `pasteboard` fences in `Draft::from_saved` before image refs inside their payload. Preserve original fence bytes on re-save. Keep image placeholder spans valid after inserted fence text.

```rust
enum PasteSource { New, SeededPlain, StoredFence(String) }
fn longest_backtick_run(text: &str) -> usize {
    text.split(|ch| ch != '`').map(str::len).max().unwrap_or(0)
}
fn fenced(text: &str) -> String {
    let ticks = "`".repeat(3.max(longest_backtick_run(text) + 1));
    format!("{ticks}pasteboard\n{text}\n{ticks}")
}
```
- [ ] Run `cargo test --locked --test tui_model --test tui_db`; require pass. Commit `[Feat] Store Large Pastes As Pasteboard Blocks`.

### Task 2: Color paste atoms in both editors

**Files:** `src/tui/draft.rs`, `src/tui/render.rs`, `src/tui/dashboard.rs`, `src/tui/mod.rs`, `tests/tui_render.rs`, `tests/tui_dashboard_render.rs`

- [ ] Add failing render tests: image cyan and paste gold occupy distinct foreground spans across wrapped rows in single editor and Ratatui dashboard; adjacent ordinary text uses body color; color-off frames contain no paste/image colors.
- [ ] Run `cargo test --locked --test tui_render --test tui_dashboard_render`; require expected color assertion failures.
- [ ] Add `Draft::paste_mask()` and `Layout::with_paste(fragments, image_mask, paste_mask, width)`, preserving existing `Layout::new` for callers with no paste highlights. Replace image-only spans with ordered `(start,end,kind)` highlight spans; paint image cyan and paste gold in both renderers. Wire live editor to `with_paste`.

```rust
let layout = render::Layout::with_paste(
    &draft.fragments(), &draft.image_mask(), &draft.paste_mask(), width,
);
```
- [ ] Run `cargo test --locked --test tui_render --test tui_dashboard_render`; require pass. Commit `[Feat] Color Pasteboard Items`.

### Task 3: Live editor and storage regression

**Files:** `tests/tui_pty.py`, `tests/tui_history_pty.py`, `tests/tui_dashboard_pty.py`, `tests/tui.rs`, `README.md`

- [ ] Add PTY regression: paste >1000 chars, observe `[Pasted Content 1001 chars]` and gold style, save, verify fenced DB description, reload task through navigation and `qqq edit`, verify atomic item, delete/save and inspect result. Add payload containing a triple-backtick line and a `NO_COLOR`/`TERM=dumb` case. Update old raw-paste expectations.
- [ ] Run `cargo test --locked --test tui`; inspect each new PTY assertion. Fix any failure from feature behavior; passing tests provide live-path coverage for Tasks 1–2.
- [ ] Fix PTY integration or draft edge cases exposed by red tests. Document fence format, reload/edit behavior, and color in `README.md`.
- [ ] Run `cargo test --locked --test tui --test tui_model --test tui_db`; require pass. Commit `[Fix] Restore Pasteboard Blocks In Interactive Editors`.

### Task 4: Verification, review, integration

**Files:** `docs/plans/2026-10-02-pasteboard-blocks.md`

- [ ] Run `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and `git diff --check`; require exit 0.
- [ ] Request read-only code review; fix confirmed Critical/Important findings and rerun affected checks.
- [ ] Rebase onto current local master, fast-forward local master, run integrated full suite, remove owned worktree/branch, install current CLI, complete task 58, verify status, then resume one blocking `qqq --json next --wait --local` call.
