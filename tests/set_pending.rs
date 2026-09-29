use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
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
fn set_pending_releases_claim_and_preserves_content_details_and_history() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Task\n\nDetails", "--parent", "1"]);
    ok(p, &["next", "--local", "--session", "parent"]);
    ok(p, &["complete", "1", "--session", "parent"]);
    ok(p, &["message", "2", "Note"]);
    std::fs::write(p.join("x.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(p, &["edit", "2", "--image", "x.png"]);
    ok(p, &["next", "--local", "--session", "worker"]);
    let before = ok(p, &["show", "2"]);
    let edited = ok(p, &["edit", "-1", "--set-pending", "--session", "worker"]);
    assert_eq!(edited["status"], "new");
    assert!(edited["harness_session"].is_null());
    for key in ["description", "parent_id", "created_at"] {
        assert_eq!(edited[key], before["task"][key]);
    }
    let detail = ok(p, &["show", "2"]);
    for key in ["messages", "images", "herdr"] {
        assert_eq!(detail[key], before[key]);
    }
    assert_eq!(detail["events"][1]["action"], "release");
    assert_eq!(detail["events"][1]["session"], "worker");
    assert_eq!(ok(p, &["next", "--local", "--session", "other"])["id"], 2);
}

#[test]
fn set_pending_checks_owner_and_combined_edits_atomically() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Task"]);
    ok(p, &["next", "--local", "--session", "worker"]);
    let before = ok(p, &["show", "1"]);
    for args in [
        vec![
            "edit",
            "1",
            "--set-pending",
            "--session",
            "wrong",
            "-d",
            "Changed",
        ],
        vec![
            "edit",
            "1",
            "--set-pending",
            "--session",
            "worker",
            "--description",
            " ",
        ],
        vec![
            "edit",
            "1",
            "--set-pending",
            "--set-status",
            "new",
            "--session",
            "worker",
        ],
        vec![
            "edit",
            "1",
            "--set-pending",
            "--set-status",
            "error",
            "--reason",
            "Failure",
            "--session",
            "worker",
        ],
        vec![
            "edit",
            "1",
            "--set-pending",
            "--edit",
            "--session",
            "worker",
        ],
    ] {
        assert!(!run(p, &args).status.success(), "{args:?}");
        assert_eq!(ok(p, &["show", "1"]), before);
    }
    let edited = ok(
        p,
        &[
            "edit",
            "1",
            "--set-pending",
            "--session",
            "worker",
            "-d",
            "Changed",
        ],
    );
    assert_eq!(edited["status"], "new");
    assert_eq!(edited["description"], "Changed");
}

#[test]
fn set_pending_with_new_dependency_waits_for_that_parent() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Old parent"]);
    ok(p, &["add", "Child", "--parent", "1"]);
    ok(p, &["next", "--local", "--session", "parent"]);
    ok(p, &["complete", "1", "--session", "parent"]);
    ok(p, &["next", "--local", "--session", "child"]);
    ok(p, &["add", "New parent"]);
    let edited = ok(
        p,
        &[
            "edit",
            "2",
            "--set-pending",
            "--set-parent",
            "3",
            "--session",
            "child",
        ],
    );
    assert_eq!(edited["parent_id"], 3);
    assert_eq!(edited["status"], "new");
    assert!(edited["harness_session"].is_null());
    assert_eq!(ok(p, &["next", "--local", "--session", "parent"])["id"], 3);
    assert!(ok(p, &["next", "--local", "--session", "child"]).is_null());
    ok(p, &["complete", "3", "--session", "parent"]);
    assert_eq!(ok(p, &["next", "--local", "--session", "child"])["id"], 2);
}

#[test]
fn set_pending_retries_error_without_session_and_rejects_new_or_completed() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Task"]);
    let before = ok(p, &["show", "1"]);
    assert!(
        !run(p, &["edit", "1", "--set-pending", "--session", "worker"])
            .status
            .success()
    );
    assert_eq!(ok(p, &["show", "1"]), before);
    ok(p, &["next", "--local", "--session", "worker"]);
    ok(
        p,
        &[
            "edit",
            "1",
            "--set-status",
            "error",
            "--reason",
            "Failure",
            "--session",
            "worker",
        ],
    );
    let retried = ok(p, &["edit", "1", "--set-pending"]);
    assert_eq!(retried["status"], "new");
    assert_eq!(ok(p, &["show", "1"])["events"][2]["session"], "manual");
    ok(p, &["next", "--local", "--session", "worker"]);
    ok(p, &["complete", "1", "--session", "worker"]);
    let before = ok(p, &["show", "1"]);
    assert!(
        !run(p, &["edit", "1", "--set-pending", "--session", "worker"])
            .status
            .success()
    );
    assert_eq!(ok(p, &["show", "1"]), before);
}
