use rusqlite::Connection;
use serde_json::Value;
use std::process::{Command, Output};
use tempfile::TempDir;

fn run(dir: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qqq"))
        .current_dir(dir.path())
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .env("PATH", dir.path())
        .arg("--json")
        .args(args)
        .output()
        .unwrap()
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
    ok(&dir, &["add", "Needs decision"]);
    ok(
        &dir,
        &[
            "next",
            "--session",
            "owner",
            "--harness-name",
            "codex",
            "--harness-session",
            "worker-log",
            "--orchestrator-name",
            "herdr",
            "--orchestrator-session",
            "w1:p1",
        ],
    );
    dir
}

fn block(dir: &TempDir) -> Value {
    ok(
        dir,
        &[
            "edit",
            "1",
            "--set-status",
            "blocked",
            "--reason",
            "Which API should I use?",
            "--session",
            "owner",
        ],
    )
}

#[test]
fn blocked_retains_identity_reason_and_dependencies_without_an_active_claim() {
    let dir = project();
    ok(&dir, &["add", "Child", "--parent", "1"]);
    let before = ok(&dir, &["show", "1"]);
    let blocked = block(&dir);
    assert_eq!(blocked["status"], "blocked");
    for field in [
        "description",
        "content_revision",
        "harness_name",
        "harness_session",
        "orchestrator_name",
        "orchestrator_session",
    ] {
        assert_eq!(blocked[field], before["task"][field]);
    }
    let detail = ok(&dir, &["show", "1"]);
    assert_eq!(detail["events"][1]["action"], "blocked");
    assert_eq!(detail["events"][1]["session"], "owner");
    assert_eq!(detail["messages"][0]["body"], "Which API should I use?");
    let conn = Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
    assert!(
        conn.query_row(
            "SELECT claim_key IS NULL FROM tasks WHERE id=1",
            [],
            |row| row.get::<_, bool>(0)
        )
        .unwrap()
    );
    assert_eq!(
        run(&dir, &["next", "--session", "owner"]).status.code(),
        Some(1)
    );
    let report = ok(&dir, &["status"]);
    assert_eq!(report["state"], "blocked");
    assert_eq!(report["counts"]["blocked"], 2);
    assert_eq!(report["tasks"][0]["reasons"][0], "blocked");
    assert!(report["tasks"][0]["owner"].is_null());
    assert_eq!(
        ok(&dir, &["list", "--status", "blocked"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ok(&dir, &["list", "--filter", "status == 'blocked'"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(&dir, &["add", "Independent"]);
    assert_eq!(ok(&dir, &["next", "--session", "owner"])["id"], 3);
}

#[test]
fn blocked_requires_owner_and_reason_and_rolls_back_late_failures() {
    let dir = project();
    let before = ok(&dir, &["show", "1"]);
    for args in [
        vec!["edit", "1", "--set-status", "blocked", "--session", "owner"],
        vec![
            "edit",
            "1",
            "--set-status",
            "blocked",
            "--session",
            "owner",
            "--reason",
            " ",
        ],
        vec![
            "edit",
            "1",
            "--set-status",
            "blocked",
            "--session",
            "wrong",
            "--reason",
            "Need help",
            "-d",
            "Changed",
        ],
        vec![
            "edit",
            "1",
            "--set-status",
            "blocked",
            "--force",
            "--reason",
            "Need help",
        ],
    ] {
        assert!(!run(&dir, &args).status.success());
        assert_eq!(ok(&dir, &["show", "1"]), before);
    }
    let conn = Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_block BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT,'no message'); END;").unwrap();
    assert!(
        !run(
            &dir,
            &[
                "edit",
                "1",
                "--set-status",
                "blocked",
                "--session",
                "owner",
                "--reason",
                "Need help",
                "-d",
                "Changed"
            ]
        )
        .status
        .success()
    );
    assert_eq!(ok(&dir, &["show", "1"]), before);
}

#[test]
fn blocked_retry_is_session_free_and_a_new_claim_replaces_identity() {
    let dir = project();
    let blocked = block(&dir);
    let retried = ok(&dir, &["edit", "1", "--set-status", "new"]);
    for field in [
        "harness_name",
        "harness_session",
        "orchestrator_name",
        "orchestrator_session",
    ] {
        assert_eq!(retried[field], blocked[field]);
    }
    let claimed = ok(&dir, &["next", "--session", "replacement"]);
    assert_eq!(claimed["harness_session"], "replacement");
    assert!(claimed["harness_name"].is_null());
    assert!(claimed["orchestrator_session"].is_null());
    assert!(
        !run(&dir, &["edit", "1", "--complete", "--session", "owner"])
            .status
            .success()
    );
    ok(
        &dir,
        &["edit", "1", "--complete", "--session", "replacement"],
    );
}

#[test]
fn blocked_snapshots_restore_status_identity_and_reason() {
    let dir = project();
    block(&dir);
    let before = ok(&dir, &["show", "1"]);
    let archive = dir.path().join("blocked.tar");
    ok(&dir, &["backup", archive.to_str().unwrap()]);
    let restored = TempDir::new().unwrap();
    ok(&restored, &["restore", archive.to_str().unwrap()]);
    assert_eq!(ok(&restored, &["show", "1"]), before);
    ok(&restored, &["edit", "1", "--set-pending"]);
    assert_eq!(ok(&restored, &["next", "--session", "another"])["id"], 1);
}
