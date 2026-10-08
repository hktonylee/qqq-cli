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
