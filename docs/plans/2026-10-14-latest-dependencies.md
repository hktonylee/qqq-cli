# Latest Dependencies Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver task #231 with the latest stable Rust dependencies and verified compatibility.

**Architecture:** Update direct constraints and resolve the latest admissible
transitive releases. Preserve the current dependency features and use the existing
tests as the runtime contract. Set and verify the Rust minimum required by the
resolved dependency graph.

**Tech Stack:** Rust, Cargo, bundled SQLite, Ratatui/Crossterm, TOML, SHA-2, image/arboard.

---

### 1. Inventory and baseline

- [x] Read `Cargo.toml`, `Cargo.lock` and installation requirements. Verify every
  direct/target dependency against crates.io's `max_stable_version`. Inventory is
  stored in ignored `target/task231-dependencies.json`.
- [x] Run fresh baseline in the isolated worktree:
  `TMPDIR=/var/tmp/qqq-task230 CARGO_TARGET_DIR=/app/qqq/target cargo test --offline --locked --bin qqq --test config --test tui_clipboard --test tui_dashboard_render --test identity --test filters -- --test-threads=1`.
  Result: 136 passed across six targets.

### 2. Manifest, lockfile and supported Rust

**Modify:** `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/reference.md`.

- [x] Set `rust-version = "1.89"` and update these dependency declarations:
  ```toml
  arboard = { version = "=3.6.1", features = ["wayland-data-control"] }
  image = { version = "=0.25.10", default-features = false, features = ["png"] }
  ratatui = "=0.30.2"
  rusqlite = { version = "0.40", features = ["bundled", "functions"] }
  sha2 = "0.11"
  toml = "1.1"
  toml_edit = "0.25"
  unicode-segmentation = "=1.13.3"
  ```
  Remove the obsolete image/Rust 1.85 compatibility comment.
- [x] Run `cargo update --ignore-rust-version`, then
  `cargo metadata --locked --format-version 1` into ignored
  `target/task231-metadata.json`. Compare direct resolved versions to the inventory
  and determine the maximum `rust_version` among resolved package IDs. If this
  exceeds 1.88, set `rust-version` to that value before compiling.
- [x] Replace `Requires Rust 1.85+` in `README.md` and `docs/reference.md` with the
  actual resolved minimum. Install that exact Rust toolchain using
  `rustup toolchain install 1.89.0 --profile minimal`. Resolved smol_str 0.3.6
  and uuid 1.28.0 require Rust 1.89.
- [x] Run `cargo check --locked --all-targets`. If compiler errors occur, inspect
  their diagnostics with systematic-debugging and extend this plan with the exact
  API repair before changing source. Do not lower dependency versions to bypass
  compilation failures.
- [x] Verify the same all-target compilation on the declared minimum toolchain.

### 2a. Compiler-diagnosed compatibility repairs

Initial all-target compilation failed with E0433 for `DatabaseName`, E0277 for
unsigned SQLite reads and E0277 for SHA-2's new digest array lacking `LowerHex`.

**Modify:** `Cargo.toml`, `src/db.rs`, `src/bulk.rs`,
`src/snapshot/format.rs`, `tests/common/compatibility.rs`.

- [x] Enable rusqlite's `fallible_uint` feature alongside `bundled` and
  `functions`, restoring the unsigned conversions previously supplied by default.
- [x] Replace `rusqlite::DatabaseName::Main` with `rusqlite::MAIN_DB` in
  `Db::peek_next_filtered` to use the new database-name API.
- [x] Convert the digest bytes to exactly two lowercase hexadecimal digits each
  at the three existing encoding sites, using this expression with each site's
  existing digest input:
  ```rust
  Sha256::digest(bytes)
      .iter()
      .map(|byte| format!("{byte:02x}"))
      .collect()
  ```
  The snapshot site uses `hash.finalize()` and the fixture-validation helper
  uses `Sha256::digest(data)`. Existing `hash_reader_reports_size_and_sha256`
  checks the standard known digest; frozen fixture hashes verify exact encoding.
- [x] Rerun all-target compilation, then the focused config/clipboard/render/
  identity/filter tests and snapshot hash regression. Extend the plan with exact
  repairs only if fresh diagnostics reveal further upgrade incompatibilities.

### 2b. Rust-minimum lint compatibility

Strict Clippy now reports `collapsible_if`: the raised Rust minimum enables
let-chain syntax that the old 1.85 minimum excluded from suggestions.

- [x] Run `cargo clippy --fix --allow-dirty --offline --locked --all-targets -- -D warnings`
  to apply its machine-applicable conditional repairs, then `cargo fmt --all`.
- [x] Inspect every changed condition for the same short-circuit order, bindings,
  branch handling and effects. Keep the strict lint configuration intact.
- [x] Rerun strict Clippy on current stable and all-target compilation on Rust
  1.89. The full suite in step 3 verifies the affected runtime behavior.

### 2c. Terminal reset compatibility

The full suite exposed four regressions in conflict recovery and Herdr return.
Ratatui 0.30's `Terminal::clear()` now queries the cursor position before clearing;
the previous release did not. A focused Herdr-error test reproduces the timeout.
Fullscreen `Terminal::resize()` clears the viewport and resets the back buffer
without a cursor query, even when the dimensions stay the same.

**Modify:** `src/tui/mod.rs`.

- [x] Replace the two terminal `clear()` calls with `resize()` using each
  terminal's current backend size. Preserve fullscreen automatic resizing and
  force a full redraw after conflict-popup entry or Herdr return.
- [x] Rerun all four existing failing terminal tests, strict Clippy, minimum
  Rust compilation, the full suite and the release build. Request review of the
  terminal repair; keep the existing PTY fixtures and assertions intact.
  The four failing scenarios and the related Ctrl-H hint test now pass.

### 3. Verify and review

- [x] Run `cargo fmt --all -- --check` and
  `cargo clippy --locked --all-targets -- -D warnings`.
- [x] Run `cargo test --locked -- --test-threads=1` with
  `TMPDIR=/var/tmp/qqq-task231` outside the repository. This covers both
  compatibility-script test selections, clipboard encoding, filters, config,
  ownership and real-terminal interactions. Investigate any failures before
  changing behavior or expectations.
  After the terminal repair, all four original failures passed in the full run.
  One undo/modal test stalled waiting for a discard prompt, then passed four
  isolated runs without changes. Its failed log is retained in ignored
  `target/task231-full-tests-undo-stall.log`. The unchanged rerun passed all
  889 tests across 47 targets, including all 141 terminal tests.
- [x] Run `cargo build --locked --release` and `target/release/qqq --help`.
- [x] Audit direct versions against the current stable inventory, features,
  lockfile consistency and the declared minimum. Request independent review
  using requesting-code-review; fix substantiated findings and rerun affected
  checks. Commit the verified upgrade with a bracketed Title Case message.
  All 20 unique direct dependencies match the latest stable inventory, required
  features are enabled and the highest declared dependency minimum is Rust 1.89.
  Independent review found no substantive issues in the upgrade or terminal
  repair. Verification ran on Linux; macOS CI was not run here.

### 4. Integrate and deliver

- [ ] Use finishing-a-development-branch: rebase onto current local master,
  fast-forward the clean base, verify identical committed tree and run relevant
  integrated checks. Install the verified release binary and compare its hash.
- [ ] Smoke-test the installed command in an isolated temporary project:
  `init`, `add`, `next --local`, `show`, `doctor` and `edit --complete`.
- [ ] Read current task #231 requirements, complete it with its recorded owner,
  and verify completion.

After delivery, remove only this worktree/merged branch and resume exactly one
blocking `qqq next --wait --local --json` process. Block on its output without
repeated model status polling.

All build/test commands use `CARGO_TARGET_DIR=/app/qqq/target`. Temporary files
use `/var/tmp/qqq-task231`; `/tmp` is full and directories under the repository
interfere with ancestor queue-discovery tests. Historical fixtures stay immutable.
