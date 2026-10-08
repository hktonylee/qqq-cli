use crate::{db::READY_TASK_PREDICATE, errors::Info};
use anyhow::{Context, Result};
use clap::{Args, ValueEnum};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Both,
    Upstream,
    Downstream,
}

#[derive(Clone, Copy, Debug, Args)]
pub struct Options {
    #[arg(long, value_enum, default_value = "both")]
    pub direction: Direction,
    #[arg(long, default_value_t = 8, value_parser = clap::value_parser!(u16).range(0..=128))]
    pub depth: u16,
    #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u32).range(1..=10000))]
    pub max_nodes: u32,
    #[arg(long, default_value_t = 2000, value_parser = clap::value_parser!(u32).range(1..=20000))]
    pub max_edges: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            direction: Direction::Both,
            depth: 8,
            max_nodes: 1000,
            max_edges: 2000,
        }
    }
}

impl crate::db::Db {
    pub fn graph(&self, reference: i64, options: Options) -> Result<Report> {
        let tx = self.conn.unchecked_transaction()?;
        let id = self.resolve_task_id(reference)?;
        let report = report(&tx, id, options)?;
        tx.commit()?;
        Ok(report)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Parent,
    Prerequisite,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Archived,
    InProgress,
    Error,
    Completed,
    ParentNotCompleted,
    PrerequisiteNotCompleted,
    MissingTask,
    UnknownStatus,
}
impl Reason {
    fn label(self) -> &'static str {
        match self {
            Self::Archived => "archived",
            Self::InProgress => "in progress",
            Self::Error => "error; retry required",
            Self::Completed => "completed",
            Self::ParentNotCompleted => "parent unfinished",
            Self::PrerequisiteNotCompleted => "prerequisite unfinished",
            Self::MissingTask => "missing task",
            Self::UnknownStatus => "unknown status",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactState {
    ImmediatelyReady,
    StillBlocked,
    AlreadyReady,
    Inactive,
}
impl ImpactState {
    fn label(self) -> &'static str {
        match self {
            Self::ImmediatelyReady => "immediately ready",
            Self::StillBlocked => "still blocked",
            Self::AlreadyReady => "already ready",
            Self::Inactive => "not a new unarchived candidate",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: i64,
    pub task_name: Option<String>,
    pub status: Option<String>,
    pub archived: Option<bool>,
    pub ready: bool,
    pub reasons: Vec<Reason>,
    pub unfinished_dependencies: usize,
    pub upstream_depth: Option<usize>,
    pub downstream_depth: Option<usize>,
    pub blocks_focus: bool,
    pub impact: Option<ImpactState>,
    pub remaining_after_completion: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub dependent: i64,
    pub prerequisite: i64,
    pub kind: Kind,
    pub unfinished: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Impact {
    pub direct_dependents: usize,
    pub transitive_dependents: usize,
    pub indirect_dependents: usize,
    pub immediately_ready: usize,
    pub still_blocked: usize,
    pub already_ready: usize,
    pub inactive_dependents: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Limits {
    pub available_nodes: usize,
    pub shown_nodes: usize,
    pub omitted_by_depth: usize,
    pub omitted_by_node_limit: usize,
    pub available_edges: usize,
    pub shown_edges: usize,
    pub omitted_edges: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostics {
    pub missing_references: usize,
    pub cycle_detected: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub format_version: u32,
    pub task_id: i64,
    pub direction: Direction,
    pub depth: u16,
    pub max_nodes: u32,
    pub max_edges: u32,
    pub focus: Node,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub impact: Impact,
    pub limits: Limits,
    pub diagnostics: Diagnostics,
}

struct Metadata {
    name: String,
    status: String,
    archived: bool,
    ready: bool,
    parent: Option<i64>,
}
struct Index {
    tasks: HashMap<i64, Metadata>,
    edges: Vec<Edge>,
    upstream: HashMap<i64, Vec<i64>>,
    downstream: HashMap<i64, Vec<i64>>,
    blockers: HashMap<i64, Vec<i64>>,
}
struct Reach {
    depths: HashMap<i64, usize>,
    order: Vec<i64>,
}

fn distances(start: i64, adjacency: &HashMap<i64, Vec<i64>>) -> Reach {
    let mut depths = HashMap::from([(start, 0)]);
    let mut order = vec![start];
    let mut pending = VecDeque::from([start]);
    while let Some(id) = pending.pop_front() {
        let depth = depths[&id] + 1;
        for next in adjacency.get(&id).into_iter().flatten() {
            if let std::collections::hash_map::Entry::Vacant(entry) = depths.entry(*next) {
                entry.insert(depth);
                pending.push_back(*next);
                order.push(*next);
            }
        }
    }
    Reach { depths, order }
}

impl Index {
    fn read(conn: &Connection) -> Result<Self> {
        let mut tasks = HashMap::new();
        let mut edges = Vec::new();
        let mut stmt = conn.prepare(&format!(
            "SELECT id,CASE WHEN instr(description,char(10))>0 THEN substr(description,1,instr(description,char(10))-1) ELSE description END,status,archived,parent_id,({READY_TASK_PREDICATE}) FROM tasks ORDER BY id"
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Metadata {
                    name: row.get(1)?,
                    status: row.get(2)?,
                    archived: row.get(3)?,
                    parent: row.get(4)?,
                    ready: row.get(5)?,
                },
            ))
        })?;
        for row in rows {
            let (id, meta) = row?;
            if let Some(parent) = meta.parent {
                edges.push(Edge {
                    dependent: id,
                    prerequisite: parent,
                    kind: Kind::Parent,
                    unfinished: false,
                });
            }
            tasks.insert(id, meta);
        }
        let mut stmt = conn.prepare("SELECT task_id,prerequisite_id FROM task_dependencies ORDER BY task_id,prerequisite_id")?;
        for row in stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))? {
            let (dependent, prerequisite) = row?;
            edges.push(Edge {
                dependent,
                prerequisite,
                kind: Kind::Prerequisite,
                unfinished: false,
            });
        }
        let mut upstream: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut downstream: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut blockers: HashMap<i64, Vec<i64>> = HashMap::new();
        for edge in &mut edges {
            edge.unfinished = tasks
                .get(&edge.prerequisite)
                .is_none_or(|meta| meta.status != "completed");
            upstream
                .entry(edge.dependent)
                .or_default()
                .push(edge.prerequisite);
            downstream
                .entry(edge.prerequisite)
                .or_default()
                .push(edge.dependent);
            if edge.unfinished {
                blockers
                    .entry(edge.dependent)
                    .or_default()
                    .push(edge.prerequisite);
            }
        }
        Ok(Self {
            tasks,
            edges,
            upstream,
            downstream,
            blockers,
        })
    }
    fn incomplete(&self, id: i64, completed: Option<i64>) -> usize {
        self.blockers
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|other| Some(**other) != completed)
            .copied()
            .collect::<HashSet<_>>()
            .len()
    }
    fn node(&self, id: i64) -> Node {
        let metadata = self.tasks.get(&id);
        let mut reasons = Vec::new();
        if let Some(meta) = metadata {
            if meta.archived {
                reasons.push(Reason::Archived);
            }
            match meta.status.as_str() {
                "new" => {
                    if meta.parent.is_some_and(|parent| {
                        self.tasks
                            .get(&parent)
                            .is_none_or(|meta| meta.status != "completed")
                    }) {
                        reasons.push(Reason::ParentNotCompleted);
                    }
                    if self.upstream.get(&id).into_iter().flatten().any(|other| {
                        Some(*other) != meta.parent
                            && self
                                .tasks
                                .get(other)
                                .is_none_or(|meta| meta.status != "completed")
                    }) {
                        reasons.push(Reason::PrerequisiteNotCompleted);
                    }
                }
                "in_progress" => reasons.push(Reason::InProgress),
                "error" => reasons.push(Reason::Error),
                "completed" => reasons.push(Reason::Completed),
                _ => reasons.push(Reason::UnknownStatus),
            }
        } else {
            reasons.push(Reason::MissingTask);
        }
        Node {
            id,
            task_name: metadata.map(|meta| meta.name.clone()),
            status: metadata.map(|meta| meta.status.clone()),
            archived: metadata.map(|meta| meta.archived),
            ready: metadata.is_some_and(|meta| meta.ready),
            reasons,
            unfinished_dependencies: self.incomplete(id, None),
            upstream_depth: None,
            downstream_depth: None,
            blocks_focus: false,
            impact: None,
            remaining_after_completion: None,
        }
    }
    fn cycle_detected(&self) -> bool {
        let mut degrees: HashMap<i64, usize> = self.tasks.keys().map(|id| (*id, 0)).collect();
        for edge in &self.edges {
            *degrees.entry(edge.dependent).or_default() += 1;
            degrees.entry(edge.prerequisite).or_default();
        }
        let mut pending: VecDeque<_> = degrees
            .iter()
            .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
            .collect();
        let mut seen = 0;
        while let Some(id) = pending.pop_front() {
            seen += 1;
            for dependent in self.downstream.get(&id).into_iter().flatten() {
                let degree = degrees.get_mut(dependent).expect("edge endpoints indexed");
                *degree -= 1;
                if *degree == 0 {
                    pending.push_back(*dependent);
                }
            }
        }
        seen != degrees.len()
    }
}

// Both traversals are minimum-hop ordered. Merge them without sorting the full graph.
fn displayed_ids(up: &Reach, down: &Reach, options: Options) -> (HashSet<i64>, usize, usize) {
    let up_order = if options.direction == Direction::Downstream {
        &[][..]
    } else {
        up.order.as_slice()
    };
    let down_order = if options.direction == Direction::Upstream {
        &[][..]
    } else {
        down.order.as_slice()
    };
    let mut all = HashSet::new();
    let mut eligible = HashSet::new();
    for (order, depths) in [(up_order, &up.depths), (down_order, &down.depths)] {
        for id in order {
            all.insert(*id);
            if depths[id] <= usize::from(options.depth) {
                eligible.insert(*id);
            }
        }
    }
    let mut selected = HashSet::new();
    let (mut a, mut b) = (0, 0);
    while selected.len() < options.max_nodes as usize {
        let first = up_order.get(a).map(|id| (*id, up.depths[id]));
        let second = down_order.get(b).map(|id| (*id, down.depths[id]));
        let chosen = match (first, second) {
            (Some(first), Some(second)) if first.1 <= second.1 => {
                a += 1;
                first
            }
            (_, Some(second)) => {
                b += 1;
                second
            }
            (Some(first), None) => {
                a += 1;
                first
            }
            (None, None) => break,
        };
        if chosen.1 > usize::from(options.depth) {
            break;
        }
        selected.insert(chosen.0);
    }
    (selected, eligible.len(), all.len() - eligible.len())
}

pub fn report(conn: &Connection, id: i64, options: Options) -> Result<Report> {
    let index = Index::read(conn)?;
    let focus = index
        .tasks
        .get(&id)
        .with_context(|| Info::missing_task(id))?;
    let upstream = distances(id, &index.upstream);
    let downstream = distances(id, &index.downstream);
    let blockers = if focus.status == "completed" {
        Reach {
            depths: HashMap::from([(id, 0)]),
            order: vec![id],
        }
    } else {
        distances(id, &index.blockers)
    };
    let direct: HashSet<_> = index
        .downstream
        .get(&id)
        .into_iter()
        .flatten()
        .filter(|other| **other != id && index.tasks.contains_key(other))
        .copied()
        .collect();
    let mut impact = Impact {
        direct_dependents: direct.len(),
        transitive_dependents: downstream
            .order
            .iter()
            .filter(|other| **other != id && index.tasks.contains_key(other))
            .count(),
        ..Default::default()
    };
    impact.indirect_dependents = impact.transitive_dependents - impact.direct_dependents;
    let mut classifications = HashMap::new();
    for dependent in direct {
        let meta = &index.tasks[&dependent];
        let remaining = index.incomplete(dependent, Some(id));
        let state = if meta.ready {
            impact.already_ready += 1;
            ImpactState::AlreadyReady
        } else if meta.status != "new" || meta.archived {
            impact.inactive_dependents += 1;
            ImpactState::Inactive
        } else if remaining == 0 && focus.status != "completed" {
            impact.immediately_ready += 1;
            ImpactState::ImmediatelyReady
        } else {
            impact.still_blocked += 1;
            ImpactState::StillBlocked
        };
        classifications.insert(dependent, (state, remaining));
    }
    let (selected, available_nodes, omitted_by_depth) =
        displayed_ids(&upstream, &downstream, options);
    let mut ids: Vec<_> = selected.iter().copied().collect();
    ids.sort_unstable();
    let nodes = ids
        .into_iter()
        .map(|node_id| {
            let mut node = index.node(node_id);
            if options.direction != Direction::Downstream {
                node.upstream_depth = upstream.depths.get(&node_id).copied();
            }
            if options.direction != Direction::Upstream {
                node.downstream_depth = downstream.depths.get(&node_id).copied();
            }
            node.blocks_focus = node_id != id && blockers.depths.contains_key(&node_id);
            if let Some((state, remaining)) = classifications.get(&node_id) {
                node.impact = Some(*state);
                node.remaining_after_completion = Some(*remaining);
            }
            node
        })
        .collect::<Vec<_>>();
    let mut edges = Vec::new();
    let mut available_edges = 0;
    for edge in &index.edges {
        if selected.contains(&edge.dependent) && selected.contains(&edge.prerequisite) {
            available_edges += 1;
            if edges.len() < options.max_edges as usize {
                edges.push(edge.clone());
            }
        }
    }
    let limits = Limits {
        available_nodes,
        shown_nodes: nodes.len(),
        omitted_by_depth,
        omitted_by_node_limit: available_nodes - nodes.len(),
        available_edges,
        shown_edges: edges.len(),
        omitted_edges: available_edges - edges.len(),
    };
    let diagnostics = Diagnostics {
        missing_references: index
            .edges
            .iter()
            .filter(|edge| {
                !index.tasks.contains_key(&edge.dependent)
                    || !index.tasks.contains_key(&edge.prerequisite)
            })
            .count(),
        cycle_detected: index.cycle_detected(),
    };
    let focus = nodes
        .iter()
        .find(|node| node.id == id)
        .cloned()
        .unwrap_or_else(|| index.node(id));
    Ok(Report {
        format_version: 1,
        task_id: id,
        direction: options.direction,
        depth: options.depth,
        max_nodes: options.max_nodes,
        max_edges: options.max_edges,
        focus,
        nodes,
        edges,
        impact,
        limits,
        diagnostics,
    })
}

fn node_line(node: &Node) -> String {
    let state = if node.ready {
        "ready".to_owned()
    } else {
        node.reasons
            .iter()
            .map(|reason| reason.label())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let impact = node
        .impact
        .map(|impact| {
            format!(
                "; {} ({} other unfinished)",
                impact.label(),
                node.remaining_after_completion.unwrap_or(0)
            )
        })
        .unwrap_or_default();
    format!(
        "#{} {} [{}]{}{}",
        node.id,
        node.task_name.as_deref().unwrap_or("<missing>"),
        state,
        if node.blocks_focus {
            "; blocks inspected task"
        } else {
            ""
        },
        impact
    )
}
pub fn lines(report: &Report) -> Vec<String> {
    let impact = &report.impact;
    let mut lines = vec![
        format!("Dependency graph #{}", report.task_id),
        node_line(&report.focus),
        format!("If only #{} were completed (hypothetical):", report.task_id),
        format!(
            "Direct dependents: {} | Transitive: {} (includes direct) | Indirect: {}",
            impact.direct_dependents, impact.transitive_dependents, impact.indirect_dependents
        ),
        format!(
            "Immediately ready: {} | Still blocked: {} | Already ready: {} | Inactive: {}",
            impact.immediately_ready,
            impact.still_blocked,
            impact.already_ready,
            impact.inactive_dependents
        ),
        "Totals cover full graph; newly ready tasks are not assumed completed.".into(),
    ];
    for (direction, heading) in [
        (Direction::Upstream, "Upstream prerequisites"),
        (Direction::Downstream, "Downstream dependents"),
    ] {
        if report.direction != Direction::Both && report.direction != direction {
            continue;
        }
        lines.push(String::new());
        lines.push(heading.into());
        let mut nodes: Vec<_> = report
            .nodes
            .iter()
            .filter_map(|node| {
                let depth = if direction == Direction::Upstream {
                    node.upstream_depth
                } else {
                    node.downstream_depth
                };
                depth.filter(|depth| *depth > 0).map(|depth| (depth, node))
            })
            .collect();
        nodes.sort_by_key(|(depth, node)| (*depth, node.id));
        if nodes.is_empty() {
            lines.push("  None displayed.".into());
        }
        for (depth, node) in nodes {
            lines.push(format!("  depth {depth}: {}", node_line(node)));
        }
    }
    lines.push(String::new());
    lines.push("Edges: dependent <- prerequisite".into());
    for edge in &report.edges {
        let kind = match edge.kind {
            Kind::Parent => "parent",
            Kind::Prerequisite => "prerequisite",
        };
        lines.push(format!(
            "  #{} <- #{} ({kind}{})",
            edge.dependent,
            edge.prerequisite,
            if edge.unfinished {
                ", unfinished"
            } else {
                ", completed"
            }
        ));
    }
    if report.edges.is_empty() {
        lines.push("  None displayed.".into());
    }
    let limits = &report.limits;
    lines.push(format!(
        "Displayed nodes: {}/{} within depth {}; edges: {}/{} between shown nodes",
        limits.shown_nodes,
        limits.available_nodes,
        report.depth,
        limits.shown_edges,
        limits.available_edges
    ));
    if limits.omitted_by_depth + limits.omitted_by_node_limit + limits.omitted_edges > 0 {
        lines.push(format!(
            "Truncated: {} nodes by depth, {} by node limit, {} edges by edge limit",
            limits.omitted_by_depth, limits.omitted_by_node_limit, limits.omitted_edges
        ));
    }
    if report.diagnostics.missing_references > 0 {
        lines.push(format!(
            "Project graph warning: {} missing references",
            report.diagnostics.missing_references
        ));
    }
    if report.diagnostics.cycle_detected {
        lines.push("Project graph warning: dependency cycle detected".into());
    }
    lines
        .into_iter()
        .map(|line| crate::output::clean(&line))
        .collect()
}
pub fn render(value: &serde_json::Value, columns: Option<usize>) -> String {
    let report: Report = serde_json::from_value(value.clone()).expect("graph output is typed");
    lines(&report)
        .into_iter()
        .map(|line| {
            let Some(columns) = columns else {
                return line;
            };
            let mut width = 0;
            line.graphemes(true)
                .take_while(|grapheme| {
                    width += grapheme.width();
                    width <= columns
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_uses_shortest_hops_once_even_with_cycles_and_many_shared_paths() {
        let graph = HashMap::from([
            (1, vec![2, 3]),
            (2, vec![4]),
            (3, vec![4, 5]),
            (4, vec![5, 1]),
        ]);
        let reach = distances(1, &graph);
        assert_eq!(reach.depths[&5], 2);
        assert_eq!(reach.depths.len(), 5);
        assert_eq!(reach.order.len(), 5);
        let (ids, available, depth_omitted) = displayed_ids(
            &reach,
            &reach,
            Options {
                depth: 1,
                max_nodes: 2,
                ..Options::default()
            },
        );
        assert!(ids.contains(&1));
        assert_eq!(ids.len(), 2);
        assert_eq!(available, 3);
        assert_eq!(depth_omitted, 2);
    }

    #[test]
    fn human_clipping_keeps_whole_graphemes_within_unicode_cell_width() {
        let report = Report {
            format_version: 1,
            task_id: 1,
            direction: Direction::Both,
            depth: 0,
            max_nodes: 1,
            max_edges: 1,
            focus: Node {
                id: 1,
                task_name: Some("雪👩‍💻é".into()),
                status: Some("new".into()),
                archived: Some(false),
                ready: true,
                reasons: vec![],
                unfinished_dependencies: 0,
                upstream_depth: Some(0),
                downstream_depth: Some(0),
                blocks_focus: false,
                impact: None,
                remaining_after_completion: None,
            },
            nodes: vec![],
            edges: vec![],
            impact: Impact::default(),
            limits: Limits {
                available_nodes: 1,
                shown_nodes: 1,
                omitted_by_depth: 0,
                omitted_by_node_limit: 0,
                available_edges: 0,
                shown_edges: 0,
                omitted_edges: 0,
            },
            diagnostics: Diagnostics {
                missing_references: 0,
                cycle_detected: false,
            },
        };
        for columns in 0..20 {
            let output = render(&serde_json::json!(report), Some(columns));
            assert!(output.lines().all(|line| line.width() <= columns));
            assert!(!output.contains("👩‍") || output.contains("👩‍💻"));
            assert!(!output.contains('e') || !output.contains("雪👩‍💻e") || output.contains("é"));
        }
    }
}
