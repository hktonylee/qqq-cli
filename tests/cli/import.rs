use super::{command, human_command, ok, project, run};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    io::Write,
    path::Path,
    process::{Output, Stdio},
};
use tempfile::TempDir;

fn stdin(dir: &Path, args: &[&str], bytes: &[u8]) -> Output {
    let mut child = command(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Err(error) = child.stdin.take().unwrap().write_all(bytes) {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    child.wait_with_output().unwrap()
}

fn stdin_ok(dir: &Path, args: &[&str], bytes: &[u8]) -> Value {
    let output = stdin(dir, args, bytes);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn batch(tasks: Value) -> Value {
    json!({"version":1,"tasks":tasks})
}

fn file_import(dir: &Path, value: &Value, dry_run: bool) -> Output {
    std::fs::write(dir.join("batch.json"), serde_json::to_vec(value).unwrap()).unwrap();
    let mut args = vec!["import", "batch.json"];
    if dry_run {
        args.push("--dry-run");
    }
    run(dir, &args)
}

fn import_ok(dir: &Path, value: &Value, dry_run: bool) -> Value {
    let output = file_import(dir, value, dry_run);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn stdin_add_preserves_full_literal_description_parent_priority_and_images() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    let image = p.join("a.png");
    std::fs::write(&image, b"\x89PNG\r\n\x1a\nfixture").unwrap();
    let text = "  λ任务🙂\r\nsecond line\n${HOME} $(touch unwanted) `echo hi` 'quoted'\n\0tail\n\n";
    let task = stdin_ok(
        p,
        &[
            "add",
            "--stdin",
            "--parent",
            "1",
            "--priority",
            "-5",
            "--image",
            image.to_str().unwrap(),
        ],
        text.as_bytes(),
    );
    assert_eq!(task["description"], text);
    assert_eq!(task["parent_id"], 1);
    assert_eq!(task["priority"], -5);
    let detail = ok(p, &["show", "2"]);
    assert_eq!(detail["task"], task);
    assert_eq!(detail["images"].as_array().unwrap().len(), 1);
    assert!(!p.join("unwanted").exists());
    assert_eq!(ok(p, &["next", "--dry-run"])["id"], 1);
}

#[test]
fn stdin_add_conflicts_blank_and_invalid_encoding_leave_database_unchanged() {
    let dir = project();
    let p = dir.path();
    let before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    for args in [
        vec!["add", "--stdin", "text"],
        vec!["add", "--stdin", "--description", "text"],
        vec!["add", "--stdin", "--edit"],
    ] {
        let out = run(p, &args);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("cannot be used"));
    }
    for text in [&b""[..], &b" \r\n\t"[..], &[0xff, 0xfe][..]] {
        assert!(!stdin(p, &["add", "--stdin"], text).status.success());
        assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
    }
    let missing = TempDir::new().unwrap();
    assert!(
        !stdin(missing.path(), &["add", "--stdin"], b" ")
            .status
            .success()
    );
    assert!(!missing.path().join(".qqq").exists());
}

#[test]
fn stdin_database_blank_constraint_fails_before_legacy_migration_or_staging_recovery() {
    let dir = TempDir::new().unwrap();
    let p = dir.path();
    let staging = p.join(".qqq/.delete-staging");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("marker"), b"pending").unwrap();
    let path = p.join(".qqq/qqq.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch(include_str!("../../src/sql/schema.sql"))
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let out = stdin(p, &["add", "--stdin"], b"  \0hidden");
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("blank-text validation"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read(staging.join("marker")).unwrap(), b"pending");
}

fn forward_batch() -> Value {
    batch(json!([
        {"key":"child","description":"  Child λ\r\nsecond\n","priority":5,"parent":{"key":"parent"}},
        {"key":"parent","description":"Parent","priority":-3},
        {"key":"tail","description":"Tail"}
    ]))
}

#[test]
fn import_file_preview_and_stdin_commit_resolve_forward_refs_deterministically() {
    let dir = project();
    let p = dir.path();
    let before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    let value = forward_batch();
    let preview = import_ok(p, &value, true);
    assert_eq!(preview["version"], 1);
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["count"], 3);
    assert_eq!(preview["mapping"], json!({}));
    assert_eq!(
        preview["creation_order"],
        json!(["parent", "child", "tail"])
    );
    assert!(
        preview["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["id"].is_null())
    );
    assert!(preview["tasks"][0]["parent_id"].is_null());
    assert_eq!(
        preview["tasks"][0]["description"],
        value["tasks"][0]["description"]
    );
    assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
    let created = stdin_ok(p, &["import", "-"], &serde_json::to_vec(&value).unwrap());
    assert_eq!(created["dry_run"], false);
    assert_eq!(created["mapping"], json!({"child":2,"parent":1,"tail":3}));
    assert_eq!(created["creation_order"], preview["creation_order"]);
    assert_eq!(created["tasks"][0]["id"], 2);
    assert_eq!(created["tasks"][0]["parent_id"], 1);
    assert_eq!(created["tasks"][2]["priority"], 0);
    let tasks = ok(p, &["list"]);
    assert_eq!(tasks[1]["description"], value["tasks"][0]["description"]);
    assert_eq!(tasks[1]["parent_id"], 1);
    assert_eq!(tasks[0]["priority"], -3);
    assert_eq!(tasks[1]["priority"], 5);
    assert_eq!(ok(p, &["next", "--dry-run"])["id"], 3);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM events", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn import_strict_validation_rejects_invalid_batches_before_writes() {
    let dir = project();
    let p = dir.path();
    let before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    let bad = [
        json!({"version":2,"tasks":[]}),
        json!({"tasks":[]}),
        json!({"version":1,"tasks":{},"extra":1}),
        batch(json!([{"key":"a","description":"one"},{"key":"a","description":"two"}])),
        batch(json!([{"key":" ","description":"one"}])),
        batch(json!([{"key":"a","description":" \n\t"}])),
        batch(json!([{"key":"a","description":"  \u{0}hidden"}])),
        batch(json!([{"key":"a","description":"one","priority":101}])),
        batch(json!([{"key":"a","description":"one","priority":-101}])),
        batch(json!([{"key":"a","description":"one","priority":1.5}])),
        batch(json!([{"key":"a","description":"one","priority":"1"}])),
        batch(json!([{"key":"a","description":"one","parent":{"id":0}}])),
        batch(json!([{"key":"a","description":"one","parent":{"id":-1}}])),
        batch(json!([{"key":"a","description":"one","parent":{"key":"missing"}}])),
        batch(json!([{"key":"a","description":"one","parent":{"key":"a"}}])),
        batch(
            json!([{"key":"a","description":"one","parent":{"key":"b"}},{"key":"b","description":"two","parent":{"key":"a"}}]),
        ),
        batch(json!([{"key":"a","description":"one","parent":{"id":1,"key":"a"}}])),
        batch(json!([{"key":"a","description":"one","parent":{}}])),
        batch(json!([{"key":"a","description":"one","images":[]}])),
        batch(json!([{"key":"a","description":false}])),
    ];
    for value in bad {
        for dry_run in [false, true] {
            let out = file_import(p, &value, dry_run);
            assert!(!out.status.success(), "{value}");
            assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
        }
    }
    for input in [
        &b"{"[..],
        &b"{\"version\":1,\"version\":1,\"tasks\":[]}"[..],
        &[0xff][..],
    ] {
        assert!(!stdin(p, &["import", "-"], input).status.success());
        assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
    }
    let empty = import_ok(p, &batch(json!([])), false);
    assert_eq!(empty["count"], 0);
    assert_eq!(empty["mapping"], json!({}));
    assert_eq!(ok(p, &["list"]), json!([]));
}

#[test]
fn import_existing_parents_follow_ordinary_add_rules_and_preview_resolution() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Existing"]);
    let value = batch(json!([{"key":"follow","description":"Follow","parent":{"id":1}}]));
    let preview = import_ok(p, &value, true);
    assert_eq!(preview["tasks"][0]["parent_id"], 1);
    assert!(preview["tasks"][0]["id"].is_null());
    ok(p, &["archive", "1"]);
    for dry_run in [false, true] {
        assert!(!file_import(p, &value, dry_run).status.success());
    }
    let missing = batch(
        json!([{"key":"first","description":"Valid"},{"key":"bad","description":"Bad","parent":{"id":999}}]),
    );
    assert!(!file_import(p, &missing, false).status.success());
    assert_eq!(
        ok(p, &["list", "--include-archived"])
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(p, &["unarchive", "1"]);
    ok(p, &["next", "--local", "--session", "done"]);
    ok(p, &["complete", "1", "--session", "done"]);
    ok(p, &["archive", "1"]);
    assert_eq!(import_ok(p, &value, false)["mapping"]["follow"], 2);
    assert_eq!(ok(p, &["next", "--dry-run"])["id"], 2);
}

#[test]
fn import_write_failure_rolls_back_tasks_activity_attachments_and_id_allocation() {
    let dir = project();
    let p = dir.path();
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    let sequences = || {
        conn.prepare("SELECT name,seq FROM sqlite_sequence ORDER BY name")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    let before_sequences = sequences();
    conn.execute_batch("CREATE TRIGGER fail_second_import BEFORE INSERT ON tasks WHEN NEW.description='Fail row' BEGIN SELECT RAISE(ABORT,'injected batch write failure'); END;").unwrap();
    let value = batch(
        json!([{"key":"root","description":"Root"},{"key":"fail","description":"Fail row","parent":{"key":"root"}}]),
    );
    let out = file_import(p, &value, false);
    assert!(!out.status.success());
    let failure: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(failure["code"], "DATABASE_ERROR");
    assert_eq!(failure["details"]["sqlite_extended_code"], 1811);
    for table in ["tasks", "events", "messages", "images", "herdr_links"] {
        assert_eq!(
            conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "{table}"
        );
    }
    assert_eq!(sequences(), before_sequences);
    assert!(!p.join(".qqq/images").exists());
    conn.execute_batch("DROP TRIGGER fail_second_import")
        .unwrap();
    assert_eq!(
        import_ok(p, &value, false)["mapping"],
        json!({"root":1,"fail":2})
    );
}

#[test]
fn import_dry_run_preserves_tables_files_staging_and_refuses_old_or_missing_projects() {
    let dir = project();
    let p = dir.path();
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    for table in ["tasks", "events", "messages", "images", "herdr_links"] {
        for operation in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!("CREATE TRIGGER import_readonly_{table}_{operation} BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT,'dry-run wrote project'); END;")).unwrap();
        }
    }
    let staging = p.join(".qqq/.delete-staging");
    std::fs::create_dir(&staging).unwrap();
    std::fs::write(staging.join("marker"), b"pending").unwrap();
    let before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    let preview = import_ok(p, &forward_batch(), true);
    assert_eq!(preview["count"], 3);
    assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
    assert_eq!(std::fs::read(staging.join("marker")).unwrap(), b"pending");
    assert!(!p.join(".qqq/images").exists());
    let missing = TempDir::new().unwrap();
    assert!(
        !file_import(missing.path(), &forward_batch(), true)
            .status
            .success()
    );
    assert!(!missing.path().join(".qqq").exists());
    let legacy = TempDir::new().unwrap();
    std::fs::create_dir(legacy.path().join(".qqq")).unwrap();
    let legacy_path = legacy.path().join(".qqq/qqq.db");
    Connection::open(&legacy_path)
        .unwrap()
        .execute_batch(include_str!("../../src/sql/schema.sql"))
        .unwrap();
    let before = std::fs::read(&legacy_path).unwrap();
    let out = file_import(legacy.path(), &forward_batch(), true);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("requires migration"));
    assert_eq!(std::fs::read(&legacy_path).unwrap(), before);
    // Static validation runs before normal open could migrate legacy data.
    assert!(
        !file_import(
            legacy.path(),
            &batch(json!([{"key":"bad","description":" "}])),
            false
        )
        .status
        .success()
    );
    assert_eq!(std::fs::read(&legacy_path).unwrap(), before);
}

#[test]
fn import_invalid_file_encoding_missing_file_and_duplicate_fields_fail_without_writes() {
    let dir = project();
    let p = dir.path();
    let before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    std::fs::write(p.join("bad.json"), [0xff, 0xfe]).unwrap();
    for name in ["bad.json", "missing.json"] {
        assert!(!run(p, &["import", name]).status.success());
    }
    for input in [
        r#"{"version":1,"tasks":[{"key":"a","key":"b","description":"Text"}]}"#,
        r#"{"version":1,"tasks":[{"key":"a","description":"Text","parent":{"id":1,"id":2}}]}"#,
    ] {
        assert!(
            !stdin(p, &["import", "-", "--dry-run"], input.as_bytes())
                .status
                .success()
        );
    }
    assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
}

#[test]
fn import_concurrent_batches_and_adds_keep_contiguous_ids_and_correct_links() {
    let dir = project();
    let p = dir.path();
    std::fs::write(
        p.join("batch.json"),
        serde_json::to_vec(&forward_batch()).unwrap(),
    )
    .unwrap();
    let mut children = Vec::new();
    for index in 0..8 {
        children.push((
            true,
            command(p)
                .args(["import", "batch.json"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ));
        children.push((
            false,
            command(p)
                .args(["add", &format!("External {index}")])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    for (is_import, child) in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        if is_import {
            let parent = report["mapping"]["parent"].as_i64().unwrap();
            assert_eq!(report["mapping"]["child"], parent + 1);
            assert_eq!(report["mapping"]["tail"], parent + 2);
            assert_eq!(report["tasks"][0]["parent_id"], parent);
            assert_eq!(
                ok(p, &["show", &(parent + 1).to_string()])["task"]["parent_id"],
                parent
            );
            for id in parent..=parent + 2 {
                assert!(ids.insert(id));
            }
        } else {
            assert!(ids.insert(report["id"].as_i64().unwrap()));
        }
    }
    assert_eq!(ids.len(), 32);
    assert_eq!(ok(p, &["list"]).as_array().unwrap().len(), 32);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    assert!(
        !conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
}

#[test]
fn import_parent_archival_and_import_are_serialized_without_unfinished_archived_parent() {
    for _ in 0..6 {
        let dir = project();
        let p = dir.path();
        ok(p, &["add", "Parent"]);
        let batch = batch(json!([{"key":"child","description":"Child","parent":{"id":1}}]));
        std::fs::write(p.join("batch.json"), serde_json::to_vec(&batch).unwrap()).unwrap();
        let import = command(p)
            .args(["import", "batch.json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let archive = command(p)
            .args(["archive", "1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let import = import.wait_with_output().unwrap();
        let archive = archive.wait_with_output().unwrap();
        assert_ne!(import.status.success(), archive.status.success());
        let tasks = ok(p, &["list", "--include-archived"]);
        if import.status.success() {
            assert_eq!(tasks.as_array().unwrap().len(), 2);
            assert_eq!(tasks[0]["archived"], false);
            assert_eq!(tasks[1]["parent_id"], 1);
        } else {
            assert_eq!(tasks.as_array().unwrap().len(), 1);
            assert_eq!(tasks[0]["archived"], true);
        }
    }
}

#[test]
fn import_human_output_shows_count_keys_ids_and_safe_preview_text() {
    let dir = project();
    let p = dir.path();
    let value = batch(json!([{"key":"key\u{1b}","description":"λ\nbody\u{1b}[31m"}]));
    std::fs::write(p.join("batch.json"), serde_json::to_vec(&value).unwrap()).unwrap();
    for dry_run in [true, false] {
        let mut cmd = human_command(p);
        cmd.args(["import", "batch.json"]).env("NO_COLOR", "1");
        if dry_run {
            cmd.arg("--dry-run");
        }
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(
            text.contains(if dry_run {
                "Validated 1 task"
            } else {
                "Imported 1 task"
            }),
            "{text}"
        );
        assert!(text.contains("key\\u{1b}"), "{text}");
        assert!(!text.contains('\u{1b}'));
        if !dry_run {
            assert!(text.contains("#1"));
        }
    }
}
