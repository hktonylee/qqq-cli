use rusqlite::Connection;
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
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .env("EDITOR", "nonexistent-qqq-editor");
    command
}
fn run(path: &Path, args: &[&str]) -> Output {
    command(path).args(args).output().unwrap()
}
fn parsed(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn ok(path: &Path, args: &[&str]) -> Value {
    parsed(run(path, args))
}
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}
fn claimed() -> TempDir {
    let dir = project();
    ok(dir.path(), &["add", "Task\n\nDetails"]);
    ok(
        dir.path(),
        &[
            "next",
            "--local",
            "--session",
            "owner",
            "--harness-name",
            "codex",
            "--harness-session",
            "display-owner",
            "--orchestrator-name",
            "herdr",
            "--orchestrator-session",
            "server-1",
        ],
    );
    dir
}
fn assert_released(task: &Value) {
    assert_eq!(task["status"], "new");
    for key in [
        "harness_name",
        "harness_session",
        "orchestrator_name",
        "orchestrator_session",
    ] {
        assert_eq!(task.get(key), Some(&Value::Null), "{key} was not cleared");
    }
}

#[test]
fn force_without_session_or_herdr_preserves_details_and_releases_claim() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Task\n\nDetails", "--parent", "1"]);
    ok(p, &["next", "--local", "--session", "parent"]);
    ok(p, &["complete", "1", "--session", "parent"]);
    ok(p, &["message", "2", "Existing note"]);
    std::fs::write(p.join("x.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(p, &["edit", "2", "--image", "x.png"]);
    ok(p, &["next", "--local", "--session", "owner"]);
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    let link = r#"{"server":null,"identity":{"agent":"codex","kind":"id","value":"owner"},"pane":{"pane_id":"p1","workspace_id":"w1","tab_id":"t1"}}"#;
    conn.execute("INSERT INTO herdr_links VALUES (2, ?)", [link])
        .unwrap();
    let before = ok(p, &["show", "2"]);
    let released = parsed(
        command(p)
            .env("PATH", p.join("missing-bin"))
            .env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", "missing-pane")
            .args(["edit", "-1", "--set-status", "new", "--force"])
            .output()
            .unwrap(),
    );
    assert_released(&released);
    assert!(
        conn.query_row(
            "SELECT claim_key IS NULL FROM tasks WHERE id=2",
            [],
            |row| row.get::<_, bool>(0)
        )
        .unwrap()
    );
    for key in ["id", "description", "parent_id", "created_at"] {
        assert_eq!(released[key], before["task"][key]);
    }
    let detail = ok(p, &["show", "2"]);
    for key in ["messages", "images", "herdr"] {
        assert_eq!(detail[key], before[key]);
    }
    assert_eq!(detail["events"][0], before["events"][0]);
    assert_eq!(detail["events"].as_array().unwrap().len(), 2);
    assert_eq!(detail["events"][1]["action"], "release");
    assert_eq!(detail["events"][1]["session"], "manual");
    assert_eq!(ok(p, &["next", "--local", "--session", "next"])["id"], 2);
}

#[test]
fn force_accepts_mismatched_explicit_and_environment_sessions_as_authors() {
    for explicit in [false, true] {
        let d = claimed();
        let p = d.path();
        let mut cmd = command(p);
        cmd.env("PATH", p.join("missing-bin"))
            .env("QQQ_SESSION", "wrong-env")
            .args(["edit", "1", "--set-status", "new", "--force"]);
        if explicit {
            cmd.args(["--session", "wrong-explicit"]);
        }
        let released = parsed(cmd.output().unwrap());
        assert_released(&released);
        assert_eq!(
            ok(p, &["show", "1"])["events"][1]["session"],
            if explicit {
                "wrong-explicit"
            } else {
                "wrong-env"
            }
        );
    }
}

#[test]
fn force_skips_ambiguous_harness_owner_matching_and_records_session_author() {
    let d = project();
    let p = d.path();
    for (description, owner, harness) in [("One", "first", "codex"), ("Two", "second", "claude")] {
        ok(p, &["add", description]);
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                owner,
                "--harness-name",
                harness,
                "--harness-session",
                "shared",
            ],
        );
    }
    let released = parsed(
        command(p)
            .env("PATH", p.join("missing-bin"))
            .args([
                "edit",
                "1",
                "--set-status",
                "new",
                "--force",
                "--harness-session",
                "shared",
            ])
            .output()
            .unwrap(),
    );
    assert_released(&released);
    assert_eq!(ok(p, &["show", "1"])["events"][1]["session"], "shared");
    assert_eq!(ok(p, &["show", "2"])["task"]["status"], "in_progress");
}

#[test]
fn default_release_still_rejects_wrong_owner_atomically() {
    let d = claimed();
    let p = d.path();
    ok(p, &["add", "Parent"]);
    let before = ok(p, &["show", "1"]);
    for explicit in [false, true] {
        let mut cmd = command(p);
        cmd.args([
            "edit",
            "1",
            "--set-status",
            "new",
            "-d",
            "Changed",
            "--set-parent",
            "2",
        ]);
        if explicit {
            cmd.args(["--session", "wrong"]);
        } else {
            cmd.env("QQQ_SESSION", "wrong");
        }
        let output = cmd.output().unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Task 1 is not claimed by session wrong")
        );
        assert_eq!(ok(p, &["show", "1"]), before);
    }
}

#[cfg(unix)]
#[test]
fn force_skips_herdr_lookup_while_default_release_requires_it() {
    use std::os::unix::fs::PermissionsExt;
    let d = claimed();
    let p = d.path();
    let bin = p.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let herdr = bin.join("herdr");
    std::fs::write(
        &herdr,
        "#!/bin/sh\nprintf called > \"$QQQ_TEST_HERDR_LOG\"\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&herdr, std::fs::Permissions::from_mode(0o755)).unwrap();
    let log = p.join("herdr.log");
    let before = ok(p, &["show", "1"]);
    let output = command(p)
        .env("PATH", &bin)
        .env("QQQ_TEST_HERDR_LOG", &log)
        .args(["edit", "1", "--set-status", "new"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(log.exists(), "Default release did not attempt Herdr lookup");
    assert_eq!(ok(p, &["show", "1"]), before);
    std::fs::remove_file(&log).unwrap();
    let released = parsed(
        command(p)
            .env("PATH", &bin)
            .env("QQQ_TEST_HERDR_LOG", &log)
            .args(["edit", "1", "--set-status", "new", "--force"])
            .output()
            .unwrap(),
    );
    assert_eq!(released["status"], "new");
    assert!(!log.exists(), "Forced release attempted Herdr lookup");
}

#[test]
fn force_combined_edits_and_release_history_roll_back_on_image_failure() {
    let d = claimed();
    let p = d.path();
    ok(p, &["add", "New parent"]);
    std::fs::write(p.join("x.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    let conn = Connection::open(p.join("qqq.db")).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_image BEFORE INSERT ON images BEGIN SELECT RAISE(ABORT,'image insertion blocked'); END").unwrap();
    let before = ok(p, &["show", "1"]);
    let args = [
        "edit",
        "1",
        "--set-status",
        "new",
        "--force",
        "-d",
        "Changed",
        "--set-parent",
        "2",
        "--image",
        "x.png",
    ];
    let output = run(p, &args);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("image insertion blocked"));
    assert_eq!(ok(p, &["show", "1"]), before);
    conn.execute_batch("DROP TRIGGER reject_image").unwrap();
    let released = ok(p, &args);
    assert_released(&released);
    assert_eq!(released["description"], "Changed");
    assert_eq!(released["parent_id"], 2);
    let detail = ok(p, &["show", "1"]);
    assert_eq!(detail["images"].as_array().unwrap().len(), 1);
    assert_eq!(detail["events"].as_array().unwrap().len(), 2);
    assert_eq!(detail["events"][1]["action"], "release");
    assert_eq!(ok(p, &["next", "--local", "--session", "next"])["id"], 2);
    assert!(ok(p, &["next", "--local", "--session", "owner"]).is_null());
    ok(p, &["complete", "2", "--session", "next"]);
    assert_eq!(ok(p, &["next", "--local", "--session", "owner"])["id"], 1);
}

#[test]
fn force_requires_literal_new_status_and_rejects_new_or_completed_tasks() {
    let d = claimed();
    let p = d.path();
    let before = ok(p, &["show", "1"]);
    for args in [
        vec!["edit", "1", "--force"],
        vec!["edit", "1", "--force", "-d", "Changed"],
        vec!["edit", "1", "--set-pending", "--force"],
        vec!["edit", "1", "--set-status", "new", "--force", "--edit"],
        vec![
            "edit",
            "1",
            "--set-status",
            "error",
            "--force",
            "--reason",
            "Failure",
        ],
    ] {
        assert!(!run(p, &args).status.success(), "{args:?}");
        assert_eq!(ok(p, &["show", "1"]), before);
    }
    ok(p, &["complete", "1", "--session", "owner"]);
    ok(p, &["add", "New task"]);
    for id in ["1", "2"] {
        let before = ok(p, &["show", id]);
        assert!(
            !run(
                p,
                &[
                    "edit",
                    id,
                    "--set-status",
                    "new",
                    "--force",
                    "-d",
                    "Changed"
                ]
            )
            .status
            .success()
        );
        assert_eq!(ok(p, &["show", id]), before);
    }
}

#[test]
fn error_retries_remain_session_free_with_or_without_force() {
    for force in [false, true] {
        let d = claimed();
        let p = d.path();
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
                "owner",
            ],
        );
        let before = ok(p, &["show", "1"]);
        let mut cmd = command(p);
        cmd.env("PATH", p.join("missing-bin"))
            .args(["edit", "1", "--set-status", "new"]);
        if force {
            cmd.arg("--force");
        }
        assert_eq!(parsed(cmd.output().unwrap())["status"], "new");
        let detail = ok(p, &["show", "1"]);
        assert_eq!(detail["messages"], before["messages"]);
        assert_eq!(detail["events"][2]["action"], "release");
        assert_eq!(detail["events"][2]["session"], "manual");
    }
}
