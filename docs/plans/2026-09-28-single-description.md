# Single Task Description Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Store one complete task description, preview first line in list, display whole body in show.

**Architecture:** Remove title from schema/model/editor. Inline fields and composition share attachment-aware atomic persistence. Current DB rewritten manually after backup; legacy title schema rejected before writes.

**Tech Stack:** Rust, clap, SQLite/rusqlite, terminal editor, Python for local backup/update.

---

### Task 1: Lock Single-Body Behavior

**Files:** Create `tests/body.rs`.

- [x] Write binary tests for exact multiline description, no title JSON/schema/help, inline/flag equivalence, conflicting forms, preview/full show, blank-only rejection, editor prefill without separators/trimming, legacy title rejection without writes.

```rust
let text = "\nFirst\n\nSecond\n";
let task = ok(p, &["add", text]);
assert_eq!(task["description"], text);
assert!(task.get("title").is_none());
```

- [x] Run `cargo test --locked --target-dir /private/tmp/qqq-task27-target --test body`; expect old title/split behavior failures.

### Task 2: One Description Through Model and Editors

**Files:** Modify `src/sql/schema.sql`, `src/db.rs`, `src/main.rs`, `src/editor.rs`, `src/tui/draft.rs`, `src/tui/mod.rs`, `src/tui/render.rs`, `src/output.rs`, `src/dispatch.rs`.

- [x] Task schema: `description TEXT NOT NULL CHECK(length(trim(description))>0)`; remove title. Task row mapping selects seven remaining columns.
- [x] Reject legacy titled databases before transactions:

```rust
ensure!(!conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name='title')", [], |r| r.get::<_, bool>(0))?,
    "Database contains legacy title column; update SQLite manually before using qqq");
```

- [x] Remove title parameter from add/edit and composition; use `nonempty(description, "Description")` for supplied content. Preserve atomic fields/status/images writes and parent checks. Update dispatch release call.
- [x] Add `text: Option<String>` positional plus `description: Option<String>` flag, mutually exclusive; resolve with `text.or(description)`. Edit retains description/status/images. Compose signatures become `compose(description: &str, external: bool)` and `Draft::new(description: &str)`.
- [x] External editor writes exact body and validates whole returned content; returns Composition `{ description: content, images: Vec::new() }`. TUI finish validates whole expanded text, returns it untrimmed; remove first-line image restriction and split.
- [x] List uses `description.lines().next().unwrap_or("")`; header DESCRIPTION. Human task output prints ID/metadata then full Description block.
- [x] Run focused body tests; adapt existing tests to whole-body API, retain prior acceptance behavior.

### Task 3: Verify and Integrate

**Files:** Update `tests/*.rs`, `tests/tui_pty.py`, `README.md`, current plan.

- [x] Replace old two-field inputs with multiline body, remove title assertions; preserve editor failure/temporary cleanup, ownership/status rollback, historical ownership/parent fixtures, JSON escapes, TUI paste and live wait tests.
- [x] Document inline/editor body semantics and legacy manual update. Run full suite, fmt, Clippy, release and diff checks. Request read-only review, fix verified findings.
- [x] Rebase latest master (including concurrent harness changes); rerun affected checks. Commit verified code, ff merge locally, rebuild root CLI.

### Task 4: Manual SQLite Update

**Files:** Local ignored `qqq.db`; backup and helper under `/private/tmp` only.

- [x] Back up current DB with SQLite backup API. Derive fresh tasks CREATE SQL from latest built binary in temporary project, preserving current ownership/status schema.
- [x] Immediate transaction with FK enforcement disabled for table rebuild; copy IDs/all matching fields and merged old text. Preserve indexes and sequence; all related tables remain untouched. Compare data and run `foreign_key_check`/`integrity_check` before commit.
- [x] Confirm live qqq show/next return whole-body model and retain current task ownership. Record backup path and verification, complete #27, close plan and clean checkout; resume queue wait.


### Verification Record

- Full combined suite: 174 tests passed. Includes error status, mutable parents,
  set-pending shortcut, attachment rollback, whole-body editors, CRLF preservation,
  TUI rendering and legacy-schema initialization race.
- `cargo fmt --check`, Clippy all targets with warnings denied, release build,
  `git diff --check`: passed. Read-only source and SQLite-helper reviews: clear.
- Local master integration: `08d0e6f` plus docs; debug/release CLI rebuilt.
- SQLite helper remained in `/private/tmp`; no new migration or schema-version
  increment. Two copy rehearsals passed, including CRLF and deleted-ID sequence.
- Live backup: `/private/tmp/qqq-before-single-description-20260929T035235479710Z.sqlite`.
  Rebuilt 33 tasks; four related tables, IDs, status/ownership, timestamps, parent
  links, indexes and sequences preserved. FK/integrity checks passed; version 4.
- Live `show 27` and repeated `next --local` returned complete description without
  title, retaining session `codex-qqq-20260928-7f62` and existing claim.
