use anyhow::{Result, ensure};
use serde_json::Value;
use std::collections::HashSet;

pub fn normalize(tags: &[String]) -> Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for tag in tags {
        let label = tag.trim();
        ensure!(
            !label.is_empty()
                && !tag
                    .chars()
                    .any(|ch| ch.is_control() || matches!(ch, ',' | '[' | ']')),
            crate::errors::Info::invalid_argument(
                "tags",
                "Tags must be nonempty labels without controls, commas or square brackets"
            )
        );
        if seen.insert(label) {
            normalized.push(label.to_owned());
        }
    }
    Ok(normalized)
}

pub fn parse(value: &str) -> Result<Vec<String>> {
    if value.trim().is_empty() {
        return Ok(Vec::new());
    }
    normalize(&value.split(',').map(str::to_owned).collect::<Vec<_>>())
}

pub fn prefix(task: &Value) -> String {
    task["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|tag| format!("[{tag}]"))
        .collect::<Vec<_>>()
        .join(" ")
}
