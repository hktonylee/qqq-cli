use super::{clean, field};
use serde_json::Value;

pub(super) fn render(value: &Value) -> String {
    let dry_run = value["dry_run"] == true;
    let count = value["count"].as_u64().unwrap_or(0);
    let action = if dry_run { "Validated" } else { "Imported" };
    let plural = if count == 1 { "task" } else { "tasks" };
    let suffix = if dry_run {
        " (dry-run; no tasks created)"
    } else {
        ""
    };
    let mut lines = vec![format!("{action} {count} {plural}{suffix}.")];
    for task in value["tasks"].as_array().into_iter().flatten() {
        if dry_run {
            let description = task["description"]
                .as_str()
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("");
            let parent = if !task["parent"]["key"].is_null() {
                format!("key {}", field(&task["parent"], "key"))
            } else if !task["parent_id"].is_null() {
                format!("#{}", field(task, "parent_id"))
            } else {
                "none".to_owned()
            };
            let prerequisites = task["depends_on"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|reference| {
                    if reference["id"].is_null() {
                        format!("key {}", field(reference, "key"))
                    } else {
                        format!("#{}", field(reference, "id"))
                    }
                })
                .collect::<Vec<_>>();
            let extras = if prerequisites.is_empty() {
                String::new()
            } else {
                format!("  Prerequisites: {}", prerequisites.join(", "))
            };
            lines.push(format!(
                "{}  Priority: {}  Parent: {}{extras}  {}",
                field(task, "key"),
                field(task, "priority"),
                parent,
                clean(description)
            ));
        } else {
            lines.push(format!("{} -> #{}", field(task, "key"), field(task, "id")));
        }
    }
    lines.join("\n")
}
