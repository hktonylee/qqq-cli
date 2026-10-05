use super::{command, human_command, ok, project, run};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio};

fn fail(dir: &Path, args: &[&str], text: &str) {
    let out = run(dir, args);
    assert!(!out.status.success(), "{args:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(text),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn finish(dir: &Path, id: &str, owner: &str) {
    assert_eq!(
        ok(
            dir,
            &[
                "next",
                "--session",
                owner,
                "--filter",
                &format!("id == {id}")
            ]
        )["id"],
        id.parse::<i64>().unwrap()
    );
    ok(dir, &["complete", id, "--session", owner]);
}

#[test]
fn mixed_prerequisites_gate_candidates_and_queue_without_duplicating_tree() {
    let d = project();
    let p = d.path();
    for title in ["Parent", "API", "UI"] {
        ok(p, &["add", title]);
    }
    let task = ok(
        p,
        &[
            "add",
            "Integrate",
            "--parent",
            "1",
            "--depends-on",
            "2",
            "--depends-on",
            "3",
            "--priority",
            "100",
        ],
    );
    assert_eq!(
        task["prerequisites"],
        json!([{"id":2,"status":"new","archived":false},{"id":3,"status":"new","archived":false}])
    );
    assert!(ok(p, &["next", "--dry-run", "--filter", "id == 4"]).is_null());
    let queue = ok(p, &["next", "--explain", "--filter", "id == 4"]);
    let row = queue["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["task"]["id"] == 4)
        .unwrap();
    assert_eq!(row["blockers"].as_array().unwrap().len(), 3);
    assert!(
        row["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("prerequisite_not_completed"))
    );
    finish(p, "1", "p");
    finish(p, "2", "a");
    assert!(ok(p, &["next", "--dry-run", "--filter", "id == 4"]).is_null());
    finish(p, "3", "u");
    assert_eq!(
        ok(p, &["next", "--dry-run", "--filter", "id == 4"])["id"],
        4
    );
    assert_eq!(
        ok(p, &["list"])
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["id"] == 4)
            .count(),
        1
    );
    let human = human_command(p).args(["show", "4"]).output().unwrap();
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(text.contains("Prerequisites:"));
    assert!(text.contains("#2"));
    assert!(text.contains("completed"));
}

#[test]
fn dependency_edits_validate_final_graph_and_preserve_owner_revision_and_history() {
    let d = project();
    let p = d.path();
    for title in ["One", "Two", "Three"] {
        ok(p, &["add", title]);
    }
    let claimed = ok(p, &["next", "--session", "owner", "--filter", "id == 1"]);
    let events = ok(p, &["show", "1"])["events"].clone();
    let edited = ok(p, &["edit", "1", "--depends-on", "2"]);
    assert_eq!(edited["status"], claimed["status"]);
    assert_eq!(edited["harness_session"], claimed["harness_session"]);
    assert_eq!(edited["content_revision"], claimed["content_revision"]);
    assert_eq!(ok(p, &["show", "1"])["events"], events);
    assert_eq!(ok(p, &["next", "--session", "owner"])["id"], 1);
    for (args, error) in [
        (vec!["edit", "1", "--depends-on", "2"], "Duplicate"),
        (vec!["edit", "1", "--depends-on", "1"], "itself"),
        (vec!["edit", "1", "--depends-on", "999"], "not found"),
        (vec!["edit", "2", "--set-parent", "1"], "cycle"),
        (
            vec!["edit", "1", "--remove-depends-on", "3"],
            "not a prerequisite",
        ),
        (
            vec!["edit", "1", "--depends-on", "3", "--depends-on", "3"],
            "Duplicate",
        ),
        (
            vec!["edit", "1", "--depends-on", "2", "--remove-depends-on", "2"],
            "both",
        ),
    ] {
        fail(p, &args, error);
    }
    ok(
        p,
        &[
            "edit",
            "1",
            "--clear-depends-on",
            "--depends-on",
            "3",
            "--set-parent",
            "2",
        ],
    );
    fail(p, &["edit", "1", "--depends-on", "2"], "parent");
    ok(p, &["edit", "1", "--remove-depends-on", "3"]);
    assert_eq!(ok(p, &["show", "1"])["task"]["prerequisites"], json!([]));
    ok(
        p,
        &["edit", "1", "--set-parent", "none", "--depends-on", "2"],
    );
    assert_eq!(ok(p, &["show", "1"])["task"]["parent_id"], Value::Null);
    ok(p, &["edit", "1", "--clear-depends-on"]);
    ok(p, &["edit", "1", "--clear-depends-on"]);
}

#[test]
fn prerequisite_archive_error_reopen_and_active_claim_rules_match_parent_safety() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "API"]);
    ok(p, &["add", "Join", "--depends-on", "1"]);
    fail(p, &["archive", "1"], "unfinished");
    ok(p, &["next", "--session", "a"]);
    ok(
        p,
        &[
            "edit",
            "1",
            "--set-status",
            "error",
            "--reason",
            "failed",
            "--session",
            "a",
        ],
    );
    assert!(ok(p, &["next", "--dry-run"]).is_null());
    ok(p, &["edit", "1", "--set-status", "new"]);
    finish(p, "1", "a");
    ok(p, &["archive", "1"]);
    assert_eq!(ok(p, &["next", "--session", "join"])["id"], 2);
    ok(p, &["unarchive", "1"]);
    ok(p, &["reopen", "1"]);
    assert_eq!(ok(p, &["next", "--session", "join"])["id"], 2);
    ok(
        p,
        &["edit", "2", "--set-status", "new", "--session", "join"],
    );
    assert!(ok(p, &["next", "--dry-run", "--filter", "id == 2"]).is_null());
    ok(p, &["archive", "2"]);
    ok(p, &["archive", "1"]);
    fail(p, &["unarchive", "2"], "archived unfinished");
    fail(
        p,
        &["add", "Later", "--depends-on", "1"],
        "archived unfinished",
    );
    fail(p, &["delete", "1"], "prerequisite");
    ok(p, &["edit", "2", "--clear-depends-on"]);
    ok(p, &["unarchive", "2"]);
}

#[test]
fn combined_graph_cycles_and_concurrent_reciprocal_edits_are_atomic() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "One"]);
    ok(p, &["add", "Two", "--parent", "1"]);
    ok(p, &["add", "Three", "--depends-on", "2"]);
    fail(
        p,
        &[
            "edit",
            "1",
            "--depends-on",
            "3",
            "--description",
            "Rejected",
        ],
        "cycle",
    );
    assert_eq!(ok(p, &["show", "1"])["task"]["description"], "One");
    ok(p, &["edit", "2", "--set-parent", "none"]);
    ok(p, &["edit", "3", "--clear-depends-on"]);
    let a = command(p)
        .args(["edit", "1", "--depends-on", "2"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let b = command(p)
        .args(["edit", "2", "--depends-on", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let results = [a.wait_with_output().unwrap(), b.wait_with_output().unwrap()];
    assert_eq!(results.iter().filter(|r| r.status.success()).count(), 1);
    assert!(
        results
            .iter()
            .filter(|r| !r.status.success())
            .all(|r| String::from_utf8_lossy(&r.stderr).contains("cycle"))
    );
}

#[test]
fn import_prerequisites_resolve_forward_refs_and_reject_mixed_cycles_without_writes() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Existing"]);
    let batch = json!({"version":1,"tasks":[
        {"key":"join","description":"Join","parent":{"key":"parent"},"depends_on":[{"key":"api"},{"id":1}]},
        {"key":"parent","description":"Parent"},
        {"key":"api","description":"API"}
    ]});
    let path = p.join("batch.json");
    std::fs::write(&path, serde_json::to_vec(&batch).unwrap()).unwrap();
    let before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    let preview = ok(p, &["import", "batch.json", "--dry-run"]);
    assert_eq!(preview["creation_order"], json!(["parent", "api", "join"]));
    assert_eq!(preview["tasks"][0]["prerequisite_ids"], json!([null, 1]));
    assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
    let result = ok(p, &["import", "batch.json"]);
    assert_eq!(result["mapping"], json!({"parent":2,"api":3,"join":4}));
    assert_eq!(result["tasks"][0]["prerequisite_ids"], json!([3, 1]));
    assert_eq!(
        ok(p, &["show", "4"])["task"]["prerequisites"],
        json!([{"id":1,"status":"new","archived":false},{"id":3,"status":"new","archived":false}])
    );
    let tasks = ok(p, &["list"]);
    for rows in [
        json!([{"key":"a","description":"A","depends_on":[{"key":"a"}]}]),
        json!([{"key":"a","description":"A","parent":{"key":"b"}},{"key":"b","description":"B","depends_on":[{"key":"a"}]}]),
        json!([{"key":"a","description":"A","parent":{"id":1},"depends_on":[{"id":1}]}]),
        json!([{"key":"a","description":"A","depends_on":[{"id":1},{"id":1}]}]),
        json!([{"key":"a","description":"A","depends_on":[{"key":"missing"}]}]),
        json!([{"key":"a","description":"A","depends_on":[{"id":999}]}]),
    ] {
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({"version":1,"tasks":rows})).unwrap(),
        )
        .unwrap();
        for suffix in [vec![], vec!["--dry-run"]] {
            let mut args = vec!["import", "batch.json"];
            args.extend(suffix);
            assert!(!run(p, &args).status.success());
            assert_eq!(ok(p, &["list"]), tasks);
        }
    }
}

#[test]
fn migration_and_snapshot_round_trip_preserve_ownership_content_and_dependency_edges() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Child", "--parent", "1"]);
    ok(p, &["next", "--session", "owner"]);
    let before = ok(p, &["show", "1"]);
    let list = ok(p, &["list"]);
    let db = p.join(".qqq/qqq.db");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("DROP TABLE task_dependencies; ALTER TABLE tasks DROP COLUMN tags; PRAGMA user_version=10;")
            .unwrap();
    }
    assert_eq!(ok(p, &["list"]), list);
    assert_eq!(ok(p, &["show", "1"]), before);
    assert_eq!(
        rusqlite::Connection::open(&db)
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        12
    );
    ok(p, &["add", "Extra"]);
    ok(p, &["edit", "2", "--depends-on", "3"]);
    let detail = ok(p, &["show", "2"]);
    ok(p, &["backup", "snapshot.tar"]);
    let restored = tempfile::TempDir::new().unwrap();
    ok(
        restored.path(),
        &["restore", p.join("snapshot.tar").to_str().unwrap()],
    );
    assert_eq!(ok(restored.path(), &["show", "2"]), detail);
    assert_eq!(
        ok(restored.path(), &["next", "--session", "owner"])["id"],
        1
    );
    assert!(
        ok(
            restored.path(),
            &["next", "--dry-run", "--filter", "id == 2"]
        )
        .is_null()
    );
    assert_eq!(ok(restored.path(), &["doctor"])["ok"], true);
}

#[test]
fn doctor_and_backup_detect_combined_dependency_cycles_and_missing_refs() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "One"]);
    ok(p, &["add", "Two", "--parent", "1"]);
    let db = p.join(".qqq/qqq.db");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute("INSERT INTO task_dependencies VALUES(1,2)", [])
            .unwrap();
    }
    let out = run(p, &["doctor"]);
    assert!(!out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["code"] == "DB_DEPENDENCIES")
    );
    fail(p, &["backup", "bad.tar"], "cycle");
    assert!(!p.join("bad.tar").exists());
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=OFF; DELETE FROM task_dependencies; INSERT INTO task_dependencies VALUES(2,999)",
        )
        .unwrap();
    }
    let out = run(p, &["doctor"]);
    assert!(!out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["code"] == "DB_DEPENDENCIES")
    );
    assert!(ok(p, &["next", "--dry-run", "--filter", "id == 2"]).is_null());
}

#[test]
fn concurrent_dependency_edits_and_claims_keep_committed_readiness_and_active_ownership() {
    for _ in 0..12 {
        let d = project();
        let p = d.path();
        ok(p, &["add", "Prerequisite"]);
        ok(p, &["add", "Dependent"]);
        let edit = command(p)
            .args(["edit", "2", "--depends-on", "1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let claim = command(p)
            .args(["next", "--session", "worker", "--filter", "id == 2"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        assert!(edit.wait_with_output().unwrap().status.success());
        let out = claim.wait_with_output().unwrap();
        assert!(out.status.success());
        let claimed: Value = serde_json::from_slice(&out.stdout).unwrap();
        let task = ok(p, &["show", "2"])["task"].clone();
        assert_eq!(task["prerequisites"].as_array().unwrap().len(), 1);
        if claimed.is_null() {
            assert_eq!(task["status"], "new");
        } else {
            assert_eq!(claimed["prerequisites"], json!([]));
            assert_eq!(task["status"], "in_progress");
            assert_eq!(ok(p, &["next", "--session", "worker"])["id"], 2);
            ok(
                p,
                &["edit", "2", "--set-status", "new", "--session", "worker"],
            );
        }
        assert!(ok(p, &["next", "--dry-run", "--filter", "id == 2"]).is_null());
    }
}

#[test]
fn wait_claim_unblocks_only_after_all_prerequisites_complete() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "One"]);
    ok(p, &["add", "Two"]);
    ok(
        p,
        &["add", "Join", "--depends-on", "1", "--depends-on", "2"],
    );
    let child = command(p)
        .args([
            "next",
            "--local",
            "--wait",
            "--session",
            "waiter",
            "--filter",
            "id == 3",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        send.send(child.wait_with_output().unwrap()).unwrap();
    });
    finish(p, "1", "one");
    assert!(
        receive
            .recv_timeout(std::time::Duration::from_millis(350))
            .is_err()
    );
    finish(p, "2", "two");
    let out = receive
        .recv_timeout(std::time::Duration::from_secs(8))
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["id"],
        3
    );
}

#[test]
fn completed_dependents_keep_status_and_reopen_validates_archived_prerequisites() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "API"]);
    finish(p, "1", "a");
    ok(p, &["add", "Join", "--depends-on", "1"]);
    finish(p, "2", "b");
    ok(p, &["add", "Downstream", "--depends-on", "2"]);
    let dependent = ok(p, &["show", "2"]);
    ok(p, &["reopen", "1"]);
    assert_eq!(ok(p, &["show", "2"])["task"]["status"], "completed");
    assert_eq!(ok(p, &["show", "2"])["events"], dependent["events"]);
    assert_eq!(
        ok(p, &["next", "--dry-run", "--filter", "id == 3"])["id"],
        3
    );
    ok(p, &["archive", "1"]);
    fail(p, &["reopen", "2"], "archived unfinished prerequisite");
    ok(p, &["edit", "2", "--clear-depends-on", "--depends-on", "1"]);
    assert_eq!(ok(p, &["show", "2"])["task"]["status"], "completed");
    ok(p, &["edit", "2", "--clear-depends-on"]);
    ok(p, &["reopen", "2"]);
    assert!(ok(p, &["next", "--dry-run", "--filter", "id == 3"]).is_null());
}

#[test]
fn invalid_prerequisite_options_fail_before_editor_and_roll_back_other_fields() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "One"]);
    ok(p, &["add", "Two"]);
    for args in [
        vec!["add", "Bad", "--depends-on", "0"],
        vec!["add", "Bad", "--parent", "1", "--depends-on", "1"],
        vec!["add", "Bad", "--depends-on", "1", "--depends-on", "1"],
        vec![
            "edit",
            "2",
            "--clear-depends-on",
            "--remove-depends-on",
            "1",
        ],
        vec!["edit", "2", "--edit", "--depends-on", "1"],
    ] {
        assert!(!run(p, &args).status.success());
    }
    let out = command(p)
        .args(["add", "--depends-on", "999"])
        .env("EDITOR", "qqq-no-such-editor")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("Task 999 not found"));
    assert_eq!(ok(p, &["list"]).as_array().unwrap().len(), 2);
    let before = ok(p, &["show", "2"]);
    fail(
        p,
        &[
            "edit",
            "2",
            "--description",
            "Rejected",
            "--priority",
            "100",
            "--depends-on",
            "999",
        ],
        "not found",
    );
    assert_eq!(ok(p, &["show", "2"]), before);
}

#[test]
fn import_edge_write_failure_rolls_back_tasks_edges_and_id_allocations() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "Existing"]);
    let db = p.join(".qqq/qqq.db");
    let conn = rusqlite::Connection::open(&db).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_second_edge BEFORE INSERT ON task_dependencies WHEN NEW.task_id=3 BEGIN SELECT RAISE(ABORT,'edge write failed'); END;").unwrap();
    let batch = json!({"version":1,"tasks":[
        {"key":"first","description":"First","depends_on":[{"id":1}]},
        {"key":"second","description":"Second","depends_on":[{"key":"first"}]}
    ]});
    std::fs::write(p.join("batch.json"), serde_json::to_vec(&batch).unwrap()).unwrap();
    let before = ok(p, &["list"]);
    let failed = run(p, &["import", "batch.json"]);
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
    let error: Value = serde_json::from_slice(&failed.stderr).unwrap();
    assert_eq!(error["code"], "DATABASE_ERROR");
    assert_eq!(error["details"]["sqlite_extended_code"], 1811);
    assert_eq!(ok(p, &["list"]), before);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM task_dependencies", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT seq FROM sqlite_sequence WHERE name='tasks'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    conn.execute_batch("DROP TRIGGER fail_second_edge").unwrap();
    assert_eq!(
        ok(p, &["import", "batch.json"])["mapping"],
        json!({"first":2,"second":3})
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM task_dependencies", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn prerequisite_errors_preserve_structured_json_codes_and_details() {
    let d = project();
    let p = d.path();
    ok(p, &["add", "API"]);
    ok(p, &["add", "Join", "--depends-on", "1"]);
    for (args, code) in [
        (vec!["add", "Bad", "--depends-on", "999"], "TASK_NOT_FOUND"),
        (vec!["edit", "2", "--depends-on", "1"], "INVALID_ARGUMENT"),
        (vec!["edit", "1", "--depends-on", "2"], "INVALID_ARGUMENT"),
        (
            vec!["edit", "2", "--remove-depends-on", "999"],
            "INVALID_ARGUMENT",
        ),
        (vec!["archive", "1"], "INVALID_TRANSITION"),
    ] {
        let output = run(p, &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], code);
        assert_eq!(error["details"]["command"], args[0]);
    }
    let error: Value =
        serde_json::from_slice(&run(p, &["add", "Bad", "--depends-on", "999"]).stderr).unwrap();
    assert_eq!(error["details"]["task_id"], 999);
}
