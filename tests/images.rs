use rusqlite::Connection;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfixture";
const JPEG: &[u8] = b"\xff\xd8\xfffixture";

fn command(dir: &Path) -> Command {
    command_mode(dir, true)
}
fn command_mode(dir: &Path, json: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    if json {
        command.arg("--json");
    }
    command
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("EDITOR")
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    command
}
fn run(dir: &Path, args: &[&str]) -> Output {
    command(dir).args(args).output().unwrap()
}
fn ok(dir: &Path, args: &[&str]) -> Value {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn error(dir: &Path, args: &[&str], code: i32, text: &str) {
    // Verify human diagnostic context; JSON envelopes have dedicated contract coverage.
    let out = command_mode(dir, false).args(args).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(code),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(text),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    fs::write(dir.path().join("a.png"), PNG).unwrap();
    fs::write(dir.path().join("b.jpg"), JPEG).unwrap();
    dir
}

fn legacy_v5_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".qqq")).unwrap();
    let conn = Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
    for migration in [
        include_str!("../src/sql/schema.sql"),
        include_str!("../src/sql/migrate_v2.sql"),
        include_str!("../src/sql/migrate_v3.sql"),
        include_str!("../src/sql/migrate_v4.sql"),
        include_str!("../src/sql/migrate_v5.sql"),
    ] {
        conn.execute_batch(migration).unwrap();
    }
    conn.execute("INSERT INTO tasks(description) VALUES ('Legacy')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO images(id,task_id,name,media_type,data) VALUES (3,1,'old.png','image/png',?1)",
        [PNG],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO images(id,task_id,name,media_type,data) VALUES (7,1,'old.jpg','image/jpeg',?1)",
        [JPEG],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO images(id,task_id,name,media_type,data) VALUES (20,1,'deleted.gif','image/gif',X'474946383961')",
        [],
    )
    .unwrap();
    conn.execute("DELETE FROM images WHERE id=20", []).unwrap();
    dir
}

#[test]
fn v5_migration_copies_images_to_files_and_preserves_ids_and_sequence() {
    let dir = legacy_v5_project();
    let p = dir.path();
    let first = p.join(".qqq/images/1/3.png");
    fs::create_dir_all(first.parent().unwrap()).unwrap();
    fs::write(&first, PNG).unwrap(); // Matching file left by interrupted migration.
    let detail = ok(p, &["show", "1"]);
    assert_eq!(detail["images"][0]["id"], 3);
    assert_eq!(detail["images"][0]["bytes"], PNG.len());
    assert_eq!(detail["images"][1]["id"], 7);
    assert_eq!(fs::read(first).unwrap(), PNG);
    assert_eq!(fs::read(p.join(".qqq/images/1/7.jpg")).unwrap(), JPEG);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        11
    );
    let has_data: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('images') WHERE name='data')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!has_data);
    conn.execute(
        "INSERT INTO images(task_id,name,media_type,bytes) VALUES (1,'new.gif','image/gif',3)",
        [],
    )
    .unwrap();
    assert!(conn.last_insert_rowid() > 20);
    assert_eq!(ok(p, &["show", "1"])["images"].as_array().unwrap().len(), 3);
}

#[test]
fn v5_migration_conflict_keeps_blob_rows_and_succeeds_after_retry() {
    let dir = legacy_v5_project();
    let p = dir.path();
    let conflict = p.join(".qqq/images/1/7.jpg");
    fs::create_dir_all(conflict.parent().unwrap()).unwrap();
    fs::write(&conflict, b"different").unwrap();
    error(p, &["show", "1"], 1, "Stored image path conflicts");
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    let old: Vec<u8> = conn
        .query_row("SELECT data FROM images WHERE id=7", [], |r| r.get(0))
        .unwrap();
    assert_eq!(old, JPEG);
    assert!(!p.join(".qqq/images/1/3.png").exists());
    assert_eq!(fs::read(&conflict).unwrap(), b"different");
    fs::remove_file(conflict).unwrap();
    ok(p, &["show", "1"]);
    assert_eq!(fs::read(p.join(".qqq/images/1/7.jpg")).unwrap(), JPEG);
}

#[test]
fn concurrent_v5_openers_migrate_files_once() {
    let dir = legacy_v5_project();
    let children: Vec<_> = (0..6)
        .map(|_| {
            command(dir.path())
                .args(["show", "1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let detail: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(detail["images"].as_array().unwrap().len(), 2);
    }
    assert_eq!(
        fs::read(dir.path().join(".qqq/images/1/3.png")).unwrap(),
        PNG
    );
    assert_eq!(
        fs::read(dir.path().join(".qqq/images/1/7.jpg")).unwrap(),
        JPEG
    );
}

#[test]
fn failed_v6_foreign_key_check_keeps_blobs_and_removes_new_files() {
    let dir = legacy_v5_project();
    let p = dir.path();
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    conn.execute(
        "INSERT INTO images(id,task_id,name,media_type,data) VALUES (30,999,'orphan.png','image/png',?1)",
        [PNG],
    )
    .unwrap();
    error(p, &["show", "1"], 1, "invalid foreign key references");
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        conn.query_row("SELECT data FROM images WHERE id=30", [], |r| r
            .get::<_, Vec<u8>>(0))
            .unwrap(),
        PNG
    );
    assert!(!p.join(".qqq/images/1/3.png").exists());
    assert!(!p.join(".qqq/images/999/30.png").exists());
}

#[test]
fn add_copies_repeated_images_and_show_exports_scoped_bytes() {
    let dir = project();
    let p = dir.path();
    let task = ok(
        p,
        &[
            "add",
            "Task\n\nDetails",
            "--image",
            "a.png",
            "--image",
            "b.jpg",
        ],
    );
    assert_eq!(task["id"], 1);
    fs::remove_file(p.join("a.png")).unwrap();
    fs::remove_file(p.join("b.jpg")).unwrap();
    let detail = ok(p, &["show", "1"]);
    assert!(detail.get("export").is_none());
    assert_eq!(detail["images"].as_array().unwrap().len(), 2);
    assert_eq!(detail["images"][0]["name"], "a.png");
    assert_eq!(detail["images"][1]["media_type"], "image/jpeg");
    assert_eq!(fs::read(p.join(".qqq/images/1/1.png")).unwrap(), PNG);
    assert_eq!(fs::read(p.join(".qqq/images/1/2.jpg")).unwrap(), JPEG);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    let has_data: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('images') WHERE name='data')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!has_data);
    let exported = ok(
        p,
        &["show", "1", "--export-image", "2", "--output", "out.jpg"],
    );
    assert_eq!(exported["task"], detail["task"]);
    assert_eq!(exported["export"]["id"], 2);
    assert_eq!(exported["export"]["bytes"], JPEG.len());
    assert_eq!(fs::read(p.join("out.jpg")).unwrap(), JPEG);
    error(
        p,
        &["show", "1", "--export-image", "2", "--output", "out.jpg"],
        1,
        "destination must not exist",
    );
    assert_eq!(fs::read(p.join("out.jpg")).unwrap(), JPEG);
    fs::remove_file(p.join(".qqq/images/1/2.jpg")).unwrap();
    assert_eq!(ok(p, &["show", "1"])["images"], detail["images"]);
    error(
        p,
        &[
            "show",
            "1",
            "--export-image",
            "2",
            "--output",
            "missing-out.jpg",
        ],
        1,
        "Cannot read stored image",
    );
    assert!(!p.join("missing-out.jpg").exists());
    ok(p, &["add", "Other"]);
    error(
        p,
        &["show", "2", "--export-image", "1", "--output", "wrong.png"],
        1,
        "Image not found",
    );
    assert!(!p.join("wrong.png").exists());
    error(
        p,
        &[
            "show",
            "99",
            "--export-image",
            "1",
            "--output",
            "missing.png",
        ],
        1,
        "Task 99 not found",
    );
    assert!(!p.join("missing.png").exists());
}

#[test]
fn image_only_negative_edit_preserves_fields_owner_and_existing_images() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["next", "--session", "parent"]);
    ok(p, &["complete", "1", "--session", "parent"]);
    ok(
        p,
        &[
            "add",
            "Child\n\nOriginal",
            "--parent",
            "1",
            "--image",
            "a.png",
        ],
    );
    ok(p, &["next", "--session", "owner"]);
    ok(p, &["message", "2", "Note"]);
    let before = ok(p, &["show", "2"]);
    let edited = ok(p, &["edit", "-1", "--image", "b.jpg", "--image", "a.png"]);
    for key in [
        "id",
        "description",
        "status",
        "harness_session",
        "parent_id",
        "created_at",
    ] {
        assert_eq!(edited[key], before["task"][key]);
    }
    let after = ok(p, &["show", "2"]);
    assert_eq!(after["messages"], before["messages"]);
    assert_eq!(after["events"], before["events"]);
    assert_eq!(after["images"].as_array().unwrap().len(), 3);
    assert_eq!(after["images"][0], before["images"][0]);
}

#[test]
fn invalid_attachments_leave_creation_fields_status_and_events_unchanged() {
    let dir = project();
    let p = dir.path();
    fs::write(p.join("bad.png"), b"not an image").unwrap();
    error(
        p,
        &["add", "Invalid", "--image", "a.png", "--image", "bad.png"],
        1,
        "Unsupported image signature",
    );
    assert_eq!(ok(p, &["list"]), serde_json::json!([]));
    ok(p, &["add", "Original\n\nDetails", "--image", "a.png"]);
    ok(p, &["next", "--session", "owner"]);
    let before = ok(p, &["show", "1"]);
    error(
        p,
        &[
            "edit",
            "1",
            "--description",
            "Changed\n\nChanged",
            "--set-status",
            "new",
            "--session",
            "owner",
            "--image",
            "b.jpg",
            "--image",
            "bad.png",
        ],
        1,
        "Unsupported image signature",
    );
    assert_eq!(ok(p, &["show", "1"]), before);
    error(
        p,
        &[
            "edit",
            "1",
            "--description",
            "Changed",
            "--set-status",
            "new",
            "--session",
            "wrong",
            "--image",
            "b.jpg",
        ],
        1,
        "not claimed by session wrong",
    );
    assert_eq!(ok(p, &["show", "1"]), before);
    ok(
        p,
        &[
            "edit",
            "1",
            "--description",
            "Changed",
            "--set-status",
            "new",
            "--session",
            "owner",
            "--image",
            "b.jpg",
        ],
    );
    let after = ok(p, &["show", "1"]);
    assert_eq!(after["task"]["description"], "Changed");
    assert_eq!(after["task"]["status"], "new");
    assert!(after["task"]["harness_session"].is_null());
    assert_eq!(after["events"].as_array().unwrap().len(), 2);
    assert_eq!(after["images"].as_array().unwrap().len(), 2);
}

#[test]
fn rejected_image_insert_rolls_back_new_task_and_released_edit() {
    let dir = project();
    let p = dir.path();
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_image BEFORE INSERT ON images WHEN NEW.name='b.jpg' BEGIN SELECT RAISE(ABORT,'image rejected'); END;").unwrap();
    error(
        p,
        &["add", "Invalid", "--image", "a.png", "--image", "b.jpg"],
        1,
        "image rejected",
    );
    assert_eq!(ok(p, &["list"]), serde_json::json!([]));
    assert_eq!(
        conn.query_row("SELECT count(*) FROM images", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(!p.join(".qqq/images/1/1.png").exists());
    ok(p, &["add", "Original", "--image", "a.png"]);
    ok(p, &["next", "--session", "owner"]);
    let before = ok(p, &["show", "1"]);
    error(
        p,
        &[
            "edit",
            "1",
            "--description",
            "Changed",
            "--set-status",
            "new",
            "--session",
            "owner",
            "--image",
            "a.png",
            "--image",
            "b.jpg",
        ],
        1,
        "image rejected",
    );
    assert_eq!(ok(p, &["show", "1"]), before);
    assert!(!p.join(".qqq/images/1/2.png").exists());
}

#[test]
fn busy_commit_removes_new_image_file_so_retry_can_use_same_id() {
    let dir = project();
    let p = dir.path();
    let reader = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    reader
        .execute_batch("BEGIN; SELECT count(*) FROM tasks;")
        .unwrap();
    error(
        p,
        &["add", "First", "--image", "a.png"],
        1,
        "database is locked",
    );
    assert!(!p.join(".qqq/images/1/1.png").exists());
    reader.execute_batch("ROLLBACK").unwrap();
    fs::write(p.join("a.png"), b"\x89PNG\r\n\x1a\nchanged").unwrap();
    ok(p, &["add", "Retried", "--image", "a.png"]);
    assert_eq!(
        fs::read(p.join(".qqq/images/1/1.png")).unwrap(),
        b"\x89PNG\r\n\x1a\nchanged"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_image_directories_cannot_escape_project_backup() {
    use std::os::unix::fs::symlink;

    let dir = project();
    let p = dir.path();
    let outside = TempDir::new().unwrap();
    symlink(outside.path(), p.join(".qqq/images")).unwrap();
    error(p, &["add", "Task", "--image", "a.png"], 1, "symlink");
    assert_eq!(ok(p, &["list"]), serde_json::json!([]));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);

    fs::remove_file(p.join(".qqq/images")).unwrap();
    fs::create_dir(p.join(".qqq/images")).unwrap();
    symlink(outside.path(), p.join(".qqq/images/1")).unwrap();
    error(p, &["add", "Task", "--image", "a.png"], 1, "symlink");
    assert_eq!(ok(p, &["list"]), serde_json::json!([]));
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn missing_nonregular_and_oversized_images_fail_without_creating_tasks() {
    let dir = project();
    let p = dir.path();
    error(
        p,
        &["add", "Task", "--image", "missing.png"],
        1,
        "Cannot read missing.png",
    );
    error(p, &["add", "Task", "--image", "."], 1, "regular file");
    let file = fs::File::create(p.join("large.png")).unwrap();
    file.set_len(20 * 1024 * 1024 + 1).unwrap();
    error(
        p,
        &["add", "Task", "--image", "large.png"],
        1,
        "20 MiB limit",
    );
    assert_eq!(ok(p, &["list"]), serde_json::json!([]));
}

#[test]
fn export_flags_are_paired_and_standalone_image_command_is_absent() {
    let dir = project();
    let p = dir.path();
    error(p, &["show", "1", "--export-image", "1"], 2, "--output");
    error(
        p,
        &["show", "1", "--output", "out.png"],
        2,
        "--export-image",
    );
    error(
        p,
        &["image", "add", "1", "a.png"],
        2,
        "unrecognized subcommand 'image'",
    );
    let help = run(p, &["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout).unwrap();
    assert!(
        !text
            .lines()
            .any(|line| line.trim_start().starts_with("image "))
    );
}

#[cfg(unix)]
#[test]
fn editor_composition_combines_flag_images_and_aborts_without_partial_save() {
    let dir = project();
    let p = dir.path();
    fs::write(
        p.join("compose.sh"),
        "printf 'Composed\\n\\nEdited details\\n' > \"$1\"\n",
    )
    .unwrap();
    let out = command(p)
        .env("EDITOR", "sh compose.sh")
        .args([
            "add",
            "Prefilled",
            "--edit",
            "--image",
            "a.png",
            "--image",
            "b.jpg",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let detail = ok(p, &["show", "1"]);
    assert_eq!(
        detail["task"]["description"],
        "Composed\n\nEdited details\n"
    );
    assert_eq!(detail["images"].as_array().unwrap().len(), 2);
    let out = command(p)
        .env("EDITOR", "false")
        .args(["add", "Failed", "--edit", "--image", "a.png"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(ok(p, &["list"]).as_array().unwrap().len(), 1);
    assert_eq!(ok(p, &["show", "1"]), detail);
}

#[cfg(unix)]
#[test]
fn forced_edit_composition_appends_images_without_changing_ownership() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Original", "--image", "a.png"]);
    ok(p, &["next", "--session", "owner"]);
    let before = ok(p, &["show", "1"]);
    fs::write(
        p.join("compose.sh"),
        "printf 'Edited\\n\\nNew details\\n' > \"$1\"\n",
    )
    .unwrap();
    let out = command(p)
        .env("EDITOR", "sh compose.sh")
        .args(["edit", "1", "--edit", "--image", "b.jpg"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = ok(p, &["show", "1"]);
    assert_eq!(after["task"]["description"], "Edited\n\nNew details\n");
    assert_eq!(
        after["task"]["harness_session"],
        before["task"]["harness_session"]
    );
    assert_eq!(after["events"], before["events"]);
    assert_eq!(after["images"][0], before["images"][0]);
    assert_eq!(after["images"].as_array().unwrap().len(), 2);
    let out = command(p)
        .env("EDITOR", "false")
        .args(["edit", "1", "--edit", "--image", "a.png"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(ok(p, &["show", "1"]), after);
}
