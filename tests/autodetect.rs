#![cfg(unix)]
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir)
        .arg("--json")
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    command
}

fn ok(command: &mut Command) -> Value {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(command(dir.path()).arg("init"));
    ok(command(dir.path()).args(["add", "Auto-detect"]));
    let script = dir.path().join("herdr");
    fs::write(&script, "#!/bin/sh\ncase \"$1 $2\" in\n'pane current'|'agent list') /bin/cat \"$QQQ_TEST_RESPONSE\";;\n*) exit 1;;\nesac\n").unwrap();
    fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

fn pane(dir: &Path) -> Value {
    json!({"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1",
           "agent":"codex","terminal_id":"terminal-1","agent_session":null,"cwd":dir})
}

fn herdr(dir: &Path, response: Value, caller: bool) -> Command {
    fs::write(
        dir.join("response.json"),
        serde_json::to_vec(&response).unwrap(),
    )
    .unwrap();
    let mut command = command(dir);
    command
        .env("PATH", dir)
        .env("QQQ_TEST_RESPONSE", dir.join("response.json"));
    if caller {
        command.env("HERDR_ENV", "1").env("HERDR_PANE_ID", "w1:p1");
    }
    command
}

#[test]
fn terminal_caller_identity_auto_claims_waits_completes_and_finds_moved_pane() {
    let dir = project();
    let p = dir.path();
    let response = json!({"result":{"pane":pane(p)}});
    let task = ok(herdr(p, response.clone(), true).arg("next"));
    assert_eq!(task["assignee"], r#"["codex","terminal","terminal-1"]"#);
    assert_eq!(
        ok(herdr(p, response, true).args(["next", "--wait"]))["id"],
        1
    );
    let detail = ok(command(p).args(["show", "1"]));
    assert_eq!(detail["herdr"]["identity"]["kind"], "terminal");
    assert_eq!(detail["herdr"]["identity"]["value"], "terminal-1");
    let mut moved = pane(p);
    moved["pane_id"] = json!("w2:p9");
    let found = ok(
        herdr(p, json!({"result":{"agents":[moved.clone()]}}), false).args(["herdr", "find", "1"]),
    );
    assert_eq!(found["pane_id"], "w2:p9");
    let done = ok(herdr(p, json!({"result":{"pane":moved}}), true).args(["complete", "1"]));
    assert_eq!(done["status"], "completed");
    assert!(done["assignee"].is_null());
}

#[test]
fn unique_cwd_terminal_identity_auto_claims_outside_herdr_from_subdirectory() {
    let dir = project();
    let p = dir.path();
    let nested = p.join("nested");
    fs::create_dir(&nested).unwrap();
    let task = ok(herdr(p, json!({"result":{"agents":[pane(p)]}}), false)
        .current_dir(&nested)
        .arg("next"));
    assert_eq!(task["id"], 1);
    assert_eq!(task["assignee"], r#"["codex","terminal","terminal-1"]"#);
}

#[test]
fn reported_session_identity_takes_priority_over_terminal_fallback() {
    let dir = project();
    let p = dir.path();
    let mut pane = pane(p);
    pane["agent_session"] = json!({"agent":"codex","kind":"id","value":"hook-session"});
    let task = ok(herdr(p, json!({"result":{"pane":pane}}), true).arg("next"));
    assert_eq!(task["assignee"], r#"["codex","id","hook-session"]"#);
}

#[test]
fn invalid_reported_session_does_not_fall_back_or_claim_task() {
    for field in ["agent", "kind", "value"] {
        let dir = project();
        let p = dir.path();
        let mut pane = pane(p);
        pane["agent_session"] = json!({"agent":"codex","kind":"id","value":"hook-session"});
        pane["agent_session"][field] = json!("");
        let output = herdr(p, json!({"result":{"pane":pane}}), true)
            .arg("next")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(ok(command(p).args(["show", "1"]))["task"]["status"], "new");
    }
}

#[test]
fn late_session_reporting_preserves_active_terminal_claim_then_uses_reported_session() {
    let dir = project();
    let p = dir.path();
    ok(command(p).args(["add", "Second"]));
    let terminal_pane = pane(p);
    let first = ok(herdr(p, json!({"result":{"pane":terminal_pane}}), true).arg("next"));
    let mut reported_pane = pane(p);
    reported_pane["agent_session"] = json!({"agent":"codex","kind":"id","value":"late-hook"});
    let response = json!({"result":{"pane":reported_pane}});
    assert_eq!(ok(herdr(p, response.clone(), true).arg("next")), first);
    assert_eq!(
        ok(herdr(p, response.clone(), true).args(["complete", "1"]))["status"],
        "completed"
    );
    let second = ok(herdr(p, response, true).arg("next"));
    assert_eq!(second["id"], 2);
    assert_eq!(second["assignee"], r#"["codex","id","late-hook"]"#);
}
