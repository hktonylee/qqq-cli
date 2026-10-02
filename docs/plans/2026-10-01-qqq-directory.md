# Project-local `.qqq` Directory Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store project database at `.qqq/qqq.db`; discover nearest `.qqq` from cwd through parents.

**Architecture:** Keep `Db::open` as path selector and schema opener. `init` creates current-directory storage; other commands find nearest storage directory. `main` derives project root from database grandparent so Herdr cwd matching keeps its contract.

**Tech Stack:** Rust, rusqlite, Cargo integration tests.

---

### Task 1: Prove path behavior

**Files:** Test `tests/cli.rs`; modify `src/db.rs`, `src/main.rs`.

- [ ] Add these integration tests to `tests/cli.rs`:

```rust
#[test]
fn init_creates_hidden_project_database() {
    let d = TempDir::new().unwrap();
    let initialized = ok(d.path(), &["init"]);
    assert_eq!(initialized["database"], d.path().join(".qqq/qqq.db").to_string_lossy().as_ref());
    assert!(d.path().join(".qqq/qqq.db").is_file());
    assert!(!d.path().join("qqq.db").exists());
}

#[test]
fn lookup_uses_nearest_hidden_directory() {
    let d = TempDir::new().unwrap();
    let child = d.path().join("child");
    let grandchild = child.join("grandchild");
    std::fs::create_dir_all(&grandchild).unwrap();
    ok(d.path(), &["init"]);
    ok(d.path(), &["add", "Outer"]);
    assert_eq!(ok(&grandchild, &["list"])[0]["description"], "Outer");
    ok(&child, &["init"]);
    ok(&child, &["add", "Inner"]);
    assert_eq!(ok(&grandchild, &["list"])[0]["description"], "Inner");
    assert_eq!(ok(d.path(), &["list"])[0]["description"], "Outer");
}

#[test]
fn nearest_hidden_directory_without_database_does_not_fall_back() {
    let d = TempDir::new().unwrap();
    let child = d.path().join("child");
    std::fs::create_dir_all(child.join(".qqq")).unwrap();
    ok(d.path(), &["init"]);
    assert!(!run(&child, &["list"]).status.success());
}

#[test]
fn root_level_legacy_database_is_not_discovered() {
    let d = TempDir::new().unwrap();
    std::fs::write(d.path().join("qqq.db"), b"legacy").unwrap();
    assert!(!run(d.path(), &["list"]).status.success());
    assert_eq!(std::fs::read(d.path().join("qqq.db")).unwrap(), b"legacy");
}
```

- [ ] Run `cargo test --locked --test cli init_creates_hidden_project_database`; expect failure because `init` still writes root `qqq.db`.
- [ ] In `src/db.rs`, keep `DB_NAME = "qqq.db"`, add `const PROJECT_DIR_NAME: &str = ".qqq";`, replace `Db::open` path selection with:

```rust
let path = if init {
    let directory = cwd.join(PROJECT_DIR_NAME);
    std::fs::create_dir_all(&directory)?;
    directory.join(DB_NAME)
} else {
    let directory = cwd
        .ancestors()
        .map(|parent| parent.join(PROJECT_DIR_NAME))
        .find(|candidate| candidate.is_dir())
        .context("No .qqq directory found; run qqq init in project root")?;
    let path = directory.join(DB_NAME);
    ensure!(path.is_file(), "No .qqq/qqq.db found in {}", directory.display());
    path
};
```

- [ ] In `src/main.rs`, derive root with `path.parent().and_then(|directory| directory.parent()).context("Database path has no project directory")?` and update CLI help for `.qqq/qqq.db`.
- [ ] Run four new path tests using `cargo test --locked --test cli`; expect new tests pass. Old location assertion still needs Task 2.

### Task 2: Align SQLite fixtures and error assertions

**Files:** Modify `tests/*.rs`.

- [ ] Replace direct test DB paths `.join("qqq.db")` with `.join(".qqq/qqq.db")`; replace Python fake-dispatch `sqlite3.connect('qqq.db')` with `sqlite3.connect('.qqq/qqq.db')`. Preserve explicit legacy-path test from Task 1.
- [ ] Update `init_is_explicit_and_repeatable` to assert `.qqq/qqq.db` after `init`; update missing-project assertions to expect `No .qqq directory found`.
- [ ] Run `cargo test --locked --quiet`; expect all integration tests pass. Inspect any remaining `qqq.db` literals for legitimate docs, legacy behavior, or missing fixture changes.

### Task 3: Document storage and migration

**Files:** Modify `.gitignore`, `README.md`.

- [ ] Add `.qqq/` to `.gitignore`; retain root `qqq.db` ignore entries for legacy databases.
- [ ] Update README storage and discovery text to `.qqq/qqq.db`; describe nearest `.qqq` precedence and project-root Herdr matching.
- [ ] Document migration: stop all writers, create `.qqq`, run `sqlite3 qqq.db ".backup '.qqq/qqq.db'"`, inspect with `qqq list`, retain old DB until verified. Do not move or delete user data in code.
- [ ] Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --quiet`; expect pass.
- [ ] Commit changed code, tests, docs with `[Feat] Store Project Database In Qqq Directory`.

### Task 4: Integrate and close queued task

**Files:** No additional source files.

- [ ] Follow finishing-a-development-branch: rebase isolated branch on current `master`, fast-forward locally, verify tests on integrated tree.
- [ ] Use existing installed `qqq` against original root DB to complete task 44 after integration, before any local DB migration. Verify `qqq show 44 --json` reports `completed`.
