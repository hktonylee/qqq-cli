use serde_json::Value;
use std::{
    path::Path,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    command
}

fn ok(dir: &Path, args: &[&str]) -> Value {
    let output = command(dir).arg("--json").args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

// Reap waiting children even if an assertion fails, so tests cannot leak workers.
struct Waiter(Option<Child>);

impl Waiter {
    fn spawn(dir: &Path, session: &str, json: bool) -> Self {
        let mut command = command(dir);
        if json {
            command.arg("--json");
        }
        Self(Some(
            command
                .args(["next", "--wait", "--session", session])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ))
    }

    fn exited(&mut self) -> bool {
        self.0.as_mut().unwrap().try_wait().unwrap().is_some()
    }

    fn assert_waiting(&mut self) {
        // Observe multiple poll intervals: no ready task must leave process alive.
        let until = Instant::now() + Duration::from_millis(600);
        while Instant::now() < until {
            assert!(!self.exited(), "waiter exited without a ready task");
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn finish(mut self) -> Output {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.exited() {
            assert!(
                Instant::now() < deadline,
                "waiter did not return ready task"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let output = self.0.take().unwrap().wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn task(self) -> Value {
        serde_json::from_slice(&self.finish().stdout).unwrap()
    }
}

#[test]
fn wait_skips_error_until_user_retries() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Task"]);
    ok(path, &["next", "--local", "--session", "worker"]);
    ok(
        path,
        &[
            "edit",
            "1",
            "--set-status",
            "error",
            "--reason",
            "Needs input",
            "--session",
            "worker",
        ],
    );
    let mut waiter = Waiter::spawn(path, "worker", true);
    waiter.assert_waiting();
    ok(path, &["edit", "1", "--set-status", "new"]);
    assert_eq!(waiter.task()["id"], 1);
}

#[test]
fn wait_claims_child_after_user_clears_parent() {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Parent"]);
    ok(path, &["add", "Child", "--parent", "1"]);
    ok(path, &["next", "--local", "--session", "parent"]);
    let mut waiter = Waiter::spawn(path, "child", true);
    waiter.assert_waiting();
    ok(path, &["edit", "2", "--set-parent", "none"]);
    assert_eq!(waiter.task()["id"], 2);
}

impl Drop for Waiter {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[test]
fn wait_dry_run_returns_unclaimed_incoming_task() {
    let dir = project();
    let p = dir.path();
    let mut waiter = Waiter(Some(
        command(p)
            .arg("--json")
            .args(["next", "--wait", "--dry-run"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    ));
    waiter.assert_waiting();
    ok(p, &["add", "Incoming"]);
    let preview = waiter.task();
    assert_eq!(preview["status"], "new");
    assert!(preview["harness_session"].is_null());
    assert!(
        ok(p, &["show", "1"])["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        ok(p, &["next", "--local", "--session", "worker"])["id"],
        preview["id"]
    );
}

#[test]
fn concurrent_waiting_previews_skip_owned_and_blocked_tasks_without_reserving() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["next", "--local", "--session", "parent"]);
    ok(p, &["add", "Child", "--parent", "1"]);
    let mut waiters: Vec<_> = (0..2)
        .map(|_| {
            Waiter(Some(
                command(p)
                    .arg("--json")
                    .args([
                        "next",
                        "--wait",
                        "--dry-run",
                        "--session",
                        "parent",
                        "--filter",
                        "id == 2",
                    ])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            ))
        })
        .collect();
    for waiter in &mut waiters {
        waiter.assert_waiting();
    }
    ok(p, &["complete", "1", "--session", "parent"]);
    let first = waiters.pop().unwrap().task();
    let second = waiters.pop().unwrap().task();
    assert_eq!(first, second);
    assert_eq!(first["id"], 2);
    assert_eq!(first["status"], "new");
    assert!(
        ok(p, &["show", "2"])["events"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(ok(p, &["next", "--local", "--session", "worker"])["id"], 2);
}

#[test]
fn wait_returns_ready_or_already_owned_task_immediately() {
    let dir = project();
    let p = dir.path();
    assert!(ok(p, &["next", "--session", "worker"]).is_null());
    ok(p, &["add", "First"]);
    let task = Waiter::spawn(p, "worker", true).task();
    assert_eq!(task["id"], 1);
    assert_eq!(task["harness_session"], "worker");
    assert_eq!(task["status"], "in_progress");
    assert_eq!(Waiter::spawn(p, "worker", true).task(), task);
    assert_eq!(ok(p, &["show", "1"])["events"].as_array().unwrap().len(), 1);
}

#[test]
fn wait_from_nested_directory_returns_task_added_later_as_single_json() {
    let dir = project();
    let p = dir.path();
    let nested = p.join("nested");
    std::fs::create_dir(&nested).unwrap();
    let mut waiter = Waiter::spawn(&nested, "worker", true);
    waiter.assert_waiting();
    ok(p, &["add", "Arrived later\n\nDetails"]);
    let task = waiter.task();
    assert_eq!(task["id"], 1);
    assert_eq!(task["description"], "Arrived later\n\nDetails");
    assert_eq!(task["harness_session"], "worker");
}

#[test]
fn wait_returns_released_task_in_human_output() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Retry"]);
    ok(p, &["next", "--session", "original"]);
    let mut waiter = Waiter::spawn(p, "worker", false);
    waiter.assert_waiting();
    ok(
        p,
        &["edit", "1", "--set-status", "new", "--session", "original"],
    );
    let output = String::from_utf8(waiter.finish().stdout).unwrap();
    assert!(output.contains("#1"));
    assert!(output.contains("Retry"));
    assert!(output.contains("Status: In progress"));
    for label in [
        "Harness name:",
        "Harness session:",
        "Orchestrator name:",
        "Orchestrator session:",
    ] {
        assert!(!output.contains(label), "{label}: {output}");
    }
    assert!(!output.contains("No ready tasks."));
    assert_eq!(ok(p, &["show", "1"])["task"]["harness_session"], "worker");
}

#[test]
fn wait_skips_blocked_child_until_parent_completes() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Child", "--parent", "1"]);
    ok(p, &["next", "--session", "parent-owner"]);
    let mut waiter = Waiter::spawn(p, "worker", true);
    waiter.assert_waiting();
    assert_eq!(ok(p, &["show", "2"])["task"]["status"], "new");
    ok(p, &["complete", "1", "--session", "parent-owner"]);
    assert_eq!(waiter.task()["id"], 2);
}

#[test]
fn concurrent_waiters_claim_each_arrival_once_while_others_keep_waiting() {
    let dir = project();
    let p = dir.path();
    let mut waiters: Vec<_> = (0..4)
        .map(|i| {
            (
                format!("worker-{i}"),
                Some(Waiter::spawn(p, &format!("worker-{i}"), true)),
            )
        })
        .collect();
    waiters[0].1.as_mut().unwrap().assert_waiting();
    for id in 1..=4 {
        ok(p, &["add", &format!("Task {id}")]);
        let deadline = Instant::now() + Duration::from_secs(5);
        let winner = loop {
            if let Some(index) = waiters
                .iter_mut()
                .position(|(_, waiter)| waiter.as_mut().is_some_and(Waiter::exited))
            {
                break index;
            }
            assert!(Instant::now() < deadline, "no waiter claimed arrival");
            thread::sleep(Duration::from_millis(20));
        };
        let task = waiters[winner].1.take().unwrap().task();
        assert_eq!(task["id"], id);
        assert_eq!(task["harness_session"], waiters[winner].0);
        let history = ok(p, &["show", &id.to_string()]);
        assert_eq!(history["events"].as_array().unwrap().len(), 1);
        assert_eq!(history["events"][0]["action"], "claim");
        assert_eq!(history["events"][0]["session"], waiters[winner].0);
        for (_, waiter) in &mut waiters {
            if let Some(waiter) = waiter {
                waiter.assert_waiting();
            }
        }
    }
}

#[test]
fn waiting_can_be_cancelled_without_leaving_claims_or_database_lock() {
    let dir = project();
    let p = dir.path();
    let mut waiter = Waiter::spawn(p, "cancelled", true);
    waiter.assert_waiting();
    drop(waiter);
    ok(p, &["add", "After cancellation"]);
    assert_eq!(ok(p, &["next", "--session", "other"])["id"], 1);
    let history = ok(p, &["show", "1"]);
    assert_eq!(history["events"].as_array().unwrap().len(), 1);
    assert_eq!(history["events"][0]["session"], "other");
}

#[test]
fn wait_still_fails_immediately_for_invalid_session_or_missing_database() {
    let dir = project();
    let output = command(dir.path())
        .args(["next", "--wait", "--session", " "])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let dir = TempDir::new().unwrap();
    let output = command(dir.path())
        .args(["next", "--wait", "--session", "worker"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No .qqq directory found"));
}

#[test]
fn native_codex_wait_claims_incoming_task_without_herdr() {
    let dir = project();
    let p = dir.path();
    let mut waiter = Waiter(Some(
        command(p)
            .env("HOME", p)
            .env("PATH", p)
            .env("CODEX_THREAD_ID", "native-wait")
            .args(["next", "--wait", "--local", "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    ));
    waiter.assert_waiting();
    ok(p, &["add", "Arrived"]);
    let output = waiter.finish();
    let task: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(task["harness_name"], "codex");
    assert_eq!(task["harness_session"], "native-wait");
}
