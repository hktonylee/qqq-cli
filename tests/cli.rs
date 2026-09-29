use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.current_dir(dir)
        .env_remove("QQQ_SESSION")
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
fn persistent_ownership_and_fifo() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "First", "--description", "Details"]);
    ok(p, &["add", "Second"]);
    assert_eq!(ok(p, &["next", "--session", "a"])["id"], 1);
    assert_eq!(ok(p, &["next", "--session", "a"])["id"], 1);
    assert_eq!(ok(p, &["next", "--session", "b"])["id"], 2);
    assert!(
        !run(p, &["complete", "1", "--session", "b"])
            .status
            .success()
    );
    assert!(!run(p, &["release", "1", "--session", "b"]).status.success());
    assert!(!run(p, &["next"]).status.success());
    ok(p, &["release", "1", "--session", "a"]);
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
    ok(p, &["describe", "1", "Long description"]);
    ok(p, &["message", "1", "First note", "--session", "a"]);
    ok(p, &["message", "1", "Second note"]);
    let png = b"\x89PNG\r\n\x1a\nfixture";
    std::fs::write(p.join("image.png"), png).unwrap();
    ok(p, &["image", "add", "1", "image.png"]);
    std::fs::remove_file(p.join("image.png")).unwrap();
    std::fs::create_dir(p.join("nested")).unwrap();
    let detail = ok(&p.join("nested"), &["show", "1"]);
    assert_eq!(detail["task"]["description"], "Long description");
    assert_eq!(detail["messages"][0]["body"], "First note");
    assert_eq!(detail["messages"][1]["body"], "Second note");
    assert_eq!(detail["images"][0]["media_type"], "image/png");
    ok(p, &["image", "export", "1", "out.png"]);
    assert_eq!(std::fs::read(p.join("out.png")).unwrap(), png);
    assert!(
        !run(p, &["image", "export", "1", "out.png"])
            .status
            .success()
    );
    std::fs::write(p.join("bad.png"), b"not image").unwrap();
    assert!(!run(p, &["image", "add", "1", "bad.png"]).status.success());
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
    assert!(!d.path().join("qqq.sqlite").exists());
    ok(d.path(), &["init"]);
    ok(d.path(), &["add", "Keep"]);
    ok(d.path(), &["init"]);
    assert_eq!(ok(d.path(), &["list"]).as_array().unwrap().len(), 1);
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
    ok(p, &["release", "1", "--session", "old-owner"]);
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
[ "$3" = "--current" ] || exit 1
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
    assert_eq!(ok(p, &["show", "1"])["task"]["status"], "pending");
    let good = exec("old:p1", &["next"]);
    assert!(good.status.success());
    let detail = ok(p, &["show", "1"]);
    assert_eq!(detail["herdr"]["identity"]["value"], "auto-session");
    let complete = exec("old:p1", &["complete", "1"]);
    assert!(complete.status.success());
    assert_eq!(ok(p, &["show", "1"])["task"]["status"], "completed");
    // Missing hook identity fails rather than treating a pane ID as an agent session.
    std::fs::write(
        &bin,
        r#"#!/bin/sh
printf '%s\n' '{"result":{"pane":{"pane_id":"w1:p1","workspace_id":"w1","tab_id":"w1:t1"}}}'
"#,
    )
    .unwrap();
    assert!(!exec("w1:p1", &["next"]).status.success());
    assert_eq!(ok(p, &["show", "2"])["task"]["status"], "pending");
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
        std::fs::write(&script, "#!/bin/sh\n[ \"$1 $2\" = \"agent list\" ] || exit 1\n/bin/cat \"$QQQ_TEST_HERDR_RESPONSE\"\n").unwrap();
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
    assert_eq!(task["owner_session"], r#"["codex","id","correct"]"#);
    assert_eq!(
        ok(p, &["show", "1"])["herdr"]["identity"]["value"],
        "correct"
    );
    assert_eq!(herdr.ok(&nested, &["next"])["id"], 1);
    herdr.ok(&nested, &["herdr", "link", "1"]);
    herdr.ok(&nested, &["release", "1"]);
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
    assert_eq!(
        herdr.ok(d.path(), &["next"])["owner_session"],
        r#"["codex","id","correct"]"#
    );
}
#[cfg(unix)]
#[test]
fn cwd_lookup_rejects_missing_ambiguous_or_unidentified_matches_before_claim() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Pending"]);
    let herdr = HerdrFixture::new();
    let mut no_identity = agent_at(p, "unknown");
    no_identity.as_object_mut().unwrap().remove("agent_session");
    let cases = [
        (serde_json::json!([]), "No Herdr agent matches"),
        (
            serde_json::json!([agent_at(p, "a"), agent_at(p, "b")]),
            "Multiple Herdr agents match",
        ),
        (
            serde_json::json!([no_identity.clone()]),
            "no agent session identity",
        ),
        (
            serde_json::json!([agent_at(p, "a"), no_identity]),
            "Multiple Herdr agents match",
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
        assert_eq!(ok(p, &["show", "1"])["task"]["status"], "pending");
    }
    // Explicit identity bypasses ambiguous Herdr discovery.
    assert_eq!(
        herdr.ok(p, &["next", "--session", "explicit"])["owner_session"],
        "explicit"
    );
}
