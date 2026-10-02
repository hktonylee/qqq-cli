# Read-Only Project Doctor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Diagnose DB and attachment damage without changing project files.

**Architecture:** `doctor` discovers project independently from normal DB opener, uses read-only SQLite, checks image metadata/files, returns structured report. CLI prints report even for unhealthy state, then exits 1; existing command error behavior stays intact.

**Tech Stack:** Rust 2024, rusqlite, serde_json, filesystem APIs, CLI integration tests.

---

### Task 1: CLI report and read-only DB checks

**Files:** `src/doctor.rs`, `src/main.rs`, `src/output.rs`, `tests/doctor.rs`.

- [x] Write failing CLI tests: healthy project returns `ok:true`, empty issues, exit 0; missing/corrupt DB returns `ok:false`, issue code/action, exit 1, JSON on stdout; run doctor twice and compare DB bytes, mtime, `.qqq` tree. Run `cargo test --locked --test doctor`; expect missing command.
- [x] Define `Diagnostic { code, path, message, action }` and `Report { ok, database, schema_version, tasks, images, issues }` with serde Serialize. Add `Doctor` Clap command and `Format::Doctor` human rendering. Change `run` return to include unhealthy exit flag while preserving existing watch/normal outputs:

```rust
let is_doctor = matches!(&cli.command, Commands::Doctor);
let value = execute(cli, display_limit)?;
let unhealthy = is_doctor && value["ok"] == false;
Ok((Some(rendered), unhealthy))
```

- [x] Special-case doctor before `Db::open`. Discover `.qqq` upward, reject journal/WAL sidecars, open database `SQLITE_OPEN_READ_ONLY`, run integrity/FK/schema checks. Convert each failure into actionable issue, never migrate or create DB. Run focused tests; commit.

### Task 2: Image checks and orphan scan

**Files:** `src/doctor.rs`, `src/images.rs`, `tests/doctor.rs`.

- [x] Write failing tests for missing image, changed byte count, same-size wrong signature, orphan file, unsafe symlink, invalid media type. Each exits 1, produces path/code/action, and leaves files unchanged. Run focused tests and confirm expected failures.
- [x] Extract pure signature detector from `ImageInput::media_type` so doctor can inspect short file header without 20 MiB input limit. Query DB image rows; derive expected paths using `ImageStore::path`, check `symlink_metadata` and file header, scan image tree without following symlinks, report orphan paths and I/O failures. Sort diagnostics for stable output. Example:

```rust
let metadata = std::fs::symlink_metadata(&path)?;
if !metadata.file_type().is_file() {
    issues.push(Diagnostic { code: "IMAGE_UNSAFE".into(), path: path.display().to_string(), message: "Image is not a regular file".into(), action: "Restore image from a verified backup.".into() });
}
if metadata.len() != expected_bytes {
    issues.push(Diagnostic { code: "IMAGE_SIZE".into(), path: path.display().to_string(), message: "Image byte count differs from DB".into(), action: "Restore image from a verified backup.".into() });
}
```

- [x] Add FK damage, schema mismatch, sidecar checks and healthy nested-cwd discovery. Run focused tests; commit.

### Task 3: Docs, review, integration

**Files:** `README.md`, this plan.

- [x] Document `qqq doctor`, JSON/exit codes, read-only scope, backup/restore recovery and stop-writers guidance. Run `qqq doctor --help`, focused tests; commit.
- [x] Run full `cargo test --locked --quiet`, format, Clippy, diff check. Request read-only review, fix Critical/Important findings, rerun affected checks.
- [x] Rebase and fast-forward local master, rerun integrated checks, install CLI, `qqq --json complete 68`, mark plan complete, remove owned worktree/branch, call `qqq --json next --wait --local` once.
