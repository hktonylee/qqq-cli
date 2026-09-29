use rusqlite::Connection;
use serde_json::Value;
use std::{path::Path, process::Command};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir)
        .env_remove("QQQ_SESSION")
        .env_remove("HERDR_ENV");
    command
}

fn ok(dir: &Path, args: &[&str]) -> Value {
    let output = command(dir).arg("--json").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

fn ids(tasks: &Value) -> Vec<i64> {
    tasks
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_i64().unwrap())
        .collect()
}

#[test]
fn completed_limit_keeps_recent_completions_and_all_unfinished_tasks() {
    let dir = project();
    let p = dir.path();
    for title in ["First", "Second", "Third", "Working", "New"] {
        ok(p, &["add", title]);
    }
    for session in ["a", "b", "c", "d"] {
        ok(p, &["next", "--session", session]);
    }
    for (id, session) in [("3", "c"), ("1", "a"), ("2", "b")] {
        ok(p, &["complete", id, "--session", session]);
    }
    // Editing older completed task must not count as a new completion.
    ok(p, &["edit", "3", "-d", "Edited after completion"]);
    assert_eq!(ids(&ok(p, &["list", "--max-completed", "0"])), [4, 5]);
    assert_eq!(ids(&ok(p, &["list", "--max-completed", "1"])), [2, 4, 5]);
    assert_eq!(ids(&ok(p, &["list", "--max-completed", "2"])), [1, 2, 4, 5]);
    assert_eq!(
        ids(&ok(p, &["list", "--max-completed", "10"])),
        [1, 2, 3, 4, 5]
    );
    assert_eq!(ids(&ok(p, &["list"])), [1, 2, 3, 4, 5]);
    assert_eq!(ok(p, &["show", "3"])["task"]["status"], "completed");
}

#[test]
fn human_tree_keeps_children_when_completed_parent_is_hidden() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Hidden parent"]);
    ok(p, &["add", "Visible child", "--parent", "1"]);
    ok(p, &["add", "Visible grandchild", "--parent", "2"]);
    ok(p, &["next", "--session", "a"]);
    ok(p, &["complete", "1", "--session", "a"]);
    let tasks = ok(p, &["list", "--max-completed", "0"]);
    assert_eq!(ids(&tasks), [2, 3]);
    assert_eq!(tasks[0]["parent_id"], 1);
    let output = command(p)
        .args(["list", "--max-completed", "0"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Visible child"));
    assert!(text.contains("└── Visible grandchild"));
    assert!(!text.contains("Hidden parent"));
    assert_eq!(text.lines().count(), 3);
}

#[test]
fn completed_limit_handles_empty_queue_and_tasks_without_completion_history() {
    let dir = project();
    let p = dir.path();
    assert_eq!(
        ok(p, &["list", "--max-completed", "0"]),
        serde_json::json!([])
    );
    for title in ["Old", "Recent"] {
        ok(p, &["add", title]);
    }
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    conn.execute(
        "UPDATE tasks SET status='completed',updated_at='2026-01-01T00:00:00Z'",
        [],
    )
    .unwrap();
    assert_eq!(ids(&ok(p, &["list", "--max-completed", "1"])), [2]);
    conn.execute(
        "UPDATE tasks SET updated_at='9999-01-01T00:00:00Z' WHERE id=1",
        [],
    )
    .unwrap();
    assert_eq!(ids(&ok(p, &["list", "--max-completed", "1"])), [1]);
    // Actual completion history takes precedence over timestamps on imported tasks.
    conn.execute("UPDATE tasks SET status='new' WHERE id=2", [])
        .unwrap();
    ok(p, &["next", "--session", "a"]);
    ok(p, &["complete", "2", "--session", "a"]);
    assert_eq!(ids(&ok(p, &["list", "--max-completed", "1"])), [2]);
    let output = command(p)
        .args(["list", "--max-completed", "0"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "No tasks yet.\n");
}

#[test]
fn completed_limit_rejects_negative_nonnumeric_missing_and_overflow_values() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Preserve"]);
    for args in [
        vec!["list", "--max-completed=-1"],
        vec!["list", "--max-completed", "bad"],
        vec!["list", "--max-completed", "18446744073709551616"],
        vec!["list", "--max-completed"],
    ] {
        let output = command(p).args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("unexpected argument"));
    }
    assert_eq!(ids(&ok(p, &["list"])), [1]);
}
