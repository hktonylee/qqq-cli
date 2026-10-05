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

pub fn parse_lines(value: &str) -> Result<Vec<String>> {
    let labels = value
        .lines()
        .filter(|line| !line.trim().is_empty() || line.chars().any(char::is_control))
        .flat_map(|line| line.split(','))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    normalize(&labels)
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

#[cfg(test)]
mod tests {
    #[test]
    fn popup_lines_preserve_order_dedupe_and_comma_compatibility() {
        assert_eq!(
            super::parse_lines("frontend\n界 面\nfrontend\n").unwrap(),
            vec!["frontend", "界 面"]
        );
        assert_eq!(
            super::parse_lines("frontend, bug\r\n\r\nops").unwrap(),
            vec!["frontend", "bug", "ops"]
        );
    }

    #[test]
    fn popup_lines_clear_blank_values_and_skip_blank_rows() {
        for value in ["", " \n\n ", "\r\n\r\n"] {
            assert!(super::parse_lines(value).unwrap().is_empty());
        }
        assert_eq!(
            super::parse_lines("\n frontend\n\n  \n bug \n").unwrap(),
            vec!["frontend", "bug"]
        );
    }

    #[test]
    fn popup_lines_reject_non_separator_controls_and_invalid_labels() {
        for value in [
            "front\tend",
            "front\rend",
            "\t",
            "front\n\t\nend",
            "front\x1bend",
            "[tag]",
            "front,,end",
            "front,\nend",
        ] {
            assert!(super::parse_lines(value).is_err(), "{value:?}");
        }
        // CLI parser retains comma-only syntax and rejects newline labels.
        assert!(super::parse("front\nend").is_err());
    }
}
