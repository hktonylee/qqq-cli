use super::{human_command, ok, project, run};
use rusqlite::Connection;
use serde_json::{Value, json};

#[test]
fn tags_normalize_without_changing_description_or_revision() {
    let dir = project();
    let p = dir.path();
    let text = "  Layout\nSecond line\n";
    let task = ok(
        p,
        &[
            "add",
            text,
            "--tag",
            " frontend ",
            "--tag",
            "frontend",
            "--tag",
            "UI",
            "--tag",
            "界 面",
        ],
    );
    assert_eq!(task["tags"], json!(["frontend", "UI", "界 面"]));
    assert_eq!(task["description"], text);
    assert_eq!(task["content_revision"], 1);
    assert_eq!(ok(p, &["list"])[0]["tags"], task["tags"]);
    assert_eq!(ok(p, &["add", "Untagged"])["tags"], json!([]));
    let preview = ok(p, &["next", "--dry-run", "--filter", "id == 1"]);
    assert_eq!(preview["tags"], task["tags"]);
    let status = ok(p, &["status"]);
    assert!(status.to_string().contains("frontend"));
}

#[test]
fn tag_edits_preserve_owned_task_content_images_and_audit() {
    let dir = project();
    let p = dir.path();
    std::fs::write(p.join("ok.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(
        p,
        &[
            "add",
            "Content",
            "--tag",
            "old",
            "--image",
            "ok.png",
            "--priority",
            "7",
        ],
    );
    ok(p, &["next", "--local", "--session", "owner"]);
    ok(p, &["message", "1", "Keep note"]);
    let before = ok(p, &["show", "1"]);
    let edited = ok(
        p,
        &[
            "edit",
            "1",
            "--set-tags",
            " frontend, bug,frontend , Frontend ",
        ],
    );
    assert_eq!(edited["tags"], json!(["frontend", "bug", "Frontend"]));
    for key in [
        "description",
        "content_revision",
        "status",
        "priority",
        "parent_id",
        "prerequisites",
        "harness_session",
        "harness_name",
        "orchestrator_session",
    ] {
        assert_eq!(edited[key], before["task"][key], "{key}");
    }
    let after = ok(p, &["show", "1"]);
    for key in ["images", "messages", "events"] {
        assert_eq!(after[key], before[key], "{key}");
    }
    assert_eq!(ok(p, &["edit", "1", "--set-tags", ""])["tags"], json!([]));
    ok(p, &["edit", "1", "--set-tags", "saved"]);
    assert_eq!(
        ok(p, &["edit", "1", "--description", "Updated"])["tags"],
        json!(["saved"])
    );
    assert_eq!(
        ok(p, &["complete", "1", "--session", "owner"])["tags"],
        json!(["saved"])
    );
}

#[test]
fn invalid_tags_reject_atomic_edits_and_additions() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Original", "--tag", "keep"]);
    let before = ok(p, &["show", "1"]);
    for value in [
        "frontend,,bug",
        ",bug",
        "bug,",
        "[frontend]",
        "line\nbreak",
        "a\u{7f}b",
    ] {
        let result = run(
            p,
            &[
                "edit",
                "1",
                "--description",
                "Changed",
                "--priority",
                "99",
                "--set-tags",
                value,
            ],
        );
        assert_eq!(result.status.code(), Some(1), "{value:?}: {result:?}");
        let error: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(error["code"], "INVALID_ARGUMENT");
        assert!(result.stdout.is_empty());
        assert_eq!(ok(p, &["show", "1"]), before);
    }
    for value in ["", " ", "a,b", "[a]", "a\nb"] {
        let result = run(p, &["add", "Bad", "--tag", value]);
        assert_eq!(result.status.code(), Some(1));
    }
    assert_eq!(ok(p, &["list"]).as_array().unwrap().len(), 1);
}

#[test]
fn valid_combined_edit_commits_tags_and_content_once() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Original"]);
    let task = ok(
        p,
        &[
            "edit",
            "1",
            "--description",
            "Changed",
            "--priority",
            "12",
            "--expected-revision",
            "1",
            "--set-tags",
            "bug",
        ],
    );
    assert_eq!(task["tags"], json!(["bug"]));
    assert_eq!(task["description"], "Changed");
    assert_eq!(task["content_revision"], 2);
    let result = run(
        p,
        &[
            "edit",
            "1",
            "--expected-revision",
            "1",
            "--set-tags",
            "stale",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    assert_eq!(ok(p, &["show", "1"])["task"], task);
}

#[test]
fn import_tags_validate_before_writes_and_survive_dry_run() {
    let dir = project();
    let p = dir.path();
    let batch = json!({"version":1,"tasks":[
        {"key":"one","description":"One","tags":[" frontend ","frontend","界"]},
        {"key":"two","description":"Two"}
    ]});
    std::fs::write(p.join("batch.json"), batch.to_string()).unwrap();
    let preview = ok(p, &["import", "batch.json", "--dry-run"]);
    assert_eq!(preview["tasks"][0]["tags"], json!(["frontend", "界"]));
    assert_eq!(preview["tasks"][1]["tags"], json!([]));
    assert!(ok(p, &["list"]).as_array().unwrap().is_empty());
    let imported = ok(p, &["import", "batch.json"]);
    assert_eq!(imported["tasks"][0]["tags"], preview["tasks"][0]["tags"]);
    assert_eq!(
        ok(p, &["show", "1"])["task"]["tags"],
        preview["tasks"][0]["tags"]
    );
    let before = ok(p, &["list"]);
    let bad = json!({"version":1,"tasks":[{"key":"ok","description":"OK","tags":["good"]},{"key":"bad","description":"Bad","tags":["a,b"]}]});
    std::fs::write(p.join("batch.json"), bad.to_string()).unwrap();
    assert_eq!(run(p, &["import", "batch.json"]).status.code(), Some(1));
    assert_eq!(ok(p, &["list"]), before);
}

#[test]
fn tags_survive_backup_restore() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Saved", "--tag", "frontend", "--tag", "界 面"]);
    let before = ok(p, &["show", "1"]);
    ok(p, &["backup", "snapshot.tar"]);
    let target = tempfile::TempDir::new().unwrap();
    ok(
        target.path(),
        &["restore", p.join("snapshot.tar").to_str().unwrap()],
    );
    assert_eq!(ok(target.path(), &["show", "1"]), before);
}

#[test]
fn schema_eleven_migrates_tasks_to_empty_tags() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Legacy"]);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    let has_tags: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name='tags')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if has_tags {
        conn.execute_batch("DROP TABLE claim_processes; ALTER TABLE tasks DROP COLUMN tags;")
            .unwrap();
    }
    conn.pragma_update(None, "user_version", 11).unwrap();
    drop(conn);
    let task = ok(p, &["show", "1"]);
    assert_eq!(task["task"]["tags"], json!([]));
    assert_eq!(task["task"]["content_revision"], 1);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        13
    );
}

#[test]
fn human_output_prefixes_tags_without_contaminating_description() {
    let dir = project();
    let p = dir.path();
    ok(
        p,
        &[
            "add",
            "First line\nSecond line",
            "--tag",
            "frontend",
            "--tag",
            "界",
        ],
    );
    for args in [&["list"][..], &["list", "--oneline"][..]] {
        let output = human_command(p)
            .env("NO_COLOR", "1")
            .env("COLUMNS", "80")
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("[frontend] [界] First line"), "{text}");
        assert_eq!(text.matches("[frontend]").count(), 1);
    }
    let output = human_command(p)
        .env("NO_COLOR", "1")
        .args(["show", "1"])
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("Tags:") && text.contains("[frontend] [界]"),
        "{text}"
    );
    assert_eq!(
        ok(p, &["show", "1"])["task"]["description"],
        "First line\nSecond line"
    );
}
