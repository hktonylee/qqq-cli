use super::compatibility::*;
use rusqlite::Connection;
use serde_json::Value;
use std::{fs, process::Stdio};

#[test]
fn repeated_normal_opens_are_idempotent_and_keep_claim_constraints() {
    let originals = tree_hashes();
    for entry in catalog().databases {
        let dir = copy_database(&entry);
        assert_migrated(dir.path(), &entry);
        let project = dir.path().join(".qqq");
        let before = hashes_at(&project);
        assert_migrated(dir.path(), &entry);
        assert_eq!(
            hashes_at(&project),
            before,
            "reopen schema {}",
            entry.schema_version
        );
        let conn = Connection::open(project.join("qqq.db")).unwrap();
        conn.execute_batch("BEGIN").unwrap();
        for sql in [
            "UPDATE tasks SET claim_key=NULL WHERE id=8",
            "UPDATE tasks SET claim_key='invalid-owner' WHERE id=3",
            "UPDATE tasks SET claim_key='invalid-owner' WHERE id=34",
            "UPDATE tasks SET claim_key='fixture-owner-linked' WHERE id=55",
            "UPDATE tasks SET status='pending' WHERE id=13",
        ] {
            assert!(
                conn.execute(sql, []).is_err(),
                "migrated CHECK/unique constraint missing: {sql}"
            );
        }
        conn.execute_batch("ROLLBACK").unwrap();
        assert_eq!(canonical(&conn), expected(&entry)["canonical"]);
        drop(conn);
        assert_eq!(hashes_at(&project), before);
    }
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn simultaneous_openers_migrate_each_historical_schema_once() {
    let originals = tree_hashes();
    for entry in catalog().databases {
        let dir = copy_database(&entry);
        let lock = Connection::open(dir.path().join(".qqq/qqq.db")).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();
        let children: Vec<_> = (0..6)
            .map(|_| {
                command(dir.path())
                    .args(["list", "--all", "--include-archived", "--json"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect();
        lock.execute_batch("ROLLBACK").unwrap();
        drop(lock);
        for child in children {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "schema {}: {}",
                entry.schema_version,
                String::from_utf8_lossy(&output.stderr)
            );
            let tasks: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(tasks.as_array().unwrap().len(), 8);
        }
        assert_migrated(dir.path(), &entry);
        let before = hashes_at(&dir.path().join(".qqq"));
        assert_migrated(dir.path(), &entry);
        assert_eq!(hashes_at(&dir.path().join(".qqq")), before);
    }
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn late_sql_migration_failures_roll_back_all_schemas_and_retry_cleanly() {
    let originals = tree_hashes();
    let catalog = catalog();
    for entry in catalog
        .databases
        .iter()
        .filter(|entry| entry.schema_version < catalog.support.current_schema)
    {
        let dir = copy_database(entry);
        let project = dir.path().join(".qqq");
        let conn = Connection::open(project.join("qqq.db")).unwrap();
        let (inject, remove, error) = if entry.schema_version < 11 {
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
        drop(conn);
        let before = hashes_at(&project);
        for json in [false, true] {
            failure(
                dir.path(),
                &["list", "--all"],
                json,
                error,
                "DATABASE_ERROR",
            );
            assert_eq!(
                hashes_at(&project),
                before,
                "SQL rollback schema {}",
                entry.schema_version
            );
            let conn = read_only(&project.join("qqq.db"));
            assert_eq!(
                conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                    .unwrap(),
                entry.schema_version
            );
        }
        Connection::open(project.join("qqq.db"))
            .unwrap()
            .execute_batch(remove)
            .unwrap();
        assert_migrated(dir.path(), entry);
    }
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn embedded_image_write_conflict_rolls_back_then_retry_preserves_bytes() {
    let originals = tree_hashes();
    for entry in catalog()
        .databases
        .iter()
        .filter(|entry| entry.schema_version < 6)
    {
        let dir = copy_database(entry);
        let project = dir.path().join(".qqq");
        let conflict = project.join("images/13/19.gif");
        fs::create_dir_all(conflict.parent().unwrap()).unwrap();
        fs::write(&conflict, b"synthetic conflict").unwrap();
        let before = hashes_at(&project);
        for json in [false, true] {
            failure(
                dir.path(),
                &["list", "--all"],
                json,
                "Stored image path conflicts",
                "COMMAND_ERROR",
            );
            assert_eq!(hashes_at(&project), before);
            assert!(
                !project.join("images/3/7.png").exists(),
                "first newly written image must roll back"
            );
        }
        fs::remove_file(conflict).unwrap();
        assert_migrated(dir.path(), entry);
    }
    assert_eq!(tree_hashes(), originals);
}
