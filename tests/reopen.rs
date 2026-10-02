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

fn v8_project() -> TempDir {
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
        include_str!("../src/migrate_v7.sql"),
        include_str!("../src/migrate_v8.sql"),
    ] {
        conn.execute_batch(migration).unwrap();
    }
    conn.execute_batch(
        "INSERT INTO tasks(id,description,status,created_at,updated_at)
         VALUES (5,'Legacy completed','completed','2026-01-01T00:00:00Z','2026-01-02T00:00:00Z');
         INSERT INTO events(id,task_id,session,action) VALUES (8,5,'owner','complete');
         INSERT INTO events(id,task_id,session,action) VALUES (20,5,'owner','archive');
         DELETE FROM events WHERE id=20;",
    )
    .unwrap();
    dir
}

#[test]
fn migration_from_eight_preserves_events_and_autoincrement() {
    let dir = v8_project();
    let path = dir.path();
    let detail = ok(path, &["show", "5"]);
    assert_eq!(detail["task"]["status"], "completed");
    assert_eq!(detail["task"]["updated_at"], "2026-01-02T00:00:00Z");
    assert_eq!(detail["events"][0]["action"], "complete");
    let conn = Connection::open(path.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        9
    );
    conn.execute(
        "INSERT INTO events(task_id,session,action) VALUES (5,'cli','reopen')",
        [],
    )
    .unwrap();
    assert_eq!(conn.last_insert_rowid(), 21);
}

#[test]
fn concurrent_version_eight_opens_migrate_once() {
    let dir = v8_project();
    let path = dir.path();
    let children: Vec<_> = (0..4)
        .map(|_| command(path).arg("show").arg("5").spawn().unwrap())
        .collect();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let detail: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(detail["events"][0]["action"], "complete");
    }
    let conn = Connection::open(path.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        9
    );
}

#[test]
fn reopen_preserves_content_and_history_and_returns_new_task() {
    let dir = project();
    let path = dir.path();
    std::fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Parent"]);
    ok(path, &["next", "--session", "worker"]);
    ok(path, &["complete", "1", "--session", "worker"]);
    ok(
        path,
        &[
            "add",
            "Keep content",
            "--parent",
            "1",
            "--priority",
            "7",
            "--image",
            "a.png",
        ],
    );
    ok(path, &["message", "2", "Keep note"]);
    ok(path, &["next", "--session", "worker"]);
    ok(path, &["complete", "2", "--session", "worker"]);
    let before = ok(path, &["show", "2"]);
    std::thread::sleep(std::time::Duration::from_millis(5));
    let reopened = ok(path, &["reopen", "-1", "--session", "reviewer"]);
    assert_eq!(reopened["id"], 2);
    assert_eq!(reopened["status"], "new");
    assert_eq!(reopened["description"], before["task"]["description"]);
    assert_eq!(reopened["priority"], 7);
    assert_eq!(reopened["parent_id"], 1);
    assert_eq!(reopened["created_at"], before["task"]["created_at"]);
    assert_ne!(reopened["updated_at"], before["task"]["updated_at"]);
    assert!(reopened["claim_key"].is_null());
    let after = ok(path, &["show", "2"]);
    assert_eq!(after["messages"], before["messages"]);
    assert_eq!(after["images"], before["images"]);
    assert_eq!(after["herdr"], before["herdr"]);
    assert_eq!(
        &after["events"].as_array().unwrap()[..2],
        before["events"].as_array().unwrap()
    );
    assert_eq!(after["events"][2]["action"], "reopen");
    assert_eq!(after["events"][2]["session"], "reviewer");
    assert_eq!(ok(path, &["list", "--max-completed", "0"])[0]["id"], 2);
    fail(path, &["reopen", "2"], "must be completed");
    assert_eq!(ok(path, &["show", "2"]), after);
    assert!(human(path, &["reopen", "1"]).contains("Status: New"));
}

#[test]
fn reopen_rejects_non_completed_without_mutation() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "New"]);
    ok(path, &["add", "Active", "--priority", "1"]);
    ok(path, &["next", "--session", "worker"]);
    ok(path, &["add", "Error", "--priority", "2"]);
    ok(path, &["next", "--session", "other"]);
    ok(
        path,
        &[
            "edit",
            "3",
            "--set-status",
            "error",
            "--reason",
            "Failure",
            "--session",
            "other",
        ],
    );
    let before: Vec<_> = (1..=3)
        .map(|id| ok(path, &["show", &id.to_string()]))
        .collect();
    for id in 1..=3 {
        fail(path, &["reopen", &id.to_string()], "must be completed");
    }
    for id in 1..=3 {
        assert_eq!(
            ok(path, &["show", &id.to_string()]),
            before[(id - 1) as usize]
        );
    }
}
