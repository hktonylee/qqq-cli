//! Synchronous, evidence-based cleanup before command execution.
use crate::{
    db::{Db, SCHEMA_VERSION, StoredLink},
    herdr::{self, Pane},
};
use anyhow::Result;
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::Duration,
};

pub(crate) const ACTOR: &str = "qqq-preflight";
static CHECKED_PATH: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn already_checked(path: &Path) -> bool {
    CHECKED_PATH.get().is_some_and(|checked| checked == path)
}

pub(crate) fn current_project() -> Result<()> {
    // Preserve missing/corrupt/old-schema inspection behavior; never migrate here.
    let Ok((db, path)) = Db::open_read_only() else {
        return Ok(());
    };
    let deaths = observe(&db.conn)?;
    drop(db);
    if !deaths.is_empty() {
        let mut conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        anyhow::ensure!(
            version == SCHEMA_VERSION,
            "Database schema changed during owner preflight"
        );
        apply(&mut conn, deaths)?;
    }
    let _ = CHECKED_PATH.set(path);
    Ok(())
}

pub(crate) fn run(conn: &mut Connection) -> Result<()> {
    let deaths = observe(conn)?;
    apply(conn, deaths)
}

struct Claim {
    id: i64,
    key: String,
    event: i64,
    link: Option<StoredLink>,
    process: Option<crate::process::Identity>,
}
struct Death {
    claim: Claim,
    reason: String,
}

enum Server {
    Agents(Vec<Pane>),
    Dead,
    Unknown,
}

#[derive(Deserialize)]
struct Sessions {
    sessions: Vec<ServerStatus>,
}
#[derive(Deserialize)]
struct ServerStatus {
    name: String,
    running: bool,
}

fn sessions() -> Option<Sessions> {
    let output = Command::new("herdr")
        .args(["session", "list", "--json"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sessions: Sessions = serde_json::from_slice(&output.stdout).ok()?;
    let mut names = std::collections::HashSet::new();
    if sessions
        .sessions
        .iter()
        .any(|session| session.name.trim().is_empty() || !names.insert(&session.name))
    {
        return None;
    }
    Some(sessions)
}

fn observe(conn: &Connection) -> Result<Vec<Death>> {
    let claims = conn
        .prepare(
            "SELECT tasks.id,tasks.claim_key,COALESCE((SELECT MAX(id) FROM events
         WHERE task_id=tasks.id AND action='claim'),0),herdr_links.link_json,
         claim_processes.claim_key,claim_processes.claim_event_id,claim_processes.process_json
         FROM tasks LEFT JOIN herdr_links ON herdr_links.task_id=tasks.id
         LEFT JOIN claim_processes ON claim_processes.task_id=tasks.id
         WHERE tasks.status='in_progress' ORDER BY tasks.id",
        )?
        .query_map([], |row| {
            let encoded: Option<String> = row.get(3)?;
            let key: String = row.get(1)?;
            let event: i64 = row.get(2)?;
            let process_key: Option<String> = row.get(4)?;
            let process_event: Option<i64> = row.get(5)?;
            let process_json: Option<String> = row.get(6)?;
            Ok(Claim {
                id: row.get(0)?,
                process: if process_key.as_ref() == Some(&key) && process_event == Some(event) {
                    process_json.and_then(|value| serde_json::from_str(&value).ok())
                } else {
                    None
                },
                key,
                event,
                link: encoded.and_then(|value| serde_json::from_str(&value).ok()),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut servers = HashMap::new();
    let mut session_snapshot = None;
    let mut deaths = Vec::new();
    let mut process_snapshot = None;
    for claim in claims {
        if let Some(process) = &claim.process {
            if let Some(snapshot) = process_snapshot
                .get_or_insert_with(crate::process::Snapshot::read)
                .as_ref()
            {
                if !snapshot.same_machine(process) {
                    continue;
                }
                if snapshot.state(process) == crate::process::State::Dead {
                    let reason = format!(
                        "Owner preflight: harness process {} ({}) is no longer alive",
                        process.pid, process.executable
                    );
                    deaths.push(Death { claim, reason });
                    continue;
                }
            }
        }
        let Some(stored) = &claim.link else { continue };
        if stored
            .claim_key
            .as_ref()
            .is_some_and(|key| key != &claim.key)
        {
            continue;
        }
        let Some(server) = stored
            .link
            .server
            .as_deref()
            .filter(|server| !server.trim().is_empty())
        else {
            continue;
        };
        let state =
            servers
                .entry(server.to_owned())
                .or_insert_with(|| match herdr::owner_agents(server) {
                    Ok(agents) => Server::Agents(agents),
                    Err(_) => {
                        let snapshot = session_snapshot.get_or_insert_with(sessions);
                        match snapshot.as_ref() {
                            Some(sessions)
                                if !sessions
                                    .sessions
                                    .iter()
                                    .any(|session| session.name == server && session.running) =>
                            {
                                Server::Dead
                            }
                            _ => Server::Unknown,
                        }
                    }
                });
        let reason = match state {
            Server::Agents(agents)
                if !agents
                    .iter()
                    .any(|pane| herdr::matches_owner(&stored.link, pane)) =>
            {
                Some(format!(
                    "Owner preflight: {} harness {} ({}) is no longer live on Herdr server {server}",
                    stored.link.identity.agent,
                    stored.link.identity.value,
                    stored.link.identity.kind
                ))
            }
            Server::Dead => Some(format!(
                "Owner preflight: Herdr orchestrator server {server} is stopped or absent"
            )),
            _ => None,
        };
        if let Some(reason) = reason {
            deaths.push(Death { claim, reason });
        }
    }
    Ok(deaths)
}

fn apply(conn: &mut Connection, deaths: Vec<Death>) -> Result<()> {
    if deaths.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for Death { claim, reason } in deaths {
        let changed = tx.execute(
            "UPDATE tasks SET status='error',claim_key=NULL,harness_name=NULL,harness_session=NULL,
             orchestrator_name=NULL,orchestrator_session=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE id=?1 AND status='in_progress' AND claim_key=?2
             AND COALESCE((SELECT MAX(id) FROM events WHERE task_id=?1 AND action='claim'),0)=?3",
            params![claim.id, claim.key, claim.event])?;
        if changed == 0 {
            continue;
        }
        tx.execute(
            "INSERT INTO events(task_id,session,action) VALUES (?,?,'error')",
            params![claim.id, ACTOR],
        )?;
        tx.execute(
            "INSERT INTO messages(task_id,body,session) VALUES (?,?,?)",
            params![claim.id, reason, ACTOR],
        )?;
    }
    tx.commit()?;
    Ok(())
}
