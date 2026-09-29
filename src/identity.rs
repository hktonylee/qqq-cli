use crate::{db::nonempty, herdr::Link};
use anyhow::Result;
use serde::Serialize;

/// Public assignment metadata. Ownership tokens are stored separately.
#[derive(Clone, Default, Serialize)]
pub struct Identity {
    pub harness_name: Option<String>,
    pub harness_session: Option<String>,
    pub orchestrator_name: Option<String>,
    pub orchestrator_session: Option<String>,
}
impl Identity {
    pub fn validate(&self) -> Result<()> {
        for (value, name) in [
            (&self.harness_name, "Harness name"),
            (&self.harness_session, "Harness session"),
            (&self.orchestrator_name, "Orchestrator name"),
            (&self.orchestrator_session, "Orchestrator session"),
        ] {
            if let Some(value) = value {
                nonempty(value, name)?;
            }
        }
        Ok(())
    }
    pub fn for_claim(session: &str, link: Option<&Link>) -> Self {
        match link {
            Some(link) => Self {
                harness_name: Some(link.identity.agent.clone()),
                harness_session: Some(link.identity.value.clone()),
                orchestrator_name: Some("herdr".into()),
                orchestrator_session: link.server.clone(),
            },
            None => Self {
                harness_session: Some(session.into()),
                ..Self::default()
            },
        }
    }
    pub fn overlay(&mut self, overrides: &Self) {
        for (target, value) in [
            (&mut self.harness_name, &overrides.harness_name),
            (&mut self.harness_session, &overrides.harness_session),
            (&mut self.orchestrator_name, &overrides.orchestrator_name),
            (
                &mut self.orchestrator_session,
                &overrides.orchestrator_session,
            ),
        ] {
            if let Some(value) = value {
                *target = Some(value.clone());
            }
        }
    }
}
