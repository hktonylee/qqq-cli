use crate::images::{ImageStore, sniff_media_type};
use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
    pub action: String,
}

#[derive(Serialize)]
pub struct Report {
    pub ok: bool,
    pub database: String,
    pub schema_version: Option<i64>,
    pub tasks: Option<i64>,
    pub images: Option<i64>,
    pub issues: Vec<Diagnostic>,
}

impl Report {
    fn new(database: &Path) -> Self {
        Self {
            ok: true,
            database: database.display().to_string(),
            schema_version: None,
            tasks: None,
            images: None,
            issues: Vec::new(),
        }
    }
    fn issue(&mut self, code: &str, path: &Path, message: impl Into<String>, action: &str) {
        self.ok = false;
        self.issues.push(Diagnostic {
            code: code.to_owned(),
            path: path.display().to_string(),
            message: message.into(),
            action: action.to_owned(),
        });
    }
}

pub fn run() -> Result<Value> {
    let cwd = std::env::current_dir()?;
    let project = cwd
        .ancestors()
        .map(|ancestor| ancestor.join(".qqq"))
        .find(|candidate| candidate.is_dir());
    let directory = project.unwrap_or_else(|| cwd.join(".qqq"));
    let db_path = directory.join("qqq.db");
    let mut report = Report::new(&db_path);
    match fs::symlink_metadata(&db_path) {
        Ok(meta) if !meta.file_type().is_file() => {
            report.issue(
                "DB_UNSAFE",
                &db_path,
                "Database path is not a regular file",
                "Replace database from a verified backup.",
            );
            return Ok(serde_json::to_value(report)?);
        }
        Ok(_) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            report.issue("DB_MISSING", &db_path, "Project database is missing", "Run qqq restore in a new directory using a verified backup, or qqq init for a new project.");
            return Ok(serde_json::to_value(report)?);
        }
        Err(error) => {
            report.issue(
                "DB_UNREADABLE",
                &db_path,
                format!("Cannot inspect database: {error}"),
                "Fix path permissions or restore from a verified backup.",
            );
            return Ok(serde_json::to_value(report)?);
        }
    }
    check_sidecars(&db_path, &mut report);
    if !report.ok {
        return Ok(serde_json::to_value(report)?);
    }
    let conn = match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => conn,
        Err(error) => {
            report.issue(
                "DB_UNREADABLE",
                &db_path,
                format!("Cannot open database: {error}"),
                "Restore database from a verified backup.",
            );
            return Ok(serde_json::to_value(report)?);
        }
    };
    if let Err(error) = conn.pragma_update(None, "query_only", "ON") {
        report.issue(
            "DB_UNREADABLE",
            &db_path,
            format!("Cannot set read-only query mode: {error}"),
            "Restore database from a verified backup.",
        );
        return Ok(serde_json::to_value(report)?);
    }
    if let Err(error) = begin_snapshot(&conn) {
        report.issue(
            "DB_UNREADABLE",
            &db_path,
            format!("Cannot start read snapshot: {error}"),
            "Stop qqq writers, then rerun qqq doctor.",
        );
        return finish_report(report, &db_path);
    }
    let integrity: rusqlite::Result<String> =
        conn.pragma_query_value(None, "integrity_check", |row| row.get(0));
    match integrity {
        Ok(value) if value == "ok" => (),
        Ok(value) => report.issue(
            "DB_INTEGRITY",
            &db_path,
            value,
            "Restore database from a verified backup.",
        ),
        Err(error) => report.issue(
            "DB_INTEGRITY",
            &db_path,
            format!("Cannot check database integrity: {error}"),
            "Restore database from a verified backup.",
        ),
    }
    if !report.ok {
        return finish_report(report, &db_path);
    }
    match conn
        .prepare("PRAGMA foreign_key_check")
        .and_then(|mut statement| statement.exists([]))
    {
        Ok(true) => report.issue(
            "DB_FOREIGN_KEY",
            &db_path,
            "Database has invalid foreign keys",
            "Restore database from a verified backup.",
        ),
        Ok(false) => (),
        Err(error) => report.issue(
            "DB_FOREIGN_KEY",
            &db_path,
            format!("Cannot check foreign keys: {error}"),
            "Restore database from a verified backup.",
        ),
    }
    report.schema_version = match conn.pragma_query_value(None, "user_version", |row| row.get(0)) {
        Ok(version) => Some(version),
        Err(error) => {
            report.issue(
                "DB_SCHEMA",
                &db_path,
                format!("Cannot read database schema version: {error}"),
                "Restore database from a verified backup.",
            );
            return finish_report(report, &db_path);
        }
    };
    if report.schema_version != Some(crate::db::SCHEMA_VERSION) {
        report.issue(
            "DB_SCHEMA",
            &db_path,
            "Unsupported database schema version",
            "Use a compatible qqq version or restore a verified backup.",
        );
        return finish_report(report, &db_path);
    }
    if let Err(error) = crate::dependencies::validate_graph(&conn) {
        report.issue(
            "DB_DEPENDENCIES",
            &db_path,
            format!("Invalid dependency graph: {error}"),
            "Restore a verified backup or repair dependency edges manually.",
        );
    }
    match conn.query_row("SELECT count(*) FROM tasks", [], |row| row.get(0)) {
        Ok(count) => report.tasks = Some(count),
        Err(error) => report.issue(
            "DB_TASKS",
            &db_path,
            format!("Cannot read tasks: {error}"),
            "Restore database from a verified backup.",
        ),
    }
    match conn.query_row("SELECT count(*) FROM images", [], |row| row.get(0)) {
        Ok(count) => report.images = Some(count),
        Err(error) => report.issue(
            "DB_IMAGES",
            &db_path,
            format!("Cannot read image metadata: {error}"),
            "Restore database from a verified backup.",
        ),
    }
    if report.images.is_some() {
        check_images(
            &conn,
            db_path.parent().expect("database has parent"),
            &mut report,
        );
    }
    check_delete_staging(db_path.parent().expect("database has parent"), &mut report);
    finish_report(report, &db_path)
}

fn check_delete_staging(project: &Path, report: &mut Report) {
    let stage = project.join(".delete-staging");
    match fs::symlink_metadata(&stage) {
        Ok(metadata) if metadata.file_type().is_dir() => report.issue(
            "DELETE_RECOVERY_PENDING",
            &stage,
            "Interrupted task deletion has staged images",
            "Run qqq list to recover staged images, then rerun qqq doctor.",
        ),
        Ok(_) => report.issue(
            "DELETE_RECOVERY_PENDING",
            &stage,
            "Deletion staging path is unsafe",
            "Move unsafe path aside, then rerun qqq doctor.",
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => report.issue(
            "DELETE_RECOVERY_PENDING",
            &stage,
            format!("Cannot inspect deletion staging: {error}"),
            "Fix path permissions, then rerun qqq doctor.",
        ),
    }
}

fn begin_snapshot(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN")?;
    // First read pins SQLite snapshot until report is finalized. In rollback
    // journal mode, a concurrent writer cannot commit during image scan.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(())
}

fn check_sidecars(db_path: &Path, report: &mut Report) {
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", db_path.display()));
        match fs::symlink_metadata(&sidecar) {
            Ok(_) => report.issue(
                "DB_SIDECAR",
                &sidecar,
                "SQLite sidecar exists; read-only check deferred",
                "Stop qqq writers, checkpoint or recover SQLite, then rerun qqq doctor.",
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => report.issue(
                "DB_SIDECAR",
                &sidecar,
                format!("Cannot inspect SQLite sidecar: {error}"),
                "Fix path permissions, then rerun qqq doctor.",
            ),
        }
    }
}

fn finish_report(mut report: Report, db_path: &Path) -> Result<Value> {
    let mut concurrent = Report::new(db_path);
    check_sidecars(db_path, &mut concurrent);
    if !concurrent.ok {
        report = concurrent;
    }
    report
        .issues
        .sort_by(|left, right| (&left.path, &left.code).cmp(&(&right.path, &right.code)));
    report
        .issues
        .dedup_by(|left, right| left.path == right.path && left.code == right.code);
    Ok(serde_json::to_value(report)?)
}

fn check_images(conn: &Connection, project: &Path, report: &mut Report) {
    let root = project.join("images");
    let store = ImageStore::new(root.clone());
    let mut expected = HashSet::new();
    let mut statement =
        match conn.prepare("SELECT id,task_id,media_type,bytes FROM images ORDER BY id") {
            Ok(statement) => statement,
            Err(error) => {
                report.issue(
                    "DB_IMAGES",
                    project,
                    format!("Cannot read image metadata: {error}"),
                    "Restore database from a verified backup.",
                );
                return;
            }
        };
    let rows = match statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    }) {
        Ok(rows) => rows,
        Err(error) => {
            report.issue(
                "DB_IMAGES",
                project,
                format!("Cannot read image metadata: {error}"),
                "Restore database from a verified backup.",
            );
            return;
        }
    };
    for row in rows {
        match row {
            Ok((id, task_id, media_type, bytes)) => {
                let path = match store.path(task_id, id, &media_type) {
                    Ok(path) => path,
                    Err(error) => {
                        report.issue(
                            "IMAGE_METADATA",
                            &root,
                            format!("Image {id} has invalid metadata: {error}"),
                            "Restore database and images from a verified backup.",
                        );
                        continue;
                    }
                };
                expected.insert(path.clone());
                check_image(&path, &root, bytes, &media_type, report);
            }
            Err(error) => report.issue(
                "DB_IMAGES",
                project,
                format!("Cannot read image metadata: {error}"),
                "Restore database from a verified backup.",
            ),
        }
    }
    scan_images(&root, &expected, report);
}

fn real_directory(path: &Path, report: &mut Report) -> bool {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => true,
        Ok(_) => {
            report.issue(
                "IMAGE_UNSAFE",
                path,
                "Image directory is not a real directory",
                "Move unsafe path aside, then restore images from a verified backup.",
            );
            false
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            report.issue(
                "IMAGE_MISSING",
                path,
                "Image directory is missing",
                "Restore images from a verified backup.",
            );
            false
        }
        Err(error) => {
            report.issue(
                "IMAGE_UNREADABLE",
                path,
                format!("Cannot inspect image directory: {error}"),
                "Fix path permissions, then rerun qqq doctor.",
            );
            false
        }
    }
}

fn check_image(
    path: &Path,
    root: &Path,
    expected_bytes: i64,
    media_type: &str,
    report: &mut Report,
) {
    if !real_directory(root, report) {
        return;
    }
    if !real_directory(path.parent().expect("image path has parent"), report) {
        return;
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            report.issue(
                "IMAGE_MISSING",
                path,
                "Stored image is missing",
                "Restore image from a verified backup.",
            );
            return;
        }
        Err(error) => {
            report.issue(
                "IMAGE_UNREADABLE",
                path,
                format!("Cannot inspect stored image: {error}"),
                "Fix path permissions, then rerun qqq doctor.",
            );
            return;
        }
    };
    if !metadata.file_type().is_file() {
        report.issue(
            "IMAGE_UNSAFE",
            path,
            "Stored image is not a regular file",
            "Move unsafe path aside, then restore image from a verified backup.",
        );
        return;
    }
    if expected_bytes < 0 || metadata.len() != expected_bytes as u64 {
        report.issue(
            "IMAGE_SIZE",
            path,
            "Image byte count differs from database",
            "Restore database and image from the same verified backup.",
        );
        return;
    }
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            report.issue(
                "IMAGE_UNREADABLE",
                path,
                format!("Cannot read stored image: {error}"),
                "Fix path permissions or restore image from a verified backup.",
            );
            return;
        }
    };
    let mut header = Vec::with_capacity(12);
    match file.take(12).read_to_end(&mut header) {
        Ok(_) if sniff_media_type(&header).ok() == Some(media_type) => (),
        Ok(_) => report.issue(
            "IMAGE_SIGNATURE",
            path,
            "Image signature differs from database media type",
            "Restore database and image from the same verified backup.",
        ),
        Err(error) => report.issue(
            "IMAGE_UNREADABLE",
            path,
            format!("Cannot read image signature: {error}"),
            "Fix path permissions or restore image from a verified backup.",
        ),
    }
}

fn scan_images(root: &Path, expected: &HashSet<PathBuf>, report: &mut Report) {
    let expected_directories: HashSet<&Path> =
        expected.iter().filter_map(|path| path.parent()).collect();
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            report.issue(
                "IMAGE_UNREADABLE",
                root,
                format!("Cannot inspect image storage: {error}"),
                "Fix path permissions, then rerun qqq doctor.",
            );
            return;
        }
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        report.issue(
            "IMAGE_UNSAFE",
            root,
            "Image storage is not a real directory",
            "Move unsafe path aside, then restore images from a verified backup.",
        );
        return;
    }
    let mut stack = vec![(root.to_path_buf(), 0_u8)];
    while let Some((directory, depth)) = stack.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                report.issue(
                    "IMAGE_UNREADABLE",
                    &directory,
                    format!("Cannot list image directory: {error}"),
                    "Fix path permissions, then rerun qqq doctor.",
                );
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    report.issue(
                        "IMAGE_UNREADABLE",
                        &directory,
                        format!("Cannot list image entry: {error}"),
                        "Fix path permissions, then rerun qqq doctor.",
                    );
                    continue;
                }
            };
            let path = entry.path();
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() => report.issue(
                    "IMAGE_UNSAFE",
                    &path,
                    "Image path is a symlink",
                    "Move unsafe path aside, then rerun qqq doctor.",
                ),
                Ok(meta)
                    if meta.is_dir()
                        && depth < 1
                        && expected_directories.contains(path.as_path()) =>
                {
                    stack.push((path, depth + 1));
                }
                Ok(meta) if meta.is_dir() && depth < 1 => report.issue(
                    "IMAGE_ORPHAN",
                    &path,
                    "Image directory has no database rows",
                    "Inspect path, then move orphan data outside .qqq/images.",
                ),
                Ok(meta) if meta.is_dir() => report.issue(
                    "IMAGE_ORPHAN",
                    &path,
                    "Unexpected nested image directory",
                    "Inspect path, then move orphan data outside .qqq/images.",
                ),
                Ok(meta) if meta.is_file() && expected.contains(&path) => (),
                Ok(_) => report.issue(
                    "IMAGE_ORPHAN",
                    &path,
                    "Image path has no database row",
                    "Inspect path, then move orphan data outside .qqq/images.",
                ),
                Err(error) => report.issue(
                    "IMAGE_UNREADABLE",
                    &path,
                    format!("Cannot inspect image path: {error}"),
                    "Fix path permissions, then rerun qqq doctor.",
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_sidecar_supersedes_incomplete_image_diagnostics() {
        let directory = tempfile::TempDir::new().unwrap();
        let db_path = directory.path().join("qqq.db");
        let writer = Connection::open(&db_path).unwrap();
        writer.busy_timeout(std::time::Duration::ZERO).unwrap();
        writer
            .execute_batch("CREATE TABLE sample (id INTEGER)")
            .unwrap();
        let reader =
            Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        begin_snapshot(&reader).unwrap();
        writer
            .execute_batch("BEGIN IMMEDIATE; INSERT INTO sample VALUES (1)")
            .unwrap();
        assert!(writer.execute_batch("COMMIT").is_err());
        let mut report = Report::new(&db_path);
        report.issue(
            "IMAGE_ORPHAN",
            &directory.path().join("images/1/1.png"),
            "Image path has no database row",
            "Inspect path.",
        );
        assert!(directory.path().join("qqq.db-journal").exists());
        let output = finish_report(report, &db_path).unwrap();
        assert_eq!(output["ok"], false);
        assert_eq!(output["issues"].as_array().unwrap().len(), 1);
        assert_eq!(output["issues"][0]["code"], "DB_SIDECAR");
        writer.execute_batch("ROLLBACK").unwrap();
    }
}
