use rusqlite::Connection;
use serde_json::{Value, json};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn command(dir: &TempDir) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.current_dir(dir.path())
        .arg("--json")
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_PANE_ID")
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
    assert!(task.get("assignee").is_none());
    assert!(task.get("claim_key").is_none());
    assert_eq!(task.get("harness_session"), Some(&expected));
    for field in ["harness_name", "orchestrator_name", "orchestrator_session"] {
        assert!(task.get(field).is_some());
    }
}
fn version_two() -> TempDir {
    let dir = TempDir::new().unwrap();
    let conn = Connection::open(dir.path().join("qqq.db")).unwrap();
    conn.execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/migrate_v2.sql"))
        .unwrap();
    conn.execute_batch(
        "INSERT INTO tasks(description,status,owner_session) VALUES ('Parent','in_progress','legacy');
        INSERT INTO tasks(description,parent_id) VALUES ('Child',1);
        INSERT INTO messages(task_id,body) VALUES (1,'Note');
        INSERT INTO events(task_id,session,action) VALUES (1,'legacy','claim');
        INSERT INTO images(task_id,name,media_type,data) VALUES (1,'x.png','image/png',X'010203');
        INSERT INTO herdr_links(task_id,link_json) VALUES (1,'{\"server\":null,\"identity\":{\"agent\":\"codex\",\"kind\":\"id\",\"value\":\"legacy-session\"},\"pane\":{\"pane_id\":\"p1\",\"workspace_id\":\"w1\",\"tab_id\":\"t1\"}}');",
    )
    .unwrap();
    dir
}

#[test]
fn task_responses_use_harness_and_orchestrator_fields() {
    let d = TempDir::new().unwrap();
    ok(&d, &["init"]);
    assignment(&ok(&d, &["add", "Task"]), Value::Null);
    assignment(&ok(&d, &["next", "--session", "a"]), json!("a"));
    assignment(&ok(&d, &["list"])[0], json!("a"));
    assignment(&ok(&d, &["show", "1"])["task"], json!("a"));
    assignment(
        &ok(&d, &["edit", "1", "--description", "Edited"]),
        json!("a"),
    );
    assignment(
        &ok(&d, &["edit", "1", "--set-status", "new", "--session", "a"]),
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
    assert!(
        text.lines()
            .filter_map(|line| line.split_once(':'))
            .any(|(label, value)| label.trim() == "Harness session" && value.trim() == "-")
    );
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
        assignment(&show["task"], json!("legacy-session"));
        assert_eq!(show["task"]["harness_name"], "codex");
        assert_eq!(show["task"]["orchestrator_name"], "herdr");
        assert!(show["task"]["orchestrator_session"].is_null());
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
        5
    );
    assert!(
        conn.query_row("SELECT owner_session FROM tasks", [], |r| r
            .get::<_, Option<String>>(0))
            .is_err()
    );
    assert!(
        conn.execute(
            "INSERT INTO tasks(description,status,claim_key) VALUES ('Dup','in_progress','legacy')",
            []
        )
        .is_err()
    );
    assert!(
        conn.execute("UPDATE tasks SET claim_key=NULL WHERE id=1", [])
            .is_err()
    );
    assert!(
        conn.execute("UPDATE tasks SET claim_key='bad' WHERE id=2", [])
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
        assignment(&tasks[0], json!("legacy-session"));
    }
}

#[test]
fn explicit_overrides_and_public_session_recover_same_claim() {
    let d = TempDir::new().unwrap();
    ok(&d, &["init"]);
    ok(&d, &["add", "Task"]);
    let task = ok(
        &d,
        &[
            "next",
            "--local",
            "--session",
            "key",
            "--harness-name",
            "codex",
            "--harness-session",
            "public",
            "--orchestrator-name",
            "herdr",
            "--orchestrator-session",
            "named",
        ],
    );
    assert_eq!(task["harness_name"], "codex");
    assert_eq!(task["harness_session"], "public");
    assert_eq!(task["orchestrator_name"], "herdr");
    assert_eq!(task["orchestrator_session"], "named");
    assert_eq!(
        ok(&d, &["next", "--local", "--harness-session", "public"])["id"],
        1
    );
    assert!(
        !run(&d, &["complete", "1", "--session", "wrong"])
            .status
            .success()
    );
    assignment(
        &ok(&d, &["complete", "1", "--harness-session", "public"]),
        Value::Null,
    );
    let detail = ok(&d, &["show", "1"]);
    assert_eq!(detail["events"][0]["session"], "key");
    assert_eq!(detail["events"][1]["session"], "key");
}

#[test]
fn ambiguous_public_sessions_need_harness_name() {
    let d = TempDir::new().unwrap();
    ok(&d, &["init"]);
    for description in ["First", "Second", "Third"] {
        ok(&d, &["add", description]);
    }
    for (key, name) in [("a", "codex"), ("b", "claude")] {
        ok(
            &d,
            &[
                "next",
                "--local",
                "--session",
                key,
                "--harness-name",
                name,
                "--harness-session",
                "shared",
            ],
        );
    }
    let out = run(&d, &["next", "--local", "--harness-session", "shared"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Multiple active claims"));
    assert_eq!(ok(&d, &["show", "3"])["task"]["status"], "new");
    assert_eq!(
        ok(
            &d,
            &[
                "next",
                "--local",
                "--harness-session",
                "shared",
                "--harness-name",
                "claude"
            ]
        )["id"],
        2
    );
    assignment(
        &ok(
            &d,
            &[
                "complete",
                "2",
                "--harness-session",
                "shared",
                "--harness-name",
                "claude",
            ],
        ),
        Value::Null,
    );
}

#[test]
fn identity_is_preserved_by_edits_and_cleared_on_release_and_error() {
    let d = TempDir::new().unwrap();
    ok(&d, &["init"]);
    ok(&d, &["add", "Task"]);
    for status in ["new", "error"] {
        let before = ok(
            &d,
            &[
                "next",
                "--local",
                "--session",
                "key",
                "--harness-name",
                "codex",
                "--harness-session",
                "visible",
                "--orchestrator-name",
                "herdr",
                "--orchestrator-session",
                "named",
            ],
        );
        let edit = ok(&d, &["edit", "1", "--description", "Edited"]);
        for field in [
            "harness_name",
            "harness_session",
            "orchestrator_name",
            "orchestrator_session",
        ] {
            assert_eq!(edit[field], before[field]);
        }
        let mut args = vec![
            "edit",
            "1",
            "--set-status",
            status,
            "--harness-session",
            "visible",
        ];
        if status == "error" {
            args.extend(["--reason", "Failure"]);
        }
        let after = ok(&d, &args);
        for field in [
            "harness_name",
            "harness_session",
            "orchestrator_name",
            "orchestrator_session",
        ] {
            assert!(after[field].is_null());
        }
    }
}

#[test]
fn blank_overrides_fail_without_claiming() {
    let d = TempDir::new().unwrap();
    ok(&d, &["init"]);
    ok(&d, &["add", "Task"]);
    for flag in [
        "--harness-name",
        "--harness-session",
        "--orchestrator-name",
        "--orchestrator-session",
    ] {
        assert!(
            !run(&d, &["next", "--local", "--session", "key", flag, " "])
                .status
                .success()
        );
    }
    assert_eq!(ok(&d, &["show", "1"])["task"]["status"], "new");
}

#[test]
fn v4_migration_preserves_error_rows_and_named_dispatched_claims() {
    let d = version_two();
    let conn = Connection::open(d.path().join("qqq.db")).unwrap();
    conn.execute_batch(include_str!("../src/migrate_v3.sql"))
        .unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    conn.execute_batch(include_str!("../src/migrate_v4.sql"))
        .unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    conn.execute(
        "UPDATE herdr_links SET link_json=json_set(link_json,'$.server','named') WHERE task_id=1",
        [],
    )
    .unwrap();
    conn.execute("UPDATE tasks SET status='error' WHERE id=2", [])
        .unwrap();
    let detail = ok(&d, &["show", "1"]);
    assert_eq!(detail["task"]["harness_name"], "codex");
    assert_eq!(detail["task"]["harness_session"], "legacy-session");
    assert_eq!(detail["task"]["orchestrator_name"], "herdr");
    assert_eq!(detail["task"]["orchestrator_session"], "named");
    assert_eq!(
        conn.query_row("SELECT claim_key FROM tasks WHERE id=1", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "legacy"
    );
    assert!(conn.prepare("SELECT assignee FROM tasks").is_err());
    assert_eq!(ok(&d, &["show", "2"])["task"]["status"], "error");
    assignment(&ok(&d, &["show", "2"])["task"], Value::Null);
    assignment(
        &ok(
            &d,
            &["complete", "1", "--harness-session", "legacy-session"],
        ),
        Value::Null,
    );
}

#[test]
fn exact_legacy_key_wins_over_another_claims_public_session() {
    let d = TempDir::new().unwrap();
    ok(&d, &["init"]);
    ok(&d, &["add", "First"]);
    ok(&d, &["add", "Second"]);
    ok(
        &d,
        &[
            "next",
            "--local",
            "--session",
            "a",
            "--harness-session",
            "visible-a",
        ],
    );
    ok(
        &d,
        &[
            "next",
            "--local",
            "--session",
            "b",
            "--harness-session",
            "a",
        ],
    );
    assert_eq!(ok(&d, &["next", "--local", "--session", "a"])["id"], 1);
    assert!(
        !run(&d, &["complete", "2", "--session", "a"])
            .status
            .success()
    );
    ok(&d, &["complete", "2", "--session", "b"]);
}

#[test]
fn stale_public_session_cannot_end_replacement_claim_while_waiting_for_lock() {
    for action in ["complete", "new", "error"] {
        let d = TempDir::new().unwrap();
        ok(&d, &["init"]);
        ok(&d, &["add", "Task"]);
        ok(
            &d,
            &[
                "next",
                "--local",
                "--session",
                "key",
                "--harness-session",
                "visible",
            ],
        );
        let conn = Connection::open(d.path().join("qqq.db")).unwrap();
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let mut args = if action == "complete" {
            vec!["complete", "1"]
        } else {
            vec!["edit", "1", "--set-status", action]
        };
        if action == "error" {
            args.extend(["--reason", "Failure"]);
        }
        args.extend(["--harness-session", "visible"]);
        let mut child = command(&d)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(600));
        assert!(child.try_wait().unwrap().is_none());
        conn.execute_batch(
            "UPDATE tasks SET claim_key='replacement',harness_session='key' WHERE id=1; COMMIT",
        )
        .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            !out.status.success(),
            "Stale {action} ended replacement claim"
        );
        let task = ok(&d, &["show", "1"]);
        assert_eq!(task["task"]["status"], "in_progress");
        assert_eq!(task["task"]["harness_session"], "key");
    }
}
