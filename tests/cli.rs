use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.arg("--json")
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    c
}
fn run(dir: &Path, args: &[&str]) -> Output {
    command(dir).args(args).output().unwrap()
}
fn ok(dir: &Path, args: &[&str]) -> Value {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("JSON output")
}
fn project() -> TempDir {
    let d = TempDir::new().unwrap();
    ok(d.path(), &["init"]);
    d
}

#[test]
fn next_dry_run_previews_priority_filter_and_readiness_without_claim() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Blocked", "--parent", "1", "--priority", "100"]);
    ok(p, &["add", "Archived", "--priority", "100"]);
    ok(p, &["archive", "3"]);
    ok(p, &["add", "Match first", "--priority", "5"]);
    ok(p, &["add", "Match second", "--priority", "5"]);
    ok(p, &["add", "Done", "--priority", "100"]);
    ok(
        p,
        &[
            "next",
            "--local",
            "--session",
            "finished",
            "--filter",
            "id == 6",
        ],
    );
    ok(p, &["complete", "6", "--session", "finished"]);
    ok(p, &["add", "Failed", "--priority", "100"]);
    ok(
        p,
        &[
            "next",
            "--local",
            "--session",
            "failed",
            "--filter",
            "id == 7",
        ],
    );
    ok(
        p,
        &[
            "edit",
            "7",
            "--set-status",
            "error",
            "--reason",
            "Retry needed",
            "--session",
            "failed",
        ],
    );
    let before = ok(p, &["list", "--include-archived"]);
    let preview = ok(p, &["next", "--dry-run", "--local", "--session", "preview"]);
    assert_eq!(preview["id"], 4);
    assert_eq!(preview["status"], "new");
    assert!(preview["harness_session"].is_null());
    assert_eq!(ok(p, &["list", "--include-archived"]), before);
    assert!(
        ok(p, &["show", "4"])["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        ok(p, &["next", "--dry-run", "--filter", "id == 5"])["id"],
        5
    );
    assert!(ok(p, &["next", "--dry-run", "--filter", "id == 2"]).is_null());
    assert!(ok(p, &["next", "--dry-run", "--filter", "false"]).is_null());
    assert_eq!(ok(p, &["next", "--local", "--session", "worker"])["id"], 4);
}

#[test]
fn next_dry_run_skips_owned_task_and_keeps_queue_without_writes() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Owned"]);
    ok(p, &["next", "--local", "--session", "owner"]);
    let queued = ok(p, &["add", "Queued"]);
    let conn = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    let link = serde_json::json!({
        "server": "saved", "identity": {"agent": "codex", "kind": "id", "value": "saved"},
        "pane": {"pane_id": "saved", "workspace_id": "saved", "tab_id": "saved", "terminal_id": "saved", "agent": "codex"}
    });
    conn.execute(
        "INSERT INTO herdr_links(task_id,link_json) VALUES(2,?)",
        [link.to_string()],
    )
    .unwrap();
    for table in ["tasks", "events", "herdr_links"] {
        for operation in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!("CREATE TRIGGER preview_no_{table}_{operation} BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT, 'preview mutated queue'); END;")).unwrap();
        }
    }
    let before_owned = ok(p, &["show", "1"]);
    let before_queued = ok(p, &["show", "2"]);
    let preview = ok(
        p,
        &[
            "next",
            "--dry-run",
            "--local",
            "--session",
            "owner",
            "--harness-name",
            "changed",
            "--harness-session",
            "changed",
            "--orchestrator-name",
            "changed",
        ],
    );
    assert_eq!(preview, queued);
    assert_eq!(ok(p, &["show", "1"]), before_owned);
    assert_eq!(ok(p, &["show", "2"]), before_queued);
    assert_eq!(ok(p, &["next", "--dry-run"]), queued);
    assert!(
        ok(
            p,
            &[
                "next",
                "--dry-run",
                "--session",
                "owner",
                "--filter",
                "false"
            ]
        )
        .is_null()
    );
}

#[test]
fn cli_name_stays_qqq_in_help_and_version() {
    let dir = TempDir::new().unwrap();
    let version = run(dir.path(), &["--version"]);
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap(),
        format!("qqq {}\n", env!("CARGO_PKG_VERSION"))
    );

    let help = run(dir.path(), &["--help"]);
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("Usage: qqq ")
    );
}
#[test]
fn persistent_ownership_and_fifo() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "First\n\nDetails"]);
    ok(p, &["add", "Second"]);
    assert_eq!(ok(p, &["next", "--session", "a"])["id"], 1);
    assert_eq!(ok(p, &["next", "--session", "a"])["id"], 1);
    assert_eq!(ok(p, &["next", "--session", "b"])["id"], 2);
    assert!(
        !run(p, &["complete", "1", "--session", "b"])
            .status
            .success()
    );
    assert!(
        !run(p, &["edit", "1", "--set-status", "new", "--session", "b"])
            .status
            .success()
    );
    assert!(!run(p, &["next"]).status.success());
    ok(p, &["edit", "1", "--set-status", "new", "--session", "a"]);
    assert_eq!(ok(p, &["next", "--session", "c"])["id"], 1);
    ok(p, &["complete", "1", "--session", "c"]);
    assert!(ok(p, &["next", "--session", "c"]).is_null());
    assert_eq!(ok(p, &["show", "1"])["task"]["status"], "completed");
    assert_eq!(ok(p, &["show", "1"])["events"].as_array().unwrap().len(), 4);
}
#[test]
fn descriptions_messages_images_and_parent_discovery() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Task"]);
    ok(p, &["edit", "1", "--description", "Long description"]);
    ok(p, &["message", "1", "First note", "--session", "a"]);
    ok(p, &["message", "1", "Second note"]);
    let png = b"\x89PNG\r\n\x1a\nfixture";
    std::fs::write(p.join("image.png"), png).unwrap();
    ok(p, &["edit", "1", "--image", "image.png"]);
    std::fs::remove_file(p.join("image.png")).unwrap();
    std::fs::create_dir(p.join("nested")).unwrap();
    let detail = ok(&p.join("nested"), &["show", "1"]);
    assert_eq!(detail["task"]["description"], "Long description");
    assert_eq!(detail["messages"][0]["body"], "First note");
    assert_eq!(detail["messages"][1]["body"], "Second note");
    assert_eq!(detail["images"][0]["media_type"], "image/png");
    ok(
        p,
        &["show", "1", "--export-image", "1", "--output", "out.png"],
    );
    assert_eq!(std::fs::read(p.join("out.png")).unwrap(), png);
    assert!(
        !run(
            p,
            &["show", "1", "--export-image", "1", "--output", "out.png"]
        )
        .status
        .success()
    );
    std::fs::write(p.join("bad.png"), b"not image").unwrap();
    assert!(
        !run(p, &["edit", "1", "--image", "bad.png"])
            .status
            .success()
    );
    assert!(!run(p, &["message", "99", "missing task"]).status.success());
    assert!(!run(p, &["add", "   "]).status.success());
}
#[test]
fn concurrent_claims_are_unique() {
    let d = project();
    for i in 0..8 {
        ok(d.path(), &["add", &format!("Task {i}")]);
    }
    let children: Vec<_> = (0..8)
        .map(|i| {
            command(d.path())
                .args(["next", "--session", &format!("agent-{i}")])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut ids = Vec::new();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        ids.push(
            serde_json::from_slice::<Value>(&out.stdout).unwrap()["id"]
                .as_i64()
                .unwrap(),
        );
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 8);
}
#[test]
fn init_is_explicit_and_repeatable() {
    let d = TempDir::new().unwrap();
    assert!(!run(d.path(), &["list"]).status.success());
    assert!(!d.path().join("qqq.db").exists());
    ok(d.path(), &["init"]);
    assert!(d.path().join(".qqq/qqq.db").is_file());
    ok(d.path(), &["add", "Keep"]);
    ok(d.path(), &["init"]);
    assert_eq!(ok(d.path(), &["list"]).as_array().unwrap().len(), 1);
}

#[test]
fn init_creates_hidden_project_database() {
    let d = TempDir::new().unwrap();
    let initialized = ok(d.path(), &["init"]);
    assert_eq!(
        initialized["database"],
        d.path()
            .canonicalize()
            .unwrap()
            .join(".qqq/qqq.db")
            .to_string_lossy()
            .as_ref()
    );
    assert!(d.path().join(".qqq/qqq.db").is_file());
    assert!(!d.path().join("qqq.db").exists());
}

#[test]
fn lookup_uses_nearest_hidden_directory() {
    let d = TempDir::new().unwrap();
    let child = d.path().join("child");
    let grandchild = child.join("grandchild");
    std::fs::create_dir_all(&grandchild).unwrap();
    ok(d.path(), &["init"]);
    ok(d.path(), &["add", "Outer"]);
    assert_eq!(ok(&grandchild, &["list"])[0]["description"], "Outer");
    ok(&child, &["init"]);
    ok(&child, &["add", "Inner"]);
    assert_eq!(ok(&grandchild, &["list"])[0]["description"], "Inner");
    assert_eq!(ok(d.path(), &["list"])[0]["description"], "Outer");
}

#[test]
fn nearest_hidden_directory_without_database_does_not_fall_back() {
    let d = TempDir::new().unwrap();
    let child = d.path().join("child");
    std::fs::create_dir_all(child.join(".qqq")).unwrap();
    ok(d.path(), &["init"]);
    let output = run(&child, &["list"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No .qqq/qqq.db found"));
}

#[test]
fn root_level_legacy_database_is_not_discovered() {
    let d = TempDir::new().unwrap();
    std::fs::write(d.path().join("qqq.db"), b"legacy").unwrap();
    let output = run(d.path(), &["list"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("No .qqq directory found"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(d.path().join("qqq.db")).unwrap(), b"legacy");
}
#[cfg(unix)]
#[test]
fn herdr_matches_session_not_pane_or_cwd() {
    use std::os::unix::fs::PermissionsExt;
    let d = project();
    let p = d.path();
    ok(p, &["add", "Task"]);
    let bin = p.join("herdr");
    std::fs::write(&bin,r#"#!/bin/sh
printf '%s\n' '{"result":{"agents":[{"pane_id":"w1:p2","workspace_id":"w1","tab_id":"w1:t2","agent_session":{"agent":"codex","kind":"id","value":"session-123","source":"hook"}}]}}'
"#).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let exec = |args: &[&str]| {
        let out = command(p)
            .env(
                "PATH",
                format!("{}:{}", p.display(), std::env::var("PATH").unwrap()),
            )
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    exec(&[
        "herdr",
        "link",
        "1",
        "--agent",
        "codex",
        "--agent-session",
        "session-123",
    ]);
    assert_eq!(exec(&["herdr", "find", "1"])["pane_id"], "w1:p2");
    ok(p, &["next", "--session", "old-owner"]);
    exec(&[
        "herdr",
        "link",
        "1",
        "--agent",
        "codex",
        "--agent-session",
        "session-123",
    ]);
    ok(
        p,
        &["edit", "1", "--set-status", "new", "--session", "old-owner"],
    );
    ok(p, &["next", "--session", "new-owner"]);
    assert!(
        ok(p, &["show", "1"])["herdr"].is_null(),
        "fresh owner must not inherit stale Herdr link"
    );
    exec(&[
        "herdr",
        "link",
        "1",
        "--agent",
        "codex",
        "--agent-session",
        "session-123",
    ]);
    std::fs::write(
        &bin,
        "#!/bin/sh\nprintf '%s\\n' '{\"result\":{\"agents\":[]}}'\n",
    )
    .unwrap();
    let out = command(p)
        .env(
            "PATH",
            format!("{}:{}", p.display(), std::env::var("PATH").unwrap()),
        )
        .args(["herdr", "find", "1"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not live"));
}

#[test]
fn concurrent_same_session_returns_same_task() {
    let d = project();
    for _ in 0..4 {
        ok(d.path(), &["add", "Work"]);
    }
    let children: Vec<_> = (0..4)
        .map(|_| {
            command(d.path())
                .args(["next", "--session", "same"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap()["id"],
            1
        );
    }
    assert_eq!(ok(d.path(), &["next", "--session", "other"])["id"], 2);
}

#[cfg(unix)]
#[test]
fn herdr_auto_claim_records_link_and_supports_moved_pane() {
    use std::os::unix::fs::PermissionsExt;
    let d = project();
    let p = d.path();
    ok(p, &["add", "Auto"]);
    ok(p, &["add", "Second"]);
    let bin = p.join("herdr");
    std::fs::write(&bin,r#"#!/bin/sh
[ "$3" = "--pane" ] && [ "$4" = "$HERDR_PANE_ID" ] || exit 1
printf '%s\n' '{"result":{"pane":{"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1","agent_session":{"agent":"codex","kind":"id","value":"auto-session","source":"hook"}}}}'
"#).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let exec = |pane: &str, args: &[&str]| {
        command(p)
            .env(
                "PATH",
                format!("{}:{}", p.display(), std::env::var("PATH").unwrap()),
            )
            .env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", pane)
            .args(args)
            .output()
            .unwrap()
    };
    let bad = exec("", &["next"]);
    assert!(!bad.status.success());
    assert_eq!(ok(p, &["show", "1"])["task"]["status"], "new");
    let good = exec("old:p1", &["next"]);
    assert!(good.status.success());
    let detail = ok(p, &["show", "1"]);
    assert_eq!(detail["herdr"]["identity"]["value"], "auto-session");
    let complete = exec("old:p1", &["complete", "1"]);
    assert!(complete.status.success());
    assert_eq!(ok(p, &["show", "1"])["task"]["status"], "completed");
    // Missing session and terminal identity fails rather than using a pane ID.
    std::fs::write(
        &bin,
        r#"#!/bin/sh
printf '%s\n' '{"result":{"pane":{"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1"}}}'
"#,
    )
    .unwrap();
    assert!(!exec("w1:p1", &["next"]).status.success());
    assert_eq!(ok(p, &["show", "2"])["task"]["status"], "new");
}

#[cfg(unix)]
struct HerdrFixture {
    dir: TempDir,
}
#[cfg(unix)]
impl HerdrFixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let script = dir.path().join("herdr");
        std::fs::write(&script, "#!/bin/sh\n[ \"$1\" = --session ] && shift 2\n[ \"$1 $2\" = \"agent list\" ] || exit 1\n/bin/cat \"$QQQ_TEST_HERDR_RESPONSE\"\n").unwrap();
        std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }
    fn agents(&self, agents: Value) {
        std::fs::write(
            self.dir.path().join("response.json"),
            serde_json::to_vec(&serde_json::json!({"result":{"agents":agents}})).unwrap(),
        )
        .unwrap();
    }
    fn run(&self, dir: &Path, args: &[&str]) -> Output {
        command(dir)
            .env("PATH", self.dir.path())
            .env(
                "QQQ_TEST_HERDR_RESPONSE",
                self.dir.path().join("response.json"),
            )
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, dir: &Path, args: &[&str]) -> Value {
        let out = self.run(dir, args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
}
#[cfg(unix)]
fn agent_at(cwd: &Path, value: &str) -> Value {
    serde_json::json!({"pane_id":format!("w1:{value}"),"workspace_id":"w1","tab_id":"w1:t1","cwd":cwd,"agent_session":{"agent":"codex","kind":"id","value":value}})
}
#[cfg(unix)]
#[test]
fn cwd_lookup_uses_db_directory_from_nested_cwd_without_herdr_env() {
    let d = project();
    let p = d.path();
    let nested = p.join("nested");
    std::fs::create_dir(&nested).unwrap();
    ok(p, &["add", "Cwd task"]);
    let herdr = HerdrFixture::new();
    herdr.agents(serde_json::json!([
        agent_at(&nested, "wrong"),
        agent_at(p, "correct")
    ]));
    let task = herdr.ok(&nested, &["next"]);
    assert_eq!(task["harness_session"], "correct");
    assert_eq!(
        ok(p, &["show", "1"])["herdr"]["identity"]["value"],
        "correct"
    );
    assert_eq!(herdr.ok(&nested, &["next"])["id"], 1);
    herdr.ok(&nested, &["herdr", "link", "1"]);
    herdr.ok(&nested, &["edit", "1", "--set-status", "new"]);
    herdr.ok(&nested, &["next"]);
    assert_eq!(herdr.ok(&nested, &["complete", "1"])["status"], "completed");
}
#[cfg(unix)]
#[test]
fn cwd_lookup_normalizes_symlinks_and_prefers_foreground_cwd() {
    let d = project();
    let aliases = TempDir::new().unwrap();
    let alias = aliases.path().join("project");
    std::os::unix::fs::symlink(d.path(), &alias).unwrap();
    ok(d.path(), &["add", "Alias"]);
    let herdr = HerdrFixture::new();
    let mut moved = agent_at(d.path(), "moved-away");
    moved["foreground_cwd"] = serde_json::json!(aliases.path());
    let mut current = agent_at(aliases.path(), "correct");
    current["foreground_cwd"] = serde_json::json!(alias.join("."));
    herdr.agents(serde_json::json!([moved, current]));
    assert_eq!(herdr.ok(d.path(), &["next"])["harness_session"], "correct");
}
#[cfg(unix)]
#[test]
fn cwd_lookup_rejects_missing_ambiguous_or_unidentified_matches_before_claim() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "New"]);
    let herdr = HerdrFixture::new();
    let mut no_identity = agent_at(p, "unknown");
    no_identity.as_object_mut().unwrap().remove("agent_session");
    let cases = [
        (serde_json::json!([]), "No Herdr agent matches project root"),
        (
            serde_json::json!([agent_at(p, "a"), agent_at(p, "b")]),
            "Multiple Herdr agents match project root",
        ),
        (
            serde_json::json!([no_identity.clone()]),
            "no session or terminal identity",
        ),
        (
            serde_json::json!([agent_at(p, "a"), no_identity]),
            "Multiple Herdr agents match project root",
        ),
    ];
    for (agents, error) in cases {
        herdr.agents(agents);
        let out = herdr.run(p, &["next"]);
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains(error),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(ok(p, &["show", "1"])["task"]["status"], "new");
    }
    // Explicit identity bypasses ambiguous Herdr discovery.
    assert_eq!(
        herdr.ok(p, &["next", "--session", "explicit"])["harness_session"],
        "explicit"
    );
}

#[test]
fn edit_new_updates_fields_and_release_history_atomically() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Old\n\nDetails"]);
    ok(p, &["next", "--session", "a"]);
    let before = ok(p, &["show", "1"]);
    for args in [
        vec![
            "edit",
            "1",
            "--set-status",
            "new",
            "--description",
            "Changed",
            "--session",
            "wrong",
        ],
        vec![
            "edit",
            "1",
            "--set-status",
            "new",
            "--description",
            " ",
            "--session",
            "a",
        ],
        vec!["edit", "1", "--set-status", "completed", "--session", "a"],
    ] {
        assert!(!run(p, &args).status.success());
        assert_eq!(ok(p, &["show", "1"]), before);
    }
    let updated = ok(
        p,
        &[
            "edit",
            "-1",
            "--set-status",
            "new",
            "--description",
            "New\n\n",
            "--session",
            "a",
        ],
    );
    assert_eq!(updated["status"], "new");
    assert!(updated["harness_session"].is_null());
    assert_eq!(updated["description"], "New\n\n");
    let after = ok(p, &["show", "1"]);
    assert_eq!(after["events"].as_array().unwrap().len(), 2);
    assert_eq!(after["events"][1]["action"], "release");
    assert_eq!(after["events"][1]["session"], "a");
    assert!(
        !run(p, &["edit", "1", "--set-status", "new", "--session", "a"])
            .status
            .success()
    );
    assert_eq!(ok(p, &["show", "1"]), after);
    ok(p, &["next", "--session", "b"]);
    ok(p, &["complete", "1", "--session", "b"]);
    let completed = ok(p, &["show", "1"]);
    assert!(
        !run(p, &["edit", "1", "--set-status", "new", "--session", "b"])
            .status
            .success()
    );
    assert_eq!(ok(p, &["show", "1"]), completed);
}

#[test]
fn edit_new_skips_editor_and_release_command_is_removed() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Task"]);
    ok(p, &["next", "--session", "a"]);
    let output = command(p)
        .args(["edit", "1", "--set-status", "new", "--session", "a"])
        .env("EDITOR", "nonexistent-qqq-editor")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["status"], "new");
    assert_eq!(task["description"], "Task");
    assert_eq!(
        run(p, &["release", "1", "--session", "a"]).status.code(),
        Some(2)
    );
}

#[test]
fn new_is_initial_status_and_pending_flag_is_rejected() {
    let d = project();
    let p = d.path();
    let task = ok(p, &["add", "Fresh"]);
    assert_eq!(task["status"], "new");
    let conn = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    assert!(
        conn.execute("UPDATE tasks SET status='pending' WHERE id=1", [])
            .is_err()
    );
    ok(p, &["next", "--session", "a"]);
    let before = ok(p, &["show", "1"]);
    assert_eq!(
        run(
            p,
            &["edit", "1", "--set-status", "pending", "--session", "a"]
        )
        .status
        .code(),
        Some(2)
    );
    assert_eq!(ok(p, &["show", "1"]), before);
    assert_eq!(
        ok(p, &["edit", "1", "--set-status", "new", "--session", "a"])["status"],
        "new"
    );
}

#[test]
fn show_recent_uses_creation_order_across_statuses_and_id_gaps() {
    let d = project();
    let p = d.path();
    for description in ["First", "Removed", "Newest\n\nDetails"] {
        ok(p, &["add", description]);
    }
    let conn = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    conn.execute("DELETE FROM tasks WHERE id=2", []).unwrap();
    conn.execute("UPDATE tasks SET created_at='same timestamp'", [])
        .unwrap();
    ok(p, &["next", "--session", "worker"]);
    ok(p, &["complete", "1", "--session", "worker"]);
    ok(p, &["edit", "1", "-d", "Recently edited older task"]);
    assert_eq!(ok(p, &["show", "-1"]), ok(p, &["show", "3"]));
    assert_eq!(ok(p, &["show", "-2"]), ok(p, &["show", "1"]));
    assert_eq!(
        ok(p, &["show", "-1"])["task"]["description"],
        "Newest\n\nDetails"
    );
    assert_eq!(ok(p, &["show", "-2"])["task"]["status"], "completed");
}

#[test]
fn show_recent_exports_only_selected_tasks_image() {
    let d = project();
    let p = d.path();
    let png = b"\x89PNG\r\n\x1a\nfixture";
    std::fs::write(p.join("image.png"), png).unwrap();
    ok(p, &["add", "Older", "--image", "image.png"]);
    ok(p, &["add", "Newest", "--image", "image.png"]);
    let detail = ok(
        p,
        &["show", "-1", "--export-image", "2", "--output", "out.png"],
    );
    assert_eq!(detail["task"]["id"], 2);
    assert_eq!(std::fs::read(p.join("out.png")).unwrap(), png);
    let wrong = run(
        p,
        &["show", "-1", "--export-image", "1", "--output", "wrong.png"],
    );
    assert_eq!(wrong.status.code(), Some(1));
    assert!(!p.join("wrong.png").exists());
}

#[test]
fn show_recent_invalid_references_leave_export_and_database_unchanged() {
    let d = project();
    let p = d.path();
    for populated in [false, true] {
        if populated {
            ok(p, &["add", "Unchanged"]);
        }
        let before = ok(p, &["list"]);
        for reference in ["0", "-2", "-9223372036854775808", "99"] {
            let result = run(
                p,
                &[
                    "show",
                    reference,
                    "--export-image",
                    "1",
                    "--output",
                    "out.png",
                ],
            );
            assert_eq!(
                result.status.code(),
                Some(1),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stdout.is_empty());
            assert!(!p.join("out.png").exists());
            assert_eq!(ok(p, &["list"]), before);
        }
        if !populated {
            assert_eq!(run(p, &["show", "-1"]).status.code(), Some(1));
        }
    }
}
