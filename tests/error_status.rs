use rusqlite::Connection;
use serde_json::Value;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn command(dir: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir.path())
        .env("HOME", dir.path())
        .env_remove("QQQ_SESSION")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .env_remove("EDITOR");
    command
}

fn run(dir: &TempDir, args: &[&str]) -> Output {
    command(dir).arg("--json").args(args).output().unwrap()
}

fn ok(dir: &TempDir, args: &[&str]) -> Value {
    let output = run(dir, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(&dir, &["init"]);
    dir
}

fn fail(dir: &TempDir, id: &str, session: &str) -> Value {
    ok(
        dir,
        &[
            "edit",
            id,
            "--set-status",
            "error",
            "--reason",
            "Missing API key",
            "--session",
            session,
        ],
    )
}

#[test]
fn failed_task_needs_explicit_manual_retry_and_retains_details() {
    let d = project();
    ok(&d, &["add", "Task", "--description", "Original details"]);
    ok(&d, &["message", "1", "Existing note"]);
    std::fs::write(d.path().join("x.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(&d, &["edit", "1", "--image", "x.png"]);
    ok(&d, &["next", "--local", "--session", "worker"]);
    let conn = Connection::open(d.path().join("qqq.db")).unwrap();
    conn.execute("INSERT INTO herdr_links VALUES (1, ?)", [LINK])
        .unwrap();
    let before = ok(&d, &["show", "1"]);
    let failed = fail(&d, "1", "worker");
    assert_eq!(failed["status"], "error");
    assert!(failed["assignee"].is_null());
    assert!(ok(&d, &["next", "--local", "--session", "worker"]).is_null());
    let detail = ok(&d, &["show", "1"]);
    assert_eq!(detail["task"]["created_at"], before["task"]["created_at"]);
    assert_eq!(detail["task"]["description"], "Original details");
    assert_eq!(detail["images"], before["images"]);
    assert_eq!(detail["herdr"], before["herdr"]);
    assert_eq!(detail["messages"][0], before["messages"][0]);
    assert_eq!(detail["messages"][1]["body"], "Missing API key");
    assert_eq!(detail["messages"][1]["session"], "worker");
    assert_eq!(detail["events"][1]["action"], "error");
    assert_eq!(detail["events"][1]["session"], "worker");
    assert!(
        !run(&d, &["complete", "1", "--session", "worker"])
            .status
            .success()
    );

    let retried = ok(
        &d,
        &["edit", "1", "--set-status", "new", "--title", "Fixed"],
    );
    assert_eq!(retried["status"], "new");
    assert_eq!(retried["title"], "Fixed");
    let detail = ok(&d, &["show", "1"]);
    assert_eq!(detail["messages"].as_array().unwrap().len(), 2);
    assert_eq!(detail["events"][2]["action"], "release");
    assert_eq!(detail["events"][2]["session"], "manual");
    assert_eq!(detail["herdr"], before["herdr"]);
    assert_eq!(
        ok(&d, &["next", "--local", "--session", "another"])["id"],
        1
    );
    assert!(ok(&d, &["show", "1"])["herdr"].is_null());
}

#[test]
fn error_status_validation_and_owner_checks_are_atomic() {
    let d = project();
    ok(&d, &["add", "Task"]);
    ok(&d, &["next", "--local", "--session", "owner"]);
    let before = ok(&d, &["show", "1"]);
    for args in [
        vec!["edit", "1", "--set-status", "error", "--session", "owner"],
        vec![
            "edit",
            "1",
            "--set-status",
            "error",
            "--reason",
            "  ",
            "--session",
            "owner",
        ],
        vec![
            "edit",
            "1",
            "--set-status",
            "error",
            "--reason",
            "Failure",
            "--session",
            "other",
            "--title",
            "Changed",
        ],
        vec![
            "edit",
            "1",
            "--set-status",
            "error",
            "--reason",
            "Failure",
            "--session",
            "owner",
            "--title",
            " ",
        ],
        vec![
            "edit",
            "1",
            "--set-status",
            "new",
            "--reason",
            "Failure",
            "--session",
            "owner",
        ],
        vec!["edit", "1", "--reason", "Failure", "--title", "Changed"],
    ] {
        assert!(!run(&d, &args).status.success(), "{args:?}");
        assert_eq!(ok(&d, &["show", "1"]), before, "{args:?}");
    }
    let failed = ok(
        &d,
        &[
            "edit",
            "1",
            "--set-status",
            "error",
            "--reason",
            "Failure",
            "--session",
            "owner",
            "--title",
            "Changed",
            "-d",
            "New details",
        ],
    );
    assert_eq!(failed["title"], "Changed");
    assert_eq!(failed["description"], "New details");
    let before = ok(&d, &["show", "1"]);
    assert!(
        !run(&d, &["edit", "1", "--set-status", "new", "--title", " "])
            .status
            .success()
    );
    assert_eq!(ok(&d, &["show", "1"]), before);
    let retried = ok(
        &d,
        &["edit", "1", "--set-status", "new", "--session", "human"],
    );
    assert_eq!(retried["status"], "new");
    assert_eq!(ok(&d, &["show", "1"])["events"][2]["session"], "human");
}

#[test]
fn only_owned_active_tasks_can_enter_error() {
    let d = project();
    ok(&d, &["add", "Task"]);
    let before = ok(&d, &["show", "1"]);
    assert!(
        !run(
            &d,
            &[
                "edit",
                "1",
                "--set-status",
                "error",
                "--reason",
                "Failure",
                "--session",
                "worker"
            ]
        )
        .status
        .success()
    );
    assert_eq!(ok(&d, &["show", "1"]), before);
    ok(&d, &["next", "--local", "--session", "worker"]);
    ok(&d, &["complete", "1", "--session", "worker"]);
    let before = ok(&d, &["show", "1"]);
    assert!(
        !run(
            &d,
            &[
                "edit",
                "1",
                "--set-status",
                "error",
                "--reason",
                "Failure",
                "--session",
                "worker"
            ]
        )
        .status
        .success()
    );
    assert_eq!(ok(&d, &["show", "1"]), before);
}

#[test]
fn failed_parent_blocks_children_while_worker_can_take_other_work() {
    let d = project();
    ok(&d, &["add", "Parent"]);
    ok(&d, &["add", "Child", "--parent", "1"]);
    ok(&d, &["add", "Independent"]);
    ok(&d, &["next", "--local", "--session", "worker"]);
    fail(&d, "1", "worker");
    assert_eq!(ok(&d, &["next", "--local", "--session", "worker"])["id"], 3);
    assert!(ok(&d, &["next", "--local", "--session", "other"]).is_null());
    ok(&d, &["edit", "1", "--set-status", "new"]);
    assert_eq!(ok(&d, &["next", "--local", "--session", "other"])["id"], 1);
    ok(&d, &["complete", "1", "--session", "other"]);
    assert_eq!(ok(&d, &["next", "--local", "--session", "other"])["id"], 2);
}

#[test]
fn error_is_visible_in_human_and_json_lists_with_completed_limit() {
    let d = project();
    ok(&d, &["add", "Needs attention"]);
    ok(&d, &["next", "--local", "--session", "worker"]);
    fail(&d, "1", "worker");
    let tasks = ok(&d, &["list", "--max-completed", "0"]);
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    assert_eq!(tasks[0]["status"], "error");
    for args in [vec!["list", "--max-completed", "0"], vec!["show", "1"]] {
        let output = command(&d).args(args).output().unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Error"));
        assert!(!text.contains('\x1b'));
    }
}

const LINK: &str = r#"{"server":null,"identity":{"agent":"codex","kind":"id","value":"legacy-session"},"pane":{"pane_id":"p1","workspace_id":"w1","tab_id":"t1"}}"#;

fn legacy_project() -> TempDir {
    let d = TempDir::new().unwrap();
    let conn = Connection::open(d.path().join("qqq.db")).unwrap();
    conn.execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v2.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v3.sql"))
        .unwrap();
    conn.execute_batch(
        "INSERT INTO tasks(title,status,assignee) VALUES ('Parent','in_progress','legacy');
         INSERT INTO tasks(title,parent_id) VALUES ('Child',1);
         INSERT INTO tasks(id,title) VALUES (99,'Deleted'); DELETE FROM tasks WHERE id=99;
         INSERT INTO messages(task_id,body) VALUES (1,'Note');
         INSERT INTO events(task_id,session,action) VALUES (1,'legacy','claim');
         INSERT INTO events(id,task_id,session,action) VALUES (77,1,'legacy','claim'); DELETE FROM events WHERE id=77;
         INSERT INTO images(task_id,name,media_type,data) VALUES (1,'x.png','image/png',X'010203');",
    ).unwrap();
    conn.execute("INSERT INTO herdr_links VALUES (1, ?)", [LINK])
        .unwrap();
    d
}

#[test]
fn v3_migration_preserves_data_sequences_constraints_and_foreign_keys() {
    let d = legacy_project();
    let conn = Connection::open(d.path().join("qqq.db")).unwrap();
    let before: (String, String) = conn
        .query_row(
            "SELECT created_at,updated_at FROM tasks WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    for _ in 0..2 {
        let detail = ok(&d, &["show", "1"]);
        assert_eq!(detail["task"]["assignee"], "legacy");
        assert_eq!(detail["task"]["created_at"], before.0);
        assert_eq!(detail["task"]["updated_at"], before.1);
        assert_eq!(detail["messages"][0]["body"], "Note");
        assert_eq!(detail["events"][0]["action"], "claim");
        assert_eq!(detail["images"][0]["bytes"], 3);
        assert_eq!(detail["herdr"]["identity"]["value"], "legacy-session");
        assert_eq!(ok(&d, &["show", "2"])["task"]["parent_id"], 1);
    }
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(ok(&d, &["add", "After deleted ID"])["id"], 100);
    fail(&d, "1", "legacy");
    assert_eq!(
        conn.query_row("SELECT MAX(id) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        78
    );
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    assert!(
        !conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
    for sql in [
        "UPDATE tasks SET assignee='bad' WHERE id=1",
        "UPDATE tasks SET status='in_progress',assignee=NULL WHERE id=1",
        "UPDATE tasks SET status='unknown' WHERE id=1",
        "INSERT INTO events(task_id,session,action) VALUES (1,'a','unknown')",
        "INSERT INTO tasks(title,parent_id) VALUES ('Bad',999)",
        "INSERT INTO messages(task_id,body) VALUES (999,'Bad')",
        "DELETE FROM tasks WHERE id=1",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(
        conn.query_row("SELECT data FROM images", [], |r| r.get::<_, Vec<u8>>(0))
            .unwrap(),
        vec![1, 2, 3]
    );
    ok(&d, &["edit", "1", "--set-status", "new"]);
    ok(&d, &["next", "--local", "--session", "a"]);
    assert!(
        conn.execute(
            "INSERT INTO tasks(title,status,assignee) VALUES ('Dup','in_progress','a')",
            []
        )
        .is_err()
    );
}

#[test]
fn concurrent_v3_opens_upgrade_once() {
    let d = legacy_project();
    let children: Vec<_> = (0..6)
        .map(|_| {
            command(&d)
                .args(["list", "--json"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let tasks: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(tasks[0]["assignee"], "legacy");
    }
    fail(&d, "1", "legacy");
}

#[test]
fn invalid_legacy_foreign_key_aborts_migration_atomically() {
    let d = legacy_project();
    let conn = Connection::open(d.path().join("qqq.db")).unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    conn.execute(
        "INSERT INTO messages(task_id,body) VALUES (999,'Orphan')",
        [],
    )
    .unwrap();
    assert!(!run(&d, &["list"]).status.success());
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        conn.query_row("SELECT assignee FROM tasks WHERE id=1", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "legacy"
    );
    assert_eq!(
        conn.query_row(
            "SELECT seq FROM sqlite_sequence WHERE name='tasks'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        99
    );
}

#[test]
fn session_free_retry_cannot_release_claim_created_while_it_waits() {
    let d = project();
    ok(&d, &["add", "Task"]);
    ok(&d, &["next", "--local", "--session", "worker"]);
    fail(&d, "1", "worker");
    let conn = Connection::open(d.path().join("qqq.db")).unwrap();
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut child = command(&d)
        .args(["edit", "1", "--set-status", "new", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Retry reads the committed error snapshot, then waits for this write lock.
    std::thread::sleep(std::time::Duration::from_millis(600));
    assert!(child.try_wait().unwrap().is_none());
    conn.execute_batch(
        "UPDATE tasks SET status='in_progress',assignee='manual' WHERE id=1; COMMIT",
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        !output.status.success(),
        "Stale retry released a fresh claim"
    );
    let detail = ok(&d, &["show", "1"]);
    assert_eq!(detail["task"]["status"], "in_progress");
    assert_eq!(detail["task"]["assignee"], "manual");
    assert_eq!(detail["events"].as_array().unwrap().len(), 2);
}

#[test]
fn failed_image_insert_rolls_back_error_reason_history_and_content() {
    let d = project();
    ok(&d, &["add", "Task"]);
    ok(&d, &["next", "--local", "--session", "worker"]);
    std::fs::write(d.path().join("x.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    let conn = Connection::open(d.path().join("qqq.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_image BEFORE INSERT ON images BEGIN SELECT RAISE(ABORT,'image insertion blocked'); END").unwrap();
    let before = ok(&d, &["show", "1"]);
    let args = [
        "edit",
        "1",
        "--set-status",
        "error",
        "--reason",
        "Failure",
        "--session",
        "worker",
        "--title",
        "Changed",
        "--image",
        "x.png",
    ];
    let output = run(&d, &args);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("image insertion blocked"));
    assert_eq!(ok(&d, &["show", "1"]), before);
    conn.execute_batch("DROP TRIGGER reject_image").unwrap();
    assert_eq!(ok(&d, &args)["status"], "error");
    let detail = ok(&d, &["show", "1"]);
    assert_eq!(detail["images"].as_array().unwrap().len(), 1);
    assert_eq!(detail["messages"][0]["body"], "Failure");
    assert_eq!(detail["events"][1]["action"], "error");
}
