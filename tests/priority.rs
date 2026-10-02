use rusqlite::Connection;
use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Stdio},
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

fn ok(path: &Path, args: &[&str]) -> Value {
    let output = command(path).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn human(path: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_qqq"))
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

fn v6_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".qqq")).unwrap();
    let conn = Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
    for migration in [
        include_str!("../src/schema.sql"),
        include_str!("../src/migrate_v2.sql"),
        include_str!("../src/migrate_v3.sql"),
        include_str!("../src/migrate_v4.sql"),
        include_str!("../src/migrate_v5.sql"),
        include_str!("../src/migrate_v6.sql"),
    ] {
        conn.execute_batch(migration).unwrap();
    }
    conn.execute_batch(
        "INSERT INTO tasks(description,created_at,updated_at) VALUES
         ('Older','2026-01-01T00:00:00Z','2026-01-02T00:00:00Z'),
         ('Newer','2026-01-03T00:00:00Z','2026-01-04T00:00:00Z');",
    )
    .unwrap();
    dir
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

#[test]
fn version_six_migration_defaults_priority_and_preserves_fifo() {
    let dir = v6_project();
    let path = dir.path();
    let tasks = ok(path, &["list"]);
    assert_eq!(tasks[0]["id"], 1);
    assert_eq!(tasks[1]["id"], 2);
    assert_eq!(tasks[0]["priority"], 0);
    assert_eq!(tasks[1]["priority"], 0);
    assert_eq!(tasks[0]["created_at"], "2026-01-01T00:00:00Z");
    assert_eq!(tasks[0]["updated_at"], "2026-01-02T00:00:00Z");
    let conn = Connection::open(path.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert_eq!(ok(path, &["next", "--session", "a"])["id"], 1);
}

#[test]
fn concurrent_version_six_opens_migrate_once() {
    let dir = v6_project();
    let path = dir.path();
    let first = command(path).arg("list").spawn().unwrap();
    let second = command(path).arg("list").spawn().unwrap();
    for child in [first, second] {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let tasks: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(tasks[0]["priority"], 0);
    }
    let conn = Connection::open(path.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        7
    );
}

#[test]
fn add_and_edit_priority_are_persisted_with_default_and_negative_values() {
    let dir = project();
    let path = dir.path();
    assert_eq!(ok(path, &["add", "High", "--priority", "8"])["priority"], 8);
    assert_eq!(ok(path, &["add", "Default"])["priority"], 0);
    assert_eq!(
        ok(path, &["add", "Low", "--priority", "-5"])["priority"],
        -5
    );
    let edited = ok(
        path,
        &["edit", "1", "--description", "Renamed", "--priority", "-3"],
    );
    assert_eq!(edited["description"], "Renamed");
    assert_eq!(edited["priority"], -3);
    assert_eq!(ok(path, &["show", "1"])["task"]["priority"], -3);
    assert_eq!(ok(path, &["list"])[2]["priority"], -5);
}

#[test]
fn external_editor_creation_and_edit_keep_requested_priority() {
    let dir = project();
    let path = dir.path();
    std::fs::write(path.join("editor.sh"), "printf 'From editor' > \"$1\"\n").unwrap();
    let add = command(path)
        .env("EDITOR", "sh ./editor.sh")
        .args(["add", "--edit", "--priority", "9"])
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let created: Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(created["priority"], 9);
    assert_eq!(created["description"], "From editor");

    let edit = command(path)
        .env("EDITOR", "sh ./editor.sh")
        .args(["edit", "1", "--edit", "--priority", "4"])
        .output()
        .unwrap();
    assert!(
        edit.status.success(),
        "{}",
        String::from_utf8_lossy(&edit.stderr)
    );
    let updated: Value = serde_json::from_slice(&edit.stdout).unwrap();
    assert_eq!(updated["priority"], 4);
}

#[test]
fn priority_only_edits_preserve_status_owner_and_history() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Active"]);
    ok(path, &["add", "Failure"]);
    ok(path, &["add", "Finished"]);
    ok(path, &["next", "--session", "owner-a"]);
    let before = ok(path, &["show", "1"]);
    let active = ok(path, &["edit", "1", "--priority", "20"]);
    assert_eq!(active["status"], "in_progress");
    assert_eq!(active["harness_session"], before["task"]["harness_session"]);
    assert_eq!(active["priority"], 20);
    assert_eq!(ok(path, &["show", "1"])["events"], before["events"]);
    assert_eq!(ok(path, &["next", "--session", "owner-a"])["id"], 1);

    ok(path, &["next", "--session", "owner-b"]);
    ok(
        path,
        &[
            "edit",
            "2",
            "--set-status",
            "error",
            "--reason",
            "Failed",
            "--session",
            "owner-b",
        ],
    );
    let failed = ok(path, &["edit", "2", "--priority", "-10"]);
    assert_eq!(failed["status"], "error");
    assert_eq!(failed["priority"], -10);

    ok(path, &["next", "--session", "owner-c"]);
    ok(path, &["complete", "3", "--session", "owner-c"]);
    let completed = ok(path, &["edit", "3", "--priority", "10"]);
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["priority"], 10);
}

#[test]
fn priority_rejects_out_of_range_and_non_integer_without_writes() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Keep"]);
    for value in ["-101", "101", "bad"] {
        let add = command(path)
            .args(["add", "Reject", "--priority", value])
            .output()
            .unwrap();
        assert_eq!(add.status.code(), Some(2), "{value}");
        assert!(add.stdout.is_empty());
        let edit = command(path)
            .args(["edit", "1", "--priority", value])
            .output()
            .unwrap();
        assert_eq!(edit.status.code(), Some(2), "{value}");
        assert!(edit.stdout.is_empty());
    }
    assert_eq!(ok(path, &["list"]).as_array().unwrap().len(), 1);
    assert_eq!(ok(path, &["show", "1"])["task"]["priority"], 0);
    let conn = Connection::open(path.join(".qqq/qqq.db")).unwrap();
    assert!(
        conn.execute("UPDATE tasks SET priority=101 WHERE id=1", [])
            .is_err()
    );
}

#[test]
fn claims_highest_ready_priority_then_oldest_id_without_displacing_owner() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Parent", "--priority", "1"]);
    ok(
        path,
        &["add", "Blocked child", "--parent", "1", "--priority", "100"],
    );
    ok(path, &["add", "High first", "--priority", "10"]);
    ok(path, &["add", "High second", "--priority", "10"]);
    assert_eq!(ok(path, &["next", "--session", "a"])["id"], 3);
    assert_eq!(ok(path, &["next", "--session", "a"])["id"], 3);
    assert_eq!(ok(path, &["next", "--session", "b"])["id"], 4);
    assert_eq!(ok(path, &["next", "--session", "c"])["id"], 1);
    assert!(ok(path, &["next", "--session", "d"]).is_null());
    ok(path, &["edit", "3", "--priority", "-100"]);
    assert_eq!(ok(path, &["next", "--session", "a"])["id"], 3);
    ok(path, &["complete", "1", "--session", "c"]);
    assert_eq!(ok(path, &["next", "--session", "d"])["id"], 2);
}

#[test]
fn concurrent_sessions_claim_unique_highest_priority_tasks() {
    let dir = project();
    let path = dir.path();
    for (description, priority) in [
        ("Low", "1"),
        ("Highest first", "9"),
        ("Highest second", "9"),
        ("Middle", "5"),
        ("Default", "0"),
    ] {
        ok(path, &["add", description, "--priority", priority]);
    }
    let children: Vec<_> = (0..3)
        .map(|index| {
            command(path)
                .args(["next", "--session", &format!("worker-{index}")])
                .spawn()
                .unwrap()
        })
        .collect();
    let mut claimed: Vec<i64> = children
        .into_iter()
        .map(|child| {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let task: Value = serde_json::from_slice(&output.stdout).unwrap();
            task["id"].as_i64().unwrap()
        })
        .collect();
    claimed.sort();
    assert_eq!(claimed, [2, 3, 4]);
}

#[test]
fn human_list_show_and_task_summaries_display_priority() {
    let dir = project();
    let path = dir.path();
    let added = human(path, &["add", "High", "--priority", "7"]);
    assert!(added.contains("Priority: 7"), "{added}");
    ok(path, &["add", "Low", "--priority", "-3"]);
    let list = human(path, &["list"]);
    assert!(list.lines().next().unwrap().contains("PRI"), "{list}");
    assert!(
        list.lines()
            .any(|line| line.contains(" 7 ") && line.ends_with("High")),
        "{list}"
    );
    assert!(
        list.lines()
            .any(|line| line.contains("-3 ") && line.ends_with("Low")),
        "{list}"
    );
    let show = human(path, &["show", "1"]);
    assert!(
        show.lines()
            .any(|line| line.trim().starts_with("Priority:") && line.ends_with('7')),
        "{show}"
    );
    let edited = human(path, &["edit", "1", "--priority", "-2"]);
    assert!(edited.contains("Priority: -2"), "{edited}");
}
