use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
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

fn human(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qqq"))
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
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn report(path: &Path) -> (i32, Value) {
    let output = run(path, &["doctor"]);
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        output.status.code().unwrap(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    ok(dir.path(), &["init"]);
    dir
}

#[test]
fn healthy_doctor_reports_counts_without_changing_project() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    let db_path = path.join(".qqq/qqq.db");
    let before_bytes = fs::read(&db_path).unwrap();
    let before_mtime = fs::metadata(&db_path).unwrap().modified().unwrap();
    let mut before_entries: Vec<_> = fs::read_dir(path.join(".qqq"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    before_entries.sort();
    for _ in 0..2 {
        let (status, result) = report(path);
        assert_eq!(status, 0);
        assert_eq!(result["ok"], true);
        assert_eq!(result["schema_version"], 12);
        assert_eq!(result["tasks"], 1);
        assert_eq!(result["images"], 1);
        assert_eq!(result["issues"], serde_json::json!([]));
    }
    assert_eq!(fs::read(&db_path).unwrap(), before_bytes);
    assert_eq!(
        fs::metadata(&db_path).unwrap().modified().unwrap(),
        before_mtime
    );
    let mut after_entries: Vec<_> = fs::read_dir(path.join(".qqq"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    after_entries.sort();
    assert_eq!(after_entries, before_entries);
}

#[test]
fn missing_and_corrupt_database_produce_json_and_nonzero_exit_without_writes() {
    let dir = TempDir::new().unwrap();
    let (status, result) = report(dir.path());
    assert_eq!(status, 1);
    assert_eq!(result["ok"], false);
    assert_eq!(result["issues"][0]["code"], "DB_MISSING");
    assert!(
        result["issues"][0]["action"]
            .as_str()
            .unwrap()
            .contains("restore")
    );
    assert!(!dir.path().join(".qqq").exists());

    fs::create_dir(dir.path().join(".qqq")).unwrap();
    let db_path = dir.path().join(".qqq/qqq.db");
    fs::write(&db_path, b"not sqlite").unwrap();
    let before = fs::read(&db_path).unwrap();
    let (status, result) = report(dir.path());
    assert_eq!(status, 1);
    assert_eq!(result["ok"], false);
    assert!(
        result["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"].as_str().unwrap().starts_with("DB_"))
    );
    assert_eq!(fs::read(&db_path).unwrap(), before);
}

#[test]
fn doctor_finds_missing_changed_and_orphan_images() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    let stored = path.join(".qqq/images/1/1.png");
    fs::remove_file(&stored).unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert_eq!(result["issues"][0]["code"], "IMAGE_MISSING");
    fs::write(&stored, b"short").unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert_eq!(result["issues"][0]["code"], "IMAGE_SIZE");
    fs::write(&stored, b"abcdefghijklmno").unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert_eq!(result["issues"][0]["code"], "IMAGE_SIGNATURE");
    fs::write(&stored, b"\x89PNG\r\n\x1a\nfixture").unwrap();
    let orphan = path.join(".qqq/images/1/999.png");
    fs::write(&orphan, b"orphan").unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert_eq!(result["issues"][0]["code"], "IMAGE_ORPHAN");
    assert_eq!(fs::read(&orphan).unwrap(), b"orphan");
}

#[test]
fn doctor_reports_empty_unreferenced_image_directory() {
    let dir = project();
    let orphan = dir.path().join(".qqq/images/999");
    fs::create_dir_all(&orphan).unwrap();
    let orphan = fs::canonicalize(orphan).unwrap();
    let (status, result) = report(dir.path());
    assert_eq!(status, 1);
    assert_eq!(result["ok"], false);
    assert!(
        result["issues"].as_array().unwrap().iter().any(|issue| {
            issue["code"] == "IMAGE_ORPHAN" && issue["path"] == orphan.display().to_string()
        }),
        "{result}"
    );
}

#[test]
fn doctor_reports_damaged_image_metadata_and_unsafe_path() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    let db_path = path.join(".qqq/qqq.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE images SET media_type='image/svg+xml' WHERE id=1",
        [],
    )
    .unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert!(
        result["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "IMAGE_METADATA")
    );
    conn.execute("UPDATE images SET media_type='image/png' WHERE id=1", [])
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let stored = path.join(".qqq/images/1/1.png");
        fs::remove_file(&stored).unwrap();
        symlink(path.join("a.png"), &stored).unwrap();
        let (status, result) = report(path);
        assert_eq!(status, 1);
        assert!(
            result["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["code"] == "IMAGE_UNSAFE")
        );
    }
}

#[test]
fn doctor_reports_foreign_key_schema_and_sidecar_damage() {
    let dir = project();
    let path = dir.path();
    let db_path = path.join(".qqq/qqq.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    conn.execute(
        "INSERT INTO messages(task_id,body) VALUES (999,'orphan')",
        [],
    )
    .unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert!(
        result["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "DB_FOREIGN_KEY")
    );
    conn.execute("DELETE FROM messages WHERE task_id=999", [])
        .unwrap();
    conn.pragma_update(None, "user_version", 109).unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert!(
        result["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "DB_SCHEMA")
    );
    conn.pragma_update(None, "user_version", 12).unwrap();
    let sidecar = path.join(".qqq/qqq.db-wal");
    fs::write(&sidecar, b"simulated").unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert_eq!(result["issues"][0]["code"], "DB_SIDECAR");
    assert_eq!(fs::read(&sidecar).unwrap(), b"simulated");
}

#[test]
fn doctor_discovers_nested_project_and_prints_actionable_human_diagnostics() {
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    let nested = path.join("nested");
    fs::create_dir(&nested).unwrap();
    let (status, result) = report(&nested);
    assert_eq!(status, 0);
    assert_eq!(result["ok"], true);
    let output = human(&nested, &["doctor"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Project healthy."));
    fs::remove_file(path.join(".qqq/images/1/1.png")).unwrap();
    let output = human(&nested, &["doctor"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("IMAGE_MISSING"), "{text}");
    assert!(text.contains("Action:"), "{text}");
    assert!(text.contains(".qqq/images/1/1.png"), "{text}");
}

#[cfg(unix)]
#[test]
fn doctor_never_follows_symlinked_image_root() {
    use std::os::unix::fs::symlink;
    let dir = project();
    let path = dir.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    let outside = TempDir::new().unwrap();
    fs::rename(path.join(".qqq/images"), path.join("stored-images")).unwrap();
    symlink(outside.path(), path.join(".qqq/images")).unwrap();
    let (status, result) = report(path);
    assert_eq!(status, 1);
    assert!(
        result["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "IMAGE_UNSAFE")
    );
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
