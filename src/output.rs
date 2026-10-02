mod detail;

use crate::{Commands, HerdrCommand};
use serde_json::Value;
use std::collections::HashMap;

pub enum Format {
    ConfigList,
    ConfigGet,
    ConfigSet,
    ConfigUnset,
    Database,
    AddedTask,
    Task,
    Tasks,
    Detail,
    Message,
    Link,
    Pane,
}

impl From<&Commands> for Format {
    fn from(command: &Commands) -> Self {
        match command {
            Commands::Config { list: true, .. } => Self::ConfigList,
            Commands::Config { unset: Some(_), .. } => Self::ConfigUnset,
            Commands::Config { value: Some(_), .. } => Self::ConfigSet,
            Commands::Config { .. } => Self::ConfigGet,
            Commands::Init => Self::Database,
            Commands::List { .. } => Self::Tasks,
            Commands::Show { .. } => Self::Detail,
            Commands::Message { .. } => Self::Message,
            Commands::Herdr { command } => match command {
                HerdrCommand::Link { .. } => Self::Link,
                HerdrCommand::Find { .. } => Self::Pane,
            },
            Commands::Add { .. } => Self::AddedTask,
            Commands::Edit { .. } | Commands::Next { .. } | Commands::Complete { .. } => Self::Task,
        }
    }
}

pub fn color_enabled(terminal: bool) -> bool {
    terminal
        && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
        && std::env::var_os("TERM").is_none_or(|value| value != "dumb")
}

// Keep user text on one line and prevent terminal control sequences in text output.
fn clean(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() {
            result.extend(c.escape_default());
        } else {
            result.push(c);
        }
    }
    result
}

fn field(value: &Value, key: &str) -> String {
    match &value[key] {
        Value::String(text) => clean(text),
        Value::Number(number) => number.to_string(),
        _ => "-".to_owned(),
    }
}

fn block(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or("")
        .lines()
        .map(|line| format!("  {}", clean(line)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn status(value: &Value) -> &str {
    match value["status"].as_str() {
        Some("new") => "New",
        Some("in_progress") => "In progress",
        Some("completed") => "Completed",
        Some("error") => "Error",
        _ => "Unknown",
    }
}

fn status_color(value: &Value, color: bool) -> Option<&'static str> {
    match value["status"].as_str() {
        Some("in_progress") if color => Some("36"),
        Some("completed") if color => Some("90"),
        Some("error") if color => Some("31"),
        _ => None,
    }
}

fn styled(text: &str, code: Option<&str>) -> String {
    match code {
        Some(code) => format!("\x1b[{code}m{text}\x1b[0m"),
        None => text.to_owned(),
    }
}

fn parent(value: &Value) -> String {
    value["parent_id"]
        .as_i64()
        .map_or_else(|| "-".to_owned(), |id| format!("#{id}"))
}

fn task(value: &Value, color: bool, assignment: bool) -> String {
    let heading = format!("#{}", field(value, "id"));
    let mut result = format!(
        "{}\nStatus: {}\nParent: {}",
        styled(&heading, color.then_some("1")),
        styled(status(value), status_color(value, color)),
        parent(value)
    );
    if assignment {
        result.push_str(&format!(
            "\nHarness name: {}\nHarness session: {}\nOrchestrator name: {}\nOrchestrator session: {}",
            field(value, "harness_name"),
            field(value, "harness_session"),
            field(value, "orchestrator_name"),
            field(value, "orchestrator_session")
        ));
    }
    result.push_str(&format!(
        "\nCreated: {}\nUpdated: {}",
        field(value, "created_at"),
        field(value, "updated_at")
    ));
    if value["description"]
        .as_str()
        .is_some_and(|text| !text.is_empty())
    {
        result.push_str(&format!(
            "\n\n{}\n{}",
            styled("Description:", color.then_some("1")),
            block(value, "description")
        ));
    }
    result
}

fn pane(value: &Value) -> String {
    let mut result = format!(
        "Workspace: {}\nTab: {}\nPane: {}",
        field(value, "workspace_id"),
        field(value, "tab_id"),
        field(value, "pane_id")
    );
    for (key, label) in [
        ("cwd", "Directory"),
        ("foreground_cwd", "Foreground directory"),
    ] {
        if !value[key].is_null() {
            result.push_str(&format!("\n{label}: {}", field(value, key)));
        }
    }
    if !value["agent_session"].is_null() {
        result.push_str(&format!("\n{}", identity(&value["agent_session"])));
    }
    result
}

fn identity(value: &Value) -> String {
    format!(
        "Agent: {}\nSession ({}): {}",
        field(value, "agent"),
        field(value, "kind"),
        field(value, "value")
    )
}

fn link(value: &Value, color: bool) -> String {
    let heading = styled("Herdr:", color.then_some("1"));
    if value.is_null() {
        return format!("{heading} Not linked");
    }
    format!(
        "{heading}\n{}\nServer: {}\n{}",
        identity(&value["identity"]),
        if value["server"].is_null() {
            "Default".to_owned()
        } else {
            field(value, "server")
        },
        pane(&value["pane"])
    )
}

fn task_tree(tasks: &[Value], color: bool) -> String {
    if tasks.is_empty() {
        return "No tasks yet.".to_owned();
    }
    let positions: HashMap<_, _> = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| (task["id"].as_i64().expect("task ID is an integer"), index))
        .collect();
    let mut children = vec![Vec::new(); tasks.len()];
    let mut roots = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        match task["parent_id"].as_i64().and_then(|id| positions.get(&id)) {
            Some(&parent) => children[parent].push(index),
            None => roots.push(index),
        }
    }

    // DB list order is creation order, so adjacency lists preserve root/sibling order.
    // Explicit stack avoids recursive calls for long dependency chains.
    let mut stack: Vec<(usize, usize, bool)> = roots
        .into_iter()
        .rev()
        .map(|index| (index, 0, true))
        .collect();
    let mut continuations = Vec::new();
    let mut lines = vec![format!("{:<6} {:<12} TASK", "ID", "STATUS")];
    while let Some((index, depth, last)) = stack.pop() {
        continuations.truncate(depth.saturating_sub(1));
        let mut prefix: String = continuations
            .iter()
            .map(|&continues| if continues { "│   " } else { "    " })
            .collect();
        if depth > 0 {
            prefix.push_str(if last { "└── " } else { "├── " });
            continuations.push(!last);
        }
        let task = &tasks[index];
        let row = format!(
            "{:<6} {:<12} {prefix}{}",
            field(task, "id"),
            status(task),
            clean(
                task["description"]
                    .as_str()
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
            )
        );
        lines.push(styled(&row, status_color(task, color)));
        for (position, &child) in children[index].iter().enumerate().rev() {
            stack.push((child, depth + 1, position + 1 == children[index].len()));
        }
    }
    lines.join("\n")
}

pub fn render(format: Format, value: &Value, color: bool) -> String {
    match format {
        Format::ConfigList => {
            let values = value.as_object().expect("config list is an object");
            if values.is_empty() {
                "No config values set.".to_owned()
            } else {
                values
                    .iter()
                    .map(|(key, value)| format!("{}={}", clean(key), config_value(value)))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        Format::ConfigGet => config_value(value),
        Format::ConfigSet => format!("Set {}.", field(value, "key")),
        Format::ConfigUnset => format!("Unset {}.", field(value, "key")),
        Format::Database => format!("Database: {}", field(value, "database")),
        Format::Task if value.is_null() => "No ready tasks.".to_owned(),
        Format::AddedTask => match value.as_array() {
            Some(tasks) => tasks
                .iter()
                .map(|item| task(item, false, false))
                .collect::<Vec<_>>()
                .join("\n\n"),
            None => task(value, false, false),
        },
        Format::Task => task(value, false, true),
        Format::Tasks => task_tree(value.as_array().expect("task list is an array"), color),
        Format::Detail => detail::render(value, color),
        Format::Message => format!(
            "Message #{}\nTask: #{}\nAuthor: {}\n{}",
            field(value, "id"),
            field(value, "task_id"),
            field(value, "session"),
            block(value, "body")
        ),
        Format::Link => link(value, false),
        Format::Pane => pane(value),
    }
}

fn config_value(value: &Value) -> String {
    match value {
        Value::String(text) => clean(text),
        other => clean(&other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{Format, render};
    use serde_json::json;

    #[test]
    fn error_rows_use_red_only_when_color_enabled() {
        let tasks = json!([{"id":1,"description":"Failure","status":"error","parent_id":null}]);
        let colored = render(Format::Tasks, &tasks, true);
        assert!(colored.contains("\x1b[31m1"));
        assert!(colored.contains("Error"));
        let plain = render(Format::Tasks, &tasks, false);
        assert!(plain.contains("Error"));
        assert!(!plain.contains('\x1b'));
    }

    #[test]
    fn interactive_add_prints_each_saved_task_in_human_output() {
        let tasks = json!([
            {"id":1,"description":"First","status":"new","parent_id":null},
            {"id":2,"description":"Second","status":"new","parent_id":null},
        ]);
        let output = render(Format::AddedTask, &tasks, false);
        assert!(output.contains("#1\nStatus: New"));
        assert!(output.contains("\n\n#2\nStatus: New"));
        assert!(output.contains("  First"));
        assert!(output.contains("  Second"));
    }
}
