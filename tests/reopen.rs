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
