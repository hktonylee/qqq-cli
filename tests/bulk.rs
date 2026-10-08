use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

struct Fixture(TempDir);
impl Fixture {
    fn new() -> Self {
        let fixture = Self(TempDir::new().unwrap());
        fixture.ok(&["init"]);
        fixture
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
        command
            .current_dir(self.path())
            .env_remove("QQQ_SESSION")
            .env_remove("CODEX_THREAD_ID")
            .env_remove("CODEX_SESSION_ID")
            .env_remove("HERDR_ENV")
            .env_remove("HERDR_PANE_ID")
            .env("PATH", self.path());
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command().arg("--json").args(args).output().unwrap()
    }
    fn ok(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.path().join(".qqq/qqq.db")).unwrap()
    }
    fn db(&self) -> Connection {
        Connection::open(self.path().join(".qqq/qqq.db")).unwrap()
    }
    fn write(&self, name: &str, value: &Value) {
        fs::write(self.path().join(name), serde_json::to_vec(value).unwrap()).unwrap();
    }
    fn apply(&self, preview: &Value) -> Value {
        self.write("preview.json", preview);
        self.ok(&["bulk", "--apply", "preview.json"])
    }
}

#[test]
fn preview_deduplicates_ids_and_applies_exact_tag_priority_batch() {
    let f = Fixture::new();
    f.ok(&["add", "First\nKeep whole description", "--tag", "old"]);
    f.ok(&["add", "Second"]);
    let before = f.bytes();
    let preview = f.ok(&[
        "bulk",
        "--id",
        "2",
        "--id",
        "1",
        "--id",
        "2",
        "--add-tag",
        " review ",
        "--priority",
        "5",
    ]);
    assert_eq!(preview["version"], 1);
    assert_eq!(preview["applied"], false);
    assert_eq!(preview["selected_ids"], json!([1, 2]));
    assert_eq!(preview["selected_count"], 2);
    assert_eq!(preview["changed_count"], 2);
    assert_eq!(preview["action_count"], 4);
    assert_eq!(
        preview["tasks"][0]["before"],
        json!({"tags":["old"],"priority":0,"archived":false})
    );
    assert_eq!(
        preview["tasks"][0]["after"],
        json!({"tags":["old","review"],"priority":5,"archived":false})
    );
    assert_eq!(
        preview["tasks"][0]["fingerprint"].as_str().unwrap().len(),
        64
    );
    assert_eq!(f.bytes(), before);
    let applied = f.apply(&preview);
    assert_eq!(applied["applied"], true);
    assert_eq!(applied["selected_ids"], json!([1, 2]));
    for id in ["1", "2"] {
        let shown = f.ok(&["show", id]);
        assert_eq!(shown["task"]["priority"], 5);
        assert_eq!(shown["task"]["status"], "new");
        assert_eq!(shown["task"]["content_revision"], 1);
        assert_eq!(shown["events"], json!([]));
    }
    assert_eq!(
        f.ok(&["show", "1"])["task"]["description"],
        "First\nKeep whole description"
    );
    assert_eq!(f.ok(&["show", "2"])["task"]["tags"], json!(["review"]));
    assert_eq!(
        f.db()
            .query_row("SELECT count(*) FROM claim_processes", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn filtered_preview_freezes_direct_ids_and_ignores_later_matching_rows() {
    let f = Fixture::new();
    f.ok(&["add", "Parent context"]);
    f.ok(&[
        "add",
        "MATCH child",
        "--parent",
        "1",
        "--tag",
        "backend",
        "--priority",
        "2",
    ]);
    f.ok(&[
        "add",
        "Match partial",
        "--tag",
        "backend-extra",
        "--priority",
        "2",
    ]);
    let preview = f.ok(&[
        "bulk",
        "--query",
        "match",
        "--status",
        "new",
        "--tag",
        "backend",
        "--filter",
        "priority == 2",
        "--priority",
        "7",
    ]);
    assert_eq!(preview["selected_ids"], json!([2]));
    f.ok(&[
        "add",
        "Match new arrival",
        "--tag",
        "backend",
        "--priority",
        "2",
    ]);
    f.apply(&preview);
    assert_eq!(f.ok(&["show", "1"])["task"]["priority"], 0);
    assert_eq!(f.ok(&["show", "2"])["task"]["priority"], 7);
    assert_eq!(f.ok(&["show", "3"])["task"]["priority"], 2);
    assert_eq!(f.ok(&["show", "4"])["task"]["priority"], 2);
}

#[test]
fn stale_selected_row_rejects_whole_batch_before_any_write() {
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    f.ok(&["add", "Second"]);
    let preview = f.ok(&["bulk", "--all", "--priority", "10"]);
    f.ok(&["edit", "2", "--priority", "3"]);
    f.write("preview.json", &preview);
    let before = f.bytes();
    let output = f.run(&["bulk", "--apply", "preview.json"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "BULK_CONFLICT");
    assert_eq!(error["details"]["conflict_ids"], json!([2]));
    assert_eq!(f.bytes(), before);
    assert_eq!(f.ok(&["show", "1"])["task"]["priority"], 0);
    assert_eq!(f.ok(&["show", "2"])["task"]["priority"], 3);
}

#[test]
fn empty_selection_and_noop_batch_preserve_database_bytes() {
    let f = Fixture::new();
    f.ok(&["add", "First", "--tag", "same"]);
    for args in [
        vec!["bulk", "--query", "absent", "--priority", "5"],
        vec!["bulk", "--id", "1", "--add-tag", "same", "--priority", "0"],
    ] {
        let before = f.bytes();
        let preview = f.ok(&args);
        assert_eq!(preview["changed_count"], 0);
        assert_eq!(preview["action_count"], 0);
        f.apply(&preview);
        assert_eq!(f.bytes(), before);
    }
}

#[test]
fn archive_batch_validates_projected_graph_and_rejects_active_rows_atomically() {
    let f = Fixture::new();
    f.ok(&["add", "Parent"]);
    f.ok(&["add", "Child", "--parent", "1"]);
    f.ok(&["add", "Active", "--priority", "10"]);
    f.ok(&["next", "--local", "--session", "worker"]);
    let before = f.bytes();
    assert!(!f.run(&["bulk", "--all", "--archive"]).status.success());
    assert_eq!(f.bytes(), before);
    let archive = f.ok(&["bulk", "--id", "1", "--id", "2", "--archive"]);
    f.apply(&archive);
    for id in ["1", "2"] {
        let shown = f.ok(&["show", id]);
        assert_eq!(shown["task"]["archived"], true);
        assert_eq!(shown["events"].as_array().unwrap().len(), 1);
        assert_eq!(shown["events"][0]["action"], "archive");
    }
    let unarchive = f.ok(&["bulk", "--id", "1", "--id", "2", "--unarchive"]);
    f.apply(&unarchive);
    for id in ["1", "2"] {
        assert_eq!(f.ok(&["show", id])["task"]["archived"], false);
    }
}
