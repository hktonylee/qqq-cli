use crate::{
    db::{Db, nonempty},
    herdr::{self, Link},
    identity::Identity,
};
use anyhow::{Context, Result};
use std::{env, path::Path};

/// Resolved ownership input, with separate public metadata for native clients.
pub struct Owner {
    pub key: String,
    pub link: Option<Link>,
    pub metadata: Option<Identity>,
}

/// Output defaults use caller context without resolving ownership or opening a DB.
pub fn is_agent_caller() -> bool {
    native_agent_context() || herdr::caller_is_agent()
}

/// Environment-only output hint; never probes process or orchestration state.
pub fn native_agent_context() -> bool {
    [
        "CODEX_THREAD_ID",
        "CODEX_SESSION_ID",
        "CLAUDE_CODE_SESSION_ID",
    ]
    .iter()
    .any(|name| env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

fn env_session(name: &str) -> Result<Option<String>> {
    let value = match env::var(name) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                crate::errors::Info::invalid_argument("--session", format!("{name} must be UTF-8"))
                    .detail("environment_variable", name)
            });
        }
    };
    nonempty(&value, name)?;
    Ok(Some(value))
}

fn native() -> Result<Option<Owner>> {
    if herdr::has_context() {
        return Ok(None);
    }
    for (agent, name) in [
        ("codex", "CODEX_THREAD_ID"),
        ("codex", "CODEX_SESSION_ID"),
        ("claude", "CLAUDE_CODE_SESSION_ID"),
    ] {
        let Some(value) = env_session(name)? else {
            continue;
        };
        let display = if agent == "codex" {
            env_session("CODEX_SESSION_ID")?.unwrap_or_else(|| value.clone())
        } else {
            value.clone()
        };
        return Ok(Some(Owner {
            key: serde_json::to_string(&(agent, "id", &value))?,
            link: None,
            metadata: Some(Identity {
                harness_name: Some(agent.into()),
                harness_session: Some(display),
                ..Identity::default()
            }),
        }));
    }
    Ok(None)
}

pub fn owner(explicit: Option<&str>, project_dir: &Path, db: &Db) -> Result<Owner> {
    if explicit.is_none()
        && let Some(owner) = native()?
    {
        return Ok(owner);
    }
    let (key, link) = herdr::owner(explicit, project_dir, db)?;
    let display = match link.as_ref().map(|link| link.identity.agent.as_str()) {
        Some("codex") => env_session("CODEX_SESSION_ID")?,
        Some("claude") => env_session("CLAUDE_CODE_SESSION_ID")?,
        _ => None,
    };
    let metadata = display.map(|display| {
        let mut identity = Identity::for_claim(&key, link.as_ref());
        identity.harness_session = Some(display);
        identity
    });
    Ok(Owner {
        key,
        link,
        metadata,
    })
}
