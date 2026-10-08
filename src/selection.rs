use crate::{
    db::{Db, READY_TASK_PREDICATE, Task},
    errors::{Code, Info},
    list_filter::{self, ListStatus},
    sql_filter::{self, CompiledFilter},
};
use anyhow::Result;
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Args, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selectors {
    /// Require exact stored tag; repeat to require every label.
    #[arg(long = "tag", value_name = "LABEL", allow_hyphen_values = true)]
    #[serde(default)]
    pub tags: Vec<String>,
    /// Match typed Luau expression; see docs/filter.md.
    #[arg(long, value_name = "EXPR", allow_hyphen_values = true)]
    pub filter: Option<String>,
    /// Match full description text, ignoring Unicode case.
    #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
    pub query: Option<String>,
    /// Include any supplied status; repeat for multiple statuses.
    #[arg(long = "status", value_enum, value_name = "STATUS")]
    #[serde(default)]
    pub statuses: Vec<ListStatus>,
    /// Require shared dependency readiness; blocked means new and unarchived.
    #[arg(long, value_enum)]
    pub readiness: Option<Readiness>,
}

#[derive(Clone, Copy, Debug, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    Ready,
    Blocked,
}

#[derive(Clone, Copy, Default)]
pub struct Visibility {
    pub include_archived: Option<bool>,
    // Some(None) explicitly selects unlimited completed tasks.
    pub max_completed: Option<Option<i64>>,
    pub fallback_completed: Option<i64>,
}

#[derive(Clone, Default)]
pub struct Prepared {
    pub filter: Option<CompiledFilter>,
    pub include_archived: bool,
    pub max_completed: Option<i64>,
}

pub fn prepare(
    saved: Option<&Selectors>,
    explicit: &Selectors,
    visibility: Visibility,
) -> Result<Prepared> {
    let criteria: Vec<_> = saved.into_iter().chain(std::iter::once(explicit)).collect();
    let labels = criteria
        .iter()
        .flat_map(|criteria| criteria.tags.iter().cloned())
        .collect::<Vec<_>>();
    let tags = crate::tags::normalize(&labels)
        .map_err(|error| Info::invalid_argument("--tag", error.to_string()))?;
    let sources: Vec<_> = criteria
        .iter()
        .filter_map(|criteria| criteria.filter.as_deref())
        .collect();
    let source = match sources.as_slice() {
        [] => None,
        [source] => Some((*source).to_owned()),
        _ => Some(
            sources
                .iter()
                .map(|source| format!("not not (\n{source}\n)"))
                .collect::<Vec<_>>()
                .join(" and "),
        ),
    };
    let mut filter = source
        .as_deref()
        .map(sql_filter::compile)
        .transpose()
        .map_err(|error| {
            error.context(
                Info::new(Code::InvalidFilter, "Invalid --filter expression")
                    .detail("argument", "--filter"),
            )
        })?;
    if !tags.is_empty() {
        filter
            .get_or_insert_with(CompiledFilter::all)
            .require_tags(tags);
    }
    for criteria in criteria {
        if let Some(query) = &criteria.query {
            filter
                .get_or_insert_with(CompiledFilter::all)
                .require_query(query);
        }
        if !criteria.statuses.is_empty() {
            let statuses: Vec<_> = criteria
                .statuses
                .iter()
                .map(|status| status.as_str())
                .collect();
            filter
                .get_or_insert_with(CompiledFilter::all)
                .require_statuses(&statuses);
        }
        if let Some(readiness) = criteria.readiness {
            let predicate = match readiness {
                Readiness::Ready => READY_TASK_PREDICATE.to_owned(),
                Readiness::Blocked => format!(
                    "tasks.status='new' AND tasks.archived=0 AND NOT ({READY_TASK_PREDICATE})"
                ),
            };
            filter
                .get_or_insert_with(CompiledFilter::all)
                .require_static(&predicate);
        }
    }
    Ok(Prepared {
        filter,
        include_archived: visibility.include_archived.unwrap_or(false),
        max_completed: visibility
            .max_completed
            .unwrap_or(visibility.fallback_completed),
    })
}

pub fn list(db: &Db, selection: &Prepared, extra_query: Option<&str>) -> Result<Vec<Task>> {
    let mut filter = selection.filter.clone();
    if let Some(query) = extra_query.filter(|query| !query.is_empty()) {
        filter
            .get_or_insert_with(CompiledFilter::all)
            .require_query(query);
    }
    let (tasks, matches) = db.list_filtered(
        selection.max_completed,
        selection.include_archived,
        filter.as_ref(),
    )?;
    Ok(list_filter::filter_tasks(
        tasks,
        None,
        &[],
        matches.as_ref(),
    ))
}
