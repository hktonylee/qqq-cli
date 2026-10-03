use super::{archived_label, block, field, link, parent, status, status_color, styled};
use serde_json::Value;

fn heading(label: &str, color: bool) -> String {
    styled(&format!("{label}:"), color.then_some("1;36"))
}

fn row(label: &str, value: &str, color: bool) -> String {
    format!(
        "  {}{}  {}",
        styled(label, color.then_some("2")),
        " ".repeat(21_usize.saturating_sub(label.len())),
        styled(value, (color && value == "-").then_some("2"))
    )
}

fn message(value: &Value, color: bool) -> String {
    let body = block(value, "body")
        .lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "  {} · {} · {}\n{body}",
        styled(&format!("#{}", field(value, "id")), color.then_some("1")),
        field(value, "session"),
        styled(&field(value, "created_at"), color.then_some("2"))
    )
}

fn image(value: &Value, color: bool) -> String {
    format!(
        "  {}: {} {}",
        styled(
            &format!("Image #{}", field(value, "id")),
            color.then_some("1")
        ),
        field(value, "name"),
        styled(
            &format!(
                "({}, {} bytes)",
                field(value, "media_type"),
                field(value, "bytes")
            ),
            color.then_some("2")
        )
    )
}

fn event(value: &Value, color: bool) -> String {
    let action = field(value, "action");
    let code = if color {
        match action.as_str() {
            "claim" => Some("36"),
            "release" => Some("33"),
            "complete" => Some("90"),
            "error" => Some("31"),
            _ => None,
        }
    } else {
        None
    };
    format!(
        "  {}  {}  {}",
        styled(&field(value, "created_at"), color.then_some("2")),
        styled(&format!("{action:<8}"), code),
        field(value, "session")
    )
}

fn collection(label: &str, rows: &[Value], color: bool) -> String {
    let title = heading(label, color);
    if rows.is_empty() {
        return format!("{title} {}", styled("None", color.then_some("2")));
    }
    let body = rows
        .iter()
        .map(|value| match label {
            "Messages" => message(value, color),
            "Images" => image(value, color),
            _ => event(value, color),
        })
        .collect::<Vec<_>>()
        .join(if label == "Messages" { "\n\n" } else { "\n" });
    format!(
        "{title} {}\n{body}",
        styled(&format!("({})", rows.len()), color.then_some("2"))
    )
}

pub(super) fn render(value: &Value, color: bool) -> String {
    let task = &value["task"];
    let mut sections = vec![format!(
        "{} · {}",
        styled(&format!("#{}", field(task, "id")), color.then_some("1")),
        styled(status(task), status_color(task, color))
    )];
    if task["description"]
        .as_str()
        .is_some_and(|text| !text.is_empty())
    {
        sections.push(format!(
            "{}\n{}",
            heading("Description", color),
            block(task, "description")
        ));
    }
    sections.push(collection(
        "Messages",
        value["messages"]
            .as_array()
            .expect("detail section is an array"),
        color,
    ));
    if let Some(prerequisites) = task["prerequisites"]
        .as_array()
        .filter(|items| !items.is_empty())
    {
        let rows = prerequisites
            .iter()
            .map(|prerequisite| {
                let state = prerequisite["status"].as_str().unwrap_or("missing");
                let archived = if prerequisite["archived"].as_bool() == Some(true) {
                    " · archived"
                } else {
                    ""
                };
                let blocked = if state == "completed" {
                    ""
                } else {
                    " · blocks claim"
                };
                format!(
                    "  #{} · {state}{archived}{blocked}",
                    field(prerequisite, "id")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("{}\n{rows}", heading("Prerequisites", color)));
    }
    sections.push(format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        heading("Details", color),
        row("Parent:", &parent(task), color),
        row("Priority:", &field(task, "priority"), color),
        row("Archived:", archived_label(task), color),
        row(
            "Created:",
            &styled(&field(task, "created_at"), color.then_some("2")),
            color
        ),
        row(
            "Updated:",
            &styled(&field(task, "updated_at"), color.then_some("2")),
            color
        )
    ));
    let assignment = [
        ("Harness name:", "harness_name"),
        ("Harness session:", "harness_session"),
        ("Orchestrator name:", "orchestrator_name"),
        ("Orchestrator session:", "orchestrator_session"),
    ]
    .iter()
    .map(|(label, key)| row(label, &field(task, key), color))
    .collect::<Vec<_>>()
    .join("\n");
    sections.push(format!("{}\n{assignment}", heading("Assignment", color)));
    for (key, label) in [("images", "Images"), ("events", "History")] {
        sections.push(collection(
            label,
            value[key].as_array().expect("detail section is an array"),
            color,
        ));
    }
    let saved_link = link(&value["herdr"], false);
    sections.push(match saved_link.split_once('\n') {
        Some((_, body)) => format!(
            "{}\n{}",
            heading("Herdr", color),
            body.lines()
                .map(|line| format!("  {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        None => format!(
            "{} {}",
            heading("Herdr", color),
            styled("Not linked", color.then_some("2"))
        ),
    });
    if let Some(export) = value.get("export") {
        sections.push(format!(
            "{} image #{} to {}",
            styled("Exported", color.then_some("32")),
            field(export, "id"),
            field(export, "path")
        ));
    }
    sections.join("\n\n")
}
