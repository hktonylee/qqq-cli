use super::{
    compatibility::*,
    recovery::{archive_payload, archives},
};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, io::Cursor, path::Path};
use tempfile::TempDir;

fn write_archive(path: &Path, manifest: &Value, payload: &BTreeMap<String, Vec<u8>>) {
    let mut archive = tar::Builder::new(fs::File::create(path).unwrap());
    let data = serde_json::to_vec(manifest).unwrap();
    for (name, bytes) in std::iter::once(("manifest.json", &data))
        .chain(payload.iter().map(|(name, bytes)| (name.as_str(), bytes)))
    {
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

#[test]
fn recovery_rejects_invalid_metadata_checksums_schema_and_claims_before_install() {
    let catalog = catalog();
    let entry = catalog
        .databases
        .iter()
        .find(|entry| entry.schema_version == 9)
        .unwrap();
    let source = copy_database(entry);
    assert_migrated(source.path(), entry);
    let paths = archives(source.path());
    let (original_manifest, original_payload) = archive_payload(&paths[0]);
    let scratch = TempDir::new().unwrap();
    for case in [
        "wrong-source",
        "future-target",
        "invalid-project",
        "empty-time",
        "missing-upgrade",
        "future-format",
        "wrong-hash",
        "wrong-image",
        "future-db",
        "zero-db",
        "legacy-title",
        "pending",
        "invalid-status",
        "missing-owner",
        "empty-owner",
        "whitespace-owner",
        "inactive-owner",
        "corrupt-db",
    ] {
        let mut manifest = original_manifest.clone();
        let mut payload = original_payload.clone();
        let (human, code) = match case {
            "wrong-source" => {
                manifest["upgrade"]["source_schema"] = json!(8);
                ("schema metadata differs", "COMMAND_ERROR")
            }
            "future-target" => {
                manifest["upgrade"]["target_schema"] = json!(catalog.support.current_schema + 1);
                ("schema metadata differs", "COMMAND_ERROR")
            }
            "invalid-project" => {
                manifest["upgrade"]["project"] = json!("relative");
                ("project metadata", "COMMAND_ERROR")
            }
            "empty-time" => {
                manifest["upgrade"]["created_at"] = json!("");
                ("project metadata", "COMMAND_ERROR")
            }
            "missing-upgrade" => {
                manifest.as_object_mut().unwrap().remove("upgrade");
                ("Unsupported snapshot format", "COMMAND_ERROR")
            }
            "future-format" => {
                manifest["version"] = json!(3);
                ("Unsupported snapshot format", "COMMAND_ERROR")
            }
            "wrong-hash" => {
                manifest["database"]["sha256"] = json!("a".repeat(64));
                ("hash differs", "COMMAND_ERROR")
            }
            "wrong-image" => {
                payload.get_mut("images/3/7.png").unwrap()[0] = 0;
                ("hash differs", "COMMAND_ERROR")
            }
            other => {
                let database = scratch.path().join("changed.db");
                fs::write(&database, &payload["qqq.db"]).unwrap();
                let (sql, human, code)=match other {
                    "future-db" => (format!("PRAGMA user_version={}",catalog.support.current_schema+1),"Unsupported snapshot database","COMMAND_ERROR"),
                    "zero-db" => ("PRAGMA user_version=0".into(),"Unsupported snapshot database","COMMAND_ERROR"),
                    "legacy-title" => ("ALTER TABLE tasks RENAME COLUMN description TO title".into(),"update SQLite manually","DATABASE_ERROR"),
                    "pending" => ("PRAGMA ignore_check_constraints=ON; UPDATE tasks SET status='pending' WHERE id=13".into(),"update SQLite manually","DATABASE_ERROR"),
                    "invalid-status" => ("PRAGMA ignore_check_constraints=ON; UPDATE tasks SET status='invalid' WHERE id=13".into(),"invalid task status or claim","DATABASE_ERROR"),
                    "missing-owner" => ("PRAGMA ignore_check_constraints=ON; UPDATE tasks SET claim_key=NULL WHERE id=8".into(),"invalid task status or claim","DATABASE_ERROR"),
                    "empty-owner" => ("UPDATE tasks SET claim_key='' WHERE id=8".into(),"invalid task status or claim","DATABASE_ERROR"),
                    "whitespace-owner" => ("UPDATE tasks SET claim_key=char(9,10,32,160) WHERE id=8".into(),"invalid task status or claim","DATABASE_ERROR"),
                    "inactive-owner" => ("PRAGMA ignore_check_constraints=ON; UPDATE tasks SET claim_key='fixture-secret-owner' WHERE id=3".into(),"invalid task status or claim","DATABASE_ERROR"),
                    "corrupt-db" => ("".into(),"file is not a database","DATABASE_ERROR"),
                    _=>unreachable!(),
                };
                if other == "corrupt-db" {
                    fs::write(&database, b"synthetic corrupt database").unwrap();
                } else {
                    Connection::open(&database)
                        .unwrap()
                        .execute_batch(&sql)
                        .unwrap();
                }
                let data = fs::read(database).unwrap();
                manifest["database"] = json!({"bytes":data.len(),"sha256":sha256(&data)});
                payload.insert("qqq.db".into(), data);
                (human, code)
            }
        };
        let archive = scratch.path().join(format!("{case}.tar"));
        write_archive(&archive, &manifest, &payload);
        let archive_bytes = fs::read(&archive).unwrap();
        for empty in [false, true] {
            for json in [false, true] {
                let target = TempDir::new().unwrap();
                if empty {
                    fs::create_dir(target.path().join(".qqq")).unwrap();
                }
                let before = hashes_at(target.path());
                let error = failure(
                    target.path(),
                    &["restore", "--recovery", archive.to_str().unwrap()],
                    json,
                    human,
                    code,
                );
                assert!(!error.to_string().contains("fixture-secret-owner"));
                assert_eq!(hashes_at(target.path()), before, "{case} changed target");
                assert_eq!(target.path().join(".qqq").exists(), empty);
                assert_eq!(
                    fs::read_dir(target.path()).unwrap().count(),
                    usize::from(empty)
                );
            }
        }
        assert_eq!(fs::read(archive).unwrap(), archive_bytes);
    }
}

#[test]
fn portable_and_recovery_modes_are_distinct_and_populated_targets_are_preserved() {
    let catalog = catalog();
    let entry = &catalog.databases[0];
    let source = copy_database(entry);
    assert_migrated(source.path(), entry);
    let paths = archives(source.path());
    let archive = paths[0].to_str().unwrap();
    let fresh = TempDir::new().unwrap();
    for json in [false, true] {
        failure(
            fresh.path(),
            &["restore", archive],
            json,
            "Snapshot kind does not match restore mode",
            "COMMAND_ERROR",
        );
        assert!(fs::read_dir(fresh.path()).unwrap().next().is_none());
        failure(
            fresh.path(),
            &[
                "restore",
                "--recovery",
                root().join(&catalog.snapshots[0].path).to_str().unwrap(),
            ],
            json,
            "Snapshot kind does not match restore mode",
            "COMMAND_ERROR",
        );
        assert!(fs::read_dir(fresh.path()).unwrap().next().is_none());
    }
    let populated = copy_database(entry);
    let before = hashes_at(populated.path());
    for json in [false, true] {
        failure(
            populated.path(),
            &["restore", "--recovery", archive],
            json,
            "Restore target .qqq is not empty",
            "COMMAND_ERROR",
        );
        assert_eq!(hashes_at(populated.path()), before);
    }
}

#[test]
fn upgrade_rejects_blank_active_owner_in_every_historical_owner_layout() {
    let catalog = catalog();
    for entry in catalog
        .databases
        .iter()
        .filter(|entry| entry.schema_version < catalog.support.current_schema)
    {
        let owner = match entry.schema_version {
            1 | 2 => "owner_session",
            3 | 4 => "assignee",
            _ => "claim_key",
        };
        for value in ["", " \t\n", "\u{a0}\u{2003}"] {
            let project = copy_database(entry);
            Connection::open(project.path().join(".qqq/qqq.db"))
                .unwrap()
                .execute(&format!("UPDATE tasks SET {owner}=? WHERE id=8"), [value])
                .unwrap();
            let before = hashes_at(&project.path().join(".qqq"));
            for json in [false, true] {
                failure(
                    project.path(),
                    &["list"],
                    json,
                    "invalid task status or claim",
                    "DATABASE_ERROR",
                );
                assert_eq!(hashes_at(&project.path().join(".qqq")), before);
                assert!(archives(project.path()).is_empty());
            }
        }
    }
}

#[test]
fn wal_previews_and_diagnostics_never_create_or_change_source_sidecars() {
    let catalog = catalog();
    let entry = catalog.databases.last().unwrap();
    for existing_sidecars in [false, true] {
        let source = copy_database(entry);
        let database = source.path().join(".qqq/qqq.db");
        let mut writer = Some(Connection::open(&database).unwrap());
        writer
            .as_ref()
            .unwrap()
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        if !existing_sidecars {
            writer = None;
        } else {
            // Keep writer connection alive so WAL/shm files remain present.
            writer
                .as_ref()
                .unwrap()
                .execute(
                    "UPDATE tasks SET description=description || ' WAL' WHERE id=3",
                    [],
                )
                .unwrap();
        }
        fs::write(
            source.path().join("preview.json"),
            br#"{"version":1,"tasks":[{"key":"preview","description":"Synthetic"}]}"#,
        )
        .unwrap();
        let before = hashes_at(source.path());
        for args in [
            vec!["next", "--dry-run"],
            vec!["next", "--dry-run", "--wait"],
            vec!["next", "--explain"],
            vec!["status"],
            vec!["import", "preview.json", "--dry-run"],
        ] {
            for json in [false, true] {
                let error = failure(source.path(), &args, json, "read-only", "DATABASE_ERROR");
                if json {
                    assert_eq!(error["details"]["reason"], "unsafe_read_only");
                }
                assert_eq!(hashes_at(source.path()), before);
            }
        }
        assert!(!source.path().join(".qqq-upgrades").exists());
        drop(writer);
    }
    for bytes in [b"".as_slice(), b"synthetic corrupt DB".as_slice()] {
        let source = copy_database(entry);
        fs::write(source.path().join(".qqq/qqq.db"), bytes).unwrap();
        fs::write(
            source.path().join(".qqq/qqq.db-journal"),
            b"keep original journal",
        )
        .unwrap();
        let before = hashes_at(source.path());
        for json in [false, true] {
            failure(
                source.path(),
                &["next", "--dry-run"],
                json,
                "read-only",
                "DATABASE_ERROR",
            );
            assert_eq!(hashes_at(source.path()), before);
        }
    }
}

#[test]
fn doctor_defers_persisted_wal_without_creating_sidecars() {
    let catalog = catalog();
    let source = copy_database(catalog.databases.last().unwrap());
    let conn = Connection::open(source.path().join(".qqq/qqq.db")).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    drop(conn);
    let before = hashes_at(source.path());
    let output = command(source.path())
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(hashes_at(source.path()), before);
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["code"] == "DB_READ_ONLY_UNSAFE")
    );
}
