use super::compatibility::*;
use rusqlite::{Connection, types::ValueRef};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
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

fn archives(project: &Path) -> Vec<PathBuf> {
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

fn archive_payload(path: &Path) -> (Value, BTreeMap<String, Vec<u8>>) {
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
