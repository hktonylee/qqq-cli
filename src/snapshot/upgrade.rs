use super::{
    backup::{append_bytes, copy_database, sync_directory, validate_database_for},
    format::{DATABASE_NAME, ImageMeta, MANIFEST_NAME, Manifest, Upgrade, hash_reader},
    restore::verify_archive,
};
use crate::{db::SCHEMA_VERSION, images::ImageStore};
use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};
use tar::Builder;
use tempfile::{Builder as TempBuilder, TempDir};

/// Caller holds the same SQLite write transaction until upgrade commit/rollback.
/// A separate read-only SQLite connection sees the original committed state,
/// while that write lock excludes qqq attachment writers and deletion staging.
pub(crate) fn capture(conn: &Connection, db_path: &Path, version: i64) -> Result<PathBuf> {
    ensure!(
        version > 0 && version < SCHEMA_VERSION,
        "Invalid upgrade source schema"
    );
    let store = db_path
        .parent()
        .context("Database has no parent")?
        .canonicalize()?;
    let project = store.parent().context("Project store has no parent")?;
    let directory = project.join(".qqq-upgrades");
    match fs::create_dir(&directory) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error).context("Cannot create upgrade snapshot directory"),
    }
    real_directory(&directory)?;
    sync_directory(project)?;

    let stage = TempDir::new_in(&directory)?;
    let database_path = stage.path().join(DATABASE_NAME);
    copy_database(db_path, &database_path)
        .context("Cannot copy original database for upgrade snapshot")?;
    validate_database_for(&database_path, true)?;
    let images = if version >= 6 {
        copy_images(conn, &store, stage.path())?
    } else {
        Vec::new()
    };
    let manifest = Manifest {
        version: 2,
        database: hash_reader(fs::File::open(&database_path)?)?,
        images,
        upgrade: Some(Upgrade {
            project: project
                .to_str()
                .context("Project path is not UTF-8")?
                .to_owned(),
            source_schema: version,
            target_schema: SCHEMA_VERSION,
            created_at: conn.query_row(
                "SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                [],
                |row| row.get(0),
            )?,
        }),
    };
    manifest.validate()?;
    let mut temporary = TempBuilder::new()
        .prefix(".upgrade-")
        .tempfile_in(&directory)?;
    {
        let mut archive = Builder::new(temporary.as_file_mut());
        append_bytes(&mut archive, MANIFEST_NAME, &serde_json::to_vec(&manifest)?)?;
        archive.append_path_with_name(&database_path, DATABASE_NAME)?;
        for image in &manifest.images {
            archive.append_path_with_name(stage.path().join(&image.path), &image.path)?;
        }
        archive.finish()?;
    }
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    let verification = TempDir::new_in(&directory)?;
    verify_archive(temporary.path(), verification.path())
        .context("Upgrade snapshot verification failed")?;
    // Tempfile's random name plus atomic no-clobber publication preserves every
    // retry's recovery record. Migration contenders seeing current schema skip it.
    let random = temporary
        .path()
        .file_name()
        .context("Snapshot has no filename")?
        .to_string_lossy();
    let destination = directory.join(format!("schema-{version}-to-{SCHEMA_VERSION}-{random}.tar"));
    temporary
        .persist_noclobber(&destination)
        .map_err(|error| error.error)
        .context("Cannot publish upgrade snapshot")?;
    sync_directory(&directory)
        .context("Cannot sync upgrade snapshot directory; upgrade aborted")?;
    Ok(destination)
}

fn real_directory(path: &Path) -> Result<()> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_dir(),
        "Upgrade snapshot directory must be a real directory: {}",
        path.display()
    );
    Ok(())
}

fn copy_images(conn: &Connection, store: &Path, stage: &Path) -> Result<Vec<ImageMeta>> {
    let images = ImageStore::new(store.join("images"));
    let mut statement =
        conn.prepare("SELECT id,task_id,media_type,bytes FROM images ORDER BY id")?;
    let mut result = Vec::new();
    for row in statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })? {
        let (id, task, media, bytes) = row?;
        let path = images.path(task, id, &media)?;
        let data = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                staged_image(store, task, &path, bytes)?
            }
            _ => images.read(task, id, &media, bytes)?,
        };
        let relative = path
            .strip_prefix(store)?
            .to_str()
            .context("Image path is not UTF-8")?
            .to_owned();
        let output = stage.join(&relative);
        fs::create_dir_all(output.parent().context("Image has no parent")?)?;
        fs::write(&output, &data)?;
        let meta = hash_reader(Cursor::new(&data))?;
        result.push(ImageMeta {
            path: relative,
            bytes: meta.bytes,
            sha256: meta.sha256,
        });
    }
    Ok(result)
}

// Interrupted deletion may have moved still-referenced attachments. Read them
// under the migration lock without recovering/removing original staging first.
fn staged_image(store: &Path, task: i64, live: &Path, bytes: i64) -> Result<Vec<u8>> {
    let root = store.join(".delete-staging");
    if !root.exists() {
        anyhow::bail!("Cannot read stored image {}", live.display());
    }
    real_directory(&root)?;
    let mut found = None;
    for entry in fs::read_dir(&root)? {
        let wrapper = entry?.path();
        real_directory(&wrapper)?;
        let name = wrapper
            .file_name()
            .and_then(|name| name.to_str())
            .context("Invalid deletion stage name")?;
        if !name
            .split_once('-')
            .is_some_and(|(id, _)| id == task.to_string())
        {
            continue;
        }
        let marker = wrapper.join("task-id");
        ensure!(
            fs::symlink_metadata(&marker)?.file_type().is_file(),
            "Unsafe deletion stage marker"
        );
        ensure!(
            fs::read_to_string(&marker)? == task.to_string(),
            "Deletion stage marker mismatch"
        );
        let images = wrapper.join("images");
        real_directory(&images)?;
        let path = images.join(live.file_name().context("Image has no filename")?);
        ensure!(found.is_none(), "Multiple deletion stages for task {task}");
        ensure!(
            fs::symlink_metadata(&path)?.file_type().is_file(),
            "Stored image is not a regular file"
        );
        let data = fs::read(&path)?;
        ensure!(
            bytes >= 0 && data.len() as i64 == bytes,
            "Stored image byte count differs from database"
        );
        found = Some(data);
    }
    found.with_context(|| format!("Cannot read stored image {}", live.display()))
}
