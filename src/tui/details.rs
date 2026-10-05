use super::render::{DetailKind, DetailRow, escape};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

fn wrap(content: Vec<DetailRow>, width: usize) -> Vec<DetailRow> {
    let width = width.max(1);
    let mut wrapped = Vec::new();
    for row in content {
        let spans = if row.spans.is_empty() {
            vec![(0, row.text.len(), row.kind)]
        } else {
            row.spans
        };
        let mut current = DetailRow::new("", row.kind);
        let mut column = 0;
        for (start, end, kind) in spans {
            let safe = escape(&row.text[start..end]);
            for grapheme in safe.graphemes(true) {
                if grapheme == "\n" {
                    wrapped.push(current);
                    current = DetailRow::new("", row.kind);
                    column = 0;
                    continue;
                }
                let (grapheme, cells) = if grapheme.width() > width {
                    ("?", 1)
                } else {
                    (grapheme, grapheme.width())
                };
                if column + cells > width {
                    wrapped.push(current);
                    current = DetailRow::new("", row.kind);
                    column = 0;
                }
                current.push(grapheme, kind);
                column += cells;
            }
        }
        wrapped.push(current);
    }
    for row in &mut wrapped {
        if row.spans.iter().all(|&(_, _, kind)| kind == row.kind) {
            row.spans.clear();
        }
    }
    wrapped
}

pub fn unavailable(id: i64, width: usize) -> Vec<DetailRow> {
    wrap(
        vec![DetailRow::new(
            format!("Task #{id} unavailable."),
            DetailKind::Warning,
        )],
        width,
    )
}

pub fn rows(value: &serde_json::Value, width: usize) -> Vec<DetailRow> {
    // Show escapes user controls before adding these trusted SGR style markers.
    let content = crate::output::detail_text(value, true)
        .split('\n')
        .map(styled_line)
        .collect();
    wrap(content, width)
}

fn styled_line(line: &str) -> DetailRow {
    let mut parts = Vec::new();
    let mut text = line;
    let mut kind = DetailKind::Body;
    while let Some((before, sequence)) = text.split_once("\x1b[") {
        parts.push((before.to_owned(), kind));
        let Some((code, rest)) = sequence.split_once('m') else {
            parts.push((format!("\\u{{1b}}[{sequence}"), DetailKind::Body));
            return DetailRow::styled(parts);
        };
        kind = match code {
            "1" => DetailKind::Heading,
            "1;36" => DetailKind::Section,
            "2" => DetailKind::Muted,
            "36" => DetailKind::InProgress,
            "90" => DetailKind::Completed,
            "31" => DetailKind::Error,
            "33" => DetailKind::Warning,
            "32" => DetailKind::Success,
            _ => DetailKind::Body,
        };
        text = rest;
    }
    parts.push((text.to_owned(), kind));
    DetailRow::styled(parts)
}

#[cfg(test)]
mod tests {
    use super::{rows, unavailable, wrap};
    use crate::db::{Task, TaskMessage};
    use crate::tui::render::{DetailKind, DetailRow};

    fn text(rows: &[DetailRow], separator: &str) -> String {
        rows.iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>()
            .join(separator)
    }

    fn styled_text(rows: &[DetailRow], kind: DetailKind) -> String {
        let mut text = String::new();
        for row in rows {
            if row.spans.is_empty() && row.kind == kind {
                text.push_str(&row.text);
            }
            for &(start, end, span_kind) in &row.spans {
                if span_kind == kind {
                    text.push_str(&row.text[start..end]);
                }
            }
        }
        text
    }

    #[test]
    fn wrapping_preserves_roles_at_full_width_and_newlines() {
        let content = vec![
            DetailRow::new("AAAAAA", DetailKind::Heading),
            DetailRow::new("B\nBB", DetailKind::Body),
            DetailRow::new("CCC", DetailKind::Heading),
            DetailRow::new("D", DetailKind::Muted),
        ];
        let wrapped = wrap(content, 3);
        assert_eq!(
            wrapped,
            vec![
                DetailRow::new("AAA", DetailKind::Heading),
                DetailRow::new("AAA", DetailKind::Heading),
                DetailRow::new("B", DetailKind::Body),
                DetailRow::new("BB", DetailKind::Body),
                DetailRow::new("CCC", DetailKind::Heading),
                DetailRow::new("D", DetailKind::Muted),
            ]
        );
    }

    fn task() -> Task {
        Task {
            id: 4,
            content_revision: 1,
            description: "Full editable description".into(),
            status: "in_progress".into(),
            priority: 8,
            parent_id: Some(2),
            prerequisites: Vec::new(),
            tags: Vec::new(),
            archived: false,
            context_only: false,
            created_at: "2026-10-02T12:00:00.000Z".into(),
            updated_at: "2026-10-02T13:00:00.000Z".into(),
            identity: crate::identity::Identity {
                harness_name: Some("codex".into()),
                harness_session: Some("session-1".into()),
                orchestrator_name: Some("herdr".into()),
                orchestrator_session: Some("default".into()),
            },
        }
    }

    #[test]
    fn unavailable_task_notice_wraps_at_minimum_width() {
        let lines = unavailable(12345, 12);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|row| row.text.len() <= 12));
        assert_eq!(text(&lines, ""), "Task #12345 unavailable.");
        assert!(lines.iter().all(|row| row.kind == DetailKind::Warning));
    }

    fn value(messages: &[TaskMessage]) -> serde_json::Value {
        serde_json::json!({
            "task": task(), "messages": messages, "images": [], "events": [], "herdr": null,
        })
    }

    #[test]
    fn show_header_sections_alignment_and_collections_match() {
        let messages = [
            TaskMessage {
                id: 1,
                body: "Earlier message".into(),
                session: None,
                created_at: "2026-10-02T13:00:00.000Z".into(),
            },
            TaskMessage {
                id: 2,
                body: "Latest message\nNext line".into(),
                session: Some("reviewer".into()),
                created_at: "2026-10-02T14:00:00.000Z".into(),
            },
        ];
        let mut value = value(&messages);
        value["images"] = serde_json::json!([{ "id": 1, "name": "screenshot.png", "media_type": "image/png", "bytes": 128 }]);
        value["events"] = serde_json::json!([{ "created_at": "2026-10-02T12:00:00.000Z", "action": "claim", "session": "worker" }]);
        value["herdr"] = serde_json::json!({ "server": "named", "identity": { "agent": "codex", "kind": "id", "value": "agent-1" }, "pane": { "workspace_id": "w1", "tab_id": "t1", "pane_id": "p1" } });
        let detail_rows = rows(&value, 1000);
        let rendered = text(&detail_rows, "\n");
        assert!(
            rendered.starts_with(
                "#4 · In progress\n\nDescription:\n  Full editable description\n\nMessages: (2)\n"
            ),
            "{rendered}"
        );
        assert_eq!(rendered, crate::output::detail_text(&value, false));
        for title in [
            "Description:",
            "Messages: (2)",
            "Details:",
            "Assignment:",
            "Images: (1)",
            "History: (1)",
            "Herdr:",
        ] {
            assert!(rendered.contains(title), "{title}: {rendered}");
        }
        for (label, expected_value) in [
            ("Parent:", "#2"),
            ("Priority:", "8"),
            ("Archived:", "no"),
            ("Created:", "2026-10-02T12:00:00.000Z"),
            ("Updated:", "2026-10-02T13:00:00.000Z"),
            ("Harness", "session-1 (codex)"),
            ("Orchestrator", "default (herdr)"),
        ] {
            let row = detail_rows
                .iter()
                .find(|row| row.text.trim_start().starts_with(label))
                .unwrap();
            assert_eq!(&row.text[25..], expected_value);
        }
        assert!(
            rendered.find("Earlier message").unwrap() < rendered.find("Latest message").unwrap()
        );
        assert!(rendered.contains("\n    Latest message\n    Next line\n\nDetails:"));
        assert!(rendered.contains("  Image #1: screenshot.png (image/png, 128 bytes)"));
        assert!(rendered.contains("  Server: named\n  Workspace: w1\n  Tab: t1\n  Pane: p1"));
        assert_eq!(
            styled_text(&detail_rows, DetailKind::InProgress),
            "In progressclaim   "
        );
        assert_eq!(
            styled_text(&detail_rows, DetailKind::Section),
            "Description:Messages:Details:Assignment:Images:History:Herdr:"
        );
        let muted = styled_text(&detail_rows, DetailKind::Muted);
        assert!(muted.contains("Harness"));
        assert!(muted.contains("Orchestrator"));
        assert!(muted.contains("2026-10-02T14:00:00.000Z"));
        assert!(!muted.contains("reviewer"));
        assert!(!muted.contains("codex"));
        for (status, kind) in [
            ("new", DetailKind::Body),
            ("in_progress", DetailKind::InProgress),
            ("completed", DetailKind::Completed),
            ("error", DetailKind::Error),
        ] {
            value["task"]["status"] = status.into();
            assert!(
                styled_text(&rows(&value, 12), kind).contains(crate::output::status_label(status))
            );
        }
    }

    #[test]
    fn prerequisite_sections_wrap_and_refresh_blockers() {
        let mut value = value(&[]);
        value["task"]["prerequisites"] = serde_json::json!([
            {"id":12345,"status":"error","archived":false},
            {"id":2,"status":"completed","archived":true}
        ]);
        let rendered = crate::output::detail_text(&value, false);
        assert!(rendered.contains(
            "Prerequisites:\n  #12345 · error · blocks claim\n  #2 · completed · archived"
        ));
        assert!(rendered.contains("Full editable description\n\nMessages: None\n\nPrerequisites:"));
        let narrow = rows(&value, 12);
        assert!(
            narrow
                .iter()
                .all(|row| unicode_width::UnicodeWidthStr::width(row.text.as_str()) <= 12)
        );
        assert!(text(&narrow, "").contains("blocks claim"));
        value["task"]["prerequisites"][0]["status"] = "completed".into();
        assert!(!text(&rows(&value, 72), "\n").contains("blocks claim"));
    }

    #[test]
    fn assignment_grouping_preserves_values_roles_and_wrapping() {
        for (label, session_key, name_key) in [
            ("Harness", "harness_session", "harness_name"),
            ("Orchestrator", "orchestrator_session", "orchestrator_name"),
        ] {
            for (session, name, expected) in [
                (Some("session-1"), Some("codex"), "session-1 (codex)"),
                (Some("session-1"), None, "session-1"),
                (None, Some("codex"), "- (codex)"),
                (None, None, "-"),
                (Some("-"), Some("-"), "- (-)"),
                (
                    Some("session\n界\x1b"),
                    Some("Co\t界"),
                    "session\\n界\\u{1b} (Co\\t界)",
                ),
            ] {
                let mut value = value(&[]);
                value["task"][session_key] = serde_json::json!(session);
                value["task"][name_key] = serde_json::json!(name);
                let wide = rows(&value, 1000);
                let assignment = wide
                    .iter()
                    .find(|row| row.text.trim_start().starts_with(label))
                    .unwrap();
                assert_eq!(assignment.text[..25].trim(), label);
                assert_eq!(&assignment.text[25..], expected);
                let lines = rows(&value, 12);
                let rendered = text(&lines, "");
                assert!(rendered.contains(expected), "{rendered}");
                assert!(
                    !rendered.contains("Harness name:") && !rendered.contains("Harness session:")
                );
                assert!(
                    !rendered.contains("Orchestrator name:")
                        && !rendered.contains("Orchestrator session:")
                );
                assert!(!rendered.contains('\x1b'));
                assert!(
                    lines
                        .iter()
                        .all(|row| unicode_width::UnicodeWidthStr::width(row.text.as_str()) <= 12)
                );
                assert!(
                    styled_text(std::slice::from_ref(assignment), DetailKind::Muted)
                        .contains(label)
                );
                if expected != "-" {
                    assert!(
                        styled_text(std::slice::from_ref(assignment), DetailKind::Body)
                            .contains(expected)
                    );
                }
            }
        }
    }

    #[test]
    fn empty_collections_and_unicode_controls_keep_show_text_and_roles() {
        assert!(
            text(&rows(&value(&[]), 80), "\n").contains(
                "Description:\n  Full editable description\n\nMessages: None\n\nDetails:"
            )
        );
        for width in [12, 80] {
            let rendered = text(&rows(&value(&[]), width), "");
            for empty in [
                "Messages: None",
                "Images: None",
                "History: None",
                "Herdr: Not linked",
            ] {
                assert!(rendered.contains(empty), "{rendered}");
            }
        }
        let messages = [TaskMessage {
            id: 1,
            body: "界界界界界界界界\n\x1b[31m unsafe\nDescription:".into(),
            session: Some("author\x1b".into()),
            created_at: "2026-10-02T14:00:00.000Z".into(),
        }];
        let value = value(&messages);
        let lines = rows(&value, 12);
        assert!(
            lines
                .iter()
                .all(|row| unicode_width::UnicodeWidthStr::width(row.text.as_str()) <= 12)
        );
        let rendered = text(&lines, "");
        assert!(!rendered.contains('\x1b'));
        assert!(rendered.contains("\\u{1b}[31m unsafe"));
        assert!(rendered.contains("界界界界界界界界"));
        assert_eq!(
            styled_text(&lines, DetailKind::Section)
                .matches("Description:")
                .count(),
            1
        );
        assert!(styled_text(&lines, DetailKind::Body).contains("    Description:"));
        assert!(styled_text(&lines, DetailKind::Heading).contains("#1"));
        assert!(!styled_text(&lines, DetailKind::Error).contains("unsafe"));
        for row in lines {
            for &(start, end, _) in &row.spans {
                assert!(row.text.is_char_boundary(start) && row.text.is_char_boundary(end));
            }
        }
    }
}
