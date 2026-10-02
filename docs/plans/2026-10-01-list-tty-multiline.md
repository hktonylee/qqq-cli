# TTY Multiline Task List Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Show full, width-wrapped task descriptions in `qqq list` on a terminal, with aligned tree continuations.

**Architecture:** `output::render` receives optional terminal columns. `task_tree` wraps escaped description lines at grapheme boundaries and builds physical rows with matching first/continuation text columns. CLI and watch mode detect stdout width; redirected and JSON output retain current behavior.

**Tech Stack:** Rust, crossterm, unicode-segmentation, unicode-width, Python PTY integration test.

---

### Task 1: Formatter behavior

**Files:** Modify `src/output.rs`; test `src/output.rs` unit module.

- [x] Add failing renderer test with tasks `Root line\nmore root`, child `First child line\nmore child`, and last child `Last child\nlast detail`. Call `render(Format::Tasks, &tasks, false, Some(40))`; assert root continuation starts with 20 spaces, first child continuation starts with 20 spaces plus `│   `, last child continuation uses spaces. Call with `None`; assert only first description lines appear.
- [x] Add failing wrap/Unicode test at 32 columns: root text `ABCDEFGHIJKLMN` splits after 12 ASCII cells; seven `界` glyphs split after six, without losing glyphs. Include blank internal description line and literal tab/escape controls; assert escaping before wrapping and no raw escape bytes.
- [x] Run `CARGO_TARGET_DIR=/private/tmp/qqq-task49-target cargo test --locked --bin qqq output::tests -- --nocapture`; expect signature/behavior failure.
- [x] Change `render(format, value, color)` to `render(format, value, color, columns: Option<usize>)`. Pass columns only into `task_tree`. Update existing unit calls with `None`.
- [x] Add `wrap_line(text: &str, width: usize) -> Vec<String>`: start one empty row; iterate `UnicodeSegmentation::graphemes(text, true)`; use `UnicodeWidthStr::width(grapheme)`; push a new row before a grapheme that exceeds remaining cells when current row is nonempty; append grapheme unchanged. A single grapheme wider than available width stays intact.
- [x] In `task_tree`, retain current tree traversal. Build `fields = format!("{:<6} {:<12} ", id, status)` and current branch prefix. Build continuation prefix from `" ".repeat(UnicodeWidthStr::width(fields.as_str()))` plus each `continuations` flag mapped to `│   ` or four spaces. If `columns` exceeds first-row prefix display width, iterate `description.lines()`, clean each line, then wrap to remaining width; prepend first prefix to first physical row, continuation prefix to later rows; style each physical row after wrapping. For empty description or unusable width, keep existing one-row preview.
- [x] Run focused formatter tests; expect pass. Commit `[Feat] Render Multiline Task Lists At Terminal Width` after focused tests.

### Task 2: TTY width and watch mode

**Files:** Modify `src/output.rs`, `src/main.rs`, `src/watch.rs`; create `tests/list_tty_pty.py`; modify `tests/output.rs`.

- [x] Add failing PTY test: seed root and two children; set PTY to 40 columns with `TIOCSWINSZ`; invoke human `qqq list` with stdout on slave. Assert multiple description lines, 20-column table padding, `│   ` child continuation, no line exceeding width. Run piped `qqq list` and `qqq --json list`; assert old first-line preview and intact JSON descriptions. Add 32-column Unicode wrap case. Rust test launches script using `env!("CARGO_BIN_EXE_qqq")`.
- [x] Run `CARGO_TARGET_DIR=/private/tmp/qqq-task49-target cargo test --locked --test output human_list_tty`; expect failure.
- [x] Add `output::terminal_columns(tty: bool) -> Option<usize>` using `rustix::termios::tcgetwinsize(stdout)` on Unix, Crossterm on other platforms, only for TTY and ignoring zero columns. In `main::run`, detect stdout TTY separately from color, and pass columns only for human `Format::Tasks`; in `watch::run`, detect stdout TTY and pass refreshed columns on each redraw. Keep JSON path untouched. PTY regression must use separate controlling/stdout terminals to catch mismatched widths.
- [x] Run PTY and all `tests/output.rs` tests; expect pass. Commit `[Feat] Detect Width For TTY Task Lists`.

### Task 3: Verification and integration

**Files:** Modify `README.md` if current list docs need one-line TTY behavior note.

- [ ] Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --quiet`, `git diff --check` with `CARGO_TARGET_DIR=/private/tmp/qqq-task49-target` where applicable; require exit 0.
- [ ] Request read-only code review; fix verified findings and rerun affected checks.
- [ ] Rebase branch onto current master, fast-forward local master, run integrated full suite, remove task worktree, delete merged branch.
- [ ] Complete queue task 49 with `qqq --json complete 49`; verify `qqq --json show 49` reports completed.
