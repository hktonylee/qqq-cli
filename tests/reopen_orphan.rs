#![cfg(unix)]

use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
    pane: Value,
}

impl Fixture {
    fn new(reported: bool) -> Self {
        let dir = TempDir::new().unwrap();
        let path = dir.path();
        fs::write(
            path.join("herdr"),
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$QQQ_TEST_CALLS\"\n[ \"$1\" = --session ] && shift 2\ncase \"$1 $2\" in\n'pane current'|'agent list') /bin/cat \"$QQQ_TEST_RESPONSE\";;\n*) exit 1;;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(path.join("herdr"), fs::Permissions::from_mode(0o755)).unwrap();
        let mut pane = json!({
            "pane_id":"w1:p1", "workspace_id":"w1", "tab_id":"w1:t1",
            "agent":"codex", "terminal_id":"terminal-1", "cwd":path,
            "agent_session":null
        });
        if reported {
            pane["agent_session"] = json!({"agent":"codex", "kind":"id", "value":"owner-id"});
        }
        let fixture = Self { dir, pane };
        fixture.respond(json!({"result":{"pane":fixture.pane,"agents":[fixture.pane]}}));
        fixture.ok(&["init"]);
        fs::write(
            fixture.dir.path().join("pixel.png"),
            b"\x89PNG\r\n\x1a\nfixture",
        )
        .unwrap();
        fixture.ok(&[
            "add",
            "Recover\nKeep body",
            "--priority",
            "7",
            "--tag",
            "recovery",
            "--image",
            "pixel.png",
        ]);
        fixture.ok(&["message", "1", "Keep note"]);
        let output = fixture
            .command()
            .env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", "w1:p1")
            .args(["next", "--local"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(fixture.dir.path().join("calls"), "").unwrap();
        fixture
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
        command
            .current_dir(self.dir.path())
            .arg("--json")
            .env_remove("QQQ_SESSION")
            .env_remove("CODEX_THREAD_ID")
            .env_remove("CODEX_SESSION_ID")
            .env_remove("HERDR_ENV")
            .env_remove("HERDR_PANE_ID")
            .env_remove("HERDR_SOCKET_PATH")
            .env("PATH", self.dir.path())
            .env("QQQ_TEST_RESPONSE", self.dir.path().join("response.json"))
            .env("QQQ_TEST_CALLS", self.dir.path().join("calls"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn respond(&self, response: Value) {
        fs::write(self.dir.path().join("response.json"), response.to_string()).unwrap();
    }

    fn ok(&self, args: &[&str]) -> Value {
        let output = self.command().args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn show(&self) -> Value {
        self.ok(&["show", "1"])
    }

    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join(".qqq/qqq.db")).unwrap()
    }

    fn reject(&self, code: &str, reason: Option<&str>) {
        let before = self.show();
        let output = self
            .command()
            .args(["reopen", "1", "--session", "reviewer"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], code, "{error}");
        if let Some(reason) = reason {
            assert_eq!(error["details"]["reason"], reason, "{error}");
        }
        assert_eq!(self.show(), before);
    }
}

#[test]
fn orphaned_terminal_and_reported_session_reopen_preserves_task_and_can_reclaim() {
    for reported in [false, true] {
        let fixture = Fixture::new(reported);
        let before = fixture.show();
        assert!(before["herdr"].get("claim_key").is_none());
        let old_owner: String = fixture
            .db()
            .query_row("SELECT claim_key FROM tasks WHERE id=1", [], |r| r.get(0))
            .unwrap();
        fixture
            .db()
            .execute(
                "UPDATE tasks SET updated_at='2026-01-01T00:00:00Z' WHERE id=1",
                [],
            )
            .unwrap();
        fixture.respond(json!({"result":{"agents":[]}}));
        let reopened = fixture.ok(&["reopen", "-1", "--session", "reviewer"]);
        assert_eq!(reopened["status"], "new");
        for field in [
            "description",
            "content_revision",
            "tags",
            "priority",
            "parent_id",
            "prerequisites",
            "created_at",
        ] {
            assert_eq!(reopened[field], before["task"][field], "{field}");
        }
        assert_ne!(reopened["updated_at"], "2026-01-01T00:00:00Z");
        for field in [
            "harness_name",
            "harness_session",
            "orchestrator_name",
            "orchestrator_session",
        ] {
            assert!(reopened[field].is_null(), "{field}");
        }
        let after = fixture.show();
        for field in ["messages", "images", "herdr"] {
            assert_eq!(after[field], before[field], "{field}");
        }
        assert_eq!(after["events"][0], before["events"][0]);
        assert_eq!(after["events"][1]["action"], "reopen");
        assert_eq!(after["events"][1]["session"], "reviewer");
        assert_eq!(after["events"].as_array().unwrap().len(), 2);
        assert!(
            fs::read_to_string(fixture.dir.path().join("calls"))
                .unwrap()
                .contains("--session default agent list")
        );
        assert_eq!(
            fixture.ok(&["next", "--local", "--session", "replacement"])["id"],
            1
        );
        assert!(fixture.show()["herdr"].is_null());
        let stale = fixture
            .command()
            .args(["complete", "1", "--session", &old_owner])
            .output()
            .unwrap();
        assert!(!stale.status.success());
        assert_eq!(fixture.show()["task"]["status"], "in_progress");
    }
}

#[test]
fn live_owner_moved_pane_and_changed_session_reporting_prevent_reopen() {
    for reported in [false, true] {
        let fixture = Fixture::new(reported);
        let mut moved = fixture.pane.clone();
        moved["pane_id"] = json!("w2:p9");
        moved["tab_id"] = json!("w2:t4");
        fixture.respond(json!({"result":{"agents":[moved]}}));
        fixture.reject("INVALID_TRANSITION", Some("owner_still_live"));
        let mut changed = fixture.pane.clone();
        changed["agent_session"] = json!({"agent":"codex","kind":"id","value":"new-report"});
        fixture.respond(json!({"result":{"agents":[changed]}}));
        fixture.reject("INVALID_TRANSITION", Some("owner_still_live"));
        let mut missing = fixture.pane.clone();
        missing["agent_session"] = Value::Null;
        fixture.respond(json!({"result":{"agents":[missing]}}));
        fixture.reject("INVALID_TRANSITION", Some("owner_still_live"));
    }
}

#[test]
fn another_agent_kind_with_same_terminal_does_not_keep_owner_alive() {
    let fixture = Fixture::new(false);
    let mut other = fixture.pane.clone();
    other["agent"] = json!("claude");
    fixture.respond(json!({"result":{"agents":[other]}}));
    assert_eq!(fixture.ok(&["reopen", "1"])["status"], "new");
}

#[test]
fn exact_reported_session_remains_live_without_saved_terminal() {
    let fixture = Fixture::new(true);
    let mut moved = fixture.pane.clone();
    moved["terminal_id"] = json!("another-terminal");
    moved["agent"] = Value::Null;
    fixture.respond(json!({"result":{"agents":[moved]}}));
    fixture.reject("INVALID_TRANSITION", Some("owner_still_live"));
}

#[test]
fn same_terminal_with_reported_agent_kind_keeps_owner_live() {
    let fixture = Fixture::new(true);
    let mut changed = fixture.pane.clone();
    changed["agent_session"] = json!({"agent":"codex","kind":"id","value":"changed-report"});
    changed["agent"] = Value::Null;
    fixture.respond(json!({"result":{"agents":[changed]}}));
    fixture.reject("INVALID_TRANSITION", Some("owner_still_live"));
}

#[test]
fn missing_saved_server_cannot_use_callers_empty_default_server() {
    let fixture = Fixture::new(true);
    fixture
        .db()
        .execute(
            "UPDATE herdr_links SET link_json=json_set(link_json,'$.server',NULL)",
            [],
        )
        .unwrap();
    fixture.respond(json!({"result":{"agents":[]}}));
    fixture.reject("DISPATCH_ERROR", Some("missing_owner_server"));
    assert!(
        fs::read_to_string(fixture.dir.path().join("calls"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn legacy_automatic_and_opaque_claim_links_can_recover_existing_work() {
    for opaque in [false, true] {
        let fixture = Fixture::new(true);
        if opaque {
            fixture.ok(&["edit", "1", "--set-status", "new", "--session", "owner-id"]);
            fixture.ok(&["next", "--local", "--session", "qqq-dispatch-legacy"]);
            fixture.ok(&[
                "herdr",
                "link",
                "1",
                "--agent",
                "codex",
                "--agent-session",
                "owner-id",
            ]);
        }
        fixture
            .db()
            .execute(
                "UPDATE herdr_links SET link_json=json_remove(link_json,'$.claim_key')",
                [],
            )
            .unwrap();
        fixture.respond(json!({"result":{"agents":[]}}));
        assert_eq!(fixture.ok(&["reopen", "1"])["status"], "new");
    }
}

#[test]
fn associated_opaque_dispatch_claim_reopens_after_owner_disappears() {
    let fixture = Fixture::new(true);
    fixture.ok(&["edit", "1", "--set-status", "new", "--session", "owner-id"]);
    fixture.ok(&["next", "--local", "--session", "qqq-dispatch-worker"]);
    fixture.ok(&[
        "herdr",
        "link",
        "1",
        "--agent",
        "codex",
        "--agent-session",
        "owner-id",
    ]);
    fixture.respond(json!({"result":{"agents":[]}}));
    assert_eq!(fixture.ok(&["reopen", "1"])["status"], "new");
}

#[test]
fn missing_and_stale_links_cannot_release_current_owner() {
    for missing in [false, true] {
        let fixture = Fixture::new(true);
        if missing {
            fixture.db().execute("DELETE FROM herdr_links", []).unwrap();
        } else {
            fixture
                .db()
                .execute("UPDATE tasks SET claim_key='replacement' WHERE id=1", [])
                .unwrap();
        }
        fixture.respond(json!({"result":{"agents":[]}}));
        fixture.reject(
            "INVALID_TRANSITION",
            Some(if missing {
                "missing_owner_link"
            } else {
                "owner_link_mismatch"
            }),
        );
        assert!(
            fs::read_to_string(fixture.dir.path().join("calls"))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn failed_and_malformed_herdr_lookup_cannot_prove_owner_absence() {
    for malformed in [false, true] {
        let fixture = Fixture::new(true);
        if malformed {
            fs::write(fixture.dir.path().join("response.json"), "{\"result\":{}}").unwrap();
        } else {
            fs::write(fixture.dir.path().join("herdr"), "#!/bin/sh\nexit 1\n").unwrap();
        }
        fixture.reject(
            "DISPATCH_ERROR",
            Some(if malformed {
                "invalid_herdr_response"
            } else {
                "herdr_command_failed"
            }),
        );
    }
}

#[test]
fn orphaned_task_with_archived_unfinished_dependency_cannot_reopen() {
    let fixture = Fixture::new(false);
    fixture.ok(&["add", "Blocked parent"]);
    fixture.ok(&["edit", "1", "--set-parent", "2"]);
    // Simulate a legacy/external archived dependency; current archive guards prevent this state.
    fixture
        .db()
        .execute("UPDATE tasks SET archived=1 WHERE id=2", [])
        .unwrap();
    fixture.respond(json!({"result":{"agents":[]}}));
    fixture.reject("INVALID_TRANSITION", None);
}

#[test]
fn concurrent_orphan_reopens_commit_once() {
    let fixture = Fixture::new(true);
    fixture.respond(json!({"result":{"agents":[]}}));
    let children: Vec<_> = (0..2)
        .map(|_| fixture.command().args(["reopen", "1"]).spawn().unwrap())
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
    assert_eq!(fixture.show()["events"].as_array().unwrap().len(), 2);
    assert_eq!(fixture.show()["task"]["status"], "new");
}
