use crate::db::{Task, TaskMessage};

pub fn rows(task: &Task, messages: &[TaskMessage], width: usize) -> Vec<String> {
    let parent = task
        .parent_id
        .map_or_else(|| "none".to_owned(), |id| format!("#{id}"));
    let mut content = vec![
        format!(
            "Task #{} | {} | Priority {} | Parent {parent}",
            task.id,
            crate::output::status_label(&task.status),
            task.priority
        ),
        format!("Messages ({}) | PgUp/PgDn scroll", messages.len()),
    ];
    if messages.is_empty() {
        content.push("No messages yet.".to_owned());
    } else {
        for message in messages.iter().rev() {
            content.push(format!(
                "#{} {} | {}",
                message.id,
                message.session.as_deref().unwrap_or("cli"),
                message.created_at
            ));
            content.push(message.body.clone());
        }
    }
    content.extend([
        String::new(),
        format!("Created: {}", task.created_at),
        format!("Updated: {}", task.updated_at),
        format!("Archived: {}", if task.archived { "yes" } else { "no" }),
        format!(
            "Harness: {} / {}",
            task.identity.harness_name.as_deref().unwrap_or("-"),
            task.identity.harness_session.as_deref().unwrap_or("-")
        ),
        format!(
            "Orchestrator: {} / {}",
            task.identity.orchestrator_name.as_deref().unwrap_or("-"),
            task.identity.orchestrator_session.as_deref().unwrap_or("-")
        ),
    ]);
    super::render::Layout::new(&[content.join("\n")], &[], width).rows
}

#[cfg(test)]
mod tests {
    use super::rows;
    use crate::db::{Task, TaskMessage};

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
        let rendered = rows(&task(), &messages, 120).join("\n");
        for value in [
            "Task #4",
            "In progress",
            "Priority 8",
            "Parent #2",
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
    }

    #[test]
    fn empty_messages_and_unicode_controls_are_readable() {
        assert!(
            rows(&task(), &[], 80)
                .join("\n")
                .contains("No messages yet.")
        );
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
                .all(|row| unicode_width::UnicodeWidthStr::width(row.as_str()) <= 12)
        );
        assert!(!lines.join("\n").contains('\x1b'));
        assert!(lines.join("").contains("\\u{1b}"));
        assert!(lines.join("").contains("界界界界界界界界"));
    }
}
