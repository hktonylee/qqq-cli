#![cfg(unix)]
#[path = "common/next_output.rs"]
mod next_output;

use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;

struct Project(TempDir);

impl Project {
    fn new(config: &str) -> Self {
        let p = Self(TempDir::new().unwrap());
        let root = p.0.path();
        fs::create_dir_all(root.join(".config/qqq")).unwrap();
        fs::write(root.join(".config/qqq/config.toml"), config).unwrap();
        fs::write(
            root.join("herdr"),
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> calls\nexit 1\n",
        )
        .unwrap();
        fs::set_permissions(root.join("herdr"), fs::Permissions::from_mode(0o755)).unwrap();
        p.ok(&["init"]);
        p
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
        command
            .current_dir(self.0.path())
            .arg("--json")
            .env("HOME", self.0.path())
            .env("PATH", self.0.path())
            .env("HERDR_ENV", "1")
            .env("HERDR_PANE_ID", "caller")
            .env("HERDR_WORKSPACE_ID", "workspace")
            .env_remove("HERDR_SOCKET_PATH")
            .env_remove("QQQ_SESSION")
            .env_remove("CODEX_THREAD_ID")
            .env_remove("CODEX_SESSION_ID");
        command
    }

    fn ok(&self, args: &[&str]) -> Value {
        let output = self.command().args(args).output().unwrap();
        if args.first() == Some(&"next") {
            return next_output::json(output);
        }
        assert!(output.status.success(), "{args:?}: {output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn assert_no_herdr(&self) {
        assert!(!self.0.path().join("calls").exists());
    }
}

#[test]
fn legacy_dispatch_configuration_never_spawns_or_blocks_local_claims() {
    for config in [
        "[herdr]\nnext-to-new-agent = true\n",
        "[herdr]\nnext-to-new-agent = 'wrong'\n",
        "invalid = [",
    ] {
        let p = Project::new(config);
        p.ok(&["add", "Task"]);
        let task = p.ok(&["next", "--session", "worker"]);
        assert_eq!(task["id"], 1);
        assert_eq!(task["harness_session"], "worker");
        assert!(task["orchestrator_session"].is_null());
        assert_eq!(p.ok(&["next", "--session", "worker"]), task);
        p.assert_no_herdr();
    }
}

#[test]
fn local_flag_remains_compatible_with_normal_claims() {
    let p = Project::new("[herdr]\nnext-to-new-agent = true\n");
    p.ok(&["add", "First"]);
    p.ok(&["add", "Second"]);
    assert_eq!(p.ok(&["next", "--local", "--session", "worker"])["id"], 1);
    p.ok(&["edit", "1", "--complete", "--session", "worker"]);
    assert_eq!(p.ok(&["next", "--session", "worker"])["id"], 2);
    p.assert_no_herdr();
}

#[test]
fn wait_claims_arriving_work_for_the_calling_worker() {
    let p = Project::new("[herdr]\nnext-to-new-agent = true\n");
    let mut child = p
        .command()
        .args(["next", "--wait", "--session", "worker"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert!(child.try_wait().unwrap().is_none());
    p.assert_no_herdr();
    p.ok(&["add", "Arrived"]);
    let started = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("waiting worker did not claim arriving work");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let task = next_output::json(child.wait_with_output().unwrap());
    assert_eq!(task["harness_session"], "worker");
    assert_eq!(task["id"], 1);
    p.assert_no_herdr();
}
