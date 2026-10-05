# Colored Task Tags Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Color real tags in dashboard list while preserving layout and plain output.

**Architecture:** Attach optional tag byte range to each existing ListRow from
actual task labels. Match prefix across wrapped rows after dirty metadata.
Render range with indexed color 222, inheriting row background.

**Tech Stack:** Rust, Ratatui, Python PTY tests.

## Task 1: Tag metadata and rendered color

**Files:** `src/tui/panel.rs`, `src/tui/dashboard.rs`,
`tests/tui_dashboard_render.rs`, `tests/tui_filter.rs`.

- [x] Add regression for wrapped real labels, literal bracket description,
  archived prefix, Unicode, dirty marker, selection and NO_COLOR. Test fixtures
  use existing `panel::rows` and `set_dirty_markers`, followed by this metadata:

```rust
panel::set_tag_ranges(&mut rows, &[panel::TagTask {
    id: 1,
    tags: &["界 面".into(), "bug".into()],
    archived: false,
}]);
```

  Assert actual tag cells use `Color::Indexed(222)` when color enabled;
  description retains status foreground and selected row background. Assert
  plain mode cells have no added tag attributes. Assert exact byte ranges only
  cover real tags across continuation rows, never description or ellipsis.

- [x] Run `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline
  --test tui_dashboard_render task_tags --test tui_filter tag_ranges`.
  Missing metadata API initially fails compilation; add minimal API then
  confirm behavioral failure before implementing matcher or color.

- [x] Add optional `tag_range: Option<(usize, usize)>` to `ListRow`, initialized
  to None in `rows`. Store `preview_end: Option<usize>` before appending preview
  ellipsis; matching excludes ellipsis bytes. Introduce metadata input:

```rust
pub struct TagTask<'a> {
    pub id: i64,
    pub tags: &'a [String],
    pub archived: bool,
}
```

  `set_tag_ranges(rows: &mut [ListRow], tasks: &[TagTask<'_>])` builds prefix
  strings from actual labels. Archived prefix precedes highlighted range.
  First row's `description_start` supplies content column; continuation rows
  use same column. For each row, zip remaining expected prefix characters with
  payload; consume exact matching bytes, intersect match with label range.
  Mismatch or end stops matching for that task. Reset at task boundary.
  Empty labels produce no range. Regenerate every frame so tag edits refresh.

- [x] In dashboard row rendering, retain dirty marker prefix, split remaining
  original text by optional tag range. Tagged span uses
  `Style::default().fg(Color::Indexed(222))` only when color enabled. Span adds
  no background or selection modifier. Normal suffix retains inherited style.

- [x] Rerun render/filter tests. Exact command:
  `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline
  --test tui_dashboard_render --test tui_filter`. Expect zero failures.

## Task 2: Live dashboard wiring and terminal verification

**Files:** `src/tui/mod.rs`, `tests/tui_dashboard_pty.py`, `docs/reference.md`.

- [x] After `set_dirty_markers`, feed displayed task metadata:

```rust
let tag_tasks: Vec<_> = displayed.iter().map(|task| panel::TagTask {
    id: task.id,
    tags: &task.tags,
    archived: task.archived,
}).collect();
panel::set_tag_ranges(&mut rows, &tag_tasks);
```

- [x] Strengthen existing Tags PTY color case with rendered tag-color evidence
  after live save; NO_COLOR case must emit no foreground/background SGR.
  Keep validation, resize, dirty draft and filter assertions intact. Add
  reference sentence stating list tags use yellow when color enabled.
- [x] Run `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline
  --test tui tui_dashboard_tags`. Expect four existing tests passing.

## Task 3: Verification and integration

- [x] Run `cargo fmt --all -- --check`, `git diff --check`, full
  `CARGO_TARGET_DIR=/app/qqq/target cargo test --locked --offline`, Clippy
  `--all-targets -- -D warnings`, release build. Record test counts.
- [x] Request independent read-only review; resolve concrete findings.
- [x] Commit `[Feat] Color Tags In Dashboard Task List`; refresh master,
  rebase, fast-forward merge. Install with `cargo install --path . --force
  --locked --offline`. Run five installed Tags PTY scenarios and check installed
  binary hash matches release. Worker then records queue evidence, completes
  #168, cleans owned worktree/branch and resumes persistent queue wait.

## Evidence

Baseline: 42 render/filter tests passed. New API absence first failed compilation;
minimal API then exposed expected color failure: Reset vs Indexed(222). Initial
focused render/filter checks passed 45 tests. Tags PTY checks passed four tests,
including color/NO_COLOR and live save redraw. Final full suite: 638 tests across
39 binaries, zero failures. fmt, Clippy -D warnings and release build passed.
Independent review found no concrete issues; reviewer reran 45 focused tests.

Integrated source: `b3aa6e7`. Installed qqq 0.5.0 passed five Tags PTY
scenarios, including real yellow SGR after live save and NO_COLOR redraw.
Installed/release SHA-256 match:
`2b1fed8a3a9e7e3a0118a256c2f1a1bb95d47eac8e3df25bc99e76de14aba716`.
No remote push or CI run.
