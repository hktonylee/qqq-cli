#[path = "common/compatibility.rs"]
mod compatibility;

use compatibility::*;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};
use tempfile::TempDir;

#[test]
fn historical_catalog_covers_current_database_and_snapshot_inventory() {
    let catalog = catalog();
    let project = TempDir::new().unwrap();
    run(project.path(), &["init"]);
    let conn = read_only(&project.path().join(".qqq/qqq.db"));
    let current: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let versions: Vec<_> = catalog
        .databases
        .iter()
        .map(|entry| entry.schema_version)
        .collect();
    assert_eq!(
        versions,
        (1..=current).collect::<Vec<_>>(),
        "new schema needs immutable fixture"
    );
    let snapshots: Vec<_> = catalog
        .snapshots
        .iter()
        .map(|entry| entry.schema_version)
        .collect();
    assert_eq!(snapshots, (9..=current).collect::<Vec<_>>());
    assert_eq!(catalog.version, 1);
    assert_eq!(catalog.support.current_schema, current);
    assert_eq!(catalog.support.normal_open_schemas, versions);
    assert_eq!(catalog.support.snapshot_database_schemas, snapshots);
    assert_eq!(catalog.support.released_snapshot_schemas, [9, 11]);
    assert_eq!(
        catalog.support.exclusions,
        [
            "version0-initialization-only",
            "legacy-title-manual-conversion",
            "legacy-pending-manual-conversion",
            "future-schema-unsupported"
        ]
    );
    assert_eq!(catalog.support.provenance_notes.len(), 2);
    assert_eq!(catalog.sqlite_generation_version, "3.53.1");
    run(project.path(), &["backup", "inventory.tar"]);
    let mut archive =
        tar::Archive::new(fs::File::open(project.path().join("inventory.tar")).unwrap());
    let mut entries = archive.entries().unwrap();
    let mut manifest = entries.next().unwrap().unwrap();
    let emitted: Value = serde_json::from_reader(&mut manifest).unwrap();
    assert!(
        catalog
            .support
            .snapshot_format_versions
            .contains(&emitted["version"].as_i64().unwrap()),
        "new snapshot format needs historical fixture"
    );
    assert!(
        catalog
            .snapshots
            .iter()
            .any(|entry| entry.format_version == emitted["version"].as_i64().unwrap())
    );
}

#[test]
fn historical_sources_and_artifacts_match_recorded_provenance() {
    let before = tree_hashes();
    let catalog = catalog();
    let provenance_data = fs::read(root().join(&catalog.source_provenance)).unwrap();
    assert_eq!(sha256(&provenance_data), catalog.source_provenance_sha256);
    let provenance: Value = serde_json::from_slice(&provenance_data).unwrap();
    assert_eq!(provenance["version"], 1);
    let sources: BTreeSet<_> = provenance["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| {
            let path = source["path"].as_str().unwrap();
            assert_eq!(
                sha256(&fs::read(root().join(path)).unwrap()),
                source["sha256"].as_str().unwrap()
            );
            for key in ["commit", "git_blob"] {
                let value = source[key].as_str().unwrap();
                assert_eq!(value.len(), 40);
                assert!(value.bytes().all(|byte| byte.is_ascii_hexdigit()));
            }
            assert!(
                source["original_path"]
                    .as_str()
                    .unwrap()
                    .starts_with("src/")
            );
            if path.starts_with("sources/schema12/") {
                assert!(source["release"].is_null());
            } else {
                assert!(source["release"].as_str().unwrap().starts_with('v'));
            }
            assert!(!source["package_version"].as_str().unwrap().is_empty());
            path.to_owned()
        })
        .collect();
    assert_eq!(sources.len(), 25);
    let releases = provenance["releases"].as_array().unwrap();
    assert_eq!(
        releases
            .iter()
            .map(|release| release["tag"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["v0.1.1", "v0.2.0", "v0.3.0", "v0.4.0"]
    );
    for release in releases {
        assert_eq!(release["snapshot_format"], 1);
        assert_eq!(
            release["snapshot_database_schema"],
            release["database_schema"]
        );
        assert!(
            catalog
                .support
                .released_snapshot_schemas
                .contains(&release["database_schema"].as_i64().unwrap())
        );
    }
    for entry in &catalog.databases {
        assert_file(&root().join(&entry.database), entry.bytes, &entry.sha256);
        expected(entry);
        assert_eq!(entry.sql_sources.len() as i64, entry.schema_version);
        assert!(entry.sql_sources.iter().all(|path| sources.contains(path)));
        assert!(entry.sql_sources[0].starts_with(&format!("sources/{}/", entry.base_release)));
        if entry.schema_version == 12 {
            assert!(entry.source_release.is_none());
            assert_eq!(entry.source_kind, "pinned-development-extension");
        } else {
            assert_eq!(entry.source_release.as_ref(), Some(&entry.base_release));
            assert_eq!(entry.source_kind, "released-source-definition");
        }
        assert_eq!(
            entry.files.len(),
            if entry.schema_version < 6 { 0 } else { 2 }
        );
        for image in &entry.files {
            assert_file(&root().join(&image.source), image.bytes, &image.sha256);
        }
    }
    for entry in &catalog.snapshots {
        assert_file(&root().join(&entry.path), entry.bytes, &entry.sha256);
        assert_eq!(entry.format_version, 1);
        assert_eq!(entry.construction, "synthetic-source-derived-gnu-tar");
        assert!(sources.contains(&entry.format_source));
        assert_eq!(
            entry.emitted_by_tagged_release,
            catalog
                .support
                .released_snapshot_schemas
                .contains(&entry.schema_version)
        );
        assert_eq!(
            entry.source_release,
            catalog
                .databases
                .iter()
                .find(|db| db.schema_version == entry.schema_version)
                .unwrap()
                .source_release
        );
    }
    assert_eq!(tree_hashes(), before);
}

#[test]
fn historical_databases_have_exact_source_data_and_integrity() {
    let before = tree_hashes();
    for entry in catalog().databases {
        let conn = read_only(&root().join(&entry.database));
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            entry.schema_version
        );
        assert_eq!(
            conn.pragma_query_value(None, "integrity_check", |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(rows(&conn, "PRAGMA foreign_key_check"), json!([]));
        let expected = expected(&entry);
        let source_tasks = rows(&conn, "SELECT * FROM tasks ORDER BY id");
        for (source, canonical) in source_tasks
            .as_array()
            .unwrap()
            .iter()
            .zip(expected["canonical"]["tasks"].as_array().unwrap())
        {
            for (column, value) in source.as_object().unwrap() {
                let key = if column == "owner_session" || column == "assignee" {
                    "claim_key"
                } else {
                    column
                };
                assert_eq!(
                    value, &canonical[key],
                    "schema {}, task {}, {column}",
                    entry.schema_version, source["id"]
                );
            }
        }
        assert_eq!(
            source_tasks
                .as_array()
                .unwrap()
                .iter()
                .map(|task| task["id"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            [3, 8, 13, 21, 34, 55, 89, 144]
        );
        for (column, since) in [
            ("parent_id", 2),
            ("harness_name", 5),
            ("priority", 7),
            ("archived", 8),
            ("content_revision", 10),
            ("tags", 12),
        ] {
            assert_eq!(
                source_tasks[0].get(column).is_some(),
                entry.schema_version >= since
            );
        }
        for (table, columns) in [
            ("messages", "id,task_id,body,session,created_at"),
            ("events", "id,task_id,session,action,created_at"),
        ] {
            assert_eq!(
                rows(&conn, &format!("SELECT {columns} FROM {table} ORDER BY id")),
                expected["canonical"][table]
            );
        }
        assert_eq!(
            rows(
                &conn,
                "SELECT task_id,link_json FROM herdr_links ORDER BY task_id"
            ),
            expected["canonical"]["links"]
        );
        assert_eq!(
            rows(&conn, "SELECT name,seq FROM sqlite_sequence ORDER BY name"),
            expected["canonical"]["sequences"]
        );
        if entry.schema_version >= 11 {
            assert_eq!(
                rows(
                    &conn,
                    "SELECT task_id,prerequisite_id FROM task_dependencies ORDER BY task_id,prerequisite_id"
                ),
                expected["canonical"]["dependencies"]
            );
        } else {
            assert_eq!(
                rows(
                    &conn,
                    "SELECT name FROM sqlite_master WHERE name='task_dependencies'"
                ),
                json!([])
            );
        }
        assert_eq!(
            rows(
                &conn,
                "SELECT id,task_id,name,media_type FROM images ORDER BY id"
            )
            .as_array()
            .unwrap()
            .len(),
            2
        );
        for image in &entry.images {
            assert_eq!(
                conn.query_row(
                    "SELECT task_id,media_type FROM images WHERE id=?",
                    [image.id],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                )
                .unwrap(),
                (image.task_id, image.media_type.clone())
            );
            if entry.schema_version < 6 {
                let data: Vec<u8> = conn
                    .query_row("SELECT data FROM images WHERE id=?", [image.id], |row| {
                        row.get(0)
                    })
                    .unwrap();
                assert_eq!(data.len() as u64, image.bytes);
                assert_eq!(sha256(&data), image.sha256);
            } else {
                assert_eq!(
                    conn.query_row("SELECT bytes FROM images WHERE id=?", [image.id], |row| row
                        .get::<_, u64>(0))
                        .unwrap(),
                    image.bytes
                );
            }
        }
    }
    assert_eq!(
        tree_hashes(),
        before,
        "source DBs must remain untouched, including sidecars"
    );
}

#[test]
fn every_historical_database_migrates_preserving_state_readiness_and_sequences() {
    let before = tree_hashes();
    for entry in catalog().databases {
        let dir = copy_database(&entry);
        let path = dir.path();
        assert_migrated(path, &entry);
        let image = root().join("assets/pixel.png");
        assert_eq!(
            run(
                path,
                &[
                    "add",
                    "After upgrade",
                    "--priority",
                    "100",
                    "--image",
                    image.to_str().unwrap()
                ]
            )["id"],
            234
        );
        run(path, &["message", "234", "After upgrade note"]);
        assert_eq!(
            run(path, &["next", "--local", "--session", "fixture-new-owner"])["id"],
            234
        );
        let conn = read_only(&path.join(".qqq/qqq.db"));
        assert_eq!(
            rows(&conn, "SELECT id,task_id FROM images WHERE id>31"),
            json!([{"id":32,"task_id":234}])
        );
        assert_eq!(
            rows(&conn, "SELECT id,task_id FROM messages WHERE id>37"),
            json!([{"id":38,"task_id":234}])
        );
        assert_eq!(
            rows(&conn, "SELECT id,task_id,action FROM events WHERE id>89"),
            json!([{"id":90,"task_id":234,"action":"claim"}])
        );
        assert_eq!(
            rows(&conn, "SELECT name,seq FROM sqlite_sequence ORDER BY name"),
            json!([{"name":"events","seq":90},{"name":"images","seq":32},{"name":"messages","seq":38},{"name":"tasks","seq":234}])
        );
    }
    assert_eq!(
        tree_hashes(),
        before,
        "migration/allocation must mutate temporary copies only"
    );
}

#[test]
fn every_historical_snapshot_restores_then_migrates_preserving_canonical_state() {
    let before = tree_hashes();
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
        let conn = read_only(&target.path().join(".qqq/qqq.db"));
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            snapshot.schema_version
        );
        drop(conn);
        assert_migrated(target.path(), entry);
    }
    assert_eq!(
        tree_hashes(),
        before,
        "restore must leave archives and original DBs untouched"
    );
}
