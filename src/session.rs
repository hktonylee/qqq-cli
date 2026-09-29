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

fn native() -> Result<Option<Owner>> {
    if herdr::has_context() {
        return Ok(None);
    }
    for name in ["CODEX_THREAD_ID", "CODEX_SESSION_ID"] {
        let value = match env::var(name) {
            Ok(value) => value,
            Err(env::VarError::NotPresent) => continue,
            Err(error) => return Err(error).with_context(|| format!("{name} must be UTF-8")),
        };
        nonempty(&value, name)?;
        return Ok(Some(Owner {
            key: serde_json::to_string(&("codex", "id", &value))?,
            link: None,
            metadata: Some(Identity {
                harness_name: Some("codex".into()),
                harness_session: Some(value),
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
    Ok(Owner {
        key,
        link,
        metadata: None,
    })
}

pub fn dispatch_caller(db: &Db) -> Result<String> {
    match native()? {
        Some(owner) => Ok(owner.key),
        None => herdr::dispatch_caller(db),
    }
}
