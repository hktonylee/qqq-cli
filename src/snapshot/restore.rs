use super::{
    backup::validate_database_for,
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
    path::{Path, PathBuf},
};
use tar::{Archive, Entry};
use tempfile::TempDir;

pub fn run(source: &Path, recovery: bool) -> Result<Value> {
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
    let (tasks, images) = validate_project(&staged_project, &manifest, recovery)?;
    sync_staged_tree(&staged_project, &manifest)?;
    install(&staged_project, &target, existing_empty)?;
    Ok(json!({
        "source": source,
        "destination": target,
        "tasks": tasks,
        "images": images,
        "bytes": bytes,
    }))
}

pub(crate) fn verify_archive(source: &Path, stage: &Path) -> Result<()> {
    let (manifest, _) = extract(source, stage)?;
    validate_project(stage, &manifest, true)?;
    Ok(())
}

fn validate_project(project: &Path, manifest: &Manifest, recovery: bool) -> Result<(i64, i64)> {
    ensure!(
        manifest.version == if recovery { 2 } else { 1 },
        "Snapshot kind does not match restore mode; use --recovery for upgrade snapshots"
    );
    let tasks = validate_database_for(&project.join(DATABASE_NAME), recovery)?;
    let conn = Connection::open_with_flags(
        project.join(DATABASE_NAME),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if let Some(upgrade) = &manifest.upgrade {
        ensure!(
            upgrade.source_schema == version && upgrade.target_schema <= crate::db::SCHEMA_VERSION,
            "Upgrade snapshot schema metadata differs from database or supported schema"
        );
    }
    if version < 6 {
        ensure!(
            manifest.images.is_empty(),
            "Embedded-image recovery snapshot has external images"
        );
        let mut statement =
            conn.prepare("SELECT id,task_id,media_type,data FROM images ORDER BY id")?;
        let store = ImageStore::new(project.join("images"));
        for row in statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })? {
            let (id, task, media, _data) = row?;
            store.path(task, id, &media)?;
        }
    } else {
        validate_database_images(project, manifest)?;
    }
    let images = conn.query_row("SELECT count(*) FROM images", [], |row| row.get(0))?;
    Ok((tasks, images))
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
    let payload_bytes =
        manifest
            .images
            .iter()
            .try_fold(manifest.database.bytes, |total, image| {
                total
                    .checked_add(image.bytes)
                    .context("Snapshot payload size overflow")
            })?;
    ensure!(
        payload_bytes <= bytes,
        "Snapshot payload exceeds archive size"
    );
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
    let bytes = entry.path_bytes();
    let name = std::str::from_utf8(bytes.as_ref())
        .context("Archive path is not UTF-8")?
        .to_owned();
    ensure!(
        !name.starts_with('/') && !name.contains('\\') && !name.contains("//"),
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
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Image filename is not UTF-8")?;
        let relative = format!("images/{task_id}/{filename}");
        let manifest_bytes = expected
            .remove(relative.as_str())
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
    sync_directory(target.parent().context("Restore target has no parent")?)
        .context("Restore installed but directory sync failed; inspect .qqq before retrying")?;
    Ok(())
}

fn sync_staged_tree(project: &Path, manifest: &Manifest) -> Result<()> {
    let mut directories = HashSet::new();
    for image in &manifest.images {
        let image_dir = project
            .join(&image.path)
            .parent()
            .context("Image path has no parent")?
            .to_path_buf();
        directories.insert(image_dir);
    }
    for directory in directories {
        sync_directory(&directory)?;
    }
    if project.join("images").is_dir() {
        sync_directory(&project.join("images"))?;
    }
    sync_directory(project)?;
    sync_directory(project.parent().context("Staging project has no parent")?)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}
