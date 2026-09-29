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
        }) || arg == "--json"
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

pub fn expand(mut args: Vec<OsString>) -> Result<Vec<OsString>> {
    let cli = crate::Cli::command();
    let builtin =
        |name: &str| name == "help" || cli.get_subcommands().any(|cmd| cmd.get_name() == name);
    let Some(index) = command_index(&args) else {
        return Ok(args);
    };
    if builtin(args[index].to_str().expect("command_index checked UTF-8")) {
        return Ok(args);
    }
    let config = crate::config::load()?;
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
            bail!("Alias cycle detected at '{name}'");
        }
        if value.trim_start().starts_with('!') {
            bail!("Shell aliases are not supported: '{name}'");
        }
        let words =
            shlex::split(value).with_context(|| format!("Invalid quoting in alias '{name}'"))?;
        if words.is_empty() {
            bail!("Alias '{name}' is empty");
        }
        let mut expanded = vec![args[0].clone()];
        expanded.extend(words.into_iter().map(OsString::from));
        if command_index(&expanded).is_none() {
            bail!("Alias '{name}' must contain a qqq command");
        }
        args.splice(index..=index, expanded.into_iter().skip(1));
    }
    Ok(args)
}
