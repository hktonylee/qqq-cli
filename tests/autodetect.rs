#![cfg(unix)]
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir)
        .arg("--json")
        .env("PATH", dir)
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("HERDR_SOCKET_PATH")
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
    fs::write(
        &script,
        r#"#!/bin/sh
if [ -n "$QQQ_TEST_CALLS" ]; then printf '%s\n' "$*" >> "$QQQ_TEST_CALLS"; fi
[ "$1" = --session ] && shift 2
case "$1 $2" in
'pane current'|'agent list') /bin/cat "$QQQ_TEST_RESPONSE";;
*) exit 1;;
esac
"#,
    )
    .unwrap();
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
        .env("QQQ_TEST_RESPONSE", dir.join("response.json"))
        .env("QQQ_TEST_CALLS", dir.join("calls"));
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
    assert_eq!(task["harness_session"], "terminal-1");
    assert_eq!(task["harness_name"], "codex");
    assert_eq!(task["orchestrator_name"], "herdr");
    assert_eq!(task["orchestrator_session"], "w1:p1");
    assert_eq!(
        ok(herdr(p, response, true).args(["next", "--wait"]))["id"],
        1
    );
    let detail = ok(command(p).args(["show", "1"]));
    assert_eq!(detail["herdr"]["identity"]["kind"], "terminal");
    assert_eq!(detail["herdr"]["identity"]["value"], "terminal-1");
    assert_eq!(detail["herdr"]["server"], "default");
    let listed = ok(command(p).args(["list", "--filter", "orchestrator_session == 'w1:p1'"]));
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0], task);
    let output = Command::new(env!("CARGO_BIN_EXE_qqq"))
        .current_dir(p)
        .env("HOME", p)
        .env("PATH", p)
        .args(["--human", "show", "1"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.lines().any(|line| {
            line.trim_start().starts_with("Orchestrator:")
                && line.trim_end().ends_with("w1:p1 (herdr)")
        }),
        "{text}"
    );
    let mut moved = pane(p);
    moved["pane_id"] = json!("w2:p9");
    let found = ok(
        herdr(p, json!({"result":{"agents":[moved.clone()]}}), false).args(["herdr", "find", "1"]),
    );
    assert_eq!(found["pane_id"], "w2:p9");
    let done = ok(
        herdr(p, json!({"result":{"pane":moved.clone()}}), true).args(["edit", "--complete", "1"]),
    );
    assert_eq!(done["status"], "completed");
    for field in [
        "harness_name",
        "harness_session",
        "orchestrator_name",
        "orchestrator_session",
    ] {
        assert_eq!(done[field], task[field]);
    }
    let completed_detail = ok(command(p).args(["show", "1"]));
    assert_eq!(completed_detail["herdr"], detail["herdr"]);
    moved["pane_id"] = json!("w3:p2");
    let found =
        ok(herdr(p, json!({"result":{"agents":[moved]}}), false).args(["herdr", "find", "1"]));
    assert_eq!(found["pane_id"], "w3:p2");
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
    assert_eq!(task["harness_session"], "terminal-1");
    assert_eq!(task["orchestrator_session"], "w1:p1");
}

#[test]
fn reported_session_identity_takes_priority_over_terminal_fallback() {
    let dir = project();
    let p = dir.path();
    let mut pane = pane(p);
    pane["agent_session"] = json!({"agent":"codex","kind":"id","value":"hook-session"});
    let task = ok(herdr(p, json!({"result":{"pane":pane}}), true).arg("next"));
    assert_eq!(task["harness_session"], "hook-session");
    assert_eq!(task["orchestrator_session"], "w1:p1");
}

#[test]
fn missing_or_blank_terminal_records_pane_without_changing_harness() {
    for terminal in [None, Some(json!(null)), Some(json!("")), Some(json!(" \t"))] {
        let dir = project();
        let p = dir.path();
        let mut caller = pane(p);
        caller["agent_session"] = json!({"agent":"codex","kind":"id","value":"hook-session"});
        if let Some(terminal) = terminal {
            caller["terminal_id"] = terminal;
        } else {
            caller.as_object_mut().unwrap().remove("terminal_id");
        }
        let task = ok(herdr(p, json!({"result":{"pane":caller}}), true).arg("next"));
        assert_eq!(task["harness_session"], "hook-session");
        assert_eq!(task["orchestrator_name"], "herdr");
        assert_eq!(task["orchestrator_session"], "w1:p1");
        let detail = ok(command(p).args(["show", "1"]));
        assert_eq!(detail["herdr"]["identity"]["value"], "hook-session");
        assert_eq!(detail["herdr"]["server"], "default");
    }
}

#[test]
fn failed_server_lookup_still_records_pane_identity() {
    let dir = project();
    let p = dir.path();
    let task = ok(herdr(p, json!({"result":{"pane":pane(p)}}), true)
        .env("HERDR_SOCKET_PATH", "/tmp/missing-server.sock")
        .arg("next"));
    assert_eq!(task["orchestrator_session"], "w1:p1");
    let detail = ok(command(p).args(["show", "1"]));
    assert!(detail["herdr"]["server"].is_null());
}

#[test]
fn existing_assignment_retains_recorded_orchestrator_session() {
    let dir = project();
    let p = dir.path();
    let response = json!({"result":{"pane":pane(p)}});
    let first =
        ok(herdr(p, response.clone(), true).args(["next", "--orchestrator-session", "default"]));
    assert_eq!(ok(herdr(p, response, true).arg("next")), first);
    let completed =
        ok(command(p).args(["edit", "--complete", "1", "--harness-session", "terminal-1"]));
    assert_eq!(completed["orchestrator_session"], "default");
}

#[test]
fn herdr_codex_claims_display_native_session_without_changing_owner() {
    for reported in [false, true] {
        for explicit_display in [false, true] {
            let dir = project();
            let p = dir.path();
            let mut caller = pane(p);
            if reported {
                caller["agent_session"] =
                    json!({"agent":"codex","kind":"id","value":"hook-session"});
            }
            let response = json!({"result":{"pane":caller.clone()}});
            let mut claim = herdr(p, response.clone(), true);
            claim
                .env("CODEX_THREAD_ID", "native-thread")
                .env("CODEX_SESSION_ID", "native-session")
                .args(["next", "--local"]);
            if explicit_display {
                claim.args(["--harness-session", "display"]);
            }
            let task = ok(&mut claim);
            assert_eq!(task["harness_name"], "codex");
            assert_eq!(
                task["harness_session"],
                if explicit_display {
                    "display"
                } else {
                    "native-session"
                }
            );
            assert_eq!(task["orchestrator_name"], "herdr");
            assert_eq!(task["orchestrator_session"], "w1:p1");
            let identity = json!({
                "agent":"codex",
                "kind":if reported { "id" } else { "terminal" },
                "value":if reported { "hook-session" } else { "terminal-1" },
            });
            let detail = ok(command(p).args(["show", "1"]));
            assert_eq!(detail["herdr"]["identity"], identity);
            let owner: Value =
                serde_json::from_str(detail["events"][0]["session"].as_str().unwrap()).unwrap();
            assert_eq!(owner, json!(["codex", identity["kind"], identity["value"]]));
            assert_eq!(
                ok(herdr(p, response.clone(), true)
                    .env("CODEX_SESSION_ID", "native-session")
                    .args(["next", "--wait", "--local"])),
                task
            );
            let found =
                ok(herdr(p, json!({"result":{"agents":[caller]}}), false)
                    .args(["herdr", "find", "1"]));
            assert_eq!(found["pane_id"], "w1:p1");
            assert_eq!(
                ok(herdr(p, response, true)
                    .env("CODEX_SESSION_ID", "native-session")
                    .args(["edit", "--complete", "1"]))["status"],
                "completed"
            );
        }
    }
}

#[test]
fn other_herdr_harness_ignores_codex_session_env() {
    for agent in ["claude", "opencode", "other-harness"] {
        for kind in ["id", "terminal"] {
            let dir = project();
            let p = dir.path();
            let mut caller = pane(p);
            caller["agent"] = json!(agent);
            let display = if kind == "id" {
                caller["agent_session"] =
                    json!({"agent":agent,"kind":"id","value":"reported-session"});
                "reported-session"
            } else {
                "terminal-1"
            };
            let response = json!({"result":{"pane":caller}});
            let task = ok(herdr(p, response.clone(), true)
                .env("CODEX_SESSION_ID", " ")
                .arg("next"));
            assert_eq!(task["harness_name"], agent);
            assert_eq!(task["harness_session"], display);
            assert_eq!(task["orchestrator_name"], "herdr");
            assert_eq!(task["orchestrator_session"], "w1:p1");
            let detail = ok(command(p).args(["show", "1"]));
            assert_eq!(
                detail["herdr"]["identity"],
                json!({"agent":agent,"kind":kind,"value":display})
            );
            assert_eq!(ok(herdr(p, response.clone(), true).arg("next")), task);
            let completed = ok(herdr(p, response, true).args(["edit", "--complete", "1"]));
            assert_eq!(completed["status"], "completed");
            for field in [
                "harness_name",
                "harness_session",
                "orchestrator_name",
                "orchestrator_session",
            ] {
                assert_eq!(completed[field], task[field]);
            }
        }
    }
}

#[test]
fn invalid_codex_session_env_fails_before_native_or_herdr_claim() {
    use std::os::unix::ffi::OsStringExt;
    for value in [
        std::ffi::OsString::from(""),
        std::ffi::OsString::from(" \t"),
        std::ffi::OsString::from_vec(vec![0xff]),
    ] {
        for exact_herdr in [false, true] {
            let dir = project();
            let p = dir.path();
            let mut claim = if exact_herdr {
                herdr(p, json!({"result":{"pane":pane(p)}}), true)
            } else {
                native(p, "valid-thread")
            };
            let output = claim
                .env("CODEX_SESSION_ID", &value)
                .arg("next")
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&output.stderr).contains("CODEX_SESSION_ID"));
            let detail = ok(command(p).args(["show", "1"]));
            assert_eq!(detail["task"]["status"], "new");
            assert_eq!(detail["events"], json!([]));
            assert!(detail["herdr"].is_null());
        }
    }
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
        ok(herdr(p, response.clone(), true).args(["edit", "--complete", "1"]))["status"],
        "completed"
    );
    let second = ok(herdr(p, response, true).arg("next"));
    assert_eq!(second["id"], 2);
    assert_eq!(second["harness_session"], "late-hook");
}

#[test]
fn legacy_dispatch_setting_preserves_current_terminal_claim_after_hook_arrives() {
    let dir = project();
    let p = dir.path();
    ok(command(p).args(["add", "Second"]));
    let first = ok(herdr(p, json!({"result":{"pane":pane(p)}}), true).args(["next", "--local"]));
    fs::create_dir_all(p.join(".config/qqq")).unwrap();
    fs::write(
        p.join(".config/qqq/config.toml"),
        "[herdr]\nnext-to-new-agent = true\n",
    )
    .unwrap();
    let mut reported_pane = pane(p);
    reported_pane["agent_session"] = json!({"agent":"codex","kind":"id","value":"late-hook"});
    let response = json!({"result":{"pane":reported_pane}});
    let next = ok(herdr(p, response.clone(), true).arg("next"));
    assert_eq!(next, first);
    assert_eq!(ok(command(p).args(["show", "2"]))["task"]["status"], "new");
    ok(herdr(p, response.clone(), true).args(["edit", "1", "--complete"]));
    let second = ok(herdr(p, response, true).arg("next"));
    assert_eq!(second["id"], 2);
    assert_eq!(second["harness_session"], "late-hook");
    let calls = fs::read_to_string(p.join("calls")).unwrap();
    assert!(!calls.contains("tab create"), "{calls}");
    assert!(!calls.contains("agent start"), "{calls}");
    assert!(!calls.contains("agent prompt"), "{calls}");
}

#[test]
fn missing_hook_metadata_preserves_active_reported_session_claim() {
    let dir = project();
    let p = dir.path();
    ok(command(p).args(["add", "Second"]));
    let mut reported_pane = pane(p);
    reported_pane["agent_session"] = json!({"agent":"codex","kind":"id","value":"existing-hook"});
    let first = ok(herdr(p, json!({"result":{"pane":reported_pane}}), true).arg("next"));
    let response = json!({"result":{"pane":pane(p)}});
    assert_eq!(ok(herdr(p, response.clone(), true).arg("next")), first);
    assert_eq!(
        ok(herdr(p, response, true).args(["edit", "--complete", "1"]))["status"],
        "completed"
    );
}

#[test]
fn explicit_claim_with_herdr_link_is_not_adopted_as_automatic_owner() {
    let dir = project();
    let p = dir.path();
    ok(command(p).args(["add", "Second"]));
    ok(command(p).args(["next", "--session", "explicit-owner"]));
    let response = json!({"result":{"pane":pane(p)}});
    ok(herdr(p, response.clone(), true).args(["herdr", "link", "1"]));
    let linked = ok(command(p).args(["show", "1"]));
    assert_eq!(linked["task"]["orchestrator_session"], "w1:p1");
    assert_eq!(linked["events"][0]["session"], "explicit-owner");
    let task = ok(herdr(p, response, true).arg("next"));
    assert_eq!(task["id"], 2);
    assert_eq!(task["harness_session"], "terminal-1");
    assert_eq!(
        ok(command(p).args(["show", "1"]))["task"]["harness_session"],
        "terminal-1"
    );
}

#[test]
fn default_link_after_hook_arrives_preserves_active_claim_identity() {
    let dir = project();
    let p = dir.path();
    ok(herdr(p, json!({"result":{"pane":pane(p)}}), true).arg("next"));
    let mut reported_pane = pane(p);
    reported_pane["agent_session"] = json!({"agent":"codex","kind":"id","value":"late-hook"});
    let response = json!({"result":{"pane":reported_pane}});
    let link = ok(herdr(p, response.clone(), true).args(["herdr", "link", "1"]));
    assert_eq!(link["identity"]["kind"], "terminal");
    assert_eq!(
        ok(herdr(p, response, true).args(["edit", "--complete", "1"]))["status"],
        "completed"
    );
}

#[test]
fn named_server_auto_fill_and_metadata_overrides_keep_real_link() {
    let dir = project();
    let p = dir.path();
    fs::write(p.join("herdr"), "#!/bin/sh\n[ \"$1\" = --session ] && shift 2\ncase \"$1 $2\" in\n'pane current'|'agent list') /bin/cat \"$QQQ_TEST_RESPONSE\";;\n'session list') /bin/cat \"$QQQ_TEST_SESSIONS\";;\n*) exit 1;;\nesac\n").unwrap();
    fs::write(
        p.join("sessions.json"),
        json!({"sessions":[{"name":"named","socket_path":"/tmp/named.sock"}]}).to_string(),
    )
    .unwrap();
    let response = json!({"result":{"pane":pane(p)}});
    let mut cmd = herdr(p, response.clone(), true);
    cmd.env("HERDR_SOCKET_PATH", "/tmp/named.sock")
        .env("QQQ_TEST_SESSIONS", p.join("sessions.json"));
    let task = ok(cmd.arg("next"));
    assert_eq!(task["orchestrator_session"], "w1:p1");
    assert_eq!(
        ok(command(p).args(["show", "1"]))["herdr"]["server"],
        "named"
    );
    let mut cmd = herdr(p, response, true);
    cmd.env("HERDR_SOCKET_PATH", "/tmp/named.sock")
        .env("QQQ_TEST_SESSIONS", p.join("sessions.json"));
    let changed = ok(cmd.args([
        "next",
        "--harness-name",
        "custom",
        "--orchestrator-name",
        "custom-orch",
        "--orchestrator-session",
        "override",
    ]));
    assert_eq!(changed["harness_name"], "custom");
    assert_eq!(changed["orchestrator_name"], "custom-orch");
    assert_eq!(changed["orchestrator_session"], "override");
    let detail = ok(command(p).args(["show", "1"]));
    assert_eq!(detail["herdr"]["identity"]["agent"], "codex");
    assert_eq!(detail["herdr"]["server"], "named");
    ok(command(p).args(["edit", "--complete", "1", "--harness-session", "terminal-1"]));
}

#[test]
fn harness_session_override_retains_other_automatic_fields() {
    let dir = project();
    let p = dir.path();
    let task = ok(herdr(p, json!({"result":{"pane":pane(p)}}), true).args([
        "next",
        "--harness-session",
        "override",
    ]));
    assert_eq!(task["harness_name"], "codex");
    assert_eq!(task["harness_session"], "override");
    assert_eq!(task["orchestrator_name"], "herdr");
    assert_eq!(task["orchestrator_session"], "w1:p1");
    let detail = ok(command(p).args(["show", "1"]));
    assert_eq!(detail["herdr"]["identity"]["value"], "terminal-1");
    ok(command(p).args(["edit", "--complete", "1", "--harness-session", "override"]));
}

#[test]
fn stored_default_server_link_ignores_callers_named_socket() {
    let dir = project();
    let p = dir.path();
    ok(herdr(p, json!({"result":{"pane":pane(p)}}), true).arg("next"));
    fs::write(p.join("herdr"), "#!/bin/sh\nif [ \"$1\" = --session ] && [ \"$2\" = default ]; then shift 2; else exit 1; fi\n/bin/cat \"$QQQ_TEST_RESPONSE\"\n").unwrap();
    let found = ok(herdr(p, json!({"result":{"agents":[pane(p)]}}), false)
        .env("HERDR_SOCKET_PATH", "/tmp/another-server.sock")
        .args(["herdr", "find", "1"]));
    assert_eq!(found["pane_id"], "w1:p1");
}

fn native(dir: &Path, value: &str) -> Command {
    let mut cmd = command(dir);
    cmd.env("PATH", dir)
        .env("QQQ_TEST_CALLS", dir.join("native-calls"))
        .env("CODEX_THREAD_ID", value);
    cmd
}

#[test]
fn native_codex_thread_claims_waits_releases_and_completes_without_herdr() {
    let dir = project();
    let p = dir.path();
    let first = ok(native(p, "native-session").arg("next"));
    assert_eq!(first["harness_name"], "codex");
    assert_eq!(first["harness_session"], "native-session");
    assert!(first["orchestrator_name"].is_null());
    assert!(first["orchestrator_session"].is_null());
    assert!(ok(command(p).args(["show", "1"]))["herdr"].is_null());
    assert_eq!(
        ok(native(p, "native-session").args(["next", "--wait"])),
        first
    );
    let wrong = native(p, "other-session")
        .args(["edit", "--complete", "1"])
        .output()
        .unwrap();
    assert_eq!(wrong.status.code(), Some(1));
    assert_eq!(ok(command(p).args(["show", "1"]))["task"], first);
    assert_eq!(
        ok(native(p, "native-session").args(["edit", "1", "--set-pending"]))["status"],
        "new"
    );
    let claimed = ok(native(p, "native-session").arg("next"));
    assert_eq!(claimed["harness_name"], "codex");
    assert_eq!(
        ok(native(p, "native-session").args(["edit", "--complete", "1"]))["status"],
        "completed"
    );
    assert!(
        !p.join("native-calls").exists(),
        "Native resolution invoked Herdr"
    );
    let db = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    let owner: String = db
        .query_row(
            "SELECT session FROM events WHERE action='claim' ORDER BY rowid LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&owner).unwrap(),
        json!(["codex", "id", "native-session"])
    );
}

#[test]
fn native_codex_keeps_thread_owner_and_displays_session_env() {
    let dir = project();
    let p = dir.path();
    let task = ok(native(p, "preferred")
        .env("CLAUDE_CODE_SESSION_ID", "other-harness")
        .env("CODEX_SESSION_ID", "legacy")
        .arg("next"));
    assert_eq!(task["harness_session"], "legacy");
    assert_eq!(
        ok(command(p)
            .env("PATH", p)
            .env("CODEX_SESSION_ID", "preferred")
            .args(["next", "--wait"])),
        task
    );
    ok(command(p)
        .env("PATH", p)
        .env("CODEX_SESSION_ID", "preferred")
        .args(["edit", "--complete", "1"]));
    ok(command(p).args(["add", "Legacy"]));
    let legacy = ok(command(p)
        .env("PATH", p)
        .env("CODEX_SESSION_ID", "legacy")
        .arg("next"));
    assert_eq!(legacy["harness_name"], "codex");
    assert_eq!(legacy["harness_session"], "legacy");
    ok(command(p)
        .env("PATH", p)
        .env("CODEX_SESSION_ID", "legacy")
        .args(["edit", "2", "--set-status", "error", "--reason", "Blocked"]));
    assert_eq!(
        ok(command(p).args(["show", "2"]))["task"]["status"],
        "error"
    );
}

#[test]
fn native_claude_claims_waits_releases_and_completes_without_herdr() {
    let dir = project();
    let p = dir.path();
    let claude = || {
        let mut cmd = command(p);
        cmd.env("CLAUDE_CODE_SESSION_ID", "claude-session")
            .env("QQQ_TEST_CALLS", p.join("native-calls"));
        cmd
    };
    let first = ok(claude().arg("next"));
    assert_eq!(first["harness_name"], "claude");
    assert_eq!(first["harness_session"], "claude-session");
    assert!(first["orchestrator_name"].is_null());
    assert!(first["orchestrator_session"].is_null());
    let detail = ok(command(p).args(["show", "1"]));
    assert!(detail["herdr"].is_null());
    assert_eq!(
        serde_json::from_str::<Value>(detail["events"][0]["session"].as_str().unwrap()).unwrap(),
        json!(["claude", "id", "claude-session"])
    );
    assert_eq!(ok(claude().args(["next", "--wait"])), first);
    let wrong = claude()
        .env("CLAUDE_CODE_SESSION_ID", "other-session")
        .args(["edit", "1", "--complete"])
        .output()
        .unwrap();
    assert_eq!(wrong.status.code(), Some(1));
    assert_eq!(ok(command(p).args(["show", "1"]))["task"], first);
    assert_eq!(
        ok(claude().args(["edit", "1", "--set-pending"]))["status"],
        "new"
    );
    assert_eq!(ok(claude().arg("next"))["harness_name"], "claude");
    assert_eq!(
        ok(claude().args(["edit", "1", "--complete"]))["status"],
        "completed"
    );
    assert!(!p.join("native-calls").exists());
}

#[test]
fn herdr_claude_displays_native_session_and_keeps_pane_identity() {
    for reported in [false, true] {
        let dir = project();
        let p = dir.path();
        let mut caller = pane(p);
        caller["agent"] = json!("claude");
        if reported {
            caller["agent_session"] = json!({"agent":"claude","kind":"id","value":"hook-session"});
        }
        let response = json!({"result":{"pane":caller}});
        let mut claim = herdr(p, response.clone(), true);
        let task = ok(claim
            .env("CLAUDE_CODE_SESSION_ID", "claude-session")
            .env("CODEX_SESSION_ID", " ")
            .arg("next"));
        assert_eq!(task["harness_name"], "claude");
        assert_eq!(task["harness_session"], "claude-session");
        assert_eq!(task["orchestrator_name"], "herdr");
        assert_eq!(task["orchestrator_session"], "w1:p1");
        let detail = ok(command(p).args(["show", "1"]));
        assert_eq!(detail["herdr"]["identity"]["agent"], "claude");
        assert_eq!(
            detail["herdr"]["identity"]["value"],
            if reported {
                "hook-session"
            } else {
                "terminal-1"
            }
        );
        let repeated = ok(herdr(p, response.clone(), true)
            .env("CLAUDE_CODE_SESSION_ID", "claude-session")
            .args(["next", "--harness-session", "public-session"]));
        assert_eq!(repeated["harness_session"], "public-session");
        assert_eq!(repeated["orchestrator_session"], "w1:p1");
        assert_eq!(
            ok(herdr(p, response, true)
                .env("CLAUDE_CODE_SESSION_ID", "claude-session")
                .args(["edit", "1", "--complete"]))["status"],
            "completed"
        );
    }
}

#[test]
fn invalid_claude_session_fails_before_native_or_herdr_claim() {
    use std::os::unix::ffi::OsStringExt;
    for value in [
        std::ffi::OsString::from(""),
        std::ffi::OsString::from(" \t"),
        std::ffi::OsString::from_vec(vec![0xff]),
    ] {
        for exact_herdr in [false, true] {
            let dir = project();
            let p = dir.path();
            let mut caller = pane(p);
            caller["agent"] = json!("claude");
            let mut cmd = if exact_herdr {
                herdr(p, json!({"result":{"pane":caller}}), true)
            } else {
                command(p)
            };
            let output = cmd
                .env("CLAUDE_CODE_SESSION_ID", &value)
                .arg("next")
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(1));
            assert!(String::from_utf8_lossy(&output.stderr).contains("CLAUDE_CODE_SESSION_ID"));
            let detail = ok(command(p).args(["show", "1"]));
            assert_eq!(detail["task"]["status"], "new");
            assert_eq!(detail["events"], json!([]));
            assert!(detail["herdr"].is_null());
        }
    }
}

#[test]
fn other_herdr_harness_ignores_claude_session_env() {
    for agent in ["codex", "opencode"] {
        let dir = project();
        let p = dir.path();
        let mut caller = pane(p);
        caller["agent"] = json!(agent);
        let task = ok(herdr(p, json!({"result":{"pane":caller}}), true)
            .env("CLAUDE_CODE_SESSION_ID", " ")
            .arg("next"));
        assert_eq!(task["harness_name"], agent);
        assert_eq!(task["harness_session"], "terminal-1");
        assert_eq!(task["orchestrator_session"], "w1:p1");
    }
}

#[test]
fn native_claim_gains_missing_herdr_metadata_without_replacing_assignment() {
    for (overrides, expected_name, expected_session) in [
        (vec![], "herdr", "w1:p1"),
        (
            vec!["--orchestrator-name", "custom-orchestrator"],
            "custom-orchestrator",
            "w1:p1",
        ),
        (
            vec!["--orchestrator-session", "assigned-pane"],
            "herdr",
            "assigned-pane",
        ),
    ] {
        let dir = project();
        let p = dir.path();
        let mut claim = native(p, "continuing-thread");
        claim
            .env("CODEX_SESSION_ID", "original-display")
            .arg("next")
            .args(overrides);
        let first = ok(&mut claim);
        let before = ok(command(p).args(["show", "1"]));
        let mut caller = pane(p);
        caller["agent_session"] = json!({"agent":"codex","kind":"id","value":"continuing-thread"});
        let response = json!({"result":{"pane":caller}});
        let repeated = ok(herdr(p, response.clone(), true)
            .env("CODEX_THREAD_ID", "continuing-thread")
            .env("CODEX_SESSION_ID", "later-display")
            .arg("next"));
        let mut expected = first;
        expected["orchestrator_name"] = json!(expected_name);
        expected["orchestrator_session"] = json!(expected_session);
        assert_eq!(repeated, expected);
        let after = ok(command(p).args(["show", "1"]));
        assert_eq!(after["events"], before["events"]);
        assert_eq!(after["herdr"]["pane"]["pane_id"], "w1:p1");
        assert_eq!(
            ok(herdr(p, response.clone(), true)
                .env("CODEX_SESSION_ID", "later-display")
                .args(["next", "--wait"])),
            repeated
        );
        assert_eq!(
            ok(herdr(p, response, true).args(["edit", "1", "--complete"]))["status"],
            "completed"
        );
    }
}

#[test]
fn native_codex_session_env_supports_public_completion_without_granting_wrong_owner() {
    let dir = project();
    let p = dir.path();
    let task = ok(native(p, "native-thread")
        .env("CODEX_SESSION_ID", "native-session")
        .arg("next"));
    assert_eq!(task["harness_session"], "native-session");
    let wrong = native(p, "wrong-thread")
        .env("CODEX_SESSION_ID", "native-session")
        .args(["edit", "--complete", "1"])
        .output()
        .unwrap();
    assert_eq!(wrong.status.code(), Some(1));
    assert_eq!(ok(command(p).args(["show", "1"]))["task"], task);
    assert_eq!(
        ok(command(p).args([
            "edit",
            "--complete",
            "1",
            "--harness-session",
            "native-session"
        ]))["status"],
        "completed"
    );
}

#[test]
fn explicit_cli_and_qqq_environment_override_invalid_automatic_context() {
    for cli_override in [true, false] {
        let dir = project();
        let p = dir.path();
        let mut cmd = native(p, " ");
        cmd.env("HERDR_PANE_ID", "")
            .env("CLAUDE_CODE_SESSION_ID", " ")
            .env("QQQ_SESSION", "env-owner")
            .arg("next");
        if cli_override {
            cmd.args(["--session", "cli-owner"]);
        }
        let task = ok(&mut cmd);
        assert_eq!(
            task["harness_session"],
            if cli_override {
                "cli-owner"
            } else {
                "env-owner"
            }
        );
        assert!(task["harness_name"].is_null());
        assert!(!p.join("native-calls").exists());
    }
}

#[test]
fn native_claim_metadata_overrides_persist_and_raw_session_recovers_owner() {
    let dir = project();
    let p = dir.path();
    let first = ok(native(p, "native").args([
        "next",
        "--harness-name",
        "custom",
        "--harness-session",
        "display",
        "--orchestrator-name",
        "custom-orch",
        "--orchestrator-session",
        "named",
    ]));
    assert_eq!(first["harness_name"], "custom");
    assert_eq!(first["harness_session"], "display");
    assert_eq!(first["orchestrator_name"], "custom-orch");
    assert_eq!(ok(native(p, "native").arg("next")), first);
    ok(command(p).env("PATH", p).args([
        "edit",
        "--complete",
        "1",
        "--harness-name",
        "custom",
        "--harness-session",
        "display",
    ]));
    ok(command(p).args(["add", "Another"]));
    let task = ok(native(p, "native").arg("next"));
    assert_eq!(task["harness_name"], "codex");
    assert_eq!(task["harness_session"], "native");
    ok(command(p)
        .env("PATH", p)
        .args(["edit", "--complete", "2", "--harness-session", "native"]));
}

#[test]
fn invalid_native_values_fail_before_claim_instead_of_falling_back() {
    use std::os::unix::ffi::OsStringExt;
    for value in [
        std::ffi::OsString::from(""),
        std::ffi::OsString::from(" \t"),
        std::ffi::OsString::from_vec(vec![0xff]),
    ] {
        let dir = project();
        let p = dir.path();
        let mut cmd = native(p, "unused");
        cmd.env("CODEX_THREAD_ID", value)
            .env("CODEX_SESSION_ID", "valid-fallback")
            .arg("next");
        let output = cmd.output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("CODEX_THREAD_ID"));
        assert_eq!(ok(command(p).args(["show", "1"]))["task"]["status"], "new");
        assert!(!p.join("native-calls").exists());
    }
}

#[test]
fn exact_herdr_pane_without_marker_takes_priority_over_codex_and_is_targeted() {
    let dir = project();
    let p = dir.path();
    let response = json!({"result":{"pane":pane(p)}});
    let task = ok(herdr(p, response.clone(), false)
        .env("HERDR_PANE_ID", "w1:p1")
        .env("CODEX_THREAD_ID", "native")
        .arg("next"));
    assert_eq!(task["harness_session"], "terminal-1");
    assert_eq!(task["orchestrator_name"], "herdr");
    let calls = fs::read_to_string(p.join("calls")).unwrap();
    assert!(calls.contains("pane current --pane w1:p1"), "{calls}");
    assert!(!calls.contains("--current"), "{calls}");
    ok(herdr(p, response, false)
        .env("HERDR_PANE_ID", "w1:p1")
        .args(["edit", "--complete", "1"]));
}

#[test]
fn invalid_exact_herdr_context_never_falls_back_to_native_session() {
    for blank_pane in [true, false] {
        let dir = project();
        let p = dir.path();
        let mut cmd = native(p, "native");
        if blank_pane {
            cmd.env("HERDR_PANE_ID", " ");
        } else {
            cmd.env("HERDR_ENV", "1");
        }
        let output = cmd.arg("next").output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("HERDR_PANE_ID"));
        assert_eq!(ok(command(p).args(["show", "1"]))["task"]["status"], "new");
        assert!(!p.join("native-calls").exists());
    }
}

#[test]
fn legacy_dispatch_setting_claims_for_each_native_caller_without_herdr() {
    let dir = project();
    let p = dir.path();
    let first = ok(native(p, "native").args(["next", "--local"]));
    ok(command(p).args(["add", "Second"]));
    fs::create_dir_all(p.join(".config/qqq")).unwrap();
    fs::write(
        p.join(".config/qqq/config.toml"),
        "[herdr]\nnext-to-new-agent=true\n",
    )
    .unwrap();
    assert_eq!(ok(native(p, "native").args(["next", "--wait"])), first);
    assert!(!p.join("native-calls").exists());
    let second = ok(native(p, "different").arg("next"));
    assert_eq!(second["id"], 2);
    assert_eq!(second["harness_session"], "different");
    assert!(!p.join("native-calls").exists());
}

#[test]
fn manual_herdr_link_discovers_herdr_even_with_native_codex_environment() {
    let dir = project();
    let p = dir.path();
    ok(native(p, "native").arg("next"));
    let link = ok(herdr(p, json!({"result":{"agents":[pane(p)]}}), false)
        .env("CODEX_THREAD_ID", "native")
        .args(["herdr", "link", "1"]));
    assert_eq!(link["identity"]["value"], "terminal-1");
    assert!(
        fs::read_to_string(p.join("calls"))
            .unwrap()
            .contains("agent list")
    );
    ok(native(p, "native").args(["edit", "--complete", "1"]));
}

#[test]
fn legacy_dispatch_setting_allows_reclaim_after_native_claim_is_released() {
    use std::process::Stdio;
    let dir = project();
    let p = dir.path();
    ok(native(p, "native").args(["next", "--local"]));
    fs::create_dir_all(p.join(".config/qqq")).unwrap();
    fs::write(
        p.join(".config/qqq/config.toml"),
        "[herdr]\nnext-to-new-agent=true\n",
    )
    .unwrap();
    let db = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut child = native(p, "native")
        .arg("next")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(600));
    assert!(child.try_wait().unwrap().is_none());
    db.execute_batch("UPDATE tasks SET status='new',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL WHERE id=1; COMMIT").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["id"],
        1
    );
    let task = ok(command(p).args(["show", "1"]));
    assert_eq!(task["task"]["status"], "in_progress");
    assert_eq!(task["task"]["harness_session"], "native");
    assert!(!p.join("native-calls").exists());
}

#[test]
fn explicit_harness_session_keeps_local_fallback_when_auto_fill_fails() {
    let dir = project();
    let p = dir.path();
    let task = ok(native(p, " ").env("HERDR_PANE_ID", " ").args([
        "next",
        "--local",
        "--harness-session",
        "explicit",
    ]));
    assert_eq!(task["harness_session"], "explicit");
    assert!(task["harness_name"].is_null());
    ok(native(p, " ").env("HERDR_PANE_ID", " ").args([
        "edit",
        "--complete",
        "1",
        "--harness-session",
        "explicit",
    ]));
    assert!(!p.join("native-calls").exists());
}
