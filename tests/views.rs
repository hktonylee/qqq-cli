use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_qqq"));
    c.current_dir(dir)
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    c
}
fn run(dir: &Path, args: &[&str]) -> Output {
    command(dir).arg("--json").args(args).output().unwrap()
}
fn ok(dir: &Path, args: &[&str]) -> Value {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}
fn ids(tasks: &Value) -> Vec<i64> {
    tasks
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_i64().unwrap())
        .collect()
}

#[test]
fn list_and_next_share_unicode_query_and_readiness() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(
        p,
        &["add", "ÉCOLE blocked", "--parent", "1", "--tag", "backend"],
    );
    ok(p, &["add", "ÉCOLE ready", "--tag", "backend"]);
    assert_eq!(
        ids(&ok(
            p,
            &["list", "--query", "école", "--readiness", "ready"]
        )),
        [3]
    );
    assert_eq!(
        ok(
            p,
            &[
                "next",
                "--dry-run",
                "--query",
                "école",
                "--tag",
                "backend",
                "--status",
                "new"
            ]
        )["id"],
        3
    );
    assert_eq!(
        ids(&ok(
            p,
            &["list", "--query", "école", "--readiness", "blocked"]
        )),
        [1, 2]
    );
    assert!(ok(p, &["next", "--dry-run", "--readiness", "blocked"]).is_null());
    assert_eq!(
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                "owner",
                "--query",
                "école",
                "--tag",
                "backend"
            ]
        )["id"],
        3
    );
    assert_eq!(
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                "owner",
                "--query",
                "absent",
                "--status",
                "completed"
            ]
        )["id"],
        3
    );
}

#[test]
fn blocked_selection_uses_parent_and_prerequisites_excludes_archives() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Prerequisite"]);
    ok(p, &["add", "Blocked", "--depends-on", "1"]);
    ok(p, &["add", "Archived", "--depends-on", "1"]);
    ok(p, &["archive", "3"]);
    assert_eq!(
        ids(&ok(
            p,
            &["list", "--readiness", "blocked", "--include-archived"]
        )),
        [2]
    );
    ok(
        p,
        &[
            "next",
            "--local",
            "--session",
            "parent",
            "--filter",
            "id == 1",
        ],
    );
    ok(p, &["complete", "1", "--session", "parent"]);
    assert!(
        ok(p, &["list", "--readiness", "blocked", "--include-archived"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(ids(&ok(p, &["list", "--readiness", "ready"])), [2]);
}

#[test]
fn named_views_persist_manage_and_discover_from_nested_directories() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Task", "--tag", "backend"]);
    let db_before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    assert_eq!(ok(p, &["view", "list"]), json!([]));
    assert!(!p.join(".qqq-views.json").exists());
    let saved = ok(
        p,
        &[
            "view",
            "save",
            "Ready backend",
            "--tag",
            " backend ",
            "--readiness",
            "ready",
            "--max-completed",
            "0",
        ],
    );
    assert_eq!(saved["criteria"]["tags"], json!(["backend"]));
    let catalog: Value =
        serde_json::from_slice(&std::fs::read(p.join(".qqq-views.json")).unwrap()).unwrap();
    assert_eq!(catalog["version"], 1);
    assert_eq!(catalog["views"], json!([saved]));
    assert_eq!(ok(p, &["view", "show", "Ready backend"]), saved);
    std::fs::create_dir(p.join("nested")).unwrap();
    assert_eq!(
        ids(&ok(&p.join("nested"), &["list", "--view", "Ready backend"])),
        [1]
    );
    assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), db_before);
    ok(p, &["view", "save", "Errors", "--status", "error"]);
    assert_eq!(ok(p, &["view", "list"])[0]["name"], "Errors");
    ok(p, &["view", "save", "Ready backend", "--tag", "UI"]);
    assert_eq!(
        ok(p, &["view", "show", "Ready backend"])["criteria"]["tags"],
        json!(["UI"])
    );
    ok(p, &["view", "remove", "Errors"]);
    assert_eq!(ok(p, &["view", "list"]).as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), db_before);
}

#[test]
fn saved_and_explicit_selectors_use_and_with_visibility_overrides() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(
        p,
        &[
            "add",
            "ÉCOLE bug",
            "--parent",
            "1",
            "--tag",
            "backend",
            "--tag",
            "bug",
            "--priority",
            "5",
        ],
    );
    ok(
        p,
        &["add", "ÉCOLE other", "--tag", "backend", "--priority", "5"],
    );
    ok(
        p,
        &[
            "add",
            "ÉCOLE bug archived",
            "--tag",
            "backend",
            "--tag",
            "bug",
            "--priority",
            "5",
        ],
    );
    ok(p, &["archive", "4"]);
    ok(
        p,
        &[
            "view",
            "save",
            "School",
            "--tag",
            "backend",
            "--query",
            "école",
            "--filter",
            "priority",
            "--status",
            "new",
            "--status",
            "error",
            "--include-archived",
            "--all",
        ],
    );
    assert_eq!(
        ids(&ok(
            p,
            &[
                "list",
                "--view",
                "School",
                "--tag",
                "bug",
                "--query",
                "bug",
                "--status",
                "new",
                "--filter",
                "priority >= 5"
            ]
        )),
        [1, 2, 4]
    );
    assert_eq!(
        ids(&ok(
            p,
            &[
                "list",
                "--view",
                "School",
                "--tag",
                "bug",
                "--hide-archived"
            ]
        )),
        [1, 2]
    );
    assert!(
        ok(p, &["list", "--view", "School", "--status", "completed"])
            .as_array()
            .unwrap()
            .is_empty()
    );
    ok(
        p,
        &[
            "next",
            "--local",
            "--session",
            "parent",
            "--filter",
            "id == 1",
        ],
    );
    ok(p, &["complete", "1", "--session", "parent"]);
    assert_eq!(
        ids(&ok(
            p,
            &[
                "list",
                "--view",
                "School",
                "--tag",
                "bug",
                "--max-completed",
                "0",
                "--hide-archived"
            ]
        )),
        [2]
    );
    ok(
        p,
        &["view", "save", "Hidden complete", "--max-completed", "0"],
    );
    assert_eq!(ids(&ok(p, &["list", "--view", "Hidden complete"])), [2, 3]);
    assert_eq!(
        ids(&ok(p, &["list", "--view", "Hidden complete", "--all"])),
        [1, 2, 3]
    );
    assert_eq!(
        ok(
            p,
            &["next", "--dry-run", "--view", "School", "--tag", "bug"]
        )["id"],
        2
    );
}

#[test]
fn unknown_and_malformed_views_fail_without_opening_or_changing_db() {
    let dir = project();
    let p = dir.path();
    let db = p.join(".qqq/qqq.db");
    std::fs::write(&db, b"sentinel, not a DB").unwrap();
    for mode in [
        vec!["list"],
        vec!["list", "--watch"],
        vec!["next", "--wait"],
        vec!["next", "--dry-run"],
        vec!["next", "--explain"],
        vec!["tui"],
    ] {
        let mut args = mode;
        args.extend(["--view", "missing"]);
        let out = run(p, &args);
        assert!(!out.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
            "INVALID_ARGUMENT"
        );
        assert_eq!(std::fs::read(&db).unwrap(), b"sentinel, not a DB");
    }
    for content in [
        r#"{"version":2,"views":[]}"#,
        r#"{"version":1,"version":1,"views":[]}"#,
        r#"{"version":1,"views":[],"unknown":true}"#,
        r#"{"version":1,"views":[{"name":"same"},{"name":"same"}]}"#,
        r#"{"version":1,"views":[{"name":"bad","criteria":{"tags":[""]}}]}"#,
        r#"{"version":1,"views":[{"name":"bad","criteria":{"filter":"has_tag('')"}}]}"#,
        r#"{"version":1,"views":[{"name":"bad","criteria":{"statuses":["invalid"]}}]}"#,
        r#"{"version":1,"views":[{"name":"bad","max_completed":-1}]}"#,
        r#"{"version":1,"views":[{"name":"bad","criteria":{"unknown":1}}]}"#,
    ] {
        std::fs::write(p.join(".qqq-views.json"), content).unwrap();
        let out = run(p, &["next", "--local", "--view", "bad"]);
        assert!(!out.status.success(), "{content}");
        let error: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["code"], "CONFIG_ERROR", "{content}: {error}");
        assert_eq!(std::fs::read(&db).unwrap(), b"sentinel, not a DB");
    }
}

#[test]
fn concurrent_view_saves_preserve_independent_definitions_and_db_bytes() {
    let dir = project();
    let p = dir.path();
    let before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    let children: Vec<_> = ["One", "Two", "Three", "四"]
        .into_iter()
        .map(|name| {
            command(p)
                .args(["--json", "view", "save", name, "--tag", name])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let views = ok(p, &["view", "list"]);
    assert_eq!(views.as_array().unwrap().len(), 4);
    assert_eq!(
        views
            .as_array()
            .unwrap()
            .iter()
            .map(|view| view["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["One", "Three", "Two", "四"]
    );
    assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
}

#[test]
fn view_browsing_is_read_only_invalid_saves_preserve_catalog() {
    let dir = project();
    let p = dir.path();
    ok(p, &["view", "save", "UI", "--tag", "UI"]);
    let path = p.join(".qqq-views.json");
    let before = std::fs::read(&path).unwrap();
    ok(p, &["view", "list"]);
    ok(p, &["view", "show", "UI"]);
    ok(p, &["list", "--view", "UI"]);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    for args in [
        vec!["view", "save", "UI", "--tag", ""],
        vec!["view", "save", "UI", "--filter", "unknown == 1"],
        vec!["view", "save", " bad "],
        vec!["view", "remove", "missing"],
    ] {
        assert!(!run(p, &args).status.success());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        ok(p, &["view", "save", "Other"]);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }
}

#[test]
fn oversized_catalog_reads_and_writes_fail_without_replacing_source() {
    let dir = project();
    let p = dir.path();
    let path = p.join(".qqq-views.json");
    let source = json!({"version":1,"views":[{"name":"x".repeat(1_048_576-250)}]}).to_string();
    assert!(source.len() < 1_048_576);
    std::fs::write(&path, &source).unwrap();
    let out = run(p, &["view", "save", "Another"]);
    assert!(!out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    let oversized = " ".repeat(1_048_577);
    std::fs::write(&path, &oversized).unwrap();
    let out = run(p, &["view", "list"]);
    assert!(!out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
        "CONFIG_ERROR"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), oversized);
}

struct Waiter(Option<Child>);
impl Drop for Waiter {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn named_view_wait_and_preview_wait_recheck_readiness_and_explicit_tags() {
    for preview in [false, true] {
        let dir = project();
        let p = dir.path();
        ok(
            p,
            &[
                "view",
                "save",
                "Worker",
                "--tag",
                "UI",
                "--readiness",
                "ready",
            ],
        );
        ok(p, &["add", "Prerequisite", "--priority", "100"]);
        ok(
            p,
            &[
                "add",
                "Blocked",
                "--tag",
                "UI",
                "--tag",
                "bug",
                "--depends-on",
                "1",
            ],
        );
        let mut cmd = command(p);
        cmd.args([
            "--json",
            "next",
            "--local",
            "--wait",
            "--session",
            "worker",
            "--view",
            "Worker",
            "--tag",
            "bug",
        ]);
        if preview {
            cmd.arg("--dry-run");
        }
        let mut waiter = Waiter(Some(
            cmd.stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ));
        thread::sleep(Duration::from_millis(400));
        assert!(waiter.0.as_mut().unwrap().try_wait().unwrap().is_none());
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                "prerequisite",
                "--filter",
                "id == 1",
            ],
        );
        ok(p, &["complete", "1", "--session", "prerequisite"]);
        let deadline = Instant::now() + Duration::from_secs(5);
        while waiter.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "named view waiter did not return"
            );
            thread::sleep(Duration::from_millis(25));
        }
        let out = waiter.0.take().unwrap().wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let task: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(task["id"], 2);
        assert_eq!(task["status"], if preview { "new" } else { "in_progress" });
        assert_eq!(ok(p, &["show", "2"])["task"]["status"], task["status"]);
    }
}

#[test]
fn named_views_preserve_atomic_claims_priority_and_owned_reuse() {
    let dir = project();
    let p = dir.path();
    ok(
        p,
        &[
            "view",
            "save",
            "Worker",
            "--tag",
            "UI",
            "--status",
            "new",
            "--readiness",
            "ready",
        ],
    );
    ok(
        p,
        &[
            "view",
            "save",
            "Other",
            "--tag",
            "absent",
            "--status",
            "completed",
        ],
    );
    ok(p, &["add", "Partial", "--tag", "UI", "--priority", "100"]);
    ok(
        p,
        &[
            "add",
            "Lower",
            "--tag",
            "UI",
            "--tag",
            "bug",
            "--priority",
            "1",
        ],
    );
    ok(
        p,
        &[
            "add",
            "Higher",
            "--tag",
            "UI",
            "--tag",
            "bug",
            "--priority",
            "5",
        ],
    );
    assert_eq!(
        ok(
            p,
            &["next", "--dry-run", "--view", "Worker", "--tag", "bug"]
        )["id"],
        3
    );
    let children: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|owner| {
            command(p)
                .args([
                    "--json",
                    "next",
                    "--local",
                    "--session",
                    owner,
                    "--view",
                    "Worker",
                    "--tag",
                    "bug",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut claimed = Vec::new();
    let mut empty = 0;
    for (owner, child) in ["a", "b", "c"].into_iter().zip(children) {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let task: Value = serde_json::from_slice(&out.stdout).unwrap();
        if task.is_null() {
            empty += 1;
        } else {
            let id = task["id"].as_i64().unwrap();
            claimed.push(id);
            assert_eq!(
                ok(
                    p,
                    &["next", "--local", "--session", owner, "--view", "Other"]
                )["id"],
                id
            );
        }
    }
    claimed.sort_unstable();
    assert_eq!(claimed, [2, 3]);
    assert_eq!(empty, 1);
    assert_eq!(ok(p, &["show", "1"])["task"]["status"], "new");
}
