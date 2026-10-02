#[allow(dead_code)]
#[path = "../src/snapshot/format.rs"]
mod format;

use format::{FileMeta, ImageMeta, Manifest, validate_image_path};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
    time::Duration,
};
use tempfile::TempDir;

fn run(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qqq"))
        .arg("--json")
        .args(args)
        .current_dir(path)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .output()
        .unwrap()
}

fn ok(path: &Path, args: &[&str]) -> Value {
    let output = run(path, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fail(path: &Path, args: &[&str], expected: &str) {
    let output = run(path, args);
    assert_eq!(output.status.code(), Some(1), "{args:?}");
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

fn digest() -> String {
    "a".repeat(64)
}

fn manifest(paths: &[&str]) -> Manifest {
    Manifest {
        version: 1,
        database: FileMeta {
            bytes: 4096,
            sha256: digest(),
        },
        images: paths
            .iter()
            .map(|path| ImageMeta {
                path: (*path).to_owned(),
                bytes: 8,
                sha256: digest(),
            })
            .collect(),
    }
}

#[test]
fn exact_image_path_grammar() {
    assert_eq!(
        validate_image_path("images/7/3.png").unwrap(),
        (7, 3, "png")
    );
    for path in [
        "/images/7/3.png",
        "../images/7/3.png",
        "images/7/../3.png",
        "images/07/3.png",
        "images/7/03.png",
        "images/0/3.png",
        "images/7/3.svg",
        "images/7/3.png/extra",
        "images\\7\\3.png",
    ] {
        assert!(validate_image_path(path).is_err(), "{path}");
    }
}

#[test]
fn manifest_rejects_duplicate_paths_and_unknown_version() {
    manifest(&["images/7/3.png"]).validate().unwrap();
    assert!(
        manifest(&["images/7/3.png", "images/7/3.png"])
            .validate()
            .is_err()
    );
    let mut unsupported = manifest(&[]);
    unsupported.version = 2;
    assert!(unsupported.validate().is_err());
}

#[test]
fn hash_reader_reports_size_and_sha256() {
    let meta = format::hash_reader("abc".as_bytes()).unwrap();
    assert_eq!(meta.bytes, 3);
    assert_eq!(
        meta.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn backup_contains_database_manifest_and_all_images() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["message", "1", "A note"]);
    ok(path, &["next", "--session", "worker"]);
    ok(path, &["complete", "1", "--session", "worker"]);
    let result = ok(path, &["backup", "snapshot.tar"]);
    assert_eq!(result["tasks"], 1);
    assert_eq!(result["images"], 1);
    assert!(result["bytes"].as_u64().unwrap() > 0);
    let mut names = Vec::new();
    let mut db_bytes = Vec::new();
    let mut manifest = None;
    for entry in tar::Archive::new(fs::File::open(path.join("snapshot.tar")).unwrap())
        .entries()
        .unwrap()
    {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        match name.as_str() {
            "manifest.json" => {
                let mut data = Vec::new();
                entry.read_to_end(&mut data).unwrap();
                manifest = Some(serde_json::from_slice::<Manifest>(&data).unwrap());
            }
            "qqq.db" => {
                entry.read_to_end(&mut db_bytes).unwrap();
            }
            "images/1/1.png" => {
                let mut data = Vec::new();
                entry.read_to_end(&mut data).unwrap();
                assert_eq!(data, b"\x89PNG\r\n\x1a\nfixture");
            }
            _ => panic!("unexpected entry {name}"),
        }
        names.push(name);
    }
    assert_eq!(names, ["manifest.json", "qqq.db", "images/1/1.png"]);
    let manifest = manifest.unwrap();
    manifest.validate().unwrap();
    assert_eq!(manifest.images.len(), 1);
    assert_eq!(manifest.database.bytes, db_bytes.len() as u64);
    fs::write(path.join("snapshot.db"), db_bytes).unwrap();
    let conn = rusqlite::Connection::open(path.join("snapshot.db")).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM tasks", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT body FROM messages", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "A note"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM events", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn backup_refuses_overwrite_and_missing_images() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("snapshot.tar"), b"keep").unwrap();
    fail(path, &["backup", "snapshot.tar"], "exists");
    assert_eq!(fs::read(path.join("snapshot.tar")).unwrap(), b"keep");
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    fs::remove_file(path.join(".qqq/images/1/1.png")).unwrap();
    fail(path, &["backup", "missing.tar"], "Cannot read stored image");
    assert!(!path.join("missing.tar").exists());
}

#[test]
fn backup_waits_for_writer_and_captures_matching_database_and_image() {
    let dir = project();
    let path = dir.path();
    let conn = rusqlite::Connection::open(path.join(".qqq/qqq.db")).unwrap();
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut backup = Command::new(env!("CARGO_BIN_EXE_qqq"))
        .arg("--json")
        .args(["backup", "snapshot.tar"])
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(backup.try_wait().unwrap().is_none());
    conn.execute(
        "INSERT INTO tasks(description) VALUES ('During backup')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO images(task_id,name,media_type,bytes) VALUES (1,'a.png','image/png',15)",
        [],
    )
    .unwrap();
    fs::create_dir_all(path.join(".qqq/images/1")).unwrap();
    fs::write(
        path.join(".qqq/images/1/1.png"),
        b"\x89PNG\r\n\x1a\nfixture",
    )
    .unwrap();
    conn.execute_batch("COMMIT").unwrap();
    let output = backup.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["tasks"], 1);
    assert_eq!(summary["images"], 1);
    let names: Vec<_> = tar::Archive::new(fs::File::open(path.join("snapshot.tar")).unwrap())
        .entries()
        .unwrap()
        .map(|entry| {
            entry
                .unwrap()
                .path()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, ["manifest.json", "qqq.db", "images/1/1.png"]);
}
