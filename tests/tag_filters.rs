use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    command
}
fn run(dir: &Path, args: &[&str]) -> Output {
    command(dir).arg("--json").args(args).output().unwrap()
}
fn ok(dir: &Path, args: &[&str]) -> Value {
    let output = run(dir, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
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
fn tag_selectors_require_all_exact_labels() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "[frontend] Text only"]);
    ok(p, &["add", "Substring", "--tag", "frontend-extra"]);
    ok(p, &["add", "Case", "--tag", "Frontend"]);
    ok(p, &["add", "Single", "--tag", "frontend"]);
    ok(p, &["add", "Match", "--tag", "frontend", "--tag", "界 面"]);
    assert_eq!(
        ids(&ok(
            p,
            &[
                "list",
                "--tag",
                " frontend ",
                "--tag",
                "界 面",
                "--tag",
                "frontend"
            ]
        )),
        [5]
    );
    assert_eq!(ids(&ok(p, &["list", "--tag", "Frontend"])), [3]);
    assert_eq!(
        ids(&ok(p, &["list", "--filter", "has_tag('frontend')"])),
        [4, 5]
    );
    assert_eq!(
        ok(
            p,
            &["next", "--dry-run", "--tag", "frontend", "--tag", "界 面"]
        )["id"],
        5
    );
    let label = "' OR 1=1 --";
    ok(p, &["add", "Literal", "--tag", label]);
    assert_eq!(ids(&ok(p, &["list", "--tag", label])), [6]);
    ok(p, &["add", "Composed", "--tag", "é"]);
    ok(p, &["add", "Decomposed", "--tag", "e\u{301}"]);
    assert_eq!(ids(&ok(p, &["list", "--tag", "é"])), [7]);
    assert_eq!(ids(&ok(p, &["list", "--tag", "e\u{301}"])), [8]);
}

#[test]
fn tag_lists_compose_with_filters_and_retain_visible_context() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(
        p,
        &[
            "add",
            "Layout detail",
            "--parent",
            "1",
            "--priority",
            "5",
            "--tag",
            "UI",
        ],
    );
    ok(p, &["add", "Layout low", "--tag", "UI"]);
    ok(
        p,
        &["add", "Layout archived", "--tag", "UI", "--priority", "9"],
    );
    ok(p, &["archive", "4"]);
    let tasks = ok(
        p,
        &[
            "list",
            "--tag",
            "UI",
            "--query",
            "DETAIL",
            "--status",
            "new",
            "--filter",
            "priority >= 5",
        ],
    );
    assert_eq!(ids(&tasks), [1, 2]);
    assert_eq!(tasks[0]["context_only"], true);
    assert!(tasks[1].get("context_only").is_none());
    assert_eq!(ids(&ok(p, &["list", "--tag", "UI"])), [1, 2, 3]);
    assert_eq!(
        ids(&ok(p, &["list", "--tag", "UI", "--include-archived"])),
        [1, 2, 3, 4]
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
        ids(&ok(p, &["list", "--tag", "UI", "--max-completed", "0"])),
        [2, 3]
    );
    assert!(
        ok(p, &["list", "--tag", "UI", "--status", "completed"])
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn tag_claims_preserve_readiness_order_and_owned_reuse() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Prerequisite"]);
    ok(p, &["add", "[frontend] Text only", "--priority", "100"]);
    ok(
        p,
        &[
            "add",
            "Blocked",
            "--parent",
            "1",
            "--depends-on",
            "2",
            "--tag",
            "frontend",
            "--priority",
            "90",
        ],
    );
    ok(
        p,
        &["add", "Archived", "--tag", "frontend", "--priority", "99"],
    );
    ok(p, &["archive", "5"]);
    ok(p, &["add", "First", "--tag", "frontend", "--priority", "5"]);
    ok(
        p,
        &["add", "Second", "--tag", "frontend", "--priority", "5"],
    );
    let report = ok(p, &["next", "--explain", "--tag", "frontend"]);
    assert_eq!(report["explanation"]["eligible_ids"], json!([6, 7]));
    assert_eq!(
        ok(
            p,
            &["next", "--local", "--session", "a", "--tag", "frontend"]
        )["id"],
        6
    );
    let before = ok(p, &["show", "6"]);
    assert_eq!(
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                "a",
                "--tag",
                "absent",
                "--filter",
                "false"
            ]
        )["id"],
        6
    );
    assert_eq!(ok(p, &["show", "6"]), before);
    let report = ok(
        p,
        &["next", "--explain", "--session", "a", "--tag", "absent"],
    );
    assert_eq!(report["explanation"]["outcome"], "owned_task_reuse");
    assert_eq!(report["explanation"]["selection"]["task"]["id"], 6);
    assert_eq!(
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                "b",
                "--filter",
                "has_tag('frontend')"
            ]
        )["id"],
        7
    );
    assert!(ok(p, &["next", "--dry-run", "--tag", "frontend"]).is_null());
    for (id, session) in [("1", "parent"), ("2", "prerequisite")] {
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                session,
                "--filter",
                &format!("id == {id}"),
            ],
        );
        ok(p, &["complete", id, "--session", session]);
        if id == "1" {
            assert!(ok(p, &["next", "--dry-run", "--tag", "frontend"]).is_null());
        }
    }
    assert_eq!(
        ok(
            p,
            &[
                "next",
                "--dry-run",
                "--tag",
                "frontend",
                "--filter",
                "priority > 50"
            ]
        )["id"],
        4
    );
    assert_eq!(
        ok(
            p,
            &["next", "--local", "--session", "c", "--tag", "frontend"]
        )["id"],
        4
    );
    assert_eq!(ok(p, &["show", "3"])["task"]["status"], "new");
}

#[test]
fn explain_distinguishes_tag_exclusion_from_dependency_blockers() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Prerequisite"]);
    ok(
        p,
        &[
            "add",
            "Both excluded and blocked",
            "--parent",
            "1",
            "--depends-on",
            "2",
        ],
    );
    ok(p, &["add", "Tag match", "--tag", "UI"]);
    let report = ok(
        p,
        &[
            "next",
            "--explain",
            "--tag",
            "UI",
            "--filter",
            "priority > 0",
        ],
    );
    assert_eq!(report["state"], "no_matching_ready");
    assert_eq!(
        report["tasks"][2]["reasons"],
        json!([
            "parent_not_completed",
            "prerequisite_not_completed",
            "tag_excluded"
        ])
    );
    assert_eq!(report["tasks"][3]["reasons"], json!(["filter_excluded"]));
    assert_eq!(report["counts"]["matching_ready"], 0);
    assert!(report["explanation"]["selection"].is_null());
}

#[test]
fn invalid_tag_selectors_fail_before_project_or_database_access() {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    for mode in [
        vec!["list"],
        vec!["list", "--watch"],
        vec!["next"],
        vec!["next", "--wait"],
        vec!["next", "--dry-run"],
        vec!["next", "--explain"],
    ] {
        for label in ["", " ", "a,b", "[UI]", "a\nb", "\tUI"] {
            let mut args = mode.clone();
            args.extend(["--tag", label]);
            let output = run(p, &args);
            assert!(!output.status.success(), "{args:?}");
            assert!(output.stdout.is_empty());
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["code"], "INVALID_ARGUMENT", "{args:?}");
            assert_eq!(error["details"]["argument"], "--tag");
            assert!(!p.join(".qqq").exists());
        }
        let mut args = mode;
        args.extend(["--filter", "has_tag('')"]);
        let output = run(p, &args);
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "INVALID_FILTER");
        assert_eq!(error["details"]["argument"], "--filter");
        assert!(!p.join(".qqq").exists());
    }
    std::fs::create_dir(p.join(".qqq")).unwrap();
    let path = p.join(".qqq/qqq.db");
    std::fs::write(&path, b"not a database").unwrap();
    let output = run(p, &["next", "--tag", "bad,tag"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(std::fs::read(path).unwrap(), b"not a database");
}

#[test]
fn tag_selectors_preserve_human_and_agent_default_output() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Task", "--tag=--json"]);
    let human = command(p)
        .args(["--human", "list", "--tag", "absent"])
        .output()
        .unwrap();
    assert!(human.status.success());
    assert_eq!(human.stdout, b"No matching tasks.\n");
    let human = command(p)
        .args(["--human", "next", "--dry-run", "--tag", "absent"])
        .output()
        .unwrap();
    assert!(human.status.success());
    assert_eq!(human.stdout, b"No task available for pickup.\n");
    let human = command(p)
        .args(["--human", "list", "--tag", "--json"])
        .output()
        .unwrap();
    assert!(human.status.success());
    assert!(String::from_utf8_lossy(&human.stdout).contains("[--json] Task"));
    let agent = command(p)
        .env("CODEX_THREAD_ID", "agent")
        .args(["list", "--tag", "--json"])
        .output()
        .unwrap();
    assert!(agent.status.success());
    assert_eq!(ids(&serde_json::from_slice(&agent.stdout).unwrap()), [1]);
    let error = command(p)
        .env("CODEX_THREAD_ID", "agent")
        .args(["next", "--tag", " "])
        .output()
        .unwrap();
    assert!(!error.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&error.stderr).unwrap()["code"],
        "INVALID_ARGUMENT"
    );
    let human = command(p)
        .args(["--human", "next", "--tag", " "])
        .output()
        .unwrap();
    assert!(!human.status.success());
    assert!(String::from_utf8_lossy(&human.stderr).contains("Tags must be nonempty labels"));
}
