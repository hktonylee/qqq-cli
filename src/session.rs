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
    ["CODEX_THREAD_ID", "CODEX_SESSION_ID"]
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
    for name in ["CODEX_THREAD_ID", "CODEX_SESSION_ID"] {
        let Some(value) = env_session(name)? else {
            continue;
        };
        let display = env_session("CODEX_SESSION_ID")?.unwrap_or_else(|| value.clone());
        return Ok(Some(Owner {
            key: serde_json::to_string(&("codex", "id", &value))?,
            link: None,
            metadata: Some(Identity {
                harness_name: Some("codex".into()),
                harness_session: Some(display),
                ..Identity::default()
            }),
        }));
    }
    Ok(None)
}

pub fn owner(explicit: Option<&str>, project_dir: &Path, db: &Db) -> Result<Owner> {
    if explicit.is_none() {
        if let Some(owner) = native()? {
            return Ok(owner);
        }
    }
    let (key, link) = herdr::owner(explicit, project_dir, db)?;
    let metadata = if link
        .as_ref()
        .is_some_and(|link| link.identity.agent == "codex")
    {
        env_session("CODEX_SESSION_ID")?.map(|display| {
            let mut identity = Identity::for_claim(&key, link.as_ref());
            identity.harness_session = Some(display);
            identity
        })
    } else {
        None
    };
    Ok(Owner {
        key,
        link,
        metadata,
    })
}

pub fn dispatch_caller(db: &Db) -> Result<String> {
    match native()? {
        Some(owner) => Ok(owner.key),
        None => herdr::dispatch_caller(db),
    }
}
