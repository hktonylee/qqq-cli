use crate::db::Task;
use clap::ValueEnum;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, ValueEnum)]
pub enum ListStatus {
    New,
    #[value(name = "in_progress")]
    InProgress,
    Completed,
    Error,
}

impl ListStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Error => "error",
        }
    }
}

pub fn filter_tasks(
    tasks: Vec<Task>,
    query: Option<&str>,
    statuses: &[ListStatus],
    matches: Option<&HashSet<i64>>,
) -> Vec<Task> {
    if query.is_none() && statuses.is_empty() && matches.is_none() {
        return tasks;
    }

    let needle = query.map(str::to_lowercase);
    let direct: Vec<bool> = tasks
        .iter()
        .map(|task| {
            needle
                .as_ref()
                .is_none_or(|needle| task.description.to_lowercase().contains(needle))
                && (statuses.is_empty()
                    || statuses.iter().any(|status| status.as_str() == task.status))
                && matches.is_none_or(|matches| matches.contains(&task.id))
        })
        .collect();
    let positions: HashMap<i64, usize> = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| (task.id, index))
        .collect();
    let mut included = direct.clone();
    for (index, matched) in direct.iter().enumerate() {
        if !matched {
            continue;
        }
        let mut parent = tasks[index].parent_id;
        let mut visited = HashSet::new();
        while let Some(parent_id) = parent {
            if !visited.insert(parent_id) {
                break;
            }
            let Some(&ancestor) = positions.get(&parent_id) else {
                break;
            };
            included[ancestor] = true;
            parent = tasks[ancestor].parent_id;
        }
    }

    tasks
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut task)| {
            included[index].then(|| {
                task.context_only = !direct[index];
                task
            })
        })
        .collect()
}
