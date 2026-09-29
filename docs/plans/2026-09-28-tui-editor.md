# Interactive Task Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Default TUI task editor with lossless large-text placeholders and image paste attachments.

**Architecture:** Typed draft atoms hold graphemes, collapsed text and image data. Terminal rendering/clipboard acquisition stay separate from SQLite persistence. Save commits task and images atomically; external editor remains explicit or nonterminal fallback.

**Tech Stack:** Rust, crossterm 0.29, arboard 3.4.1, image 0.25.5, unicode-segmentation 1.12, unicode-width 0.2, rusqlite.

---

### Task 1: Paste Model and Image Input

Files: `src/tui/draft.rs`, `src/images.rs`, `src/tui/mod.rs`, `Cargo.toml`, `Cargo.lock`.

- [ ] Add failing model tests for text expansion, duplicate labels, atomic deletion,
  grapheme navigation, image placeholder removal, title validation and image limits.
  Example contract:
  ```rust
  let mut draft = Draft::new("Title", "");
  let text = "🦀".repeat(1001);
  draft.paste(&text);
  assert!(draft.visible().contains("1001 chars"));
  assert_eq!(draft.finish().unwrap().description, text);
  draft.backspace();
  assert_eq!(draft.finish().unwrap().description, "");
  ```
- [ ] Run `cargo test --locked --bin qqq tui::draft`; observe missing model failures.
- [ ] Add typed `ImageInput { name: String, data: Vec<u8> }`, signature/size validator
  and file reader. Implement `Draft` with atom cursor, paste threshold, insertion,
  navigation, deletion, visible text, expansion and `finish() -> Result<Composition>`.
  `Composition` contains title, description, image inputs. Placeholders use atom
  identity, never string replacement. Preserve original large text.
- [ ] Run model tests; commit `[Feat] Model Collapsed Task Draft Pastes`.

### Task 2: Atomic Draft Persistence

Files: `src/db.rs`, `tests/tui_db.rs`.

- [ ] Add failing tests against real temporary SQLite DB: save draft + PNG, preserve
  metadata on edit, no changes when second image insert trigger aborts.
  SQL failure fixture:
  ```sql
  CREATE TRIGGER reject_image BEFORE INSERT ON images
  WHEN NEW.name='fail.png' BEGIN SELECT RAISE(ABORT,'image rejected'); END;
  ```
- [ ] Run `cargo test --locked --test tui_db`; observe missing save API failures.
- [ ] Implement `Db::save_composition(id: Option<i64>, parent: Option<i64>, draft:
  &Composition) -> Result<Task>` with immediate transaction. Validate all image
  inputs before insert, preserve edit metadata, commit after task and images insert.
- [ ] Run atomic DB tests; commit `[Feat] Save Task Drafts And Images Atomically`.

### Task 3: Terminal Editor and Clipboard

Files: `src/tui/mod.rs`, `src/tui/clipboard.rs`, `src/tui/render.rs`, `tests/tui.rs`.

- [ ] Add PTY tests for default TUI add, bracketed large paste, cancel, blank-title
  validation and JSON-only stdout. Test rendered rows/cursor for narrow terminal,
  wide Unicode, scrolling and control characters.
- [ ] Run targeted tests; verify editor absent or no terminal mode sequences.
- [ ] Implement guarded alternate-screen/raw-mode/bracketed-paste loop on stderr.
  Ctrl-S validates and submits; Esc/Ctrl-C cancel; Enter inserts newline; arrows,
  Home/End, Backspace/Delete operate on atoms. Ctrl-V prefers clipboard image,
  falls back to text; explicit path paste recognizes validated image file. Report
  acquisition errors inside footer and keep draft. Redraw uses bounded viewport,
  safe escaped text and grapheme display widths. Guard restores terminal on errors.
- [ ] Run terminal/model tests; commit `[Feat] Add Terminal Task Composer`.

### Task 4: CLI Entry, Docs and Integration

Files: `src/main.rs`, `src/editor.rs`, `tests/editor.rs`, `tests/tui.rs`, `README.md`.

- [ ] Add CLI tests: `edit --edit` forces external editor even with prefill flags;
  edit TUI saves fields/images but preserves ownership; conflicting status/editor
  flag rejects before opening editor. Add PTY cleanup check after cancel/save.
- [ ] Route terminal interactive calls through TUI; preserve nonterminal external
  fallback. Add `edit -e/--edit` flag conflicting with `--set-status`. Keep inline
  updates direct; call atomic save for TUI compositions. Pin negative ID pre-editor.
- [ ] Document keys, threshold, clipboard/path paste, placeholder removal, atomic
  saves, external fallback and platform/terminal constraints.
- [ ] Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  `cargo test --locked`, `cargo build --locked --release`, `git diff --check`.
- [ ] Request code review. Fix findings, rebase current master, verify integrated
  changes, fast-forward local master, rebuild CLI, record and complete qqq #22.
