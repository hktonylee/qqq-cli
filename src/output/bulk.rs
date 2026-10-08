use super::clean;
use serde_json::Value;

pub(super) fn render(value: &Value) -> String {
    let applied = value["applied"] == true;
    let mut text = format!(
        "{} {} tasks: {} changed, {} actions.",
        if applied {
            "Applied bulk changes to"
        } else {
            "Bulk preview for"
        },
        value["selected_count"],
        value["changed_count"],
        value["action_count"]
    );
    for row in value["tasks"].as_array().into_iter().flatten() {
        text.push_str(&format!("\n#{}", row["id"]));
        for field in ["tags", "priority", "archived"] {
            text.push_str(&format!(
                "\n  {field}: {} -> {}",
                clean(&row["before"][field].to_string()),
                clean(&row["after"][field].to_string())
            ));
        }
    }
    if !applied {
        text.push_str(
            "\nPreview only. Save preview with --json, then confirm with qqq bulk --apply PATH.",
        );
    }
    text
}
