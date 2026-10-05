#[allow(dead_code)]
#[path = "common/compatibility.rs"]
mod compatibility;

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
    run_mode(path, args, true)
}
fn run_mode(path: &Path, args: &[&str], json: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_qqq"));
    if json {
        command.arg("--json");
    }
    command
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
    // Keep readable human diagnostics covered alongside JSON contract tests.
    let output = run_mode(path, args, false);
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
    assert_eq!(manifest.images[0].path, "images/1/1.png");
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
    fs::write(path.join(".qqq/images/1/1.png"), b"short").unwrap();
    fail(path, &["backup", "wrong-size.tar"], "byte count differs");
    assert!(!path.join("wrong-size.tar").exists());
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
    let restored = TempDir::new().unwrap();
    ok(
        restored.path(),
        &["restore", path.join("snapshot.tar").to_str().unwrap()],
    );
    assert_eq!(
        ok(restored.path(), &["show", "1"])["task"]["description"],
        "During backup"
    );
    assert_eq!(
        fs::read(restored.path().join(".qqq/images/1/1.png")).unwrap(),
        b"\x89PNG\r\n\x1a\nfixture"
    );
}

#[test]
fn restore_released_schema_nine_backup_then_migrate_preserves_content_and_images() {
    let catalog = compatibility::catalog();
    let snapshot = catalog
        .snapshots
        .iter()
        .find(|entry| entry.schema_version == 9)
        .unwrap();
    let entry = catalog
        .databases
        .iter()
        .find(|entry| entry.schema_version == 9)
        .unwrap();
    let before = compatibility::tree_hashes();
    let target = TempDir::new().unwrap();
    let archive = compatibility::root().join(&snapshot.path);
    ok(target.path(), &["restore", archive.to_str().unwrap()]);
    let conn = compatibility::read_only(&target.path().join(".qqq/qqq.db"));
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        9
    );
    drop(conn);
    compatibility::assert_migrated(target.path(), entry);
    let restored = ok(target.path(), &["show", "13"]);
    assert_eq!(
        restored["task"]["description"],
        compatibility::expected(entry)["canonical"]["tasks"][2]["description"]
    );
    assert_eq!(restored["task"]["content_revision"], 1);
    assert_eq!(restored["messages"][0]["body"], "Queued note\nUnicode 界");
    assert_eq!(compatibility::tree_hashes(), before);
}

#[test]
fn restore_round_trip_into_new_and_empty_project_locations() {
    let source = project();
    let path = source.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["message", "1", "A note"]);
    ok(path, &["next", "--session", "worker"]);
    ok(path, &["complete", "1", "--session", "worker"]);
    ok(path, &["backup", "snapshot.tar"]);
    let archive = path.join("snapshot.tar");
    for existing_empty in [false, true] {
        let target = TempDir::new().unwrap();
        if existing_empty {
            fs::create_dir(target.path().join(".qqq")).unwrap();
        }
        let result = ok(target.path(), &["restore", archive.to_str().unwrap()]);
        assert_eq!(result["tasks"], 1);
        assert_eq!(result["images"], 1);
        assert_eq!(
            ok(target.path(), &["show", "1"])["messages"][0]["body"],
            "A note"
        );
        assert_eq!(
            ok(target.path(), &["show", "1"])["events"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            fs::read(target.path().join(".qqq/images/1/1.png")).unwrap(),
            b"\x89PNG\r\n\x1a\nfixture"
        );
        ok(target.path(), &["add", "After restore"]);
        assert_eq!(ok(target.path(), &["list"])[1]["id"], 2);
    }
}

#[test]
fn restore_refuses_existing_data_and_ancestor_project_without_mutation() {
    let source = project();
    ok(source.path(), &["add", "Original"]);
    ok(source.path(), &["backup", "snapshot.tar"]);
    let archive = source.path().join("snapshot.tar");
    let target = project();
    ok(target.path(), &["add", "Keep target"]);
    let before = ok(target.path(), &["show", "1"]);
    fail(
        target.path(),
        &["restore", archive.to_str().unwrap()],
        "not empty",
    );
    assert_eq!(ok(target.path(), &["show", "1"]), before);
    let nested = source.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fail(
        &nested,
        &["restore", archive.to_str().unwrap()],
        "ancestor project",
    );
    assert!(!nested.join(".qqq").exists());
}

#[test]
fn restore_rejects_missing_and_changed_images_without_creating_project() {
    let source = project();
    let path = source.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["backup", "snapshot.tar"]);
    let archive = path.join("snapshot.tar");
    for (name, image_content) in [
        ("missing", None),
        ("changed", Some(b"different".as_slice())),
        ("same-size-change", Some(b"abcdefghijklmno".as_slice())),
    ] {
        let tampered = path.join(format!("{name}.tar"));
        rewrite_image_archive(&archive, &tampered, image_content);
        let target = TempDir::new().unwrap();
        fail(
            target.path(),
            &["restore", tampered.to_str().unwrap()],
            "image",
        );
        assert!(!target.path().join(".qqq").exists());
    }
}

#[test]
fn restore_rejects_corrupt_manifest_database_extra_entry_and_duplicate() {
    let source = project();
    let path = source.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["backup", "snapshot.tar"]);
    let original = read_entries(&path.join("snapshot.tar"));
    for (name, change, expected) in [
        ("bad-manifest", 0, "Invalid snapshot manifest"),
        ("bad-db", 1, "hash"),
    ] {
        let mut entries = original.clone();
        if change == 0 {
            entries[0].1 = b"{broken".to_vec();
        } else {
            entries[1].1[100] ^= 0xFF;
        }
        let archive = path.join(format!("{name}.tar"));
        write_entries(&archive, &entries);
        let target = TempDir::new().unwrap();
        fail(
            target.path(),
            &["restore", archive.to_str().unwrap()],
            expected,
        );
        assert!(!target.path().join(".qqq").exists());
    }
    for (name, extra, expected) in [
        (
            "extra",
            ("outside.txt".to_owned(), b"x".to_vec()),
            "Unsafe archive path",
        ),
        ("duplicate", original[2].clone(), "duplicate"),
    ] {
        let mut entries = original.clone();
        entries.push(extra);
        let archive = path.join(format!("{name}.tar"));
        write_entries(&archive, &entries);
        let target = TempDir::new().unwrap();
        fail(
            target.path(),
            &["restore", archive.to_str().unwrap()],
            expected,
        );
        assert!(!target.path().join(".qqq").exists());
    }
}

#[test]
fn restore_rejects_link_and_traversal_tar_entries() {
    let source = project();
    let path = source.path();
    ok(path, &["backup", "snapshot.tar"]);
    let base = read_entries(&path.join("snapshot.tar"));
    for (name, link) in [("traversal", false), ("link", true)] {
        let archive = path.join(format!("{name}.tar"));
        let mut output = tar::Builder::new(fs::File::create(&archive).unwrap());
        append_entries(&mut output, &base);
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o600);
        if link {
            header.set_path("images/1/1.png").unwrap();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name("../../outside").unwrap();
        } else {
            header.as_mut_bytes()[..10].copy_from_slice(b"../outside");
        }
        header.set_cksum();
        output.append(&header, std::io::empty()).unwrap();
        output.finish().unwrap();
        let target = TempDir::new().unwrap();
        fail(
            target.path(),
            &["restore", archive.to_str().unwrap()],
            if link {
                "regular file"
            } else {
                "Unsafe archive path"
            },
        );
        assert!(!target.path().join(".qqq").exists());
        assert!(!target.path().join("outside").exists());
    }
}

#[test]
fn restore_rejects_database_integrity_and_image_count_mismatch() {
    let source = project();
    let path = source.path();
    fs::write(path.join("a.png"), b"\x89PNG\r\n\x1a\nfixture").unwrap();
    ok(path, &["add", "Keep", "--image", "a.png"]);
    ok(path, &["backup", "snapshot.tar"]);
    let original = read_entries(&path.join("snapshot.tar"));
    let mut bad_db = original.clone();
    bad_db[1].1[0] = b'X';
    let mut manifest: Manifest = serde_json::from_slice(&bad_db[0].1).unwrap();
    manifest.database = format::hash_reader(bad_db[1].1.as_slice()).unwrap();
    bad_db[0].1 = serde_json::to_vec(&manifest).unwrap();
    let bad_db_archive = path.join("bad-integrity.tar");
    write_entries(&bad_db_archive, &bad_db);
    let target = TempDir::new().unwrap();
    fail(
        target.path(),
        &["restore", bad_db_archive.to_str().unwrap()],
        "database",
    );
    assert!(!target.path().join(".qqq").exists());

    let mut missing_manifest_image = original;
    let mut manifest: Manifest = serde_json::from_slice(&missing_manifest_image[0].1).unwrap();
    manifest.images.clear();
    missing_manifest_image[0].1 = serde_json::to_vec(&manifest).unwrap();
    missing_manifest_image.pop();
    let archive = path.join("bad-image-count.tar");
    write_entries(&archive, &missing_manifest_image);
    let target = TempDir::new().unwrap();
    fail(
        target.path(),
        &["restore", archive.to_str().unwrap()],
        "missing image",
    );
    assert!(!target.path().join(".qqq").exists());
}

#[test]
fn failed_restore_preserves_existing_empty_project_and_rejects_source_symlink() {
    let source = project();
    ok(source.path(), &["backup", "snapshot.tar"]);
    let archive = source.path().join("snapshot.tar");
    let target = TempDir::new().unwrap();
    fs::create_dir(target.path().join(".qqq")).unwrap();
    let mut truncated = fs::read(&archive).unwrap();
    truncated.truncate(700);
    let bad = source.path().join("truncated.tar");
    fs::write(&bad, truncated).unwrap();
    let output = run(target.path(), &["restore", bad.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(target.path().join(".qqq").is_dir());
    assert_eq!(fs::read_dir(target.path().join(".qqq")).unwrap().count(), 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = source.path().join("link.tar");
        symlink(&archive, &link).unwrap();
        fail(
            target.path(),
            &["restore", link.to_str().unwrap()],
            "regular file",
        );
        assert_eq!(fs::read_dir(target.path().join(".qqq")).unwrap().count(), 0);
    }
}

fn read_entries(source: &Path) -> Vec<(String, Vec<u8>)> {
    tar::Archive::new(fs::File::open(source).unwrap())
        .entries()
        .unwrap()
        .map(|entry| {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_string_lossy().into_owned();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            (name, data)
        })
        .collect()
}

fn append_entries(output: &mut tar::Builder<fs::File>, entries: &[(String, Vec<u8>)]) {
    for (name, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        output
            .append_data(&mut header, name, data.as_slice())
            .unwrap();
    }
}

fn write_entries(destination: &Path, entries: &[(String, Vec<u8>)]) {
    let mut output = tar::Builder::new(fs::File::create(destination).unwrap());
    append_entries(&mut output, entries);
    output.finish().unwrap();
}

fn rewrite_image_archive(source: &Path, destination: &Path, replacement: Option<&[u8]>) {
    let mut entries = read_entries(source);
    if let Some(replacement) = replacement {
        entries
            .iter_mut()
            .find(|(name, _)| name == "images/1/1.png")
            .unwrap()
            .1 = replacement.to_vec();
    } else {
        entries.retain(|(name, _)| name != "images/1/1.png");
    }
    write_entries(destination, &entries);
}
