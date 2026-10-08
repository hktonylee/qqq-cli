use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

#[derive(Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub sqlite_generation_version: String,
    pub source_provenance: String,
    pub source_provenance_sha256: String,
    pub support: Support,
    pub databases: Vec<DatabaseFixture>,
    pub snapshots: Vec<SnapshotFixture>,
}

#[derive(Deserialize)]
pub struct Support {
    pub current_schema: i64,
    pub normal_open_schemas: Vec<i64>,
    pub snapshot_database_schemas: Vec<i64>,
    pub snapshot_format_versions: Vec<i64>,
    pub released_snapshot_schemas: Vec<i64>,
    pub exclusions: Vec<String>,
    pub provenance_notes: Vec<String>,
}

#[derive(Deserialize)]
pub struct DatabaseFixture {
    pub schema_version: i64,
    pub database: String,
    pub bytes: u64,
    pub sha256: String,
    pub source_release: Option<String>,
    pub base_release: String,
    pub sql_sources: Vec<String>,
    pub source_kind: String,
    pub files: Vec<ImageFile>,
    pub images: Vec<Image>,
    pub expected: String,
    pub expected_sha256: String,
}

#[derive(Deserialize)]
pub struct ImageFile {
    pub path: String,
    pub source: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Deserialize)]
pub struct Image {
    pub id: i64,
    pub task_id: i64,
    pub path: String,
    pub media_type: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Deserialize)]
pub struct SnapshotFixture {
    pub schema_version: i64,
    pub format_version: i64,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub source_release: Option<String>,
    pub emitted_by_tagged_release: bool,
    pub format_source: String,
    pub construction: String,
}

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compatibility")
}

pub fn catalog() -> Catalog {
    serde_json::from_slice(
        &fs::read(root().join("manifest.json"))
            .expect("historical compatibility catalog must exist"),
    )
    .unwrap()
}

pub fn expected(entry: &DatabaseFixture) -> Value {
    let data = fs::read(root().join(&entry.expected)).unwrap();
    assert_eq!(sha256(&data), entry.expected_sha256);
    serde_json::from_slice(&data).unwrap()
}

pub fn sha256(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

pub fn assert_file(path: &Path, bytes: u64, hash: &str) {
    let data = fs::read(path).unwrap();
    assert_eq!(data.len() as u64, bytes, "{}", path.display());
    assert_eq!(sha256(&data), hash, "{}", path.display());
}

pub fn hashes_at(base: &Path) -> BTreeMap<PathBuf, String> {
    fn visit(base: &Path, path: &Path, hashes: &mut BTreeMap<PathBuf, String>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(base, &path, hashes);
            } else {
                hashes.insert(
                    path.strip_prefix(base).unwrap().to_owned(),
                    sha256(&fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut hashes = BTreeMap::new();
    visit(base, base, &mut hashes);
    hashes
}

pub fn tree_hashes() -> BTreeMap<PathBuf, String> {
    hashes_at(&root())
}

pub fn read_only(path: &Path) -> Connection {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}

pub fn copy_database(entry: &DatabaseFixture) -> TempDir {
    let dir = TempDir::new().unwrap();
    let project = dir.path().join(".qqq");
    fs::create_dir(&project).unwrap();
    fs::copy(root().join(&entry.database), project.join("qqq.db")).unwrap();
    for image in &entry.files {
        let target = project.join(&image.path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root().join(&image.source), target).unwrap();
    }
    dir
}

pub fn command(path: &Path) -> Command {
    let binary = std::env::var_os("QQQ_COMPATIBILITY_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_qqq").into());
    let mut command = Command::new(binary);
    command
        .current_dir(path)
        .env("PATH", root().join("no-executables"))
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID");
    command
}

pub fn run(path: &Path, args: &[&str]) -> Value {
    let output = command(path).args(args).arg("--json").output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

pub fn failure(path: &Path, args: &[&str], json: bool, human: &str, code: &str) -> Value {
    let output = command(path)
        .args(args)
        .arg(if json { "--json" } else { "--human" })
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{args:?} expected {human:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "failure must keep stdout empty");
    if json {
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], code, "{error}");
        assert!(error["message"].is_string());
        assert!(error["details"].is_object());
        error
    } else {
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(human),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Value::Null
    }
}

pub fn rows(conn: &Connection, query: &str) -> Value {
    let mut statement = conn.prepare(query).unwrap();
    let columns: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let rows = statement
        .query_map([], |row| {
            let mut object = Map::new();
            for (index, name) in columns.iter().enumerate() {
                let value = match row.get_ref(index)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(value) => json!(value),
                    ValueRef::Text(value) => {
                        let text = std::str::from_utf8(value).unwrap();
                        if name == "tags" || name == "link_json" {
                            serde_json::from_str(text).unwrap()
                        } else {
                            json!(text)
                        }
                    }
                    other => panic!("unexpected canonical SQL value: {other:?}"),
                };
                object.insert(name.clone(), value);
            }
            Ok(Value::Object(object))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    Value::Array(rows)
}

pub fn canonical(conn: &Connection) -> Value {
    json!({
        "schema_version": conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0)).unwrap(),
        "tasks": rows(conn, "SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session,priority,archived,content_revision,tags FROM tasks ORDER BY id"),
        "messages": rows(conn, "SELECT id,task_id,body,session,created_at FROM messages ORDER BY id"),
        "events": rows(conn, "SELECT id,task_id,session,action,created_at FROM events ORDER BY id"),
        "images": rows(conn, "SELECT id,task_id,name,media_type,bytes FROM images ORDER BY id"),
        "links": rows(conn, "SELECT task_id,link_json FROM herdr_links ORDER BY task_id"),
        "dependencies": rows(conn, "SELECT task_id,prerequisite_id FROM task_dependencies ORDER BY task_id,prerequisite_id"),
        "processes": rows(conn, "SELECT task_id,claim_key,claim_event_id,process_json FROM claim_processes ORDER BY task_id"),
        "sequences": rows(conn, "SELECT name,seq FROM sqlite_sequence ORDER BY name"),
    })
}

pub fn assert_migrated(project: &Path, entry: &DatabaseFixture) {
    run(project, &["list", "--all"]);
    let conn = read_only(&project.join(".qqq/qqq.db"));
    let expected = expected(entry);
    assert_eq!(
        canonical(&conn),
        expected["canonical"],
        "schema {}",
        entry.schema_version
    );
    let linked = run(project, &["show", "8"]);
    assert_eq!(linked["task"]["id"], 8);
    assert_eq!(
        linked["herdr"],
        expected["canonical"]["links"][0]["link_json"]
    );
    for image in &entry.images {
        assert_file(
            &project.join(".qqq").join(&image.path),
            image.bytes,
            &image.sha256,
        );
    }
    let report = run(project, &["status"]);
    let mut ready = Vec::new();
    let mut blockers = Map::new();
    let mut owners = Map::new();
    for row in report["tasks"].as_array().unwrap() {
        let id = row["task"]["id"].as_i64().unwrap();
        if row["ready"] == true {
            ready.push(id);
        }
        let ids: Vec<_> = row["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|blocker| blocker["id"].clone())
            .collect();
        if !ids.is_empty() {
            blockers.insert(id.to_string(), json!(ids));
        }
        if !row["owner"].is_null() {
            owners.insert(id.to_string(), row["owner"].clone());
        }
    }
    ready.sort_unstable();
    assert_eq!(json!(ready), expected["queue"]["ready_ids"]);
    assert_eq!(Value::Object(blockers), expected["queue"]["blockers"]);
    assert_eq!(Value::Object(owners), expected["queue"]["owners"]);
    let preview = run(
        project,
        &["next", "--explain", "--session", "fixture-preview"],
    );
    assert_eq!(
        preview["explanation"]["selection"]["task"]["id"],
        expected["queue"]["selection_id"]
    );
    for (session, id) in [("fixture-owner-linked", 8), ("fixture-owner-bare", 55)] {
        assert_eq!(
            run(project, &["next", "--local", "--session", session])["id"],
            id
        );
    }
    for task in expected["canonical"]["tasks"].as_array().unwrap() {
        if task["status"] == "in_progress" {
            let session = task["harness_session"].as_str().unwrap();
            assert_eq!(
                run(project, &["next", "--local", "--harness-session", session])["id"],
                task["id"],
                "public identity must recover migrated claim"
            );
        }
    }
    assert_eq!(
        canonical(&conn),
        expected["canonical"],
        "diagnostics/owner reuse changed fixture state"
    );
}
