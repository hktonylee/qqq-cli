# Markdown Image References Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Save pasted TUI images as Markdown references to their stored files; restore image atoms when reopening tasks.

**Architecture:** Draft composition records byte spans of newly pasted image atoms. DB save allocates task/image IDs and replaces only those spans within the existing write transaction. Navigation supplies stored image metadata to draft parser so exact Markdown and legacy references reload atomically.

**Tech Stack:** Rust, rusqlite, Crossterm TUI, Python PTY integration tests.

---

### Task 1: Image reference format and draft model

**Files:** `src/images.rs`, `src/tui/draft.rs`, `tests/tui_model.rs`

- [x] Add failing `tests/tui_model.rs` cases: pasted image yields one `image_spans` range over its provisional text; stored `![name](.qqq/images/1/2.png)` reloads as one masked atom and serializes identically; two legacy `[Image: dup.png]` labels map to distinct IDs, remain clean until edited, then serialize as distinct Markdown links; unmatched Markdown/labels remain text; deleting loaded atom removes link.
- [x] Run `cargo test --locked --test tui_model`; confirm new assertions fail from missing spans/parser.
- [x] Add `ImageReference { id, name, media_type }` in `src/images.rs` with `markdown(task_id) -> Result<String>`. Use existing media-type mapping; format `.qqq/images/{task_id}/{id}.{ext}`. Escape `\\`, `[`, `]` in alt text; normalize CR/LF/tab to spaces. Keep original `name` in metadata.
- [x] Add `Composition.image_spans: Vec<Range<usize>>`; new image atoms record byte ranges when serializing provisional `[Image: name]`. Add stored-image atom holding visible label, original token, normalized Markdown token. `Draft::from_saved(description, task_id, &[ImageReference]) -> Result<Draft>` matches exact references, then exact legacy labels in ID order. `is_dirty_against` serializes original tokens; `finish` serializes normalized tokens. Update word motion and image mask for stored atoms.
- [x] Run `cargo test --locked --test tui_model`; require pass. Commit `[Feat] Model Stored Image References`.

### Task 2: Atomic DB reference persistence

**Files:** `src/db.rs`, `src/editor.rs`, `tests/tui_db.rs`, `tests/images.rs`

- [x] Add failing `tests/tui_db.rs` case: `Db::save_composition(None, ...)` with a pasted image returns description `![name](.qqq/images/1/1.png)`, file exists, `Db::image_references(1)` identifies same image; subsequent edit with another pasted image uses next image ID while retaining old Markdown; appended flag image has no auto-link. Add rollback case with image-insert trigger: description and file state remain unchanged.
- [x] Run `cargo test --locked --test tui_db`; confirm expected failures.
- [x] Make `save_images` return inserted IDs. Add `Db::image_references(task_id)` typed metadata query ordered by image ID. Keep public `add`/`edit` wrappers with no spans. Route `save_composition` through internal add/edit variants receiving spans. In each transaction, after image insertion, replace validated spans with `ImageReference::markdown` using corresponding IDs; update `tasks.description` before commit. Reject invalid span count/order/bounds. Keep existing file cleanup guard and transaction semantics.
- [x] Set external editor composition `image_spans` to empty; update any test composition literals. Run `cargo test --locked --test tui_db --test images --test cli`; require pass. Commit `[Feat] Persist Markdown Image Links Atomically`.

### Task 3: Reload via TUI and PTY regression

**Files:** `src/tui/mod.rs`, `tests/tui_pty.py`, `tests/tui_history_pty.py`

- [x] Update PTY save assertion from `[Image: test image.png]` to `![test image.png](.qqq/images/1/1.png)`. Add history scenario: save pasted PNG, navigate back, observe image atom label, remove it with Backspace, save, verify link gone while attachment metadata remains; navigate legacy description with matching metadata and verify atom plus Markdown conversion on save.
- [x] Run `cargo test --locked --test tui`; confirm changed save assertion fails before TUI/DB wiring or new history scenario catches reload failure.
- [x] Extend `Target::Task` to carry preloaded `Draft`. `adjacent_target` obtains `Db::image_references(id)` and constructs `Draft::from_saved`. `load_target` installs that draft. Preserve current confirmation/dirty behavior and cursor reset.
- [x] Run `cargo test --locked --test tui --test tui_model --test tui_db`; require pass. Commit `[Fix] Restore Image Atoms On Task Reload`.

### Task 4: Verification, review, integration

**Files:** `docs/plans/2026-10-01-markdown-image-references.md`; `README.md` only if image behavior docs need correction.

- [ ] Run `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `git diff --check`; require exit 0.
- [ ] Request read-only code review; fix confirmed issues, rerun affected checks.
- [ ] Rebase branch onto current local master, fast-forward local master, run integrated full suite, remove owned worktree and merged branch. Complete task 57 with `qqq --json complete 57`, verify `qqq --json show 57`, then resume one blocking `qqq --json next --wait --local` call.
