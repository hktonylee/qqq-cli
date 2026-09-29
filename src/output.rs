use crate::{Commands, HerdrCommand, ImageCommand};
use serde_json::Value;
use std::collections::HashMap;

pub enum Format {
    Database,
    Task,
    Tasks,
    Detail,
    Message,
    Image,
    Export,
    Link,
    Pane,
}

impl From<&Commands> for Format {
    fn from(command: &Commands) -> Self {
        match command {
            Commands::Init => Self::Database,
            Commands::List { .. } => Self::Tasks,
            Commands::Show { .. } => Self::Detail,
            Commands::Message { .. } => Self::Message,
            Commands::Image { command } => match command {
                ImageCommand::Add { .. } => Self::Image,
                ImageCommand::Export { .. } => Self::Export,
            },
            Commands::Herdr { command } => match command {
                HerdrCommand::Link { .. } => Self::Link,
                HerdrCommand::Find { .. } => Self::Pane,
            },
            Commands::Add { .. }
            | Commands::Edit { .. }
            | Commands::Next { .. }
            | Commands::Complete { .. } => Self::Task,
        }
    }
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
        _ => "Unknown",
    }
}

fn parent(value: &Value) -> String {
    value["parent_id"]
        .as_i64()
        .map_or_else(|| "-".to_owned(), |id| format!("#{id}"))
}

fn task(value: &Value) -> String {
    let mut result = format!(
        "#{} {}\nStatus: {}\nParent: {}\nAssignee: {}\nCreated: {}\nUpdated: {}",
        field(value, "id"),
        field(value, "title"),
        status(value),
        parent(value),
        field(value, "assignee"),
        field(value, "created_at"),
        field(value, "updated_at")
    );
    if value["description"]
        .as_str()
        .is_some_and(|text| !text.is_empty())
    {
        result.push_str(&format!(
            "\n\nDescription:\n{}",
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

fn link(value: &Value) -> String {
    if value.is_null() {
        return "Herdr: Not linked".to_owned();
    }
    format!(
        "Herdr:\n{}\nServer: {}\n{}",
        identity(&value["identity"]),
        if value["server"].is_null() {
            "Default".to_owned()
        } else {
            field(value, "server")
        },
        pane(&value["pane"])
    )
}

fn image(value: &Value) -> String {
    format!(
        "Image #{}: {} ({}, {} bytes)",
        field(value, "id"),
        field(value, "name"),
        field(value, "media_type"),
        field(value, "bytes")
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
    let mut lines = vec![format!("{:<6} {:<12} TITLE", "ID", "STATUS")];
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
            field(task, "title")
        );
        let code = match task["status"].as_str() {
            Some("in_progress") if color => Some("36"),
            Some("completed") if color => Some("90"),
            _ => None,
        };
        lines.push(match code {
            Some(code) => format!("\x1b[{code}m{row}\x1b[0m"),
            None => row,
        });
        for (position, &child) in children[index].iter().enumerate().rev() {
            stack.push((child, depth + 1, position + 1 == children[index].len()));
        }
    }
    lines.join("\n")
}

pub fn render(format: Format, value: &Value, color: bool) -> String {
    match format {
        Format::Database => format!("Database: {}", field(value, "database")),
        Format::Task if value.is_null() => "No ready tasks.".to_owned(),
        Format::Task => task(value),
        Format::Tasks => task_tree(value.as_array().expect("task list is an array"), color),
        Format::Detail => {
            let mut result = task(&value["task"]);
            for (key, label) in [
                ("messages", "Messages"),
                ("images", "Images"),
                ("events", "History"),
            ] {
                result.push_str(&format!("\n\n{label}:"));
                let rows = value[key].as_array().expect("detail section is an array");
                if rows.is_empty() {
                    result.push_str(" None");
                }
                for row in rows {
                    let text = match key {
                        "messages" => format!(
                            "#{} · {} · {}\n{}",
                            field(row, "id"),
                            field(row, "session"),
                            field(row, "created_at"),
                            block(row, "body")
                        ),
                        "images" => image(row),
                        _ => format!(
                            "{} · {} · {}",
                            field(row, "created_at"),
                            field(row, "action"),
                            field(row, "session")
                        ),
                    };
                    result.push_str(&format!("\n  {text}"));
                }
            }
            result.push_str(&format!("\n\n{}", link(&value["herdr"])));
            result
        }
        Format::Message => format!(
            "Message #{}\nTask: #{}\nAuthor: {}\n{}",
            field(value, "id"),
            field(value, "task_id"),
            field(value, "session"),
            block(value, "body")
        ),
        Format::Image => format!("{}\nTask: #{}", image(value), field(value, "task_id")),
        Format::Export => format!(
            "Exported image #{} to {}",
            field(value, "id"),
            field(value, "path")
        ),
        Format::Link => link(value),
        Format::Pane => pane(value),
    }
}
