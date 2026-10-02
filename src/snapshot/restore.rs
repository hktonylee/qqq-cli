use super::{
    backup::validate_database,
    format::{
        DATABASE_NAME, MANIFEST_NAME, MAX_MANIFEST_BYTES, Manifest, hash_reader,
        validate_image_path,
    },
};
use crate::images::ImageStore;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use tar::{Archive, Entry};
use tempfile::TempDir;

pub fn run(source: &Path) -> Result<Value> {
    let cwd = std::env::current_dir()?.canonicalize()?;
    let source = absolute(source, &cwd);
    let metadata = fs::symlink_metadata(&source)
        .with_context(|| format!("Cannot read snapshot {}", source.display()))?;
    ensure!(
        metadata.file_type().is_file(),
        "Snapshot source must be a regular file"
    );
    let target = cwd.join(".qqq");
    for ancestor in cwd.ancestors().skip(1) {
        match fs::symlink_metadata(ancestor.join(".qqq")) {
            Ok(_) => anyhow::bail!("Cannot restore beneath ancestor project"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    let existing_empty = match fs::symlink_metadata(&target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
        Ok(metadata) => {
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "Restore target .qqq is not empty or is unsafe"
            );
            ensure!(
                fs::read_dir(&target)?.next().is_none(),
                "Restore target .qqq is not empty"
            );
            true
        }
    };
    let stage = TempDir::new_in(&cwd)?;
    let staged_project = stage.path().join(".qqq");
    fs::create_dir(&staged_project)?;
    let (manifest, bytes) = extract(&source, &staged_project)?;
    let tasks = validate_database(&staged_project.join(DATABASE_NAME))?;
    validate_database_images(&staged_project, &manifest)?;
    install(&staged_project, &target, existing_empty)?;
    Ok(json!({
        "source": source,
        "destination": target,
        "tasks": tasks,
        "images": manifest.images.len(),
        "bytes": bytes,
    }))
}

fn absolute(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn extract(source: &Path, staged_project: &Path) -> Result<(Manifest, u64)> {
    let file = fs::File::open(source)?;
    let bytes = file.metadata()?.len();
    let mut archive = Archive::new(file);
    let mut entries = archive.entries()?;
    let mut first = entries.next().context("Snapshot archive is empty")??;
    ensure!(
        first.header().entry_type().is_file(),
        "Snapshot manifest is not a regular file"
    );
    ensure!(
        entry_name(&first)? == MANIFEST_NAME,
        "Snapshot manifest must be first"
    );
    ensure!(
        first.header().size()? <= MAX_MANIFEST_BYTES,
        "Snapshot manifest is too large"
    );
    let mut manifest_bytes = Vec::new();
    first
        .by_ref()
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut manifest_bytes)?;
    ensure!(
        manifest_bytes.len() as u64 <= MAX_MANIFEST_BYTES,
        "Snapshot manifest is too large"
    );
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).context("Invalid snapshot manifest")?;
    manifest.validate()?;
    drop(first);

    let mut expected: HashSet<String> = manifest
        .images
        .iter()
        .map(|image| image.path.clone())
        .collect();
    expected.insert(DATABASE_NAME.to_owned());
    let image_sizes: HashMap<_, _> = manifest
        .images
        .iter()
        .map(|image| (image.path.as_str(), image.bytes))
        .collect();
    for entry in entries {
        let mut entry = entry?;
        ensure!(
            entry.header().entry_type().is_file(),
            "Archive entry is not a regular file"
        );
        let name = entry_name(&entry)?;
        ensure!(
            expected.remove(&name),
            "Unexpected or duplicate archive entry: {name}"
        );
        let expected_bytes = if name == DATABASE_NAME {
            manifest.database.bytes
        } else {
            image_sizes[name.as_str()]
        };
        ensure!(
            entry.header().size()? == expected_bytes,
            "Snapshot entry size differs from manifest: {name}"
        );
        let output = staged_project.join(&name);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)?;
        let copied = std::io::copy(&mut entry, &mut file)?;
        ensure!(copied == expected_bytes, "Snapshot entry truncated: {name}");
        file.flush()?;
        file.sync_all()?;
    }
    ensure!(
        expected.is_empty(),
        "Snapshot is missing database or image entries"
    );
    check_hash(
        &staged_project.join(DATABASE_NAME),
        manifest.database.bytes,
        &manifest.database.sha256,
    )?;
    for image in &manifest.images {
        check_hash(
            &staged_project.join(&image.path),
            image.bytes,
            &image.sha256,
        )
        .with_context(|| format!("Invalid snapshot image {}", image.path))?;
    }
    Ok((manifest, bytes))
}

fn entry_name(entry: &Entry<'_, fs::File>) -> Result<String> {
    let path = entry.path()?;
    ensure!(
        path.components()
            .all(|component| matches!(component, Component::Normal(_))),
        "Unsafe archive path"
    );
    let name = path
        .to_str()
        .context("Archive path is not UTF-8")?
        .to_owned();
    ensure!(
        !name.contains('\\') && !name.contains("//"),
        "Unsafe archive path"
    );
    ensure!(
        name == MANIFEST_NAME || name == DATABASE_NAME || validate_image_path(&name).is_ok(),
        "Unsafe archive path"
    );
    Ok(name)
}

fn check_hash(path: &Path, bytes: u64, expected: &str) -> Result<()> {
    let actual = hash_reader(fs::File::open(path)?)?;
    ensure!(
        actual.bytes == bytes,
        "Snapshot file size differs from manifest"
    );
    ensure!(
        actual.sha256 == expected,
        "Snapshot file hash differs from manifest"
    );
    Ok(())
}

fn validate_database_images(project: &Path, manifest: &Manifest) -> Result<()> {
    let conn = Connection::open_with_flags(
        project.join(DATABASE_NAME),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let store = ImageStore::new(project.join("images"));
    let mut expected: HashMap<_, _> = manifest
        .images
        .iter()
        .map(|image| (image.path.as_str(), image.bytes))
        .collect();
    let mut statement =
        conn.prepare("SELECT id,task_id,media_type,bytes FROM images ORDER BY id")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    for row in rows {
        let (id, task_id, media_type, bytes) = row?;
        let path = store.path(task_id, id, &media_type)?;
        let relative = path
            .strip_prefix(project)?
            .to_str()
            .context("Image path is not UTF-8")?;
        let manifest_bytes = expected
            .remove(relative)
            .with_context(|| format!("Snapshot missing image {relative}"))?;
        ensure!(
            bytes >= 0 && bytes as u64 == manifest_bytes,
            "Snapshot image byte count differs from database"
        );
    }
    ensure!(
        expected.is_empty(),
        "Snapshot has images absent from database"
    );
    Ok(())
}

fn install(staged: &Path, target: &Path, existing_empty: bool) -> Result<()> {
    if existing_empty {
        fs::remove_dir(target).context("Restore target .qqq is not empty")?;
    }
    if let Err(error) = fs::rename(staged, target) {
        if existing_empty && !target.exists() {
            fs::create_dir(target)
                .context("Cannot restore original empty .qqq after failed install")?;
        }
        return Err(error).context("Cannot install snapshot");
    }
    Ok(())
}
