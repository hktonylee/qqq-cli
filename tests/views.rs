use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
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
