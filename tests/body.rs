use rusqlite::Connection;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command(p: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.current_dir(p)
        .env("HOME", p)
        .env_remove("EDITOR")
        .env_remove("QQQ_SESSION")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    c
}
fn run(p: &Path, args: &[&str]) -> Output {
    command(p).arg("--json").args(args).output().unwrap()
}
fn ok(p: &Path, args: &[&str]) -> Value {
    let out = run(p, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

#[test]
fn inline_and_flag_store_one_complete_body_without_title() {
    let d = project();
    let p = d.path();
    let body = "\nFirst line  \n\nSecond line\n";
    let task = ok(p, &["add", body]);
    assert_eq!(task["description"], body);
    assert!(task.get("title").is_none());
    let flagged = ok(p, &["add", "--description", body]);
    assert_eq!(flagged["description"], body);
    assert!(flagged.get("title").is_none());
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    let columns: Vec<String> = conn
        .prepare("SELECT name FROM pragma_table_info('tasks')")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!columns.iter().any(|name| name == "title"));
    assert_eq!(
        conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        4
    );
    assert_eq!(
        run(p, &["add", "Positional", "-d", "Flagged"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run(p, &["edit", "1", "--title", "Removed"]).status.code(),
        Some(2)
    );
}

#[test]
fn list_previews_first_line_but_show_and_json_keep_whole_body() {
    let d = project();
    let p = d.path();
    let body = "First line\n\nSecond line\nThird line";
    ok(p, &["add", body]);
    let list = command(p).arg("list").output().unwrap();
    assert!(list.status.success());
    let list = String::from_utf8(list.stdout).unwrap();
    assert!(list.contains("First line"));
    assert!(!list.contains("Second line"));
    assert!(!list.contains("Third line"));
    assert!(!list.contains("TITLE"));
    let shown = command(p).args(["show", "1"]).output().unwrap();
    assert!(shown.status.success());
    let shown = String::from_utf8(shown.stdout).unwrap();
    assert!(
        shown.contains("First line\n  \n  Second line\n  Third line"),
        "{shown}"
    );
    assert_eq!(ok(p, &["list"])[0]["description"], body);
    assert_eq!(ok(p, &["show", "1"])["task"]["description"], body);
}

#[test]
fn blank_body_is_rejected_but_blank_first_line_is_valid() {
    let d = project();
    let p = d.path();
    for body in ["", " \n\t"] {
        let out = run(p, &["add", body]);
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains("Description"));
    }
    ok(p, &["add", "\nContent\n"]);
    let before = ok(p, &["show", "1"]);
    for body in ["", " \n\t"] {
        assert_eq!(run(p, &["edit", "1", "-d", body]).status.code(), Some(1));
        assert_eq!(ok(p, &["show", "1"]), before);
    }
    let next = "\n\nReplacement\n";
    assert_eq!(ok(p, &["edit", "-1", "-d", next])["description"], next);
}

#[cfg(unix)]
#[test]
fn external_editor_round_trips_exact_prefill_and_complete_returned_buffer() {
    let d = project();
    let p = d.path();
    let before = "\nOriginal\n\nBody\n";
    let returned = "  \r\nFull buffer  \r\n\r\nRest\r\n";
    fs::write(p.join("saved.txt"), returned).unwrap();
    fs::write(
        p.join("edit.sh"),
        "cp \"$1\" prefilled.txt\ncp saved.txt \"$1\"\n",
    )
    .unwrap();
    let out = command(p)
        .arg("--json")
        .env("EDITOR", "sh edit.sh")
        .args(["add", before, "--edit"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(fs::read_to_string(p.join("prefilled.txt")).unwrap(), before);
    let task: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(task["description"], returned);
    assert!(task.get("title").is_none());
    fs::write(p.join("edit.sh"), "cp \"$1\" prefilled.txt\n").unwrap();
    let out = command(p)
        .arg("--json")
        .env("EDITOR", "sh edit.sh")
        .args(["edit", "1"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(p.join("prefilled.txt")).unwrap(),
        returned
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["description"],
        returned
    );
}

#[test]
fn legacy_title_schema_requires_manual_update_without_writes() {
    let d = TempDir::new().unwrap();
    let p = d.path();
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    conn.execute_batch("CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,description TEXT NOT NULL,status TEXT NOT NULL,assignee TEXT,created_at TEXT,updated_at TEXT,parent_id INTEGER);
        CREATE TABLE events(id INTEGER PRIMARY KEY,task_id INTEGER,action TEXT);
        INSERT INTO tasks VALUES (1,'First','Rest','new',NULL,'before','before',NULL);
        PRAGMA user_version=3;").unwrap();
    let before: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='tasks'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let out = run(p, &["list"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("update SQLite manually"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE name='tasks'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        before
    );
    assert_eq!(
        conn.query_row(
            "SELECT title||':'||description||':'||updated_at FROM tasks",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "First:Rest:before"
    );
}

#[test]
fn legacy_schema_guard_rechecks_after_waiting_for_initialization_lock() {
    use std::{process::Stdio, thread, time::Duration};
    let d = TempDir::new().unwrap();
    let p = d.path();
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    conn.execute_batch("CREATE TABLE lock_marker(x INTEGER); BEGIN IMMEDIATE;")
        .unwrap();
    let mut child = command(p)
        .args(["--json", "init"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(1000));
    assert!(child.try_wait().unwrap().is_none());
    conn.execute_batch("CREATE TABLE tasks(id INTEGER PRIMARY KEY,title TEXT NOT NULL,description TEXT NOT NULL,status TEXT NOT NULL,assignee TEXT,created_at TEXT,updated_at TEXT,parent_id INTEGER);
        CREATE TABLE events(id INTEGER PRIMARY KEY,task_id INTEGER,session TEXT,action TEXT,created_at TEXT);
        INSERT INTO tasks VALUES(1,'First','Rest','new',NULL,'before','before',NULL);
        PRAGMA user_version=3; COMMIT;").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("update SQLite manually"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        conn.query_row("SELECT title||':'||description FROM tasks", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "First:Rest"
    );
    assert_eq!(
        conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        3
    );
}
