use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qqq"))
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .args(args)
        .output()
        .unwrap()
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    assert!(run(dir.path(), &["init"]).status.success());
    dir
}

fn assert_empty(output: Output, json: bool) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        if json {
            b"null\n".as_slice()
        } else {
            b"No task available for pickup.\n".as_slice()
        }
    );
}

#[test]
fn empty_local_next_exits_one_in_json_and_human_modes() {
    let dir = project();
    for mode in ["--json", "--human"] {
        assert_empty(
            run(
                dir.path(),
                &[mode, "next", "--local", "--session", "worker"],
            ),
            mode == "--json",
        );
    }
}

#[test]
fn empty_preview_exits_one_without_resolving_a_session() {
    let dir = project();
    for mode in ["--json", "--human"] {
        assert_empty(
            run(dir.path(), &[mode, "next", "--dry-run"]),
            mode == "--json",
        );
    }
}

#[test]
fn filtered_empty_next_exits_one_without_claiming_other_work() {
    let dir = project();
    assert!(run(dir.path(), &["add", "Task"]).status.success());
    for preview in [false, true] {
        let mut args = vec![
            "--json",
            "next",
            "--local",
            "--session",
            "worker",
            "--filter",
            "false",
        ];
        if preview {
            args.push("--dry-run");
        }
        assert_empty(run(dir.path(), &args), true);
    }
    let shown = run(dir.path(), &["--json", "show", "1"]);
    assert!(shown.status.success());
    let detail: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(detail["task"]["status"], "new");
    assert!(detail["events"].as_array().unwrap().is_empty());
}

#[test]
fn successful_pickup_and_owned_reuse_exit_zero() {
    let dir = project();
    assert!(run(dir.path(), &["add", "Task"]).status.success());
    for filtered in [false, true] {
        let mut args = vec!["--json", "next", "--local", "--session", "worker"];
        if filtered {
            args.extend(["--filter", "false", "--wait"]);
        }
        let output = run(dir.path(), &args);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let task: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(task["id"], 1);
        assert_eq!(task["status"], "in_progress");
    }
}

#[test]
fn empty_explanation_remains_successful_diagnostic() {
    let dir = project();
    let output = run(dir.path(), &["--json", "next", "--explain"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["explanation"]["selection"].is_null());
}
