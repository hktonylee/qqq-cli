use crate::errors::Info;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

#[derive(Default)]
pub struct Changes<'a> {
    pub add: &'a [i64],
    pub remove: &'a [i64],
    pub clear: bool,
}

impl Changes<'_> {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty() && !self.clear
    }
}

pub fn ensure_available(conn: &Connection, id: i64, unfinished: bool) -> Result<()> {
    let (archived, status): (bool, String) = conn
        .query_row("SELECT archived,status FROM tasks WHERE id=?", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?
        .with_context(|| Info::missing_task(id))?;
    ensure!(
        !unfinished || !archived || status == "completed",
        Info::transition(
            id,
            &status,
            &["completed"],
            format!("Task {id} is archived unfinished prerequisite")
        )
        .detail("reason", "archived_unfinished_prerequisite")
        .detail("actual_archived", archived)
    );
    Ok(())
}

pub fn ensure_all_available(conn: &Connection, id: i64) -> Result<()> {
    let parent: Option<i64> =
        conn.query_row("SELECT parent_id FROM tasks WHERE id=?", [id], |r| r.get(0))?;
    if let Some(parent) = parent {
        crate::db::ensure_parent_available(conn, parent, true)?;
    }
    let ids = conn
        .prepare("SELECT prerequisite_id FROM task_dependencies WHERE task_id=?")?
        .query_map([id], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for prerequisite in ids {
        ensure_available(conn, prerequisite, true)?;
    }
    Ok(())
}

pub fn validate_add(
    conn: &Connection,
    parent: Option<i64>,
    ids: &[i64],
    unfinished: bool,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        ensure!(
            seen.insert(*id),
            Info::invalid_argument("--depends-on", format!("Duplicate prerequisite {id}"))
                .detail("prerequisite_id", *id)
        );
        ensure!(
            Some(*id) != parent,
            Info::invalid_argument(
                "--depends-on",
                format!("Prerequisite {id} duplicates parent")
            )
            .detail("prerequisite_id", *id)
        );
        ensure_available(conn, *id, unfinished)?;
    }
    Ok(())
}

// Caller holds an immediate transaction; final graph validation rolls back all metadata edits.
pub fn edit(conn: &Connection, id: i64, changes: &Changes<'_>) -> Result<()> {
    ensure!(
        !changes.clear || changes.remove.is_empty(),
        Info::invalid_argument(
            "--clear-depends-on",
            "Cannot both clear and remove prerequisites"
        )
    );
    let (parent, unfinished): (Option<i64>, bool) = conn
        .query_row(
            "SELECT parent_id,status!='completed' FROM tasks WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .with_context(|| Info::missing_task(id))?;
    let mut added = BTreeSet::new();
    let mut removed = BTreeSet::new();
    for prerequisite in changes.add {
        ensure!(
            *prerequisite != id,
            Info::invalid_argument("--depends-on", format!("Task {id} cannot depend on itself"))
                .detail("task_id", id)
        );
        ensure!(
            added.insert(*prerequisite),
            Info::invalid_argument(
                "--depends-on",
                format!("Duplicate prerequisite {prerequisite}")
            )
            .detail("task_id", id)
            .detail("prerequisite_id", *prerequisite)
        );
        ensure!(
            !changes.remove.contains(prerequisite),
            Info::invalid_argument(
                "--depends-on",
                format!("Cannot both add and remove prerequisite {prerequisite}")
            )
            .detail("task_id", id)
            .detail("prerequisite_id", *prerequisite)
        );
    }
    for prerequisite in changes.remove {
        ensure!(
            removed.insert(*prerequisite),
            Info::invalid_argument(
                "--remove-depends-on",
                format!("Duplicate removal of prerequisite {prerequisite}")
            )
            .detail("task_id", id)
            .detail("prerequisite_id", *prerequisite)
        );
        ensure!(
            conn.execute(
                "DELETE FROM task_dependencies WHERE task_id=? AND prerequisite_id=?",
                params![id, prerequisite]
            )? == 1,
            Info::invalid_argument(
                "--remove-depends-on",
                format!("Task {prerequisite} is not a prerequisite of task {id}")
            )
            .detail("task_id", id)
            .detail("prerequisite_id", *prerequisite)
        );
    }
    if changes.clear {
        conn.execute("DELETE FROM task_dependencies WHERE task_id=?", [id])?;
    }
    validate_add(conn, parent, changes.add, unfinished)?;
    for prerequisite in changes.add {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_dependencies WHERE task_id=? AND prerequisite_id=?)",
            params![id, prerequisite],
            |r| r.get(0),
        )?;
        ensure!(
            !exists,
            Info::invalid_argument(
                "--depends-on",
                format!("Duplicate prerequisite {prerequisite}")
            )
            .detail("task_id", id)
            .detail("prerequisite_id", *prerequisite)
        );
        conn.execute(
            "INSERT INTO task_dependencies(task_id,prerequisite_id) VALUES (?,?)",
            params![id, prerequisite],
        )?;
    }
    validate_graph(conn)
}

// O(tasks + edges). Parent and extra edges form one dependency graph.
pub fn validate_graph(conn: &Connection) -> Result<()> {
    let tasks = conn
        .prepare("SELECT id,parent_id FROM tasks")?
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut edges: HashMap<i64, HashSet<i64>> =
        tasks.iter().map(|(id, _)| (*id, HashSet::new())).collect();
    for (id, parent) in &tasks {
        if let Some(parent) = parent {
            ensure!(
                edges.contains_key(parent),
                Info::invalid_argument(
                    "dependencies",
                    format!("Task {id} references missing parent {parent}")
                )
                .detail("task_id", *id)
                .detail("parent_id", *parent)
            );
            edges.get_mut(id).expect("Known task").insert(*parent);
        }
    }
    let extras = conn
        .prepare("SELECT task_id,prerequisite_id FROM task_dependencies")?
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, prerequisite) in extras {
        ensure!(
            edges.contains_key(&prerequisite),
            Info::invalid_argument(
                "dependencies",
                format!("Task {id} references missing prerequisite {prerequisite}")
            )
            .detail("task_id", id)
            .detail("prerequisite_id", prerequisite)
        );
        let parents = edges.get_mut(&id).with_context(|| Info::missing_task(id))?;
        ensure!(
            parents.insert(prerequisite),
            Info::invalid_argument(
                "dependencies",
                format!("Prerequisite {prerequisite} duplicates parent of task {id}")
            )
            .detail("task_id", id)
            .detail("prerequisite_id", prerequisite)
        );
    }
    let mut degrees: HashMap<i64, usize> = edges
        .iter()
        .map(|(id, parents)| (*id, parents.len()))
        .collect();
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for (id, parents) in &edges {
        for parent in parents {
            children.entry(*parent).or_default().push(*id);
        }
    }
    let mut ready: VecDeque<i64> = degrees
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        if let Some(children) = children.get(&id) {
            for child in children {
                let degree = degrees.get_mut(child).expect("Known child");
                *degree -= 1;
                if *degree == 0 {
                    ready.push_back(*child);
                }
            }
        }
    }
    ensure!(
        visited == edges.len(),
        Info::invalid_argument(
            "dependencies",
            "Dependency graph contains a dependency cycle"
        )
        .detail("reason", "dependency_cycle")
    );
    Ok(())
}
