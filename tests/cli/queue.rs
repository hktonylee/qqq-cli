use super::{command, human_command, ok, project, run};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::path::Path;
use tempfile::TempDir;

fn row(report: &Value, id: i64) -> &Value {
    report["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["task"]["id"] == id)
        .unwrap()
}

fn fixture() -> TempDir {
    let dir = project();
    let path = dir.path();
    ok(path, &["add", "Parent"]);
    ok(
        path,
        &["add", "Child", "--parent", "1", "--priority", "100"],
    );
    ok(
        path,
        &["add", "Grandchild", "--parent", "2", "--priority", "100"],
    );
    ok(path, &["add", "First ready", "--priority", "9"]);
    ok(path, &["add", "Second ready", "--priority", "9"]);
    ok(path, &["add", "Done item", "--priority", "100"]);
    ok(path, &["add", "Failed item", "--priority", "100"]);
    ok(path, &["add", "Owned item", "--priority", "100"]);
    ok(path, &["add", "Archived item", "--priority", "100"]);
    ok(path, &["archive", "9"]);
    assert_eq!(ok(path, &["next", "--local", "--session", "done"])["id"], 6);
    ok(path, &["complete", "6", "--session", "done"]);
    assert_eq!(
        ok(path, &["next", "--local", "--session", "failed"])["id"],
        7
    );
    ok(
        path,
        &[
            "edit",
            "7",
            "--set-status",
            "error",
            "--reason",
            "Failure",
            "--session",
            "failed",
        ],
    );
    assert_eq!(
        ok(path, &["next", "--local", "--session", "active"])["id"],
        8
    );
    dir
}

#[test]
fn queue_diagnostics_empty_and_missing_project_do_not_create_state() {
    let missing = TempDir::new().unwrap();
    for args in [&["status"][..], &["next", "--explain"][..]] {
        let output = run(missing.path(), args);
        assert!(!output.status.success());
        assert!(!missing.path().join(".qqq").exists());
    }
    let dir = project();
    for args in [&["status"][..], &["next", "--explain"][..]] {
        let report = ok(dir.path(), args);
        assert_eq!(report["state"], "empty");
        assert_eq!(report["tasks"], json!([]));
        assert_eq!(
            report["counts"],
            json!({"total":0,"new":0,"ready":0,"blocked":0,
            "matching_ready":0,"in_progress":0,"error":0,"completed":0,"archived":0})
        );
        if args[0] == "next" {
            assert_eq!(report["explanation"]["outcome"], "empty_queue");
            assert!(report["explanation"]["selection"].is_null());
        } else {
            assert!(report["explanation"].is_null());
        }
    }
}

#[test]
fn queue_diagnostics_old_schema_is_not_migrated() {
    for version in [1, 9] {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".qqq")).unwrap();
        let path = dir.path().join(".qqq/qqq.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(include_str!("../../src/sql/schema.sql"))
            .unwrap();
        if version == 9 {
            for sql in [
                include_str!("../../src/sql/migrate_v2.sql"),
                include_str!("../../src/sql/migrate_v3.sql"),
                include_str!("../../src/sql/migrate_v4.sql"),
                include_str!("../../src/sql/migrate_v5.sql"),
                include_str!("../../src/sql/migrate_v6.sql"),
                include_str!("../../src/sql/migrate_v7.sql"),
                include_str!("../../src/sql/migrate_v8.sql"),
                include_str!("../../src/sql/migrate_v9.sql"),
            ] {
                conn.execute_batch(sql).unwrap();
            }
        }
        conn.execute("INSERT INTO tasks(description) VALUES ('Old task')", [])
            .unwrap();
        drop(conn);
        let before = std::fs::read(&path).unwrap();
        for args in [&["status"][..], &["next", "--explain"][..]] {
            let output = run(dir.path(), args);
            assert!(!output.status.success());
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains("requires migration") && error.contains("qqq list"),
                "{error}"
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            version
        );
    }
}

#[test]
fn queue_status_counts_readiness_blockers_owners_and_archive_scope() {
    let dir = fixture();
    let report = ok(dir.path(), &["status"]);
    assert_eq!(
        report["counts"],
        json!({"total":8,"new":5,"ready":3,"blocked":2,
        "matching_ready":3,"in_progress":1,"error":1,"completed":1,"archived":0})
    );
    assert_eq!(report["state"], "ready");
    assert_eq!(
        row(&report, 2)["blockers"],
        json!([{"id":1,"status":"new","archived":false}])
    );
    assert_eq!(
        row(&report, 3)["blockers"],
        json!([{"id":2,"status":"new","archived":false}])
    );
    assert_eq!(row(&report, 8)["owner"], "active");
    assert_eq!(row(&report, 4)["queue_rank"], 1);
    assert_eq!(row(&report, 5)["queue_rank"], 2);
    assert_eq!(row(&report, 1)["queue_rank"], 3);
    assert!(row(&report, 2)["queue_rank"].is_null());
    assert!(
        report["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["task"]["id"] != 9)
    );
    let included = ok(dir.path(), &["status", "--include-archived"]);
    assert_eq!(included["counts"]["total"], 9);
    assert_eq!(included["counts"]["ready"], 3);
    assert_eq!(included["counts"]["blocked"], 3);
    assert_eq!(included["counts"]["archived"], 1);
    assert_eq!(row(&included, 9)["ready"], false);
    assert!(
        row(&included, 9)["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("archived"))
    );
}

#[test]
fn queue_explain_matches_real_priority_filter_and_owner_reuse() {
    let dir = fixture();
    let path = dir.path();
    let global = ok(path, &["next", "--explain"]);
    assert_eq!(global["explanation"]["outcome"], "ready_candidate");
    assert_eq!(
        global["explanation"]["ordering"],
        json!(["priority_desc", "id_asc"])
    );
    assert_eq!(global["explanation"]["eligible_ids"], json!([4, 5, 1]));
    assert_eq!(global["explanation"]["selection"]["kind"], "queued");
    assert_eq!(
        global["explanation"]["selection"]["task"],
        ok(path, &["next", "--dry-run"])
    );
    let filtered = ok(path, &["next", "--explain", "--filter", "id == 5"]);
    assert_eq!(filtered["counts"]["ready"], 3);
    assert_eq!(filtered["counts"]["matching_ready"], 1);
    assert_eq!(
        filtered["explanation"]["selection"]["task"],
        ok(path, &["next", "--dry-run", "--filter", "id == 5"])
    );
    assert_eq!(row(&filtered, 4)["matches_filter"], false);
    assert!(
        row(&filtered, 4)["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("filter_excluded"))
    );
    let excluded = ok(path, &["next", "--explain", "--filter", "false"]);
    assert_eq!(excluded["state"], "no_matching_ready");
    assert_eq!(
        excluded["explanation"]["outcome"],
        "no_matching_ready_candidate"
    );
    assert!(excluded["explanation"]["selection"].is_null());
    let owned = ok(
        path,
        &[
            "next",
            "--explain",
            "--session",
            "active",
            "--filter",
            "false",
        ],
    );
    assert_eq!(owned["explanation"]["outcome"], "owned_task_reuse");
    assert_eq!(owned["explanation"]["selection"]["kind"], "owned");
    assert_eq!(
        owned["explanation"]["selection"]["task"],
        ok(
            path,
            &[
                "next",
                "--local",
                "--session",
                "active",
                "--filter",
                "false"
            ]
        )
    );
    assert_eq!(
        ok(path, &["next", "--dry-run", "--session", "active"])["id"],
        4
    );
    assert_eq!(
        ok(path, &["next", "--local", "--session", "fresh"])["id"],
        4
    );
    let claimed = ok(path, &["status"]);
    assert_eq!(claimed["counts"]["ready"], 2);
    assert_eq!(claimed["counts"]["in_progress"], 2);
    assert_eq!(row(&claimed, 4)["owner"], "fresh");
}

fn deny_mutations(path: &Path) -> Connection {
    let conn = Connection::open(path.join(".qqq/qqq.db")).unwrap();
    for table in ["tasks", "messages", "events", "herdr_links", "images"] {
        for operation in ["INSERT", "UPDATE", "DELETE"] {
            conn.execute_batch(&format!("CREATE TRIGGER diag_no_{table}_{operation} BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT, 'diagnostic mutated queue'); END;")).unwrap();
        }
    }
    conn
}

#[test]
fn queue_diagnostics_leave_claims_events_messages_and_identity_unchanged() {
    let dir = fixture();
    let path = dir.path();
    ok(path, &["message", "8", "Activity", "--session", "reviewer"]);
    let before: Vec<_> = (1..=9)
        .map(|id| ok(path, &["show", &id.to_string()]))
        .collect();
    let _conn = deny_mutations(path);
    for args in [
        vec!["status"],
        vec!["status", "--include-archived"],
        vec!["next", "--explain"],
        vec![
            "next",
            "--explain",
            "--session",
            "active",
            "--harness-name",
            "different",
            "--harness-session",
            "different",
            "--orchestrator-name",
            "different",
            "--filter",
            "false",
        ],
    ] {
        ok(path, &args);
    }
    for id in 1..=9 {
        assert_eq!(
            ok(path, &["show", &id.to_string()]),
            before[id as usize - 1]
        );
    }
}

#[test]
fn queue_states_distinguish_unavailable_queues_and_missing_blockers() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent"]);
    ok(p, &["add", "Child", "--parent", "1"]);
    ok(p, &["next", "--local", "--session", "worker"]);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    let cases = [
        (
            "UPDATE tasks SET status='error',claim_key=NULL",
            "error",
            "error_queue",
        ),
        (
            "UPDATE tasks SET status='in_progress',claim_key='owner-'||id",
            "in_progress",
            "in_progress_queue",
        ),
        (
            "UPDATE tasks SET status='completed',claim_key=NULL",
            "no_ready",
            "no_ready_tasks",
        ),
        (
            "UPDATE tasks SET status='new',claim_key=NULL WHERE id=2",
            "ready",
            "ready_candidate",
        ),
        (
            "UPDATE tasks SET status='error' WHERE id=1",
            "no_ready",
            "no_ready_tasks",
        ),
        (
            "UPDATE tasks SET archived=1 WHERE id=1",
            "blocked",
            "blocked_queue",
        ),
    ];
    for (sql, state, outcome) in cases {
        conn.execute_batch(sql).unwrap();
        let report = ok(p, &["next", "--explain"]);
        assert_eq!(report["state"], state, "{sql}");
        assert_eq!(report["explanation"]["outcome"], outcome, "{sql}");
    }
    let report = ok(p, &["status"]);
    assert_eq!(
        row(&report, 2)["blockers"],
        json!([{"id":1,"status":"error","archived":true}])
    );
    // A damaged foreign-key reference must remain unavailable, like real next.
    conn.execute_batch("PRAGMA foreign_keys=OFF; UPDATE tasks SET parent_id=99 WHERE id=2")
        .unwrap();
    let report = ok(p, &["next", "--explain"]);
    assert_eq!(
        row(&report, 2)["blockers"],
        json!([{"id":99,"status":null,"archived":null}])
    );
    assert!(report["explanation"]["selection"].is_null());
    assert!(ok(p, &["next", "--dry-run"]).is_null());
}

#[test]
fn queue_activity_uses_edits_messages_events_and_deterministic_ties_without_expiry() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Owned"]);
    ok(p, &["next", "--local", "--session", "worker"]);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    conn.execute_batch("UPDATE tasks SET updated_at='2001-01-01T00:00:00.000Z'; UPDATE events SET created_at='2000-01-01T00:00:00.000Z'").unwrap();
    let activity = |args: &[&str]| row(&ok(p, args), 1)["latest_activity"].clone();
    assert_eq!(
        activity(&["status"]),
        json!({"at":"2001-01-01T00:00:00.000Z","source":"task","id":null,"session":null,"action":null})
    );
    ok(p, &["message", "1", "Message one", "--session", "author"]);
    let message = activity(&["status"]);
    assert_eq!(message["source"], "message");
    assert_eq!(message["session"], "author");
    conn.execute_batch("UPDATE messages SET created_at='2002-01-01T00:00:00.000Z'; UPDATE events SET created_at='2003-01-01T00:00:00.000Z'").unwrap();
    assert_eq!(activity(&["status"])["source"], "event");
    assert_eq!(activity(&["status"])["action"], "claim");
    conn.execute_batch("UPDATE tasks SET updated_at='2004-01-01T00:00:00.000Z'; UPDATE messages SET created_at='2004-01-01T00:00:00.000Z'; UPDATE events SET created_at='2004-01-01T00:00:00.000Z'; INSERT INTO messages(task_id,body,session,created_at) VALUES (1,'Later ID','second','2004-01-01T00:00:00.000Z')").unwrap();
    assert_eq!(activity(&["status"])["session"], "second");
    conn.execute_batch("DELETE FROM messages").unwrap();
    assert_eq!(activity(&["status"])["source"], "event");
    conn.execute_batch("UPDATE tasks SET updated_at='2005-01-01T00:00:00.000Z'")
        .unwrap();
    assert_eq!(activity(&["status"])["source"], "task");
    let report = ok(p, &["next", "--explain", "--session", "worker"]);
    assert_eq!(report["explanation"]["outcome"], "owned_task_reuse");
    assert_eq!(row(&report, 1)["owner"], "worker");
    assert_eq!(ok(p, &["next", "--local", "--session", "worker"])["id"], 1);
}

#[test]
fn queue_owner_context_resolves_exact_harness_ambiguity_and_archived_reuse() {
    let dir = project();
    let p = dir.path();
    for description in ["First", "Second", "Queued"] {
        ok(p, &["add", description]);
    }
    for (key, name) in [("key-1", "codex"), ("key-2", "claude")] {
        ok(
            p,
            &[
                "next",
                "--local",
                "--session",
                key,
                "--harness-name",
                name,
                "--harness-session",
                "shared",
            ],
        );
    }
    let before = ok(p, &["show", "1"]);
    let ambiguous = run(p, &["next", "--explain", "--harness-session", "shared"]);
    assert!(!ambiguous.status.success());
    let failure: Value = serde_json::from_slice(&ambiguous.stderr).unwrap();
    assert_eq!(failure["code"], "INVALID_ARGUMENT");
    assert_eq!(failure["details"]["argument"], "--harness-name");
    assert_eq!(failure["details"]["matches"], 2);
    let resolved = ok(
        p,
        &[
            "next",
            "--explain",
            "--harness-session",
            "shared",
            "--harness-name",
            "codex",
        ],
    );
    assert_eq!(resolved["explanation"]["owner_input"], "shared");
    assert_eq!(resolved["explanation"]["resolved_owner"], "key-1");
    assert_eq!(resolved["explanation"]["selection"]["task"]["id"], 1);
    let exact = ok(
        p,
        &[
            "next",
            "--explain",
            "--session",
            "key-1",
            "--harness-name",
            "other",
        ],
    );
    assert_eq!(exact["explanation"]["selection"]["task"]["id"], 1);
    assert_eq!(ok(p, &["show", "1"]), before);
    let via_env = command(p)
        .args(["next", "--explain"])
        .env("QQQ_SESSION", "key-1")
        .output()
        .unwrap();
    assert!(via_env.status.success());
    let report: Value = serde_json::from_slice(&via_env.stdout).unwrap();
    assert_eq!(report["explanation"]["outcome"], "owned_task_reuse");
    // Normal archive rejects active tasks; simulate a legacy active archived row.
    Connection::open(p.join(".qqq/qqq.db"))
        .unwrap()
        .execute("UPDATE tasks SET archived=1 WHERE id=1", [])
        .unwrap();
    let archived = ok(
        p,
        &[
            "next",
            "--explain",
            "--session",
            "key-1",
            "--filter",
            "false",
        ],
    );
    assert_eq!(archived["explanation"]["selection"]["task"]["id"], 1);
    assert!(
        archived["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["task"]["id"] != 1)
    );
}

#[cfg(unix)]
#[test]
fn queue_diagnostics_skip_session_discovery_config_dispatch_and_recovery() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Candidate"]);
    let stub = p.join("herdr");
    std::fs::write(&stub, "#!/bin/sh\necho called >> herdr-calls\nexit 42\n").unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(p.join(".config/qqq")).unwrap();
    // Valid dispatch config and invalid config both must be irrelevant to diagnostics.
    for config in ["[herdr]\nnext-to-new-agent = true\n", "invalid = ["] {
        std::fs::write(p.join(".config/qqq/config.toml"), config).unwrap();
        for args in [
            vec!["status"],
            vec!["next", "--explain"],
            vec!["next", "--explain", "--harness-session", "unowned"],
        ] {
            let output = command(p)
                .args(&args)
                .env("PATH", p)
                .env("CODEX_THREAD_ID", "native")
                .env("HERDR_ENV", "native")
                .env("HERDR_PANE_ID", "pane")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            if args[0] == "next" {
                assert_eq!(report["explanation"]["selection"]["kind"], "queued");
                if args.len() == 2 {
                    assert!(report["explanation"]["owner_input"].is_null());
                }
            }
        }
    }
    assert!(!p.join("herdr-calls").exists());
    // Leave pending deletion staging untouched as well as queue tables.
    let _conn = deny_mutations(p);
    let staging = p.join(".qqq/.delete-staging");
    std::fs::create_dir(&staging).unwrap();
    ok(p, &["status"]);
    ok(p, &["next", "--explain"]);
    assert!(
        staging.is_dir(),
        "diagnostics must not run deletion recovery"
    );
}

#[test]
fn queue_invalid_flags_filters_and_owner_inputs_do_not_change_database() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Candidate"]);
    let before = std::fs::read(p.join(".qqq/qqq.db")).unwrap();
    for args in [
        vec!["next", "--explain", "--wait"],
        vec!["next", "--explain", "--dry-run"],
        vec!["next", "--include-archived"],
        vec!["next", "--explain", "--filter", "oops("],
        vec!["next", "--explain", "--session", " "],
    ] {
        assert!(!run(p, &args).status.success(), "{args:?}");
        assert_eq!(std::fs::read(p.join(".qqq/qqq.db")).unwrap(), before);
    }
}

#[test]
fn queue_human_output_counts_unfinished_tasks_and_escapes_controls() {
    let dir = fixture();
    let p = dir.path();
    ok(
        p,
        &["edit", "4", "--description", "Ready\u{1b}[31m\nSecond line"],
    );
    let output = human_output(p, &["status"]);
    assert!(
        output.contains("Ready: 3  Blocked: 2  In progress: 1  Error: 1  Completed: 1"),
        "{output}"
    );
    assert!(output.contains("Owner: active"));
    assert!(
        output.contains("Blocker: #1 (New, archived: false)"),
        "{output}"
    );
    assert!(output.contains("Ready\\u{1b}[31m"));
    assert!(!output.contains("Done item"));
    assert!(!output.contains("Second line"));
    assert!(!output.contains('\u{1b}'));
    let explain = human_output(p, &["next", "--explain", "--filter", "false"]);
    assert!(explain.contains("Done item"));
    assert!(explain.contains("none match filter"));
    assert!(explain.contains("filter excluded"));
    assert!(explain.contains("priority descending, ID ascending"));
    assert!(explain.contains("Selection: none."));
}

fn human_output(path: &Path, args: &[&str]) -> String {
    let output = human_command(path)
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn queue_concurrent_transitions_keep_counts_blockers_selection_and_activity_coherent() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Parent", "--priority", "10"]);
    ok(p, &["add", "Child", "--parent", "1", "--priority", "20"]);
    ok(p, &["add", "Fallback"]);
    let conn = Connection::open(p.join(".qqq/qqq.db")).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    let running = Arc::new(AtomicBool::new(true));
    let transitions = Arc::new(AtomicUsize::new(0));
    let writer_running = running.clone();
    let writer_transitions = transitions.clone();
    let db_path = p.join(".qqq/qqq.db");
    let writer = std::thread::spawn(move || {
        let mut conn = Connection::open(db_path).unwrap();
        conn.busy_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let mut generation = 0usize;
        while writer_running.load(Ordering::SeqCst) {
            generation += 1;
            let at = format!("2030-01-01T00:00:00.{generation:09}Z");
            let tx = conn.transaction().unwrap();
            let (status, owner) = match generation % 3 {
                0 => ("new", None),
                1 => ("completed", None),
                _ => ("in_progress", Some("worker")),
            };
            tx.execute(
                "UPDATE tasks SET status=?1,claim_key=?2,updated_at=?3 WHERE id=1",
                rusqlite::params![status, owner, at],
            )
            .unwrap();
            tx.execute("UPDATE tasks SET updated_at=? WHERE id=2", [&at])
                .unwrap();
            tx.execute("DELETE FROM messages", []).unwrap();
            for id in [1, 2] {
                tx.execute(
                    "INSERT INTO messages(task_id,body,created_at) VALUES (?1,'Transition',?2)",
                    rusqlite::params![id, at],
                )
                .unwrap();
            }
            tx.commit().unwrap();
            writer_transitions.store(generation, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    // Stop/join writer even if assertion panics, so failed tests cannot leave a worker.
    let result = std::panic::catch_unwind(|| {
        for _ in 0..24 {
            let report = ok(p, &["next", "--explain"]);
            let rows = report["tasks"].as_array().unwrap();
            let mut ready = Vec::new();
            let mut new = 0usize;
            for row in rows {
                let task = &row["task"];
                let is_new = task["status"] == "new";
                new += usize::from(is_new);
                let parent = task["parent_id"]
                    .as_i64()
                    .map(|id| rows.iter().find(|row| row["task"]["id"] == id).unwrap());
                let is_ready =
                    is_new && parent.is_none_or(|row| row["task"]["status"] == "completed");
                assert_eq!(row["ready"], is_ready);
                if is_ready {
                    ready.push(task);
                }
                if let Some(parent) = parent {
                    if is_new && !is_ready {
                        assert_eq!(row["blockers"][0]["status"], parent["task"]["status"]);
                    }
                }
                if task["id"] == 1 || task["id"] == 2 {
                    assert_eq!(row["latest_activity"]["at"], task["updated_at"]);
                }
            }
            ready.sort_by(|a, b| {
                b["priority"]
                    .as_i64()
                    .unwrap()
                    .cmp(&a["priority"].as_i64().unwrap())
                    .then(a["id"].as_i64().unwrap().cmp(&b["id"].as_i64().unwrap()))
            });
            assert_eq!(report["counts"]["new"], new);
            assert_eq!(report["counts"]["ready"], ready.len());
            assert_eq!(report["counts"]["blocked"], new - ready.len());
            assert_eq!(report["counts"]["matching_ready"], ready.len());
            assert_eq!(
                report["explanation"]["eligible_ids"],
                Value::Array(ready.iter().map(|task| task["id"].clone()).collect())
            );
            assert_eq!(report["explanation"]["selection"]["task"], *ready[0]);
            for (index, task) in ready.iter().enumerate() {
                assert_eq!(
                    row(&report, task["id"].as_i64().unwrap())["queue_rank"],
                    index + 1
                );
            }
        }
    });
    running.store(false, Ordering::SeqCst);
    writer.join().unwrap();
    assert!(transitions.load(Ordering::SeqCst) > 1);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
