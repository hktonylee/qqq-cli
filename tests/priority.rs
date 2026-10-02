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
