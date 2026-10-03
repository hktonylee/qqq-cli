mod buffers;
mod clipboard;
mod conflict;
mod dashboard;
mod details;
pub mod draft;
mod handoff;
mod panel;
mod render;

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
    ExitBuffers { keys: Vec<DraftKey>, index: usize },
    Switch { target: Target, focus_editor: bool },
    Action { action: TaskAction, dirty: bool },
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

#[derive(Clone, Copy)]
pub enum TaskAction {
    Complete(i64),
    Retry(i64),
    Reopen(i64),
    SetArchived(i64, bool),
    Priority(i64, i64),
    Parent(i64, crate::db::ParentChange),
}

impl TaskAction {
    fn id(self) -> i64 {
        match self {
            Self::Complete(id)
            | Self::Retry(id)
            | Self::Reopen(id)
            | Self::SetArchived(id, _)
            | Self::Priority(id, _)
            | Self::Parent(id, _) => id,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Complete(_) => "Complete",
            Self::Retry(_) => "Retry",
            Self::Reopen(_) => "Reopen",
            Self::SetArchived(_, true) => "Archive",
            Self::SetArchived(_, false) => "Unarchive",
            Self::Priority(_, _) => "Set priority",
            Self::Parent(_, _) => "Set parent",
        }
    }

    fn success(self) -> String {
        match self {
            Self::Complete(id) => format!("Completed #{id}"),
            Self::Retry(id) => format!("Retried #{id}"),
            Self::Reopen(id) => format!("Reopened #{id}"),
            Self::SetArchived(id, true) => format!("Archived #{id}"),
            Self::SetArchived(id, false) => format!("Unarchived #{id}"),
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
}

const ACTION_MENU_ITEMS: [(char, &str); 6] = [
    ('c', "Complete"),
    ('r', "Retry error"),
    ('o', "Reopen"),
    ('a', "Archive"),
    ('p', "Priority"),
    ('d', "Parent"),
];

fn action_menu_items(can_retry: bool) -> impl Iterator<Item = (char, &'static str)> {
    ACTION_MENU_ITEMS
        .into_iter()
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
        id: i64,
        kind: ActionInputKind,
        value: String,
        error: String,
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
            lines.extend(action_menu_items(*can_retry).map(|(key, label)| {
                let label = if key == 'a' && *archived {
                    "Unarchive"
                } else {
                    label
                };
                PopupRow::new(format!("{key} {label}"), PopupKind::Action)
            }));
            lines.push(PopupRow::new(
                if width >= "Up/Down Select  Enter Apply  Esc Cancel".len() {
                    "Up/Down Select  Enter Apply  Esc Cancel"
                } else {
                    "Up/Dn Enter"
                },
                PopupKind::Hint,
            ));
            lines[1 + selected].kind = PopupKind::SelectedAction;
            lines
        }
        ActionUi::Input {
            id,
            kind,
            value,
            error,
        } => {
            let mut lines = match kind {
                ActionInputKind::Priority => {
                    vec![
                        PopupRow::new(format!("Priority task #{id}"), PopupKind::Heading),
                        PopupRow::new("Enter -100..100", PopupKind::Hint),
                    ]
                }
                ActionInputKind::Parent => {
                    vec![
                        PopupRow::new(format!("Parent task #{id}"), PopupKind::Heading),
                        PopupRow::new("ID / none", PopupKind::Hint),
                    ]
                }
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
    }
}

fn task_target(db: &crate::db::Db, id: i64) -> Result<Target> {
    let snapshot = db.content_snapshot(id)?;
    let draft = Draft::from_saved(&snapshot.task.description, id, &snapshot.references)?;
    Ok(Target::Task {
        id,
        description: snapshot.task.description,
        status: snapshot.task.status,
        revision: snapshot.task.content_revision,
        draft,
    })
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
        dashboard: bool,
        include_archived: bool,
        after_save_new: AfterSaveNew,
    },
}
type ActionHandler<'a> = dyn FnMut(&mut crate::db::Db, TaskAction) -> Result<crate::db::Task> + 'a;
impl Mode<'_, '_> {
    fn db(&self) -> Option<&crate::db::Db> {
        match self {
            Self::Single(db) => *db,
            Self::Continuous { db, .. } | Self::Edit { db, .. } => Some(db),
        }
    }
}
fn run_action(
    mode: &mut Mode<'_, '_>,
    action: TaskAction,
    include_archived: bool,
) -> Result<Target> {
    let Mode::Continuous {
        db,
        action: Some(handler),
        ..
    } = mode
    else {
        bail!("Task actions require dashboard");
    };
    let task = handler(db, action)?;
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
            dashboard: false,
            include_archived: false,
            after_save_new: AfterSaveNew::OpenNew,
        },
        None,
    )
    .map(|_| ())
}
pub fn compose_dashboard(
    db: &mut crate::db::Db,
    include_archived: bool,
    after_save_new: AfterSaveNew,
    save: &mut dyn FnMut(&mut crate::db::Db, Outcome) -> Result<i64>,
    action: &mut ActionHandler<'_>,
) -> Result<()> {
    compose_inner(
        "",
        Mode::Continuous {
            db,
            save,
            action: Some(action),
            dashboard: true,
            include_archived,
            after_save_new,
        },
        None,
    )
    .map(|_| ())
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
    let include_archived = matches!(
        mode,
        Mode::Continuous {
            include_archived: true,
            ..
        }
    );
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
    let mut message = String::new();
    let mut message_is_error = false;
    let mut confirmation: Option<Confirmation> = None;
    let mut action_ui: Option<ActionUi> = None;
    let mut conflict_ui: Option<conflict::View> = None;
    let mut save_revision_override = None;
    let mut saved_any = false;
    let mut buffers = DraftBuffers::default();
    loop {
        let size = terminal::size()?;
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
        let (dashboard_tasks, list_version, details_rows, has_herdr_link) = if dashboard {
            let db = mode.db().expect("dashboard has database");
            // Capture before listing so commits during rendering trigger another refresh.
            let version = db.data_version()?;
            let mut tasks = if include_archived || target_id.is_some() {
                db.list_with_archived(None, true)?
            } else {
                db.list(None)?
            };
            if let Some(task) = tasks.iter().find(|task| Some(task.id) == target_id) {
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
            let details_area =
                dashboard::panes(ratatui::layout::Rect::new(0, 0, size.0, size.1)).details;
            let details_width = usize::from(dashboard::details_content(details_area).width).max(1);
            let details_rows = match target_id.filter(|_| details_area.height > 0) {
                Some(id) => match tasks.iter().find(|task| task.id == id) {
                    Some(task) => details::rows(&db.show_task(task)?, details_width),
                    None => details::unavailable(id, details_width),
                },
                None => Vec::new(),
            };
            let has_herdr_link = match target_id {
                Some(id) => db.link(id)?.is_some(),
                None => false,
            };
            if !include_archived {
                tasks.retain(|task| !task.archived);
            }
            (Some(tasks), Some(version), details_rows, has_herdr_link)
        } else {
            (None, None, Vec::new(), false)
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
            let tasks = dashboard_tasks
                .as_ref()
                .expect("dashboard has task snapshot");
            let filter_views: Vec<_> = tasks
                .iter()
                .map(|task| panel::FilterTask {
                    id: task.id,
                    parent_id: task.parent_id,
                    description: &task.description,
                })
                .collect();
            let filtered = panel::filter_tasks(&filter_views, &filter_query);
            let displayed: Vec<_> = tasks
                .iter()
                .filter(|task| filtered.included_ids.contains(&task.id))
                .collect();
            let statuses: HashMap<_, _> = tasks
                .iter()
                .map(|task| (task.id, task.status.as_str()))
                .collect();
            let mut dirty_ids: HashSet<_> = buffers.task_ids().collect();
            if let Some(id) = target_id.filter(|_| active_dirty) {
                dirty_ids.insert(id);
            }
            let dirty_column = displayed.iter().any(|task| dirty_ids.contains(&task.id));
            rows = if !filter_query.is_empty() && displayed.is_empty() {
                Vec::new()
            } else {
                let list_width = usize::from(
                    dashboard::panes(ratatui::layout::Rect::new(0, 0, size.0, size.1))
                        .list
                        .width,
                )
                .saturating_sub(2 + if dirty_column { 4 } else { 0 });
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
            for row in &mut rows {
                row.dirty = row.task_id.is_some_and(|id| dirty_ids.contains(&id));
            }
            list_row_count = rows.len();
            visible_ids = Some(panel::visible_ids(&rows));
            let popup_content = dashboard::popup_layout(
                ratatui::layout::Rect::new(0, 0, size.0, size.1),
                usize::MAX,
            )
            .content;
            let modal_lines = conflict_ui
                .as_ref()
                .map(|ui| {
                    ui.rows(
                        usize::from(popup_content.width),
                        usize::from(popup_content.height),
                    )
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
                    Some(Confirmation::Action { action, dirty }) => Some(vec![
                        render::PopupRow::new(
                            format!("{} task #{}?", action.label(), action.id()),
                            render::PopupKind::Heading,
                        ),
                        render::PopupRow::new(
                            if *dirty { "Lose draft?" } else { "" },
                            render::PopupKind::Warning,
                        ),
                        render::PopupRow::new("y confirm  n/Esc cancel", render::PopupKind::Hint),
                    ]),
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
                            top: &mut list_top,
                            follow_selected: list_follow_selected,
                            modal_lines: modal_lines.as_deref(),
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
            popup_terminal.draw(|frame| dashboard::popup(frame, &rows, terminal.color))?;
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
                    if Some(db.data_version()?) != list_version {
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
                    && conflict_ui.is_none()
                    && match mouse.kind {
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                            dashboard::wheel_area(
                                size,
                                mouse.column,
                                mouse.row,
                                filter_focused || !filter_query.is_empty(),
                            )
                            .is_some()
                        }
                        MouseEventKind::Down(MouseButton::Left) => dashboard::click_target(
                            size,
                            mouse.column,
                            mouse.row,
                            dashboard::HitState {
                                rows: &rows,
                                filter_visible: filter_focused || !filter_query.is_empty(),
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
                if let Some(ActionUi::Input { value, error, .. }) = action_ui.as_mut() {
                    value.extend(text.chars().filter(|ch| !ch.is_control()));
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
                match dashboard::wheel_area(
                    size,
                    mouse.column,
                    mouse.row,
                    filter_focused || !filter_query.is_empty(),
                ) {
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
                        filter_visible: filter_focused || !filter_query.is_empty(),
                        list_top,
                        editor_top: top,
                        layout: &layout,
                    },
                ) {
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
                let cancel_key = control && key.code == KeyCode::Char('c');
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
                                Confirmation::Action { action, .. } => {
                                    match run_action(&mut mode, action, include_archived) {
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
                                        Err(error) => {
                                            message = format!("{error:#}");
                                            message_is_error = true;
                                            action_ui = Some(ActionUi::Error {
                                                text: message.clone(),
                                                top: 0,
                                            });
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
                if let Some(ui) = action_ui.take() {
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
                                KeyCode::Char('c') => Some(TaskAction::Complete(id)),
                                KeyCode::Char('r') if can_retry => Some(TaskAction::Retry(id)),
                                KeyCode::Char('o') => Some(TaskAction::Reopen(id)),
                                KeyCode::Char('a') => Some(TaskAction::SetArchived(id, !archived)),
                                KeyCode::Char('p') => {
                                    action_ui = Some(ActionUi::Input {
                                        id,
                                        kind: ActionInputKind::Priority,
                                        value: String::new(),
                                        error: String::new(),
                                    });
                                    None
                                }
                                KeyCode::Char('d') => {
                                    action_ui = Some(ActionUi::Input {
                                        id,
                                        kind: ActionInputKind::Parent,
                                        value: String::new(),
                                        error: String::new(),
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
                                confirmation = Some(Confirmation::Action {
                                    action,
                                    dirty: draft.is_dirty_against(&baseline.description),
                                });
                            }
                        }
                        ActionUi::Input {
                            id,
                            kind,
                            mut value,
                            mut error,
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
                                });
                            }
                            KeyCode::Enter => match parse_action_input(id, kind, &value) {
                                Ok(action) if draft.is_dirty_against(&baseline.description) => {
                                    confirmation = Some(Confirmation::Action {
                                        action,
                                        dirty: true,
                                    });
                                }
                                Ok(action) => match run_action(&mut mode, action, include_archived)
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
                                },
                                Err(problem) => {
                                    error = problem;
                                    action_ui = Some(ActionUi::Input {
                                        id,
                                        kind,
                                        value,
                                        error,
                                    });
                                }
                            },
                            _ => {
                                action_ui = Some(ActionUi::Input {
                                    id,
                                    kind,
                                    value,
                                    error,
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
                if dashboard && control && key.code == KeyCode::Char('g') {
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
                    if control && key.code == KeyCode::Char('u') {
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
    use crate::output::{Format, render};
    use serde_json::json;

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
