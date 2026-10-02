mod detail;

use crate::{Commands, HerdrCommand};
use serde_json::Value;
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
            Commands::Tui => Self::Task,
            Commands::Edit { .. } | Commands::Next { .. } | Commands::Complete { .. } => Self::Task,
        }
    }
}

pub fn color_enabled(terminal: bool) -> bool {
    terminal
        && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
        && std::env::var_os("TERM").is_none_or(|value| value != "dumb")
}

pub fn terminal_columns(terminal: bool) -> Option<usize> {
    if !terminal {
        return None;
    }
    #[cfg(unix)]
    let columns = rustix::termios::tcgetwinsize(std::io::stdout())
        .ok()
        .map(|size| size.ws_col);
    #[cfg(not(unix))]
    let columns = crossterm::terminal::size().ok().map(|(columns, _)| columns);
    columns.map(usize::from).filter(|&columns| columns > 0)
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

fn wrap_line(text: &str, width: usize) -> Vec<String> {
    let mut rows = vec![String::new()];
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let cells = grapheme.width();
        if used > 0 && used + cells > width {
            rows.push(String::new());
            used = 0;
        }
        rows.last_mut()
            .expect("wrapped line always has a row")
            .push_str(grapheme);
        used += cells;
    }
    rows
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
    status_label(value["status"].as_str().unwrap_or(""))
}

pub fn status_label(status: &str) -> &'static str {
    match status {
        "new" => "New",
        "in_progress" => "In progress",
        "completed" => "Completed",
        "error" => "Error",
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

fn task_tree(tasks: &[Value], color: bool, columns: Option<usize>) -> String {
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
        let fields = format!("{:<6} {:<12} ", field(task, "id"), status(task));
        let first_prefix = format!("{fields}{prefix}");
        let continuation_prefix = format!(
            "{}{}",
            " ".repeat(fields.width()),
            continuations
                .iter()
                .map(|&continues| if continues { "│   " } else { "    " })
                .collect::<String>()
        );
        let description = task["description"].as_str().unwrap_or("");
        let context_description = task["context_only"]
            .as_bool()
            .unwrap_or(false)
            .then(|| format!("[context] {description}"));
        let description = context_description.as_deref().unwrap_or(description);
        let available = columns
            .and_then(|width| width.checked_sub(first_prefix.width()))
            .filter(|&width| width > 0);
        let row_color = status_color(task, color);
        if let Some(width) = available {
            let mut first = true;
            for line in description.lines() {
                for part in wrap_line(&clean(line), width) {
                    let padding = if first {
                        &first_prefix
                    } else {
                        &continuation_prefix
                    };
                    lines.push(styled(&format!("{padding}{part}"), row_color));
                    first = false;
                }
            }
            if first {
                lines.push(styled(&first_prefix, row_color));
            }
        } else {
            let first_line = clean(description.lines().next().unwrap_or(""));
            lines.push(styled(&format!("{first_prefix}{first_line}"), row_color));
        }
        for (position, &child) in children[index].iter().enumerate().rev() {
            stack.push((child, depth + 1, position + 1 == children[index].len()));
        }
    }
    lines.join("\n")
}

pub fn render(format: Format, value: &Value, color: bool, columns: Option<usize>) -> String {
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
        Format::Tasks => task_tree(
            value.as_array().expect("task list is an array"),
            color,
            columns,
        ),
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
        let colored = render(Format::Tasks, &tasks, true, None);
        assert!(colored.contains("\x1b[31m1"));
        assert!(colored.contains("Error"));
        let plain = render(Format::Tasks, &tasks, false, None);
        assert!(plain.contains("Error"));
        assert!(!plain.contains('\x1b'));
    }

    #[test]
    fn interactive_add_prints_each_saved_task_in_human_output() {
        let tasks = json!([
            {"id":1,"description":"First","status":"new","parent_id":null},
            {"id":2,"description":"Second","status":"new","parent_id":null},
        ]);
        let output = render(Format::AddedTask, &tasks, false, None);
        assert!(output.contains("#1\nStatus: New"));
        assert!(output.contains("\n\n#2\nStatus: New"));
        assert!(output.contains("  First"));
        assert!(output.contains("  Second"));
    }

    #[test]
    fn tty_list_shows_multiline_descriptions_with_tree_padding() {
        let tasks = json!([
            {"id":1,"description":"Root line\nmore root","status":"new","parent_id":null},
            {"id":2,"description":"First child line\nmore child","status":"new","parent_id":1},
            {"id":3,"description":"Last child\nlast detail","status":"new","parent_id":1}
        ]);
        let rendered = render(Format::Tasks, &tasks, false, Some(40));
        let rows: Vec<_> = rendered.lines().collect();
        let pad = " ".repeat(20);
        assert_eq!(rows.len(), 7, "{rendered}");
        assert_eq!(rows[2], format!("{pad}more root"));
        assert_eq!(rows[4], format!("{pad}│   more child"));
        assert_eq!(rows[6], format!("{pad}    last detail"));
        assert!(rows[3].ends_with("├── First child line"));
        assert!(rows[5].ends_with("└── Last child"));

        let piped = render(Format::Tasks, &tasks, false, None);
        assert_eq!(piped.lines().count(), 4);
        assert!(!piped.contains("more root"));
        assert!(!piped.contains("more child"));

        let narrow = render(Format::Tasks, &tasks, false, Some(20));
        assert_eq!(narrow.lines().count(), 4);
        assert!(!narrow.contains("more root"));
    }

    #[test]
    fn tty_list_colors_every_status_row_after_wrapping() {
        let tasks =
            json!([{"id":1,"description":"First\nSecond","status":"error","parent_id":null}]);
        let rendered = render(Format::Tasks, &tasks, true, Some(40));
        let rows: Vec<_> = rendered.lines().collect();
        assert!(rows[1].starts_with("\x1b[31m1"));
        assert!(rows[2].starts_with("\x1b[31m"));
        assert!(rows[2].ends_with("Second\x1b[0m"));
    }

    #[test]
    fn tty_list_wraps_by_display_cells_and_escapes_controls() {
        let tasks = json!([{
            "id":1,
            "description":"ABCDEFGHIJKLMN\n界界界界界界界\n\nC\t\u{1b}",
            "status":"new",
            "parent_id":null
        }]);
        let rendered = render(Format::Tasks, &tasks, false, Some(32));
        let rows: Vec<_> = rendered.lines().collect();
        let pad = " ".repeat(20);
        assert_eq!(&rows[1][20..], "ABCDEFGHIJKL");
        assert_eq!(rows[2], format!("{pad}MN"));
        assert_eq!(rows[3], format!("{pad}界界界界界界"));
        assert_eq!(rows[4], format!("{pad}界"));
        assert_eq!(rows[5], pad);
        assert!(rendered.contains("C\\t\\u{1b}"));
        assert!(!rendered.contains('\u{1b}'));
        assert!(
            rows.iter()
                .skip(1)
                .all(|row| unicode_width::UnicodeWidthStr::width(*row) <= 32)
        );
    }
}
