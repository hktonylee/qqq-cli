use crate::db::{ensure_parent_available, nonempty, validate_description, validate_priority};
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
}

#[derive(Serialize)]
struct ResultTask {
    key: String,
    description: String,
    priority: i64,
    parent: Option<Parent>,
    id: Option<i64>,
    parent_id: Option<i64>,
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
    let batch: Batch = serde_json::from_str(&content).context("Invalid import JSON")?;
    validate(batch)
}

fn validate(batch: Batch) -> Result<ValidatedBatch> {
    ensure!(
        batch.version == 1,
        "Unsupported import version {}; expected 1",
        batch.version
    );
    let mut keys = HashMap::new();
    for (index, task) in batch.tasks.iter().enumerate() {
        nonempty(&task.key, "Batch task key")?;
        ensure!(
            keys.insert(task.key.as_str(), index).is_none(),
            "Duplicate batch task key {:?}",
            task.key
        );
        validate_description(&task.description)
            .with_context(|| format!("Batch task {:?}", task.key))?;
        validate_priority(task.priority).with_context(|| format!("Batch task {:?}", task.key))?;
    }
    let mut existing_parents = BTreeSet::new();
    let mut children = vec![Vec::new(); batch.tasks.len()];
    let mut ready = BinaryHeap::new();
    for (index, task) in batch.tasks.iter().enumerate() {
        match &task.parent {
            Some(Parent::Local(parent)) => {
                nonempty(&parent.key, "Batch parent key")?;
                let parent_index = keys.get(parent.key.as_str()).with_context(|| {
                    format!(
                        "Batch task {:?} references missing parent key {:?}",
                        task.key, parent.key
                    )
                })?;
                children[*parent_index].push(index);
            }
            parent => {
                if let Some(Parent::Existing(parent)) = parent {
                    ensure!(parent.id > 0, "Existing parent must be a positive task ID");
                    existing_parents.insert(parent.id);
                }
                ready.push(Reverse(index));
            }
        }
    }
    let mut order = Vec::with_capacity(batch.tasks.len());
    while let Some(Reverse(index)) = ready.pop() {
        order.push(index);
        ready.extend(children[index].iter().copied().map(Reverse));
    }
    ensure!(
        order.len() == batch.tasks.len(),
        "Import batch contains dependency cycle"
    );
    Ok(ValidatedBatch {
        tasks: batch.tasks,
        order,
        existing_parents,
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
    let mut mapping = BTreeMap::new();
    if !dry_run {
        let mut insert =
            tx.prepare("INSERT INTO tasks(description,parent_id,priority) VALUES (?,?,?)")?;
        for index in &batch.order {
            let task = &batch.tasks[*index];
            let parent_id = parent_id(task.parent.as_ref(), &mapping);
            ensure!(
                task.parent.is_none() || parent_id.is_some(),
                "Unresolved batch parent for {:?}",
                task.key
            );
            insert
                .execute(params![task.description, parent_id, task.priority])
                .with_context(|| format!("Failed to import task {:?}", task.key))?;
            mapping.insert(task.key.clone(), tx.last_insert_rowid());
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
