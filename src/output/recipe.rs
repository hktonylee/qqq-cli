use super::{clean, field, import};
use serde_json::Value;

pub(super) fn render(value: &Value) -> String {
    let mut text = import::render(value);
    if value["dry_run"] != true {
        return text;
    }
    for task in value["tasks"].as_array().into_iter().flatten() {
        text.push_str(&format!("\n\n{}:\n", field(task, "key")));
        for (index, line) in task["description"]
            .as_str()
            .unwrap_or("")
            .split('\n')
            .enumerate()
        {
            if index != 0 {
                text.push('\n');
            }
            text.push_str("  ");
            text.push_str(&clean(line));
        }
        let tags = task["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(clean)
            .collect::<Vec<_>>();
        text.push_str(&format!(
            "\n  Tags: {}",
            if tags.is_empty() {
                "none".to_owned()
            } else {
                tags.join(", ")
            }
        ));
    }
    text
}
