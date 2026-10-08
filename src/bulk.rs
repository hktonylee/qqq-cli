//! Frozen metadata previews and one-transaction application.
use crate::{
    archive,
    db::{self, Db, Task},
    errors::{Code, Info},
    list_filter::{self, ListStatus},
    sql_filter::CompiledFilter,
    tags,
};
use anyhow::{Context, Result, ensure};
use clap::Args as ClapArgs;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(ClapArgs)]
pub(crate) struct Args {
    /// Apply exact saved JSON preview; - reads stdin. Explicit confirmation.
    #[arg(long, value_name="PATH", conflicts_with_all=["ids","query","statuses","filter","tags","all","include_archived","add_tags","remove_tags","set_tags","priority","archive","unarchive"])]
    pub apply: Option<PathBuf>,
    /// Saved positive task ID; repeat. Conflicts with selectors.
    #[arg(long="id", value_name="ID", conflicts_with_all=["query","statuses","filter","tags","all","include_archived"])]
    ids: Vec<i64>,
    /// Case-insensitive whole-description substring.
    #[arg(long)]
    query: Option<String>,
    /// Include any of these statuses; repeat for OR.
    #[arg(long = "status", value_enum)]
    statuses: Vec<ListStatus>,
    /// Existing Luau filter expression.
    #[arg(long)]
    pub filter: Option<String>,
    /// Require all exact tag labels; repeat.
    #[arg(long = "tag", value_name = "LABEL")]
    pub tags: Vec<String>,
    /// Explicitly select all visible tasks without display limits.
    #[arg(long)]
    all: bool,
    /// Include archived tasks in selector selection.
    #[arg(long)]
    include_archived: bool,
    /// Add tag label; repeat.
    #[arg(long = "add-tag", value_name = "LABEL")]
    add_tags: Vec<String>,
    /// Remove tag label; repeat.
    #[arg(long = "remove-tag", value_name = "LABEL")]
    remove_tags: Vec<String>,
    /// Replace all tags with comma-separated labels; empty clears.
    #[arg(long, value_name="LABELS", conflicts_with_all=["add_tags","remove_tags"])]
    set_tags: Option<String>,
    /// Set priority -100..100.
    #[arg(long, allow_hyphen_values = true)]
    priority: Option<i64>,
    /// Archive selected tasks, preserving normal archive restrictions.
    #[arg(long, conflicts_with = "unarchive")]
    archive: bool,
    /// Unarchive selected tasks.
    #[arg(long)]
    unarchive: bool,
}

impl Args {
    pub fn actions(&self) -> Result<Actions> {
        ensure!(
            !self.ids.is_empty()
                || self.query.is_some()
                || !self.statuses.is_empty()
                || self.filter.is_some()
                || !self.tags.is_empty()
                || self.all,
            Info::invalid_argument(
                "selection",
                "Bulk preview requires IDs, a selector or --all"
            )
        );
        Actions {
            add_tags: self.add_tags.clone(),
            remove_tags: self.remove_tags.clone(),
            set_tags: self.set_tags.as_deref().map(tags::parse).transpose()?,
            priority: self.priority,
            archived: if self.archive {
                Some(true)
            } else if self.unarchive {
                Some(false)
            } else {
                None
            },
        }
        .normalize()
    }
    pub fn selection<'a>(&'a self, filter: Option<&'a CompiledFilter>) -> Selection<'a> {
        if !self.ids.is_empty() {
            Selection::Ids(&self.ids)
        } else {
            Selection::Filter {
                query: self.query.as_deref(),
                statuses: &self.statuses,
                include_archived: self.include_archived,
                filter,
            }
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Actions {
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
    pub set_tags: Option<Vec<String>>,
    pub priority: Option<i64>,
    pub archived: Option<bool>,
}
impl Actions {
    pub fn normalize(mut self) -> Result<Self> {
        self.add_tags = tags::normalize(&self.add_tags)?;
        self.remove_tags = tags::normalize(&self.remove_tags)?;
        self.set_tags = self.set_tags.as_deref().map(tags::normalize).transpose()?;
        ensure!(
            self.set_tags.is_none() || (self.add_tags.is_empty() && self.remove_tags.is_empty()),
            Info::invalid_argument("tags", "Cannot replace tags and add/remove tags together")
        );
        ensure!(
            !self
                .add_tags
                .iter()
                .any(|tag| self.remove_tags.contains(tag)),
            Info::invalid_argument("tags", "Cannot add and remove same tag")
        );
        if let Some(priority) = self.priority {
            db::validate_priority(priority)?;
        }
        ensure!(
            !self.add_tags.is_empty()
                || !self.remove_tags.is_empty()
                || self.set_tags.is_some()
                || self.priority.is_some()
                || self.archived.is_some(),
            Info::invalid_argument("actions", "Bulk preview requires at least one action")
        );
        Ok(self)
    }
    fn after(&self, before: &Metadata) -> Metadata {
        let tags = self.set_tags.clone().unwrap_or_else(|| {
            let mut labels: Vec<_> = before
                .tags
                .iter()
                .filter(|tag| !self.remove_tags.contains(tag))
                .cloned()
                .collect();
            for tag in &self.add_tags {
                if !labels.contains(tag) {
                    labels.push(tag.clone());
                }
            }
            labels
        });
        Metadata {
            tags,
            priority: self.priority.unwrap_or(before.priority),
            archived: self.archived.unwrap_or(before.archived),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Metadata {
    pub tags: Vec<String>,
    pub priority: i64,
    pub archived: bool,
}
impl Metadata {
    fn from_task(task: &Task) -> Self {
        Self {
            tags: task.tags.clone(),
            priority: task.priority,
            archived: task.archived,
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            tags::normalize(&self.tags)? == self.tags,
            Info::invalid_argument("preview", "Preview tags must be normalized")
        );
        db::validate_priority(self.priority)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Row {
    pub id: i64,
    pub before: Metadata,
    pub after: Metadata,
    pub fingerprint: String,
}
impl Row {
    fn count(&self) -> usize {
        usize::from(self.before.tags != self.after.tags)
            + usize::from(self.before.priority != self.after.priority)
            + usize::from(self.before.archived != self.after.archived)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    pub version: u32,
    pub applied: bool,
    pub database: String,
    pub actions: Actions,
    pub selected_ids: Vec<i64>,
    pub selected_count: usize,
    pub changed_count: usize,
    pub action_count: usize,
    pub tasks: Vec<Row>,
}
impl Report {
    fn validate(&self) -> Result<()> {
        let invalid = || {
            Info::invalid_argument(
                "preview",
                "Invalid bulk preview; regenerate and review preview",
            )
        };
        ensure!(self.version == 1 && !self.applied, invalid());
        ensure!(self.actions.clone().normalize()? == self.actions, invalid());
        ensure!(!self.database.is_empty(), invalid());
        ensure!(
            self.selected_count == self.tasks.len()
                && self.selected_ids == self.tasks.iter().map(|t| t.id).collect::<Vec<_>>(),
            invalid()
        );
        ensure!(
            self.selected_ids.iter().all(|id| *id > 0)
                && self.selected_ids.windows(2).all(|ids| ids[0] < ids[1]),
            invalid()
        );
        ensure!(
            self.changed_count == self.tasks.iter().filter(|t| t.count() > 0).count()
                && self.action_count == self.tasks.iter().map(Row::count).sum::<usize>(),
            invalid()
        );
        for row in &self.tasks {
            row.before.validate()?;
            row.after.validate()?;
            ensure!(row.after == self.actions.after(&row.before), invalid());
            ensure!(
                row.fingerprint.len() == 64
                    && row.fingerprint.bytes().all(|b| b.is_ascii_hexdigit()),
                invalid()
            );
        }
        Ok(())
    }
}

pub(crate) enum Selection<'a> {
    Ids(&'a [i64]),
    Filter {
        query: Option<&'a str>,
        statuses: &'a [ListStatus],
        include_archived: bool,
        filter: Option<&'a CompiledFilter>,
    },
}
fn database(conn: &Connection) -> Result<String> {
    let path = conn
        .path()
        .context("Bulk actions require file-backed database")?;
    Ok(std::fs::canonicalize(path)?.to_string_lossy().into_owned())
}
fn fingerprint(conn: &Connection, task: &Task) -> Result<String> {
    let claim: Option<String> =
        conn.query_row("SELECT claim_key FROM tasks WHERE id=?", [task.id], |r| {
            r.get(0)
        })?;
    let event: Option<i64> = conn.query_row(
        "SELECT max(id) FROM events WHERE task_id=?",
        [task.id],
        |r| r.get(0),
    )?;
    let bytes = serde_json::to_vec(&serde_json::json!({"task":task,"claim":claim,"event":event}))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn task(conn: &Connection, id: i64) -> Result<Option<Task>> {
    Ok(conn
        .query_row(
            &format!("SELECT {} FROM tasks WHERE id=?", db::TASK_COLUMNS),
            [id],
            db::task_row,
        )
        .optional()?)
}
fn validate_archive(conn: &Connection, tasks: &[Task], report: &Report) -> Result<()> {
    let changes: BTreeMap<_, _> = report
        .tasks
        .iter()
        .map(|row| (row.id, row.after.archived))
        .collect();
    for (task, row) in tasks.iter().zip(&report.tasks) {
        archive::validate(conn, task, row.after.archived, &changes)?;
    }
    Ok(())
}
pub(crate) fn preview(db: &Db, selection: Selection<'_>, actions: Actions) -> Result<Report> {
    let actions = actions.normalize()?;
    let tx = db.conn.unchecked_transaction()?;
    let tasks = match selection {
        Selection::Ids(ids) => {
            let mut ids = ids.to_vec();
            ids.sort_unstable();
            ids.dedup();
            ensure!(
                ids.iter().all(|id| *id > 0),
                Info::invalid_argument("--id", "Bulk IDs must be positive saved task IDs")
            );
            ids.into_iter()
                .map(|id| task(&tx, id)?.with_context(|| Info::missing_task(id)))
                .collect::<Result<Vec<_>>>()?
        }
        Selection::Filter {
            query,
            statuses,
            include_archived,
            filter,
        } => {
            let (tasks, matches) = db.list_filtered(None, include_archived, filter)?;
            list_filter::direct_tasks(tasks, query, statuses, matches.as_ref())
        }
    };
    let rows = tasks
        .iter()
        .map(|task| {
            let before = Metadata::from_task(task);
            Ok(Row {
                id: task.id,
                after: actions.after(&before),
                before,
                fingerprint: fingerprint(&tx, task)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let report = Report {
        version: 1,
        applied: false,
        database: database(&tx)?,
        actions,
        selected_ids: rows.iter().map(|row| row.id).collect(),
        selected_count: rows.len(),
        changed_count: rows.iter().filter(|row| row.count() > 0).count(),
        action_count: rows.iter().map(Row::count).sum(),
        tasks: rows,
    };
    validate_archive(&tx, &tasks, &report)?;
    tx.commit()?;
    Ok(report)
}
pub(crate) fn read(path: &Path) -> Result<Report> {
    let mut contents = String::new();
    if path == Path::new("-") {
        std::io::stdin().read_to_string(&mut contents)?;
    } else {
        contents = std::fs::read_to_string(path)?;
    }
    let report: Report = serde_json::from_str(&contents).map_err(|error| {
        anyhow::Error::new(error).context(Info::invalid_argument(
            "--apply",
            "Invalid bulk preview JSON",
        ))
    })?;
    report.validate()?;
    Ok(report)
}
pub(crate) fn apply(db: &mut Db, report: &Report, actor: &str) -> Result<Report> {
    report.validate()?;
    db::nonempty(actor, "Actor")?;
    let tx = db
        .conn
        .transaction_with_behavior(TransactionBehavior::Immediate)?;
    ensure!(
        database(&tx)? == report.database,
        Info::invalid_argument("--apply", "Bulk preview belongs to another project")
    );
    let mut tasks = Vec::new();
    let mut conflicts = Vec::new();
    for row in &report.tasks {
        match task(&tx, row.id)? {
            Some(task) => {
                if Metadata::from_task(&task) != row.before
                    || fingerprint(&tx, &task)? != row.fingerprint
                {
                    conflicts.push(row.id);
                }
                tasks.push(task);
            }
            None => conflicts.push(row.id),
        }
    }
    ensure!(
        conflicts.is_empty(),
        Info::new(
            Code::BulkConflict,
            "Bulk selection changed; regenerate and review preview"
        )
        .detail("reason", "stale_bulk_preview")
        .detail("selected_ids", report.selected_ids.clone())
        .detail("conflict_ids", conflicts)
    );
    validate_archive(&tx, &tasks, report)?;
    for row in report.tasks.iter().filter(|row| row.count() > 0) {
        tx.execute("UPDATE tasks SET tags=?,priority=?,archived=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?",
            params![serde_json::to_string(&row.after.tags)?,row.after.priority,row.after.archived,row.id])?;
        if row.before.archived != row.after.archived {
            tx.execute(
                "INSERT INTO events(task_id,session,action) VALUES (?,?,?)",
                params![
                    row.id,
                    actor,
                    if row.after.archived {
                        "archive"
                    } else {
                        "unarchive"
                    }
                ],
            )?;
        }
    }
    tx.commit()?;
    let mut result = report.clone();
    result.applied = true;
    Ok(result)
}
