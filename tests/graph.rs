use serde_json::Value;
use std::{
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    command
        .current_dir(dir)
        .env("HOME", dir)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    command
}
fn run(dir: &Path, args: &[&str]) -> Output {
    command(dir).arg("--json").args(args).output().unwrap()
}
fn ok(dir: &Path, args: &[&str]) -> Value {
    let out = run(dir, args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

#[test]
fn graph_separates_two_blockers_and_immediate_versus_transitive_impact() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "A"]);
    ok(p, &["add", "B"]);
    ok(
        p,
        &[
            "add",
            "Integration",
            "--depends-on",
            "1",
            "--depends-on",
            "2",
        ],
    );
    ok(p, &["add", "Direct", "--parent", "1"]);
    ok(p, &["add", "Later", "--parent", "4"]);
    let graph = ok(p, &["graph", "1"]);
    assert_eq!(graph["format_version"], 1);
    assert_eq!(graph["impact"]["direct_dependents"], 2);
    assert_eq!(graph["impact"]["transitive_dependents"], 3);
    assert_eq!(graph["impact"]["indirect_dependents"], 1);
    assert_eq!(graph["impact"]["immediately_ready"], 1);
    assert_eq!(graph["impact"]["still_blocked"], 1);
    assert!(
        graph["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["dependent"] == 4
                && edge["prerequisite"] == 1
                && edge["kind"] == "parent")
    );
    assert!(
        graph["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["dependent"] == 3
                && edge["prerequisite"] == 1
                && edge["kind"] == "prerequisite")
    );
    let integration = ok(p, &["graph", "3", "--direction", "upstream"]);
    assert_eq!(integration["nodes"].as_array().unwrap().len(), 3);
    let first = &integration["nodes"][0];
    assert_eq!(first["id"], 1);
    assert_eq!(first["blocks_focus"], true);
    assert_eq!(first["upstream_depth"], 1);
    assert_eq!(first["downstream_depth"], Value::Null);
}

#[test]
fn graph_browsing_is_read_only_and_missing_references_preserve_error_contracts() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "One"]);
    ok(p, &["add", "Two", "--depends-on", "1"]);
    ok(
        p,
        &[
            "next",
            "--local",
            "--session",
            "owner",
            "--filter",
            "id == 1",
        ],
    );
    let db = p.join(".qqq/qqq.db");
    let before = std::fs::read(&db).unwrap();
    let graph = ok(p, &["graph", "-1"]);
    assert_eq!(graph["task_id"], 2);
    assert_eq!(graph["focus"]["ready"], false);
    assert_eq!(std::fs::read(&db).unwrap(), before);
    for (reference, code) in [
        ("999", "TASK_NOT_FOUND"),
        ("-999", "TASK_NOT_FOUND"),
        ("0", "INVALID_ARGUMENT"),
    ] {
        let out = run(p, &["graph", reference]);
        assert!(!out.status.success() && out.stdout.is_empty());
        let error: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(error["code"], code);
        assert_eq!(error["details"]["command"], "graph");
        assert_eq!(std::fs::read(&db).unwrap(), before);
    }
    for args in [
        vec!["graph", "1", "--depth", "129"],
        vec!["graph", "1", "--max-nodes", "0"],
        vec!["graph", "1", "--max-edges", "0"],
        vec!["graph", "1", "--direction", "sideways"],
    ] {
        let out = run(p, &args);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
            "INVALID_ARGUMENT"
        );
        assert_eq!(std::fs::read(&db).unwrap(), before);
    }
}

fn files(path: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut found = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            found.extend(files(&entry.path()));
        } else {
            found.insert(entry.path(), std::fs::read(entry.path()).unwrap());
        }
    }
    found
}

#[test]
fn graph_never_mutates_project_files_and_refuses_schema_migration() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Root"]);
    std::fs::write(p.join(".qqq.toml"), "include_archived = true\n").unwrap();
    std::fs::write(p.join(".qqq-views.json"), "preserve even malformed views").unwrap();
    let db = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    db.execute_batch("CREATE TRIGGER deny_task_update BEFORE UPDATE ON tasks BEGIN SELECT RAISE(ABORT,'graph attempted mutation'); END;
        CREATE TRIGGER deny_task_insert BEFORE INSERT ON tasks BEGIN SELECT RAISE(ABORT,'graph attempted mutation'); END;
        CREATE TRIGGER deny_task_delete BEFORE DELETE ON tasks BEGIN SELECT RAISE(ABORT,'graph attempted mutation'); END;").unwrap();
    drop(db);
    let before = files(p);
    assert_eq!(ok(p, &["graph", "1"])["task_id"], 1);
    assert_eq!(files(p), before);
    let db = rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap();
    db.pragma_update(None, "user_version", 12).unwrap();
    drop(db);
    let old = files(p);
    let out = run(p, &["graph", "1"]);
    assert!(!out.status.success() && out.stdout.is_empty());
    let error: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(error["code"], "DATABASE_ERROR");
    assert_eq!(error["details"]["reason"], "migration_required");
    assert_eq!(files(p), old);
}

#[test]
fn graph_human_output_is_plain_sanitized_and_deterministic() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Unicode 雪 👩‍💻\u{1b}[31m\t title"]);
    ok(p, &["add", "Child", "--parent", "1"]);
    let baseline = command(p).args(["--human", "graph", "1"]).output().unwrap();
    assert!(baseline.status.success() && baseline.stderr.is_empty());
    let text = String::from_utf8(baseline.stdout.clone()).unwrap();
    assert!(text.contains("Dependency graph #1") && text.contains("雪 👩‍💻"));
    assert!(text.contains("Immediately ready: 1") && text.contains("#2 <- #1 (parent"));
    assert!(!text.contains('\u{1b}') && !text.contains('\t'));
    for (key, value) in [("NO_COLOR", "1"), ("TERM", "dumb")] {
        let out = command(p)
            .env(key, value)
            .args(["--human", "graph", "1"])
            .output()
            .unwrap();
        assert_eq!(out.stdout, baseline.stdout);
        assert!(out.stderr.is_empty());
        let out = command(p)
            .env(key, value)
            .args(["--json", "graph", "1"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            report["focus"]["task_name"],
            "Unicode 雪 👩‍💻\u{1b}[31m\t title"
        );
        assert!(!out.stdout.contains(&0x1b));
    }
    assert_eq!(
        run(p, &["graph", "1"]).stdout,
        run(p, &["graph", "1"]).stdout
    );
}

fn node(report: &Value, id: i64) -> &Value {
    report["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == id)
        .unwrap()
}
fn database(p: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(p.join(".qqq/qqq.db")).unwrap()
}

#[test]
fn graph_diamond_counts_each_descendant_once_and_matches_live_readiness() {
    let dir = project();
    let p = dir.path();
    ok(p, &["add", "Root"]);
    ok(p, &["add", "Left", "--parent", "1"]);
    ok(p, &["add", "Right", "--depends-on", "1"]);
    ok(p, &["add", "Shared", "--parent", "2", "--depends-on", "3"]);
    ok(p, &["add", "Leaf", "--parent", "4"]);
    let report = ok(p, &["graph", "1"]);
    assert_eq!(report["impact"]["direct_dependents"], 2);
    assert_eq!(report["impact"]["transitive_dependents"], 4);
    assert_eq!(report["impact"]["indirect_dependents"], 2);
    assert_eq!(report["impact"]["immediately_ready"], 2);
    assert_eq!(node(&report, 4)["downstream_depth"], 2);
    assert_eq!(report["nodes"].as_array().unwrap().len(), 5);
    assert_eq!(
        ok(p, &["graph", "5", "--direction", "downstream"])["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(p, &["next", "--local", "--session", "owner"]);
    ok(p, &["complete", "1", "--session", "owner"]);
    let report = ok(p, &["graph", "1"]);
    assert_eq!(report["impact"]["immediately_ready"], 0);
    assert_eq!(report["impact"]["already_ready"], 2);
    let ready = ok(p, &["list", "--readiness", "ready"]);
    assert_eq!(
        ready
            .as_array()
            .unwrap()
            .iter()
            .filter(|task| task["context_only"] != true)
            .map(|task| task["id"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    let up = ok(p, &["graph", "4", "--direction", "upstream"]);
    assert!(node(&up, 2)["blocks_focus"].as_bool().unwrap());
    assert_eq!(
        node(&up, 1)["blocks_focus"],
        false,
        "completed root terminates blocker traversal"
    );
    assert_eq!(
        node(&up, 1)["upstream_depth"],
        2,
        "historical edge remains visible"
    );
}

#[test]
fn graph_mixed_states_and_two_prerequisites_share_queue_readiness() {
    let dir = project();
    let p = dir.path();
    let db = database(p);
    db.execute_batch("INSERT INTO tasks(id,description,status,archived,claim_key) VALUES
        (1,'Root','new',0,NULL),(2,'Failed blocker','error',0,NULL),(3,'Archived blocker','new',1,NULL),
        (4,'Archived completed','completed',1,NULL),(5,'Immediate','new',0,NULL),(6,'Two blockers','new',0,NULL),
        (7,'Archived dependent','new',1,NULL),(8,'Running dependent','in_progress',0,'manual'),(9,'Done dependent','completed',0,NULL),
        (10,'Failed dependent','error',0,NULL),(11,'Archived dependency candidate','new',0,NULL);
        INSERT INTO task_dependencies(task_id,prerequisite_id) VALUES
        (5,1),(5,4),(6,1),(6,2),(7,1),(8,1),(9,1),(10,1),(11,1),(11,3);").unwrap();
    drop(db);
    let report = ok(p, &["graph", "1"]);
    assert_eq!(report["impact"]["direct_dependents"], 7);
    assert_eq!(report["impact"]["immediately_ready"], 1);
    assert_eq!(report["impact"]["still_blocked"], 2);
    assert_eq!(report["impact"]["inactive_dependents"], 4);
    assert_eq!(node(&report, 5)["remaining_after_completion"], 0);
    assert_eq!(node(&report, 6)["remaining_after_completion"], 1);
    let blocked = ok(p, &["graph", "6", "--direction", "upstream"]);
    assert_eq!(node(&blocked, 2)["reasons"], serde_json::json!(["error"]));
    let archived = ok(p, &["graph", "11", "--direction", "upstream"]);
    assert_eq!(
        node(&archived, 3)["reasons"],
        serde_json::json!(["archived"])
    );
    let db = database(p);
    db.execute("UPDATE tasks SET status='completed' WHERE id=1", [])
        .unwrap();
    drop(db);
    assert_eq!(ok(p, &["graph", "1"])["impact"]["already_ready"], 1);
    let ready = ok(p, &["list", "--readiness", "ready"]);
    assert_eq!(
        ready
            .as_array()
            .unwrap()
            .iter()
            .filter(|task| task["context_only"] != true)
            .map(|task| task["id"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![5]
    );
    let db = database(p);
    db.execute("UPDATE tasks SET status='completed' WHERE id=2", [])
        .unwrap();
    drop(db);
    assert_eq!(ok(p, &["graph", "6"])["focus"]["ready"], true);
    assert_eq!(ok(p, &["graph", "11"])["focus"]["ready"], false);
}

#[test]
fn graph_corrupt_cycles_dangling_links_and_duplicate_edge_kinds_remain_read_only() {
    let dir = project();
    let p = dir.path();
    let db = database(p);
    db.execute_batch("PRAGMA foreign_keys=OFF;
        INSERT INTO tasks(id,description,parent_id) VALUES (1,'Cycle root',2),(2,'Cycle child',1),(3,'Missing parent',900),(4,'Missing extra',NULL);
        INSERT INTO task_dependencies(task_id,prerequisite_id) VALUES (2,1),(4,901),(902,1);").unwrap();
    drop(db);
    let before = files(p);
    let report = ok(p, &["graph", "1"]);
    assert_eq!(report["diagnostics"]["cycle_detected"], true);
    assert_eq!(report["diagnostics"]["missing_references"], 3);
    assert_eq!(report["impact"]["direct_dependents"], 1);
    assert_eq!(report["impact"]["transitive_dependents"], 1);
    assert_eq!(node(&report, 2)["unfinished_dependencies"], 1);
    assert_eq!(node(&report, 902)["status"], Value::Null);
    assert_eq!(
        node(&report, 902)["reasons"],
        serde_json::json!(["missing_task"])
    );
    let edges = report["edges"].as_array().unwrap();
    assert_eq!(
        edges
            .iter()
            .filter(|edge| edge["dependent"] == 2 && edge["prerequisite"] == 1)
            .count(),
        2
    );
    let missing = ok(p, &["graph", "3"]);
    assert_eq!(node(&missing, 900)["blocks_focus"], true);
    assert_eq!(missing["focus"]["ready"], false);
    let missing = ok(p, &["graph", "4"]);
    assert_eq!(node(&missing, 901)["blocks_focus"], true);
    assert_eq!(files(p), before);
}

#[test]
fn graph_deep_and_shared_layers_have_exact_totals_and_bounded_presentation() {
    let dir = project();
    let p = dir.path();
    let mut db = database(p);
    let tx = db.transaction().unwrap();
    for id in 1..=1800 {
        tx.execute(
            "INSERT INTO tasks(id,description,parent_id) VALUES (?1,?2,?3)",
            rusqlite::params![id, format!("Chain {id}"), (id > 1).then_some(id - 1)],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let report = ok(
        p,
        &[
            "graph",
            "1",
            "--depth",
            "128",
            "--max-nodes",
            "20",
            "--max-edges",
            "5",
        ],
    );
    assert_eq!(report["impact"]["transitive_dependents"], 1799);
    assert_eq!(report["impact"]["immediately_ready"], 1);
    assert_eq!(report["nodes"].as_array().unwrap().len(), 20);
    assert_eq!(report["limits"]["omitted_by_depth"], 1671);
    assert_eq!(report["limits"]["omitted_by_node_limit"], 109);
    assert_eq!(report["limits"]["omitted_edges"], 14);
    assert_eq!(report["edges"].as_array().unwrap().len(), 5);
    assert_eq!(
        ok(
            p,
            &["graph", "1800", "--direction", "upstream", "--depth", "0"]
        )["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let tx = db.transaction().unwrap();
    for id in 2000..=2119 {
        tx.execute(
            "INSERT INTO tasks(id,description) VALUES (?,?)",
            rusqlite::params![id, format!("Layer {id}")],
        )
        .unwrap();
        if id >= 2002 {
            let previous = 2000 + ((id - 2000) / 2 - 1) * 2;
            for prerequisite in previous..=previous + 1 {
                tx.execute(
                    "INSERT INTO task_dependencies(task_id,prerequisite_id) VALUES (?,?)",
                    [id, prerequisite],
                )
                .unwrap();
            }
        }
    }
    tx.commit().unwrap();
    drop(db);
    let report = ok(p, &["graph", "2000", "--max-nodes", "15"]);
    assert_eq!(report["impact"]["direct_dependents"], 2);
    assert_eq!(report["impact"]["transitive_dependents"], 118);
    assert_eq!(
        report["impact"]["immediately_ready"], 0,
        "other root remains unfinished"
    );
    assert_eq!(report["nodes"].as_array().unwrap().len(), 15);
    assert!(
        report["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|node| node["id"] != 2001),
        "upstream then downstream would incorrectly include sibling root"
    );
    assert_eq!(
        run(p, &["graph", "2000", "--max-nodes", "15"]).stdout,
        run(p, &["graph", "2000", "--max-nodes", "15"]).stdout
    );
}

#[test]
fn graph_bulk_reads_share_one_snapshot_during_dependency_changes() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let dir = project();
    let p = dir.path();
    let db = database(p);
    db.execute_batch("PRAGMA journal_mode=DELETE;
        INSERT INTO tasks(id,description,status) VALUES (1,'Done','completed'),(2,'Candidate','new'),(3,'Waiting','new');
        INSERT INTO task_dependencies VALUES (2,1);").unwrap();
    drop(db);
    let stop = Arc::new(AtomicBool::new(false));
    let writer_stop = stop.clone();
    let db_path = p.join(".qqq/qqq.db");
    let writer = std::thread::spawn(move || {
        let mut db = rusqlite::Connection::open(db_path).unwrap();
        db.busy_timeout(std::time::Duration::from_secs(5)).unwrap();
        let mut prerequisite = 3;
        while !writer_stop.load(Ordering::Relaxed) {
            let tx = db.transaction().unwrap();
            tx.execute("DELETE FROM task_dependencies WHERE task_id=2", [])
                .unwrap();
            tx.execute("INSERT INTO task_dependencies VALUES (2,?)", [prerequisite])
                .unwrap();
            tx.commit().unwrap();
            prerequisite = if prerequisite == 1 { 3 } else { 1 };
        }
    });
    for _ in 0..25 {
        let report = ok(p, &["graph", "2", "--direction", "upstream"]);
        let prerequisite = report["edges"][0]["prerequisite"].as_i64().unwrap();
        assert_eq!(report["focus"]["ready"], prerequisite == 1);
        assert_eq!(
            node(&report, prerequisite)["blocks_focus"],
            prerequisite == 3
        );
        assert_eq!(
            report["focus"]["unfinished_dependencies"],
            usize::from(prerequisite == 3)
        );
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
}
