use super::format::{DATABASE_NAME, FileMeta, ImageMeta, MANIFEST_NAME, Manifest, hash_reader};
use crate::{db::Db, images::ImageStore};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde_json::{Value, json};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};
use tar::{Builder, Header};
use tempfile::{NamedTempFile, TempDir};

pub fn run(db: &mut Db, db_path: &Path, destination: &Path) -> Result<Value> {
    let destination = absolute(destination)?;
    ensure_absent(&destination)?;
    let parent = destination
        .parent()
        .context("Backup destination has no parent")?;
    let parent_metadata = fs::symlink_metadata(parent)?;
    ensure!(
        parent_metadata.is_dir() && !parent_metadata.file_type().is_symlink(),
        "Backup destination parent must be a real directory"
    );
    let parent = parent.canonicalize()?;
    let project_store = db_path
        .parent()
        .context("Database has no parent")?
        .canonicalize()?;
    ensure!(
        !parent.starts_with(&project_store),
        "Backup destination must be outside .qqq"
    );
    let destination = parent.join(
        destination
            .file_name()
            .context("Backup destination has no filename")?,
    );
    ensure_absent(&destination)?;

    let stage = TempDir::new_in(&parent)?;
    let staged_db = stage.path().join(DATABASE_NAME);
    let staged_images = stage.path().join("images");
    let images = {
        let tx = db
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        copy_database(db_path, &staged_db)?;
        let images = copy_images(&tx, &db.image_store, &staged_images)?;
        tx.commit()?;
        images
    };
    let tasks = validate_database(&staged_db)?;
    let database = hash_reader(fs::File::open(&staged_db)?)?;
    let manifest = Manifest {
        version: 1,
        database,
        images,
    };
    manifest.validate()?;
    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let mut temp = NamedTempFile::new_in(&parent)?;
    {
        let mut archive = Builder::new(temp.as_file_mut());
        append_bytes(&mut archive, MANIFEST_NAME, &manifest_bytes)?;
        archive.append_path_with_name(&staged_db, DATABASE_NAME)?;
        for image in &manifest.images {
            archive.append_path_with_name(stage.path().join(&image.path), &image.path)?;
        }
        archive.finish()?;
    }
    temp.as_file().sync_all()?;
    let bytes = temp.as_file().metadata()?.len();
    temp.persist_noclobber(&destination)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "Backup destination already exists or cannot be created: {}",
                destination.display()
            )
        })?;
    if let Err(error) = sync_directory(&parent) {
        if let Err(cleanup) = fs::remove_file(&destination) {
            anyhow::bail!(
                "Backup directory sync failed: {error}; cannot remove {}: {cleanup}",
                destination.display()
            );
        }
        return Err(error).context("Backup directory sync failed; destination removed");
    }
    Ok(json!({
        "destination": destination,
        "tasks": tasks,
        "images": manifest.images.len(),
        "bytes": bytes,
    }))
}

fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn ensure_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => anyhow::bail!("Backup destination already exists: {}", path.display()),
    }
}

fn copy_database(source: &Path, destination: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    conn.execute(
        "VACUUM INTO ?1",
        params![destination.to_str().context("Snapshot path is not UTF-8")?],
    )?;
    Ok(())
}

fn copy_images(conn: &Connection, store: &ImageStore, stage: &Path) -> Result<Vec<ImageMeta>> {
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
    let mut images = Vec::new();
    for row in rows {
        let (id, task_id, media_type, expected_bytes) = row?;
        let source_path = store.path(task_id, id, &media_type)?;
        let filename = source_path
            .file_name()
            .context("Image path has no filename")?;
        let relative = format!(
            "images/{task_id}/{}",
            filename.to_str().context("Image filename is not UTF-8")?
        );
        let data = store.read(task_id, id, &media_type, expected_bytes)?;
        let output = stage.join(task_id.to_string()).join(filename);
        fs::create_dir_all(output.parent().context("Image path has no parent")?)?;
        fs::write(&output, &data)?;
        let FileMeta { bytes, sha256 } = hash_reader(fs::File::open(&output)?)?;
        images.push(ImageMeta {
            path: relative,
            bytes,
            sha256,
        });
    }
    Ok(images)
}

pub(crate) fn validate_database(path: &Path) -> Result<i64> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = conn.pragma_query_value(None, "integrity_check", |row| row.get(0))?;
    ensure!(
        integrity == "ok",
        "Snapshot database integrity check failed: {integrity}"
    );
    ensure!(
        !conn.prepare("PRAGMA foreign_key_check")?.exists([])?,
        "Snapshot database has invalid foreign keys"
    );
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        version == 9,
        "Unsupported snapshot database schema version {version}"
    );
    Ok(conn.query_row("SELECT count(*) FROM tasks", [], |row| row.get(0))?)
}

fn append_bytes(archive: &mut Builder<&mut fs::File>, name: &str, data: &[u8]) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    archive.append_data(&mut header, name, Cursor::new(data))?;
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
