use super::render::{DetailKind, DetailRow, Layout};
use crate::db::{Task, TaskMessage};

fn wrap(content: Vec<DetailRow>, width: usize) -> Vec<DetailRow> {
    let last = content.len().saturating_sub(1);
    let fragments: Vec<_> = content
        .iter()
        .enumerate()
        .map(|(index, row)| {
            if index == last {
                row.text.clone()
            } else {
                format!("{}\n", row.text)
            }
        })
        .collect();
    let layout = Layout::new(&fragments, &[], width);
    let mut kinds = vec![DetailKind::Body; layout.rows.len()];
    for (index, row) in content.iter().enumerate() {
        let start = layout.positions[index].0;
        let end = if index == last {
            kinds.len()
        } else {
            layout.positions[index + 1].0
        };
        kinds[start..end].fill(row.kind);
    }
    layout
        .rows
        .into_iter()
        .zip(kinds)
        .map(|(text, kind)| DetailRow::new(text, kind))
        .collect()
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
    let mut content = vec![DetailRow::new(
        format!(
            "Task #{} | {} | Priority {} | Parent {parent}",
            task.id,
            crate::output::status_label(&task.status),
            task.priority
        ),
        DetailKind::Heading,
    )];
    if messages.is_empty() {
        content.push(DetailRow::new("No messages yet.", DetailKind::Muted));
    } else {
        content.push(DetailRow::new(
            format!("Messages ({}) | PgUp/PgDn scroll", messages.len()),
            DetailKind::Heading,
        ));
        for message in messages.iter().rev() {
            content.push(DetailRow::new(
                format!(
                    "#{} {} | {}",
                    message.id,
                    message.session.as_deref().unwrap_or("cli"),
                    message.created_at
                ),
                DetailKind::MessageHeader,
            ));
            content.push(DetailRow::new(message.body.clone(), DetailKind::Body));
        }
    }
    content.extend([
        DetailRow::new("", DetailKind::Body),
        DetailRow::new(format!("Created: {}", task.created_at), DetailKind::Muted),
        DetailRow::new(format!("Updated: {}", task.updated_at), DetailKind::Muted),
        DetailRow::new(
            format!("Archived: {}", if task.archived { "yes" } else { "no" }),
            DetailKind::Muted,
        ),
        DetailRow::new(
            format!(
                "Harness: {} / {}",
                task.identity.harness_name.as_deref().unwrap_or("-"),
                task.identity.harness_session.as_deref().unwrap_or("-")
            ),
            DetailKind::Muted,
        ),
        DetailRow::new(
            format!(
                "Orchestrator: {} / {}",
                task.identity.orchestrator_name.as_deref().unwrap_or("-"),
                task.identity.orchestrator_session.as_deref().unwrap_or("-")
            ),
            DetailKind::Muted,
        ),
    ]);
    wrap(content, width)
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
        let detail_rows = rows(&task(), &messages, 120);
        let rendered = text(&detail_rows, "\n");
        for value in [
            "Task #4",
            "In progress",
            "Priority 8",
            "Parent #2",
            "Messages (2) | PgUp/PgDn scroll",
            "reviewer",
            "2026-10-02T14:00",
            "Created:",
            "Updated:",
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
        assert!(rendered.find("Earlier message").unwrap() < rendered.find("Created:").unwrap());
        assert!(!rendered.contains("Full editable description"));
        assert_eq!(detail_rows[0].kind, DetailKind::Heading);
        assert_eq!(detail_rows[1].kind, DetailKind::Heading);
        assert_eq!(detail_rows[2].kind, DetailKind::MessageHeader);
        assert_eq!(detail_rows[3].kind, DetailKind::Body);
        assert_eq!(detail_rows[4].kind, DetailKind::Body);
        assert!(
            detail_rows
                .iter()
                .filter(|row| row.text.starts_with("Created:") || row.text.starts_with("Harness:"))
                .all(|row| row.kind == DetailKind::Muted)
        );
    }

    #[test]
    fn empty_messages_and_unicode_controls_are_readable() {
        for width in [12, 80] {
            let rendered = text(&rows(&task(), &[], width), "");
            assert!(rendered.contains("No messages yet."));
            assert!(!rendered.contains("Messages (0)"));
            assert!(!rendered.contains("PgUp/PgDn scroll"));
            assert!(rendered.contains("Created:"));
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
    }
}
