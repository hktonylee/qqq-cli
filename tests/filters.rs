use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir)
        .arg("--json")
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    command
}
fn run(dir: &Path, args: &[&str]) -> Output {
    command(dir).args(args).output().unwrap()
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
fn list_combines_luau_search_status_and_parent_context() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Root"]);
    ok(
        p,
        &[
            "add",
            "Auth details\nsecond line",
            "--parent",
            "1",
            "--priority",
            "5",
        ],
    );
    ok(p, &["add", "Auth low"]);
    let tasks = ok(
        p,
        &[
            "list",
            "--filter",
            "like(task_name, '%details%') and priority > 0",
            "--query",
            "SECOND",
            "--status",
            "new",
        ],
    );
    assert_eq!(ids(&tasks), [1, 2]);
    assert_eq!(tasks[0]["context_only"], true);
    assert!(tasks[1].get("context_only").is_none());
    assert_eq!(
        ok(
            p,
            &[
                "list",
                "--filter",
                "created_time == created_at and updated_time == updated_at"
            ]
        )
        .as_array()
        .unwrap()
        .len(),
        3
    );
    assert!(
        ok(
            p,
            &["list", "--filter", "priority > 0", "--status", "completed"]
        )
        .as_array()
        .unwrap()
        .is_empty()
    );
    let human = Command::new(env!("CARGO_BIN_EXE_qqq"))
        .arg("--human")
        .current_dir(p)
        .env("HOME", p)
        .args(["list", "--filter", "false"])
        .output()
        .unwrap();
    assert!(human.status.success());
    assert_eq!(
        String::from_utf8(human.stdout).unwrap(),
        "No matching tasks.\n"
    );
}

#[test]
fn archive_and_completed_limits_still_bound_context() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["next", "--local", "--session", "parent"]);
    ok(p, &["complete", "1", "--session", "parent"]);
    ok(p, &["add", "Child", "--parent", "1"]);
    ok(p, &["add", "Archived", "--priority", "9"]);
    ok(p, &["archive", "3"]);
    let tasks = ok(p, &["list", "--filter", "id == 2", "--max-completed", "0"]);
    assert_eq!(ids(&tasks), [2]);
    assert!(tasks[0].get("context_only").is_none());
    assert_eq!(
        ids(&ok(p, &["list", "--filter", "archived"])),
        Vec::<i64>::new()
    );
    assert_eq!(
        ids(&ok(
            p,
            &["list", "--filter", "archived", "--include-archived"]
        )),
        [3]
    );
}

#[test]
fn invalid_filter_fails_before_database_open_or_claim_changes() {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    for subcommand in ["list", "next"] {
        let out = run(p, &[subcommand, "--filter", "os.execute('bad')"]);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("Invalid --filter"));
        assert!(!p.join(".qqq").exists());
    }
    ok(p, &["init"]);
    ok(p, &["add", "Task"]);
    let before = ok(p, &["show", "1"]);
    let out = run(
        p,
        &[
            "next",
            "--local",
            "--session",
            "a",
            "--filter",
            "like(id, '%')",
        ],
    );
    assert!(!out.status.success());
    assert_eq!(ok(p, &["show", "1"]), before);
}

#[test]
fn runtime_filter_error_rolls_back_new_claim_and_owned_task_bypasses_predicate() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Task"]);
    let before = ok(p, &["show", "1"]);
    let filter = "like(description, '%', 'xx')";
    let output = run(
        p,
        &["next", "--local", "--session", "a", "--filter", filter],
    );
    assert!(!output.status.success());
    let failure: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure["code"], "INVALID_FILTER");
    assert_eq!(failure["details"]["reason"], "evaluation_failed");
    assert_eq!(ok(p, &["show", "1"]), before);
    assert_eq!(ok(p, &["next", "--local", "--session", "a"])["id"], 1);
    assert_eq!(
        ok(
            p,
            &["next", "--local", "--session", "a", "--filter", filter]
        )["id"],
        1
    );
}

#[test]
fn next_filters_only_new_ready_candidates_keeps_owned_task() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Other", "--priority", "100"]);
    ok(p, &["add", "Auth first", "--priority", "5"]);
    ok(p, &["add", "Auth second", "--priority", "5"]);
    ok(
        p,
        &["add", "Auth blocked", "--parent", "1", "--priority", "90"],
    );
    ok(p, &["add", "Auth archived", "--priority", "99"]);
    ok(p, &["archive", "5"]);
    let filter = "like(description, 'Auth%')";
    assert_eq!(
        ok(
            p,
            &["next", "--local", "--session", "a", "--filter", filter]
        )["id"],
        2
    );
    assert_eq!(
        ok(
            p,
            &["next", "--local", "--session", "a", "--filter", "false"]
        )["id"],
        2
    );
    assert_eq!(
        ok(
            p,
            &["next", "--local", "--session", "b", "--filter", filter]
        )["id"],
        3
    );
    assert!(
        ok(
            p,
            &["next", "--local", "--session", "c", "--filter", filter]
        )
        .is_null()
    );
    assert_eq!(
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                "parent",
                "--filter",
                "id == 1"
            ]
        )["id"],
        1
    );
    ok(p, &["complete", "1", "--session", "parent"]);
    assert_eq!(
        ok(
            p,
            &["next", "--local", "--session", "c", "--filter", filter]
        )["id"],
        4
    );
}

#[test]
fn concurrent_filtered_claims_are_distinct_and_leave_unmatched_tasks_new() {
    let dir = project();
    let p = dir.path();
    for description in ["Other", "Match A", "Match B"] {
        ok(p, &["add", description]);
    }
    let children: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|owner| {
            command(p)
                .args([
                    "next",
                    "--local",
                    "--session",
                    owner,
                    "--filter",
                    "like(description, 'Match%')",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut claimed = Vec::new();
    let mut empty = 0;
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let task: Value = serde_json::from_slice(&output.stdout).unwrap();
        if task.is_null() {
            empty += 1;
        } else {
            claimed.push(task["id"].as_i64().unwrap());
        }
    }
    claimed.sort();
    assert_eq!(claimed, [2, 3]);
    assert_eq!(empty, 1);
    assert_eq!(ok(p, &["show", "1"])["task"]["status"], "new");
}

#[test]
fn wait_ignores_unmatched_commits_and_claims_later_match() {
    let dir = project();
    let p = dir.path();
    let mut child = command(p)
        .args([
            "next",
            "--local",
            "--wait",
            "--session",
            "waiter",
            "--filter",
            "priority > 0",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    ok(p, &["add", "Unmatched"]);
    thread::sleep(Duration::from_millis(400));
    assert!(child.try_wait().unwrap().is_none());
    ok(p, &["add", "Matched", "--priority", "3"]);
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("filtered waiter did not claim matching task");
        }
        thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["id"],
        2
    );
    assert_eq!(ok(p, &["show", "1"])["task"]["status"], "new");
}
