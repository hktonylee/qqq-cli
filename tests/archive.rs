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

fn v7_project() -> TempDir {
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
    ] {
        conn.execute_batch(migration).unwrap();
    }
    conn.execute_batch(
        "INSERT INTO tasks(id,description,created_at,updated_at,priority) VALUES
         (5,'Legacy','2026-01-01T00:00:00Z','2026-01-02T00:00:00Z',4);
         INSERT INTO events(id,task_id,session,action) VALUES (8,5,'old-owner','release');
         INSERT INTO images(id,task_id,name,media_type,bytes) VALUES (7,5,'a.png','image/png',3);",
    )
    .unwrap();
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
        8
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
    assert_eq!(conn.last_insert_rowid(), 9);
    assert_eq!(ok(path, &["add", "After migration"])["id"], 6);
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
        8
    );
}
