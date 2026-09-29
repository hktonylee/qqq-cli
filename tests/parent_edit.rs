use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

fn command(path: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(path)
        .arg("--json")
        .env("HOME", path)
        .env_remove("QQQ_SESSION")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .env("EDITOR", "nonexistent-qqq-editor");
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
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

#[test]
fn setting_and_clearing_parent_changes_fresh_claim_eligibility() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Child"]);
    ok(p, &["add", "Parent"]);
    assert_eq!(ok(p, &["edit", "1", "--set-parent", "2"])["parent_id"], 2);
    assert_eq!(
        ok(p, &["next", "--local", "--session", "parent-worker"])["id"],
        2
    );
    assert!(ok(p, &["next", "--local", "--session", "child-worker"]).is_null());
    assert!(ok(p, &["edit", "1", "--set-parent", "none"])["parent_id"].is_null());
    assert_eq!(
        ok(p, &["next", "--local", "--session", "child-worker"])["id"],
        1
    );
    ok(
        p,
        &[
            "edit",
            "1",
            "--set-status",
            "new",
            "--session",
            "child-worker",
            "--set-parent",
            "2",
        ],
    );
    assert!(ok(p, &["next", "--local", "--session", "child-worker"]).is_null());
    ok(p, &["complete", "2", "--session", "parent-worker"]);
    assert_eq!(
        ok(p, &["next", "--local", "--session", "child-worker"])["id"],
        1
    );
}

#[test]
fn changing_parent_preserves_existing_claim_and_task_details() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "First parent"]);
    ok(p, &["add", "Child", "-d", "Details"]);
    ok(p, &["add", "Second parent"]);
    ok(p, &["next", "--local", "--session", "parent"]);
    ok(p, &["next", "--local", "--session", "child"]);
    ok(p, &["message", "2", "Note"]);
    std::fs::write(p.join("x.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(p, &["edit", "2", "--image", "x.png"]);
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    conn.execute("INSERT INTO herdr_links VALUES (2,?)", [json!({"server":null,"identity":{"agent":"codex","kind":"id","value":"child"},"pane":{"pane_id":"p1","workspace_id":"w1","tab_id":"t1"}}).to_string()]).unwrap();
    let before = ok(p, &["show", "2"]);
    for parent in ["1", "3", "none"] {
        let edited = ok(p, &["edit", "2", "--set-parent", parent]);
        assert_eq!(edited["status"], "in_progress");
        assert_eq!(edited["assignee"], "child");
        assert_eq!(edited["title"], "Child");
        assert_eq!(edited["description"], "Details");
        assert_eq!(edited["created_at"], before["task"]["created_at"]);
        let detail = ok(p, &["show", "2"]);
        for key in ["messages", "images", "events", "herdr"] {
            assert_eq!(detail[key], before[key]);
        }
        assert_eq!(ok(p, &["next", "--local", "--session", "child"])["id"], 2);
    }
}

#[test]
fn missing_self_and_descendant_parents_leave_edits_unchanged() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Root"]);
    ok(p, &["add", "Child", "--parent", "1"]);
    ok(p, &["add", "Grandchild", "--parent", "2"]);
    let before = ok(p, &["show", "1"]);
    for parent in ["999", "1", "2", "3", "0", "bad", "9223372036854775808"] {
        let out = run(
            p,
            &[
                "edit",
                "1",
                "--set-parent",
                parent,
                "--title",
                "Changed",
                "-d",
                "Changed",
            ],
        );
        assert!(!out.status.success(), "Parent {parent}");
        assert!(out.stdout.is_empty());
        assert_eq!(ok(p, &["show", "1"]), before);
    }
    assert!(!run(p, &["edit", "1", "--set-parent"]).status.success());
    assert_eq!(ok(p, &["show", "1"]), before);
}

#[test]
fn parent_content_status_and_images_share_atomic_transaction() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Child"]);
    ok(p, &["next", "--local", "--session", "parent"]);
    ok(p, &["next", "--local", "--session", "child"]);
    std::fs::write(p.join("x.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    let before = ok(p, &["show", "2"]);
    assert!(
        !run(
            p,
            &[
                "edit",
                "2",
                "--set-parent",
                "1",
                "--set-status",
                "new",
                "--session",
                "wrong",
                "--title",
                "Changed",
                "--image",
                "x.png"
            ]
        )
        .status
        .success()
    );
    assert_eq!(ok(p, &["show", "2"]), before);
    assert!(
        !run(p, &["edit", "2", "--set-parent", "1", "--title", " "])
            .status
            .success()
    );
    assert_eq!(ok(p, &["show", "2"]), before);
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_image BEFORE INSERT ON images BEGIN SELECT RAISE(ABORT,'image rejected'); END").unwrap();
    let args = [
        "edit",
        "2",
        "--set-parent",
        "1",
        "--set-status",
        "error",
        "--reason",
        "Failure",
        "--session",
        "child",
        "--title",
        "Changed",
        "--image",
        "x.png",
    ];
    assert!(!run(p, &args).status.success());
    assert_eq!(ok(p, &["show", "2"]), before);
    conn.execute_batch("DROP TRIGGER reject_image").unwrap();
    let edited = ok(p, &args);
    assert_eq!(edited["parent_id"], 1);
    assert_eq!(edited["status"], "error");
    assert_eq!(edited["title"], "Changed");
    let detail = ok(p, &["show", "2"]);
    assert_eq!(detail["messages"][0]["body"], "Failure");
    assert_eq!(detail["images"].as_array().unwrap().len(), 1);
    assert_eq!(detail["events"][1]["action"], "error");
    let retried = ok(
        p,
        &["edit", "2", "--set-status", "new", "--set-parent", "none"],
    );
    assert!(retried["parent_id"].is_null());
    assert_eq!(retried["status"], "new");
}

#[test]
fn concurrent_reciprocal_edits_cannot_create_cycle() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "First"]);
    ok(p, &["add", "Second"]);
    let children: Vec<_> = [("1", "2"), ("2", "1")]
        .into_iter()
        .map(|(id, parent)| {
            command(p)
                .args(["edit", id, "--set-parent", parent])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let outputs: Vec<_> = children
        .into_iter()
        .map(|child| child.wait_with_output().unwrap())
        .collect();
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.status.success())
            .count(),
        1
    );
    let rejected = outputs
        .iter()
        .find(|output| !output.status.success())
        .unwrap();
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("cycle"));
    let tasks = ok(p, &["list"]);
    assert_ne!(
        tasks[0]["parent_id"].is_null(),
        tasks[1]["parent_id"].is_null()
    );
}

#[cfg(unix)]
#[test]
fn forced_editor_and_negative_task_reference_support_parent_changes() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Child"]);
    std::fs::write(
        p.join("editor.sh"),
        "printf 'Edited child\\n\\nNew details\\n' > \"$1\"\n",
    )
    .unwrap();
    let output = command(p)
        .args(["edit", "-1", "--edit", "--set-parent", "1"])
        .env("EDITOR", "sh ./editor.sh")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["id"], 2);
    assert_eq!(task["parent_id"], 1);
    assert_eq!(task["title"], "Edited child");
    assert_eq!(task["description"], "New details");
    assert!(ok(p, &["edit", "-1", "--set-parent", "none"])["parent_id"].is_null());
}

#[test]
fn cyclic_parent_ancestry_from_direct_sql_is_rejected_without_hanging() {
    let d = project();
    let p = d.path();
    for title in ["First", "Second", "Target"] {
        ok(p, &["add", title]);
    }
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    conn.execute_batch(
        "UPDATE tasks SET parent_id=2 WHERE id=1; UPDATE tasks SET parent_id=1 WHERE id=2",
    )
    .unwrap();
    let before = ok(p, &["show", "3"]);
    let output = run(p, &["edit", "3", "--set-parent", "1"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cycle"));
    assert_eq!(ok(p, &["show", "3"]), before);
}
