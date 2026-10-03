use crate::db::{ensure_parent_available, nonempty, validate_description, validate_priority};
use crate::errors::Info;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap},
    io::Read,
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    version: u32,
    tasks: Vec<InputTask>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputTask {
    key: String,
    description: String,
    #[serde(default)]
    priority: i64,
    parent: Option<Parent>,
    #[serde(default)]
    depends_on: Vec<Parent>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum Parent {
    Existing(ExistingParent),
    Local(LocalParent),
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExistingParent {
    id: i64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LocalParent {
    key: String,
}

pub struct ValidatedBatch {
    tasks: Vec<InputTask>,
    order: Vec<usize>,
    existing_parents: BTreeSet<i64>,
    existing_prerequisites: BTreeSet<i64>,
}

#[derive(Serialize)]
struct ResultTask {
    key: String,
    description: String,
    priority: i64,
    parent: Option<Parent>,
    id: Option<i64>,
    parent_id: Option<i64>,
    depends_on: Vec<Parent>,
    prerequisite_ids: Vec<Option<i64>>,
}

#[derive(Serialize)]
pub struct Report {
    version: u32,
    dry_run: bool,
    count: usize,
    mapping: BTreeMap<String, i64>,
    creation_order: Vec<String>,
    tasks: Vec<ResultTask>,
}

pub fn read(path: &Path) -> Result<ValidatedBatch> {
    let content = if path == Path::new("-") {
        let mut content = String::new();
        std::io::stdin()
            .read_to_string(&mut content)
            .context("Failed to read import stdin as UTF-8")?;
        content
    } else {
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read import file {} as UTF-8", path.display()))?
    };
    let batch: Batch = serde_json::from_str(&content).map_err(|error| {
        let info = Info::invalid_argument("path", "Invalid import JSON")
            .detail("reason", "invalid_import_json")
            .detail("line", error.line())
            .detail("column", error.column());
        anyhow::Error::new(error).context(info)
    })?;
    validate(batch)
}

fn validate(batch: Batch) -> Result<ValidatedBatch> {
    ensure!(
        batch.version == 1,
        Info::invalid_argument(
            "version",
            format!("Unsupported import version {}; expected 1", batch.version)
        )
        .detail("reason", "unsupported_import_version")
        .detail("expected_version", 1)
        .detail("actual_version", batch.version)
    );
    let mut keys = HashMap::new();
    for (index, task) in batch.tasks.iter().enumerate() {
        nonempty(&task.key, "Batch task key")?;
        ensure!(
            keys.insert(task.key.as_str(), index).is_none(),
            Info::invalid_argument("key", "Duplicate batch task key")
                .detail("reason", "duplicate_batch_key")
                .detail("batch_key", task.key.clone())
                .human(format!("Duplicate batch task key {:?}", task.key))
        );
        validate_description(&task.description)
            .with_context(|| format!("Batch task {:?}", task.key))?;
        validate_priority(task.priority).with_context(|| format!("Batch task {:?}", task.key))?;
    }
    let mut existing_parents = BTreeSet::new();
    let mut existing_prerequisites = BTreeSet::new();
    let mut children = vec![Vec::new(); batch.tasks.len()];
    let mut degrees = vec![0_usize; batch.tasks.len()];
    let mut ready = BinaryHeap::new();
    for (index, task) in batch.tasks.iter().enumerate() {
        let mut seen = BTreeSet::new();
        for (reference, is_parent) in task
            .parent
            .iter()
            .map(|p| (p, true))
            .chain(task.depends_on.iter().map(|p| (p, false)))
        {
            let argument = if is_parent { "parent" } else { "depends_on" };
            match reference {
                Parent::Local(parent) => {
                    nonempty(&parent.key, "Batch dependency key")?;
                    ensure!(
                        seen.insert((None, Some(parent.key.as_str()))),
                        Info::invalid_argument(
                            argument,
                            "Duplicate dependency or parent/prerequisite overlap"
                        )
                        .detail("batch_key", task.key.clone())
                    );
                    let parent_index = keys.get(parent.key.as_str()).with_context(|| {
                        let label = if is_parent { "parent" } else { "prerequisite" };
                        Info::invalid_argument(
                            argument,
                            format!("Batch task references missing {label} key"),
                        )
                        .detail(
                            "reason",
                            if is_parent {
                                "missing_batch_parent"
                            } else {
                                "missing_batch_prerequisite"
                            },
                        )
                        .detail("batch_key", task.key.clone())
                        .detail(
                            if is_parent {
                                "parent_key"
                            } else {
                                "prerequisite_key"
                            },
                            parent.key.clone(),
                        )
                        .human(format!(
                            "Batch task {:?} references missing {label} key {:?}",
                            task.key, parent.key
                        ))
                    })?;
                    children[*parent_index].push(index);
                    degrees[index] += 1;
                }
                Parent::Existing(parent) => {
                    ensure!(
                        parent.id > 0,
                        Info::invalid_argument(
                            argument,
                            "Existing dependency must be a positive task ID"
                        )
                        .detail("task_id", parent.id)
                    );
                    ensure!(
                        seen.insert((Some(parent.id), None)),
                        Info::invalid_argument(
                            argument,
                            "Duplicate dependency or parent/prerequisite overlap"
                        )
                        .detail("batch_key", task.key.clone())
                    );
                    if is_parent {
                        existing_parents.insert(parent.id);
                    } else {
                        existing_prerequisites.insert(parent.id);
                    }
                }
            }
        }
        if degrees[index] == 0 {
            ready.push(Reverse(index));
        }
    }
    let mut order = Vec::with_capacity(batch.tasks.len());
    while let Some(Reverse(index)) = ready.pop() {
        order.push(index);
        for child in &children[index] {
            degrees[*child] -= 1;
            if degrees[*child] == 0 {
                ready.push(Reverse(*child));
            }
        }
    }
    ensure!(
        order.len() == batch.tasks.len(),
        Info::invalid_argument("parent", "Import batch contains dependency cycle")
            .detail("reason", "dependency_cycle")
    );
    Ok(ValidatedBatch {
        tasks: batch.tasks,
        order,
        existing_parents,
        existing_prerequisites,
    })
}

fn parent_id(parent: Option<&Parent>, mapping: &BTreeMap<String, i64>) -> Option<i64> {
    match parent {
        Some(Parent::Existing(parent)) => Some(parent.id),
        Some(Parent::Local(parent)) => mapping.get(&parent.key).copied(),
        None => None,
    }
}

pub fn run(conn: &mut Connection, batch: &ValidatedBatch, dry_run: bool) -> Result<Report> {
    let behavior = if dry_run {
        TransactionBehavior::Deferred
    } else {
        TransactionBehavior::Immediate
    };
    let tx = conn.transaction_with_behavior(behavior)?;
    for id in &batch.existing_parents {
        ensure_parent_available(&tx, *id, true)?;
    }
    for id in &batch.existing_prerequisites {
        crate::dependencies::ensure_available(&tx, *id, true)?;
    }
    crate::dependencies::validate_graph(&tx)?;
    let mut mapping = BTreeMap::new();
    if !dry_run {
        let mut insert =
            tx.prepare("INSERT INTO tasks(description,parent_id,priority) VALUES (?,?,?)")?;
        for index in &batch.order {
            let task = &batch.tasks[*index];
            let resolved_parent = parent_id(task.parent.as_ref(), &mapping);
            ensure!(
                task.parent.is_none() || resolved_parent.is_some(),
                "Unresolved batch parent for {:?}",
                task.key
            );
            insert
                .execute(params![task.description, resolved_parent, task.priority])
                .with_context(|| format!("Failed to import task {:?}", task.key))?;
            let id = tx.last_insert_rowid();
            mapping.insert(task.key.clone(), id);
            for reference in &task.depends_on {
                let prerequisite = parent_id(Some(reference), &mapping)
                    .context("Unresolved batch prerequisite")?;
                tx.execute(
                    "INSERT INTO task_dependencies(task_id,prerequisite_id) VALUES (?,?)",
                    params![id, prerequisite],
                )?;
            }
        }
    }
    let tasks = batch
        .tasks
        .iter()
        .map(|task| ResultTask {
            key: task.key.clone(),
            description: task.description.clone(),
            priority: task.priority,
            parent: task.parent.clone(),
            id: mapping.get(&task.key).copied(),
            parent_id: parent_id(task.parent.as_ref(), &mapping),
            depends_on: task.depends_on.clone(),
            prerequisite_ids: task
                .depends_on
                .iter()
                .map(|reference| parent_id(Some(reference), &mapping))
                .collect(),
        })
        .collect();
    let creation_order = batch
        .order
        .iter()
        .map(|index| batch.tasks[*index].key.clone())
        .collect();
    let report = Report {
        version: 1,
        dry_run,
        count: batch.tasks.len(),
        mapping,
        creation_order,
        tasks,
    };
    tx.commit()?;
    Ok(report)
}
