use rusqlite::Connection;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
    time::Duration,
};
use tempfile::TempDir;

fn command(path: &Path, json: bool, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    if json {
        command.arg("--json");
    }
    command
        .args(args)
        .current_dir(path)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .output()
        .unwrap()
}

fn ok(path: &Path, args: &[&str]) -> Value {
    let output = command(path, true, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fail(path: &Path, args: &[&str], expected: &str) {
    let output = command(path, true, args);
    assert_eq!(output.status.code(), Some(1), "{args:?}");
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

fn db(path: &Path) -> Connection {
    Connection::open(path.join(".qqq/qqq.db")).unwrap()
}

fn count(conn: &Connection, table: &str, task_id: i64) -> i64 {
    conn.query_row(
        &format!("SELECT count(*) FROM {table} WHERE task_id=?"),
        [task_id],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn delete_previews_then_removes_task_data_and_never_reuses_id() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["message", "1", "note"]);
    ok(path, &["archive", "1"]);
    let conn = db(path);
    conn.execute(
        "INSERT INTO herdr_links(task_id,link_json) VALUES (1,'{}')",
        [],
    )
    .unwrap();
    let stored = path.join(".qqq/images/1/1.png");
    let db_path = path.join(".qqq/qqq.db");
    let before = fs::read(&db_path).unwrap();
    let before_mtime = fs::metadata(&db_path).unwrap().modified().unwrap();
    let preview = ok(path, &["delete", "1"]);
    assert_eq!(preview["deleted"], false);
    assert_eq!(preview["task"]["id"], 1);
    assert_eq!(preview["messages"], 1);
    assert_eq!(preview["images"], 1);
    assert_eq!(preview["herdr_links"], 1);
    assert_eq!(preview["image_paths"].as_array().unwrap().len(), 1);
    assert_eq!(fs::read(&db_path).unwrap(), before);
    assert_eq!(
        fs::metadata(&db_path).unwrap().modified().unwrap(),
        before_mtime
    );
    assert!(stored.is_file());
    assert!(!path.join(".qqq/.delete-staging").exists());
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tasks WHERE id=1", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let human = command(path, false, &["delete", "1"]);
    assert!(human.status.success());
    let human = String::from_utf8(human.stdout).unwrap();
    assert!(human.contains("Preview"), "{human}");
    assert!(human.contains("Description: Keep"), "{human}");
    assert!(human.contains("qqq backup"), "{human}");
    assert!(human.contains("--yes"), "{human}");

    let deleted = ok(path, &["delete", "1", "--yes"]);
    assert_eq!(deleted["deleted"], true);
    assert_eq!(deleted["task"]["id"], 1);
    assert!(!stored.exists());
    assert!(!path.join(".qqq/images/1").exists());
    for table in ["messages", "events", "images", "herdr_links"] {
        assert_eq!(count(&conn, table, 1), 0, "{table}");
    }
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tasks WHERE id=1", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    fail(path, &["show", "1"], "not found");
    assert_eq!(ok(path, &["doctor"])["ok"], true);
    assert_eq!(ok(path, &["add", "Later"])["id"], 2);
}

#[test]
fn delete_requires_archived_leaf_and_explains_unblocking_steps() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Parent"]);
    fail(path, &["delete", "1"], "archive");
    let conn = db(path);
    conn.execute(
        "UPDATE tasks SET status='in_progress',claim_key='owner' WHERE id=1",
        [],
    )
    .unwrap();
    fail(path, &["delete", "1", "--yes"], "release");
    conn.execute(
        "UPDATE tasks SET status='new',claim_key=NULL WHERE id=1",
        [],
    )
    .unwrap();
    ok(path, &["add", "Child", "--parent", "1"]);
    ok(path, &["archive", "2"]);
    ok(path, &["archive", "1"]);
    fail(path, &["delete", "1"], "child");
    assert_eq!(ok(path, &["show", "1"])["task"]["archived"], true);
    assert_eq!(ok(path, &["show", "2"])["task"]["parent_id"], 1);
}

#[test]
fn delete_restores_staged_images_when_database_delete_fails() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["message", "1", "note"]);
    ok(path, &["archive", "1"]);
    let conn = db(path);
    conn.execute(
        "INSERT INTO herdr_links(task_id,link_json) VALUES (1,'{}')",
        [],
    )
    .unwrap();
    conn.execute_batch(
        "CREATE TRIGGER block_delete BEFORE DELETE ON tasks BEGIN SELECT RAISE(ABORT,'blocked'); END;",
    )
    .unwrap();
    fail(path, &["delete", "1", "--yes"], "blocked");
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tasks WHERE id=1", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(path.join(".qqq/images/1/1.png").is_file());
    assert_eq!(count(&conn, "images", 1), 1);
    assert_eq!(count(&conn, "messages", 1), 1);
    assert_eq!(count(&conn, "events", 1), 1);
    assert_eq!(count(&conn, "herdr_links", 1), 1);
    assert!(!path.join(".qqq/.delete-staging").exists());
}

#[test]
fn delete_rejects_missing_image_without_changing_database() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["archive", "1"]);
    fs::remove_file(path.join(".qqq/images/1/1.png")).unwrap();
    let db_path = path.join(".qqq/qqq.db");
    let before = fs::read(&db_path).unwrap();
    fail(path, &["delete", "1", "--yes"], "missing");
    assert_eq!(fs::read(&db_path).unwrap(), before);
    assert_eq!(ok(path, &["show", "1"])["task"]["id"], 1);
}

fn stage_task_image(path: &Path) -> std::path::PathBuf {
    let wrapper = path.join(".qqq/.delete-staging/1-test");
    fs::create_dir_all(&wrapper).unwrap();
    fs::write(wrapper.join("task-id"), b"1").unwrap();
    fs::rename(path.join(".qqq/images/1"), wrapper.join("images")).unwrap();
    wrapper
}

#[test]
fn next_open_restores_interrupted_precommit_stage() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["archive", "1"]);
    let wrapper = stage_task_image(path);
    let doctor = command(path, true, &["doctor"]);
    assert_eq!(doctor.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| { issue["code"] == "DELETE_RECOVERY_PENDING" })
    );
    assert_eq!(ok(path, &["show", "1"])["task"]["id"], 1);
    assert!(path.join(".qqq/images/1/1.png").is_file());
    assert!(!wrapper.exists());
}

#[test]
fn preview_never_recovers_interrupted_stage() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["archive", "1"]);
    let wrapper = stage_task_image(path);
    fail(path, &["delete", "1"], "recovery");
    assert!(wrapper.join("images/1.png").is_file());
    assert!(!path.join(".qqq/images/1").exists());
}

#[test]
fn preview_never_migrates_legacy_database() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Keep"]);
    ok(path, &["archive", "1"]);
    let conn = db(path);
    conn.pragma_update(None, "user_version", 8).unwrap();
    let db_path = path.join(".qqq/qqq.db");
    let before = fs::read(&db_path).unwrap();
    fail(path, &["delete", "1"], "schema version");
    assert_eq!(fs::read(&db_path).unwrap(), before);
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        8
    );
}

#[test]
fn next_open_cleans_interrupted_postcommit_stage() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["archive", "1"]);
    let wrapper = stage_task_image(path);
    let conn = db(path);
    conn.execute_batch(
        "DELETE FROM images WHERE task_id=1;
         DELETE FROM events WHERE task_id=1;
         DELETE FROM tasks WHERE id=1;",
    )
    .unwrap();
    assert_eq!(
        ok(path, &["list", "--include-archived"]),
        serde_json::json!([])
    );
    assert!(!wrapper.exists());
    assert!(!path.join(".qqq/.delete-staging").exists());
    assert!(!path.join(".qqq/images/1").exists());
    assert_eq!(ok(path, &["add", "Later"])["id"], 2);
}

#[test]
fn next_open_removes_empty_stage_from_interrupted_setup() {
    let dir = project();
    let path = dir.path();
    let wrapper = path.join(".qqq/.delete-staging/1-test");
    fs::create_dir_all(&wrapper).unwrap();
    assert_eq!(ok(path, &["list"]), serde_json::json!([]));
    assert!(!wrapper.exists());
    assert!(!path.join(".qqq/.delete-staging").exists());
}

#[test]
fn confirmed_human_output_distinguishes_completed_deletion() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Gone"]);
    ok(path, &["archive", "1"]);
    let output = command(path, false, &["delete", "1", "--yes"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Deleted task #1"), "{text}");
    assert!(!text.contains("Re-run with --yes"), "{text}");
}

#[test]
fn permanent_delete_requires_explicit_positive_id() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Keep"]);
    ok(path, &["archive", "1"]);
    let output = command(path, true, &["delete", "-1", "--yes"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(ok(path, &["show", "1"])["task"]["id"], 1);
}

#[test]
fn delete_rechecks_archive_guard_after_competing_writer_commits() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Keep"]);
    ok(path, &["archive", "1"]);
    let conn = db(path);
    conn.execute_batch("BEGIN IMMEDIATE; UPDATE tasks SET archived=0 WHERE id=1")
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_qqq"))
        .args(["--json", "delete", "1", "--yes"])
        .current_dir(path)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(child.try_wait().unwrap().is_none());
    conn.execute_batch("COMMIT").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("archive"));
    assert!(output.stdout.is_empty());
    assert_eq!(ok(path, &["show", "1"])["task"]["archived"], false);
}

#[cfg(unix)]
#[test]
fn delete_rejects_symlinked_attachment_without_touching_target() {
    use std::os::unix::fs::symlink;
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["archive", "1"]);
    let stored = path.join(".qqq/images/1/1.png");
    fs::remove_file(&stored).unwrap();
    symlink(path.join("a.png"), &stored).unwrap();
    fail(path, &["delete", "1", "--yes"], "unsafe");
    assert_eq!(
        fs::read(path.join("a.png")).unwrap(),
        b"\x89PNG\r\n\x1a\nfixture"
    );
    assert_eq!(ok(path, &["show", "1"])["task"]["id"], 1);
}
