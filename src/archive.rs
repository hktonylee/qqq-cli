//! Shared single-task and projected-batch archive validation.
use crate::{db::Task, errors::Info};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension};
use std::collections::BTreeMap;

pub(crate) fn validate(
    conn: &Connection,
    task: &Task,
    archived: bool,
    changes: &BTreeMap<i64, bool>,
) -> Result<()> {
    if task.archived == archived {
        return Ok(());
    }
    if archived {
        ensure!(
            task.status != "in_progress",
            Info::transition(
                task.id,
                &task.status,
                &["new", "error", "completed"],
                format!("Task {} is in progress and cannot be archived", task.id)
            )
        );
        if task.status != "completed" {
            let mut statement = conn.prepare(
                "SELECT id,archived FROM tasks WHERE status!='completed' AND
                 (parent_id=?1 OR id IN (SELECT task_id FROM task_dependencies WHERE prerequisite_id=?1))"
            )?;
            let rows = statement.query_map([task.id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?))
            })?;
            for row in rows {
                let (id, current) = row?;
                ensure!(
                    changes.get(&id).copied().unwrap_or(current),
                    Info::transition(
                        task.id,
                        &task.status,
                        &["completed"],
                        format!(
                            "Task {} has non-archived unfinished child or dependent",
                            task.id
                        )
                    )
                    .detail("reason", "unfinished_child")
                );
            }
        }
    } else if task.status != "completed" {
        if let Some(parent) = task.parent_id {
            available(conn, parent, true, changes)?;
        }
        for prerequisite in &task.prerequisites {
            available(conn, prerequisite.id, false, changes)?;
        }
    }
    Ok(())
}

fn available(
    conn: &Connection,
    id: i64,
    parent: bool,
    changes: &BTreeMap<i64, bool>,
) -> Result<()> {
    let (status, archived) = conn
        .query_row("SELECT status,archived FROM tasks WHERE id=?", [id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?))
        })
        .optional()?
        .with_context(|| Info::missing_task(id))?;
    let archived = changes.get(&id).copied().unwrap_or(archived);
    let mut info = Info::transition(
        id,
        &status,
        &["completed"],
        format!(
            "Task {id} is archived unfinished {}",
            if parent { "parent" } else { "prerequisite" }
        ),
    )
    .detail("actual_archived", true);
    if !parent {
        info = info.detail("reason", "archived_unfinished_prerequisite");
    }
    ensure!(!archived || status == "completed", info);
    Ok(())
}
