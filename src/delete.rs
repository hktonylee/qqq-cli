use crate::{
    db::{Db, Task, task_row},
    images::ImageStore,
};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

const STAGING_DIR: &str = ".delete-staging";
const TASK_COLUMNS: &str = "id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session,priority,archived,content_revision";

#[derive(Serialize)]
pub struct DeleteReport {
    pub deleted: bool,
    pub task: Task,
    pub messages: i64,
    pub events: i64,
    pub herdr_links: i64,
    pub images: i64,
    pub image_paths: Vec<String>,
    pub backup_recommended: bool,
}

struct Plan {
    report: DeleteReport,
    paths: Vec<PathBuf>,
}

struct Stage {
    original: PathBuf,
    wrapper: PathBuf,
    staging_root: PathBuf,
    images_root: PathBuf,
    project: PathBuf,
}

pub fn preview_cli(id: i64) -> Result<DeleteReport> {
    ensure!(id > 0, "Deletion requires positive task ID");
    let cwd = std::env::current_dir()?;
    let project = cwd
        .ancestors()
        .map(|ancestor| ancestor.join(".qqq"))
        .find(|candidate| candidate.is_dir())
        .context("No .qqq directory found; run qqq init in project root")?;
    ensure!(real_directory(&project)?, "Project directory is missing");
    let staging_root = project.join(STAGING_DIR);
    ensure!(
        fs::symlink_metadata(&staging_root).is_err_and(|error| error.kind() == ErrorKind::NotFound),
        "Deletion recovery pending; run qqq list before preview"
    );
    let db_path = project.join("qqq.db");
    ensure!(
        fs::symlink_metadata(&db_path)?.file_type().is_file(),
        "Database path is not a regular file"
    );
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = db_path.as_os_str().to_os_string();
        sidecar.push(suffix);
        ensure!(
            fs::symlink_metadata(&sidecar).is_err_and(|error| error.kind() == ErrorKind::NotFound),
            "SQLite sidecar exists or cannot be inspected; stop writers before preview"
        );
    }
    let mut header = [0_u8; 20];
    fs::File::open(&db_path)?.read_exact(&mut header)?;
    ensure!(
        header[18] != 2 && header[19] != 2,
        "SQLite WAL database needs checkpoint before read-only preview"
    );
    let conn = Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.pragma_update(None, "query_only", "ON")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        version == 10,
        "Database schema version {version} needs migration; run qqq list before preview"
    );
    Ok(plan(&conn, &db_path, id)?.report)
}

pub fn run(db: &mut Db, db_path: &Path, id: i64) -> Result<DeleteReport> {
    ensure!(id > 0, "Deletion requires positive task ID");
    let tx = db
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut plan = plan(&tx, db_path, id)?;
    let stage = stage_images(db_path, id, &plan.paths)?;
    if let Err(error) = delete_rows(&tx, id) {
        return abort(tx, stage.as_ref(), error);
    }
    if let Err(error) = tx.execute_batch("COMMIT") {
        if tx.is_autocommit() {
            drop(tx);
            if stage.is_some() {
                recover(&mut db.conn, db_path)?;
            }
            bail!("Task {id} may have been deleted: commit returned {error}");
        }
        return abort(tx, stage.as_ref(), error.into());
    }
    drop(tx);
    if stage.is_some() {
        recover(&mut db.conn, db_path)
            .with_context(|| format!("Task {id} deleted; staged image cleanup pending"))?;
    }
    plan.report.deleted = true;
    Ok(plan.report)
}

fn plan(conn: &Connection, db_path: &Path, id: i64) -> Result<Plan> {
    let task = conn
        .query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
            [id],
            task_row,
        )
        .optional()?
        .with_context(|| format!("Task {id} not found"))?;
    ensure!(
        task.status != "in_progress",
        "Task {id} is in progress; release or complete it before archiving and deletion"
    );
    ensure!(
        task.archived,
        "Task {id} is not archived; archive it before deletion"
    );
    let child = conn
        .query_row(
            "SELECT id FROM tasks WHERE parent_id=? ORDER BY id LIMIT 1",
            [id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    ensure!(
        child.is_none(),
        "Task {id} has dependent child #{}; delete or reparent child first",
        child.unwrap_or_default()
    );
    let project = db_path
        .parent()
        .context("Database has no parent directory")?;
    let store = ImageStore::new(project.join("images"));
    let paths = conn
        .prepare("SELECT id,media_type FROM images WHERE task_id=? ORDER BY id")?
        .query_map([id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .map(|row| {
            let (image_id, media_type) = row?;
            store.path(id, image_id, &media_type)
        })
        .collect::<Result<Vec<_>>>()?;
    let report = DeleteReport {
        deleted: false,
        task,
        messages: count(conn, "messages", id)?,
        events: count(conn, "events", id)?,
        herdr_links: count(conn, "herdr_links", id)?,
        images: paths.len() as i64,
        image_paths: paths
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        backup_recommended: true,
    };
    Ok(Plan { report, paths })
}

fn count(conn: &Connection, table: &str, id: i64) -> Result<i64> {
    let sql = match table {
        "messages" => "SELECT count(*) FROM messages WHERE task_id=?",
        "events" => "SELECT count(*) FROM events WHERE task_id=?",
        "herdr_links" => "SELECT count(*) FROM herdr_links WHERE task_id=?",
        _ => unreachable!("static table name"),
    };
    Ok(conn.query_row(sql, [id], |row| row.get(0))?)
}

fn delete_rows(tx: &Transaction<'_>, id: i64) -> Result<()> {
    for sql in [
        "DELETE FROM messages WHERE task_id=?",
        "DELETE FROM events WHERE task_id=?",
        "DELETE FROM herdr_links WHERE task_id=?",
        "DELETE FROM images WHERE task_id=?",
    ] {
        tx.execute(sql, [id])?;
    }
    ensure!(
        tx.execute("DELETE FROM tasks WHERE id=?", [id])? == 1,
        "Task {id} disappeared during deletion"
    );
    Ok(())
}

fn real_directory(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.file_type().is_dir(),
                "Unsafe directory {}",
                path.display()
            );
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("Cannot inspect {}", path.display())),
    }
}

fn stage_images(db_path: &Path, id: i64, expected: &[PathBuf]) -> Result<Option<Stage>> {
    let project = db_path
        .parent()
        .context("Database has no parent directory")?;
    let images_root = project.join("images");
    let original = images_root.join(id.to_string());
    if !real_directory(&images_root)? || !real_directory(&original)? {
        ensure!(
            expected.is_empty(),
            "Stored image directory missing for task {id}; restore images before deletion"
        );
        return Ok(None);
    }
    let expected: HashSet<&Path> = expected.iter().map(PathBuf::as_path).collect();
    let mut seen = HashSet::new();
    for entry in fs::read_dir(&original)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.file_type().is_file() && expected.contains(path.as_path()),
            "Unexpected or unsafe image path {}; inspect before deletion",
            path.display()
        );
        seen.insert(path);
    }
    for path in expected {
        ensure!(
            seen.contains(path),
            "Stored image {} is missing; restore image before deletion",
            path.display()
        );
    }
    let staging_root = project.join(STAGING_DIR);
    match fs::create_dir(&staging_root) {
        Ok(()) => sync_dir(project)?,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            ensure!(real_directory(&staging_root)?, "Staging root is missing");
        }
        Err(error) => return Err(error).context("Cannot create deletion staging directory"),
    }
    let wrapper = tempfile::Builder::new()
        .prefix(&format!("{id}-"))
        .tempdir_in(&staging_root)?;
    let marker = wrapper.path().join("task-id");
    fs::write(&marker, id.to_string())?;
    fs::File::open(&marker)?.sync_all()?;
    sync_dir(wrapper.path())?;
    sync_dir(&staging_root)?;
    let wrapper = wrapper.keep();
    if let Err(error) = fs::rename(&original, wrapper.join("images")) {
        let _ = fs::remove_dir_all(&wrapper);
        return Err(error).with_context(|| format!("Cannot stage images for task {id}"));
    }
    let stage = Stage {
        original,
        wrapper,
        staging_root,
        images_root,
        project: project.to_path_buf(),
    };
    if let Err(error) = sync_dir(&stage.images_root).and_then(|()| sync_dir(&stage.wrapper)) {
        restore_stage(&stage)?;
        return Err(error).context("Cannot sync staged images");
    }
    Ok(Some(stage))
}

fn abort(tx: Transaction<'_>, stage: Option<&Stage>, cause: anyhow::Error) -> Result<DeleteReport> {
    let restore_error = stage.and_then(|stage| restore_stage(stage).err());
    let rollback_error = tx.rollback().err();
    if let Some(error) = restore_error {
        bail!(
            "Deletion failed: {cause:#}; image restore failed: {error:#}. Run qqq again to recover staging"
        );
    }
    if let Some(error) = rollback_error {
        bail!("Deletion failed: {cause:#}; database rollback failed: {error}");
    }
    Err(cause)
}

fn restore_stage(stage: &Stage) -> Result<()> {
    fs::rename(stage.wrapper.join("images"), &stage.original)
        .with_context(|| format!("Cannot restore {}", stage.original.display()))?;
    sync_dir(&stage.images_root)?;
    sync_dir(&stage.wrapper)?;
    cleanup_stage(stage)
}

fn cleanup_stage(stage: &Stage) -> Result<()> {
    fs::remove_dir_all(&stage.wrapper)?;
    sync_dir(&stage.staging_root)?;
    if fs::read_dir(&stage.staging_root)?.next().is_none() {
        fs::remove_dir(&stage.staging_root)?;
        sync_dir(&stage.project)?;
    }
    Ok(())
}

pub fn recover(conn: &mut Connection, db_path: &Path) -> Result<()> {
    recover_with_prelock(conn, db_path, || {})
}

fn recover_with_prelock(
    conn: &mut Connection,
    db_path: &Path,
    after_precheck: impl FnOnce(),
) -> Result<()> {
    let project = db_path
        .parent()
        .context("Database has no parent directory")?;
    let staging_root = project.join(STAGING_DIR);
    if !real_directory(&staging_root)? {
        return Ok(());
    }
    after_precheck();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Another CLI may have completed cleanup while this connection waited.
    if !real_directory(&staging_root)? {
        tx.commit()?;
        return Ok(());
    }
    for entry in fs::read_dir(&staging_root)? {
        let wrapper = entry?.path();
        ensure!(real_directory(&wrapper)?, "Unsafe deletion stage");
        let staged_images = wrapper.join("images");
        if !real_directory(&staged_images)? {
            ensure!(
                fs::read_dir(&wrapper)?.all(|entry| entry
                    .as_ref()
                    .is_ok_and(|entry| entry.file_name() == "task-id")),
                "Unexpected deletion stage contents in {}",
                wrapper.display()
            );
            fs::remove_dir_all(&wrapper)?;
            sync_dir(&staging_root)?;
            continue;
        }
        let name = wrapper
            .file_name()
            .and_then(|name| name.to_str())
            .context("Invalid deletion stage name")?;
        let id = name
            .split_once('-')
            .and_then(|(id, _)| id.parse::<i64>().ok())
            .filter(|id| *id > 0)
            .context("Invalid deletion stage task ID")?;
        let marker = wrapper.join("task-id");
        let marker_meta = fs::symlink_metadata(&marker)
            .with_context(|| format!("Missing deletion stage marker in {}", wrapper.display()))?;
        ensure!(
            marker_meta.file_type().is_file(),
            "Unsafe deletion stage marker"
        );
        ensure!(
            fs::read_to_string(&marker)? == id.to_string(),
            "Deletion stage marker mismatch in {}",
            wrapper.display()
        );
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?)",
            [id],
            |row| row.get(0),
        )?;
        if exists {
            let images_root = project.join("images");
            ensure!(
                real_directory(&images_root)?,
                "Image root missing during deletion recovery"
            );
            let original = images_root.join(id.to_string());
            ensure!(
                fs::symlink_metadata(&original)
                    .is_err_and(|error| error.kind() == ErrorKind::NotFound),
                "Image directory conflicts with deletion recovery: {}",
                original.display()
            );
            fs::rename(&staged_images, &original)?;
            sync_dir(&images_root)?;
            sync_dir(&wrapper)?;
        } else {
            fs::remove_dir_all(&staged_images)?;
            sync_dir(&wrapper)?;
        }
        fs::remove_file(&marker)?;
        fs::remove_dir(&wrapper)?;
        sync_dir(&staging_root)?;
    }
    fs::remove_dir(&staging_root)?;
    sync_dir(project)?;
    tx.commit()?;
    Ok(())
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_tolerates_stage_removed_after_precheck() {
        let project = tempfile::TempDir::new().unwrap();
        let db_path = project.path().join("qqq.db");
        let staging_root = project.path().join(STAGING_DIR);
        fs::create_dir(&staging_root).unwrap();
        let mut conn = Connection::open(&db_path).unwrap();
        recover_with_prelock(&mut conn, &db_path, || {
            fs::remove_dir(&staging_root).unwrap();
        })
        .unwrap();
        assert!(!staging_root.exists());
    }
}
