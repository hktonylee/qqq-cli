use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Output, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::Duration,
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
fn ok(dir: &Path, args: &[&str]) -> Value {
    let out = command(dir).arg("--json").args(args).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

struct Watcher {
    child: Child,
    lines: Receiver<String>,
    reader: Option<JoinHandle<()>>,
}
impl Watcher {
    fn start(dir: &Path, args: &[&str]) -> Self {
        let mut child = command(dir)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if sender.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines,
            reader: Some(reader),
        }
    }
    fn line(&self) -> String {
        self.lines
            .recv_timeout(Duration::from_secs(5))
            .expect("watch snapshot was not flushed")
    }
    fn snapshot(&self) -> Value {
        serde_json::from_str(&self.line()).expect("watch JSON must be one complete value per line")
    }
    fn idle(&mut self) {
        assert!(
            self.lines.recv_timeout(Duration::from_millis(400)).is_err(),
            "unchanged DB was redrawn"
        );
        assert!(
            self.child.try_wait().unwrap().is_none(),
            "watch exited while idle"
        );
    }
}
impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
    }
}

#[test]
fn watch_streams_initial_list_and_committed_changes_without_idle_repeats() {
    let dir = project();
    let p = dir.path();
    let mut watch = Watcher::start(p, &["list", "--watch", "--json"]);
    assert_eq!(watch.snapshot(), serde_json::json!([]));
    watch.idle();
    ok(p, &["add", "First"]);
    assert_eq!(watch.snapshot()[0]["description"], "First");
    ok(p, &["edit", "1", "--description", "Changed"]);
    let changed = watch.snapshot();
    assert_eq!(changed[0]["description"], "Changed");
    ok(p, &["message", "1", "Only message changed"]);
    assert_eq!(
        watch.snapshot(),
        changed,
        "all DB commits should trigger refresh"
    );
    watch.idle();
    ok(p, &["next", "--local", "--session", "worker"]);
    assert_eq!(watch.snapshot()[0]["status"], "in_progress");
    ok(p, &["complete", "1", "--session", "worker"]);
    assert_eq!(watch.snapshot()[0]["status"], "completed");
}

#[test]
fn watch_applies_query_and_status_to_each_json_snapshot() {
    let dir = project();
    let p = dir.path();
    let watch = Watcher::start(
        p,
        &[
            "list", "--watch", "--json", "--query", "needle", "--status", "new",
        ],
    );
    assert_eq!(watch.snapshot(), serde_json::json!([]));
    ok(p, &["add", "Other"]);
    assert_eq!(watch.snapshot(), serde_json::json!([]));
    ok(p, &["add", "Needle task"]);
    let matched = watch.snapshot();
    assert_eq!(matched.as_array().unwrap().len(), 1);
    assert_eq!(matched[0]["description"], "Needle task");
    ok(p, &["next", "--session", "a"]);
    assert_eq!(watch.snapshot()[0]["description"], "Needle task");
    ok(p, &["next", "--session", "b"]);
    assert_eq!(watch.snapshot(), serde_json::json!([]));
}

#[test]
fn watch_human_filter_reports_empty_matches() {
    let dir = project();
    let p = dir.path();
    let watch = Watcher::start(p, &["list", "--watch", "--query", "needle"]);
    assert_eq!(watch.line(), "No matching tasks.");
    ok(p, &["add", "Other"]);
    assert_eq!(watch.line(), "No matching tasks.");
}

#[test]
fn human_watch_handles_tree_alias_and_nested_directory_without_ansi_in_pipes() {
    let dir = project();
    let p = dir.path();
    std::fs::create_dir_all(p.join(".config/qqq")).unwrap();
    std::fs::write(p.join(".config/qqq/config.toml"), "[alias]\nls='list'\n").unwrap();
    let mut watch = Watcher::start(p, &["ls", "--watch"]);
    assert_eq!(watch.line(), "No tasks yet.");
    watch.idle();
    ok(p, &["add", "Parent"]);
    assert!(watch.line().starts_with("ID"));
    let row = watch.line();
    assert!(row.contains("Parent"));
    assert!(!row.contains('\x1b'));
    ok(p, &["add", "Child", "--parent", "1"]);
    assert!(watch.line().starts_with("ID"));
    assert!(watch.line().contains("Parent"));
    let child = watch.line();
    assert!(child.contains("└── Child"), "{child}");
    assert!(!child.contains('\x1b'));
    std::fs::create_dir(p.join("nested")).unwrap();
    let nested = Watcher::start(&p.join("nested"), &["list", "--watch", "--json"]);
    assert_eq!(nested.snapshot().as_array().unwrap().len(), 2);
}

#[test]
fn rollback_does_not_refresh_and_cancelled_watch_leaves_database_unlocked() {
    let dir = project();
    let p = dir.path();
    let mut watch = Watcher::start(p, &["--json", "list", "--watch"]);
    assert_eq!(watch.snapshot(), serde_json::json!([]));
    let conn = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    conn.execute_batch(
        "BEGIN IMMEDIATE; INSERT INTO tasks(description) VALUES ('Rolled back'); ROLLBACK;",
    )
    .unwrap();
    watch.idle();
    drop(watch);
    ok(p, &["add", "After watch"]);
    assert_eq!(
        ok(p, &["next", "--local", "--session", "worker"])["description"],
        "After watch"
    );
}

#[test]
fn watch_requires_project_and_regular_list_still_returns_single_json() {
    let dir = TempDir::new().unwrap();
    let out: Output = command(dir.path())
        .args(["list", "--watch"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("No .qqq directory found"));
    ok(dir.path(), &["init"]);
    assert_eq!(ok(dir.path(), &["list"]), serde_json::json!([]));
}

#[test]
fn watch_keeps_completed_limit_on_every_refresh() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "First"]);
    ok(p, &["add", "Second"]);
    let watch = Watcher::start(p, &["list", "--watch", "--max-completed", "0", "--json"]);
    assert_eq!(watch.snapshot().as_array().unwrap().len(), 2);
    ok(p, &["next", "--local", "--session", "worker"]);
    assert_eq!(watch.snapshot()[0]["status"], "in_progress");
    ok(p, &["complete", "1", "--session", "worker"]);
    let remaining = watch.snapshot();
    assert_eq!(remaining.as_array().unwrap().len(), 1);
    assert_eq!(remaining[0]["description"], "Second");
    ok(p, &["message", "2", "Note"]);
    assert_eq!(watch.snapshot(), remaining);
}

#[test]
fn watch_uses_human_display_default_and_honors_overrides() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "First"]);
    ok(p, &["next", "--local", "--session", "worker"]);
    ok(p, &["complete", "1", "--session", "worker"]);
    ok(p, &["add", "Second"]);
    std::fs::create_dir_all(p.join(".config/qqq")).unwrap();
    std::fs::write(
        p.join(".config/qqq/config.toml"),
        "[display]\nmax-completed = 0\n",
    )
    .unwrap();

    {
        let json = Watcher::start(p, &["list", "--watch", "--json"]);
        assert_eq!(json.snapshot().as_array().unwrap().len(), 2);
    }
    for args in [
        vec!["list", "--watch", "--all"],
        vec!["list", "--watch", "--max-completed", "1"],
    ] {
        let watch = Watcher::start(p, &args);
        assert!(watch.line().starts_with("ID"));
        assert!(watch.line().contains("First"));
        assert!(watch.line().contains("Second"));
    }

    let mut watch = Watcher::start(p, &["list", "--watch"]);
    assert!(watch.line().starts_with("ID"));
    assert!(watch.line().contains("Second"));
    watch.idle();
    ok(p, &["next", "--local", "--session", "worker"]);
    assert!(watch.line().starts_with("ID"));
    assert!(watch.line().contains("In progress"));
    ok(p, &["complete", "2", "--session", "worker"]);
    assert_eq!(watch.line(), "No tasks to display.");
    watch.idle();
}
