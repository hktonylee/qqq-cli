use rusqlite::Connection;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfixture";
const JPEG: &[u8] = b"\xff\xd8\xfffixture";

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .arg("--json")
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("EDITOR")
        .env_remove("QQQ_SESSION")
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
    let out = run(dir, args);
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
    let conn = Connection::open(p.join("qqq.db")).unwrap();
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
