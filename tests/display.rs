use serde_json::Value;
use std::{
    fs,
    process::{Command, Output},
};
use tempfile::TempDir;

struct Project {
    dir: TempDir,
}
impl Project {
    fn new() -> Self {
        let p = Self {
            dir: TempDir::new().unwrap(),
        };
        p.ok(&["init"]);
        p
    }
    fn config(&self, content: &str) {
        let dir = self.dir.path().join(".config/qqq");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.toml"), content).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_qqq"))
            .current_dir(self.dir.path())
            .env("HOME", self.dir.path())
            .env_remove("QQQ_SESSION")
            .env_remove("HERDR_ENV")
            .args(args)
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    fn ids(&self, args: &[&str]) -> Vec<i64> {
        self.ok(args)
            .lines()
            .skip(1)
            .map(|row| row.split_whitespace().next().unwrap().parse().unwrap())
            .collect()
    }
    fn tasks(&self) -> Value {
        serde_json::from_str(&self.ok(&["list", "--json"])).unwrap()
    }
    fn fixture(&self) {
        for title in ["Old created", "Second", "Third"] {
            self.ok(&["add", title]);
        }
        for session in ["a", "b", "c"] {
            self.ok(&["next", "--session", session]);
        }
        for (id, session) in [("2", "b"), ("3", "c"), ("1", "a")] {
            self.ok(&["complete", id, "--session", session]);
        }
        self.ok(&["add", "New task"]);
        self.ok(&["next", "--session", "active"]);
        self.ok(&["add", "Unclaimed"]);
        self.ok(&["add", "Hidden-parent child", "--parent", "2"]);
        self.ok(&["add", "Visible-parent child", "--parent", "3"]);
    }
}

#[test]
fn limit_uses_completion_order_keeps_active_tasks_and_preserves_json() {
    let p = Project::new();
    p.fixture();
    let before = p.tasks();
    p.config("[display]\nmax-completed = 2\n");
    assert_eq!(p.ids(&["list"]), vec![1, 3, 7, 4, 5, 6]);
    assert_eq!(p.tasks(), before);
    // Editing older completed task must not make it a newer completion.
    p.ok(&["edit", "2", "--title", "Edited older completion"]);
    assert_eq!(p.ids(&["list"]), vec![1, 3, 7, 4, 5, 6]);
    assert_eq!(p.ids(&["list", "--all"]), vec![1, 2, 6, 3, 7, 4, 5]);
    assert!(p.ok(&["show", "2"]).contains("Edited older completion"));
}

#[test]
fn zero_missing_and_large_limits_have_clear_boundaries() {
    let p = Project::new();
    p.fixture();
    assert_eq!(p.ids(&["list"]), vec![1, 2, 6, 3, 7, 4, 5]);
    p.config("display.max-completed = 0\n");
    let list = p.ok(&["list"]);
    assert_eq!(p.ids(&["list"]), vec![4, 5, 6, 7]);
    assert!(!list.contains("Completed"));
    assert!(!list.contains("└──"));
    p.config("[display]\nmax-completed = 9223372036854775807\n");
    assert_eq!(p.ids(&["list"]), vec![1, 2, 6, 3, 7, 4, 5]);
    p.config("[display]\n");
    assert_eq!(p.ids(&["list"]), vec![1, 2, 6, 3, 7, 4, 5]);
}

#[test]
fn limited_empty_lists_show_no_tasks_to_display() {
    let p = Project::new();
    p.config("[display]\nmax-completed = 0\n");
    assert_eq!(p.ok(&["list"]), "No tasks to display.\n");
    p.ok(&["add", "Task"]);
    p.ok(&["next", "--session", "a"]);
    p.ok(&["complete", "1", "--session", "a"]);
    assert_eq!(p.ok(&["list"]), "No tasks to display.\n");
    assert!(p.ok(&["list", "--all"]).contains("Task"));
}

#[test]
fn aliases_share_display_setting_and_json_stays_full() {
    let p = Project::new();
    p.fixture();
    p.config("[alias]\nls = 'list'\nall = 'list --all'\n[display]\nmax-completed = 1\n");
    assert_eq!(p.ids(&["ls"]), vec![1, 4, 5, 6, 7]);
    assert_eq!(p.ids(&["all"]), vec![1, 2, 6, 3, 7, 4, 5]);
    let tasks: Value = serde_json::from_str(&p.ok(&["ls", "--json"])).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 7);
}

#[test]
fn invalid_config_fails_human_list_but_full_views_remain_usable() {
    let p = Project::new();
    p.fixture();
    for content in [
        "[display",
        "[display]\nmax-completed = -1",
        "[display]\nmax-completed = 'two'",
        "[display]\nmax-completed = 1.5",
        "[display]\nmax-completed = true",
    ] {
        p.config(content);
        let out = p.run(&["list"]);
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("config.toml"));
        assert_eq!(p.ids(&["list", "--all"]), vec![1, 2, 6, 3, 7, 4, 5]);
        assert_eq!(p.tasks().as_array().unwrap().len(), 7);
        assert!(p.run(&["--help"]).status.success());
    }
}

#[test]
fn legacy_completed_tasks_without_events_use_newest_id_fallback() {
    let p = Project::new();
    let conn = rusqlite::Connection::open(p.dir.path().join("qqq.db")).unwrap();
    conn.execute_batch("INSERT INTO tasks(title,status) VALUES ('First','completed'),('Second','completed'),('Third','completed');").unwrap();
    p.config("[display]\nmax-completed = 1\n");
    assert_eq!(p.ids(&["list"]), vec![3]);
}
