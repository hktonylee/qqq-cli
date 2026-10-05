use rusqlite::Connection;
use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

fn command(path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .arg("--json")
        .current_dir(path)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn run(path: &Path, args: &[&str]) -> Output {
    command(path).args(args).output().unwrap()
}

fn ok(path: &Path, args: &[&str]) -> Value {
    let output = run(path, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fail(path: &Path, args: &[&str], expected: &str) {
    let output = run(path, args);
    assert_eq!(output.status.code(), Some(1), "{args:?}");
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

fn human(path: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_qqq"))
        .arg("--human")
        .current_dir(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn v7_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".qqq")).unwrap();
    let conn = Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
    for migration in [
        include_str!("../src/sql/schema.sql"),
        include_str!("../src/sql/migrate_v2.sql"),
        include_str!("../src/sql/migrate_v3.sql"),
        include_str!("../src/sql/migrate_v4.sql"),
        include_str!("../src/sql/migrate_v5.sql"),
        include_str!("../src/sql/migrate_v6.sql"),
        include_str!("../src/sql/migrate_v7.sql"),
    ] {
        conn.execute_batch(migration).unwrap();
    }
    conn.execute_batch(
        "INSERT INTO tasks(id,description,created_at,updated_at,priority) VALUES
         (5,'Legacy','2026-01-01T00:00:00Z','2026-01-02T00:00:00Z',4);
         INSERT INTO events(id,task_id,session,action) VALUES (8,5,'old-owner','release');
         INSERT INTO images(id,task_id,name,media_type,bytes) VALUES (7,5,'a.png','image/png',3);
         INSERT INTO tasks(id,description) VALUES (12,'Deleted task');
         DELETE FROM tasks WHERE id=12;
         INSERT INTO events(id,task_id,session,action) VALUES (20,5,'old-owner','claim');
         DELETE FROM events WHERE id=20;",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join(".qqq/images/5")).unwrap();
    std::fs::write(dir.path().join(".qqq/images/5/7.png"), b"png").unwrap();
    dir
}

#[test]
fn migration_from_seven_preserves_data_and_defaults_to_unarchived() {
    let dir = v7_project();
    let path = dir.path();
    let task = &ok(path, &["list"])[0];
    assert_eq!(task["id"], 5);
    assert_eq!(task["priority"], 4);
    assert_eq!(task["created_at"], "2026-01-01T00:00:00Z");
    assert_eq!(task["updated_at"], "2026-01-02T00:00:00Z");
    assert_eq!(task["archived"], false);
    let detail = ok(path, &["show", "5"]);
    assert_eq!(detail["events"][0]["action"], "release");
    assert_eq!(detail["images"][0]["id"], 7);
    let conn = Connection::open(path.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        12
    );
    assert_eq!(
        conn.query_row("SELECT id FROM events WHERE task_id=5", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        8
    );
    conn.execute(
        "INSERT INTO events(task_id,session,action) VALUES (5,'cli','archive')",
        [],
    )
    .unwrap();
    assert_eq!(conn.last_insert_rowid(), 21);
    assert_eq!(ok(path, &["add", "After migration"])["id"], 13);
}

#[test]
fn concurrent_version_seven_opens_migrate_once() {
    let dir = v7_project();
    let path = dir.path();
    let children: Vec<_> = (0..4)
        .map(|_| command(path).arg("list").spawn().unwrap())
        .collect();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let tasks: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(tasks[0]["archived"], false);
    }
    let conn = Connection::open(path.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        12
    );
}

#[test]
fn archive_round_trip_preserves_task_and_history_without_duplicate_events() {
    let dir = project();
    let path = dir.path();
    std::fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    let created = ok(
        path,
        &["add", "Keep content", "--priority", "7", "--image", "a.png"],
    );
    ok(path, &["message", "1", "Keep note"]);
    let before = ok(path, &["show", "1"]);
    let archived = ok(path, &["archive", "1"]);
    assert_eq!(archived["status"], created["status"]);
    assert_eq!(archived["description"], created["description"]);
    assert_eq!(archived["priority"], 7);
    assert_eq!(archived["archived"], true);
    let after = ok(path, &["show", "1"]);
    assert_eq!(after["messages"], before["messages"]);
    assert_eq!(after["images"], before["images"]);
    assert_eq!(
        std::fs::read(path.join(".qqq/images/1/1.png")).unwrap(),
        b"\x89PNG\r\n\x1a\nfixture"
    );
    assert_eq!(after["events"].as_array().unwrap().len(), 1);
    assert_eq!(after["events"][0]["action"], "archive");
    assert_eq!(after["events"][0]["session"], "cli");
    let repeated = ok(path, &["archive", "1"]);
    assert_eq!(repeated["updated_at"], archived["updated_at"]);
    assert_eq!(ok(path, &["show", "1"])["events"], after["events"]);
    let restored = ok(path, &["unarchive", "1"]);
    assert_eq!(restored["archived"], false);
    assert_eq!(ok(path, &["show", "1"])["events"][1]["action"], "unarchive");
    let repeated = ok(path, &["unarchive", "1"]);
    assert_eq!(repeated["updated_at"], restored["updated_at"]);
    assert_eq!(
        ok(path, &["show", "1"])["events"].as_array().unwrap().len(),
        2
    );

    ok(path, &["next", "--session", "worker"]);
    ok(path, &["complete", "1", "--session", "worker"]);
    let completed_before = ok(path, &["show", "1"]);
    let completed = ok(path, &["archive", "1"]);
    assert_eq!(completed["status"], "completed");
    let completed_after = ok(path, &["show", "1"]);
    assert_eq!(completed_after["events"][2], completed_before["events"][2]);
    assert_eq!(completed_after["events"][3], completed_before["events"][3]);
    assert_eq!(completed_after["events"][4]["action"], "archive");
}

#[test]
fn active_tasks_cannot_be_archived() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Active"]);
    ok(path, &["next", "--session", "worker"]);
    fail(path, &["archive", "1"], "in progress");
    assert_eq!(ok(path, &["show", "1"])["task"]["archived"], false);
    assert_eq!(ok(path, &["next", "--session", "worker"])["id"], 1);
}

#[test]
fn errored_task_keeps_status_and_history_across_archive() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Failed"]);
    ok(path, &["next", "--session", "worker"]);
    ok(
        path,
        &[
            "edit",
            "1",
            "--set-status",
            "error",
            "--reason",
            "Failure",
            "--session",
            "worker",
        ],
    );
    let before = ok(path, &["show", "1"]);
    let archived = ok(path, &["archive", "1", "--session", "reviewer"]);
    assert_eq!(archived["status"], "error");
    assert_eq!(archived["archived"], true);
    let after = ok(path, &["show", "1"]);
    assert_eq!(after["messages"], before["messages"]);
    assert_eq!(after["events"][0], before["events"][0]);
    assert_eq!(after["events"][1], before["events"][1]);
    assert_eq!(after["events"][2]["session"], "reviewer");
    assert_eq!(after["events"][2]["action"], "archive");
    assert_eq!(ok(path, &["unarchive", "1"])["status"], "error");
}

#[test]
fn archive_rejects_blocked_children_and_completed_parent_still_releases_child() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Parent"]);
    ok(path, &["add", "Child", "--parent", "1"]);
    ok(path, &["add", "Grandchild", "--parent", "2"]);
    fail(path, &["archive", "1"], "unfinished child");
    fail(path, &["archive", "2"], "unfinished child");
    ok(path, &["archive", "3"]);
    ok(path, &["archive", "2"]);
    ok(path, &["archive", "1"]);
    fail(path, &["unarchive", "2"], "archived unfinished parent");
    ok(path, &["unarchive", "1"]);
    ok(path, &["unarchive", "2"]);
    ok(path, &["unarchive", "3"]);
    ok(path, &["next", "--session", "parent-worker"]);
    ok(path, &["complete", "1", "--session", "parent-worker"]);
    ok(path, &["archive", "1"]);
    assert_eq!(ok(path, &["next", "--session", "child-worker"])["id"], 2);
}

#[test]
fn archived_unfinished_parent_rejects_new_and_reparented_unfinished_children() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Parent"]);
    ok(path, &["add", "Existing"]);
    ok(path, &["add", "Archived child"]);
    ok(path, &["archive", "3"]);
    ok(path, &["archive", "1"]);
    fail(
        path,
        &["add", "Blocked", "--parent", "1"],
        "archived unfinished parent",
    );
    fail(
        path,
        &["edit", "2", "--set-parent", "1"],
        "archived unfinished parent",
    );
    fail(
        path,
        &["edit", "3", "--set-parent", "1"],
        "archived unfinished parent",
    );
    assert_eq!(ok(path, &["show", "2"])["task"]["parent_id"], Value::Null);
}

#[test]
fn default_list_hides_archived_and_explicit_list_restores_visibility() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Visible"]);
    ok(path, &["add", "Hidden"]);
    ok(path, &["archive", "2"]);
    let visible = ok(path, &["list"]);
    assert_eq!(visible.as_array().unwrap().len(), 1);
    assert_eq!(visible[0]["description"], "Visible");
    let all = ok(path, &["list", "--include-archived"]);
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_eq!(all[1]["archived"], true);
    assert_eq!(all[1]["description"], "Hidden");
}

#[test]
fn hidden_completed_task_does_not_consume_visible_completion_limit() {
    let dir = project();
    let path = dir.path();
    for id in 1..=3 {
        ok(path, &["add", &format!("Task {id}")]);
        ok(path, &["next", "--session", "worker"]);
        ok(path, &["complete", &id.to_string(), "--session", "worker"]);
    }
    ok(path, &["archive", "3"]);
    let visible = ok(path, &["list", "--max-completed", "1"]);
    assert_eq!(visible.as_array().unwrap().len(), 1);
    assert_eq!(visible[0]["id"], 2);
    let included = ok(
        path,
        &["list", "--max-completed", "1", "--include-archived"],
    );
    assert_eq!(included.as_array().unwrap().len(), 1);
    assert_eq!(included[0]["id"], 3);
    assert_eq!(ok(path, &["list", "-a"]).as_array().unwrap().len(), 2);
}

#[test]
fn archived_ready_task_is_skipped_by_concurrent_claims() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "High", "--priority", "100"]);
    ok(path, &["add", "First", "--priority", "5"]);
    ok(path, &["add", "Second", "--priority", "5"]);
    ok(path, &["archive", "1"]);
    let first = command(path)
        .args(["next", "--session", "one"])
        .spawn()
        .unwrap();
    let second = command(path)
        .args(["next", "--session", "two"])
        .spawn()
        .unwrap();
    let mut ids: Vec<i64> = [first, second]
        .into_iter()
        .map(|child| {
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            let task: Value = serde_json::from_slice(&output.stdout).unwrap();
            task["id"].as_i64().unwrap()
        })
        .collect();
    ids.sort();
    assert_eq!(ids, [2, 3]);
    assert!(ok(path, &["next", "--session", "three"]).is_null());
    ok(path, &["unarchive", "1"]);
    assert_eq!(ok(path, &["next", "--session", "three"])["id"], 1);
}

#[test]
fn human_and_json_views_identify_archived_tasks_without_changing_description() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Keep title"]);
    let archive = human(path, &["archive", "-1"]);
    assert!(archive.contains("Archived: yes"), "{archive}");
    let show = human(path, &["show", "1"]);
    assert!(show.contains("Archived:") && show.contains("yes"), "{show}");
    let list = human(path, &["list", "--include-archived"]);
    assert!(list.contains("[archived] Keep title"), "{list}");
    let json = ok(path, &["list", "--include-archived"]);
    assert_eq!(json[0]["description"], "Keep title");
    assert_eq!(json[0]["archived"], true);
    let restored = human(path, &["unarchive", "-1"]);
    assert!(restored.contains("Archived: no"), "{restored}");
    assert!(!human(path, &["list"]).contains("[archived]"));
}
