use crate::db::nonempty;
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
fn call<T: DeserializeOwned>(server: Option<&str>, args: &[&str]) -> Result<T> {
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
pub fn explicit(agent: String, value: String, server: Option<String>) -> Result<Link> {
    nonempty(&agent, "Agent")?;
    nonempty(&value, "Agent session")?;
    let identity = AgentSession {
        agent,
        kind: "id".into(),
        value,
    };
    let pane = find(&identity, server.as_deref())?;
    Ok(Link {
        server,
        identity,
        pane,
    })
}
pub fn current() -> Result<Link> {
    ensure!(
        std::env::var("HERDR_ENV").as_deref() == Ok("1"),
        "No session identity; use --session or QQQ_SESSION (Herdr auto-detection requires HERDR_ENV=1)"
    );
    let pane_id = std::env::var("HERDR_PANE_ID").context("HERDR_PANE_ID missing; use --session")?;
    nonempty(&pane_id, "HERDR_PANE_ID")?;
    let current: Current = call(None, &["pane", "current", "--current"])?;
    let identity=current.pane.agent_session.clone().context("Herdr pane has no agent session identity; use --session, then qqq herdr link when available")?;
    nonempty(&identity.value, "Herdr agent session")?;
    Ok(Link {
        server: None,
        identity,
        pane: current.pane,
    })
}
/// Prefer exact caller context; outside Herdr discover a unique agent at DB root.
pub fn discover(project_dir: &Path) -> Result<Link> {
    if std::env::var("HERDR_ENV").as_deref() == Ok("1") {
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
    let identity = pane.agent_session.clone().context(
        "Matching Herdr agent has no agent session identity; use --session or enable Herdr session reporting"
    )?;
    nonempty(&identity.value, "Herdr agent session")?;
    Ok(Link {
        server: None,
        identity,
        pane,
    })
}
pub fn owner(explicit: Option<&str>, project_dir: &Path) -> Result<(String, Option<Link>)> {
    if let Some(session) = explicit {
        nonempty(session, "Session")?;
        return Ok((session.into(), None));
    }
    let link = discover(project_dir)?;
    // JSON tuple encoding avoids collisions when identity strings contain delimiters.
    let owner = serde_json::to_string(&(
        &link.identity.agent,
        &link.identity.kind,
        &link.identity.value,
    ))?;
    Ok((owner, Some(link)))
}
