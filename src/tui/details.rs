use super::render::{DetailKind, DetailRow, escape};
use crate::db::{Task, TaskMessage};
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

pub fn rows(task: &Task, messages: &[TaskMessage], width: usize) -> Vec<DetailRow> {
    let parent = task
        .parent_id
        .map_or_else(|| "none".to_owned(), |id| format!("#{id}"));
    let status_kind = match task.status.as_str() {
        "in_progress" => DetailKind::InProgress,
        "completed" => DetailKind::Completed,
        "error" => DetailKind::Error,
        _ => DetailKind::Body,
    };
    let mut content = vec![DetailRow::styled([
        ("Task ".into(), DetailKind::Body),
        (format!("#{}", task.id), DetailKind::Heading),
        (" | ".into(), DetailKind::Body),
        (
            crate::output::status_label(&task.status).into(),
            status_kind,
        ),
        (" | Priority ".into(), DetailKind::Muted),
        (task.priority.to_string(), DetailKind::Body),
        (" | Parent ".into(), DetailKind::Muted),
        (parent, DetailKind::Body),
    ])];
    if messages.is_empty() {
        content.push(DetailRow::new("No messages yet.", DetailKind::Muted));
    } else {
        for message in messages.iter().rev() {
            content.push(DetailRow::styled([
                (format!("#{}", message.id), DetailKind::MessageHeader),
                (
                    format!(" {} | ", message.session.as_deref().unwrap_or("cli")),
                    DetailKind::Body,
                ),
                (message.created_at.clone(), DetailKind::Muted),
            ]));
            content.push(DetailRow::new(message.body.clone(), DetailKind::Body));
        }
    }
    content.extend([
        DetailRow::new("", DetailKind::Body),
        DetailRow::new(
            format!("Created {}  Updated {}", task.created_at, task.updated_at),
            DetailKind::Muted,
        ),
        DetailRow::styled([
            ("Archived: ".into(), DetailKind::Muted),
            (
                (if task.archived { "yes" } else { "no" }).into(),
                DetailKind::Body,
            ),
        ]),
        assignment(
            "Harness",
            &task.identity.harness_name,
            &task.identity.harness_session,
        ),
        assignment(
            "Orchestrator",
            &task.identity.orchestrator_name,
            &task.identity.orchestrator_session,
        ),
    ]);
    wrap(content, width)
}

fn assignment(label: &str, name: &Option<String>, session: &Option<String>) -> DetailRow {
    DetailRow::styled([
        (format!("{label}: "), DetailKind::Muted),
        (
            name.as_deref().unwrap_or("-").into(),
            if name.is_some() {
                DetailKind::Body
            } else {
                DetailKind::Muted
            },
        ),
        (" / ".into(), DetailKind::Body),
        (
            session.as_deref().unwrap_or("-").into(),
            if session.is_some() {
                DetailKind::Body
            } else {
                DetailKind::Muted
            },
        ),
    ])
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
            DetailRow::new("CCC", DetailKind::MessageHeader),
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
                DetailRow::new("CCC", DetailKind::MessageHeader),
                DetailRow::new("D", DetailKind::Muted),
            ]
        );
    }

    fn task() -> Task {
        Task {
            id: 4,
            description: "Full editable description".into(),
            status: "in_progress".into(),
            priority: 8,
            parent_id: Some(2),
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

    #[test]
    fn summary_and_latest_messages_precede_remaining_metadata() {
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
        let detail_rows = rows(&task(), &messages, 66);
        let rendered = text(&detail_rows, "\n");
        for value in [
            "Task #4",
            "In progress",
            "Priority 8",
            "Parent #2",
            "reviewer",
            "2026-10-02T14:00",
            "Created ",
            "Updated ",
            "codex",
            "session-1",
            "herdr",
            "default",
        ] {
            assert!(rendered.contains(value), "{value}: {rendered}");
        }
        assert!(
            rendered.find("Latest message").unwrap() < rendered.find("Earlier message").unwrap()
        );
        assert!(rendered.find("Earlier message").unwrap() < rendered.find("Created ").unwrap());
        assert_eq!(
            detail_rows
                .iter()
                .find(|row| row.text.starts_with("Created ")),
            Some(&DetailRow::new(
                "Created 2026-10-02T12:00:00.000Z  Updated 2026-10-02T13:00:00.000Z",
                DetailKind::Muted,
            ))
        );
        assert!(!rendered.contains("Full editable description"));
        assert!(!rendered.contains("Messages ("));
        assert!(!rendered.contains("PgUp/PgDn scroll"));
        assert_eq!(detail_rows[0].kind, DetailKind::Body);
        assert_eq!(detail_rows[1].kind, DetailKind::Body);
        assert_eq!(detail_rows[2].kind, DetailKind::Body);
        assert_eq!(detail_rows[3].kind, DetailKind::Body);
        assert_eq!(detail_rows[4].kind, DetailKind::Body);
        assert!(
            detail_rows
                .iter()
                .filter(|row| row.text.starts_with("Created "))
                .all(|row| row.kind == DetailKind::Muted)
        );
        for width in [12, 66] {
            let detail_rows = rows(&task(), &messages, width);
            assert_eq!(styled_text(&detail_rows, DetailKind::Heading), "#4");
            assert_eq!(styled_text(&detail_rows, DetailKind::MessageHeader), "#2#1");
            assert_eq!(
                styled_text(&detail_rows, DetailKind::InProgress),
                "In progress"
            );
            let muted = styled_text(&detail_rows, DetailKind::Muted);
            assert!(muted.contains("2026-10-02T14:00:00.000Z"));
            assert!(muted.contains("Harness: "));
            assert!(!muted.contains("reviewer"));
            assert!(!muted.contains("codex"));
            let body = styled_text(&detail_rows, DetailKind::Body);
            assert!(body.contains("Latest messageNext line"));
            assert!(body.contains("codex / session-1"));
        }
        for (status, kind) in [
            ("new", DetailKind::Body),
            ("in_progress", DetailKind::InProgress),
            ("completed", DetailKind::Completed),
            ("error", DetailKind::Error),
        ] {
            let mut fixture = task();
            fixture.status = status.into();
            let detail_rows = rows(&fixture, &[], 12);
            assert!(styled_text(&detail_rows, kind).contains(crate::output::status_label(status)));
        }
    }

    #[test]
    fn empty_messages_and_unicode_controls_are_readable() {
        for width in [12, 80] {
            let rendered = text(&rows(&task(), &[], width), "");
            assert!(rendered.contains("No messages yet."));
            assert!(!rendered.contains("Messages (0)"));
            assert!(!rendered.contains("PgUp/PgDn scroll"));
            assert!(rendered.contains("Created "));
            assert!(rendered.contains("Orchestrator:"));
        }
        let messages = [TaskMessage {
            id: 1,
            body: "界界界界界界界界\n\x1b[31m unsafe".into(),
            session: Some("author\x1b".into()),
            created_at: "2026-10-02T14:00:00.000Z".into(),
        }];
        let lines = rows(&task(), &messages, 12);
        assert!(
            lines
                .iter()
                .all(|row| unicode_width::UnicodeWidthStr::width(row.text.as_str()) <= 12)
        );
        assert!(!text(&lines, "\n").contains('\x1b'));
        assert!(text(&lines, "").contains("\\u{1b}"));
        assert!(text(&lines, "").contains("界界界界界界界界"));
        assert!(
            text(&lines, "")
                .contains("Created 2026-10-02T12:00:00.000Z  Updated 2026-10-02T13:00:00.000Z")
        );
    }
}
