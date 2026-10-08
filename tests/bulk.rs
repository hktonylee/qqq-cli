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
    fn error(&self, args: &[&str], code: &str) -> Value {
        let output = self.run(args);
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty());
        let value: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(value["code"], code, "{value}");
        value
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

#[test]
fn tag_only_apply_does_not_update_unselected_metadata_columns() {
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    f.db()
        .execute_batch(
            "CREATE TRIGGER priority_guard BEFORE UPDATE OF priority ON tasks
        BEGIN SELECT RAISE(ABORT,'priority must remain untouched'); END;",
        )
        .unwrap();
    let preview = f.ok(&["bulk", "--id", "1", "--add-tag", "review"]);
    f.apply(&preview);
    assert_eq!(f.ok(&["show", "1"])["task"]["tags"], json!(["review"]));
    assert_eq!(f.ok(&["show", "1"])["task"]["priority"], 0);
}

#[test]
fn suppressed_late_update_rejects_and_rolls_back_entire_batch() {
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    f.ok(&["add", "Second"]);
    f.db()
        .execute_batch(
            "CREATE TRIGGER ignore_second BEFORE UPDATE OF priority ON tasks
        WHEN OLD.id=2 BEGIN SELECT RAISE(IGNORE); END;",
        )
        .unwrap();
    let preview = f.ok(&["bulk", "--all", "--priority", "10"]);
    f.write("preview.json", &preview);
    let before = f.bytes();
    assert!(!f.run(&["bulk", "--apply", "preview.json"]).status.success());
    assert_eq!(f.bytes(), before);
    for id in ["1", "2"] {
        assert_eq!(f.ok(&["show", id])["task"]["priority"], 0);
    }
}

#[test]
fn tag_operations_preserve_order_normalize_and_allow_empty_replace() {
    let f = Fixture::new();
    f.ok(&["add", "First", "--tag", "old", "--tag", "keep"]);
    let preview = f.ok(&[
        "bulk",
        "--all",
        "--remove-tag",
        " old ",
        "--add-tag",
        "界 面",
        "--add-tag",
        "keep",
    ]);
    assert_eq!(
        preview["tasks"][0]["after"]["tags"],
        json!(["keep", "界 面"])
    );
    assert_eq!(preview["action_count"], 1);
    f.apply(&preview);
    let replaced = f.ok(&[
        "bulk",
        "--all",
        "--set-tags",
        " new, 界 面,new ",
        "--priority",
        "-100",
    ]);
    assert_eq!(
        replaced["tasks"][0]["after"]["tags"],
        json!(["new", "界 面"])
    );
    f.apply(&replaced);
    let cleared = f.ok(&["bulk", "--all", "--set-tags", "", "--priority", "100"]);
    f.apply(&cleared);
    assert_eq!(f.ok(&["show", "1"])["task"]["tags"], json!([]));
    assert_eq!(f.ok(&["show", "1"])["task"]["priority"], 100);
}

#[test]
fn invalid_actions_selectors_and_cli_combinations_never_write() {
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    for args in [
        vec!["bulk", "--all"],
        vec!["bulk", "--priority", "5"],
        vec!["bulk", "--include-archived", "--priority", "5"],
        vec!["bulk", "--id", "0", "--priority", "5"],
        vec!["bulk", "--id", "-1", "--priority", "5"],
        vec!["bulk", "--all", "--priority", "101"],
        vec!["bulk", "--all", "--priority", "-101"],
        vec!["bulk", "--all", "--priority", "wat"],
        vec!["bulk", "--all", "--add-tag", ""],
        vec!["bulk", "--all", "--remove-tag", "bad[tag]"],
        vec!["bulk", "--all", "--set-tags", "x,,y"],
        vec!["bulk", "--all", "--add-tag", " x ", "--remove-tag", "x"],
        vec!["bulk", "--all", "--set-tags", "x", "--add-tag", "y"],
        vec!["bulk", "--all", "--set-tags", "x", "--remove-tag", "y"],
        vec!["bulk", "--all", "--archive", "--unarchive"],
        vec!["bulk", "--id", "1", "--query", "First", "--priority", "5"],
        vec!["bulk", "--id", "1", "--tag", "x", "--priority", "5"],
        vec!["bulk", "--all", "--status", "pending", "--priority", "5"],
        vec!["bulk", "--apply", "preview.json", "--priority", "5"],
        vec!["bulk", "--apply", "preview.json", "--all"],
        vec!["bulk", "--all", "--tag", "x\ny", "--priority", "5"],
    ] {
        let before = f.bytes();
        f.error(&args, "INVALID_ARGUMENT");
        assert_eq!(f.bytes(), before, "{args:?}");
    }
    let before = f.bytes();
    f.error(
        &["bulk", "--all", "--filter", "bad(", "--priority", "5"],
        "INVALID_FILTER",
    );
    let error = f.error(
        &["bulk", "--id", "1", "--id", "999", "--priority", "5"],
        "TASK_NOT_FOUND",
    );
    assert_eq!(error["details"]["task_id"], 999);
    assert_eq!(f.bytes(), before);
}

#[test]
fn malformed_preview_and_cross_project_apply_reject_without_writes() {
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    let preview = f.ok(&["bulk", "--all", "--priority", "5"]);
    let variants = [
        ("version", json!(2)),
        ("applied", json!(true)),
        ("selected_count", json!(2)),
        ("changed_count", json!(0)),
        ("action_count", json!(0)),
        ("selected_ids", json!([1, 1])),
        ("selected_ids", json!([0])),
        ("database", json!("")),
        ("extra", json!(true)),
        ("tasks", json!([])),
        ("version", json!("1")),
    ];
    for (key, value) in variants {
        let mut invalid = preview.clone();
        invalid[key] = value;
        f.write("bad.json", &invalid);
        let before = f.bytes();
        f.error(&["bulk", "--apply", "bad.json"], "INVALID_ARGUMENT");
        assert_eq!(f.bytes(), before);
    }
    for (path, value) in [
        ("/tasks/0/after/priority", json!(6)),
        ("/tasks/0/fingerprint", json!("bad")),
        ("/tasks/0/before/tags", json!([" x "])),
        ("/tasks/0/after/priority", json!(101)),
        ("/actions/extra", json!(true)),
    ] {
        let mut invalid = preview.clone();
        if path == "/actions/extra" {
            invalid["actions"]["extra"] = value;
        } else {
            *invalid.pointer_mut(path).unwrap() = value;
        }
        f.write("bad.json", &invalid);
        let before = f.bytes();
        f.error(&["bulk", "--apply", "bad.json"], "INVALID_ARGUMENT");
        assert_eq!(f.bytes(), before);
    }
    let raw = serde_json::to_string(&preview).unwrap();
    for invalid in [
        format!("{{\"version\":1,{}", &raw[1..]),
        raw.replace("\"add_tags\":[]", "\"add_tags\":[],\"add_tags\":[]"),
        raw.replace("\"priority\":0", "\"priority\":0,\"priority\":0"),
        raw.replace("\"id\":1", "\"id\":1,\"id\":1"),
        "{".to_owned(),
    ] {
        fs::write(f.path().join("bad.json"), invalid).unwrap();
        f.error(&["bulk", "--apply", "bad.json"], "INVALID_ARGUMENT");
    }
    let other = Fixture::new();
    other.ok(&["add", "Other"]);
    other.write("foreign.json", &preview);
    let before = other.bytes();
    other.error(&["bulk", "--apply", "foreign.json"], "INVALID_ARGUMENT");
    assert_eq!(other.bytes(), before);
}

#[test]
fn late_update_and_event_failures_roll_back_every_row_and_history() {
    for trigger in [
        "CREATE TRIGGER reject_second BEFORE UPDATE ON tasks WHEN OLD.id=2
         BEGIN SELECT RAISE(ABORT,'injected update'); END;",
        "CREATE TRIGGER reject_second BEFORE INSERT ON events WHEN NEW.task_id=2
         BEGIN SELECT RAISE(ABORT,'injected event'); END;",
        "CREATE TRIGGER reject_second BEFORE INSERT ON events WHEN NEW.task_id=2
         BEGIN SELECT RAISE(IGNORE); END;",
    ] {
        let f = Fixture::new();
        f.ok(&["add", "First"]);
        f.ok(&["add", "Second"]);
        f.db().execute_batch(trigger).unwrap();
        let preview = f.ok(&["bulk", "--all", "--archive", "--priority", "5"]);
        f.write("preview.json", &preview);
        let before = f.bytes();
        assert!(
            !f.run(&["bulk", "--apply", "preview.json"]).status.success(),
            "{trigger}"
        );
        assert_eq!(f.bytes(), before, "{trigger}");
        assert_eq!(
            f.db()
                .query_row("SELECT count(*) FROM events", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn selected_content_owner_dependencies_and_missing_rows_invalidate_preview() {
    for mutation in [
        "tags",
        "description",
        "owner",
        "parent",
        "prerequisite",
        "archive",
        "delete",
        "image",
        "dependency_state",
    ] {
        let f = Fixture::new();
        f.ok(&["add", "First"]);
        f.ok(&["add", "Second"]);
        f.ok(&["add", "Prerequisite"]);
        if mutation == "dependency_state" {
            f.ok(&["edit", "2", "--depends-on", "3"]);
        }
        let preview = f.ok(&["bulk", "--id", "1", "--id", "2", "--add-tag", "review"]);
        match mutation {
            "tags" => {
                f.ok(&["edit", "2", "--set-tags", "other"]);
            }
            "description" => {
                f.ok(&["edit", "2", "-d", "Changed"]);
            }
            "owner" => {
                f.ok(&[
                    "next",
                    "--local",
                    "--session",
                    "foreign",
                    "--filter",
                    "id==2",
                ]);
            }
            "parent" => {
                f.ok(&["edit", "2", "--set-parent", "3"]);
            }
            "prerequisite" => {
                f.ok(&["edit", "2", "--depends-on", "3"]);
            }
            "archive" => {
                f.ok(&["archive", "2"]);
            }
            "delete" => {
                f.ok(&["archive", "2"]);
                f.ok(&["delete", "2", "--yes"]);
            }
            "image" => {
                fs::write(f.path().join("changed.png"), b"\x89PNG\r\n\x1a\nchanged").unwrap();
                f.ok(&["edit", "2", "--image", "changed.png"]);
            }
            "dependency_state" => {
                f.ok(&[
                    "next",
                    "--local",
                    "--session",
                    "finisher",
                    "--filter",
                    "id==3",
                ]);
                f.ok(&["complete", "3", "--session", "finisher"]);
            }
            _ => unreachable!(),
        }
        f.write("preview.json", &preview);
        let before = f.bytes();
        let error = f.error(&["bulk", "--apply", "preview.json"], "BULK_CONFLICT");
        assert_eq!(error["details"]["conflict_ids"], json!([2]), "{mutation}");
        assert_eq!(f.bytes(), before, "{mutation}");
    }
}

#[test]
fn previews_preserve_staging_and_old_schema_without_creating_projects() {
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    let staging = f.path().join(".qqq/delete-staging");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("marker"), "pending").unwrap();
    let before = f.bytes();
    f.ok(&["bulk", "--all", "--priority", "5"]);
    assert_eq!(f.bytes(), before);
    assert_eq!(
        fs::read_to_string(staging.join("marker")).unwrap(),
        "pending"
    );
    f.db()
        .execute_batch("DROP TABLE claim_processes; PRAGMA user_version=12;")
        .unwrap();
    let before = f.bytes();
    f.error(&["bulk", "--all", "--priority", "5"], "DATABASE_ERROR");
    assert_eq!(f.bytes(), before);
    f.error(&["bulk", "--all", "--priority", "101"], "INVALID_ARGUMENT");
    assert_eq!(f.bytes(), before);
    assert_eq!(
        fs::read_to_string(staging.join("marker")).unwrap(),
        "pending"
    );
    let absent = TempDir::new().unwrap();
    let output = f
        .command()
        .current_dir(absent.path())
        .args(["--json", "bulk", "--all", "--priority", "5"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!absent.path().join(".qqq").exists());
}

#[test]
fn concurrent_apply_has_one_complete_winner_and_one_stale_conflict() {
    use std::process::Stdio;
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    f.ok(&["add", "Second"]);
    let preview = f.ok(&["bulk", "--all", "--add-tag", "review", "--priority", "5"]);
    f.write("preview.json", &preview);
    let spawn = || {
        f.command()
            .args(["--json", "bulk", "--apply", "preview.json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let a = spawn();
    let b = spawn();
    let mut successes = 0;
    for child in [a, b] {
        let output = child.wait_with_output().unwrap();
        if output.status.success() {
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["changed_count"], 2);
            successes += 1;
        } else {
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["code"], "BULK_CONFLICT");
        }
    }
    assert_eq!(successes, 1);
    for id in ["1", "2"] {
        let shown = f.ok(&["show", id]);
        assert_eq!(shown["task"]["priority"], 5);
        assert_eq!(shown["task"]["tags"], json!(["review"]));
    }
}

#[test]
fn human_preview_and_agent_output_defaults_show_all_values() {
    let f = Fixture::new();
    f.ok(&["add", "First", "--tag", "old"]);
    f.ok(&["add", "Second"]);
    let args = ["bulk", "--all", "--add-tag", "界 面", "--priority", "5"];
    let out = f.command().args(args).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    for part in [
        "Bulk preview for 2 tasks",
        "2 changed",
        "4 actions",
        "#1",
        "#2",
        "old",
        "界 面",
        "0 -> 5",
        "false -> false",
        "--apply",
    ] {
        assert!(text.contains(part), "{part}: {text}");
    }
    let out = f
        .command()
        .env("CODEX_THREAD_ID", "bulk-agent")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["selected_count"], 2);
    let out = f
        .command()
        .env("CODEX_THREAD_ID", "bulk-agent")
        .arg("--human")
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("Bulk preview")
    );
}

#[test]
fn bulk_rejects_invalid_global_assignment_metadata_like_other_commands() {
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    let before = f.bytes();
    f.error(
        &["--harness-name", " ", "bulk", "--all", "--priority", "5"],
        "INVALID_ARGUMENT",
    );
    assert_eq!(f.bytes(), before);
}

#[test]
fn direct_status_selection_includes_all_completed_rows_and_explicit_archived_ids() {
    let f = Fixture::new();
    for description in ["Finished one", "Finished two", "Failed", "Fresh", "Hidden"] {
        f.ok(&["add", description]);
    }
    for id in ["1", "2"] {
        let selector = format!("id=={id}");
        f.ok(&[
            "next",
            "--local",
            "--session",
            "finisher",
            "--filter",
            &selector,
        ]);
        f.ok(&["complete", id, "--session", "finisher"]);
    }
    f.ok(&[
        "next",
        "--local",
        "--session",
        "worker",
        "--filter",
        "id==3",
    ]);
    f.ok(&[
        "edit",
        "3",
        "--set-status",
        "error",
        "--reason",
        "failure",
        "--session",
        "worker",
    ]);
    f.ok(&["archive", "5"]);
    assert_eq!(
        f.ok(&["bulk", "--status", "completed", "--priority", "5"])["selected_ids"],
        json!([1, 2])
    );
    assert_eq!(
        f.ok(&[
            "bulk",
            "--status",
            "new",
            "--status",
            "error",
            "--priority",
            "5"
        ])["selected_ids"],
        json!([3, 4])
    );
    assert_eq!(
        f.ok(&[
            "bulk",
            "--all",
            "--include-archived",
            "--status",
            "new",
            "--priority",
            "5"
        ])["selected_ids"],
        json!([4, 5])
    );
    assert_eq!(
        f.ok(&["bulk", "--id", "5", "--priority", "5"])["selected_ids"],
        json!([5])
    );
}

#[test]
fn metadata_apply_preserves_content_claims_links_graph_images_and_history() {
    let f = Fixture::new();
    f.ok(&["add", "Parent"]);
    f.ok(&["add", "Prerequisite"]);
    for id in ["1", "2"] {
        let selector = format!("id=={id}");
        f.ok(&[
            "next",
            "--local",
            "--session",
            "finisher",
            "--filter",
            &selector,
        ]);
        f.ok(&["complete", id, "--session", "finisher"]);
    }
    fs::write(f.path().join("keep.png"), b"\x89PNG\r\n\x1a\nkeep").unwrap();
    f.ok(&[
        "add",
        "Whole\nUnicode λ🙂",
        "--parent",
        "1",
        "--depends-on",
        "2",
        "--image",
        "keep.png",
        "--tag",
        "keep",
    ]);
    f.ok(&[
        "next",
        "--local",
        "--session",
        "foreign",
        "--filter",
        "id==3",
    ]);
    f.ok(&["message", "3", "Keep history\nbody"]);
    let process =
        json!({"machine":"foreign-machine", "pid":123, "started_at":"old", "executable":"codex"})
            .to_string();
    let link =
        json!({"server":"missing", "identity":{"agent":"codex","kind":"id","value":"foreign"},
        "pane":{"pane_id":"w1:p1", "workspace_id":"w1", "tab_id":"w1:t1"}, "claim_key":"foreign"})
        .to_string();
    let conn = f.db();
    conn.execute(
        "INSERT INTO claim_processes(task_id,claim_key,claim_event_id,process_json)
        VALUES (3,'foreign',(SELECT max(id) FROM events WHERE task_id=3 AND action='claim'),?)",
        [&process],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO herdr_links(task_id,link_json) VALUES (3,?)",
        [&link],
    )
    .unwrap();
    let before = f.ok(&["show", "3"]);
    let image = fs::read(f.path().join(".qqq/images/3/1.png")).unwrap();
    let preview = f.ok(&[
        "bulk",
        "--id",
        "3",
        "--add-tag",
        "review",
        "--priority",
        "7",
    ]);
    assert!(!serde_json::to_string(&preview).unwrap().contains("foreign"));
    f.apply(&preview);
    let after = f.ok(&["show", "3"]);
    for field in ["messages", "images", "herdr", "events"] {
        assert_eq!(after[field], before[field], "{field}");
    }
    for (field, value) in before["task"].as_object().unwrap() {
        if ["tags", "priority", "updated_at"].contains(&field.as_str()) {
            continue;
        }
        assert_eq!(&after["task"][field], value, "{field}");
    }
    assert_eq!(after["task"]["priority"], 7);
    assert_eq!(after["task"]["tags"], json!(["keep", "review"]));
    assert_eq!(
        conn.query_row(
            "SELECT process_json FROM claim_processes WHERE task_id=3",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        process
    );
    assert_eq!(
        conn.query_row(
            "SELECT link_json FROM herdr_links WHERE task_id=3",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        link
    );
    assert_eq!(
        conn.query_row("SELECT claim_key FROM tasks WHERE id=3", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "foreign"
    );
    assert_eq!(
        fs::read(f.path().join(".qqq/images/3/1.png")).unwrap(),
        image
    );
}

#[test]
fn archive_graph_handles_extra_prerequisites_and_revalidates_new_dependents() {
    let f = Fixture::new();
    f.ok(&["add", "Parent"]);
    f.ok(&["add", "Prerequisite"]);
    f.ok(&["add", "Child", "--parent", "1", "--depends-on", "2"]);
    let preview = f.ok(&["bulk", "--all", "--archive"]);
    f.apply(&preview);
    f.error(&["bulk", "--id", "3", "--unarchive"], "INVALID_TRANSITION");
    let preview = f.ok(&["bulk", "--all", "--include-archived", "--unarchive"]);
    f.apply(&preview);
    for id in ["1", "2", "3"] {
        assert_eq!(f.ok(&["show", id])["task"]["archived"], false);
    }
    f.ok(&["add", "Standalone"]);
    let preview = f.ok(&["bulk", "--id", "4", "--archive", "--priority", "5"]);
    f.ok(&["add", "New dependent", "--depends-on", "4"]);
    f.write("preview.json", &preview);
    let before = f.bytes();
    f.error(&["bulk", "--apply", "preview.json"], "INVALID_TRANSITION");
    assert_eq!(f.bytes(), before);
}

#[test]
fn stdin_apply_is_explicit_confirmation_and_applied_reports_are_rejected() {
    use std::{io::Write, process::Stdio};
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    let preview = f.ok(&["bulk", "--all", "--archive"]);
    let mut child = f
        .command()
        .args([
            "--json",
            "--harness-session",
            "reviewer",
            "bulk",
            "--apply",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&preview).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let applied: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(applied["applied"], true);
    let detail = f.ok(&["show", "1"]);
    assert_eq!(detail["events"][0]["action"], "archive");
    assert_eq!(detail["events"][0]["session"], "reviewer");
    f.write("applied.json", &applied);
    let before = f.bytes();
    f.error(&["bulk", "--apply", "applied.json"], "INVALID_ARGUMENT");
    assert_eq!(f.bytes(), before);
    f.write("preview.json", &preview);
    f.error(&["bulk", "--apply", "preview.json"], "BULK_CONFLICT");
    assert_eq!(f.bytes(), before);
}

#[test]
fn private_claim_key_changes_conflict_without_disclosing_owner_data() {
    let f = Fixture::new();
    f.ok(&["add", "Owned"]);
    f.ok(&["next", "--local", "--session", "private-owner"]);
    let preview = f.ok(&["bulk", "--all", "--priority", "5"]);
    let text = serde_json::to_string(&preview).unwrap();
    assert!(!text.contains("private-owner") && !text.contains("claim_key"));
    f.db()
        .execute(
            "UPDATE tasks SET claim_key='private-replacement' WHERE id=1",
            [],
        )
        .unwrap();
    f.write("preview.json", &preview);
    let before = f.bytes();
    f.error(&["bulk", "--apply", "preview.json"], "BULK_CONFLICT");
    assert_eq!(f.bytes(), before);
}

#[test]
fn concurrent_message_history_survives_metadata_apply_without_spurious_conflict() {
    let f = Fixture::new();
    f.ok(&["add", "First"]);
    let preview = f.ok(&["bulk", "--all", "--priority", "5"]);
    f.ok(&["message", "1", "Concurrent note"]);
    f.apply(&preview);
    let detail = f.ok(&["show", "1"]);
    assert_eq!(detail["messages"][0]["body"], "Concurrent note");
    assert_eq!(detail["task"]["priority"], 5);
}
