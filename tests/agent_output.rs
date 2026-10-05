use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;

fn command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command.current_dir(root).env("HOME", root);
    for variable in [
        "QQQ_SESSION",
        "CODEX_THREAD_ID",
        "CODEX_SESSION_ID",
        "HERDR_ENV",
        "HERDR_PANE_ID",
        "HERDR_SOCKET_PATH",
        "EDITOR",
    ] {
        command.env_remove(variable);
    }
    command
}

fn agent(root: &Path) -> Command {
    let mut command = command(root);
    command.env("CODEX_THREAD_ID", "agent-thread");
    command
}

fn success(command: &mut Command) -> Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    output
}

fn json(command: &mut Command) -> Value {
    let output = success(command);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "Expected JSON: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn project() -> TempDir {
    let root = TempDir::new().unwrap();
    success(command(root.path()).arg("init"));
    root
}

fn config(root: &Path, text: &str) {
    let directory = root.join(".config/qqq");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("config.toml"), text).unwrap();
}

fn json_error(command: &mut Command, status: i32, code: &str) {
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(status));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap_or_else(|error| {
        panic!(
            "Expected JSON error: {error}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(error["code"], code);
    assert!(error["details"].is_object());
}

#[test]
fn native_agent_defaults_to_json_through_task_lifecycle() {
    let root = TempDir::new().unwrap();
    let p = root.path();
    assert!(json(agent(p).arg("init"))["database"].is_string());
    assert_eq!(
        json(agent(p).args(["add", "First\nSecond"]))["description"],
        "First\nSecond"
    );
    assert_eq!(json(agent(p).arg("list"))[0]["id"], 1);
    assert_eq!(json(agent(p).args(["show", "1"]))["task"]["id"], 1);
    assert_eq!(
        json(agent(p).args(["next", "--local"]))["status"],
        "in_progress"
    );
    assert_eq!(
        json(agent(p).args(["complete", "1"]))["status"],
        "completed"
    );
    assert!(json(agent(p).args(["next", "--local"])).is_null());
}

#[test]
fn native_session_marker_also_selects_json_without_thread_marker() {
    let root = project();
    let mut cmd = command(root.path());
    cmd.env("CODEX_SESSION_ID", "agent-session");
    assert_eq!(json(cmd.arg("list")), serde_json::json!([]));
}

#[test]
fn native_agent_parser_runtime_and_alias_failures_are_json() {
    let root = project();
    let p = root.path();
    json_error(agent(p).args(["show", "42"]), 1, "TASK_NOT_FOUND");
    json_error(agent(p).args(["show", "wrong"]), 2, "INVALID_ARGUMENT");
    json_error(agent(p).arg("unknown-command"), 2, "INVALID_ARGUMENT");
    config(p, "[alias]\nbad = '!exit 1'\n");
    json_error(agent(p).arg("bad"), 1, "INVALID_ARGUMENT");
}

#[test]
fn human_override_works_before_after_commands_and_aliases() {
    let root = project();
    let p = root.path();
    success(command(p).args(["add", "Task"]));
    config(p, "[alias]\ns = 'show'\nh = '--human show'\n");
    for args in [
        vec!["--human", "show", "1"],
        vec!["show", "1", "--human"],
        vec!["--human", "s", "1"],
        vec!["s", "1", "--human"],
        vec!["h", "1"],
    ] {
        let output = success(agent(p).args(args));
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Task"));
        assert!(serde_json::from_str::<Value>(&text).is_err(), "{text}");
    }
    let output = agent(p).args(["--human", "show", "42"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("error:"));
    let output = agent(p)
        .args(["show", "wrong", "--human"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("error:"));
}

#[test]
fn explicit_json_keeps_working_and_conflicting_modes_have_json_error() {
    let root = project();
    let p = root.path();
    assert_eq!(
        json(command(p).args(["list", "--json"])),
        serde_json::json!([])
    );
    for args in [
        vec!["--human", "--json", "list"],
        vec!["list", "--json", "--human"],
    ] {
        json_error(command(p).args(args), 2, "INVALID_ARGUMENT");
    }
}

#[test]
fn literal_output_flags_do_not_override_agent_default() {
    let root = project();
    let p = root.path();
    assert_eq!(
        json(agent(p).args(["add", "--", "--human"]))["description"],
        "--human"
    );
    assert_eq!(
        json(agent(p).args(["add", "--description=--human"]))["description"],
        "--human"
    );
    assert_eq!(
        json(agent(p).args(["message", "1", "--", "--human"]))["body"],
        "--human"
    );
    json_error(
        agent(p).args(["add", "Task", "--priority", "--human"]),
        2,
        "INVALID_ARGUMENT",
    );
    let output = success(command(p).args(["add", "--", "--json"]));
    assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
}

#[test]
fn no_agent_context_keeps_human_output_even_when_piped_or_session_supplied() {
    let root = project();
    let p = root.path();
    for marker in ["CODEX_THREAD_ID", "CODEX_SESSION_ID"] {
        let output = success(command(p).env(marker, "  ").arg("list"));
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
    }
    let output = success(command(p).env("QQQ_SESSION", "manual-owner").arg("list"));
    assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
    let output = success(command(p).args(["list", "--harness-name", "codex"]));
    assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
}

#[test]
fn agent_lists_keep_all_completed_tasks_despite_human_display_limit() {
    let root = project();
    let p = root.path();
    config(p, "[display]\nmax-completed = 1\n");
    for id in ["1", "2"] {
        json(agent(p).args(["add", "Done"]));
        json(agent(p).args(["next", "--local"]));
        json(agent(p).args(["complete", id]));
    }
    assert_eq!(json(agent(p).arg("list")).as_array().unwrap().len(), 2);
}

#[test]
fn agent_help_and_version_remain_text() {
    let root = TempDir::new().unwrap();
    let help = success(agent(root.path()).arg("--help"));
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage: qqq"));
    let version = success(agent(root.path()).arg("--version"));
    assert_eq!(
        String::from_utf8(version.stdout).unwrap(),
        format!("qqq {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn agent_watch_defaults_to_json_snapshots() {
    let root = project();
    let mut child = agent(root.path())
        .args(["list", "--watch"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap(),
        serde_json::json!([])
    );
}

#[cfg(unix)]
mod herdr {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn caller(root: &Path, pane: Value) -> Command {
        let script = root.join("herdr");
        fs::write(&script, "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$QQQ_TEST_CALLS\"\n[ \"$1 $2\" = 'pane current' ] || exit 1\n/bin/cat \"$QQQ_TEST_RESPONSE\"\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            root.join("pane.json"),
            serde_json::to_vec(&serde_json::json!({"result":{"pane":pane}})).unwrap(),
        )
        .unwrap();
        let mut cmd = command(root);
        cmd.env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", "w1:p1")
            .env("PATH", root)
            .env("QQQ_TEST_RESPONSE", root.join("pane.json"))
            .env("QQQ_TEST_CALLS", root.join("calls"));
        cmd
    }

    #[test]
    fn exact_agent_pane_selects_json_without_native_marker_or_project_db() {
        for metadata in [
            serde_json::json!({"agent":"claude"}),
            serde_json::json!({"agent_session":{"agent":"codex","kind":"id","value":"session"}}),
        ] {
            let root = TempDir::new().unwrap();
            let mut pane =
                serde_json::json!({"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1"});
            pane.as_object_mut()
                .unwrap()
                .extend(metadata.as_object().unwrap().clone());
            assert!(json(caller(root.path(), pane).arg("init"))["database"].is_string());
            assert_eq!(
                fs::read_to_string(root.path().join("calls")).unwrap(),
                "pane current --pane w1:p1\n"
            );
        }
    }

    #[test]
    fn ordinary_panes_and_unavailable_herdr_keep_human_output() {
        let root = project();
        let pane = serde_json::json!({"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1","agent":"  "});
        let output = success(caller(root.path(), pane).arg("list"));
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
        fs::remove_file(root.path().join("herdr")).unwrap();
        let output = success(
            command(root.path())
                .env("HERDR_ENV", "1")
                .env("HERDR_PANE_ID", "w1:p1")
                .env("PATH", root.path())
                .arg("list"),
        );
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
    }

    #[test]
    fn exact_agent_pane_parser_errors_are_json() {
        let root = project();
        let pane = serde_json::json!({"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1","agent":"codex"});
        json_error(
            caller(root.path(), pane).args(["show", "wrong"]),
            2,
            "INVALID_ARGUMENT",
        );
    }

    #[test]
    fn explicit_output_modes_skip_optional_herdr_lookup() {
        let root = project();
        let p = root.path();
        let pane = serde_json::json!({"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1","agent":"codex"});
        json(caller(p, pane.clone()).args(["list", "--json"]));
        let output = success(caller(p, pane).args(["--human", "list"]));
        assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
        assert!(!p.join("calls").exists());
    }
}
