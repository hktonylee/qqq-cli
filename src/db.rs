use crate::images::ImageInput;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub const DB_NAME: &str = "qqq.db";
pub struct Db {
    pub conn: Connection,
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
#[derive(Serialize)]
pub struct Task {
    pub id: i64,
    pub description: String,
    pub status: String,
    #[serde(flatten)]
    pub identity: crate::identity::Identity,
    pub created_at: String,
    pub updated_at: String,
    pub parent_id: Option<i64>,
}
fn task_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        description: r.get(1)?,
        status: r.get(2)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        parent_id: r.get(6)?,
        identity: crate::identity::Identity {
            harness_name: r.get(7)?,
            harness_session: r.get(8)?,
            orchestrator_name: r.get(9)?,
            orchestrator_session: r.get(10)?,
        },
    })
}
pub fn nonempty(value: &str, name: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{name} must not be empty");
    Ok(())
}
fn ensure_description_schema(conn: &Connection) -> Result<()> {
    ensure!(
        !conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tasks') WHERE name='title')",
            [],
            |r| r.get::<_, bool>(0)
        )?,
        "Database contains legacy title column; update SQLite manually before using qqq"
    );
    Ok(())
}
impl Db {
    pub fn open(init: bool) -> Result<(Self, PathBuf)> {
        let cwd = std::env::current_dir()?;
        let path = if init {
            cwd.join(DB_NAME)
        } else {
            cwd.ancestors()
                .map(|p| p.join(DB_NAME))
                .find(|p| p.is_file())
                .context("No qqq.db found; run qqq init in project root")?
        };
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | if init {
                OpenFlags::SQLITE_OPEN_CREATE
            } else {
                OpenFlags::empty()
            };
        let mut conn = Connection::open_with_flags(&path, flags)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(
            (1..=5).contains(&version) || (init && version == 0),
            "Unsupported database schema version {version}"
        );
        ensure_description_schema(&conn)?;
        if version < 5 {
            // Rebuild CHECK constraints without changing references to tasks.
            // SQLite requires foreign_keys to change outside a transaction.
            conn.pragma_update(None, "foreign_keys", "OFF")?;
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // Another CLI may have migrated while we waited for the write lock.
            let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
            ensure!(
                (1..=5).contains(&version) || (init && version == 0),
                "Unsupported database schema version {version}"
            );
            ensure_description_schema(&tx)?;
            if version == 0 {
                tx.execute_batch(include_str!("schema.sql"))?;
            }
            if version < 2 {
                tx.execute_batch(include_str!("migrate_v2.sql"))?;
            }
            if version < 3 {
                tx.execute_batch(include_str!("migrate_v3.sql"))?;
            }
            if version < 4 {
                tx.execute_batch(include_str!("migrate_v4.sql"))?;
                ensure!(
                    !tx.prepare("PRAGMA foreign_key_check")?.exists([])?,
                    "Database migration found invalid foreign key references"
                );
            }
            if version < 5 {
                tx.execute_batch(include_str!("migrate_v5.sql"))?;
            }
            tx.commit()?;
            conn.pragma_update(None, "foreign_keys", "ON")?;
        }
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(
            version == 5,
            "Unsupported database schema version {version}"
        );
        Ok((Self { conn }, path))
    }
    pub fn task(&self, id: i64) -> Result<Task> {
        self.conn.query_row("SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session FROM tasks WHERE id=?",[id],task_row).optional()?.with_context(||format!("Task {id} not found"))
    }
    pub fn add(
        &mut self,
        description: &str,
        parent_id: Option<i64>,
        images: &[ImageInput],
    ) -> Result<Task> {
        nonempty(description, "Description")?;
        if let Some(id) = parent_id {
            self.task(id)?;
        }
        for image in images {
            image.media_type()?;
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO tasks(description,parent_id) VALUES (?,?)",
            params![description, parent_id],
        )?;
        let id = tx.last_insert_rowid();
        Self::save_images(&tx, id, images)?;
        let task = tx.query_row("SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session FROM tasks WHERE id=?", [id], task_row)?;
        tx.commit()?;
        Ok(task)
    }
    pub fn save_composition(
        &mut self,
        id: Option<i64>,
        parent: Option<i64>,
        draft: &crate::tui::draft::Composition,
    ) -> Result<Task> {
        match id {
            Some(id) => self.edit(id, Some(&draft.description), None, &draft.images, None),
            None => self.add(&draft.description, parent, &draft.images),
        }
    }
    pub fn list(&self, max_completed: Option<i64>) -> Result<Vec<Task>> {
        Ok(self.conn.prepare(
            "SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session FROM tasks
             WHERE ?1 IS NULL OR status!='completed' OR id IN (
                 SELECT id FROM tasks WHERE status='completed'
                 ORDER BY (SELECT MAX(id) FROM events WHERE task_id=tasks.id AND action='complete') DESC,
                          updated_at DESC, id DESC
                 LIMIT COALESCE(?1,-1)
             )
             ORDER BY id"
        )?.query_map([max_completed],task_row)?.collect::<rusqlite::Result<_>>()?)
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
        if let Some(description) = description {
            nonempty(description, "Description")?;
        }
        for image in images {
            image.media_type()?;
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(parent) = parent {
            let parent_id = match parent {
                ParentChange::Clear => None,
                ParentChange::Set(parent_id) => {
                    ensure!(parent_id != id, "Task {id} cannot depend on itself");
                    ensure!(
                        tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?)",
                            [parent_id],
                            |row| row.get::<_, bool>(0)
                        )?,
                        "Task {parent_id} not found"
                    );
                    let cycle: bool = tx.query_row(
                        "WITH RECURSIVE ancestors(id,parent_id) AS (
                            SELECT id,parent_id FROM tasks WHERE id=?1
                            UNION
                            SELECT tasks.id,tasks.parent_id FROM tasks JOIN ancestors ON tasks.id=ancestors.parent_id
                         )
                         SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)
                            OR NOT EXISTS(SELECT 1 FROM ancestors WHERE parent_id IS NULL)",
                        params![parent_id, id],
                        |row| row.get(0),
                    )?;
                    ensure!(
                        !cycle,
                        "Parent {parent_id} would create a dependency cycle for task {id}"
                    );
                    Some(parent_id)
                }
            };
            ensure!(
                tx.execute(
                    "UPDATE tasks SET parent_id=? WHERE id=?",
                    params![parent_id, id]
                )? == 1,
                "Task {id} not found"
            );
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
                    "Task {id} is not in progress or error"
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
                    "Task {id} is no longer in error; inspect its current status before retrying"
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
                ensure!(tx.execute("UPDATE tasks SET status='error',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL WHERE id=? AND status='in_progress' AND claim_key=?",params![id,session])?==1,"Task {id} is not claimed by session {session}");
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
        ensure!(tx.execute("UPDATE tasks SET description=COALESCE(?,description),updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",params![description,id])?==1,"Task {id} not found");
        Self::save_images(&tx, id, images)?;
        let task = tx.query_row(
            "SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session FROM tasks WHERE id=?",
            [id], task_row,
        )?;
        tx.commit()?;
        Ok(task)
    }
    pub fn resolve_task_id(&self, reference: i64) -> Result<i64> {
        ensure!(
            reference != 0,
            "Task reference must be a positive ID or negative creation index"
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
            .with_context(|| format!("No task at recent creation index {reference}"))
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
    fn release_claim(conn: &Connection, id: i64, session: &str) -> Result<()> {
        nonempty(session, "Session")?;
        ensure!(conn.execute("UPDATE tasks SET status='new',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL WHERE id=? AND status='in_progress' AND claim_key=?",params![id,session])?==1,"Task {id} is not claimed by session {session}");
        conn.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?,?,'release')",
            params![id, session],
        )?;
        Ok(())
    }
    fn owner_key(conn: &Connection, session: &str, harness_name: Option<&str>) -> Result<String> {
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
            "Multiple active claims match harness session {session}; use --harness-name or original --session token"
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
        Ok(self.conn.query_row(
            "SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session FROM tasks WHERE status='in_progress' AND claim_key=?",
            [session], task_row,
        ).optional()?)
    }
    pub fn has_ready(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE status='new' AND
                (parent_id IS NULL OR EXISTS(SELECT 1 FROM tasks parent WHERE parent.id=tasks.parent_id AND parent.status='completed')))",
            [], |row| row.get(0),
        )?)
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
        self.claim(session, link, metadata, overrides, true)
    }
    /// Retrieve an existing assignment without claiming queued work if it vanished.
    pub fn owned_with_identity(
        &mut self,
        session: &str,
        overrides: &crate::identity::Identity,
    ) -> Result<Option<Task>> {
        self.claim(session, None, None, overrides, false)
    }
    fn claim(
        &mut self,
        session: &str,
        link: Option<&crate::herdr::Link>,
        metadata: Option<&crate::identity::Identity>,
        overrides: &crate::identity::Identity,
        allow_new: bool,
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
            None if allow_new => tx
                .query_row(
                    "SELECT id FROM tasks WHERE status='new'
                     AND (parent_id IS NULL OR EXISTS
                         (SELECT 1 FROM tasks parent WHERE parent.id=tasks.parent_id AND parent.status='completed'))
                     ORDER BY id LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?,
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
                tx.query_row("SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session FROM tasks WHERE id=?", [id], task_row)?.identity
            } else {
                metadata
                    .cloned()
                    .unwrap_or_else(|| crate::identity::Identity::for_claim(&session, link))
            };
            identity.overlay(overrides);
            Self::save_identity(&tx, id, &identity)?;
        }
        let task = id.map(|id| tx.query_row(
            "SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session FROM tasks WHERE id=?",
            [id], task_row,
        )).transpose()?;
        tx.commit()?;
        Ok(task)
    }
    pub fn complete(&mut self, id: i64, session: &str, harness_name: Option<&str>) -> Result<Task> {
        nonempty(session, "Session")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let session = Self::owner_key(&tx, session, harness_name)?;
        ensure!(tx.execute("UPDATE tasks SET status='completed',claim_key=NULL,harness_name=NULL,harness_session=NULL,orchestrator_name=NULL,orchestrator_session=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status='in_progress' AND claim_key=?",params![id,session])?==1,"Task {id} is not claimed by session {session}");
        tx.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?, ?, 'complete')",
            params![id, session],
        )?;
        let task = tx.query_row(
            "SELECT id,description,status,claim_key,created_at,updated_at,parent_id,harness_name,harness_session,orchestrator_name,orchestrator_session FROM tasks WHERE id=?",
            [id], task_row,
        )?;
        tx.commit()?;
        Ok(task)
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
        let task = self.task(id)?;
        let messages=self.conn.prepare("SELECT id,body,session,created_at FROM messages WHERE task_id=? ORDER BY id")?.query_map([id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"body":r.get::<_,String>(1)?,"session":r.get::<_,Option<String>>(2)?,"created_at":r.get::<_,String>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let images=self.conn.prepare("SELECT id,name,media_type,length(data) FROM images WHERE task_id=? ORDER BY id")?.query_map([id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"media_type":r.get::<_,String>(2)?,"bytes":r.get::<_,i64>(3)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let events=self.conn.prepare("SELECT session,action,created_at FROM events WHERE task_id=? ORDER BY id")?.query_map([id],|r|Ok(json!({"session":r.get::<_,String>(0)?,"action":r.get::<_,String>(1)?,"created_at":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(
            json!({"task":task,"messages":messages,"images":images,"events":events,"herdr":self.link(id)?}),
        )
    }
    fn save_images(conn: &Connection, id: i64, images: &[ImageInput]) -> Result<()> {
        for image in images {
            conn.execute(
                "INSERT INTO images(task_id,name,media_type,data) VALUES (?,?,?,?)",
                params![id, image.name, image.media_type()?, image.data],
            )?;
        }
        Ok(())
    }
    pub fn image_export(&self, task_id: i64, id: i64, path: &Path) -> Result<Value> {
        use std::io::Write;
        self.task(task_id)?;
        let data: Vec<u8> = self
            .conn
            .query_row(
                "SELECT data FROM images WHERE id=? AND task_id=?",
                [id, task_id],
                |r| r.get(0),
            )
            .optional()?
            .context("Image not found")?;
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
        conn.execute("INSERT INTO herdr_links(task_id,link_json) VALUES (?,?) ON CONFLICT(task_id) DO UPDATE SET link_json=excluded.link_json",params![id,serde_json::to_string(link)?])?;
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
            .with_context(|| format!("Task {id} not found"))?;
        if let Some(expected) = expected_claim {
            ensure!(
                key.as_deref() == Some(expected),
                "Task {id} is no longer claimed by dispatch session {expected}"
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
