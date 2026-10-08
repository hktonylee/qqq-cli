#![cfg(unix)]

use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::PermissionsExt,
    process::{Child, Command, Stdio},
};
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
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
fn invalid_tag_selectors_precede_active_owner_preflight() {
    let f = Fixture::new();
    f.gone();
    let db_path = f.dir.path().join(".qqq/qqq.db");
    let before = fs::read(&db_path).unwrap();
    for args in [
        vec!["list", "--tag", "bad,tag"],
        vec!["list", "--watch", "--tag", ""],
        vec!["next", "--local", "--tag", "[bad]"],
        vec!["next", "--explain", "--filter", "has_tag('')"],
        vec!["next", "--wait", "--filter", r"has_tag('bad\ntag')"],
        vec!["next", "--dry-run", "--tag", "\tbad"],
    ] {
        let out = f.command().args(&args).output().unwrap();
        assert!(!out.status.success(), "{args:?}");
        assert!(out.stdout.is_empty());
        let error: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(
            error["code"],
            if args.contains(&"--filter") {
                "INVALID_FILTER"
            } else {
                "INVALID_ARGUMENT"
            }
        );
        assert_eq!(fs::read(&db_path).unwrap(), before);
        assert_eq!(fs::read_to_string(f.dir.path().join("calls")).unwrap(), "");
    }
    assert_eq!(f.ok(&["show", "1"])["task"]["status"], "error");
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
        "empty-identity",
        "incomplete-agent",
        "foreign-binding",
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
            "incomplete-agent" => {
                pane["agent"] = Value::Null;
                pane["agent_session"] = Value::Null;
                f.write("agents", json!({"result":{"agents":[pane]}}));
            }
            "foreign-binding" => {
                let process = json!({"machine":"another-machine","pid":42,"started_at":"start","executable":"codex"});
                f.db().execute("INSERT OR REPLACE INTO claim_processes(task_id,claim_key,claim_event_id,process_json)
                    SELECT id,claim_key,(SELECT MAX(id) FROM events WHERE task_id=1 AND action='claim'),? FROM tasks WHERE id=1", [process.to_string()]).unwrap();
                f.gone();
            }
            "missing-server" | "mismatched-link" | "empty-identity" => {
                let db = f.db();
                let mut link: Value = serde_json::from_str(
                    &db.query_row("SELECT link_json FROM herdr_links", [], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap(),
                )
                .unwrap();
                if mode == "empty-identity" {
                    link["identity"]["value"] = json!("");
                } else {
                    link[if mode == "missing-server" {
                        "server"
                    } else {
                        "claim_key"
                    }] = if mode == "missing-server" {
                        Value::Null
                    } else {
                        json!("wrong-owner")
                    };
                }
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
fn hung_herdr_probe_is_unknown_and_command_finishes_within_timeout() {
    let f = Fixture::new();
    let script = fs::read_to_string(f.dir.path().join("herdr"))
        .unwrap()
        .lines()
        .map(|line| {
            if line.starts_with("'agent list')") {
                "'agent list') exec /bin/sleep 30;;"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(f.dir.path().join("herdr"), script).unwrap();
    let started = std::time::Instant::now();
    assert_eq!(f.ok(&["show", "1"])["task"]["status"], "in_progress");
    assert!(started.elapsed() < std::time::Duration::from_secs(6));
}

#[test]
fn init_new_nested_project_does_not_scan_parent_claims() {
    let f = Fixture::new();
    f.gone();
    let nested = f.dir.path().join("child-project");
    fs::create_dir(&nested).unwrap();
    let out = f
        .command()
        .current_dir(&nested)
        .arg("init")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(nested.join(".qqq/qqq.db").exists());
    assert_eq!(
        f.db()
            .query_row("SELECT status FROM tasks WHERE id=1", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "in_progress"
    );
}

#[test]
fn rejected_nested_restore_does_not_scan_parent_claims() {
    let f = Fixture::new();
    f.ok(&["backup", "copy.tar"]);
    f.gone();
    let nested = f.dir.path().join("restored-project");
    fs::create_dir(&nested).unwrap();
    let out = f
        .command()
        .current_dir(&nested)
        .args(["restore", "../copy.tar"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!nested.join(".qqq/qqq.db").exists());
    assert_eq!(
        f.db()
            .query_row("SELECT status FROM tasks WHERE id=1", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "in_progress"
    );
}

#[test]
fn relink_after_observation_wins_over_old_owner_failure() {
    let f = Fixture::new();
    f.gone();
    let script = fs::read_to_string(f.dir.path().join("herdr")).unwrap().replace(
        "'agent list')", "'agent list') if [ -f \"$QQQ_TEST_BLOCK\" ]; then : > \"$QQQ_TEST_ENTERED\"; while [ -f \"$QQQ_TEST_BLOCK\" ]; do /bin/sleep 0.01; done; fi;");
    fs::write(f.dir.path().join("herdr"), script).unwrap();
    let block = f.dir.path().join("block");
    let entered = f.dir.path().join("entered");
    fs::write(&block, "").unwrap();
    let mut child = Harness(
        f.command()
            .env("QQQ_TEST_BLOCK", &block)
            .env("QQQ_TEST_ENTERED", &entered)
            .args(["show", "1"])
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !entered.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "preflight never probed owner"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    f.db().execute("UPDATE herdr_links SET link_json=json_set(link_json,'$.server','new-live-server','$.identity.value','new-terminal','$.pane.terminal_id','new-terminal') WHERE task_id=1", []).unwrap();
    fs::remove_file(block).unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(&mut child.0.stdout.take().unwrap(), &mut text).unwrap();
    assert!(child.0.wait().unwrap().success());
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["task"]["status"],
        "in_progress"
    );
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

struct Harness(Child);
impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn native_command(dir: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir.path())
        .arg("--json")
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    command
}

fn native_claim() -> (TempDir, Harness) {
    let dir = TempDir::new().unwrap();
    for args in [&["init"][..], &["add", "Native task"][..]] {
        assert!(
            native_command(&dir)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    fs::write(
        dir.path().join("parent.rs"),
        r#"
use std::{io::Read, process::Command};
fn main() {
    assert!(Command::new(std::env::var_os("QQQ_BIN").unwrap())
        .args(["--json", "next", "--local"]).status().unwrap().success());
    println!("READY");
    let _ = std::io::stdin().read_exact(&mut [0u8]);
}
"#,
    )
    .unwrap();
    let compile = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg(dir.path().join("parent.rs"))
        .arg("-o")
        .arg(dir.path().join("codex"))
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let mut parent = Command::new(dir.path().join("codex"));
    parent
        .current_dir(dir.path())
        .env("QQQ_BIN", env!("CARGO_BIN_EXE_qqq"))
        .env("CODEX_THREAD_ID", "native-preflight-owner")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("QQQ_SESSION")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut harness = Harness(parent.spawn().unwrap());
    let mut reader = BufReader::new(harness.0.stdout.take().unwrap());
    let mut text = String::new();
    loop {
        let mut line = String::new();
        assert_ne!(
            reader.read_line(&mut line).unwrap(),
            0,
            "harness exited: {text}"
        );
        if line.trim() == "READY" {
            break;
        }
        text.push_str(&line);
    }
    let task: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(task["status"], "in_progress");
    (dir, harness)
}

#[test]
fn native_harness_death_is_detected_before_command_without_herdr_link() {
    let (dir, mut harness) = native_claim();
    let out = native_command(&dir).args(["show", "1"]).output().unwrap();
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["task"]["status"],
        "in_progress"
    );
    harness.0.kill().unwrap();
    harness.0.wait().unwrap();
    let out = native_command(&dir).args(["show", "1"]).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shown: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(shown["task"]["status"], "error");
    assert!(
        shown["messages"][0]["body"]
            .as_str()
            .unwrap()
            .contains("process")
    );
    assert!(shown["herdr"].is_null());
}

#[test]
fn concurrent_commands_fail_one_owner_once() {
    let f = Fixture::new();
    f.gone();
    let mut commands: Vec<_> = (0..5)
        .map(|_| f.command().args(["show", "1"]).spawn().unwrap())
        .collect();
    for command in &mut commands {
        assert!(command.wait().unwrap().success());
    }
    let shown = f.ok(&["show", "1"]);
    assert_eq!(shown["events"].as_array().unwrap().len(), 2);
    assert_eq!(shown["messages"].as_array().unwrap().len(), 2);
}

#[test]
fn same_key_reclaim_after_observation_wins_over_stale_failure() {
    let f = Fixture::new();
    f.gone();
    let script = fs::read_to_string(f.dir.path().join("herdr")).unwrap().replace(
        "'agent list')", "'agent list') if [ -f \"$QQQ_TEST_BLOCK\" ]; then : > \"$QQQ_TEST_ENTERED\"; while [ -f \"$QQQ_TEST_BLOCK\" ]; do /bin/sleep 0.01; done; fi;");
    fs::write(f.dir.path().join("herdr"), script).unwrap();
    let block = f.dir.path().join("block");
    let entered = f.dir.path().join("entered");
    fs::write(&block, "").unwrap();
    let child = f
        .command()
        .env("QQQ_TEST_BLOCK", &block)
        .env("QQQ_TEST_ENTERED", &entered)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(["show", "1"])
        .spawn()
        .unwrap();
    let mut child = Harness(child);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !entered.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "preflight never probed owner"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let db = f.db();
    db.execute_batch("BEGIN IMMEDIATE; INSERT INTO events(task_id,session,action) SELECT id,claim_key,'release' FROM tasks WHERE id=1;
        INSERT INTO events(task_id,session,action) SELECT id,claim_key,'claim' FROM tasks WHERE id=1; COMMIT;").unwrap();
    fs::remove_file(block).unwrap();
    let output = child.0.stdout.take().unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(&mut BufReader::new(output), &mut text).unwrap();
    assert!(child.0.wait().unwrap().success());
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["task"]["status"],
        "in_progress"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM events WHERE action='error'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn preflight_failed_owner_can_be_explicitly_reopened() {
    let f = Fixture::new();
    f.gone();
    assert_eq!(f.ok(&["show", "1"])["task"]["status"], "error");
    f.ok(&["archive", "1"]);
    f.ok(&["unarchive", "1"]);
    assert_eq!(
        f.ok(&["reopen", "1", "--session", "reviewer"])["status"],
        "new"
    );
    assert_eq!(
        f.ok(&["next", "--local", "--session", "replacement"])["id"],
        1
    );
    assert!(f.ok(&["show", "1"])["herdr"].is_null());
}

#[test]
fn inspection_preflight_fails_dead_owner_before_preview_without_claiming_work() {
    for args in [
        &["status"][..],
        &["doctor"][..],
        &["next", "--dry-run"][..],
        &["next", "--explain"][..],
    ] {
        let f = Fixture::new();
        f.ok(&["add", "Queued"]);
        f.gone();
        f.ok(args);
        let db = f.db();
        assert_eq!(
            db.query_row("SELECT status FROM tasks WHERE id=1", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "error",
            "{args:?}"
        );
        assert_eq!(
            db.query_row("SELECT status FROM tasks WHERE id=2", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "new"
        );
    }
}

#[test]
fn native_pid_reuse_foreign_machine_and_stale_binding_are_distinguished() {
    for mode in ["reuse", "foreign", "stale-event", "mismatched-key"] {
        let (dir, _harness) = native_claim();
        let db = Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
        let original: String = db
            .query_row("SELECT process_json FROM claim_processes", [], |r| r.get(0))
            .unwrap();
        let mut identity: Value = serde_json::from_str(&original).unwrap();
        identity["started_at"] = json!("different process start");
        match mode {
            "foreign" => identity["machine"] = json!("another-machine"),
            "stale-event" => {
                db.execute("INSERT INTO events(task_id,session,action) SELECT id,claim_key,'claim' FROM tasks WHERE id=1", []).unwrap();
            }
            "mismatched-key" => {
                db.execute("UPDATE claim_processes SET claim_key='stale-owner'", [])
                    .unwrap();
            }
            _ => (),
        }
        db.execute(
            "UPDATE claim_processes SET process_json=?",
            [identity.to_string()],
        )
        .unwrap();
        let out = native_command(&dir).args(["show", "1"]).output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let task: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            task["task"]["status"],
            if mode == "reuse" {
                "error"
            } else {
                "in_progress"
            },
            "{mode}"
        );
    }
}

#[test]
fn retrieving_native_assignment_preserves_original_process_binding() {
    let (dir, _harness) = native_claim();
    let db = Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
    let before: String = db
        .query_row("SELECT process_json FROM claim_processes", [], |r| r.get(0))
        .unwrap();
    let out = native_command(&dir)
        .args([
            "next",
            "--local",
            "--harness-session",
            "native-preflight-owner",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        db.query_row("SELECT process_json FROM claim_processes", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        before
    );
}
