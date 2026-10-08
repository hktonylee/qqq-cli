#![cfg(unix)]

use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, process::Command};
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
    pane: Value,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("herdr"),
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$QQQ_TEST_CALLS"
[ "$1" = --session ] && shift 2
case "$1 $2" in
'pane current') /bin/cat "$QQQ_TEST_PANE";;
'agent list') [ -f "$QQQ_TEST_FAIL" ] && exit 1; /bin/cat "$QQQ_TEST_AGENTS";;
'session list') /bin/cat "$QQQ_TEST_SESSIONS";;
*) exit 1;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(dir.path().join("herdr"), fs::Permissions::from_mode(0o755)).unwrap();
        let pane = json!({"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1",
            "agent":"codex","terminal_id":"terminal-1","agent_session":null,"cwd":dir.path()});
        let fixture = Self { dir, pane };
        fixture.write("pane", json!({"result":{"pane":fixture.pane}}));
        fixture.write("agents", json!({"result":{"agents":[fixture.pane]}}));
        fixture.write(
            "sessions",
            json!({"sessions":[{"name":"work","running":true}]}),
        );
        fixture.ok(&["init"]);
        fixture.ok(&["add", "Preserve body", "--priority", "7", "--tag", "keep"]);
        fixture.ok(&["message", "1", "Keep note"]);
        let out = fixture
            .command()
            .env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", "w1:p1")
            .args(["next", "--local"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let db = fixture.db();
        let mut link: Value = serde_json::from_str(
            &db.query_row(
                "SELECT link_json FROM herdr_links WHERE task_id=1",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
        )
        .unwrap();
        link["server"] = json!("work");
        db.execute(
            "UPDATE herdr_links SET link_json=? WHERE task_id=1",
            [link.to_string()],
        )
        .unwrap();
        fs::write(fixture.dir.path().join("calls"), "").unwrap();
        fixture
    }

    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
        c.current_dir(self.dir.path())
            .arg("--json")
            .env_remove("QQQ_SESSION")
            .env_remove("CODEX_THREAD_ID")
            .env_remove("CODEX_SESSION_ID")
            .env_remove("HERDR_ENV")
            .env_remove("HERDR_PANE_ID")
            .env_remove("HERDR_SOCKET_PATH")
            .env("PATH", self.dir.path())
            .env("QQQ_TEST_PANE", self.dir.path().join("pane"))
            .env("QQQ_TEST_AGENTS", self.dir.path().join("agents"))
            .env("QQQ_TEST_SESSIONS", self.dir.path().join("sessions"))
            .env("QQQ_TEST_CALLS", self.dir.path().join("calls"))
            .env("QQQ_TEST_FAIL", self.dir.path().join("fail"));
        c
    }

    fn ok(&self, args: &[&str]) -> Value {
        let out = self.command().args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn db(&self) -> Connection {
        Connection::open(self.dir.path().join(".qqq/qqq.db")).unwrap()
    }
    fn write(&self, name: &str, value: Value) {
        fs::write(self.dir.path().join(name), value.to_string()).unwrap();
    }
    fn gone(&self) {
        self.write("agents", json!({"result":{"agents":[]}}));
    }
}

#[test]
fn dead_harness_fails_before_show_preserving_data_and_exactly_one_reason() {
    let f = Fixture::new();
    let before = f.ok(&["show", "1"]);
    f.gone();
    let after = f.ok(&["show", "1"]);
    assert_eq!(after["task"]["status"], "error");
    for field in [
        "description",
        "content_revision",
        "priority",
        "tags",
        "created_at",
        "parent_id",
        "prerequisites",
    ] {
        assert_eq!(after["task"][field], before["task"][field], "{field}");
    }
    for field in [
        "harness_name",
        "harness_session",
        "orchestrator_name",
        "orchestrator_session",
    ] {
        assert!(after["task"][field].is_null(), "{field}");
    }
    assert_eq!(after["herdr"], before["herdr"]);
    assert_eq!(after["images"], before["images"]);
    assert_eq!(after["messages"][0], before["messages"][0]);
    assert_eq!(after["events"][0], before["events"][0]);
    assert_eq!(after["events"][1]["action"], "error");
    assert_eq!(after["events"][1]["session"], "qqq-preflight");
    assert!(
        after["messages"][1]["body"]
            .as_str()
            .unwrap()
            .contains("work")
    );
    assert_eq!(after["messages"].as_array().unwrap().len(), 2);
    assert_eq!(after["events"].as_array().unwrap().len(), 2);
    assert_eq!(f.ok(&["show", "1"]), after);
    assert!(
        fs::read_to_string(f.dir.path().join("calls"))
            .unwrap()
            .contains("--session work agent list")
    );
}

#[test]
fn stopped_or_absent_saved_orchestrator_fails_before_list() {
    for sessions in [json!([]), json!([{"name":"work","running":false}])] {
        let f = Fixture::new();
        fs::write(f.dir.path().join("fail"), "").unwrap();
        f.write("sessions", json!({"sessions":sessions}));
        assert_eq!(f.ok(&["list"])[0]["status"], "error");
        let shown = f.ok(&["show", "1"]);
        assert!(
            shown["messages"][1]["body"]
                .as_str()
                .unwrap()
                .contains("orchestrator")
        );
    }
}

#[test]
fn live_and_unknown_owner_evidence_leave_db_bytes_unchanged() {
    for mode in [
        "live",
        "moved",
        "changed",
        "failed",
        "malformed",
        "missing-running",
        "missing-server",
        "mismatched-link",
    ] {
        let f = Fixture::new();
        let mut pane = f.pane.clone();
        match mode {
            "moved" => {
                pane["pane_id"] = json!("w2:p8");
                f.write("agents", json!({"result":{"agents":[pane]}}));
            }
            "changed" => {
                pane["agent_session"] = json!({"agent":"codex","kind":"id","value":"new-report"});
                f.write("agents", json!({"result":{"agents":[pane]}}));
            }
            "failed" | "missing-running" => {
                fs::write(f.dir.path().join("fail"), "").unwrap();
                if mode == "missing-running" {
                    f.write("sessions", json!({"sessions":[{"name":"work"}]}));
                }
            }
            "malformed" => {
                fs::write(f.dir.path().join("agents"), "{}").unwrap();
                fs::write(f.dir.path().join("sessions"), "{}").unwrap();
            }
            "missing-server" | "mismatched-link" => {
                let db = f.db();
                let mut link: Value = serde_json::from_str(
                    &db.query_row("SELECT link_json FROM herdr_links", [], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap(),
                )
                .unwrap();
                link[if mode == "missing-server" {
                    "server"
                } else {
                    "claim_key"
                }] = if mode == "missing-server" {
                    Value::Null
                } else {
                    json!("wrong-owner")
                };
                db.execute("UPDATE herdr_links SET link_json=?", [link.to_string()])
                    .unwrap();
                f.gone();
            }
            _ => (),
        }
        let path = f.dir.path().join(".qqq/qqq.db");
        let before = fs::read(&path).unwrap();
        assert_eq!(
            f.ok(&["show", "1"])["task"]["status"],
            "in_progress",
            "{mode}"
        );
        assert_eq!(fs::read(path).unwrap(), before, "{mode}");
    }
}

#[test]
fn two_claims_on_saved_server_use_one_probe() {
    let f = Fixture::new();
    f.ok(&["add", "Second"]);
    f.ok(&["next", "--local", "--session", "second-owner"]);
    let db = f.db();
    let mut link: Value = serde_json::from_str(
        &db.query_row(
            "SELECT link_json FROM herdr_links WHERE task_id=1",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap(),
    )
    .unwrap();
    link["claim_key"] = json!("second-owner");
    db.execute(
        "INSERT INTO herdr_links(task_id,link_json) VALUES(2,?)",
        [link.to_string()],
    )
    .unwrap();
    fs::write(f.dir.path().join("calls"), "").unwrap();
    f.gone();
    let tasks = f.ok(&["list"]);
    assert!(
        tasks
            .as_array()
            .unwrap()
            .iter()
            .all(|task| task["status"] == "error")
    );
    let calls = fs::read_to_string(f.dir.path().join("calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|line| *line == "--session work agent list")
            .count(),
        1,
        "{calls}"
    );
}
