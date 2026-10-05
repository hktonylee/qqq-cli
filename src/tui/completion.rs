use crate::errors::{Code, Info};
use anyhow::Error;

pub(super) const FORCE_REQUIRED: &str = "Select Force complete before confirming.";

pub(crate) fn discovery_unavailable(error: &Error) -> bool {
    error.downcast_ref::<Info>().is_some_and(|info| {
        matches!(info.code, Code::DispatchError)
            || info
                .details
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|reason| {
                    matches!(
                        reason,
                        "missing_project_agent"
                            | "ambiguous_project_agent"
                            | "missing_herdr_context"
                            | "missing_herdr_pane"
                            | "missing_agent_identity"
                            | "missing_agent_kind"
                    )
                })
    })
}

pub(super) fn can_force_after_error(error: &Error) -> bool {
    discovery_unavailable(error)
        || error
            .downcast_ref::<Info>()
            .is_some_and(|info| matches!(info.code, Code::OwnershipMismatch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_recovery_accepts_discovery_and_owner_failures() {
        for error in [
            Info::new(Code::DispatchError, "Herdr failed").into(),
            Info::invalid_argument("--session", "Missing identity")
                .detail("reason", "missing_agent_identity")
                .into(),
            Info::new(Code::OwnershipMismatch, "Owner changed").into(),
        ] {
            assert!(can_force_after_error(&error));
        }
    }

    #[test]
    fn completion_recovery_preserves_database_and_invalid_input_errors() {
        for error in [
            Error::new(rusqlite::Error::InvalidQuery),
            Info::new(Code::DatabaseError, "DB damaged").into(),
            Info::invalid_argument("--session", "Session must not be empty").into(),
            anyhow::anyhow!("unclassified failure"),
        ] {
            assert!(!can_force_after_error(&error));
        }
    }
}
