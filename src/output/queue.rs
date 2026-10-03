use super::{clean, field, status_label, styled};
use serde_json::Value;

pub(super) fn render(value: &Value, color: bool, explain: bool) -> String {
    let counts = &value["counts"];
    let mut lines = vec![styled("Queue status", color.then_some("1"))];
    lines.push(format!(
        "Ready: {}  Blocked: {}  In progress: {}  Error: {}  Completed: {}",
        field(counts, "ready"),
        field(counts, "blocked"),
        field(counts, "in_progress"),
        field(counts, "error"),
        field(counts, "completed")
    ));
    lines.push(format!(
        "Total: {}  Archived: {}  Matching ready: {}",
        field(counts, "total"),
        field(counts, "archived"),
        field(counts, "matching_ready")
    ));
    lines.push(
        match value["state"].as_str().unwrap_or("") {
            "empty" => "Queue empty.",
            "ready" => "Ready tasks available.",
            "no_matching_ready" => "Ready tasks exist; none match filter.",
            "blocked" => "No ready tasks; unfinished tasks blocked.",
            "error" => "No ready tasks; unfinished tasks in error.",
            "in_progress" => "No ready tasks; workers own unfinished tasks.",
            _ => "No ready tasks; completed or mixed unavailable tasks remain.",
        }
        .to_owned(),
    );
    if explain {
        let explanation = &value["explanation"];
        lines.push("Order: priority descending, ID ascending on ties.".to_owned());
        if !explanation["owner_input"].is_null() {
            lines.push(format!(
                "Owner context: {} -> {}",
                field(explanation, "owner_input"),
                field(explanation, "resolved_owner")
            ));
        }
        let selection = &explanation["selection"];
        if selection.is_null() {
            lines.push("Selection: none.".to_owned());
        } else if selection["kind"] == "owned" {
            lines.push(format!(
                "Selection: reuse owned task #{} before queue/filter selection.",
                field(&selection["task"], "id")
            ));
        } else {
            lines.push(format!(
                "Selection: queued task #{} (preview only).",
                field(&selection["task"], "id")
            ));
        }
    }
    for row in value["tasks"].as_array().into_iter().flatten() {
        let task = &row["task"];
        if !explain && task["status"] == "completed" {
            continue;
        }
        let description = task["description"]
            .as_str()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("");
        let readiness = if row["ready"] == true {
            "ready"
        } else {
            "unavailable"
        };
        lines.push(String::new());
        lines.push(format!(
            "#{}  {}  {}  Priority: {}  {}",
            field(task, "id"),
            status_label(task["status"].as_str().unwrap_or("")),
            readiness,
            field(task, "priority"),
            clean(description)
        ));
        if !row["queue_rank"].is_null() {
            lines.push(format!("  Queue rank: {}", field(row, "queue_rank")));
        }
        let reasons: Vec<_> = row["reasons"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|reason| reason.replace('_', " "))
            .collect();
        if !reasons.is_empty() {
            lines.push(format!("  Reasons: {}", reasons.join(", ")));
        }
        if !row["owner"].is_null() {
            lines.push(format!("  Owner: {}", field(row, "owner")));
            if !task["harness_session"].is_null() {
                lines.push(format!(
                    "  Harness: {} / {}",
                    field(task, "harness_name"),
                    field(task, "harness_session")
                ));
            }
        }
        for blocker in row["blockers"].as_array().into_iter().flatten() {
            let status = blocker["status"]
                .as_str()
                .map(status_label)
                .unwrap_or("Missing");
            lines.push(format!(
                "  Blocker: #{} ({status}, archived: {})",
                field(blocker, "id"),
                blocker["archived"]
                    .as_bool()
                    .map_or_else(|| "unknown".to_owned(), |value| value.to_string())
            ));
        }
        let activity = &row["latest_activity"];
        let mut detail = field(activity, "source");
        if !activity["action"].is_null() {
            detail.push_str(&format!(" {}", field(activity, "action")));
        }
        if !activity["session"].is_null() {
            detail.push_str(&format!(" by {}", field(activity, "session")));
        }
        lines.push(format!(
            "  Latest activity: {} ({detail})",
            field(activity, "at")
        ));
    }
    lines.join("\n")
}
