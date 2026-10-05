use super::compatibility::*;
use rusqlite::Connection;
use serde_json::json;
use std::fs;

#[test]
fn unsupported_versions_legacy_layouts_and_corrupt_db_fail_without_changes() {
    let originals = tree_hashes();
    let catalog = catalog();
    let source = &catalog.databases[0];
    for case in ["future", "zero", "title", "pending", "corrupt"] {
        let dir = copy_database(source);
        let project = dir.path().join(".qqq");
        let path = project.join("qqq.db");
        let conn = Connection::open(&path).unwrap();
        let (human, reason) = match case {
            "future" => {
                conn.pragma_update(None, "user_version", catalog.support.current_schema + 1)
                    .unwrap();
                (
                    "Unsupported database schema version",
                    Some("unsupported_schema"),
                )
            }
            "zero" => {
                conn.pragma_update(None, "user_version", 0).unwrap();
                (
                    "Unsupported database schema version",
                    Some("unsupported_schema"),
                )
            }
            "title" => {
                conn.execute_batch("ALTER TABLE tasks RENAME COLUMN description TO title")
                    .unwrap();
                ("update SQLite manually", Some("legacy_title_column"))
            }
            "pending" => {
                conn.execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE tasks SET status='pending' WHERE id=13").unwrap();
                ("CHECK constraint failed", None)
            }
            "corrupt" => ("file is not a database", None),
            _ => unreachable!(),
        };
        drop(conn);
        if case == "corrupt" {
            fs::write(&path, b"invalid synthetic SQLite fixture").unwrap();
        }
        let before = hashes_at(&project);
        for json in [false, true] {
            let error = failure(
                dir.path(),
                &["list", "--all"],
                json,
                human,
                "DATABASE_ERROR",
            );
            if json {
                if let Some(reason) = reason {
                    assert_eq!(error["details"]["reason"], reason);
                }
            }
            assert_eq!(
                hashes_at(&project),
                before,
                "{case} rejection must not change DB or image files"
            );
        }
    }
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn invalid_historical_foreign_keys_roll_back_images_and_allow_repaired_retry() {
    let originals = tree_hashes();
    let catalog = catalog();
    let entry = catalog
        .databases
        .iter()
        .find(|entry| entry.schema_version == 5)
        .unwrap();
    let dir = copy_database(entry);
    let project = dir.path().join(".qqq");
    let path = project.join("qqq.db");
    let conn = Connection::open(&path).unwrap();
    conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
    conn.execute("UPDATE images SET task_id=999 WHERE id=19", [])
        .unwrap();
    drop(conn);
    let before = hashes_at(&project);
    for json in [false, true] {
        failure(
            dir.path(),
            &["list", "--all"],
            json,
            "invalid foreign key references",
            "COMMAND_ERROR",
        );
        assert_eq!(hashes_at(&project), before);
        assert!(!project.join("images/3/7.png").exists());
        assert!(!project.join("images/999/19.gif").exists());
    }
    Connection::open(path)
        .unwrap()
        .execute("UPDATE images SET task_id=13 WHERE id=19", [])
        .unwrap();
    assert_migrated(dir.path(), entry);
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn read_only_diagnostics_and_import_preview_preserve_old_and_current_projects() {
    let originals = tree_hashes();
    let catalog = catalog();
    for entry in &catalog.databases {
        let dir = copy_database(entry);
        let project = dir.path().join(".qqq");
        let import = dir.path().join("preview.json");
        fs::write(
            &import,
            serde_json::to_vec(
                &json!({"version":1,"tasks":[{"key":"preview","description":"Synthetic preview"}]}),
            )
            .unwrap(),
        )
        .unwrap();
        let commands = [
            vec!["status"],
            vec!["next", "--explain", "--session", "fixture-preview"],
            vec!["import", import.to_str().unwrap(), "--dry-run"],
        ];
        let staging = project.join(".delete-staging");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("marker"), b"synthetic pending staging").unwrap();
        let before = hashes_at(&project);
        for args in &commands {
            if entry.schema_version < catalog.support.current_schema {
                for json in [false, true] {
                    let error = failure(
                        dir.path(),
                        args,
                        json,
                        "requires migration",
                        "DATABASE_ERROR",
                    );
                    if json {
                        assert_eq!(error["details"]["reason"], "migration_required");
                        assert_eq!(error["details"]["schema_version"], entry.schema_version);
                        assert_eq!(
                            error["details"]["expected_schema_version"],
                            catalog.support.current_schema
                        );
                    }
                }
            } else {
                run(dir.path(), args);
            }
            assert_eq!(hashes_at(&project), before);
        }
        fs::remove_dir_all(staging).unwrap();
        assert_migrated(dir.path(), entry);
        let before = hashes_at(&project);
        for args in &commands {
            run(dir.path(), args);
        }
        assert_eq!(
            run(
                dir.path(),
                &["next", "--dry-run", "--session", "fixture-owner-linked"]
            )["id"],
            expected(entry)["queue"]["selection_id"]
        );
        assert_eq!(
            hashes_at(&project),
            before,
            "read-only/preview commands changed upgraded project"
        );
    }
    assert_eq!(tree_hashes(), originals);
}

#[test]
fn next_dry_run_retains_normal_upgrade_semantics_without_claiming() {
    let originals = tree_hashes();
    for entry in catalog().databases {
        let dir = copy_database(&entry);
        let preview = run(
            dir.path(),
            &["next", "--dry-run", "--session", "fixture-owner-linked"],
        );
        assert_eq!(preview["id"], expected(&entry)["queue"]["selection_id"]);
        let conn = read_only(&dir.path().join(".qqq/qqq.db"));
        assert_eq!(
            canonical(&conn),
            expected(&entry)["canonical"],
            "dry-run may upgrade but must not claim"
        );
        assert_migrated(dir.path(), &entry);
    }
    assert_eq!(tree_hashes(), originals);
}
