use crate::db::{Db, READY_TASK_PREDICATE, Task, filter_evaluation_error, task_row};
use crate::sql_filter::CompiledFilter;
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params_from_iter};
use serde::Serialize;
use std::collections::BTreeMap;

const TASK_COLUMNS: &str = "id,description,status,claim_key,created_at,updated_at,parent_id,
    harness_name,harness_session,orchestrator_name,orchestrator_session,priority,archived,content_revision";

#[derive(Default, Serialize)]
pub struct Counts {
    total: usize,
    new: usize,
    ready: usize,
    blocked: usize,
    matching_ready: usize,
    in_progress: usize,
    error: usize,
    completed: usize,
    archived: usize,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Empty,
    Ready,
    NoMatchingReady,
    Blocked,
    Error,
    InProgress,
    NoReady,
}

impl Counts {
    fn state(&self) -> State {
        let unfinished = self.new + self.in_progress + self.error;
        match () {
            _ if self.total == 0 => State::Empty,
            _ if self.matching_ready > 0 => State::Ready,
            _ if self.ready > 0 => State::NoMatchingReady,
            _ if unfinished > 0 && self.blocked == unfinished => State::Blocked,
            _ if unfinished > 0 && self.error == unfinished => State::Error,
            _ if unfinished > 0 && self.in_progress == unfinished => State::InProgress,
            _ => State::NoReady,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Reason {
    Archived,
    ParentNotCompleted,
    InProgress,
    Error,
    Completed,
    FilterExcluded,
}

#[derive(Serialize)]
struct Blocker {
    id: i64,
    status: Option<String>,
    archived: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum ActivitySource {
    Task,
    Message,
    Event,
}

#[derive(Serialize)]
struct Activity {
    at: String,
    source: ActivitySource,
    id: Option<i64>,
    session: Option<String>,
    action: Option<String>,
}

#[derive(Serialize)]
struct Row {
    task: Task,
    ready: bool,
    matches_filter: bool,
    queue_rank: Option<usize>,
    reasons: Vec<Reason>,
    owner: Option<String>,
    blockers: Vec<Blocker>,
    latest_activity: Activity,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SelectionKind {
    Owned,
    Queued,
}

#[derive(Serialize)]
struct Selection {
    kind: SelectionKind,
    task: Task,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    OwnedTaskReuse,
    ReadyCandidate,
    EmptyQueue,
    NoMatchingReadyCandidate,
    BlockedQueue,
    ErrorQueue,
    InProgressQueue,
    NoReadyTasks,
}

#[derive(Serialize)]
struct Explanation {
    owner_input: Option<String>,
    resolved_owner: Option<String>,
    outcome: Outcome,
    ordering: [&'static str; 2],
    eligible_ids: Vec<i64>,
    selection: Option<Selection>,
}

#[derive(Serialize)]
pub struct Report {
    include_archived: bool,
    counts: Counts,
    state: State,
    tasks: Vec<Row>,
    explanation: Option<Explanation>,
}

fn latest_activity(conn: &Connection) -> Result<BTreeMap<i64, Activity>> {
    // One scan per activity source, rather than a query for every task.
    let mut statement = conn.prepare(
        "WITH activity AS (
            SELECT id task_id,updated_at at,0 source,NULL id,NULL session,NULL action FROM tasks
            UNION ALL SELECT task_id,created_at,2,id,session,NULL FROM messages
            UNION ALL SELECT task_id,created_at,1,id,session,action FROM events
        ), ranked AS (
            SELECT *,row_number() OVER (
                PARTITION BY task_id ORDER BY at DESC,source DESC,id DESC
            ) position FROM activity
        ) SELECT task_id,at,source,id,session,action FROM ranked WHERE position=1",
    )?;
    let rows = statement.query_map([], |row| {
        let source = match row.get::<_, i64>(2)? {
            2 => ActivitySource::Message,
            1 => ActivitySource::Event,
            _ => ActivitySource::Task,
        };
        Ok((
            row.get(0)?,
            Activity {
                at: row.get(1)?,
                source,
                id: row.get(3)?,
                session: row.get(4)?,
                action: row.get(5)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub struct Options<'a> {
    pub include_archived: bool,
    pub filter: Option<&'a CompiledFilter>,
    pub explain: bool,
    pub owner: Option<&'a str>,
    pub harness_name: Option<&'a str>,
}

pub fn report(conn: &mut Connection, options: Options<'_>) -> Result<Report> {
    report_with_activity(conn, options, latest_activity)
}

fn report_with_activity(
    conn: &mut Connection,
    options: Options<'_>,
    read_activity: impl FnOnce(&Connection) -> Result<BTreeMap<i64, Activity>>,
) -> Result<Report> {
    let Options {
        include_archived,
        filter,
        explain,
        owner,
        harness_name,
    } = options;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let raw_rows = (|| -> rusqlite::Result<Vec<_>> {
        let predicate = filter.map_or("1", CompiledFilter::sql);
        let mut statement = tx.prepare(&format!(
            "SELECT {TASK_COLUMNS},({READY_TASK_PREDICATE}),COALESCE(({predicate}),0)
             FROM tasks ORDER BY id"
        ))?;
        let rows = statement.query_map(
            params_from_iter(filter.map_or(&[][..], CompiledFilter::params)),
            |row| {
                Ok((
                    task_row(row)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, bool>(14)?,
                    row.get::<_, bool>(15)?,
                ))
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })();
    let raw_rows = match filter {
        Some(_) => raw_rows.context(filter_evaluation_error())?,
        None => raw_rows?,
    };
    let parents: BTreeMap<_, _> = raw_rows
        .iter()
        .map(|(task, ..)| (task.id, (task.status.clone(), task.archived)))
        .collect();
    let mut activity = read_activity(&tx)?;
    let mut ready: Vec<_> = raw_rows.iter().filter(|row| row.2).collect();
    ready.sort_by(|a, b| b.0.priority.cmp(&a.0.priority).then(a.0.id.cmp(&b.0.id)));
    let ranks: BTreeMap<_, _> = ready
        .iter()
        .enumerate()
        .map(|(index, row)| (row.0.id, index + 1))
        .collect();
    let eligible_ids: Vec<i64> = ready
        .iter()
        .filter(|row| row.3)
        .map(|row| row.0.id)
        .collect();
    // Reuse evaluated filter results: SQLite time functions can change between
    // statements in the same read transaction.
    let candidate_id = eligible_ids.first().copied();
    let mut counts = Counts::default();
    let mut tasks = Vec::new();
    for (task, claim_key, ready, matches_filter) in raw_rows {
        if task.archived && !include_archived {
            continue;
        }
        counts.total += 1;
        counts.archived += usize::from(task.archived);
        let mut reasons = Vec::new();
        if task.archived {
            reasons.push(Reason::Archived);
        }
        let mut blockers = Vec::new();
        if task.status == "new" {
            if let Some(id) = task.parent_id {
                let parent = parents.get(&id);
                if parent.is_none_or(|(status, _)| status != "completed") {
                    reasons.push(Reason::ParentNotCompleted);
                    blockers.push(Blocker {
                        id,
                        status: parent.map(|(status, _)| status.clone()),
                        archived: parent.map(|(_, archived)| *archived),
                    });
                }
            }
            counts.new += 1;
            counts.ready += usize::from(ready);
            counts.blocked += usize::from(!ready);
            counts.matching_ready += usize::from(ready && matches_filter);
        } else {
            match task.status.as_str() {
                "in_progress" => {
                    counts.in_progress += 1;
                    reasons.push(Reason::InProgress);
                }
                "error" => {
                    counts.error += 1;
                    reasons.push(Reason::Error);
                }
                "completed" => {
                    counts.completed += 1;
                    reasons.push(Reason::Completed);
                }
                _ => unreachable!("task status is constrained by schema"),
            }
        }
        if !matches_filter {
            reasons.push(Reason::FilterExcluded);
        }
        let owner = (task.status == "in_progress")
            .then_some(claim_key)
            .flatten();
        let queue_rank = ranks.get(&task.id).copied();
        let latest_activity = activity
            .remove(&task.id)
            .expect("every task has an update timestamp");
        tasks.push(Row {
            task,
            ready,
            matches_filter,
            queue_rank,
            reasons,
            owner,
            blockers,
            latest_activity,
        });
    }
    let state = counts.state();
    let explanation = if explain {
        let resolved_owner = owner
            .map(|owner| Db::owner_key(&tx, owner, harness_name))
            .transpose()?;
        let owned = resolved_owner.as_ref().map(|owner| {
            tx.query_row(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE status='in_progress' AND claim_key=?"), [owner], task_row).optional()
        }).transpose()?.flatten();
        let selection = match owned {
            Some(task) => Some(Selection {
                kind: SelectionKind::Owned,
                task,
            }),
            None => candidate_id
                .map(|id| {
                    tx.query_row(
                        &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id=?"),
                        [id],
                        task_row,
                    )
                    .map(|task| Selection {
                        kind: SelectionKind::Queued,
                        task,
                    })
                })
                .transpose()?,
        };
        let outcome = match selection.as_ref().map(|selection| &selection.kind) {
            Some(SelectionKind::Owned) => Outcome::OwnedTaskReuse,
            Some(SelectionKind::Queued) => Outcome::ReadyCandidate,
            None => match state {
                State::Empty => Outcome::EmptyQueue,
                State::NoMatchingReady => Outcome::NoMatchingReadyCandidate,
                State::Blocked => Outcome::BlockedQueue,
                State::Error => Outcome::ErrorQueue,
                State::InProgress => Outcome::InProgressQueue,
                State::NoReady | State::Ready => Outcome::NoReadyTasks,
            },
        };
        Some(Explanation {
            owner_input: owner.map(str::to_owned),
            resolved_owner,
            outcome,
            ordering: ["priority_desc", "id_asc"],
            eligible_ids,
            selection,
        })
    } else {
        None
    };
    tx.commit()?;
    Ok(Report {
        include_archived,
        counts,
        state,
        tasks,
        explanation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_sensitive_filter_selection_reuses_reported_eligibility() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in [
            include_str!("sql/schema.sql"),
            include_str!("sql/migrate_v2.sql"),
            include_str!("sql/migrate_v3.sql"),
            include_str!("sql/migrate_v4.sql"),
            include_str!("sql/migrate_v5.sql"),
            include_str!("sql/migrate_v6.sql"),
            include_str!("sql/migrate_v7.sql"),
            include_str!("sql/migrate_v8.sql"),
            include_str!("sql/migrate_v9.sql"),
            include_str!("sql/migrate_v10.sql"),
        ] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute(
            "INSERT INTO tasks(description) VALUES ('Time-sensitive task')",
            [],
        )
        .unwrap();
        let cutoff: f64 = conn
            .query_row("SELECT julianday('now','+0.25 seconds')", [], |row| {
                row.get(0)
            })
            .unwrap();
        let filter =
            crate::sql_filter::compile(&format!("julianday(\"now\") < {cutoff:.15}")).unwrap();
        let report = report_with_activity(
            &mut conn,
            Options {
                include_archived: false,
                filter: Some(&filter),
                explain: true,
                owner: None,
                harness_name: None,
            },
            |conn| {
                // Activity scanning can cross a date-filter cutoff even though the
                // transaction still sees identical task rows. Advance through it.
                while conn
                    .query_row("SELECT julianday('now')", [], |row| row.get::<_, f64>(0))
                    .unwrap()
                    <= cutoff
                {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                latest_activity(conn)
            },
        )
        .unwrap();
        assert_eq!(report.counts.matching_ready, 1);
        let explanation = report.explanation.unwrap();
        assert_eq!(explanation.eligible_ids, vec![1]);
        assert_eq!(explanation.selection.unwrap().task.id, 1);
    }
}
