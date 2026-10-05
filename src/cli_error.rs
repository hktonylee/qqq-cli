use crate::{
    Cli, Commands,
    db::ContentConflict,
    errors::{Code, Info, Recovery},
};
use anyhow::Error;
use clap::{
    CommandFactory,
    error::{ContextKind, ContextValue},
};
use serde_json::{Value, json};
use std::{ffi::OsString, io::Write};

pub fn requests_json(arguments: &[OsString], default_json: bool) -> bool {
    output_override(arguments).unwrap_or(default_json)
}

pub fn output_override(arguments: &[OsString]) -> Option<bool> {
    let root = Cli::command();
    let mut command = &root;
    let mut index = 1;
    let mut human = false;
    while let Some(value) = arguments.get(index) {
        let value = value.to_string_lossy();
        if value == "--" {
            break;
        }
        if value == "--json" || value.starts_with("--json=") {
            return Some(true);
        }
        if value == "--human" || value.starts_with("--human=") {
            human = true;
        }
        let option = command
            .get_arguments()
            .chain(root.get_arguments().filter(|arg| arg.is_global_set()))
            .find(|arg| {
                arg.get_long()
                    .is_some_and(|long| value == format!("--{long}"))
                    || arg
                        .get_short()
                        .is_some_and(|short| value == format!("-{short}"))
            });
        if let Some(option) = option {
            if option.get_action().takes_values()
                && arguments.get(index + 1).is_some_and(|next| {
                    option.is_allow_hyphen_values_set() || !next.to_string_lossy().starts_with('-')
                })
            {
                index += 2;
                continue;
            }
        } else if let Some(subcommand) = command
            .get_subcommands()
            .find(|child| child.get_name() == value)
        {
            command = subcommand;
        }
        index += 1;
    }
    human.then_some(false)
}

pub fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::Config { .. } => "config",
        Commands::Init => "init",
        Commands::Add { .. } => "add",
        Commands::Import { .. } => "import",
        Commands::Tui { .. } => "tui",
        Commands::List { .. } => "list",
        Commands::Show { .. } => "show",
        Commands::Edit { .. } => "edit",
        Commands::Archive { .. } => "archive",
        Commands::Unarchive { .. } => "unarchive",
        Commands::Delete { .. } => "delete",
        Commands::Next { .. } => "next",
        Commands::Status { .. } => "status",
        Commands::Complete { .. } => "complete",
        Commands::Reopen { .. } => "reopen",
        Commands::Backup { .. } => "backup",
        Commands::Restore { .. } => "restore",
        Commands::Doctor => "doctor",
        Commands::Message { .. } => "message",
        Commands::Herdr { .. } => "herdr",
    }
}

pub fn parser_payload(error: &clap::Error) -> Value {
    use clap::error::ErrorKind;
    let argument = match error.get(ContextKind::InvalidArg) {
        Some(ContextValue::String(argument)) => argument
            .split_whitespace()
            .next()
            .filter(|arg| arg.starts_with('-') || arg.starts_with('<'))
            .map(|arg| arg.split('=').next().unwrap_or(arg)),
        _ => None,
    };
    let message = match error.kind() {
        ErrorKind::UnknownArgument => argument.map_or("unexpected CLI argument".into(), |arg| {
            format!("unexpected argument '{arg}'")
        }),
        ErrorKind::InvalidSubcommand => match error.get(ContextKind::InvalidSubcommand) {
            Some(ContextValue::String(name)) => format!("unrecognized subcommand '{name}'"),
            _ => "unrecognized subcommand".into(),
        },
        ErrorKind::MissingRequiredArgument => "required arguments were not provided".into(),
        ErrorKind::ArgumentConflict => "CLI arguments cannot be used with each other".into(),
        _ => "Invalid CLI arguments".into(),
    };
    let mut info =
        Info::new(Code::InvalidArgument, message).detail("reason", format!("{:?}", error.kind()));
    if let Some(argument) = argument {
        info = info.detail("argument", argument.to_owned());
    }
    json!({"code": info.code, "message": info.message, "details": info.details})
}

pub fn payload(error: &Error, command: Option<&str>) -> Value {
    let mut info = if let Some(sqlite) = error
        .downcast_ref::<rusqlite::Error>()
        .and_then(rusqlite::Error::sqlite_error)
        .filter(|sqlite| {
            matches!(
                sqlite.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
        }) {
        Info::new(
            Code::DbBusy,
            "Database is busy; retry after current transaction finishes",
        )
        .detail("sqlite_code", sqlite.extended_code & 0xff)
        .detail("sqlite_extended_code", sqlite.extended_code)
    } else if let Some(conflict) = error.downcast_ref::<ContentConflict>() {
        Info::new(Code::ContentConflict, conflict.to_string())
            .detail("task_id", conflict.task_id)
            .detail("expected_revision", conflict.expected_revision)
            .detail(
                "current_revision",
                conflict.current.as_ref().map(|content| content.revision),
            )
            .detail(
                "reason",
                if conflict.current.is_some() {
                    "revision_changed"
                } else {
                    "task_removed"
                },
            )
    } else if let Some(domain) = error.downcast_ref::<Info>() {
        Info::new(domain.code, &domain.message).with_details(&domain.details)
    } else if let Some(sqlite) = error.downcast_ref::<rusqlite::Error>() {
        let mut info = Info::new(Code::DatabaseError, "Database operation failed");
        if let Some(sqlite) = sqlite.sqlite_error() {
            info = info
                .detail("sqlite_code", sqlite.extended_code & 0xff)
                .detail("sqlite_extended_code", sqlite.extended_code);
        }
        info
    } else if let Some(io) = error.downcast_ref::<std::io::Error>() {
        Info::new(Code::IoError, "IO operation failed")
            .detail("io_kind", format!("{:?}", io.kind()))
            .detail("os_code", io.raw_os_error())
    } else {
        Info::new(
            Code::CommandError,
            "Command failed; run without --json for diagnostic context",
        )
    };
    if let Some(command) = command {
        info = info.detail("command", command);
    }
    if let Some(recovery) = error.downcast_ref::<Recovery>() {
        info = info.detail(
            "recovery",
            json!({
                "local_draft": recovery.local_draft,
                "current_text": recovery.current_text,
                "attachments_manifest": recovery.attachments_manifest,
            }),
        );
    }
    json!({"code": info.code, "message": info.message, "details": info.details})
}

pub fn emit(payload: &Value) {
    let mut stderr = std::io::stderr().lock();
    let _ = serde_json::to_writer(&mut stderr, payload);
    let _ = writeln!(stderr);
}

pub fn annotate(error: Error, info: Info) -> Error {
    if error.downcast_ref::<Info>().is_some() || error.downcast_ref::<ContentConflict>().is_some() {
        error
    } else {
        error.context(info)
    }
}

pub fn config_error(error: Error) -> Error {
    let message = error.to_string();
    annotate(
        error,
        Info::new(Code::ConfigError, message)
            .detail("path", json!(crate::config::path()))
            .human("Configuration failed"),
    )
}

pub fn session_error(error: Error) -> Error {
    annotate(
        error,
        Info::invalid_argument(
            "--session",
            "Cannot resolve owner; supply --session or disambiguate harness",
        ),
    )
}

pub fn image_error(error: Error) -> Error {
    if error.downcast_ref::<std::io::Error>().is_some() {
        error
    } else {
        annotate(
            error,
            Info::invalid_argument("--image", "Invalid image attachment"),
        )
    }
}

impl Info {
    fn with_details(mut self, details: &serde_json::Map<String, Value>) -> Self {
        self.details.clone_from(details);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_busy_and_locked_survive_context_without_driver_text() {
        for code in [5, 6] {
            let error = Error::new(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(code),
                Some("private driver detail".into()),
            ))
            .context(Info::new(Code::CommandError, "safe context"));
            let value = payload(&error, Some("edit"));
            assert_eq!(value["code"], "DB_BUSY");
            assert_eq!(value["details"]["sqlite_code"], code);
            assert!(!value.to_string().contains("private"));
        }
    }

    #[test]
    fn public_domain_details_survive_private_human_context() {
        let error = Error::new(
            Info::new(Code::OwnershipMismatch, "Task belongs to another owner")
                .detail("task_id", 1)
                .human("private owner key"),
        )
        .context("private wrapping context");
        let value = payload(&error, Some("complete"));
        assert_eq!(value["code"], "OWNERSHIP_MISMATCH");
        assert_eq!(value["details"]["task_id"], 1);
        assert!(!value.to_string().contains("private"));
    }
}
