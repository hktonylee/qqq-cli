mod buffers;
mod bulk;
mod clipboard;
pub(crate) mod completion;
mod conflict;
mod dashboard;
mod details;
pub mod draft;
mod graph_inspector;
mod handoff;
mod jump;
mod panel;
mod render;
mod tag_input;
mod view_picker;

use crate::config::AfterSaveNew;
use anyhow::{Result, bail, ensure};
use buffers::{DraftBuffers, DraftKey, ParkedDraft};
#[cfg(unix)]
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
    style::ResetColor,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use draft::{Composition, Draft};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::collections::{HashMap, HashSet};
use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct Outcome {
    pub composition: Composition,
    pub tags: Vec<String>,
    pub target_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub expected_revision: Option<i64>,
}

#[derive(Clone, Default)]
struct Baseline {
    description: String,
    revision: Option<i64>,
}

enum Target {
    New {
        parent_id: Option<i64>,
    },
    Task {
        id: i64,
        description: String,
        status: String,
        revision: i64,
        draft: Draft,
    },
    Retained {
        key: DraftKey,
        status: Option<String>,
        saved: ParkedDraft,
    },
}

enum Confirmation {
    Exit,
    ExitBuffers {
        keys: Vec<DraftKey>,
        index: usize,
    },
    Switch {
        target: Target,
        focus_editor: bool,
    },
    Action {
        action: TaskAction,
        dirty: bool,
        error: String,
    },
}

fn action_confirmation(action: TaskAction, dirty: bool) -> Confirmation {
    Confirmation::Action {
        action,
        dirty,
        error: String::new(),
    }
}

fn confirm_buffers_exit(buffers: &DraftBuffers) -> Option<Confirmation> {
    let keys = buffers.keys();
    (!keys.is_empty()).then_some(Confirmation::ExitBuffers { keys, index: 0 })
}

fn buffer_exit_lines(key: DraftKey, buffers: &DraftBuffers, width: usize) -> Vec<render::PopupRow> {
    use render::{PopupKind, PopupRow};
    let mut rows = vec![PopupRow::new(
        format!("Discard {}?", key.label()),
        PopupKind::Heading,
    )];
    if let Some(saved) = buffers.get(key) {
        rows.extend(
            wrap_modal(&saved.draft.fragments().concat(), width)
                .into_iter()
                .take(3)
                .map(|line| PopupRow::new(line, PopupKind::Warning)),
        );
    }
    rows.push(PopupRow::new(
        "y continue  n/Esc cancel exit",
        PopupKind::Hint,
    ));
    rows
}

#[derive(Clone)]
pub enum TaskAction {
    Complete(i64),
    MarkError(i64, String),
    ForceComplete(i64),
    Retry(i64),
    Reopen(i64),
    SetArchived(i64, bool),
    Priority(i64, i64),
    Tags(i64, Vec<String>),
    Parent(i64, crate::db::ParentChange),
}

impl TaskAction {
    fn is_completion(&self) -> bool {
        matches!(self, Self::Complete(_) | Self::ForceComplete(_))
    }

    fn id(&self) -> i64 {
        match self {
            Self::Complete(id)
            | Self::MarkError(id, _)
            | Self::ForceComplete(id)
            | Self::Retry(id)
            | Self::Reopen(id)
            | Self::SetArchived(id, _)
            | Self::Tags(id, _)
            | Self::Priority(id, _)
            | Self::Parent(id, _) => *id,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Complete(_) => "Complete",
            Self::MarkError(..) => "Mark error",
            Self::ForceComplete(_) => "Force complete",
            Self::Retry(_) => "Retry",
            Self::Reopen(_) => "Reopen",
            Self::SetArchived(_, true) => "Archive",
            Self::SetArchived(_, false) => "Unarchive",
            Self::Priority(_, _) => "Set priority",
            Self::Tags(..) => "Set tags",
            Self::Parent(_, _) => "Set parent",
        }
    }

    fn success(&self) -> String {
        match self {
            Self::Complete(id) | Self::ForceComplete(id) => format!("Completed #{id}"),
            Self::MarkError(id, _) => format!("Marked error #{id}"),
            Self::Retry(id) => format!("Retried #{id}"),
            Self::Reopen(id) => format!("Reopened #{id}"),
            Self::SetArchived(id, true) => format!("Archived #{id}"),
            Self::SetArchived(id, false) => format!("Unarchived #{id}"),
            Self::Tags(id, _) => format!("Tags saved #{id}"),
            Self::Priority(id, priority) => format!("Priority #{id}: {priority}"),
            Self::Parent(id, crate::db::ParentChange::Set(parent)) => {
                format!("Parent #{id}: #{parent}")
            }
            Self::Parent(id, crate::db::ParentChange::Clear) => format!("Parent cleared #{id}"),
        }
    }
}

#[derive(Clone, Copy)]
enum ActionInputKind {
    Priority,
    Parent,
    ErrorReason,
    Tags,
}

const ACTION_MENU_GROUPS: [&[(char, &str)]; 3] = [
    &[
        ('c', "Complete"),
        ('e', "Mark error"),
        ('r', "Retry error"),
        ('o', "Reopen"),
    ],
    &[('p', "Priority"), ('d', "Set parent")],
    &[('a', "Archive")],
];

fn action_menu_items(can_retry: bool) -> impl Iterator<Item = (char, &'static str)> {
    ACTION_MENU_GROUPS
        .into_iter()
        .flatten()
        .copied()
        .filter(move |(key, _)| can_retry || *key != 'r')
}

enum ActionUi {
    Menu {
        id: i64,
        archived: bool,
        can_retry: bool,
        selected: usize,
    },
    Input {
        id: Option<i64>,
        kind: ActionInputKind,
        value: String,
        error: String,
        cursor: tag_input::Cursor,
    },
    Error {
        text: String,
        top: usize,
    },
}

fn wrap_modal(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let word_width = word.width();
        if !line.is_empty() && line.width() + 1 + word_width > width {
            lines.push(std::mem::take(&mut line));
        }
        if word_width > width {
            for grapheme in word.graphemes(true) {
                if !line.is_empty() && line.width() + grapheme.width() > width {
                    lines.push(std::mem::take(&mut line));
                }
                line.push_str(grapheme);
            }
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn action_lines(ui: &ActionUi, width: usize, height: usize) -> Vec<render::PopupRow> {
    use render::{PopupKind, PopupRow};
    match ui {
        ActionUi::Menu {
            id,
            archived,
            can_retry,
            selected,
        } => {
            let mut lines = vec![PopupRow::new(
                format!("Task actions #{id}"),
                PopupKind::Heading,
            )];
            let mut action_index = 0;
            for (group_index, group) in ACTION_MENU_GROUPS.into_iter().enumerate() {
                if group_index > 0 {
                    lines.push(PopupRow::new("", PopupKind::Body));
                }
                for &(key, label) in group {
                    if key == 'r' && !can_retry {
                        continue;
                    }
                    let label = if key == 'a' && *archived {
                        "Unarchive"
                    } else {
                        label
                    };
                    let kind = if action_index == *selected {
                        PopupKind::SelectedAction
                    } else {
                        PopupKind::Action
                    };
                    lines.push(PopupRow::new(format!("{key} {label}"), kind));
                    action_index += 1;
                }
            }
            lines.push(PopupRow::new(
                if width >= "Up/Down Select  Enter Apply  Esc Cancel".len() {
                    "Up/Down Select  Enter Apply  Esc Cancel"
                } else {
                    "Up/Dn Enter"
                },
                PopupKind::Hint,
            ));
            if lines.len() > height {
                let hint = lines.pop().expect("menu has hint");
                let heading = lines.remove(0);
                let selected_row = lines
                    .iter()
                    .position(|row| row.kind == PopupKind::SelectedAction)
                    .expect("menu selection is valid");
                let body_height =
                    height.saturating_sub(usize::from(height >= 2) + usize::from(height >= 3));
                let mut start = selected_row.saturating_sub(body_height.saturating_sub(1));
                if lines[start].text.is_empty() {
                    start += 1;
                }
                lines = lines.into_iter().skip(start).take(body_height).collect();
                if height >= 3 {
                    lines.insert(0, heading);
                }
                if height >= 2 {
                    lines.push(hint);
                }
            }
            lines
        }
        ActionUi::Input {
            id,
            kind,
            value,
            error,
            cursor,
        } => {
            let task_label = id.map(|id| format!("#{id}")).unwrap_or_default();
            let mut lines = match kind {
                ActionInputKind::Priority => {
                    vec![
                        PopupRow::new(format!("Priority task {task_label}"), PopupKind::Heading),
                        PopupRow::new("Enter -100..100", PopupKind::Hint),
                    ]
                }
                ActionInputKind::Parent => {
                    vec![
                        PopupRow::new(format!("Parent task {task_label}"), PopupKind::Heading),
                        PopupRow::new("ID / none", PopupKind::Hint),
                    ]
                }
                ActionInputKind::Tags => {
                    return tag_input::rows(*id, value, cursor, error, width, height);
                }
                ActionInputKind::ErrorReason => vec![
                    PopupRow::new(format!("Error task {task_label}"), PopupKind::Heading),
                    PopupRow::new("Enter error reason", PopupKind::Hint),
                ],
            };
            lines.push(PopupRow::new(format!("> {value}"), PopupKind::Input));
            if !error.is_empty() {
                lines.extend(
                    wrap_modal(error, width)
                        .into_iter()
                        .map(|text| PopupRow::new(text, PopupKind::Error)),
                );
            }
            lines.push(PopupRow::new("Enter apply  Esc cancel", PopupKind::Hint));
            lines
        }
        ActionUi::Error { text, top } => {
            let wrapped = wrap_modal(text, width);
            let available = height.saturating_sub(2);
            let start = (*top).min(wrapped.len().saturating_sub(available));
            let mut lines = vec![PopupRow::new("Action error", PopupKind::Error)];
            lines.extend(
                wrapped
                    .into_iter()
                    .skip(start)
                    .take(available)
                    .map(|text| PopupRow::new(text, PopupKind::Error)),
            );
            lines.push(PopupRow::new("Up/Down Esc", PopupKind::Hint));
            lines
        }
    }
}

fn action_confirmation_lines(
    action: &TaskAction,
    dirty: bool,
    error: &str,
    width: usize,
    height: usize,
) -> Vec<render::PopupRow> {
    use render::{PopupKind, PopupRow};
    let mut rows = vec![PopupRow::new(
        format!("{} task #{}?", action.label(), action.id()),
        PopupKind::Heading,
    )];
    if matches!(action, TaskAction::ForceComplete(_)) {
        rows.extend(
            wrap_modal("Complete without matching task owner.", width)
                .into_iter()
                .map(|text| PopupRow::new(text, PopupKind::Warning)),
        );
    }
    if let TaskAction::MarkError(_, reason) = action {
        rows.extend(
            wrap_modal(&format!("Reason: {reason}"), width)
                .into_iter()
                .take(3)
                .map(|text| PopupRow::new(text, PopupKind::Hint)),
        );
    }
    rows.push(PopupRow::new(
        if dirty { "Lose draft?" } else { "" },
        PopupKind::Warning,
    ));
    if action.is_completion() {
        rows.truncate(height.saturating_sub(1));
        let available = height.saturating_sub(rows.len() + 1);
        if !error.is_empty() {
            rows.extend(
                wrap_modal(error, width)
                    .into_iter()
                    .take(available)
                    .map(|line| PopupRow::new(line, PopupKind::Error)),
            );
        }
    }
    let hint = if action.is_completion() {
        if width >= "y confirm  Y force  n/Esc cancel".len() {
            "y confirm  Y force  n/Esc cancel"
        } else if width >= "y Y force Esc".len() {
            "y Y force Esc"
        } else {
            "y Y Esc"
        }
    } else {
        "y confirm  n/Esc cancel"
    };
    rows.push(PopupRow::new(hint, PopupKind::Hint));
    rows
}

fn parse_action_input(
    id: i64,
    kind: ActionInputKind,
    value: &str,
) -> std::result::Result<TaskAction, String> {
    match kind {
        ActionInputKind::Priority => {
            let priority = value
                .trim()
                .parse::<i64>()
                .map_err(|_| "Priority must be -100..100".to_owned())?;
            if !(-100..=100).contains(&priority) {
                return Err("Priority must be -100..100".to_owned());
            }
            Ok(TaskAction::Priority(id, priority))
        }
        ActionInputKind::Parent => value
            .trim()
            .parse()
            .map(|parent| TaskAction::Parent(id, parent)),
        ActionInputKind::Tags => crate::tags::parse_lines(value)
            .map(|tags| TaskAction::Tags(id, tags))
            .map_err(|error| error.to_string()),
        ActionInputKind::ErrorReason => {
            let reason = value.trim();
            if reason.is_empty() {
                return Err("Error reason cannot be empty".into());
            }
            Ok(TaskAction::MarkError(id, reason.to_owned()))
        }
    }
}

fn task_target(db: &crate::db::Db, id: i64) -> Result<Target> {
    task_target_with_archived(db, id).map(|(target, _)| target)
}

fn task_target_with_archived(db: &crate::db::Db, id: i64) -> Result<(Target, bool)> {
    let snapshot = db.content_snapshot(id)?;
    let archived = snapshot.task.archived;
    let draft = Draft::from_saved(&snapshot.task.description, id, &snapshot.references)?;
    Ok((
        Target::Task {
            id,
            description: snapshot.task.description,
            status: snapshot.task.status,
            revision: snapshot.task.content_revision,
            draft,
        },
        archived,
    ))
}

fn adjacent_target(
    db: &crate::db::Db,
    current: Option<i64>,
    older: bool,
    include_archived: bool,
    visible_ids: Option<&[i64]>,
) -> Result<Option<Target>> {
    if !older && current.is_none() {
        return Ok(None);
    }
    let found = if let Some(ids) = visible_ids {
        panel::adjacent_visible_id(ids, current, older)
            .map(|id| {
                db.task(id)
                    .map(|task| (task.id, task.description, task.status))
            })
            .transpose()?
    } else {
        db.adjacent_task(current, older, include_archived)?
    };
    Ok(match found {
        Some((id, _, _)) => Some(task_target(db, id)?),
        None if !older => Some(Target::New { parent_id: None }),
        None => None,
    })
}

fn restore_target(target: Target, buffers: &mut DraftBuffers) -> Target {
    let key = match &target {
        Target::New { parent_id } => DraftKey::New(*parent_id),
        Target::Task { id, .. } => DraftKey::Task(*id),
        Target::Retained { .. } => return target,
    };
    let Some(saved) = buffers.take(key) else {
        return target;
    };
    let status = match target {
        Target::Task { status, .. } => Some(status),
        Target::New { .. } => None,
        Target::Retained { .. } => unreachable!("retained target returned above"),
    };
    Target::Retained { key, status, saved }
}

fn load_target(
    target: Target,
    draft: &mut Draft,
    target_id: &mut Option<i64>,
    target_status: &mut Option<String>,
    draft_parent_id: &mut Option<i64>,
    baseline: &mut Baseline,
    top: &mut usize,
) -> bool {
    match target {
        Target::New { parent_id } => {
            *target_id = None;
            *target_status = None;
            *draft_parent_id = parent_id;
            *baseline = Baseline::default();
            *draft = Draft::new("");
        }
        Target::Task {
            id,
            description,
            status,
            revision,
            draft: loaded,
        } => {
            *target_id = Some(id);
            *target_status = Some(status);
            *draft_parent_id = None;
            *baseline = Baseline {
                description,
                revision: Some(revision),
            };
            *draft = loaded;
        }
        Target::Retained { key, status, saved } => {
            *target_id = match key {
                DraftKey::Task(id) => Some(id),
                DraftKey::New(_) => None,
            };
            *draft_parent_id = match key {
                DraftKey::Task(_) => None,
                DraftKey::New(parent) => parent,
            };
            *target_status = status;
            *baseline = saved.baseline;
            *draft = saved.draft;
            *top = saved.top;
            return saved.follow_cursor;
        }
    }
    *top = 0;
    true
}

struct TerminalGuard {
    color: bool,
    mouse: bool,
}
impl TerminalGuard {
    fn enter(mouse: bool) -> Result<Self> {
        ensure!(
            io::stdin().is_terminal() && io::stderr().is_terminal(),
            "Interactive editor requires terminal input and stderr"
        );
        terminal::enable_raw_mode()?;
        let guard = Self {
            color: crate::output::color_enabled(io::stderr().is_terminal()),
            mouse,
        };
        execute!(
            io::stderr(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            Show
        )?;
        if mouse {
            execute!(io::stderr(), EnableMouseCapture)?;
            #[cfg(unix)]
            execute!(
                io::stderr(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }
        Ok(guard)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.mouse {
            #[cfg(unix)]
            let _ = execute!(io::stderr(), PopKeyboardEnhancementFlags);
            let _ = execute!(io::stderr(), DisableMouseCapture);
        }
        if self.color {
            let _ = execute!(io::stderr(), ResetColor);
        }
        let _ = execute!(
            io::stderr(),
            DisableBracketedPaste,
            LeaveAlternateScreen,
            Show
        );
        let _ = terminal::disable_raw_mode();
    }
}
fn paste(draft: &mut Draft, text: &str) -> Result<()> {
    if let Some(image) = clipboard::path_image(text)? {
        draft.image(image)?;
    } else {
        draft.paste(text);
    }
    Ok(())
}
enum Mode<'a, 'b> {
    Single(Option<&'a crate::db::Db>),
    Edit {
        db: &'a mut crate::db::Db,
        save: &'b mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
    },
    Continuous {
        db: &'a mut crate::db::Db,
        save: &'b mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
        action: Option<&'b mut ActionHandler<'b>>,
        completion: Option<&'b mut CompletionHandler<'b>>,
        dashboard: bool,
        include_archived: bool,
        after_save_new: AfterSaveNew,
        view_context: Option<Box<crate::views::ViewContext>>,
    },
}
type ActionHandler<'a> = dyn FnMut(&mut crate::db::Db, TaskAction) -> Result<crate::db::Task> + 'a;
type CompletionHandler<'a> = dyn FnMut(&crate::db::Db, i64) -> Result<TaskAction> + 'a;
impl Mode<'_, '_> {
    fn db(&self) -> Option<&crate::db::Db> {
        match self {
            Self::Single(db) => *db,
            Self::Continuous { db, .. } | Self::Edit { db, .. } => Some(db),
        }
    }

    fn task_action(&mut self, action: TaskAction) -> Result<crate::db::Task> {
        match self {
            Self::Continuous {
                db,
                action: Some(handler),
                ..
            } => handler(db, action),
            _ => bail!("Task actions require dashboard"),
        }
    }

    fn completion_action(&mut self, id: i64) -> Result<TaskAction> {
        match self {
            Self::Continuous {
                db,
                completion: Some(handler),
                ..
            } => handler(db, id),
            _ => bail!("Task actions require dashboard"),
        }
    }

    fn bulk_apply(&mut self, report: &crate::bulk::Report) -> Result<crate::bulk::Report> {
        match self {
            Self::Continuous {
                db,
                dashboard: true,
                ..
            } => crate::bulk::apply(db, report, "tui"),
            _ => bail!("Bulk actions require dashboard"),
        }
    }
}
fn run_action(
    mode: &mut Mode<'_, '_>,
    action: TaskAction,
    include_archived: bool,
) -> Result<Target> {
    let task = mode.task_action(action)?;
    let db = mode.db().expect("task actions have database");
    if task.archived && !include_archived {
        return Ok(Target::New { parent_id: None });
    }
    task_target(db, task.id)
}
fn cancel(saved_any: bool, dashboard: bool) -> Result<Option<Outcome>> {
    if saved_any || dashboard {
        Ok(None)
    } else {
        bail!("Editor cancelled; task not saved")
    }
}
pub fn compose(description: &str, navigation: Option<&crate::db::Db>) -> Result<Outcome> {
    compose_inner(description, Mode::Single(navigation), None)
        .map(|saved| saved.expect("single editor returns saved composition"))
}
pub fn compose_existing(
    db: &mut crate::db::Db,
    snapshot: crate::db::ContentSnapshot,
    description: Option<&str>,
    save: &mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
) -> Result<()> {
    let task = snapshot.task;
    let draft = Draft::from_saved(
        description.unwrap_or(&task.description),
        task.id,
        &snapshot.references,
    )?;
    let target = Target::Task {
        id: task.id,
        description: task.description,
        status: task.status,
        revision: task.content_revision,
        draft,
    };
    compose_inner("", Mode::Edit { db, save }, Some(target)).map(|_| ())
}
pub fn compose_continuously(
    db: &mut crate::db::Db,
    save: &mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
) -> Result<()> {
    compose_inner(
        "",
        Mode::Continuous {
            db,
            save,
            action: None,
            completion: None,
            dashboard: false,
            include_archived: false,
            after_save_new: AfterSaveNew::OpenNew,
            view_context: None,
        },
        None,
    )
    .map(|_| ())
}
pub fn compose_dashboard(
    db: &mut crate::db::Db,
    include_archived: bool,
    after_save_new: AfterSaveNew,
    view_context: crate::views::ViewContext,
    save: &mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
    completion: &mut CompletionHandler<'_>,
    action: &mut ActionHandler<'_>,
) -> Result<()> {
    compose_inner(
        "",
        Mode::Continuous {
            db,
            save,
            action: Some(action),
            completion: Some(completion),
            dashboard: true,
            include_archived,
            after_save_new,
            view_context: Some(Box::new(view_context)),
        },
        None,
    )
    .map(|_| ())
}
#[derive(Clone)]
struct DashboardSnapshot {
    tasks: Vec<crate::db::Task>,
    selected: Vec<crate::db::Task>,
    version: i64,
    details: Vec<render::DetailRow>,
    has_herdr_link: bool,
}

fn dashboard_snapshot(
    db: &crate::db::Db,
    target_id: Option<i64>,
    size: (u16, u16),
    selection: &crate::selection::Prepared,
    query: &str,
) -> Result<DashboardSnapshot> {
    // Capture before reads so commits during rendering trigger another refresh.
    let version = db.data_version()?;
    let tasks = if selection.include_archived || target_id.is_some() {
        db.list_with_archived(None, true)?
    } else {
        db.list(None)?
    };
    let area = dashboard::panes(ratatui::layout::Rect::new(0, 0, size.0, size.1)).details;
    let width = usize::from(dashboard::details_content(area).width).max(1);
    let details = match target_id.filter(|_| area.height > 0) {
        Some(id) => match tasks.iter().find(|task| task.id == id) {
            Some(task) => details::rows(&db.show_task(task)?, width),
            None => details::unavailable(id, width),
        },
        None => Vec::new(),
    };
    let has_herdr_link = match target_id {
        Some(id) => db.link(id)?.is_some(),
        None => false,
    };
    let selected = crate::selection::list(db, selection, Some(query))?;
    Ok(DashboardSnapshot {
        tasks,
        selected,
        version,
        details,
        has_herdr_link,
    })
}

fn compose_inner(
    description: &str,
    mut mode: Mode<'_, '_>,
    initial_target: Option<Target>,
) -> Result<Option<Outcome>> {
    let dashboard = matches!(
        mode,
        Mode::Continuous {
            dashboard: true,
            ..
        }
    );
    let mut view_context = match &mut mode {
        Mode::Continuous { view_context, .. } => view_context.take(),
        _ => None,
    };
    let mut include_archived = matches!(
        mode,
        Mode::Continuous {
            include_archived: true,
            ..
        }
    );
    if let Some(context) = &view_context {
        include_archived = context.prepared.include_archived;
    }
    let after_save_new = match &mode {
        Mode::Continuous { after_save_new, .. } => *after_save_new,
        Mode::Single(_) | Mode::Edit { .. } => AfterSaveNew::default(),
    };
    let mut terminal = TerminalGuard::enter(dashboard)?;
    let mut dashboard_terminal = if dashboard {
        Some(Terminal::new(CrosstermBackend::new(io::stderr()))?)
    } else {
        None
    };
    let mut draft = Draft::new(description);
    let mut baseline = Baseline {
        description: description.to_owned(),
        revision: None,
    };
    let mut target_id = None;
    let mut target_status = None;
    let mut draft_parent_id = None;
    let mut top = 0;
    if let Some(target) = initial_target {
        load_target(
            target,
            &mut draft,
            &mut target_id,
            &mut target_status,
            &mut draft_parent_id,
            &mut baseline,
            &mut top,
        );
    }
    let mut list_top = 0;
    let mut details_top = 0;
    let mut details_id = None;
    let mut list_follow_selected = true;
    let mut editor_follow_cursor = true;
    let mut filter_query = String::new();
    let mut filter_focused = false;
    let mut completed_limit = view_context
        .as_ref()
        .and_then(|context| context.prepared.max_completed);
    let mut show_completed = completed_limit != Some(0);
    let mut message = String::new();
    let mut message_is_error = false;
    let mut confirmation: Option<Confirmation> = None;
    let mut action_ui: Option<ActionUi> = None;
    let mut jump_ui: Option<jump::View> = None;
    let mut conflict_ui: Option<conflict::View> = None;
    let mut view_ui: Option<view_picker::Picker> = None;
    let mut bulk_ui: Option<bulk::View> = None;
    let mut bulk_selected = std::collections::BTreeSet::new();
    let mut graph_ui: Option<graph_inspector::Inspector> = None;
    let mut graph_background: Option<DashboardSnapshot> = None;
    let mut save_revision_override = None;
    let mut saved_any = false;
    let mut buffers = DraftBuffers::default();
    loop {
        let size = terminal::size()?;
        let filter_visible =
            dashboard::filter_visible(&filter_query, filter_focused, show_completed);
        if details_id != target_id {
            details_top = 0;
            details_id = target_id;
        }
        let fragments = draft.fragments();
        let image_mask = draft.image_mask();
        let paste_mask = draft.paste_mask();
        let layout = if paste_mask.contains(&true) {
            render::Layout::with_paste(&fragments, &image_mask, &paste_mask, size.0 as usize)
        } else {
            render::Layout::new(&fragments, &image_mask, size.0 as usize)
        };
        let prepared_selection = dashboard.then(|| {
            let mut selection = view_context
                .as_ref()
                .expect("dashboard has view context")
                .prepared
                .clone();
            selection.include_archived = include_archived;
            selection.max_completed = if !show_completed {
                Some(0)
            } else {
                completed_limit.filter(|limit| *limit != 0)
            };
            selection
        });
        let (dashboard_tasks, selected_tasks, list_version, details_rows, has_herdr_link) =
            if dashboard {
                let db = mode.db().expect("dashboard has database");
                let mut frame = match dashboard_snapshot(
                    db,
                    target_id,
                    size,
                    prepared_selection
                        .as_ref()
                        .expect("dashboard prepares selection"),
                    &filter_query,
                ) {
                    Ok(frame) => {
                        if graph_ui.is_some() {
                            graph_background = Some(frame.clone());
                        } else {
                            graph_background = None;
                        }
                        frame
                    }
                    Err(error) => {
                        let Some(mut previous) = graph_background.clone() else {
                            return Err(error);
                        };
                        previous.version = db.data_version().unwrap_or(previous.version);
                        if let Some(ui) = &mut graph_ui {
                            ui.set_error(&error, previous.version);
                        } else {
                            message = format!("{error:#}; local drafts kept");
                            message_is_error = true;
                        }
                        previous
                    }
                };
                if let Some(task) = frame.tasks.iter().find(|task| Some(task.id) == target_id) {
                    target_status = Some(task.status.clone());
                    if let Some(ActionUi::Menu {
                        can_retry,
                        selected,
                        ..
                    }) = &mut action_ui
                    {
                        let retry = task.status == "error";
                        if *can_retry != retry {
                            let key = action_menu_items(*can_retry)
                                .nth(*selected)
                                .expect("menu selection is valid")
                                .0;
                            *can_retry = retry;
                            *selected = action_menu_items(retry)
                                .position(|(candidate, _)| candidate == key)
                                .unwrap_or_else(|| {
                                    (*selected).min(action_menu_items(retry).count() - 1)
                                });
                        }
                    }
                }
                if !include_archived {
                    frame.tasks.retain(|task| !task.archived);
                }
                (
                    Some(frame.tasks),
                    Some(frame.selected),
                    Some(frame.version),
                    frame.details,
                    frame.has_herdr_link,
                )
            } else {
                (None, None, None, Vec::new(), false)
            };
        let buffer_footer = match &confirmation {
            Some(Confirmation::ExitBuffers { keys, index }) => {
                let key = keys[*index];
                let full = format!("Discard {}? (y/N)", key.label());
                if full.len() <= usize::from(size.0) {
                    full
                } else {
                    match key {
                        DraftKey::Task(id) => format!("Drop #{id}? y/N"),
                        DraftKey::New(None) => "Drop new? y/N".into(),
                        DraftKey::New(Some(id)) => format!("Drop P{id}? y/N"),
                    }
                }
            }
            _ => String::new(),
        };
        let footer = match &confirmation {
            Some(Confirmation::ExitBuffers { .. }) => &buffer_footer,
            Some(Confirmation::Exit) if usize::from(size.0) < "Discard draft? (y/N)".len() => {
                "Discard? y/N"
            }
            Some(Confirmation::Exit) => "Discard draft? (y/N)",
            Some(Confirmation::Switch { .. })
                if usize::from(size.0) < "Discard changes and switch? (y/N)".len() =>
            {
                "Switch? y/N"
            }
            Some(Confirmation::Switch { .. }) => "Discard changes and switch? (y/N)",
            Some(Confirmation::Action { .. }) => "Confirm action? (y/N)",
            None => &message,
        };
        let active_dirty = draft.is_dirty_against(&baseline.description);
        let (mut title, status_start) = match (mode.db().is_some(), target_id) {
            (true, Some(id)) => {
                let prefix = format!("Task Editor - Task #{id} ");
                let label = target_status
                    .as_deref()
                    .map_or("Removed", crate::output::status_label);
                (format!("{prefix}({label})"), Some(prefix.len()))
            }
            (true, None) => (
                match draft_parent_id {
                    Some(parent) => format!("Task Editor - New Task (parent #{parent})"),
                    None => "Task Editor - New Task".to_owned(),
                },
                None,
            ),
            (false, _) => ("Task Editor".to_owned(), None),
        };
        if dashboard && target_id.is_none() && active_dirty {
            title.push_str(" [*]");
        }
        if let Some(view) = view_context
            .as_ref()
            .and_then(|context| context.active.as_ref())
        {
            title.push_str(&format!(" · View: {}", view.name));
        }
        let chrome = render::Chrome {
            title: &title,
            title_status_color: if dashboard {
                None
            } else {
                status_start.zip(
                    target_status
                        .as_deref()
                        .and_then(crate::output::status_color_code),
                )
            },
            keys: if filter_focused && has_herdr_link {
                render::FILTER_HERDR_KEYS
            } else if filter_focused {
                render::FILTER_KEYS
            } else if dashboard && has_herdr_link {
                render::DASHBOARD_HERDR_KEYS
            } else if dashboard && target_id.is_none() && bulk_selected.is_empty() {
                render::DASHBOARD_NEW_KEYS
            } else if dashboard {
                render::DASHBOARD_KEYS
            } else {
                match &mode {
                    Mode::Continuous { .. } => render::ADD_KEYS,
                    Mode::Single(Some(_)) => render::NAV_KEYS,
                    Mode::Single(None) => render::KEYS,
                    Mode::Edit { .. } => render::KEYS,
                }
            },
            message: footer,
        };
        let mut visible_ids = None;
        let mut list_row_count = 0;
        let mut rows = Vec::new();
        if dashboard {
            let snapshot = dashboard_tasks
                .as_ref()
                .expect("dashboard has task snapshot");
            let selection = prepared_selection
                .as_ref()
                .expect("dashboard prepares selection");
            let tasks = selected_tasks
                .as_ref()
                .expect("dashboard has selected tasks");
            let filter_views: Vec<_> = tasks
                .iter()
                .map(|task| panel::FilterTask {
                    id: task.id,
                    parent_id: task.parent_id,
                    description: &task.description,
                    status: &task.status,
                })
                .collect();
            let displayed: Vec<_> = tasks.iter().collect();
            let statuses: HashMap<_, _> = snapshot
                .iter()
                .map(|task| (task.id, task.status.as_str()))
                .collect();
            let mut dirty_ids: HashSet<_> = buffers.task_ids().collect();
            if let Some(id) = target_id.filter(|_| active_dirty) {
                dirty_ids.insert(id);
            }
            rows = if displayed.is_empty()
                && (!snapshot.is_empty() || selection.filter.is_some() || !filter_query.is_empty())
            {
                Vec::new()
            } else {
                let list_width = usize::from(
                    dashboard::panes(ratatui::layout::Rect::new(0, 0, size.0, size.1))
                        .list
                        .width,
                )
                .saturating_sub(dashboard::LIST_ROW_PREFIX.len());
                let tree = crate::output::render(
                    if size.0 < dashboard::COMPACT_COLUMNS {
                        crate::output::Format::CompactTasks
                    } else {
                        crate::output::Format::Tasks
                    },
                    &serde_json::json!(displayed),
                    false,
                    Some(list_width),
                );
                panel::rows(&tree, list_width)
            };
            let displayed_views: Vec<_> = filter_views
                .iter()
                .map(|task| panel::FilterTask {
                    id: task.id,
                    parent_id: task.parent_id,
                    description: task.description,
                    status: task.status,
                })
                .collect();
            panel::set_dirty_markers(
                &mut rows,
                &displayed_views,
                &dirty_ids,
                size.0 >= dashboard::COMPACT_COLUMNS,
            );
            let tag_tasks: Vec<_> = displayed
                .iter()
                .map(|task| panel::TagTask {
                    id: task.id,
                    tags: &task.tags,
                    archived: task.archived,
                    context_only: task.context_only,
                })
                .collect();
            panel::set_tag_ranges(&mut rows, &tag_tasks);
            list_row_count = rows.len();
            visible_ids = Some(panel::visible_ids(&rows));
            let popup_content = dashboard::popup_layout(
                ratatui::layout::Rect::new(0, 0, size.0, size.1),
                usize::MAX,
            )
            .content;
            if let Some(ui) = &mut graph_ui {
                ui.refresh(
                    mode.db().expect("dashboard has database"),
                    list_version.expect("dashboard captures version"),
                );
            }
            let modal_lines = graph_ui
                .as_mut()
                .map(|ui| {
                    ui.rows(
                        usize::from(popup_content.width),
                        usize::from(popup_content.height),
                    )
                })
                .or_else(|| {
                    view_ui.as_mut().map(|ui| {
                        ui.rows(
                            usize::from(popup_content.width),
                            usize::from(popup_content.height),
                        )
                    })
                })
                .or_else(|| {
                    conflict_ui.as_ref().map(|ui| {
                        ui.rows(
                            usize::from(popup_content.width),
                            usize::from(popup_content.height),
                        )
                    })
                })
                .or_else(|| {
                    jump_ui
                        .as_ref()
                        .map(|ui| ui.rows(usize::from(popup_content.width)))
                })
                .or_else(|| {
                    bulk_ui.as_mut().map(|ui| {
                        ui.rows(
                            usize::from(popup_content.width),
                            usize::from(popup_content.height),
                        )
                    })
                })
                .or_else(|| {
                    action_ui.as_ref().map(|ui| {
                        action_lines(
                            ui,
                            usize::from(popup_content.width),
                            usize::from(popup_content.height),
                        )
                    })
                })
                .or_else(|| match &confirmation {
                    Some(Confirmation::ExitBuffers { keys, index }) => Some(buffer_exit_lines(
                        keys[*index],
                        &buffers,
                        usize::from(popup_content.width),
                    )),
                    Some(Confirmation::Action {
                        action,
                        dirty,
                        error,
                    }) => Some(action_confirmation_lines(
                        action,
                        *dirty,
                        error,
                        usize::from(popup_content.width),
                        usize::from(popup_content.height),
                    )),
                    _ => None,
                });
            dashboard_terminal
                .as_mut()
                .expect("dashboard has Ratatui terminal")
                .draw(|frame| {
                    dashboard::draw(
                        frame,
                        &rows,
                        &statuses,
                        target_id,
                        dashboard::View {
                            query: &filter_query,
                            focused: filter_focused,
                            show_completed,
                            bulk_selected: Some(&bulk_selected),
                            top: &mut list_top,
                            follow_selected: list_follow_selected,
                            modal_lines: modal_lines.as_deref(),
                            hide_cursor: matches!(
                                &confirmation,
                                Some(Confirmation::Action { action, .. }) if action.is_completion()
                            ),
                            details: target_id.map(|_| dashboard::DetailsView {
                                rows: &details_rows,
                                top: &mut details_top,
                            }),
                        },
                        render::DashboardEditor {
                            layout: &layout,
                            cursor: draft.cursor(),
                            top: &mut top,
                            chrome: &chrome,
                            message_is_error: confirmation.is_none() && message_is_error,
                            follow_cursor: editor_follow_cursor,
                        },
                        terminal.color,
                    );
                })?;
        } else if let Some(ui) = &conflict_ui {
            let area = dashboard::popup_layout(
                ratatui::layout::Rect::new(0, 0, size.0, size.1),
                usize::MAX,
            )
            .content;
            let rows = ui.rows(usize::from(area.width), usize::from(area.height));
            let mut popup_terminal = Terminal::new(CrosstermBackend::new(io::stderr()))?;
            popup_terminal.clear()?;
            popup_terminal.draw(|frame| dashboard::popup(frame, &rows, terminal.color, true))?;
        } else {
            render::draw(
                &mut io::stderr(),
                &layout,
                draft.cursor(),
                &mut top,
                size,
                &chrome,
                terminal.color,
            )?;
        }
        let refresh_interval = Duration::from_millis(250);
        let mut refresh_deadline = Instant::now() + refresh_interval;
        let input = loop {
            if dashboard {
                let timeout = refresh_deadline.saturating_duration_since(Instant::now());
                if !event::poll(timeout)? || Instant::now() >= refresh_deadline {
                    let db = mode.db().expect("dashboard has database");
                    let version = match db.data_version() {
                        Ok(version) => version,
                        Err(_) if graph_ui.is_some() || graph_background.is_some() => break None,
                        Err(error) => return Err(error),
                    };
                    if Some(version) != list_version {
                        break None;
                    }
                    refresh_deadline = Instant::now() + refresh_interval;
                    continue;
                }
            }
            let input = event::read()?;
            if let Event::Mouse(mouse) = &input {
                let active = dashboard
                    && confirmation.is_none()
                    && action_ui.is_none()
                    && jump_ui.is_none()
                    && conflict_ui.is_none()
                    && view_ui.is_none()
                    && bulk_ui.is_none()
                    && graph_ui.is_none()
                    && match mouse.kind {
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                            dashboard::wheel_area(size, mouse.column, mouse.row, filter_visible)
                                .is_some()
                        }
                        MouseEventKind::Down(MouseButton::Left) => dashboard::click_target(
                            size,
                            mouse.column,
                            mouse.row,
                            dashboard::HitState {
                                rows: &rows,
                                filter_visible,
                                list_top,
                                editor_top: top,
                                layout: &layout,
                            },
                        )
                        .is_some(),
                        _ => false,
                    };
                if !active {
                    continue;
                }
            }
            break Some(input);
        };
        let Some(input) = input else {
            continue;
        };
        match input {
            Event::Paste(text) if confirmation.is_none() && conflict_ui.is_none() => {
                if view_ui.is_some() || graph_ui.is_some() {
                    continue;
                } else if let Some(ui) = bulk_ui.as_mut() {
                    ui.paste(&text);
                } else if let Some(ui) = jump_ui.as_mut() {
                    ui.paste(&text);
                } else if let Some(ActionUi::Input {
                    kind,
                    value,
                    error,
                    cursor,
                    ..
                }) = action_ui.as_mut()
                {
                    if matches!(kind, ActionInputKind::Tags) {
                        // Normalize CRLF separators; preserve other controls for validation.
                        cursor.insert(value, &text.replace("\r\n", "\n"));
                    } else {
                        value.extend(text.chars().filter_map(|ch| {
                            if ch.is_control() {
                                matches!(kind, ActionInputKind::ErrorReason).then_some(' ')
                            } else {
                                Some(ch)
                            }
                        }));
                    }
                    error.clear();
                } else if action_ui.is_some() {
                    // Menu never edits draft.
                } else if filter_focused {
                    filter_query.extend(text.chars().filter(|ch| !ch.is_control()));
                    list_top = 0;
                    list_follow_selected = true;
                } else {
                    editor_follow_cursor = true;
                    let result = paste(&mut draft, &text);
                    message_is_error = result.is_err();
                    message = result
                        .err()
                        .map_or_else(String::new, |error| format!("{error:#}"));
                }
            }
            Event::Mouse(mouse)
                if dashboard
                    && matches!(
                        mouse.kind,
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                    ) =>
            {
                let down = match mouse.kind {
                    MouseEventKind::ScrollDown => true,
                    MouseEventKind::ScrollUp => false,
                    _ => continue,
                };
                match dashboard::wheel_area(size, mouse.column, mouse.row, filter_visible) {
                    Some(dashboard::WheelArea::List(height)) => {
                        list_follow_selected = false;
                        list_top = panel::wheel_top(list_top, list_row_count, height, down);
                    }
                    Some(dashboard::WheelArea::Editor(height)) => {
                        editor_follow_cursor = false;
                        top = panel::wheel_top(top, layout.rows.len(), height, down);
                    }
                    Some(dashboard::WheelArea::Details(height)) => {
                        details_top =
                            panel::wheel_top(details_top, details_rows.len(), height, down);
                    }
                    None => (),
                }
            }
            Event::Mouse(mouse)
                if dashboard && mouse.kind == MouseEventKind::Down(MouseButton::Left) =>
            {
                match dashboard::click_target(
                    size,
                    mouse.column,
                    mouse.row,
                    dashboard::HitState {
                        rows: &rows,
                        filter_visible,
                        list_top,
                        editor_top: top,
                        layout: &layout,
                    },
                ) {
                    Some(dashboard::ClickTarget::ToggleCompleted) => {
                        show_completed = !show_completed;
                        list_top = 0;
                        list_follow_selected = true;
                    }
                    Some(dashboard::ClickTarget::Editor(cursor)) => {
                        filter_focused = false;
                        draft.set_cursor(cursor);
                        editor_follow_cursor = true;
                    }
                    Some(dashboard::ClickTarget::Task(id)) if target_id == Some(id) => {
                        filter_focused = false;
                        editor_follow_cursor = true;
                    }
                    Some(dashboard::ClickTarget::Task(id)) => {
                        let db = mode.db().expect("dashboard has database");
                        let target = db.task(id).and_then(|task| task_target(db, task.id));
                        match target {
                            Ok(target) => {
                                buffers.park(
                                    DraftKey::current(target_id, draft_parent_id),
                                    &mut draft,
                                    &baseline,
                                    top,
                                    editor_follow_cursor,
                                );
                                editor_follow_cursor = load_target(
                                    restore_target(target, &mut buffers),
                                    &mut draft,
                                    &mut target_id,
                                    &mut target_status,
                                    &mut draft_parent_id,
                                    &mut baseline,
                                    &mut top,
                                );
                                list_follow_selected = true;
                                filter_focused = false;
                                message.clear();
                                message_is_error = false;
                            }
                            Err(error) => {
                                message = format!("{error:#}");
                                message_is_error = true;
                            }
                        }
                    }
                    None => (),
                }
            }
            Event::Key(mut key) if key.kind != KeyEventKind::Release => {
                if let Some(ui) = &mut graph_ui {
                    let area = dashboard::popup_layout(
                        ratatui::layout::Rect::new(0, 0, size.0, size.1),
                        usize::MAX,
                    )
                    .content;
                    if ui.key(key, usize::from(area.height)) {
                        graph_ui = None;
                    }
                    continue;
                }
                if let Some(mut ui) = view_ui.take() {
                    match ui.key(key) {
                        Some(view_picker::Action::Apply(view)) => {
                            let current =
                                view_context.as_ref().expect("dashboard has view context");
                            let mut next = current.clone();
                            let result = next.select(view).and_then(|()| {
                                crate::selection::list(
                                    mode.db().expect("dashboard has database"),
                                    &next.prepared,
                                    Some(&filter_query),
                                )
                                .map(|_| ())
                            });
                            match result {
                                Ok(()) => {
                                    include_archived = next.prepared.include_archived;
                                    completed_limit = next.prepared.max_completed;
                                    show_completed = completed_limit != Some(0);
                                    view_context = Some(next);
                                    list_top = 0;
                                    list_follow_selected = true;
                                    message.clear();
                                    message_is_error = false;
                                }
                                Err(error) => {
                                    ui.set_error(format!("{error:#}"));
                                    view_ui = Some(ui);
                                }
                            }
                        }
                        Some(view_picker::Action::Cancel) => (),
                        None => view_ui = Some(ui),
                    }
                    continue;
                }
                if let Some(mut ui) = bulk_ui.take() {
                    match ui.key(key) {
                        bulk::Intent::None => bulk_ui = Some(ui),
                        bulk::Intent::Close => {
                            message = "Bulk preview cancelled".to_owned();
                            message_is_error = false;
                        }
                        bulk::Intent::Clear => {
                            bulk_selected.clear();
                            message = "Bulk selection cleared".to_owned();
                            message_is_error = false;
                        }
                        bulk::Intent::Preview(actions) => {
                            match crate::bulk::preview(
                                mode.db().expect("bulk has dashboard DB"),
                                crate::bulk::Selection::Ids(ui.ids()),
                                actions,
                            ) {
                                Ok(report) => ui.show_preview(report),
                                Err(error) => ui.show_error(format!("{error:#}")),
                            }
                            bulk_ui = Some(ui);
                        }
                        bulk::Intent::Apply(report) => match mode.bulk_apply(&report) {
                            Ok(applied) => {
                                bulk_selected.clear();
                                message = format!("Bulk applied: {} tasks", applied.changed_count);
                                message_is_error = false;
                            }
                            Err(error) => {
                                ui.show_error(format!("{error:#}"));
                                bulk_ui = Some(ui);
                            }
                        },
                    }
                    continue;
                }
                if let Some(mut ui) = jump_ui.take() {
                    match ui.key(key) {
                        Some(jump::Action::Go(id)) => match task_target_with_archived(
                            mode.db().expect("dashboard has database"),
                            id,
                        ) {
                            Ok((target, archived)) => {
                                buffers.park(
                                    DraftKey::current(target_id, draft_parent_id),
                                    &mut draft,
                                    &baseline,
                                    top,
                                    editor_follow_cursor,
                                );
                                editor_follow_cursor = load_target(
                                    restore_target(target, &mut buffers),
                                    &mut draft,
                                    &mut target_id,
                                    &mut target_status,
                                    &mut draft_parent_id,
                                    &mut baseline,
                                    &mut top,
                                );
                                include_archived |= archived;
                                show_completed |= target_status.as_deref() == Some("completed");
                                filter_query.clear();
                                filter_focused = false;
                                list_top = 0;
                                list_follow_selected = true;
                                message.clear();
                                message_is_error = false;
                            }
                            Err(error) => {
                                ui.set_error(format!("{error:#}"));
                                jump_ui = Some(ui);
                            }
                        },
                        Some(jump::Action::Cancel) => (),
                        None => jump_ui = Some(ui),
                    }
                    continue;
                }
                if let Some(mut ui) = conflict_ui.take() {
                    let area = dashboard::popup_layout(
                        ratatui::layout::Rect::new(0, 0, size.0, size.1),
                        usize::MAX,
                    )
                    .content;
                    match ui.key(key, usize::from(area.width), usize::from(area.height)) {
                        Some(conflict::Action::Keep) => {
                            message = "Local draft kept".to_owned();
                            message_is_error = false;
                            continue;
                        }
                        Some(conflict::Action::Reload) => {
                            match target_id.and_then(|id| mode.db().map(|db| task_target(db, id))) {
                                Some(Ok(target)) => {
                                    editor_follow_cursor = load_target(
                                        target,
                                        &mut draft,
                                        &mut target_id,
                                        &mut target_status,
                                        &mut draft_parent_id,
                                        &mut baseline,
                                        &mut top,
                                    );
                                    list_follow_selected = true;
                                    message = "Reloaded current DB text".to_owned();
                                    message_is_error = false;
                                }
                                Some(Err(error)) => {
                                    if target_id.is_some_and(|id| {
                                        mode.db().is_some_and(|db| {
                                            db.task_exists(id).is_ok_and(|exists| !exists)
                                        })
                                    }) {
                                        ui.mark_removed();
                                    }
                                    message = format!("{error:#}; local draft kept");
                                    message_is_error = true;
                                    conflict_ui = Some(ui);
                                }
                                None => conflict_ui = Some(ui),
                            }
                            continue;
                        }
                        Some(conflict::Action::Overwrite(revision)) => {
                            save_revision_override = Some(revision);
                            key = crossterm::event::KeyEvent::new(
                                KeyCode::Char('s'),
                                KeyModifiers::CONTROL,
                            );
                        }
                        None => {
                            conflict_ui = Some(ui);
                            continue;
                        }
                    }
                }
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                if dashboard
                    && control
                    && key.code == KeyCode::Char('o')
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                    && confirmation.is_none()
                    && action_ui.is_none()
                {
                    if let Some(id) = target_id {
                        graph_background = Some(DashboardSnapshot {
                            tasks: dashboard_tasks
                                .as_ref()
                                .expect("dashboard has tasks")
                                .clone(),
                            selected: selected_tasks
                                .as_ref()
                                .expect("dashboard has selection")
                                .clone(),
                            version: list_version.expect("dashboard has version"),
                            details: details_rows.clone(),
                            has_herdr_link,
                        });
                        graph_ui = Some(graph_inspector::Inspector::new(id));
                    } else {
                        message = "Select task to inspect dependency graph".into();
                        message_is_error = false;
                    }
                    continue;
                }
                if dashboard
                    && control
                    && key.code == KeyCode::Char('b')
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                    && confirmation.is_none()
                    && action_ui.is_none()
                {
                    let context = view_context.as_ref().expect("dashboard has view context");
                    match crate::views::load(&context.path) {
                        Ok(catalog) => {
                            view_ui = Some(view_picker::Picker::new(
                                catalog.views,
                                context.active.as_ref().map(|view| view.name.as_str()),
                            ))
                        }
                        Err(error) => {
                            message = format!("{error:#}");
                            message_is_error = true;
                        }
                    }
                    continue;
                }
                let cancel_key = control && key.code == KeyCode::Char('c');
                if matches!(
                    action_ui,
                    Some(ActionUi::Input {
                        kind: ActionInputKind::Tags,
                        ..
                    })
                ) && (cancel_key || key.code == KeyCode::Esc)
                {
                    action_ui = None;
                    continue;
                }
                if dashboard
                    && (cancel_key || key.code == KeyCode::Esc)
                    && (!filter_query.is_empty() || filter_focused)
                {
                    if key.code == KeyCode::Esc || filter_query.is_empty() {
                        filter_focused = false;
                    }
                    filter_query.clear();
                    list_top = 0;
                    list_follow_selected = true;
                    continue;
                }
                if dashboard
                    && cancel_key
                    && matches!(confirmation, Some(Confirmation::ExitBuffers { .. }))
                {
                    continue;
                }
                if dashboard
                    && target_id.is_none()
                    && cancel_key
                    && draft.is_dirty_against(&baseline.description)
                {
                    confirmation = Some(Confirmation::Exit);
                    action_ui = None;
                    filter_focused = false;
                    continue;
                }
                let editor_escape =
                    key.code == KeyCode::Esc && confirmation.is_none() && action_ui.is_none();
                if dashboard && (editor_escape || (target_id.is_some() && cancel_key)) {
                    if target_id.is_none() && draft.is_empty() && draft_parent_id.is_none() {
                        if let Some(pending) = confirm_buffers_exit(&buffers) {
                            confirmation = Some(pending);
                            action_ui = None;
                            continue;
                        }
                        return cancel(saved_any, dashboard);
                    }
                    if draft.is_dirty_against(&baseline.description) {
                        confirmation = Some(Confirmation::Switch {
                            target: Target::New { parent_id: None },
                            focus_editor: true,
                        });
                        action_ui = None;
                        filter_focused = false;
                    } else {
                        editor_follow_cursor = load_target(
                            restore_target(Target::New { parent_id: None }, &mut buffers),
                            &mut draft,
                            &mut target_id,
                            &mut target_status,
                            &mut draft_parent_id,
                            &mut baseline,
                            &mut top,
                        );
                        confirmation = None;
                        action_ui = None;
                        filter_focused = false;
                        list_follow_selected = true;
                        message.clear();
                        message_is_error = false;
                    }
                    continue;
                }
                if let Some(pending) = confirmation.take() {
                    if control && key.code == KeyCode::Char('c') {
                        if dashboard {
                            confirmation = Some(pending);
                            continue;
                        }
                        return cancel(saved_any, dashboard);
                    }
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    {
                        match key.code {
                            KeyCode::Char('y' | 'Y') => match pending {
                                Confirmation::Exit => {
                                    if let Some(pending) = confirm_buffers_exit(&buffers) {
                                        confirmation = Some(pending);
                                    } else {
                                        return cancel(saved_any, dashboard);
                                    }
                                }
                                Confirmation::ExitBuffers { keys, index } => {
                                    if index + 1 == keys.len() {
                                        return cancel(saved_any, dashboard);
                                    }
                                    confirmation = Some(Confirmation::ExitBuffers {
                                        keys,
                                        index: index + 1,
                                    });
                                }
                                Confirmation::Switch {
                                    target,
                                    focus_editor,
                                } => {
                                    editor_follow_cursor = load_target(
                                        restore_target(target, &mut buffers),
                                        &mut draft,
                                        &mut target_id,
                                        &mut target_status,
                                        &mut draft_parent_id,
                                        &mut baseline,
                                        &mut top,
                                    );
                                    list_follow_selected = true;
                                    if focus_editor {
                                        filter_focused = false;
                                    }
                                    message.clear();
                                    message_is_error = false;
                                }
                                Confirmation::Action { action, dirty, .. } => {
                                    let selected = if action.is_completion() {
                                        if key.code == KeyCode::Char('Y') {
                                            TaskAction::ForceComplete(action.id())
                                        } else {
                                            TaskAction::Complete(action.id())
                                        }
                                    } else {
                                        action.clone()
                                    };
                                    match run_action(&mut mode, selected.clone(), include_archived)
                                    {
                                        Ok(target) => {
                                            editor_follow_cursor = load_target(
                                                restore_target(target, &mut buffers),
                                                &mut draft,
                                                &mut target_id,
                                                &mut target_status,
                                                &mut draft_parent_id,
                                                &mut baseline,
                                                &mut top,
                                            );
                                            list_follow_selected = true;
                                            message = selected.success();
                                            message_is_error = false;
                                        }
                                        Err(error) => {
                                            message = format!("{error:#}");
                                            message_is_error = true;
                                            if matches!(selected, TaskAction::Complete(_))
                                                && completion::can_force_after_error(&error)
                                            {
                                                confirmation = Some(Confirmation::Action {
                                                    action,
                                                    dirty,
                                                    error: message.clone(),
                                                });
                                            } else {
                                                action_ui = Some(ActionUi::Error {
                                                    text: message.clone(),
                                                    top: 0,
                                                });
                                            }
                                        }
                                    }
                                }
                            },
                            KeyCode::Char('n' | 'N') | KeyCode::Enter | KeyCode::Esc => {
                                if let Confirmation::ExitBuffers { keys, index } = pending {
                                    let key = keys[index];
                                    let removed = matches!(key, DraftKey::Task(id)
                                        if mode.db().is_some_and(|db|
                                            db.task_exists(id).is_ok_and(|exists| !exists)));
                                    if removed {
                                        buffers.park(
                                            DraftKey::current(target_id, draft_parent_id),
                                            &mut draft,
                                            &baseline,
                                            top,
                                            editor_follow_cursor,
                                        );
                                        if let Some(saved) = buffers.take(key) {
                                            editor_follow_cursor = load_target(
                                                Target::Retained {
                                                    key,
                                                    status: None,
                                                    saved,
                                                },
                                                &mut draft,
                                                &mut target_id,
                                                &mut target_status,
                                                &mut draft_parent_id,
                                                &mut baseline,
                                                &mut top,
                                            );
                                            list_follow_selected = true;
                                        }
                                    }
                                }
                                message.clear();
                                message_is_error = false;
                            }
                            _ => confirmation = Some(pending),
                        }
                    } else {
                        confirmation = Some(pending);
                    }
                    continue;
                }
                if dashboard && control && key.code == KeyCode::Char('h') {
                    let focus = target_id
                        .ok_or_else(|| anyhow::anyhow!("Select task to open its Herdr agent"))
                        .and_then(|id| {
                            handoff::focus(mode.db().expect("dashboard has database"), id)
                        });
                    let result = match focus {
                        Ok(server) => {
                            drop(terminal);
                            let result = handoff::attach(server.as_deref());
                            terminal = TerminalGuard::enter(dashboard)?;
                            dashboard_terminal
                                .as_mut()
                                .expect("dashboard has terminal")
                                .clear()?;
                            result
                        }
                        Err(error) => Err(error),
                    };
                    message_is_error = result.is_err();
                    message = result.err().map_or_else(
                        || "Returned from Herdr".to_owned(),
                        |error| format!("{error:#}"),
                    );
                    continue;
                }
                if let Some(mut ui) = action_ui.take() {
                    if let ActionUi::Input {
                        kind: ActionInputKind::Tags,
                        value,
                        error,
                        cursor,
                        ..
                    } = &mut ui
                    {
                        if cursor.edit(value, error, key) {
                            action_ui = Some(ui);
                            continue;
                        }
                        if control
                            && key.code == KeyCode::Char('s')
                            && !key
                                .modifiers
                                .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                        {
                            key =
                                crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
                        }
                    }
                    if control && key.code == KeyCode::Char('c') {
                        if let Some(pending) = confirm_buffers_exit(&buffers) {
                            confirmation = Some(pending);
                            continue;
                        }
                        return cancel(saved_any, dashboard);
                    }
                    if key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    {
                        action_ui = Some(ui);
                        continue;
                    }
                    match ui {
                        ActionUi::Menu {
                            id,
                            archived,
                            can_retry,
                            mut selected,
                        } => {
                            let count = action_menu_items(can_retry).count();
                            if matches!(key.code, KeyCode::Up | KeyCode::Down) {
                                selected = if key.code == KeyCode::Up {
                                    (selected + count - 1) % count
                                } else {
                                    (selected + 1) % count
                                };
                                action_ui = Some(ActionUi::Menu {
                                    id,
                                    archived,
                                    can_retry,
                                    selected,
                                });
                                continue;
                            }
                            let activation = if key.code == KeyCode::Enter {
                                KeyCode::Char(
                                    action_menu_items(can_retry)
                                        .nth(selected)
                                        .expect("menu selection is valid")
                                        .0,
                                )
                            } else {
                                key.code
                            };
                            let chosen = match activation {
                                KeyCode::Char('c') => match mode.completion_action(id) {
                                    Ok(action) => Some(action),
                                    Err(error) => {
                                        action_ui = Some(ActionUi::Error {
                                            text: format!("{error:#}"),
                                            top: 0,
                                        });
                                        None
                                    }
                                },
                                KeyCode::Char('r') if can_retry => Some(TaskAction::Retry(id)),
                                KeyCode::Char('o') => Some(TaskAction::Reopen(id)),
                                KeyCode::Char('a') => Some(TaskAction::SetArchived(id, !archived)),
                                KeyCode::Char('p') => {
                                    action_ui = Some(ActionUi::Input {
                                        id: Some(id),
                                        kind: ActionInputKind::Priority,
                                        value: String::new(),
                                        error: String::new(),
                                        cursor: tag_input::Cursor::at_end(""),
                                    });
                                    None
                                }
                                KeyCode::Char('d') => {
                                    action_ui = Some(ActionUi::Input {
                                        id: Some(id),
                                        kind: ActionInputKind::Parent,
                                        value: String::new(),
                                        error: String::new(),
                                        cursor: tag_input::Cursor::at_end(""),
                                    });
                                    None
                                }
                                KeyCode::Char('e') => {
                                    action_ui = Some(ActionUi::Input {
                                        id: Some(id),
                                        kind: ActionInputKind::ErrorReason,
                                        value: String::new(),
                                        error: String::new(),
                                        cursor: tag_input::Cursor::at_end(""),
                                    });
                                    None
                                }
                                KeyCode::Esc => None,
                                _ => {
                                    action_ui = Some(ActionUi::Menu {
                                        id,
                                        archived,
                                        can_retry,
                                        selected,
                                    });
                                    None
                                }
                            };
                            if let Some(action) = chosen {
                                confirmation = Some(action_confirmation(
                                    action,
                                    draft.is_dirty_against(&baseline.description),
                                ));
                            }
                        }
                        ActionUi::Input {
                            id,
                            kind,
                            mut value,
                            mut error,
                            cursor,
                        } => match key.code {
                            KeyCode::Esc => (),
                            KeyCode::Backspace => {
                                if let Some((index, _)) = value.grapheme_indices(true).next_back() {
                                    value.truncate(index);
                                }
                                error.clear();
                                action_ui = Some(ActionUi::Input {
                                    id,
                                    kind,
                                    value,
                                    error,
                                    cursor,
                                });
                            }
                            KeyCode::Char(ch) => {
                                if !ch.is_control() {
                                    value.push(ch);
                                    error.clear();
                                }
                                action_ui = Some(ActionUi::Input {
                                    id,
                                    kind,
                                    value,
                                    error,
                                    cursor,
                                });
                            }
                            KeyCode::Enter
                                if id.is_none() && matches!(kind, ActionInputKind::Tags) =>
                            {
                                match crate::tags::parse_lines(&value) {
                                    Ok(tags) => {
                                        draft.tags = tags;
                                        message =
                                            "Draft tags saved; Ctrl-S creates task".to_owned();
                                        message_is_error = false;
                                    }
                                    Err(failure) => {
                                        error = failure.to_string();
                                        action_ui = Some(ActionUi::Input {
                                            id,
                                            kind,
                                            value,
                                            error,
                                            cursor,
                                        });
                                    }
                                }
                            }
                            KeyCode::Enter => match parse_action_input(
                                id.expect("task input has selected task"),
                                kind,
                                &value,
                            ) {
                                Ok(action @ TaskAction::Tags(..)) => {
                                    match mode.task_action(action.clone()) {
                                        Ok(_) => {
                                            message = action.success();
                                            message_is_error = false;
                                        }
                                        Err(failure) => {
                                            error = format!("{failure:#}");
                                            action_ui = Some(ActionUi::Input {
                                                id,
                                                kind,
                                                value,
                                                error,
                                                cursor,
                                            });
                                        }
                                    }
                                }
                                Ok(action)
                                    if matches!(action, TaskAction::MarkError(..))
                                        || draft.is_dirty_against(&baseline.description) =>
                                {
                                    confirmation = Some(action_confirmation(
                                        action,
                                        draft.is_dirty_against(&baseline.description),
                                    ));
                                }
                                Ok(action) => {
                                    match run_action(&mut mode, action.clone(), include_archived) {
                                        Ok(target) => {
                                            editor_follow_cursor = load_target(
                                                restore_target(target, &mut buffers),
                                                &mut draft,
                                                &mut target_id,
                                                &mut target_status,
                                                &mut draft_parent_id,
                                                &mut baseline,
                                                &mut top,
                                            );
                                            list_follow_selected = true;
                                            message = action.success();
                                            message_is_error = false;
                                        }
                                        Err(failure) => {
                                            message = format!("{failure:#}");
                                            message_is_error = true;
                                            action_ui = Some(ActionUi::Error {
                                                text: message.clone(),
                                                top: 0,
                                            });
                                        }
                                    }
                                }
                                Err(problem) => {
                                    error = problem;
                                    action_ui = Some(ActionUi::Input {
                                        id,
                                        kind,
                                        value,
                                        error,
                                        cursor,
                                    });
                                }
                            },
                            _ => {
                                action_ui = Some(ActionUi::Input {
                                    id,
                                    kind,
                                    value,
                                    error,
                                    cursor,
                                })
                            }
                        },
                        ActionUi::Error { text, mut top } => match key.code {
                            KeyCode::Esc | KeyCode::Enter => (),
                            KeyCode::Up | KeyCode::Char('k') => {
                                top = top.saturating_sub(1);
                                action_ui = Some(ActionUi::Error { text, top });
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                top = top.saturating_add(1);
                                action_ui = Some(ActionUi::Error { text, top });
                            }
                            _ => action_ui = Some(ActionUi::Error { text, top }),
                        },
                    }
                    continue;
                }
                if dashboard
                    && control
                    && key.code == KeyCode::Char('l')
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                {
                    match target_id.map(|id| mode.db().expect("dashboard has database").task(id)) {
                        Some(Ok(task)) => {
                            let value = task.tags.join("\n");
                            action_ui = Some(ActionUi::Input {
                                id: Some(task.id),
                                kind: ActionInputKind::Tags,
                                cursor: tag_input::Cursor::at_end(&value),
                                value,
                                error: String::new(),
                            });
                        }
                        Some(Err(error)) => {
                            action_ui = Some(ActionUi::Error {
                                text: format!("{error:#}"),
                                top: 0,
                            });
                        }
                        None => {
                            let value = draft.tags.join("\n");
                            action_ui = Some(ActionUi::Input {
                                id: None,
                                kind: ActionInputKind::Tags,
                                cursor: tag_input::Cursor::at_end(&value),
                                value,
                                error: String::new(),
                            });
                        }
                    }
                    continue;
                }
                if dashboard
                    && control
                    && key.code == KeyCode::Char('k')
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                {
                    jump_ui = Some(jump::View::default());
                    continue;
                }
                if dashboard
                    && control
                    && key.code == KeyCode::Char('p')
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                {
                    if let Some(parent) = target_id {
                        let target = Target::New {
                            parent_id: Some(parent),
                        };
                        buffers.park(
                            DraftKey::current(target_id, draft_parent_id),
                            &mut draft,
                            &baseline,
                            top,
                            editor_follow_cursor,
                        );
                        editor_follow_cursor = load_target(
                            restore_target(target, &mut buffers),
                            &mut draft,
                            &mut target_id,
                            &mut target_status,
                            &mut draft_parent_id,
                            &mut baseline,
                            &mut top,
                        );
                        filter_focused = false;
                        list_follow_selected = true;
                        message.clear();
                        message_is_error = false;
                    } else {
                        message = "Select parent task first".to_owned();
                        message_is_error = true;
                    }
                    continue;
                }
                if dashboard
                    && control
                    && key.code == KeyCode::Char('d')
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                {
                    if let Some(id) = target_id {
                        if !bulk_selected.insert(id) {
                            bulk_selected.remove(&id);
                        }
                        message = format!("Bulk selected: {}", bulk_selected.len());
                        message_is_error = false;
                    } else {
                        message = "Select saved task before Ctrl-D".to_owned();
                        message_is_error = true;
                    }
                    continue;
                }
                if dashboard && control && key.code == KeyCode::Char('g') {
                    if !bulk_selected.is_empty() {
                        bulk_ui = Some(bulk::View::menu(bulk_selected.iter().copied().collect()));
                        continue;
                    }
                    match target_id {
                        Some(id) => match mode.db().expect("dashboard has database").task(id) {
                            Ok(task) => {
                                action_ui = Some(ActionUi::Menu {
                                    id,
                                    archived: task.archived,
                                    can_retry: task.status == "error",
                                    selected: 0,
                                });
                                message.clear();
                                message_is_error = false;
                            }
                            Err(error) => {
                                message = format!("{error:#}");
                                message_is_error = true;
                            }
                        },
                        None => {
                            message = "Select task for actions".to_owned();
                            message_is_error = true;
                        }
                    }
                    continue;
                }
                if key.modifiers.contains(KeyModifiers::SHIFT)
                    && !matches!(mode, Mode::Edit { .. })
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    && matches!(key.code, KeyCode::Up | KeyCode::Down)
                {
                    if let Some(db) = mode.db() {
                        let older = key.code == KeyCode::Up;
                        match adjacent_target(
                            db,
                            target_id,
                            older,
                            include_archived,
                            visible_ids.as_deref(),
                        ) {
                            Ok(Some(target))
                                if !dashboard && draft.is_dirty_against(&baseline.description) =>
                            {
                                confirmation = Some(Confirmation::Switch {
                                    target,
                                    focus_editor: false,
                                });
                            }
                            Ok(Some(target)) => {
                                if dashboard {
                                    buffers.park(
                                        DraftKey::current(target_id, draft_parent_id),
                                        &mut draft,
                                        &baseline,
                                        top,
                                        editor_follow_cursor,
                                    );
                                }
                                editor_follow_cursor = load_target(
                                    restore_target(target, &mut buffers),
                                    &mut draft,
                                    &mut target_id,
                                    &mut target_status,
                                    &mut draft_parent_id,
                                    &mut baseline,
                                    &mut top,
                                );
                                list_follow_selected = true;
                                message.clear();
                                message_is_error = false;
                            }
                            Ok(None) => {
                                list_follow_selected = true;
                                message_is_error = false;
                                message = if older {
                                    "No older task".to_owned()
                                } else {
                                    "Already at new task".to_owned()
                                };
                            }
                            Err(error) => {
                                message_is_error = true;
                                message = format!("{error:#}");
                            }
                        }
                        continue;
                    }
                }
                // Legacy Ctrl+/ sends 0x1f, which Crossterm decodes as Ctrl+7.
                // Extended keyboard protocols can report Ctrl+/ or Ctrl+_.
                if dashboard
                    && !filter_focused
                    && control
                    && matches!(key.code, KeyCode::Char('/' | '_' | '7'))
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                {
                    filter_focused = true;
                    continue;
                }
                if filter_focused {
                    if control && key.code == KeyCode::Char('t') {
                        show_completed = !show_completed;
                        list_top = 0;
                        list_follow_selected = true;
                    } else if control && key.code == KeyCode::Char('u') {
                        filter_query.clear();
                        list_top = 0;
                        list_follow_selected = true;
                    } else if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    {
                        match key.code {
                            KeyCode::Tab | KeyCode::Enter => filter_focused = false,
                            KeyCode::Backspace => {
                                if let Some((index, _)) =
                                    filter_query.grapheme_indices(true).next_back()
                                {
                                    filter_query.truncate(index);
                                    list_top = 0;
                                    list_follow_selected = true;
                                }
                            }
                            KeyCode::Char(character) => {
                                filter_query.push(character);
                                list_top = 0;
                                list_follow_selected = true;
                            }
                            _ => (),
                        }
                    }
                    continue;
                }
                if dashboard
                    && target_id.is_some()
                    && key.modifiers.is_empty()
                    && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
                {
                    let area =
                        dashboard::panes(ratatui::layout::Rect::new(0, 0, size.0, size.1)).details;
                    if area.height == 0 {
                        continue;
                    }
                    let page = dashboard::details_height(area).max(1);
                    details_top = if key.code == KeyCode::PageDown {
                        details_top
                            .saturating_add(page)
                            .min(details_rows.len().saturating_sub(page))
                    } else {
                        details_top.saturating_sub(page)
                    };
                    continue;
                }
                if control {
                    match key.code {
                        KeyCode::Char('s') => match draft.finish() {
                            Ok(composition) => {
                                let keep_saved = dashboard
                                    && (target_id.is_some()
                                        || after_save_new == AfterSaveNew::OpenSaved);
                                let outcome = Outcome {
                                    composition,
                                    tags: draft.tags.clone(),
                                    target_id,
                                    parent_id: draft_parent_id,
                                    expected_revision: save_revision_override
                                        .take()
                                        .or(baseline.revision),
                                };
                                let finish_after_save = matches!(mode, Mode::Edit { .. });
                                match &mut mode {
                                    Mode::Single(_) => return Ok(Some(outcome)),
                                    Mode::Continuous { db, save, .. } | Mode::Edit { db, save } => {
                                        match save(db, outcome).and_then(|id| {
                                            let target = if keep_saved {
                                                task_target(db, id)?
                                            } else {
                                                Target::New { parent_id: None }
                                            };
                                            Ok((id, target))
                                        }) {
                                            Ok((id, target)) => {
                                                if finish_after_save {
                                                    return Ok(None);
                                                }
                                                saved_any = true;
                                                editor_follow_cursor = load_target(
                                                    restore_target(target, &mut buffers),
                                                    &mut draft,
                                                    &mut target_id,
                                                    &mut target_status,
                                                    &mut draft_parent_id,
                                                    &mut baseline,
                                                    &mut top,
                                                );
                                                list_follow_selected = true;
                                                message = if keep_saved {
                                                    format!("Saved #{id}")
                                                } else {
                                                    format!("Saved #{id}. New task")
                                                };
                                                message_is_error = false;
                                            }
                                            Err(error) => {
                                                if let Some(content_conflict) =
                                                    error
                                                        .downcast_ref::<crate::db::ContentConflict>(
                                                        )
                                                {
                                                    conflict_ui = Some(conflict::View::new(
                                                        content_conflict,
                                                        draft.finish()?.description,
                                                    ));
                                                    filter_focused = false;
                                                }
                                                message = format!("{error:#}");
                                                message_is_error = true;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(error) => {
                                message = error.to_string();
                                message_is_error = true;
                            }
                        },
                        KeyCode::Char('c') => {
                            if let Some(pending) = confirm_buffers_exit(&buffers) {
                                confirmation = Some(pending);
                                action_ui = None;
                            } else {
                                return cancel(saved_any, dashboard);
                            }
                        }
                        KeyCode::Char('v') => {
                            editor_follow_cursor = true;
                            let result = clipboard::read().and_then(|value| match value {
                                clipboard::Paste::Text(text) => paste(&mut draft, &text),
                                clipboard::Paste::Image(image) => draft.image(image),
                            });
                            message_is_error = result.is_err();
                            message = result
                                .err()
                                .map_or_else(String::new, |error| format!("{error:#}"));
                        }
                        KeyCode::Char('a') => {
                            editor_follow_cursor = true;
                            draft.home();
                        }
                        KeyCode::Char('e') => {
                            editor_follow_cursor = true;
                            draft.end();
                        }
                        KeyCode::Char('w') => {
                            editor_follow_cursor = true;
                            draft.delete_previous_word();
                        }
                        _ => (),
                    }
                    continue;
                }
                message.clear();
                message_is_error = false;
                if matches!(
                    key.code,
                    KeyCode::Enter
                        | KeyCode::Tab
                        | KeyCode::Backspace
                        | KeyCode::Delete
                        | KeyCode::Left
                        | KeyCode::Right
                        | KeyCode::Home
                        | KeyCode::End
                        | KeyCode::Up
                        | KeyCode::Down
                ) || matches!(key.code, KeyCode::Char(character)
                    if !key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                        || (character == '/'
                            && key.modifiers.contains(KeyModifiers::ALT)
                            && !key.modifiers.contains(KeyModifiers::SUPER)))
                {
                    editor_follow_cursor = true;
                }
                match key.code {
                    KeyCode::Esc if draft.is_empty() => return cancel(saved_any, dashboard),
                    KeyCode::Esc => confirmation = Some(Confirmation::Exit),
                    KeyCode::Char(character)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::ALT | KeyModifiers::SUPER)
                            || (character == '/'
                                && key.modifiers.contains(KeyModifiers::ALT)
                                && !key.modifiers.contains(KeyModifiers::SUPER)) =>
                    {
                        draft.insert(&character.to_string())
                    }
                    KeyCode::Enter => draft.insert("\n"),
                    KeyCode::Tab => draft.insert("\t"),
                    KeyCode::Backspace => draft.backspace(),
                    KeyCode::Delete => draft.delete(),
                    KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                        draft.previous_word()
                    }
                    KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                        draft.next_word()
                    }
                    KeyCode::Left => draft.left(),
                    KeyCode::Right => draft.right(),
                    KeyCode::Home => draft.home(),
                    KeyCode::End => draft.end(),
                    KeyCode::Up | KeyCode::Down => {
                        let (row, column) = layout.positions[draft.cursor()];
                        let target = if key.code == KeyCode::Up {
                            row.saturating_sub(1)
                        } else {
                            row + 1
                        };
                        let cursor = layout.nearest(target, column);
                        if cursor != draft.cursor() {
                            draft.set_cursor(cursor);
                        } else if key.code == KeyCode::Down {
                            draft.right();
                        } else {
                            draft.left();
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tag_input;
    use crate::output::{Format, render};
    use serde_json::json;

    #[test]
    fn multiline_tag_popup_keeps_separate_rows_and_empty_final_caret() {
        use super::{ActionInputKind, ActionUi, action_lines, dashboard, render::PopupKind};
        let ui = ActionUi::Input {
            id: Some(1),
            kind: ActionInputKind::Tags,
            value: "frontend\n界 面\n".into(),
            error: String::new(),
            cursor: tag_input::Cursor::at_end("frontend\n界 面\n"),
        };
        let rows = action_lines(&ui, 46, 18);
        assert_eq!(
            rows.iter()
                .filter(|row| row.kind == PopupKind::Input)
                .map(|row| row.text.as_str())
                .collect::<Vec<_>>(),
            ["> frontend", "> 界 面", "> "]
        );
        assert_eq!(
            rows.last().unwrap().text,
            "Enter line Ctrl-S apply Ctrl-U delete line Esc"
        );
        for color in [true, false] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(72, 24)).unwrap();
            terminal
                .draw(|frame| dashboard::popup(frame, &rows, color, true))
                .unwrap();
            let content =
                dashboard::popup_layout(ratatui::layout::Rect::new(0, 0, 72, 24), rows.len())
                    .content;
            let last_input = rows
                .iter()
                .rposition(|row| row.kind == PopupKind::Input)
                .unwrap();
            assert_eq!(
                terminal.get_cursor_position().unwrap(),
                ratatui::layout::Position::new(content.x + 2, content.y + last_input as u16)
            );
        }
    }

    #[test]
    fn multiline_tag_popup_keeps_tail_input_errors_and_footer_in_short_terminals() {
        use super::{ActionInputKind, ActionUi, action_lines, render::PopupKind};
        let value = (0..30)
            .map(|id| format!("tag{id:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        for error in [
            "",
            "Tags must be nonempty labels without controls, commas or square brackets",
        ] {
            let ui = ActionUi::Input {
                id: Some(1),
                kind: ActionInputKind::Tags,
                value: value.clone(),
                error: error.into(),
                cursor: tag_input::Cursor::at_end(&value),
            };
            for width in [12, 18, 28, 44, 45, 46] {
                for height in 1..=18 {
                    let rows = action_lines(&ui, width, height);
                    assert!(rows.len() <= height);
                    assert_eq!(
                        rows.iter()
                            .rfind(|row| row.kind == PopupKind::Input)
                            .unwrap()
                            .text,
                        "> tag29",
                        "width={width} height={height}"
                    );
                    if height >= 2 {
                        let footer = rows.last().unwrap();
                        assert_eq!(footer.kind, PopupKind::Hint);
                        assert!(
                            unicode_width::UnicodeWidthStr::width(footer.text.as_str()) <= width
                        );
                        assert!(footer.text.contains("Esc"));
                    }
                    if !error.is_empty() && height >= 4 {
                        assert!(rows.iter().any(|row| row.kind == PopupKind::Error));
                    }
                }
            }
        }
    }

    #[test]
    fn grouped_action_menu_labels_separators_and_selection() {
        use super::{ActionUi, action_lines, action_menu_items, render::PopupKind};
        for can_retry in [false, true] {
            for archived in [false, true] {
                let mut expected = vec!["Task actions #1", "c Complete", "e Mark error"];
                if can_retry {
                    expected.push("r Retry error");
                }
                expected.extend([
                    "o Reopen",
                    "",
                    "p Priority",
                    "d Set parent",
                    "",
                    if archived { "a Unarchive" } else { "a Archive" },
                    "Up/Down Select  Enter Apply  Esc Cancel",
                ]);
                for selected in 0..action_menu_items(can_retry).count() {
                    let rows = action_lines(
                        &ActionUi::Menu {
                            id: 1,
                            archived,
                            can_retry,
                            selected,
                        },
                        46,
                        20,
                    );
                    assert_eq!(
                        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
                        expected
                    );
                    let selected_rows = rows
                        .iter()
                        .filter(|row| row.kind == PopupKind::SelectedAction)
                        .collect::<Vec<_>>();
                    assert_eq!(selected_rows.len(), 1);
                    assert_eq!(
                        selected_rows[0].text.chars().next(),
                        Some(action_menu_items(can_retry).nth(selected).unwrap().0)
                    );
                    for row in rows.iter().filter(|row| row.text.is_empty()) {
                        assert_eq!(row.kind, PopupKind::Body);
                    }
                }
            }
        }
    }

    #[test]
    fn grouped_action_menu_keeps_selected_action_visible_at_small_heights() {
        use super::{ActionUi, action_lines, action_menu_items, render::PopupKind};
        for can_retry in [false, true] {
            for height in 1..=12 {
                for selected in 0..action_menu_items(can_retry).count() {
                    let rows = action_lines(
                        &ActionUi::Menu {
                            id: 1,
                            archived: false,
                            can_retry,
                            selected,
                        },
                        12,
                        height,
                    );
                    assert!(
                        rows.len() <= height,
                        "height {height}, selection {selected}"
                    );
                    let selected_rows = rows
                        .iter()
                        .filter(|row| row.kind == PopupKind::SelectedAction)
                        .collect::<Vec<_>>();
                    assert_eq!(selected_rows.len(), 1);
                    assert_eq!(
                        selected_rows[0].text.chars().next(),
                        Some(action_menu_items(can_retry).nth(selected).unwrap().0)
                    );
                    if height >= 2 {
                        assert_eq!(rows.last().unwrap().kind, PopupKind::Hint);
                    }
                    if height >= 3 {
                        assert_eq!(rows.first().unwrap().kind, PopupKind::Heading);
                    }
                }
            }
        }
    }

    #[test]
    fn tui_preview_cap_handles_wrapped_text_and_narrow_tree_prefixes() {
        let tasks = json!([
            {"id":1,"description":"ABCDEFGHIJKLMNOPQRSTUVWXYZ".repeat(4),"status":"new","parent_id":null},
            {"id":2,"description":"Child first\nChild second\nChild third\nHidden","status":"new","parent_id":1}
        ]);
        for width in [24, 20, 10] {
            for format in [Format::Tasks, Format::CompactTasks] {
                let compact = matches!(format, Format::CompactTasks);
                let tree = render(format, &tasks, false, Some(width));
                if compact {
                    assert!(!tree.contains("New"));
                    assert!(tree.contains("└──"));
                }
                let rows = super::panel::rows(&tree, width);
                assert_eq!(rows.len(), 6);
                assert_eq!(rows[0].task_id, Some(1));
                for id in [1, 2] {
                    let preview: Vec<_> =
                        rows.iter().filter(|row| row.task_id == Some(id)).collect();
                    assert_eq!(preview.len(), 3, "task {id}, width {width}");
                    assert!(
                        preview[2].text.ends_with("..."),
                        "task {id}, width {width}: {}",
                        preview[2].text
                    );
                    assert!(
                        unicode_width::UnicodeWidthStr::width(preview[2].text.as_str()) <= width
                    );
                }
            }
        }
    }
}
