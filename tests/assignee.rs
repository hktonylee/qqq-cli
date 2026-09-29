use rusqlite::Connection;
use serde_json::{Value, json};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn command(dir: &TempDir) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.current_dir(dir.path())
        .arg("--json")
        .env_remove("QQQ_SESSION")
        .env_remove("HERDR_ENV");
    c
}
fn run(dir: &TempDir, args: &[&str]) -> Output {
    command(dir).args(args).output().unwrap()
}
fn ok(dir: &TempDir, args: &[&str]) -> Value {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn assignment(task: &Value, expected: Value) {
    assert!(task.get("owner_session").is_none());
    assert_eq!(task.get("assignee"), Some(&expected));
}
fn version_two() -> TempDir {
    let dir = TempDir::new().unwrap();
    let conn = Connection::open(dir.path().join("qqq.db")).unwrap();
    conn.execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v2.sql"))
        .unwrap();
    conn.execute_batch(
        "INSERT INTO tasks(title,status,owner_session) VALUES ('Parent','in_progress','legacy');
        INSERT INTO tasks(title,parent_id) VALUES ('Child',1);
        INSERT INTO messages(task_id,body) VALUES (1,'Note');
        INSERT INTO events(task_id,session,action) VALUES (1,'legacy','claim');
        INSERT INTO images(task_id,name,media_type,data) VALUES (1,'x.png','image/png',X'010203');
        INSERT INTO herdr_links(task_id,link_json) VALUES (1,'{\"server\":null,\"identity\":{\"agent\":\"codex\",\"kind\":\"id\",\"value\":\"legacy-session\"},\"pane\":{\"pane_id\":\"p1\",\"workspace_id\":\"w1\",\"tab_id\":\"t1\"}}');",
    )
    .unwrap();
    dir
}

#[test]
fn task_responses_use_assignee_only() {
    let d = TempDir::new().unwrap();
    ok(&d, &["init"]);
    assignment(&ok(&d, &["add", "Task"]), Value::Null);
    assignment(&ok(&d, &["next", "--session", "a"]), json!("a"));
    assignment(&ok(&d, &["list"])[0], json!("a"));
    assignment(&ok(&d, &["show", "1"])["task"], json!("a"));
    assignment(&ok(&d, &["edit", "1", "--title", "Edited"]), json!("a"));
    assignment(
        &ok(
            &d,
            &["edit", "1", "--set-status", "pending", "--session", "a"],
        ),
        Value::Null,
    );
    assignment(&ok(&d, &["next", "--session", "b"]), json!("b"));
    assignment(&ok(&d, &["complete", "1", "--session", "b"]), Value::Null);
    let out = Command::new(env!("CARGO_BIN_EXE_qqq"))
        .current_dir(d.path())
        .args(["show", "1"])
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("Assignee: -"));
    assert!(!text.contains("Owner:"));
}

#[test]
fn v2_upgrade_preserves_data_and_enforces_renamed_constraints() {
    let d = version_two();
    let conn = Connection::open(d.path().join("qqq.db")).unwrap();
    let before: (String, String) = conn
        .query_row(
            "SELECT created_at,updated_at FROM tasks WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    for _ in 0..2 {
        let show = ok(&d, &["show", "1"]);
        assignment(&show["task"], json!("legacy"));
        assert_eq!(show["task"]["created_at"], before.0);
        assert_eq!(show["task"]["updated_at"], before.1);
        assert_eq!(show["messages"][0]["body"], "Note");
        assert_eq!(show["events"][0]["action"], "claim");
        assert_eq!(show["images"][0]["bytes"], 3);
        assert_eq!(show["herdr"]["identity"]["value"], "legacy-session");
        assert_eq!(ok(&d, &["show", "2"])["task"]["parent_id"], 1);
    }
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert!(
        conn.query_row("SELECT owner_session FROM tasks", [], |r| r
            .get::<_, Option<String>>(0))
            .is_err()
    );
    assert!(
        conn.execute(
            "INSERT INTO tasks(title,status,assignee) VALUES ('Dup','in_progress','legacy')",
            []
        )
        .is_err()
    );
    assert!(
        conn.execute("UPDATE tasks SET assignee=NULL WHERE id=1", [])
            .is_err()
    );
    assert!(
        conn.execute("UPDATE tasks SET assignee='bad' WHERE id=2", [])
            .is_err()
    );
    assert_eq!(
        conn.query_row("SELECT data FROM images", [], |r| r.get::<_, Vec<u8>>(0))
            .unwrap(),
        vec![1, 2, 3]
    );
    assert!(
        !run(&d, &["complete", "1", "--session", "wrong"])
            .status
            .success()
    );
    assert_eq!(ok(&d, &["next", "--session", "legacy"])["id"], 1);
    assignment(
        &ok(&d, &["complete", "1", "--session", "legacy"]),
        Value::Null,
    );
    assert_eq!(ok(&d, &["next", "--session", "child"])["id"], 2);
}

#[test]
fn concurrent_v2_opens_migrate_once() {
    let d = version_two();
    let children: Vec<_> = (0..6)
        .map(|_| {
            command(&d)
                .arg("list")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let tasks: Value = serde_json::from_slice(&out.stdout).unwrap();
        assignment(&tasks[0], json!("legacy"));
    }
}
