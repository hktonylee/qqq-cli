use super::compatibility::*;
use rusqlite::{Connection, types::ValueRef};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

fn raw_state(conn: &Connection) -> Value {
    let mut state = BTreeMap::new();
    let mut schema = conn
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY name")
        .unwrap();
    let entries = schema
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    for (kind, name) in entries {
        if kind != "table" {
            continue;
        }
        let mut statement = conn
            .prepare(&format!(
                "SELECT * FROM \"{}\" ORDER BY 1",
                name.replace('"', "\"\"")
            ))
            .unwrap();
        let columns = statement.column_count();
        let data = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|index| {
                        Ok(match row.get_ref(index)? {
                            ValueRef::Null => Value::Null,
                            ValueRef::Integer(value) => json!(value),
                            ValueRef::Real(value) => json!(value),
                            ValueRef::Text(value) => json!(std::str::from_utf8(value).unwrap()),
                            ValueRef::Blob(value) => {
                                json!({"sha256":sha256(value),"bytes":value.len()})
                            }
                        })
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        state.insert(name, json!(data));
    }
    json!({"version":conn.pragma_query_value(None,"user_version",|row|row.get::<_,i64>(0)).unwrap(),
           "schema":rows(conn,"SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY name"),"tables":state})
}

pub(super) fn archives(project: &Path) -> Vec<PathBuf> {
    let directory = project.join(".qqq-upgrades");
    if !directory.exists() {
        return vec![];
    }
    let mut paths: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    assert!(
        paths
            .iter()
            .all(|path| path.extension().is_some_and(|ext| ext == "tar")),
        "partial recovery files: {paths:?}"
    );
    paths
}

pub(super) fn archive_payload(path: &Path) -> (Value, BTreeMap<String, Vec<u8>>) {
    let mut payload = BTreeMap::new();
    for entry in tar::Archive::new(fs::File::open(path).unwrap())
        .entries()
        .unwrap()
    {
        let mut entry = entry.unwrap();
        let name = entry.path().unwrap().to_string_lossy().into_owned();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        assert!(payload.insert(name, data).is_none());
    }
    let manifest: Value =
        serde_json::from_slice(&payload.remove("manifest.json").unwrap()).unwrap();
    assert_eq!(manifest["database"]["sha256"], sha256(&payload["qqq.db"]));
    assert_eq!(manifest["database"]["bytes"], payload["qqq.db"].len());
    for image in manifest["images"].as_array().unwrap() {
        let data = &payload[image["path"].as_str().unwrap()];
        assert_eq!(image["sha256"], sha256(data));
        assert_eq!(image["bytes"], data.len());
    }
    assert_eq!(
        payload.len(),
        1 + manifest["images"].as_array().unwrap().len()
    );
    (manifest, payload)
}

#[test]
fn every_upgrade_saves_original_schema_and_recovers_before_retry() {
    let originals = tree_hashes();
    let catalog = catalog();
    for entry in catalog
        .databases
        .iter()
        .filter(|entry| entry.schema_version < catalog.support.current_schema)
    {
        let source = copy_database(entry);
        let before = raw_state(&read_only(&source.path().join(".qqq/qqq.db")));
        assert_migrated(source.path(), entry);
        let paths = archives(source.path());
        assert_eq!(
            paths.len(),
            1,
            "schema {} requires one pre-upgrade snapshot",
            entry.schema_version
        );
        let (manifest, payload) = archive_payload(&paths[0]);
        assert_eq!(manifest["version"], 2);
        assert_eq!(manifest["upgrade"]["source_schema"], entry.schema_version);
        assert_eq!(
            manifest["upgrade"]["target_schema"],
            catalog.support.current_schema
        );
        assert_eq!(
            manifest["upgrade"]["project"],
            source
                .path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_ref()
        );
        assert!(
            !manifest["upgrade"]["created_at"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        let recovered = TempDir::new().unwrap();
        let result = run(
            recovered.path(),
            &["restore", "--recovery", paths[0].to_str().unwrap()],
        );
        assert_eq!(result["tasks"], 8);
        assert_eq!(result["images"], entry.images.len());
        assert_eq!(
            raw_state(&read_only(&recovered.path().join(".qqq/qqq.db"))),
            before
        );
        assert_eq!(
            fs::read(recovered.path().join(".qqq/qqq.db")).unwrap(),
            payload["qqq.db"]
        );
        assert!(archives(recovered.path()).is_empty());
        for image in &entry.files {
            assert_file(
                &recovered.path().join(".qqq").join(&image.path),
                image.bytes,
                &image.sha256,
            );
        }
        if entry.schema_version < 6 {
            assert!(manifest["images"].as_array().unwrap().is_empty());
        }
        assert_migrated(recovered.path(), entry);
        assert_eq!(archives(recovered.path()).len(), 1);
        assert_eq!(archives(source.path()), paths);
    }
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn initialization_and_current_schema_never_create_upgrade_snapshots() {
    let fresh = TempDir::new().unwrap();
    run(fresh.path(), &["init"]);
    run(fresh.path(), &["init"]);
    run(fresh.path(), &["list"]);
    assert!(!fresh.path().join(".qqq-upgrades").exists());
    let catalog = catalog();
    let entry = catalog.databases.last().unwrap();
    let source = copy_database(entry);
    assert_migrated(source.path(), entry);
    assert!(!source.path().join(".qqq-upgrades").exists());
}

fn assert_original_recovery(archive: &Path, before: &Value) {
    let target = TempDir::new().unwrap();
    run(
        target.path(),
        &["restore", "--recovery", archive.to_str().unwrap()],
    );
    assert_eq!(
        raw_state(&read_only(&target.path().join(".qqq/qqq.db"))),
        *before
    );
}

#[test]
fn concurrent_upgrade_contenders_publish_one_matching_original_snapshot() {
    for entry in catalog()
        .databases
        .iter()
        .filter(|entry| entry.schema_version < catalog().support.current_schema)
    {
        let source = copy_database(entry);
        let lock = Connection::open(source.path().join(".qqq/qqq.db")).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();
        let before = raw_state(&lock);
        let children: Vec<_> = (0..6)
            .map(|_| {
                command(source.path())
                    .args(["list", "--all", "--include-archived", "--json"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect();
        assert!(archives(source.path()).is_empty());
        lock.execute_batch("ROLLBACK").unwrap();
        for child in children {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                output.stderr.is_empty(),
                "JSON result must retain clean stderr"
            );
            let tasks: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(tasks.as_array().unwrap().len(), 8);
        }
        let paths = archives(source.path());
        assert_eq!(paths.len(), 1, "schema {}", entry.schema_version);
        assert_original_recovery(&paths[0], &before);
        assert_migrated(source.path(), entry);
    }
}

#[test]
fn committed_writer_and_wal_data_match_original_snapshot_attachments() {
    let catalog = catalog();
    let entry = catalog
        .databases
        .iter()
        .find(|entry| entry.schema_version == 9)
        .unwrap();
    for wal in [false, true] {
        let source = copy_database(entry);
        let project = source.path().join(".qqq");
        let conn = Connection::open(project.join("qqq.db")).unwrap();
        if wal {
            conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        }
        conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let child = command(source.path())
            .args(["list", "--all", "--human"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        conn.execute(
            "UPDATE tasks SET description=description || ' writer commit' WHERE id=3",
            [],
        )
        .unwrap();
        let image = project.join("images/3/7.png");
        let mut data = fs::read(&image).unwrap();
        data.extend_from_slice(b"synthetic writer attachment");
        fs::write(image, &data).unwrap();
        conn.execute("UPDATE images SET bytes=? WHERE id=7", [data.len() as i64])
            .unwrap();
        let before = raw_state(&conn);
        assert!(archives(source.path()).is_empty());
        conn.execute_batch("COMMIT").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let paths = archives(source.path());
        assert_eq!(paths.len(), 1);
        assert!(String::from_utf8_lossy(&output.stderr).contains(paths[0].to_str().unwrap()));
        let (_, payload) = archive_payload(&paths[0]);
        assert_eq!(payload["images/3/7.png"], data);
        assert_original_recovery(&paths[0], &before);
        assert_eq!(fs::read(project.join("images/3/7.png")).unwrap(), data);
    }
}

#[test]
fn failed_sql_and_image_migrations_retain_original_recovery_records() {
    let catalog = catalog();
    for entry in catalog
        .databases
        .iter()
        .filter(|entry| entry.schema_version < catalog.support.current_schema)
    {
        let source = copy_database(entry);
        let project = source.path().join(".qqq");
        let conn = Connection::open(project.join("qqq.db")).unwrap();
        let (inject, remove, diagnostic) = if entry.schema_version < 11 {
            (
                "CREATE TABLE task_dependencies(sentinel TEXT)",
                "DROP TABLE task_dependencies",
                "already exists",
            )
        } else if entry.schema_version < 12 {
            (
                "ALTER TABLE tasks ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'",
                "ALTER TABLE tasks DROP COLUMN tags",
                "duplicate column",
            )
        } else {
            (
                "CREATE TABLE claim_processes(sentinel TEXT)",
                "DROP TABLE claim_processes",
                "already exists",
            )
        };
        conn.execute_batch(inject).unwrap();
        let state = raw_state(&conn);
        drop(conn);
        let original = hashes_at(&project);
        for json in [false, true] {
            failure(source.path(), &["list"], json, diagnostic, "DATABASE_ERROR");
            assert_eq!(hashes_at(&project), original);
        }
        let paths = archives(source.path());
        assert_eq!(paths.len(), 2, "both retries retain distinct records");
        let saved: Vec<_> = paths.iter().map(|path| fs::read(path).unwrap()).collect();
        for path in &paths {
            assert_original_recovery(path, &state);
        }
        Connection::open(project.join("qqq.db"))
            .unwrap()
            .execute_batch(remove)
            .unwrap();
        assert_migrated(source.path(), entry);
        assert_eq!(archives(source.path()).len(), 3);
        for (path, bytes) in paths.iter().zip(saved) {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
    let entry = &catalog.databases[0];
    let source = copy_database(entry);
    let project = source.path().join(".qqq");
    let conflict = project.join("images/13/19.gif");
    fs::create_dir_all(conflict.parent().unwrap()).unwrap();
    fs::write(&conflict, b"synthetic conflict").unwrap();
    let state = raw_state(&read_only(&project.join("qqq.db")));
    let original = hashes_at(&project);
    failure(
        source.path(),
        &["list"],
        true,
        "Stored image path conflicts",
        "COMMAND_ERROR",
    );
    assert_eq!(hashes_at(&project), original);
    let paths = archives(source.path());
    assert_eq!(paths.len(), 1);
    assert_original_recovery(&paths[0], &state);
    fs::remove_file(conflict).unwrap();
    assert_migrated(source.path(), entry);
}

#[test]
fn snapshot_directory_and_attachment_failures_abort_before_source_changes() {
    let catalog = catalog();
    let entry = catalog
        .databases
        .iter()
        .find(|entry| entry.schema_version == 9)
        .unwrap();
    for case in [
        "directory-file",
        "missing-image",
        "wrong-size",
        "directory-symlink",
    ] {
        let source = copy_database(entry);
        let project = source.path().join(".qqq");
        let directory = source.path().join(".qqq-upgrades");
        let outside = TempDir::new().unwrap();
        let image = project.join("images/3/7.png");
        let diagnostic = match case {
            "directory-file" => {
                fs::write(&directory, b"keep").unwrap();
                "real directory"
            }
            "missing-image" => {
                fs::remove_file(image).unwrap();
                "Cannot read stored image"
            }
            "wrong-size" => {
                fs::write(image, b"short").unwrap();
                "byte count differs"
            }
            "directory-symlink" => {
                #[cfg(unix)]
                std::os::unix::fs::symlink(outside.path(), &directory).unwrap();
                "real directory"
            }
            _ => unreachable!(),
        };
        let original = hashes_at(&project);
        for json in [false, true] {
            failure(source.path(), &["list"], json, diagnostic, "COMMAND_ERROR");
            assert_eq!(hashes_at(&project), original);
            assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
        }
        if directory.is_file()
            || fs::symlink_metadata(&directory)
                .unwrap()
                .file_type()
                .is_symlink()
        {
            fs::remove_file(&directory).unwrap();
        } else {
            assert!(archives(source.path()).is_empty());
        }
        fs::copy(
            root().join(
                &entry
                    .files
                    .iter()
                    .find(|image| image.path == "images/3/7.png")
                    .unwrap()
                    .source,
            ),
            project.join("images/3/7.png"),
        )
        .unwrap();
        assert_migrated(source.path(), entry);
    }
}

#[test]
fn busy_migration_commit_rolls_back_but_retains_verified_snapshot() {
    let catalog = catalog();
    let entry = catalog
        .databases
        .iter()
        .find(|entry| entry.schema_version == 9)
        .unwrap();
    let source = copy_database(entry);
    let project = source.path().join(".qqq");
    // Closing any other descriptor for this DB can release this process's
    // POSIX SQLite locks. Hash before acquiring the held reader transaction.
    let original = hashes_at(&project);
    let reader = read_only(&project.join("qqq.db"));
    reader.execute_batch("BEGIN").unwrap();
    reader
        .query_row("SELECT count(*) FROM tasks", [], |row| row.get::<_, i64>(0))
        .unwrap();
    let state = raw_state(&reader);
    failure(
        source.path(),
        &["list"],
        true,
        "Database is busy",
        "DB_BUSY",
    );
    assert_eq!(hashes_at(&project), original);
    let paths = archives(source.path());
    assert_eq!(paths.len(), 1);
    assert_original_recovery(&paths[0], &state);
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    assert_migrated(source.path(), entry);
    assert_eq!(archives(source.path()).len(), 2);
}

#[test]
fn pending_deletion_attachments_are_captured_without_pre_upgrade_recovery() {
    let catalog = catalog();
    let entry = catalog
        .databases
        .iter()
        .find(|entry| entry.schema_version == 9)
        .unwrap();
    let source = copy_database(entry);
    let project = source.path().join(".qqq");
    let wrapper = project.join(".delete-staging/3-test");
    fs::create_dir_all(&wrapper).unwrap();
    fs::write(wrapper.join("task-id"), b"3").unwrap();
    fs::rename(project.join("images/3"), wrapper.join("images")).unwrap();
    let conn = Connection::open(project.join("qqq.db")).unwrap();
    conn.execute_batch("CREATE TABLE task_dependencies(sentinel TEXT)")
        .unwrap();
    let state = raw_state(&conn);
    drop(conn);
    let original = hashes_at(&project);
    failure(
        source.path(),
        &["list"],
        true,
        "already exists",
        "DATABASE_ERROR",
    );
    assert_eq!(
        hashes_at(&project),
        original,
        "capture must leave live staging unchanged"
    );
    let paths = archives(source.path());
    assert_eq!(paths.len(), 1);
    let recovered = TempDir::new().unwrap();
    run(
        recovered.path(),
        &["restore", "--recovery", paths[0].to_str().unwrap()],
    );
    assert_eq!(
        raw_state(&read_only(&recovered.path().join(".qqq/qqq.db"))),
        state
    );
    for image in &entry.files {
        assert_file(
            &recovered.path().join(".qqq").join(&image.path),
            image.bytes,
            &image.sha256,
        );
    }
    assert!(!recovered.path().join(".qqq/.delete-staging").exists());
    Connection::open(project.join("qqq.db"))
        .unwrap()
        .execute_batch("DROP TABLE task_dependencies")
        .unwrap();
    assert_migrated(source.path(), entry);
    assert!(!project.join(".delete-staging").exists());
}

#[test]
fn snapshot_verification_failure_leaves_original_embedded_images_intact() {
    let catalog = catalog();
    let entry = &catalog.databases[0];
    let source = copy_database(entry);
    let project = source.path().join(".qqq");
    Connection::open(project.join("qqq.db"))
        .unwrap()
        .execute(
            "UPDATE images SET media_type='invalid/synthetic' WHERE id=19",
            [],
        )
        .unwrap();
    let original = hashes_at(&project);
    for json in [false, true] {
        failure(
            source.path(),
            &["list"],
            json,
            "Upgrade snapshot verification failed",
            "COMMAND_ERROR",
        );
        assert_eq!(hashes_at(&project), original);
        assert!(archives(source.path()).is_empty());
        assert!(!project.join("images").exists());
    }
    Connection::open(project.join("qqq.db"))
        .unwrap()
        .execute("UPDATE images SET media_type='image/gif' WHERE id=19", [])
        .unwrap();
    assert_migrated(source.path(), entry);
}

#[test]
fn read_only_preview_waits_for_candidate_without_claiming_or_recovery() {
    let project = TempDir::new().unwrap();
    run(project.path(), &["init"]);
    let staging = project.path().join(".qqq/.delete-staging");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("marker"), b"keep pending stage").unwrap();
    let mut child = command(project.path())
        .args(["next", "--dry-run", "--wait", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(300));
    assert!(
        child.try_wait().unwrap().is_none(),
        "preview must wait until candidate exists"
    );
    let conn = Connection::open(project.path().join(".qqq/qqq.db")).unwrap();
    conn.execute(
        "INSERT INTO tasks(description) VALUES('Synthetic waiting preview')",
        [],
    )
    .unwrap();
    drop(conn);
    let before = hashes_at(project.path());
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("preview did not return committed candidate");
        }
        thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview["id"], 1);
    assert_eq!(preview["status"], "new");
    assert!(preview["harness_session"].is_null());
    assert_eq!(hashes_at(project.path()), before);
    assert!(archives(project.path()).is_empty());
}

#[test]
fn read_only_wait_rejects_later_wal_transition_without_creating_sidecars() {
    let project = TempDir::new().unwrap();
    run(project.path(), &["init"]);
    let database = project.path().join(".qqq/qqq.db");
    let mut child = command(project.path())
        .args(["next", "--dry-run", "--wait", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    thread::sleep(Duration::from_millis(300));
    assert!(child.try_wait().unwrap().is_none());
    let writer = Connection::open(&database).unwrap();
    writer.busy_timeout(Duration::from_secs(5)).unwrap();
    writer.pragma_update(None, "journal_mode", "WAL").unwrap();
    drop(writer);
    let before = hashes_at(project.path());
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("read-only wait did not reject WAL transition");
        }
        thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "DATABASE_ERROR");
    assert_eq!(error["details"]["reason"], "unsafe_read_only");
    assert_eq!(hashes_at(project.path()), before);
}
