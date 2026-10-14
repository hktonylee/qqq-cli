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

- [ ] Set `rust-version = "1.88"` and update these dependency declarations:
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
- [ ] Run `cargo update --ignore-rust-version`, then
  `cargo metadata --locked --format-version 1` into ignored
  `target/task231-metadata.json`. Compare direct resolved versions to the inventory
  and determine the maximum `rust_version` among resolved package IDs. If this
  exceeds 1.88, set `rust-version` to that value before compiling.
- [ ] Replace `Requires Rust 1.85+` in `README.md` and `docs/reference.md` with the
  actual resolved minimum. Install that exact Rust toolchain using
  `rustup toolchain install <resolved-minimum> --profile minimal`.
- [ ] Run `cargo check --locked --all-targets`. If compiler errors occur, inspect
  their diagnostics with systematic-debugging and extend this plan with the exact
  API repair before changing source. Do not lower dependency versions to bypass
  compilation failures.
- [ ] Verify the same all-target compilation on the declared minimum toolchain.

### 3. Verify and review

- [ ] Run `cargo fmt --all -- --check` and
  `cargo clippy --locked --all-targets -- -D warnings`.
- [ ] Run `cargo test --locked -- --test-threads=1` with
  `TMPDIR=/var/tmp/qqq-task231` outside the repository. This covers both
  compatibility-script test selections, clipboard encoding, filters, config,
  ownership and real-terminal interactions. Investigate any failures before
  changing behavior or expectations.
- [ ] Run `cargo build --locked --release` and `target/release/qqq --help`.
- [ ] Audit direct versions against the current stable inventory, features,
  lockfile consistency and the declared minimum. Request independent review
  using requesting-code-review; fix substantiated findings and rerun affected
  checks. Commit the verified upgrade with a bracketed Title Case message.

### 4. Integrate and deliver

- [ ] Use finishing-a-development-branch: rebase onto current local master,
  fast-forward the clean base, verify identical committed tree and run relevant
  integrated checks. Install the verified release binary and compare its hash.
- [ ] Smoke-test the installed command in an isolated temporary project:
  `init`, `add`, `next --local`, `show`, `doctor` and `edit --complete`.
- [ ] Read current task #231 requirements, complete it with its recorded owner,
  verify completion, remove only this worktree/merged branch, and resume exactly
  one blocking `qqq next --wait --local --json` process. Block on its output
  without repeated model status polling.

All build/test commands use `CARGO_TARGET_DIR=/app/qqq/target`. Temporary files
use `/var/tmp/qqq-task231`; `/tmp` is full and directories under the repository
interfere with ancestor queue-discovery tests. Historical fixtures stay immutable.
