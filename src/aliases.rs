use crate::errors::{Code, Info};
use anyhow::{Context, Result, bail};
use clap::CommandFactory;
use std::{collections::HashSet, ffi::OsString};

// Only inspect the root command. Leave command arguments and invalid options to Clap.
fn command_index(args: &[OsString]) -> Option<usize> {
    let mut index = 1;
    while let Some(arg) = args.get(index) {
        let arg = arg.to_str()?;
        let takes_value = [
            "--session",
            "--harness-name",
            "--harness-session",
            "--orchestrator-name",
            "--orchestrator-session",
        ];
        if takes_value.contains(&arg) {
            index += 2;
        } else if takes_value.iter().any(|flag| {
            arg.strip_prefix(flag)
                .is_some_and(|rest| rest.starts_with('='))
        }) || matches!(arg, "--json" | "--human")
        {
            index += 1;
        } else if arg.starts_with('-') {
            return None;
        } else {
            return Some(index);
        }
    }
    None
}

pub fn expand(mut args: Vec<OsString>, default_json: bool) -> Result<Vec<OsString>> {
    let cli = crate::Cli::command();
    let builtin =
        |name: &str| name == "help" || cli.get_subcommands().any(|cmd| cmd.get_name() == name);
    let Some(index) = command_index(&args) else {
        return Ok(args);
    };
    if builtin(args[index].to_str().expect("command_index checked UTF-8")) {
        return Ok(args);
    }
    let config = crate::config::load().map_err(crate::cli_error::config_error)?;
    let mut seen = HashSet::new();
    while let Some(index) = command_index(&args) {
        let name = args[index].to_str().expect("command_index checked UTF-8");
        if builtin(name) {
            break;
        }
        let Some(value) = config.alias.get(name) else {
            break;
        };
        if !seen.insert(name.to_owned()) {
            bail!(
                Info::invalid_argument("alias", format!("Alias cycle detected at '{name}'"))
                    .detail("alias", name)
                    .detail("reason", "cycle")
            );
        }
        if value.trim_start().starts_with('!') {
            bail!(
                Info::invalid_argument(
                    "alias",
                    format!("Shell aliases are not supported: '{name}'")
                )
                .detail("alias", name)
                .detail("reason", "shell_alias")
            );
        }
        let words = shlex::split(value).with_context(|| {
            Info::invalid_argument("alias", format!("Invalid quoting in alias '{name}'"))
                .detail("alias", name)
                .detail("reason", "invalid_quoting")
        })?;
        if words.is_empty() {
            bail!(
                Info::new(Code::InvalidArgument, format!("Alias '{name}' is empty"))
                    .detail("alias", name)
                    .detail("reason", "empty_alias")
            );
        }
        let mut expanded = vec![args[0].clone()];
        expanded.extend(words.into_iter().map(OsString::from));
        let mut prospective = args.clone();
        prospective.splice(index..=index, expanded.iter().skip(1).cloned());
        crate::errors::set_json_output(crate::cli_error::requests_json(&prospective, default_json));
        if command_index(&expanded).is_none() {
            bail!(
                Info::invalid_argument(
                    "alias",
                    format!("Alias '{name}' must contain a qqq command")
                )
                .detail("alias", name)
                .detail("reason", "missing_command")
            );
        }
        args = prospective;
    }
    Ok(args)
}
