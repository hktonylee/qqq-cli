use anyhow::{Context, Result, bail, ensure};
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
#[derive(Serialize)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub status: String,
    pub assignee: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub parent_id: Option<i64>,
}
fn task_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        title: r.get(1)?,
        description: r.get(2)?,
        status: r.get(3)?,
        assignee: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
        parent_id: r.get(7)?,
    })
}
pub fn nonempty(value: &str, name: &str) -> Result<()> {
    ensure!(!value.trim().is_empty(), "{name} must not be empty");
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
            (1..=3).contains(&version) || (init && version == 0),
            "Unsupported database schema version {version}"
        );
        if version < 3 {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // Another CLI may have migrated while we waited for the write lock.
            let version: i64 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
            ensure!(
                (1..=3).contains(&version) || (init && version == 0),
                "Unsupported database schema version {version}"
            );
            if version == 0 {
                tx.execute_batch(include_str!("schema.sql"))?;
            }
            if version < 2 {
                tx.execute_batch(include_str!("migrate_v2.sql"))?;
            }
            if version < 3 {
                tx.execute_batch(include_str!("migrate_v3.sql"))?;
            }
            tx.commit()?;
        }
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        ensure!(
            version == 3,
            "Unsupported database schema version {version}"
        );
        Ok((Self { conn }, path))
    }
    pub fn task(&self, id: i64) -> Result<Task> {
        self.conn.query_row("SELECT id,title,description,status,assignee,created_at,updated_at,parent_id FROM tasks WHERE id=?",[id],task_row).optional()?.with_context(||format!("Task {id} not found"))
    }
    pub fn add(&self, title: &str, description: &str, parent_id: Option<i64>) -> Result<Task> {
        nonempty(title, "Title")?;
        if let Some(id) = parent_id {
            self.task(id)?;
        }
        self.conn.execute(
            "INSERT INTO tasks(title,description,parent_id) VALUES (?,?,?)",
            params![title, description, parent_id],
        )?;
        self.task(self.conn.last_insert_rowid())
    }
    pub fn list(&self) -> Result<Vec<Task>> {
        Ok(self.conn.prepare("SELECT id,title,description,status,assignee,created_at,updated_at,parent_id FROM tasks ORDER BY id")?.query_map([],task_row)?.collect::<rusqlite::Result<_>>()?)
    }
    pub fn edit(
        &mut self,
        id: i64,
        title: Option<&str>,
        description: Option<&str>,
        release_session: Option<&str>,
    ) -> Result<Task> {
        if let Some(title) = title {
            nonempty(title, "Title")?;
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(session) = release_session {
            nonempty(session, "Session")?;
            ensure!(tx.execute("UPDATE tasks SET status='new',assignee=NULL WHERE id=? AND status='in_progress' AND assignee=?",params![id,session])?==1,"Task {id} is not claimed by session {session}");
            tx.execute(
                "INSERT INTO events(task_id,session,action) VALUES (?,?,'release')",
                params![id, session],
            )?;
        }
        ensure!(tx.execute("UPDATE tasks SET title=COALESCE(?,title),description=COALESCE(?,description),updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",params![title,description,id])?==1,"Task {id} not found");
        let task = tx.query_row(
            "SELECT id,title,description,status,assignee,created_at,updated_at,parent_id FROM tasks WHERE id=?",
            [id], task_row,
        )?;
        tx.commit()?;
        Ok(task)
    }
    pub fn resolve_edit_id(&self, reference: i64) -> Result<i64> {
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
    pub fn owned(&self, session: &str) -> Result<Option<Task>> {
        nonempty(session, "Session")?;
        Ok(self.conn.query_row(
            "SELECT id,title,description,status,assignee,created_at,updated_at,parent_id FROM tasks WHERE status='in_progress' AND assignee=?",
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
        nonempty(session, "Session")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let owned: Option<i64> = tx
            .query_row(
                "SELECT id FROM tasks WHERE status='in_progress' AND assignee=?",
                [session],
                |r| r.get(0),
            )
            .optional()?;
        let id = match owned {
            Some(id) => Some(id),
            None => tx
                .query_row(
                    "SELECT id FROM tasks WHERE status='new'
                     AND (parent_id IS NULL OR EXISTS
                         (SELECT 1 FROM tasks parent WHERE parent.id=tasks.parent_id AND parent.status='completed'))
                     ORDER BY id LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?,
        };
        if let Some(id) = id {
            if owned.is_none() {
                tx.execute("DELETE FROM herdr_links WHERE task_id=?", [id])?;
                tx.execute("UPDATE tasks SET status='in_progress',assignee=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",params![session,id])?;
                tx.execute(
                    "INSERT INTO events(task_id,session,action) VALUES (?,?,'claim')",
                    params![id, session],
                )?;
            }
            if let Some(link) = link {
                Self::save_link(&tx, id, link)?;
            }
        }
        let task = id.map(|id| tx.query_row(
            "SELECT id,title,description,status,assignee,created_at,updated_at,parent_id FROM tasks WHERE id=?",
            [id], task_row,
        )).transpose()?;
        tx.commit()?;
        Ok(task)
    }
    pub fn complete(&mut self, id: i64, session: &str) -> Result<Task> {
        nonempty(session, "Session")?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(tx.execute("UPDATE tasks SET status='completed',assignee=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND status='in_progress' AND assignee=?",params![id,session])?==1,"Task {id} is not claimed by session {session}");
        tx.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?, ?, 'complete')",
            params![id, session],
        )?;
        let task = tx.query_row(
            "SELECT id,title,description,status,assignee,created_at,updated_at,parent_id FROM tasks WHERE id=?",
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
    pub fn image_add(&self, id: i64, path: &Path) -> Result<Value> {
        use std::io::Read;
        self.task(id)?;
        let file =
            std::fs::File::open(path).with_context(|| format!("Cannot read {}", path.display()))?;
        ensure!(file.metadata()?.is_file(), "Image must be a regular file");
        let mut data = Vec::new();
        file.take(20 * 1024 * 1024 + 1).read_to_end(&mut data)?;
        ensure!(data.len() <= 20 * 1024 * 1024, "Image exceeds 20 MiB limit");
        let media = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if data.starts_with(b"\xff\xd8\xff") {
            "image/jpeg"
        } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            "image/gif"
        } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            bail!("Unsupported image signature; expected PNG, JPEG, GIF or WebP")
        };
        let name = path
            .file_name()
            .context("Missing image filename")?
            .to_string_lossy();
        self.conn.execute(
            "INSERT INTO images(task_id,name,media_type,data) VALUES (?,?,?,?)",
            params![id, name, media, data],
        )?;
        Ok(
            json!({"id":self.conn.last_insert_rowid(),"task_id":id,"name":name,"media_type":media,"bytes":data.len()}),
        )
    }
    pub fn image_export(&self, id: i64, path: &Path) -> Result<Value> {
        use std::io::Write;
        let data: Vec<u8> = self
            .conn
            .query_row("SELECT data FROM images WHERE id=?", [id], |r| r.get(0))
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
    pub fn set_link(&self, id: i64, link: &crate::herdr::Link) -> Result<()> {
        self.task(id)?;
        Self::save_link(&self.conn, id, link)
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
