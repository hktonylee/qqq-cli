# Images Through Task Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task inline. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Attach images with add/edit and export with show; remove image subcommand.

**Architecture:** Shared image reader validates bytes before task transaction. Add and edit insert all images inside same transaction as task fields and status. Show exports only images belonging to its task. Reconcile with task #22 Composition persistence before final integration.

**Tech Stack:** Rust, clap, rusqlite, serde_json, tempfile.

---

### Task 1: Specify CLI Contract

**Files:** Create `tests/images.rs`.

- [x] Add regression tests using actual binary and temporary SQLite projects:

```rust
let task = ok(p, &["add", "Task", "--image", "a.png", "--image", "b.jpg"]);
assert_eq!(task["id"], 1);
let detail = ok(p, &["show", "1", "--export-image", "1", "--output", "out.png"]);
assert_eq!(detail["images"].as_array().unwrap().len(), 2);
assert_eq!(std::fs::read(p.join("out.png")).unwrap(), PNG);
```

- [x] Test image-only negative edit, unchanged fields, task-scoped export, paired flags and absent standalone command. Snapshot show before invalid image / unauthorized release / trigger failures and assert equality after failure.
- [x] Run `cargo test --locked --target-dir /private/tmp/qqq-task23-target --test images`; expect missing `--image` / `--export-image` failures.

### Task 2: Atomic Attachment Persistence

**Files:** Create `src/images.rs`; modify `src/db.rs`, `src/main.rs`, `src/dispatch.rs`.

- [x] Define shared reader contract:

```rust
#[derive(Clone, Debug)]
pub struct ImageInput { pub name: String, pub data: Vec<u8> }
```

`ImageInput::read(&Path)` opens regular file, caps read at `20 * 1024 * 1024 + 1`, obtains filename, validates signature through `media_type() -> Result<&'static str>`.

- [x] Extend add/edit signatures with `images: &[ImageInput]`; add becomes mutable. Validate every media type, then start immediate transaction. Execute existing task SQL and insert every attachment before querying saved task and committing:

```rust
for image in images {
    tx.execute("INSERT INTO images(task_id,name,media_type,data) VALUES (?,?,?,?)",
        params![id, image.name, image.media_type()?, image.data])?;
}
```

- [x] Remove file-reading image_add. Extend image_export query to `SELECT data FROM images WHERE id=? AND task_id=?`; task lookup precedes export. Keep create_new.
- [x] Pass empty image slice from dispatch rollback edit.
- [x] Add repeatable PathBuf vectors to Add/Edit:

```rust
#[arg(long = "image", value_name = "PATH")]
images: Vec<PathBuf>,
```

Read selected paths through `ImageInput::read`; image-only edit skips editor. Pass inputs into task transaction.
- [x] Run focused tests to verify attachment and rollback contract.

### Task 3: Show Export and Remove Legacy Interface

**Files:** Modify `src/main.rs`, `src/output.rs`, `tests/cli.rs`, `tests/output.rs`, `tests/aliases.rs`, `README.md`.

- [x] Extend Show with paired typed arguments:

```rust
#[arg(long, requires = "output")]
export_image: Option<i64>,
#[arg(long, requires = "export_image", value_name = "PATH")]
output: Option<PathBuf>,
```

Build normal detail; when supplied, set `detail["export"] = db.image_export(id, image, &path)?`. Detail renderer appends `Exported image #ID to PATH` when export exists.
- [x] Remove Commands::Image, ImageCommand and standalone output formats. Replace old CLI image test calls with edit --image and show export. Use config alias as nested command coverage; attachment alias `img = 'edit 1 --image'` preserves non-UTF8 path test.
- [x] Document repeated paths, atomic failure, image-only edit and scoped export; remove README standalone examples.
- [x] Run full tests, fmt and Clippy; commit verified CLI change.

### Task 4: Integrate TUI and Finish

**Files:** Reconcile task #22 changes in `src/main.rs`, `src/db.rs`, `src/images.rs`, `tests/tui_db.rs`, `README.md`.

- [x] Rebase onto completed task #22. Keep shared ImageInput validator. Composition persistence uses:

```rust
match id {
    Some(id) => self.edit(id, Some(&draft.title), Some(&draft.description), None, &draft.images),
    None => self.add(&draft.title, &draft.description, parent, &draft.images),
}
```

Update direct database test calls with empty attachment slices. Extend successful add/edit composition images with flagged inputs before atomic save; cancellation returns without save.
- [x] Run full suite, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo build --locked --release`, `git diff --check` in isolated target. Request read-only code review; fix verified findings.
- [x] Rebase latest master, verify affected combined behavior, ff merge locally and rebuild root binary.

After integration, log task evidence and complete #23 with qqq CLI. Remove isolated checkout and branch; resume next --local --wait with same session.


## Verification

Integrated implementation `9eede07` into local master. Fresh isolated suite: 141
tests passed, including eight attachment regressions and real PTY pasted plus
flagged image save/cancel. Formatting, Clippy with denied warnings, release build
and diff checks passed. Final read-only review found no actionable issues; its
15 focused attachment/TUI/persistence tests passed. Root debug and release CLI
rebuilt after integration. Database schema unchanged.
