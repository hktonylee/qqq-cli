use super::compatibility::*;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{fs, io::Cursor, path::Path};
use tempfile::TempDir;

fn snapshot_bytes(
    entry: &DatabaseFixture,
    database: Vec<u8>,
    version: i64,
) -> Vec<(String, Vec<u8>)> {
    let manifest = json!({
        "version": version,
        "database": {"bytes":database.len(),"sha256":sha256(&database)},
        "images": entry.images.iter().map(|image| json!({"path":image.path,"bytes":image.bytes,"sha256":image.sha256})).collect::<Vec<_>>(),
    });
    let mut entries = vec![
        (
            "manifest.json".to_owned(),
            serde_json::to_vec(&manifest).unwrap(),
        ),
        ("qqq.db".to_owned(), database),
    ];
    for image in &entry.images {
        let data = if entry.schema_version < 6 {
            read_only(&root().join(&entry.database))
                .query_row("SELECT data FROM images WHERE id=?", [image.id], |row| {
                    row.get::<_, Vec<u8>>(0)
                })
                .unwrap()
        } else {
            let file = entry
                .files
                .iter()
                .find(|file| file.path == image.path)
                .unwrap();
            fs::read(root().join(&file.source)).unwrap()
        };
        entries.push((image.path.clone(), data));
    }
    entries
}

fn write_archive(path: &Path, entries: &[(String, Vec<u8>)]) {
    let mut archive = tar::Builder::new(fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive
            .append_data(&mut header, name, Cursor::new(bytes))
            .unwrap();
    }
    archive.finish().unwrap();
}

fn reject_archive(archive: &Path, human: &str, code: &str) {
    let before_archive = fs::read(archive).unwrap();
    for empty in [false, true] {
        for json in [false, true] {
            let target = TempDir::new().unwrap();
            if empty {
                fs::create_dir(target.path().join(".qqq")).unwrap();
            }
            let before = hashes_at(target.path());
            let error = failure(
                target.path(),
                &["restore", archive.to_str().unwrap()],
                json,
                human,
                code,
            );
            if json {
                let reason = match human {
                    "update SQLite manually" => Some("legacy_title_column"),
                    "legacy pending status" => Some("legacy_pending_status"),
                    "invalid task status or claim" => Some("invalid_task_state"),
                    _ => None,
                };
                if let Some(reason) = reason {
                    assert_eq!(error["details"]["reason"], reason);
                }
                assert!(!error.to_string().contains("fixture-secret-owner"));
            }
            assert_eq!(hashes_at(target.path()), before);
            assert_eq!(target.path().join(".qqq").exists(), empty);
            assert_eq!(
                fs::read_dir(target.path()).unwrap().count(),
                usize::from(empty),
                "failed restore must remove staging"
            );
        }
    }
    assert_eq!(fs::read(archive).unwrap(), before_archive);
}

#[test]
fn restored_historical_snapshots_are_idempotent_on_repeated_normal_opens() {
    let originals = tree_hashes();
    let catalog = catalog();
    for snapshot in &catalog.snapshots {
        let entry = catalog
            .databases
            .iter()
            .find(|entry| entry.schema_version == snapshot.schema_version)
            .unwrap();
        let target = TempDir::new().unwrap();
        let archive = root().join(&snapshot.path);
        run(target.path(), &["restore", archive.to_str().unwrap()]);
        assert_migrated(target.path(), entry);
        let before = hashes_at(&target.path().join(".qqq"));
        assert_migrated(target.path(), entry);
        assert_eq!(hashes_at(&target.path().join(".qqq")), before);
    }
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn portable_restore_rejects_db_versions_supported_only_for_normal_open() {
    let originals = tree_hashes();
    let catalog = catalog();
    let output = TempDir::new().unwrap();
    for entry in catalog.databases.iter().filter(|entry| {
        !catalog
            .support
            .snapshot_database_schemas
            .contains(&entry.schema_version)
    }) {
        let archive = output
            .path()
            .join(format!("schema{}.tar", entry.schema_version));
        write_archive(
            &archive,
            &snapshot_bytes(entry, fs::read(root().join(&entry.database)).unwrap(), 1),
        );
        reject_archive(
            &archive,
            "Unsupported snapshot database schema version",
            "COMMAND_ERROR",
        );
    }
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn invalid_historical_snapshots_fail_before_destination_or_images_change() {
    let originals = tree_hashes();
    let catalog = catalog();
    for (case, schema) in [
        ("future", 12),
        ("title", 9),
        ("pending", 9),
        ("invalid-status", 9),
        ("missing-owner", 9),
        ("inactive-owner", 9),
        ("corrupt", 12),
        ("foreign-key", 12),
        ("cycle", 12),
        ("future-format", 12),
        ("image-hash", 9),
        ("image-size", 12),
    ] {
        let entry = catalog
            .databases
            .iter()
            .find(|entry| entry.schema_version == schema)
            .unwrap();
        let source = copy_database(entry);
        let path = source.path().join(".qqq/qqq.db");
        let conn = Connection::open(&path).unwrap();
        let (human, code) = match case {
            "future" => {
                conn.pragma_update(None, "user_version", catalog.support.current_schema + 1)
                    .unwrap();
                (
                    "Unsupported snapshot database schema version",
                    "COMMAND_ERROR",
                )
            }
            "title" => {
                conn.execute_batch("ALTER TABLE tasks RENAME COLUMN description TO title")
                    .unwrap();
                ("update SQLite manually", "DATABASE_ERROR")
            }
            "pending" => {
                conn.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE tasks SET status='pending' WHERE id=13").unwrap();
                ("legacy pending status", "DATABASE_ERROR")
            }
            "invalid-status" | "missing-owner" | "inactive-owner" => {
                let sql = match case {
                    "invalid-status" => "UPDATE tasks SET status='unsupported-state' WHERE id=13",
                    "missing-owner" => "UPDATE tasks SET claim_key=NULL WHERE id=8",
                    "inactive-owner" => {
                        "UPDATE tasks SET claim_key='fixture-secret-owner' WHERE id=3"
                    }
                    _ => unreachable!(),
                };
                conn.pragma_update(None, "ignore_check_constraints", "ON")
                    .unwrap();
                conn.execute_batch(sql).unwrap();
                ("invalid task status or claim", "DATABASE_ERROR")
            }
            "corrupt" => ("file is not a database", "DATABASE_ERROR"),
            "foreign-key" => {
                conn.execute_batch(
                    "PRAGMA foreign_keys=OFF; UPDATE images SET task_id=999 WHERE id=19",
                )
                .unwrap();
                ("invalid foreign keys", "COMMAND_ERROR")
            }
            "cycle" => {
                conn.execute_batch("UPDATE tasks SET parent_id=13 WHERE id=3")
                    .unwrap();
                ("cycle", "INVALID_ARGUMENT")
            }
            "future-format" => ("Unsupported snapshot format version", "COMMAND_ERROR"),
            "image-hash" => ("hash", "COMMAND_ERROR"),
            "image-size" => ("image", "COMMAND_ERROR"),
            _ => unreachable!(),
        };
        drop(conn);
        if case == "pending" {
            let conn = read_only(&path);
            assert_eq!(
                conn.query_row("SELECT status FROM tasks WHERE id=13", [], |row| row
                    .get::<_, String>(0))
                    .unwrap(),
                "pending"
            );
        }
        if case == "corrupt" {
            fs::write(&path, b"corrupt synthetic DB").unwrap();
        }
        let mut entries = snapshot_bytes(
            entry,
            fs::read(path).unwrap(),
            if case == "future-format" { 2 } else { 1 },
        );
        if case == "image-hash" {
            entries[2].1[0] ^= 0xff;
        }
        if case == "image-size" {
            entries[2].1.push(0);
            let mut manifest: Value = serde_json::from_slice(&entries[0].1).unwrap();
            manifest["images"][0]["bytes"] = json!(entries[2].1.len());
            manifest["images"][0]["sha256"] = json!(sha256(&entries[2].1));
            entries[0].1 = serde_json::to_vec(&manifest).unwrap();
        }
        let archive = source.path().join("invalid.tar");
        write_archive(&archive, &entries);
        reject_archive(&archive, human, code);
    }
    assert_eq!(tree_hashes(), originals);
}
