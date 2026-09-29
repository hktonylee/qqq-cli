use crate::db::{Db, nonempty};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub agent: String,
    pub kind: String,
    pub value: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub foreground_cwd: Option<PathBuf>,
    #[serde(default)]
    pub agent_session: Option<AgentSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub server: Option<String>,
    pub identity: AgentSession,
    pub pane: Pane,
}
#[derive(Deserialize)]
struct Envelope<T> {
    result: T,
}
#[derive(Deserialize)]
struct Agents {
    agents: Vec<Pane>,
}
#[derive(Deserialize)]
struct Current {
    pane: Pane,
}
pub(crate) fn call<T: DeserializeOwned>(server: Option<&str>, args: &[&str]) -> Result<T> {
    let mut cmd = Command::new("herdr");
    if let Some(server) = server {
        nonempty(server, "Herdr server")?;
        cmd.args(["--session", server]);
    }
    let out = cmd
        .args(args)
        .output()
        .context("Cannot run herdr; install Herdr or provide --session for local ownership")?;
    ensure!(
        out.status.success(),
        "Herdr failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let response: Envelope<T> =
        serde_json::from_slice(&out.stdout).context("Invalid Herdr response")?;
    Ok(response.result)
}
pub fn find(identity: &AgentSession, server: Option<&str>) -> Result<Pane> {
    let agents: Agents = call(server, &["agent", "list"])?;
    let mut matches = agents.agents.into_iter().filter(|pane| {
        if identity.kind == "terminal" {
            return pane.terminal_id.as_deref() == Some(&identity.value)
                && pane.agent.as_deref() == Some(&identity.agent);
        }
        pane.agent_session.as_ref().is_some_and(|s| {
            s.agent == identity.agent && s.kind == identity.kind && s.value == identity.value
        })
    });
    let pane = matches
        .next()
        .context("Linked Herdr agent session is not live")?;
    ensure!(
        matches.next().is_none(),
        "Multiple Herdr panes match agent session; resolve duplicate session reports"
    );
    Ok(pane)
}

pub fn pane_identity(pane: &Pane) -> Result<AgentSession> {
    if let Some(identity) = &pane.agent_session {
        nonempty(&identity.agent, "Herdr agent kind")?;
        nonempty(&identity.kind, "Herdr identity kind")?;
        nonempty(&identity.value, "Herdr agent session")?;
        return Ok(identity.clone());
    }
    let value = pane
        .terminal_id
        .as_deref()
        .context("Herdr agent has no session or terminal identity")?;
    let agent = pane.agent.as_deref().context("Herdr agent kind missing")?;
    nonempty(value, "Herdr terminal identity")?;
    nonempty(agent, "Herdr agent kind")?;
    Ok(AgentSession {
        agent: agent.into(),
        kind: "terminal".into(),
        value: value.into(),
    })
}

fn retained_identity(pane: &Pane, db: &Db) -> Result<AgentSession> {
    let identity = pane_identity(pane)?;
    if let Some(terminal) = &pane.terminal_id {
        // Keep active auto-claims reachable while session reporting changes.
        if let Some(active) = db.active_identity_for_terminal(&identity.agent, terminal)? {
            return Ok(active);
        }
    }
    Ok(identity)
}

pub fn has_context() -> bool {
    std::env::var_os("HERDR_PANE_ID").is_some() || std::env::var("HERDR_ENV").as_deref() == Ok("1")
}

fn current_pane() -> Result<Pane> {
    ensure!(
        has_context(),
        "No exact Herdr context; use --session or QQQ_SESSION"
    );
    let pane_id = std::env::var("HERDR_PANE_ID")
        .context("HERDR_PANE_ID missing or not UTF-8; use --session")?;
    nonempty(&pane_id, "HERDR_PANE_ID")?;
    let current: Current = call(None, &["pane", "current", "--pane", &pane_id])?;
    Ok(current.pane)
}

pub fn dispatch_caller(db: &Db) -> Result<String> {
    let pane = current_pane()?;
    let identity = retained_identity(&pane, db)?;
    Ok(serde_json::to_string(&(
        identity.agent,
        identity.kind,
        identity.value,
    ))?)
}
pub fn explicit(agent: String, value: String, server: Option<String>) -> Result<Link> {
    nonempty(&agent, "Agent")?;
    nonempty(&value, "Agent session")?;
    let identity = AgentSession {
        agent,
        kind: "id".into(),
        value,
    };
    let pane = find(&identity, server.as_deref())?;
    let server = server.or_else(server_session);
    Ok(Link {
        server,
        identity,
        pane,
    })
}
/// Discover named server without failing claims when optional metadata is unavailable.
pub fn server_session() -> Option<String> {
    let Some(socket) = std::env::var_os("HERDR_SOCKET_PATH") else {
        return Some("default".into());
    };
    #[derive(Deserialize)]
    struct Sessions {
        sessions: Vec<ServerSession>,
    }
    #[derive(Deserialize)]
    struct ServerSession {
        name: String,
        socket_path: PathBuf,
    }
    let out = Command::new("herdr")
        .args(["session", "list", "--json"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sessions: Sessions = serde_json::from_slice(&out.stdout).ok()?;
    let socket = PathBuf::from(socket);
    let mut matches = sessions
        .sessions
        .into_iter()
        .filter(|s| s.socket_path == socket);
    let session = matches.next()?;
    if matches.next().is_some() || session.name.trim().is_empty() {
        return None;
    }
    Some(session.name)
}
pub fn current() -> Result<Link> {
    let pane = current_pane()?;
    let identity = pane_identity(&pane).context(
        "Cannot identify current Herdr agent; use --session or enable Herdr session reporting",
    )?;
    Ok(Link {
        server: server_session(),
        identity,
        pane,
    })
}
/// Prefer exact caller context; outside Herdr discover a unique agent at DB root.
pub fn discover(project_dir: &Path) -> Result<Link> {
    if has_context() {
        return current();
    }
    let project_dir = project_dir
        .canonicalize()
        .context("Cannot resolve database directory")?;
    let agents: Agents = call(None, &["agent", "list"])?;
    let mut matches = agents.agents.into_iter().filter(|pane| {
        pane.foreground_cwd
            .as_deref()
            .or(pane.cwd.as_deref())
            .filter(|cwd| cwd.is_absolute())
            .and_then(|cwd| cwd.canonicalize().ok())
            .is_some_and(|cwd| cwd == project_dir)
    });
    let pane = matches.next().with_context(|| {
        format!(
            "No Herdr agent matches database directory {}; use --session or QQQ_SESSION",
            project_dir.display()
        )
    })?;
    ensure!(
        matches.next().is_none(),
        "Multiple Herdr agents match database directory {}; use --session or run inside intended Herdr pane",
        project_dir.display()
    );
    let identity = pane_identity(&pane).context(
        "Cannot identify matching Herdr agent; use --session or enable Herdr session reporting",
    )?;
    Ok(Link {
        server: server_session(),
        identity,
        pane,
    })
}
pub fn owner(
    explicit: Option<&str>,
    project_dir: &Path,
    db: &Db,
) -> Result<(String, Option<Link>)> {
    if let Some(session) = explicit {
        nonempty(session, "Session")?;
        return Ok((session.into(), None));
    }
    let mut link = discover(project_dir)?;
    link.identity = retained_identity(&link.pane, db)?;
    // JSON tuple encoding avoids collisions when identity strings contain delimiters.
    let owner = serde_json::to_string(&(
        &link.identity.agent,
        &link.identity.kind,
        &link.identity.value,
    ))?;
    Ok((owner, Some(link)))
}
