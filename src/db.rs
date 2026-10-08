use crate::images::{ImageInput, ImageReference, ImageStore, PendingFiles};
use crate::sql_filter::CompiledFilter;
use anyhow::{Context, Result, ensure};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
    params_from_iter,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::Read,
    ops::Range,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) fn filter_evaluation_error() -> crate::errors::Info {
    crate::errors::Info::new(
        crate::errors::Code::InvalidFilter,
        "Invalid --filter evaluation",
    )
    .detail("argument", "--filter")
    .detail("reason", "evaluation_failed")
}

pub const DB_NAME: &str = "qqq.db";
const PROJECT_DIR_NAME: &str = ".qqq";
pub(crate) const SCHEMA_VERSION: i64 = 12;
pub(crate) const READY_TASK_PREDICATE: &str = "tasks.status='new' AND tasks.archived=0 AND
    (tasks.parent_id IS NULL OR EXISTS
    (SELECT 1 FROM tasks parent WHERE parent.id=tasks.parent_id AND parent.status='completed')) AND NOT EXISTS (
    SELECT 1 FROM task_dependencies dependency LEFT JOIN tasks prerequisite ON prerequisite.id=dependency.prerequisite_id
    WHERE dependency.task_id=tasks.id AND (prerequisite.id IS NULL OR prerequisite.status!='completed'))";
pub(crate) const TASK_COLUMNS: &str = "id,description,status,claim_key,created_at,updated_at,
    parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session,priority,archived,content_revision,
    (SELECT json_group_array(json_object('id',prerequisite_id,'status',status,
        'archived',json(CASE WHEN archived IS NULL THEN NULL WHEN archived THEN 'true' ELSE 'false' END)))
     FROM (SELECT dependency.prerequisite_id, prerequisite.status, prerequisite.archived
           FROM task_dependencies dependency LEFT JOIN tasks prerequisite ON prerequisite.id=dependency.prerequisite_id
           WHERE dependency.task_id=tasks.id ORDER BY dependency.prerequisite_id)),tags";

pub struct Db {
    pub conn: Connection,
    pub image_store: ImageStore,
}
#[derive(Clone, Copy)]
pub enum EditTransition<'a> {
    New {
        session: &'a str,
        harness_name: Option<&'a str>,
    },
    NewExact(&'a str),
    ForceNew(&'a str),
    RetryError(&'a str),
    Error {
        session: &'a str,
        reason: &'a str,
        harness_name: Option<&'a str>,
    },
}
#[derive(Default)]
pub struct AddOptions<'a> {
    pub parent: Option<i64>,
    pub priority: i64,
    pub prerequisites: &'a [i64],
    pub tags: &'a [String],
}

#[derive(Default)]
pub struct EditOptions<'a> {
    pub transition: Option<EditTransition<'a>>,
    pub parent: Option<ParentChange>,
    pub priority: Option<i64>,
    pub expected_revision: Option<i64>,
    pub dependencies: crate::dependencies::Changes<'a>,
    pub tags: Option<&'a [String]>,
}

#[derive(Debug)]
pub struct CurrentContent {
    pub description: String,
    pub revision: i64,
}

#[derive(Debug)]
pub struct ContentConflict {
    pub task_id: i64,
    pub expected_revision: i64,
    pub current: Option<CurrentContent>,
}

impl std::fmt::Display for ContentConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.current {
            Some(current) => write!(
                f,
                "Content conflict for task {}: expected revision {}, current revision {}; save rejected",
                self.task_id, self.expected_revision, current.revision
            ),
            None => write!(
                f,
                "Content conflict for task {}: task was removed; save rejected",
                self.task_id
            ),
        }
    }
}

impl std::error::Error for ContentConflict {}

pub struct ContentSnapshot {
    pub task: Task,
    pub references: Vec<ImageReference>,
}

#[derive(Clone, Copy, Debug)]
pub enum ParentChange {
    Set(i64),
    Clear,
}

impl std::str::FromStr for ParentChange {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        if value == "none" {
            return Ok(Self::Clear);
        }
        value
            .parse::<i64>()
            .ok()
            .filter(|id| *id > 0)
            .map(Self::Set)
            .ok_or_else(|| "Parent must be a positive task ID or none".to_owned())
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Prerequisite {
    pub id: i64,
    pub status: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Serialize)]
pub struct Task {
    pub id: i64,
    pub description: String,
    pub content_revision: i64,
    pub status: String,
    pub priority: i64,
    pub archived: bool,
    #[serde(flatten)]
    pub identity: crate::identity::Identity,
    pub created_at: String,
    pub updated_at: String,
    pub parent_id: Option<i64>,
    pub prerequisites: Vec<Prerequisite>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub context_only: bool,
}
fn is_false(value: &bool) -> bool {
    !*value
}
#[derive(Serialize)]
pub struct TaskMessage {
    pub id: i64,
    pub body: String,
    pub session: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct StoredLink {
    #[serde(flatten)]
    pub(crate) link: crate::herdr::Link,
    #[serde(default)]
    pub(crate) claim_key: Option<String>,
}
pub(crate) fn task_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        description: r.get(1)?,
        content_revision: r.get(13)?,
        status: r.get(2)?,
        priority: r.get(11)?,
        archived: r.get(12)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        parent_id: r.get(6)?,
        prerequisites: serde_json::from_str(&r.get::<_, String>(14)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                14,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        tags: serde_json::from_str(&r.get::<_, String>(15)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                15,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        context_only: false,
        identity: crate::identity::Identity {
            harness_name: r.get(7)?,
            harness_session: r.get(8)?,
            orchestrator_name: r.get(9)?,
            orchestrator_session: r.get(10)?,
        },
    })
}
pub fn nonempty(value: &str, name: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty(),
        crate::errors::Info::invalid_argument(name, format!("{name} must not be empty"))
    );
    Ok(())
}
pub(crate) fn validate_description(description: &str) -> Result<()> {
    nonempty(description, "Description")?;
    // SQLite length() stops at NUL; mirror CHECK(length(trim(description))>0).
    ensure!(
        !description
            .split('\0')
            .next()
            .unwrap_or("")
            .trim_matches(' ')
            .is_empty(),
        crate::errors::Info::invalid_argument(
            "description",
            "Description fails database blank-text validation"
        )
    );
    Ok(())
}
pub fn validate_priority(priority: i64) -> Result<()> {
    ensure!(
        (-100..=100).contains(&priority),
        crate::errors::Info::invalid_argument(
            "--priority",
            "Priority must be between -100 and 100"
        )
    );
    Ok(())
}
pub fn transition_error(
    conn: &Connection,
    id: i64,
    expected: &[&str],
    session: Option<&str>,
    human: String,
) -> Result<crate::errors::Info> {
    let state: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT status,claim_key FROM tasks WHERE id=?",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((status, key)) = state else {
        return Ok(crate::errors::Info::missing_task(id));
    };
    if expected.contains(&status.as_str())
        && session.is_some_and(|session| key.as_deref() != Some(session))
    {
        return Ok(crate::errors::Info::new(
            crate::errors::Code::OwnershipMismatch,
            format!("Task {id} is not claimed by session used for this command"),
        )
        .detail("task_id", id)
        .detail("actual_status", status)
        .detail("expected_statuses", expected.to_vec())
        .human(human));
    }
    let message = if session.is_none() {
        human.clone()
    } else {
        format!(
            "Task {id} is not claimed by session used for this command; current status {status}"
        )
    };
    Ok(crate::errors::Info::transition(id, &status, expected, message).human(human))
}
pub(crate) fn ensure_parent_available(
    conn: &Connection,
    parent_id: i64,
    unfinished: bool,
) -> Result<()> {
    let (status, archived): (String, bool) = conn
        .query_row(
            "SELECT status,archived FROM tasks WHERE id=?",
            [parent_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .with_context(|| crate::errors::Info::missing_task(parent_id))?;
    ensure!(
        !unfinished || !archived || status == "completed",
        crate::errors::Info::transition(
            parent_id,
            &status,
            &["completed"],
            format!("Task {parent_id} is archived unfinished parent")
        )
        .detail("actual_archived", true)
    );
    Ok(())
}

fn image_description(
    source: &str,
    spans: &[Range<usize>],
    images: &[ImageInput],
    image_ids: &[i64],
    task_id: i64,
) -> Result<String> {
    ensure!(
        spans.len() <= images.len() && image_ids.len() == images.len(),
        "Invalid image reference count"
    );
    let mut description = String::new();
    let mut cursor = 0;
    for (index, span) in spans.iter().enumerate() {
        ensure!(
            span.start >= cursor && span.end > span.start,
            "Invalid image reference span"
        );
        let prefix = source
            .get(cursor..span.start)
            .context("Invalid image reference span")?;
        let marker = source
            .get(span.clone())
            .context("Invalid image reference span")?;
        ensure!(
            marker == format!("[Image: {}]", images[index].name),
            "Invalid image reference marker"
        );
        description.push_str(prefix);
        description.push_str(
            &ImageReference {
                id: image_ids[index],
                name: images[index].name.clone(),
                media_type: images[index].media_type()?.to_owned(),
            }
            .markdown(task_id)?,
        );
        cursor = span.end;
    }
    description.push_str(
        source
            .get(cursor..)
            .context("Invalid image reference span")?,
    );
    Ok(description)
}
pub(crate) fn ensure_description_schema(conn: &Connection) -> Result<()> {
    ensure!(
        !conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name='title')",
            [],
            |r| r.get::<_, bool>(0)
        )?,
        crate::errors::Info::new(
            crate::errors::Code::DatabaseError,
            "Database contains legacy title column; update SQLite manually before using qqq"
        )
        .detail("reason", "legacy_title_column")
    );
    Ok(())
}

fn commit_with_files(tx: Transaction<'_>, pending: &mut PendingFiles) -> Result<()> {
    match tx.execute_batch("COMMIT") {
        Ok(()) => {
            pending.keep();
            Ok(())
        }
        Err(error) => {
            // A failed COMMIT can leave the transaction open (for example,
            // SQLITE_BUSY). Remove files while its write lock still excludes
            // another writer from reusing rolled-back image IDs.
            if tx.is_autocommit() {
                pending.keep();
                return Err(error.into());
            }
            let cleanup_error = pending.discard().err();
            let rollback_error = tx.rollback().err();
            if let Some(cleanup_error) = cleanup_error {
                return Err(anyhow::Error::new(error).context(format!(
                    "Commit failed; image cleanup failed: {cleanup_error:#}"
                )));
            }
            if let Some(rollback_error) = rollback_error {
                return Err(anyhow::Error::new(error)
                    .context(format!("Commit failed; rollback failed: {rollback_error}")));
            }
            Err(error.into())
        }
    }
}
fn database_path(init: bool) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let path = if init {
        let directory = cwd.join(PROJECT_DIR_NAME);
        std::fs::create_dir_all(&directory)?;
        directory.join(DB_NAME)
    } else {
        let directory = cwd
            .ancestors()
            .map(|parent| parent.join(PROJECT_DIR_NAME))
            .find(|candidate| candidate.is_dir())
            .context(crate::errors::Info::new(
                crate::errors::Code::ProjectNotFound,
                "No .qqq directory found; run qqq init in project root",
            ))?;
        let path = directory.join(DB_NAME);
        ensure!(
            path.is_file(),
            crate::errors::Info::new(
                crate::errors::Code::ProjectNotFound,
                format!("No .qqq/qqq.db found in {}", directory.display())
            )
        );
        path
    };
    Ok(path)
}

pub(crate) fn ensure_read_only_safe(path: &Path) -> Result<()> {
    let unsafe_state = || {
        crate::errors::Info::new(
        crate::errors::Code::DatabaseError,
        "SQLite WAL state is unsafe for read-only access; stop writers and use journal_mode=DELETE before retrying",
    ).detail("reason", "unsafe_read_only")
    };
    // For valid DBs READ_ONLY returns SQLITE_READONLY_ROLLBACK before hot
    // journal recovery. Ordinary
    // writers' journals must remain readable for concurrent queue diagnostics.
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        ensure!(
            std::fs::symlink_metadata(&sidecar)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            unsafe_state()
        );
    }
    let mut header = [0_u8; 20];
    let mut file = std::fs::File::open(path)?;
    let bytes = file.read(&mut header)?;
    let valid_header = bytes == header.len()
        && &header[..16] == b"SQLite format 3\0"
        && file.metadata()?.len() >= 512;
    if valid_header {
        ensure!(header[18] != 2 && header[19] != 2, unsafe_state());
    } else {
        // Defer malformed/empty DB with journal before SQLite can perform
        // VFS-dependent cleanup; let SQLite diagnose bare corrupt DBs.
        let mut journal = path.as_os_str().to_os_string();
        journal.push("-journal");
        ensure!(
            std::fs::symlink_metadata(journal)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
            unsafe_state()
        );
    }
    Ok(())
}
impl Db {
    pub fn open(init: bool) -> Result<(Self, PathBuf)> {
        let path = database_path(init)?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | if init {
                OpenFlags::SQLITE_OPEN_CREATE
            } else {
                OpenFlags::empty()
            };
        let image_store = ImageStore::new(
            path.parent()
                .context("Missing database directory")?
                .join("images"),
        );
        let mut conn = Connection::open_with_flags(&path, flags)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(
            (1..=SCHEMA_VERSION).contains(&version) || (init && version == 0),
            crate::errors::Info::new(
                crate::errors::Code::DatabaseError,
                format!("Unsupported database schema version {version}")
            )
            .detail("schema_version", version)
            .detail("reason", "unsupported_schema")
        );
        ensure_description_schema(&conn)?;
        if version < SCHEMA_VERSION {
            // Rebuild CHECK constraints without changing references to tasks.
            // SQLite requires foreign_keys to change outside a transaction.
            let disable_foreign_keys = version < 6;
            if disable_foreign_keys {
                conn.pragma_update(None, "foreign_keys", "OFF")?;
            }
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut pending = PendingFiles::new();
            let mut had_images = false;
            // Another CLI may have migrated while we waited for the write lock.
            let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
            ensure!(
                (1..=SCHEMA_VERSION).contains(&version) || (init && version == 0),
                crate::errors::Info::new(
                    crate::errors::Code::DatabaseError,
                    format!("Unsupported database schema version {version}")
                )
                .detail("schema_version", version)
                .detail("reason", "unsupported_schema")
            );
            ensure_description_schema(&tx)?;
            if version > 0 && version < SCHEMA_VERSION {
                let saved = crate::snapshot::upgrade::capture(&tx, &path, version)?;
                if !crate::errors::json_output() {
                    eprintln!("Saved pre-upgrade snapshot: {}", saved.display());
                }
            }
            if version == 0 {
                tx.execute_batch(include_str!("sql/schema.sql"))?;
            }
            if version < 2 {
                tx.execute_batch(include_str!("sql/migrate_v2.sql"))?;
            }
            if version < 3 {
                tx.execute_batch(include_str!("sql/migrate_v3.sql"))?;
            }
            if version < 4 {
                tx.execute_batch(include_str!("sql/migrate_v4.sql"))?;
                ensure!(
                    !tx.prepare("PRAGMA foreign_key_check")?.exists([])?,
                    "Database migration found invalid foreign key references"
                );
            }
            if version < 5 {
                tx.execute_batch(include_str!("sql/migrate_v5.sql"))?;
            }
            if version < 6 {
                {
                    let mut statement =
                        tx.prepare("SELECT id,task_id,media_type,data FROM images ORDER BY id")?;
                    let rows = statement.query_map([], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Vec<u8>>(3)?,
                        ))
                    })?;
                    for row in rows {
                        let (image_id, task_id, media_type, data) = row?;
                        image_store.write(&mut pending, task_id, image_id, &media_type, &data)?;
                        had_images = true;
                    }
                }
                tx.execute_batch(include_str!("sql/migrate_v6.sql"))?;
                ensure!(
                    !tx.prepare("PRAGMA foreign_key_check")?.exists([])?,
                    "Database migration found invalid foreign key references"
                );
            }
            if version < 7 {
                tx.execute_batch(include_str!("sql/migrate_v7.sql"))?;
            }
            if version < 8 {
                tx.execute_batch(include_str!("sql/migrate_v8.sql"))?;
            }
            if version < 9 {
                tx.execute_batch(include_str!("sql/migrate_v9.sql"))?;
            }
            if version < 10 {
                tx.execute_batch(include_str!("sql/migrate_v10.sql"))?;
            }
            if version < 11 {
                tx.execute_batch(include_str!("sql/migrate_v11.sql"))?;
            }
            if version < 12 {
                tx.execute_batch(include_str!("sql/migrate_v12.sql"))?;
            }
            commit_with_files(tx, &mut pending)?;
            if disable_foreign_keys {
                conn.pragma_update(None, "foreign_keys", "ON")?;
            }
            if had_images {
                if let Err(error) = conn.execute_batch("VACUUM") {
                    eprintln!("Warning: migrated image bytes; SQLite compaction failed: {error}");
                }
            }
        }
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(
            version == SCHEMA_VERSION,
            crate::errors::Info::new(
                crate::errors::Code::DatabaseError,
                format!("Unsupported database schema version {version}")
            )
            .detail("schema_version", version)
            .detail("reason", "unsupported_schema")
        );
        let mut db = Self { conn, image_store };
        if !crate::preflight::already_checked(&path) {
            crate::preflight::run(&mut db.conn)?;
        }
        Ok((db, path))
    }
    pub fn open_read_only() -> Result<(Self, PathBuf)> {
        let path = database_path(false)?;
        // SQLite READ_ONLY can create WAL/shm files. Refuse that mode and
        // existing sidecars before SQLite touches the live project.
        ensure_read_only_safe(&path)?;
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        ensure_read_only_safe(&path)?;
        conn.pragma_update(None, "query_only", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        ensure!(
            (1..=SCHEMA_VERSION).contains(&version),
            crate::errors::Info::new(
                crate::errors::Code::DatabaseError,
                format!("Unsupported database schema version {version}")
            )
            .detail("schema_version", version)
            .detail("reason", "unsupported_schema")
        );
        ensure!(
            version == SCHEMA_VERSION,
            crate::errors::Info::new(crate::errors::Code::DatabaseError, format!("Database schema version {version} requires migration; run qqq list before diagnostics")).detail("schema_version", version).detail("expected_schema_version", SCHEMA_VERSION).detail("reason", "migration_required")
        );
        ensure_description_schema(&conn)?;
        let image_store = ImageStore::new(
            path.parent()
                .context("Missing database directory")?
                .join("images"),
        );
        Ok((Self { conn, image_store }, path))
    }
    pub fn task(&self, id: i64) -> Result<Task> {
        self.conn
            .query_row(
                &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
                [id],
                task_row,
            )
            .optional()?
            .with_context(|| crate::errors::Info::missing_task(id))
    }
    pub fn task_exists(&self, id: i64) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?)",
            [id],
            |row| row.get(0),
        )?)
    }
    pub fn content_snapshot(&self, id: i64) -> Result<ContentSnapshot> {
        let tx = self.conn.unchecked_transaction()?;
        let task = tx
            .query_row(
                &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
                [id],
                task_row,
            )
            .optional()?
            .with_context(|| crate::errors::Info::missing_task(id))?;
        // Same connection participates in the read transaction above.
        let references = self.image_references(id)?;
        tx.commit()?;
        Ok(ContentSnapshot { task, references })
    }
    pub fn set_archived(&mut self, id: i64, archived: bool, actor: &str) -> Result<Task> {
        nonempty(actor, "Actor")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task = tx
            .query_row(
                &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
                [id],
                task_row,
            )
            .optional()?
            .with_context(|| crate::errors::Info::missing_task(id))?;
        if task.archived == archived {
            tx.commit()?;
            return Ok(task);
        }
        if archived {
            ensure!(
                task.status != "in_progress",
                crate::errors::Info::transition(
                    id,
                    &task.status,
                    &["new", "error", "completed"],
                    format!("Task {id} is in progress and cannot be archived")
                )
            );
            if task.status != "completed" {
                let unfinished_child: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM tasks WHERE (parent_id=?1 OR id IN (SELECT task_id FROM task_dependencies WHERE prerequisite_id=?1)) AND archived=0 AND status!='completed')",
                    [id],
                    |row| row.get(0),
                )?;
                ensure!(
                    !unfinished_child,
                    crate::errors::Info::transition(
                        id,
                        &task.status,
                        &["completed"],
                        format!("Task {id} has non-archived unfinished child or dependent")
                    )
                    .detail("reason", "unfinished_child")
                );
            }
        } else if task.status != "completed" {
            crate::dependencies::ensure_all_available(&tx, id)?;
        }
        tx.execute(
            "UPDATE tasks SET archived=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
            params![archived, id],
        )?;
        tx.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?,?,?)",
            params![id, actor, if archived { "archive" } else { "unarchive" }],
        )?;
        let task = tx.query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
            [id],
            task_row,
        )?;
        tx.commit()?;
        Ok(task)
    }
    pub fn add(
        &mut self,
        description: &str,
        parent_id: Option<i64>,
        images: &[ImageInput],
    ) -> Result<Task> {
        self.add_with_options(
            description,
            images,
            AddOptions {
                parent: parent_id,
                ..Default::default()
            },
        )
    }
    pub fn add_with_priority(
        &mut self,
        description: &str,
        parent_id: Option<i64>,
        images: &[ImageInput],
        priority: i64,
    ) -> Result<Task> {
        if priority == 0 {
            self.add(description, parent_id, images)
        } else {
            self.add_with_options(
                description,
                images,
                AddOptions {
                    parent: parent_id,
                    priority,
                    ..Default::default()
                },
            )
        }
    }
    pub fn add_with_dependencies(
        &mut self,
        description: &str,
        parent: Option<i64>,
        images: &[ImageInput],
        priority: i64,
        prerequisites: &[i64],
    ) -> Result<Task> {
        if prerequisites.is_empty() {
            self.add_with_priority(description, parent, images, priority)
        } else {
            self.add_with_options(
                description,
                images,
                AddOptions {
                    parent,
                    priority,
                    prerequisites,
                    ..Default::default()
                },
            )
        }
    }
    pub fn add_with_options(
        &mut self,
        description: &str,
        images: &[ImageInput],
        options: AddOptions<'_>,
    ) -> Result<Task> {
        self.add_with_spans(description, images, &[], options)
    }
    fn add_with_spans(
        &mut self,
        description: &str,
        images: &[ImageInput],
        image_spans: &[Range<usize>],
        options: AddOptions<'_>,
    ) -> Result<Task> {
        let AddOptions {
            parent: parent_id,
            priority,
            prerequisites,
            tags,
        } = options;
        let tags = serde_json::to_string(&crate::tags::normalize(tags)?)?;
        validate_description(description)?;
        validate_priority(priority)?;
        for image in images {
            image.media_type()?;
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(parent_id) = parent_id {
            ensure_parent_available(&tx, parent_id, true)?;
        }
        crate::dependencies::validate_add(&tx, parent_id, prerequisites, true)?;
        let mut pending = PendingFiles::new();
        tx.execute(
            "INSERT INTO tasks(description,parent_id,priority,tags) VALUES (?,?,?,?)",
            params![description, parent_id, priority, tags],
        )?;
        let id = tx.last_insert_rowid();
        for prerequisite in prerequisites {
            tx.execute(
                "INSERT INTO task_dependencies(task_id,prerequisite_id) VALUES (?,?)",
                params![id, prerequisite],
            )?;
        }
        let image_ids = Self::save_images(&tx, &self.image_store, &mut pending, id, images)?;
        if !image_spans.is_empty() {
            let description = image_description(description, image_spans, images, &image_ids, id)?;
            tx.execute(
                "UPDATE tasks SET description=? WHERE id=?",
                params![description, id],
            )?;
        }
        let task = tx.query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
            [id],
            task_row,
        )?;
        commit_with_files(tx, &mut pending)?;
        Ok(task)
    }
    pub fn save_composition(
        &mut self,
        id: Option<i64>,
        parent: Option<i64>,
        draft: &crate::tui::draft::Composition,
    ) -> Result<Task> {
        self.save_composition_with_priority(id, parent, draft, 0)
    }
    pub fn save_composition_with_priority(
        &mut self,
        id: Option<i64>,
        parent: Option<i64>,
        draft: &crate::tui::draft::Composition,
        priority: i64,
    ) -> Result<Task> {
        self.save_composition_with_dependencies(id, parent, draft, priority, &[])
    }
    pub fn save_composition_with_dependencies(
        &mut self,
        id: Option<i64>,
        parent: Option<i64>,
        draft: &crate::tui::draft::Composition,
        priority: i64,
        prerequisites: &[i64],
    ) -> Result<Task> {
        self.save_composition_with_options(
            id,
            draft,
            AddOptions {
                parent,
                priority,
                prerequisites,
                ..Default::default()
            },
        )
    }
    pub fn save_composition_with_options(
        &mut self,
        id: Option<i64>,
        draft: &crate::tui::draft::Composition,
        options: AddOptions<'_>,
    ) -> Result<Task> {
        match id {
            Some(id) => self.edit_composition(id, draft, None),
            None => self.add_with_spans(
                &draft.description,
                &draft.images,
                &draft.image_spans,
                options,
            ),
        }
    }
    pub fn edit_composition(
        &mut self,
        id: i64,
        draft: &crate::tui::draft::Composition,
        parent: Option<ParentChange>,
    ) -> Result<Task> {
        self.edit_composition_with_priority(id, draft, parent, None)
    }
    pub fn edit_composition_with_priority(
        &mut self,
        id: i64,
        draft: &crate::tui::draft::Composition,
        parent: Option<ParentChange>,
        priority: Option<i64>,
    ) -> Result<Task> {
        self.edit_composition_guarded(id, draft, parent, priority, None)
    }
    pub fn edit_composition_guarded(
        &mut self,
        id: i64,
        draft: &crate::tui::draft::Composition,
        parent: Option<ParentChange>,
        priority: Option<i64>,
        expected_revision: Option<i64>,
    ) -> Result<Task> {
        self.edit_with_spans(
            id,
            Some(&draft.description),
            &draft.images,
            &draft.image_spans,
            EditOptions {
                transition: None,
                parent,
                priority,
                expected_revision,
                dependencies: Default::default(),
                tags: None,
            },
        )
    }
    pub fn image_references(&self, task_id: i64) -> Result<Vec<ImageReference>> {
        Self::image_references_from(&self.conn, task_id)
    }
    fn image_references_from(conn: &Connection, task_id: i64) -> Result<Vec<ImageReference>> {
        Ok(conn
            .prepare("SELECT id,name,media_type FROM images WHERE task_id=? ORDER BY id")?
            .query_map([task_id], |row| {
                Ok(ImageReference {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    media_type: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn list(&self, max_completed: Option<i64>) -> Result<Vec<Task>> {
        self.list_with_archived(max_completed, false)
    }
    pub fn list_with_archived(
        &self,
        max_completed: Option<i64>,
        include_archived: bool,
    ) -> Result<Vec<Task>> {
        Ok(self.conn.prepare(
            &format!("SELECT {TASK_COLUMNS} FROM tasks
             WHERE (?2 OR archived=0) AND (?1 IS NULL OR status!='completed' OR id IN (
                 SELECT id FROM tasks WHERE status='completed' AND (?2 OR archived=0)
                 ORDER BY (SELECT MAX(id) FROM events WHERE task_id=tasks.id AND action='complete') DESC,
                          updated_at DESC, id DESC
                 LIMIT COALESCE(?1,-1)
             ))
             ORDER BY id")
        )?.query_map(params![max_completed, include_archived],task_row)?.collect::<rusqlite::Result<_>>()?)
    }
    /// Evaluate matches in the same snapshot as base rows; ancestors remain available.
    pub fn list_filtered(
        &self,
        max_completed: Option<i64>,
        include_archived: bool,
        filter: Option<&CompiledFilter>,
    ) -> Result<(Vec<Task>, Option<HashSet<i64>>)> {
        let Some(filter) = filter else {
            return Ok((
                self.list_with_archived(max_completed, include_archived)?,
                None,
            ));
        };
        let limit = filter.params().len() + 1;
        let archive = limit + 1;
        let sql = format!(
            "SELECT {TASK_COLUMNS},({predicate}) FROM tasks
             WHERE (?{archive} OR archived=0) AND (?{limit} IS NULL OR status!='completed' OR id IN (
                 SELECT id FROM tasks WHERE status='completed' AND (?{archive} OR archived=0)
                 ORDER BY (SELECT MAX(id) FROM events WHERE task_id=tasks.id AND action='complete') DESC,
                          updated_at DESC, id DESC LIMIT COALESCE(?{limit},-1)
             )) ORDER BY id",
            predicate = filter.sql()
        );
        let mut values = filter.params().to_vec();
        values.push(max_completed.map_or(
            rusqlite::types::Value::Null,
            rusqlite::types::Value::Integer,
        ));
        values.push(rusqlite::types::Value::Integer(i64::from(include_archived)));
        let mut statement = self.conn.prepare(&sql).context(filter_evaluation_error())?;
        let rows = statement
            .query_map(params_from_iter(values.iter()), |row| {
                Ok((task_row(row)?, row.get::<_, bool>(16)?))
            })
            .context(filter_evaluation_error())?;
        let mut tasks = Vec::new();
        let mut matches = HashSet::new();
        for row in rows {
            let (task, matched) = row.context(filter_evaluation_error())?;
            if matched {
                matches.insert(task.id);
            }
            tasks.push(task);
        }
        Ok((tasks, Some(matches)))
    }
    pub fn data_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "data_version", |row| row.get(0))?)
    }
    pub fn edit(
        &mut self,
        id: i64,
        description: Option<&str>,
        transition: Option<EditTransition<'_>>,
        images: &[ImageInput],
        parent: Option<ParentChange>,
    ) -> Result<Task> {
        self.edit_with_priority(id, description, transition, images, parent, None)
    }
    pub fn edit_with_priority(
        &mut self,
        id: i64,
        description: Option<&str>,
        transition: Option<EditTransition<'_>>,
        images: &[ImageInput],
        parent: Option<ParentChange>,
        priority: Option<i64>,
    ) -> Result<Task> {
        self.edit_guarded(
            id,
            description,
            images,
            EditOptions {
                transition,
                parent,
                priority,
                expected_revision: None,
                dependencies: Default::default(),
                tags: None,
            },
        )
    }
    pub fn edit_guarded(
        &mut self,
        id: i64,
        description: Option<&str>,
        images: &[ImageInput],
        options: EditOptions<'_>,
    ) -> Result<Task> {
        self.edit_with_spans(id, description, images, &[], options)
    }
    fn edit_with_spans(
        &mut self,
        id: i64,
        description: Option<&str>,
        images: &[ImageInput],
        image_spans: &[Range<usize>],
        changes: EditOptions<'_>,
    ) -> Result<Task> {
        let EditOptions {
            transition,
            parent,
            priority,
            expected_revision,
            dependencies,
            tags,
        } = changes;
        let tags = tags
            .map(crate::tags::normalize)
            .transpose()?
            .map(|labels| serde_json::to_string(&labels))
            .transpose()?;
        if let Some(description) = description {
            nonempty(description, "Description")?;
        }
        if let Some(priority) = priority {
            validate_priority(priority)?;
        }
        for image in images {
            image.media_type()?;
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(expected_revision) = expected_revision {
            ensure!(expected_revision > 0, "Expected revision must be positive");
            let current = tx
                .query_row(
                    "SELECT description,content_revision FROM tasks WHERE id=?",
                    [id],
                    |row| {
                        Ok(CurrentContent {
                            description: row.get(0)?,
                            revision: row.get(1)?,
                        })
                    },
                )
                .optional()?;
            if current.as_ref().map(|content| content.revision) != Some(expected_revision) {
                return Err(ContentConflict {
                    task_id: id,
                    expected_revision,
                    current,
                }
                .into());
            }
        }
        let mut pending = PendingFiles::new();
        if let Some(parent) = parent {
            let parent_id = match parent {
                ParentChange::Clear => None,
                ParentChange::Set(parent_id) => {
                    ensure!(
                        parent_id != id,
                        crate::errors::Info::invalid_argument(
                            "--set-parent",
                            format!("Task {id} cannot depend on itself")
                        )
                        .detail("task_id", id)
                    );
                    let unfinished: bool = tx
                        .query_row(
                            "SELECT status!='completed' FROM tasks WHERE id=?",
                            [id],
                            |row| row.get(0),
                        )
                        .optional()?
                        .with_context(|| crate::errors::Info::missing_task(id))?;
                    ensure_parent_available(&tx, parent_id, unfinished)?;
                    Some(parent_id)
                }
            };
            ensure!(
                tx.execute(
                    "UPDATE tasks SET parent_id=? WHERE id=?",
                    params![parent_id, id]
                )? == 1,
                crate::errors::Info::missing_task(id)
            );
        }
        if parent.is_some() || !dependencies.is_empty() {
            crate::dependencies::edit(&tx, id, &dependencies)?;
        }
        match transition {
            Some(EditTransition::New {
                session,
                harness_name,
            }) => {
                let session = Self::owner_key(&tx, session, harness_name)?;
                Self::release_claim(&tx, id, &session)?;
            }
            Some(EditTransition::NewExact(session)) => {
                Self::release_claim(&tx, id, session)?;
            }
            Some(EditTransition::ForceNew(session)) => {
                nonempty(session, "Session")?;
                ensure!(
                    tx.execute(
                        "UPDATE tasks SET status='new',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL WHERE id=? AND status IN ('in_progress','error')",
                        [id]
                    )? == 1,
                    transition_error(&tx, id, &["in_progress", "error"], None, format!("Task {id} is not in progress or error"))?
                );
                tx.execute(
                    "INSERT INTO events(task_id,session,action) VALUES (?,?,'release')",
                    params![id, session],
                )?;
            }
            Some(EditTransition::RetryError(session)) => {
                nonempty(session, "Session")?;
                ensure!(
                    tx.execute(
                        "UPDATE tasks SET status='new',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL WHERE id=? AND status='error'",
                        [id]
                    )? == 1,
                    transition_error(&tx, id, &["error"], None, format!("Task {id} is no longer in error; inspect its current status before retrying"))?
                );
                tx.execute(
                    "INSERT INTO events(task_id,session,action) VALUES (?,?,'release')",
                    params![id, session],
                )?;
            }
            Some(EditTransition::Error {
                session,
                reason,
                harness_name,
            }) => {
                let session = Self::owner_key(&tx, session, harness_name)?;
                nonempty(reason, "Error reason")?;
                ensure!(tx.execute("UPDATE tasks SET status='error',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL WHERE id=? AND status='in_progress' AND claim_key=?",params![id,session])?==1,Self::transition_error_for_owner(&tx, id, &session)?);
                tx.execute(
                    "INSERT INTO events(task_id,session,action) VALUES (?,?,'error')",
                    params![id, session],
                )?;
                tx.execute(
                    "INSERT INTO messages(task_id,body,session) VALUES (?,?,?)",
                    params![id, reason, session],
                )?;
            }
            None => {}
        }
        ensure!(tx.execute("UPDATE tasks SET description=COALESCE(?,description),priority=COALESCE(?,priority),tags=COALESCE(?,tags),updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",params![description,priority,tags,id])?==1,crate::errors::Info::missing_task(id));
        let image_ids = Self::save_images(&tx, &self.image_store, &mut pending, id, images)?;
        if !image_spans.is_empty() {
            let source = description.context("Image references require description")?;
            let description = image_description(source, image_spans, images, &image_ids, id)?;
            tx.execute(
                "UPDATE tasks SET description=? WHERE id=?",
                params![description, id],
            )?;
        }
        let task = tx.query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
            [id],
            task_row,
        )?;
        commit_with_files(tx, &mut pending)?;
        Ok(task)
    }
    pub fn resolve_task_id(&self, reference: i64) -> Result<i64> {
        ensure!(
            reference != 0,
            crate::errors::Info::invalid_argument(
                "task_id",
                "Task reference must be a positive ID or negative creation index"
            )
        );
        if reference > 0 {
            return Ok(reference);
        }
        // AUTOINCREMENT IDs preserve insertion order even when timestamps tie.
        // Add before negating so i64::MIN maps safely to i64::MAX.
        let offset = -(reference + 1);
        self.conn
            .query_row(
                "SELECT id FROM tasks ORDER BY id DESC LIMIT 1 OFFSET ?",
                [offset],
                |row| row.get(0),
            )
            .optional()?
            .with_context(|| {
                crate::errors::Info::new(
                    crate::errors::Code::TaskNotFound,
                    format!("No task at recent creation index {reference}"),
                )
                .detail("task_reference", reference)
            })
    }
    pub fn adjacent_task(
        &self,
        current: Option<i64>,
        older: bool,
        include_archived: bool,
    ) -> Result<Option<(i64, String, String)>> {
        let found = match (older, current) {
            (true, None) => self.conn.query_row(
                "SELECT id,description,status FROM tasks WHERE (?1 OR archived=0) ORDER BY id DESC LIMIT 1",
                [include_archived],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ),
            (true, Some(id)) => self.conn.query_row(
                "SELECT id,description,status FROM tasks WHERE id < ?1 AND (?2 OR archived=0) ORDER BY id DESC LIMIT 1",
                params![id, include_archived],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ),
            (false, Some(id)) => self.conn.query_row(
                "SELECT id,description,status FROM tasks WHERE id > ?1 AND (?2 OR archived=0) ORDER BY id ASC LIMIT 1",
                params![id, include_archived],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            ),
            (false, None) => return Ok(None),
        };
        Ok(found.optional()?)
    }
    pub fn active_identity_for_terminal(
        &self,
        agent: &str,
        terminal: &str,
    ) -> Result<Option<crate::herdr::AgentSession>> {
        let mut query = self.conn.prepare(
            "SELECT tasks.claim_key,herdr_links.link_json FROM tasks
             JOIN herdr_links ON herdr_links.task_id=tasks.id
             WHERE tasks.status='in_progress'
               AND json_extract(herdr_links.link_json,'$.pane.terminal_id')=?
               AND json_extract(herdr_links.link_json,'$.identity.agent')=?",
        )?;
        let records = query.query_map([terminal, agent], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut active = None;
        for record in records {
            let (claim_key, encoded) = record?;
            let link: crate::herdr::Link = serde_json::from_str(&encoded)?;
            let owner = serde_json::to_string(&(
                &link.identity.agent,
                &link.identity.kind,
                &link.identity.value,
            ))?;
            // Explicit and dispatched assignments have separate owner IDs.
            if claim_key == owner {
                ensure!(
                    active.is_none(),
                    "Multiple active claims match Herdr terminal; use --session"
                );
                active = Some(link.identity);
            }
        }
        Ok(active)
    }
    fn transition_error_for_owner(
        conn: &Connection,
        id: i64,
        session: &str,
    ) -> Result<crate::errors::Info> {
        transition_error(
            conn,
            id,
            &["in_progress"],
            Some(session),
            format!("Task {id} is not claimed by session {session}"),
        )
    }
    fn release_claim(conn: &Connection, id: i64, session: &str) -> Result<()> {
        nonempty(session, "Session")?;
        ensure!(conn.execute("UPDATE tasks SET status='new',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL WHERE id=? AND status='in_progress' AND claim_key=?",params![id,session])?==1,Self::transition_error_for_owner(conn, id, session)?);
        conn.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?,?,'release')",
            params![id, session],
        )?;
        Ok(())
    }
    pub(crate) fn owner_key(
        conn: &Connection,
        session: &str,
        harness_name: Option<&str>,
    ) -> Result<String> {
        nonempty(session, "Session")?;
        let exact: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE status='in_progress' AND claim_key=?)",
            [session],
            |row| row.get(0),
        )?;
        if exact {
            return Ok(session.into());
        }
        let keys = conn
            .prepare(
                "SELECT claim_key FROM tasks WHERE status='in_progress' AND harness_session=?1
             AND (?2 IS NULL OR harness_name=?2) LIMIT 2",
            )?
            .query_map(params![session, harness_name], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            keys.len() <= 1,
            crate::errors::Info::invalid_argument("--harness-name", "Multiple active claims match harness session; use --harness-name or original --session token")
                .detail("matches", keys.len()).human(format!("Multiple active claims match harness session {session}; use --harness-name or original --session token"))
        );
        Ok(keys.into_iter().next().unwrap_or_else(|| session.into()))
    }
    pub fn resolve_owner(&self, session: &str, harness_name: Option<&str>) -> Result<String> {
        Self::owner_key(&self.conn, session, harness_name)
    }
    fn save_identity(
        conn: &Connection,
        id: i64,
        identity: &crate::identity::Identity,
    ) -> Result<()> {
        conn.execute(
            "UPDATE tasks SET harness_name=?,harness_session=?,orchestrator_name=?,orchestrator_session=? WHERE id=? AND status='in_progress'",
            params![identity.harness_name, identity.harness_session, identity.orchestrator_name, identity.orchestrator_session, id],
        )?;
        Ok(())
    }
    pub fn owned_with_name(
        &self,
        session: &str,
        harness_name: Option<&str>,
    ) -> Result<Option<Task>> {
        let session = self.resolve_owner(session, harness_name)?;
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {TASK_COLUMNS} FROM tasks WHERE status='in_progress' AND claim_key=?"
                ),
                [session],
                task_row,
            )
            .optional()?)
    }
    pub fn has_ready(&self) -> Result<bool> {
        Ok(Self::ready_task_id(&self.conn, None)?.is_some())
    }
    pub fn has_ready_filtered(&self, filter: Option<&CompiledFilter>) -> Result<bool> {
        match filter {
            None => self.has_ready(),
            Some(filter) => Ok(Self::ready_task_id(&self.conn, Some(filter))?.is_some()),
        }
    }
    pub(crate) fn ready_task_id(
        conn: &Connection,
        filter: Option<&CompiledFilter>,
    ) -> Result<Option<i64>> {
        let predicate = filter.map_or("1", CompiledFilter::sql);
        let sql = format!(
            "SELECT id FROM tasks WHERE {READY_TASK_PREDICATE}
            AND ({predicate}) ORDER BY priority DESC,id ASC LIMIT 1"
        );
        let result = conn
            .query_row(
                &sql,
                params_from_iter(filter.map_or(&[][..], CompiledFilter::params)),
                |row| row.get(0),
            )
            .optional();
        match filter {
            Some(_) => Ok(result.context(filter_evaluation_error())?),
            None => Ok(result?),
        }
    }
    pub fn peek_next_filtered(&mut self, filter: Option<&CompiledFilter>) -> Result<Option<Task>> {
        if self.conn.is_readonly(rusqlite::DatabaseName::Main)? {
            if let Some(path) = self.conn.path() {
                // --wait may outlive a writer changing the journaling mode.
                // Do this before beginning a read transaction (raw fd closes
                // must not release an active SQLite POSIX lock).
                ensure_read_only_safe(Path::new(path))?;
            }
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        let id = Self::ready_task_id(&tx, filter)?;
        let task = id
            .map(|id| {
                tx.query_row(
                    &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
                    [id],
                    task_row,
                )
            })
            .transpose()?;
        tx.commit()?;
        Ok(task)
    }
    pub fn next(
        &mut self,
        session: &str,
        link: Option<&crate::herdr::Link>,
    ) -> Result<Option<Task>> {
        self.next_with_identity(session, link, None, &crate::identity::Identity::default())
    }
    pub fn next_with_identity(
        &mut self,
        session: &str,
        link: Option<&crate::herdr::Link>,
        metadata: Option<&crate::identity::Identity>,
        overrides: &crate::identity::Identity,
    ) -> Result<Option<Task>> {
        self.claim(session, link, metadata, overrides, true, None)
    }
    pub fn next_filtered(
        &mut self,
        session: &str,
        link: Option<&crate::herdr::Link>,
        filter: Option<&CompiledFilter>,
    ) -> Result<Option<Task>> {
        match filter {
            None => self.next(session, link),
            Some(filter) => self.next_with_identity_filtered(
                session,
                link,
                None,
                &crate::identity::Identity::default(),
                Some(filter),
            ),
        }
    }
    pub fn next_with_identity_filtered(
        &mut self,
        session: &str,
        link: Option<&crate::herdr::Link>,
        metadata: Option<&crate::identity::Identity>,
        overrides: &crate::identity::Identity,
        filter: Option<&CompiledFilter>,
    ) -> Result<Option<Task>> {
        match filter {
            None => self.next_with_identity(session, link, metadata, overrides),
            Some(filter) => self.claim(session, link, metadata, overrides, true, Some(filter)),
        }
    }
    /// Retrieve an existing assignment without claiming queued work if it vanished.
    pub fn owned_with_identity(
        &mut self,
        session: &str,
        overrides: &crate::identity::Identity,
    ) -> Result<Option<Task>> {
        self.claim(session, None, None, overrides, false, None)
    }
    fn claim(
        &mut self,
        session: &str,
        link: Option<&crate::herdr::Link>,
        metadata: Option<&crate::identity::Identity>,
        overrides: &crate::identity::Identity,
        allow_new: bool,
        filter: Option<&CompiledFilter>,
    ) -> Result<Option<Task>> {
        nonempty(session, "Session")?;
        overrides.validate()?;
        if let Some(metadata) = metadata {
            metadata.validate()?;
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let session = Self::owner_key(&tx, session, overrides.harness_name.as_deref())?;
        let owned: Option<i64> = tx
            .query_row(
                "SELECT id FROM tasks WHERE status='in_progress' AND claim_key=?",
                [&session],
                |r| r.get(0),
            )
            .optional()?;
        let id = match owned {
            Some(id) => Some(id),
            None if allow_new => Self::ready_task_id(&tx, filter)?,
            None => None,
        };
        if let Some(id) = id {
            if owned.is_none() {
                tx.execute("DELETE FROM herdr_links WHERE task_id=?", [id])?;
                tx.execute("UPDATE tasks SET status='in_progress',claim_key=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",params![session,id])?;
                tx.execute(
                    "INSERT INTO events(task_id,session,action) VALUES (?,?,'claim')",
                    params![id, session],
                )?;
            }
            if let Some(link) = link {
                Self::save_link(&tx, id, link)?;
            }
            let mut identity = if owned.is_some() {
                tx.query_row(
                    &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
                    [id],
                    task_row,
                )?
                .identity
            } else {
                metadata
                    .cloned()
                    .unwrap_or_else(|| crate::identity::Identity::for_claim(&session, link))
            };
            identity.overlay(overrides);
            Self::save_identity(&tx, id, &identity)?;
        }
        let task = id
            .map(|id| {
                tx.query_row(
                    &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
                    [id],
                    task_row,
                )
            })
            .transpose()?;
        tx.commit()?;
        Ok(task)
    }
    pub fn complete(&mut self, id: i64, session: &str, harness_name: Option<&str>) -> Result<Task> {
        nonempty(session, "Session")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let session = Self::owner_key(&tx, session, harness_name)?;
        ensure!(tx.execute("UPDATE tasks SET status='completed',claim_key=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status='in_progress' AND claim_key=?",params![id,session])?==1,Self::transition_error_for_owner(&tx, id, &session)?);
        tx.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?, ?, 'complete')",
            params![id, session],
        )?;
        let task = tx.query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
            [id],
            task_row,
        )?;
        tx.commit()?;
        Ok(task)
    }
    pub fn force_complete(&mut self, id: i64, actor: &str) -> Result<Task> {
        nonempty(actor, "Actor")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(
            tx.execute("UPDATE tasks SET status='completed',claim_key=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status IN ('new','in_progress','error')", [id])? == 1,
            transition_error(
                &tx,
                id,
                &["new", "in_progress", "error"],
                None,
                format!("Task {id} must be unfinished to force complete"),
            )?
        );
        tx.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?, ?, 'complete')",
            params![id, actor],
        )?;
        let task = tx.query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
            [id],
            task_row,
        )?;
        tx.commit()?;
        Ok(task)
    }
    pub fn reopen(&mut self, id: i64, actor: &str) -> Result<Task> {
        nonempty(actor, "Actor")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task = tx
            .query_row(
                &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
                [id],
                task_row,
            )
            .optional()?
            .with_context(|| crate::errors::Info::missing_task(id))?;
        ensure!(
            task.status == "completed" || task.status == "in_progress",
            crate::errors::Info::transition(
                id,
                &task.status,
                &["completed"],
                format!("Task {id} must be completed to reopen")
            )
        );
        ensure!(
            !task.archived,
            crate::errors::Info::transition(
                id,
                &task.status,
                &["completed"],
                format!("Task {id} is archived; unarchive first")
            )
            .detail("actual_archived", true)
            .detail("expected_archived", false)
        );
        crate::dependencies::ensure_all_available(&tx, id)?;
        if task.status == "in_progress" {
            Self::ensure_orphaned_claim(&tx, &task)?;
        }
        tx.execute(
            "UPDATE tasks SET status='new',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
            [id],
        )?;
        tx.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?,?,'reopen')",
            params![id, actor],
        )?;
        let task = tx.query_row(
            &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
            [id],
            task_row,
        )?;
        tx.commit()?;
        Ok(task)
    }
    fn ensure_orphaned_claim(conn: &Connection, task: &Task) -> Result<()> {
        let (claim, encoded): (Option<String>, Option<String>) = conn.query_row(
            "SELECT tasks.claim_key,herdr_links.link_json FROM tasks
             LEFT JOIN herdr_links ON herdr_links.task_id=tasks.id WHERE tasks.id=?",
            [task.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let invalid = |reason| {
            crate::errors::Info::transition(
                task.id,
                &task.status,
                &["completed"],
                format!(
                    "Task {} must be completed to reopen; cannot verify absent Herdr owner",
                    task.id
                ),
            )
            .detail("reason", reason)
        };
        let encoded = encoded.with_context(|| invalid("missing_owner_link"))?;
        let stored: StoredLink = serde_json::from_str(&encoded)?;
        // Fresh claims replace saved links, including before claim bindings existed.
        // Preserve recovery for legacy dispatched/explicit owners with opaque keys.
        ensure!(
            claim.is_some()
                && stored
                    .claim_key
                    .as_ref()
                    .is_none_or(|expected| claim.as_ref() == Some(expected)),
            invalid("owner_link_mismatch")
        );
        ensure!(
            !crate::herdr::owner_is_live(&stored.link)?,
            crate::errors::Info::transition(
                task.id,
                &task.status,
                &["completed"],
                format!("Task {} owning Herdr agent session is still live", task.id),
            )
            .detail("reason", "owner_still_live")
        );
        Ok(())
    }
    pub fn message(&self, id: i64, body: &str, session: Option<&str>) -> Result<Value> {
        nonempty(body, "Message")?;
        self.task(id)?;
        self.conn.execute(
            "INSERT INTO messages(task_id,body,session) VALUES (?,?,?)",
            params![id, body, session],
        )?;
        Ok(json!({"id":self.conn.last_insert_rowid(),"task_id":id,"body":body,"session":session}))
    }
    pub fn show(&self, id: i64) -> Result<Value> {
        self.show_task(&self.task(id)?)
    }
    pub(crate) fn show_task(&self, task: &Task) -> Result<Value> {
        // TUI already listed this task; deletion before section reads is harmless.
        let id = task.id;
        let messages = self.task_messages(id)?;
        let images=self.conn.prepare("SELECT id,name,media_type,bytes FROM images WHERE task_id=? ORDER BY id")?.query_map([id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"media_type":r.get::<_,String>(2)?,"bytes":r.get::<_,i64>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let events=self.conn.prepare("SELECT session,action,created_at FROM events WHERE task_id=? ORDER BY id")?.query_map([id],|r|Ok(json!({"session":r.get::<_,String>(0)?,"action":r.get::<_,String>(1)?,"created_at":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(
            json!({"task":task,"messages":messages,"images":images,"events":events,"herdr":self.link(id)?}),
        )
    }
    pub fn task_messages(&self, id: i64) -> Result<Vec<TaskMessage>> {
        Ok(self
            .conn
            .prepare("SELECT id,body,session,created_at FROM messages WHERE task_id=? ORDER BY id")?
            .query_map([id], |row| {
                Ok(TaskMessage {
                    id: row.get(0)?,
                    body: row.get(1)?,
                    session: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    fn save_images(
        conn: &Connection,
        store: &ImageStore,
        pending: &mut PendingFiles,
        id: i64,
        images: &[ImageInput],
    ) -> Result<Vec<i64>> {
        let mut ids = Vec::with_capacity(images.len());
        for image in images {
            let media_type = image.media_type()?;
            conn.execute(
                "INSERT INTO images(task_id,name,media_type,bytes) VALUES (?,?,?,?)",
                params![id, image.name, media_type, image.data.len() as i64],
            )?;
            let image_id = conn.last_insert_rowid();
            store.write(pending, id, image_id, media_type, &image.data)?;
            ids.push(image_id);
        }
        Ok(ids)
    }
    pub fn image_export(&self, task_id: i64, id: i64, path: &Path) -> Result<Value> {
        use std::io::Write;
        self.task(task_id)?;
        let (media_type, bytes): (String, i64) = self
            .conn
            .query_row(
                "SELECT media_type,bytes FROM images WHERE id=? AND task_id=?",
                [id, task_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .context("Image not found")?;
        let data = self.image_store.read(task_id, id, &media_type, bytes)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| {
                format!(
                    "Cannot create {}; destination must not exist",
                    path.display()
                )
            })?;
        file.write_all(&data)?;
        Ok(json!({"id":id,"path":path,"bytes":data.len()}))
    }
    fn save_link(conn: &Connection, id: i64, link: &crate::herdr::Link) -> Result<()> {
        let claim_key = conn.query_row("SELECT claim_key FROM tasks WHERE id=?", [id], |row| {
            row.get(0)
        })?;
        let stored = StoredLink {
            link: link.clone(),
            claim_key,
        };
        conn.execute("INSERT INTO herdr_links(task_id,link_json) VALUES (?,?) ON CONFLICT(task_id) DO UPDATE SET link_json=excluded.link_json",params![id,serde_json::to_string(&stored)?])?;
        Ok(())
    }
    pub fn set_link_with_identity(
        &mut self,
        id: i64,
        link: &crate::herdr::Link,
        overrides: &crate::identity::Identity,
        expected_claim: Option<&str>,
    ) -> Result<()> {
        overrides.validate()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let key: Option<String> = tx
            .query_row("SELECT claim_key FROM tasks WHERE id=?", [id], |row| {
                row.get(0)
            })
            .optional()?
            .with_context(|| crate::errors::Info::missing_task(id))?;
        if let Some(expected) = expected_claim {
            ensure!(
                key.as_deref() == Some(expected),
                crate::errors::Info::new(
                    crate::errors::Code::OwnershipMismatch,
                    format!("Task {id} is no longer claimed by expected dispatch owner")
                )
                .detail("task_id", id)
                .human(format!(
                    "Task {id} is no longer claimed by dispatch session {expected}"
                ))
            );
        }
        Self::save_link(&tx, id, link)?;
        if let Some(key) = key {
            let mut identity = crate::identity::Identity::for_claim(&key, Some(link));
            identity.overlay(overrides);
            Self::save_identity(&tx, id, &identity)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn link(&self, id: i64) -> Result<Option<crate::herdr::Link>> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT link_json FROM herdr_links WHERE task_id=?",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        text.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
    }
}
