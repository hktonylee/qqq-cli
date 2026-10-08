use super::{compatibility::*, recovery};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use tempfile::TempDir;

fn reference() -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/reference.md")).unwrap()
}

fn marked_block<'a>(document: &'a str, name: &str) -> &'a str {
    let start = format!("<!-- {name}:start -->");
    let end = format!("<!-- {name}:end -->");
    assert_eq!(
        document.matches(&start).count(),
        1,
        "missing/duplicate {start}"
    );
    assert_eq!(document.matches(&end).count(), 1, "missing/duplicate {end}");
    document
        .split_once(&start)
        .unwrap()
        .1
        .split_once(&end)
        .expect("documentation block markers must be ordered")
        .0
}

fn schema_range(versions: &[i64]) -> String {
    let first = *versions.first().expect("supported schemas cannot be empty");
    let last = *versions.last().unwrap();
    assert_eq!(versions, (first..=last).collect::<Vec<_>>());
    if first == last {
        first.to_string()
    } else {
        format!("{first}–{last}")
    }
}

#[test]
fn compatibility_documentation_matches_fixture_matrix_and_runtime() {
    let catalog = catalog();
    let initialized = TempDir::new().unwrap();
    run(initialized.path(), &["init"]);
    let current: i64 = read_only(&initialized.path().join(".qqq/qqq.db"))
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(catalog.support.current_schema, current);
    let normal: Vec<_> = catalog
        .databases
        .iter()
        .map(|db| db.schema_version)
        .collect();
    assert_eq!(
        normal,
        (1..=current).collect::<Vec<_>>(),
        "new schema needs matrix coverage"
    );
    assert_eq!(catalog.support.normal_open_schemas, normal);
    let portable: Vec<_> = catalog
        .snapshots
        .iter()
        .map(|db| db.schema_version)
        .collect();
    assert_eq!(portable, catalog.support.snapshot_database_schemas);
    assert_eq!(
        portable.last(),
        Some(&current),
        "new schema needs portable fixture coverage"
    );
    run(initialized.path(), &["backup", "portable.tar"]);
    let (portable_manifest, _) =
        recovery::archive_payload(&initialized.path().join("portable.tar"));
    let portable_format = portable_manifest["version"].as_i64().unwrap();
    assert_eq!(catalog.support.snapshot_format_versions, [portable_format]);
    assert!(
        catalog
            .snapshots
            .iter()
            .all(|entry| entry.format_version == portable_format)
    );

    let oldest = &catalog.databases[0];
    let source = copy_database(oldest);
    run(source.path(), &["list"]);
    let snapshots = recovery::archives(source.path());
    assert_eq!(snapshots.len(), 1);
    let (upgrade, _) = recovery::archive_payload(&snapshots[0]);
    // Upgrade metadata uses SCHEMA_VERSION directly; init must emit same version.
    assert_eq!(upgrade["upgrade"]["target_schema"], current);
    assert_eq!(upgrade["upgrade"]["source_schema"], oldest.schema_version);
    let recovered = TempDir::new().unwrap();
    run(
        recovered.path(),
        &["restore", "--recovery", snapshots[0].to_str().unwrap()],
    );
    assert_eq!(
        read_only(&recovered.path().join(".qqq/qqq.db"))
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        oldest.schema_version
    );

    assert!(
        catalog
            .support
            .exclusions
            .iter()
            .any(|value| value == "version0-initialization-only")
    );
    assert!(
        catalog
            .support
            .exclusions
            .iter()
            .any(|value| value == "future-schema-unsupported")
    );
    let manual: Vec<_> = catalog
        .support
        .exclusions
        .iter()
        .filter_map(|value| {
            value
                .strip_prefix("legacy-")?
                .strip_suffix("-manual-conversion")
        })
        .collect();
    let upgrades: Vec<_> = normal
        .iter()
        .copied()
        .filter(|version| *version < current)
        .collect();
    let expected = BTreeMap::from([
        ("Current DB schema".to_owned(), current.to_string()),
        ("Normal-open DB schemas".to_owned(), schema_range(&normal)),
        (
            "Automatic upgrade source schemas".to_owned(),
            schema_range(&upgrades),
        ),
        (
            "Portable snapshot format".to_owned(),
            portable_format.to_string(),
        ),
        (
            "Portable snapshot DB schemas".to_owned(),
            schema_range(&portable),
        ),
        (
            "Upgrade recovery format".to_owned(),
            upgrade["version"].to_string(),
        ),
        (
            "Upgrade recovery source schemas".to_owned(),
            schema_range(&upgrades),
        ),
        ("Initialization-only schema".to_owned(), "0".to_owned()),
        ("Manual-conversion layouts".to_owned(), manual.join(", ")),
    ]);
    let document = reference();
    let mut published = BTreeMap::new();
    for line in marked_block(&document, "compatibility-support").lines() {
        let cells: Vec<_> = line.trim().split('|').map(str::trim).collect();
        if cells.len() != 4 || matches!(cells[1], "Contract" | "---") {
            continue;
        }
        assert!(
            published
                .insert(cells[1].to_owned(), cells[2].replace('`', ""))
                .is_none(),
            "duplicate published compatibility fact: {}",
            cells[1]
        );
    }
    assert_eq!(
        published, expected,
        "published compatibility facts drifted from fixture matrix/runtime"
    );
}

fn shell_example(name: &str, directory: &Path, temp_root: &Path, input: &str) -> String {
    let document = reference();
    let script = marked_block(&document, name)
        .trim()
        .strip_prefix("```sh\n")
        .and_then(|block| block.strip_suffix("```"))
        .expect("documented example must contain one shell block");
    let binary = std::env::var_os("QQQ_COMPATIBILITY_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_qqq").into());
    // Synthetic saved servers must not be compared with user's running Herdr.
    let probes = TempDir::new().unwrap();
    fs::write(probes.path().join("herdr"), "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            probes.path().join("herdr"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let mut path = vec![
        probes.path().to_path_buf(),
        Path::new(&binary).parent().unwrap().to_path_buf(),
    ];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut child = Command::new("/bin/sh")
        .args(["-eu", "-c", script])
        .current_dir(directory)
        .env("PATH", std::env::join_paths(path).unwrap())
        .env("TMPDIR", temp_root)
        .env_remove("QQQ_SESSION")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .env_remove("HERDR_ENV")
        .env_remove("HERDR_PANE_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{name}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn compatibility_documentation_portable_example_runs_on_historical_projects() {
    let original_hashes = tree_hashes();
    let catalog = catalog();
    for entry in catalog.databases.iter().filter(|entry| {
        catalog
            .support
            .snapshot_database_schemas
            .contains(&entry.schema_version)
    }) {
        let project = copy_database(entry);
        let parent = TempDir::new().unwrap();
        let original = parent.path().join("original-project");
        fs::create_dir(&original).unwrap();
        fs::rename(project.path().join(".qqq"), original.join(".qqq")).unwrap();
        shell_example(
            "compatibility-portable-example",
            &original,
            parent.path(),
            "",
        );
        let restored = parent.path().join("restored-project");
        assert_migrated(&restored, entry);
    }
    assert_eq!(tree_hashes(), original_hashes);
}

#[test]
fn compatibility_documentation_recovery_example_runs_on_every_upgrade_source() {
    let original_hashes = tree_hashes();
    let catalog = catalog();
    for entry in catalog
        .databases
        .iter()
        .filter(|entry| entry.schema_version < catalog.support.current_schema)
    {
        let project = copy_database(entry);
        run(project.path(), &["list"]);
        let paths = recovery::archives(project.path());
        assert_eq!(paths.len(), 1);
        let temp_root = TempDir::new().unwrap();
        let output = shell_example(
            "compatibility-recovery-example",
            project.path(),
            temp_root.path(),
            &format!("{}\n", paths[0].display()),
        );
        let recovered = fs::read_dir(temp_root.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(
            output.contains(&format!("Recovered project: {}", recovered.display())),
            "{output}"
        );
        assert!(
            output.contains(&format!("{}\nok", entry.schema_version)),
            "SQLite inspection must precede migration: {output}"
        );
        assert_eq!(recovery::archives(&recovered).len(), 1);
        assert_migrated(&recovered, entry);
    }
    assert_eq!(tree_hashes(), original_hashes);
}
