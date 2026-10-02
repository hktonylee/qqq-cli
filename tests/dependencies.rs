use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.arg("--json")
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_PANE_ID")
        .env_remove("HERDR_ENV");
    c
}
fn run(dir: &Path, args: &[&str]) -> Output {
    command(dir).args(args).output().unwrap()
}
fn ok(dir: &Path, args: &[&str]) -> Value {
    let out = run(dir, args);
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
fn parent_release_blocks_children_completion_unlocks_chain_in_fifo_order() {
    let d = project();
    let p = d.path();
    assert_eq!(ok(p, &["add", "Parent"])["parent_id"], Value::Null);
    let child = ok(p, &["add", "Child", "--parent", "1"]);
    assert_eq!(child["parent_id"], 1);
    assert_eq!(ok(p, &["show", "2"])["task"], child);
    assert_eq!(ok(p, &["list"])[1]["parent_id"], 1);
    ok(p, &["add", "Grandchild", "--parent", "2"]);
    ok(p, &["add", "Unrelated"]);
    assert_eq!(ok(p, &["next", "--session", "a"])["id"], 1);
    assert_eq!(ok(p, &["next", "--session", "b"])["id"], 4);
    assert!(ok(p, &["next", "--session", "c"]).is_null());
    ok(p, &["edit", "1", "--set-status", "new", "--session", "a"]);
    assert_eq!(ok(p, &["next", "--session", "c"])["id"], 1);
    assert!(ok(p, &["next", "--session", "a"]).is_null());
    ok(p, &["complete", "1", "--session", "c"]);
    let child = ok(p, &["next", "--session", "a"]);
    assert_eq!(child["id"], 2);
    assert_eq!(child["parent_id"], 1);
    assert_eq!(ok(p, &["next", "--session", "a"]), child);
    assert!(ok(p, &["next", "--session", "c"]).is_null());
    assert_eq!(ok(p, &["complete", "2", "--session", "a"])["parent_id"], 1);
    assert_eq!(ok(p, &["next", "--session", "c"])["id"], 3);
    ok(p, &["add", "Later child", "--parent", "1"]);
    assert_eq!(ok(p, &["next", "--session", "a"])["id"], 5);
}

#[test]
fn invalid_parent_is_rejected_without_creating_task_or_opening_editor() {
    let d = project();
    let p = d.path();
    for parent in ["1", "0", "999"] {
        let out = command(p)
            .args(["add", "--parent", parent])
            .env("EDITOR", "nonexistent-editor-qqq")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains(&format!("Task {parent} not found")));
        assert!(out.stdout.is_empty());
    }
    assert_eq!(ok(p, &["list"]), json!([]));
    ok(p, &["add", "Parent"]);
    assert!(!run(p, &["add", "Self", "--parent", "2"]).status.success());
    assert_eq!(ok(p, &["list"]).as_array().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn editor_composition_preserves_parent() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Parent"]);
    std::fs::write(
        p.join("editor.sh"),
        "printf 'Edited child\\n\\nDetails\\n' > \"$1\"\n",
    )
    .unwrap();
    let out = command(p)
        .args(["add", "--parent", "1"])
        .env("EDITOR", "sh ./editor.sh")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let task: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(task["description"], "Edited child\n\nDetails\n");
    assert_eq!(task["parent_id"], 1);
}

#[test]
fn concurrent_sessions_cannot_claim_blocked_children() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["next", "--session", "parent-owner"]);
    for _ in 0..4 {
        ok(p, &["add", "Child", "--parent", "1"]);
    }
    let claim = || {
        let children: Vec<_> = (0..4)
            .map(|i| {
                command(p)
                    .args(["next", "--session", &format!("child-{i}")])
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect();
        children
            .into_iter()
            .map(|child| {
                let out = child.wait_with_output().unwrap();
                assert!(out.status.success());
                serde_json::from_slice::<Value>(&out.stdout).unwrap()
            })
            .collect::<Vec<_>>()
    };
    assert!(claim().iter().all(Value::is_null));
    ok(p, &["complete", "1", "--session", "parent-owner"]);
    let mut ids: Vec<_> = claim()
        .iter()
        .map(|task| task["id"].as_i64().unwrap())
        .collect();
    ids.sort();
    assert_eq!(ids, vec![2, 3, 4, 5]);
}

fn legacy_project() -> TempDir {
    let d = TempDir::new().unwrap();
    std::fs::create_dir(d.path().join(".qqq")).unwrap();
    let conn = Connection::open(d.path().join(".qqq/qqq.db")).unwrap();
    conn.execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    conn.execute_batch(
        "INSERT INTO tasks(description,status,owner_session) VALUES ('Legacy','in_progress','owner');
        INSERT INTO messages(task_id,body) VALUES (1,'Note');
        INSERT INTO images(task_id,name,media_type,data) VALUES (1,'x.png','image/png',X'010203');
        INSERT INTO events(task_id,session,action) VALUES (1,'owner','claim');",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO herdr_links(task_id,link_json) VALUES (1,?)",
        [json!({
            "server": null,
            "identity": {"agent": "codex", "kind": "id", "value": "legacy-session"},
            "pane": {"pane_id": "p1", "workspace_id": "w1", "tab_id": "t1"}
        })
        .to_string()],
    )
    .unwrap();
    d
}

#[test]
fn version_one_migration_preserves_data_and_is_repeatable() {
    let d = legacy_project();
    let p = d.path();
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    let before: (String, String) = conn
        .query_row("SELECT created_at,updated_at FROM tasks", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    for _ in 0..2 {
        let show = ok(p, &["show", "1"]);
        assert_eq!(show["task"]["description"], "Legacy");
        assert_eq!(show["task"]["harness_session"], "legacy-session");
        assert_eq!(show["task"]["created_at"], before.0);
        assert_eq!(show["task"]["updated_at"], before.1);
        assert!(show["task"].as_object().unwrap().contains_key("parent_id"));
        assert_eq!(show["task"]["parent_id"], Value::Null);
        assert_eq!(show["messages"][0]["body"], "Note");
        assert_eq!(show["images"][0]["bytes"], 3);
        assert_eq!(show["events"][0]["action"], "claim");
        assert_eq!(show["herdr"]["identity"]["value"], "legacy-session");
        ok(p, &["init"]);
    }
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 8);
    assert_eq!(
        std::fs::read(p.join(".qqq/images/1/1.png")).unwrap(),
        vec![1, 2, 3]
    );
    assert_eq!(ok(p, &["next", "--session", "owner"])["id"], 1);
    assert_eq!(ok(p, &["add", "Child", "--parent", "1"])["parent_id"], 1);
    assert!(
        conn.execute(
            "INSERT INTO tasks(description,parent_id) VALUES ('Invalid',999)",
            []
        )
        .is_err()
    );
}

#[test]
fn concurrent_legacy_opens_migrate_once() {
    let d = legacy_project();
    let children: Vec<_> = (0..6)
        .map(|_| {
            command(d.path())
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
        assert!(tasks[0].as_object().unwrap().contains_key("parent_id"));
    }
}

#[test]
fn unknown_schema_is_rejected_without_modification() {
    let d = legacy_project();
    let conn = Connection::open(d.path().join(".qqq/qqq.db")).unwrap();
    conn.pragma_update(None, "user_version", 9).unwrap();
    for command in ["init", "list"] {
        let out = run(d.path(), &[command]);
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("Unsupported database schema version 9")
        );
    }
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 9);
    assert!(conn.prepare("SELECT parent_id FROM tasks").is_err());
}
