mod clipboard;
mod dashboard;
mod details;
pub mod draft;
mod panel;
mod render;

use crate::config::AfterSaveNew;
use anyhow::{Result, bail, ensure};
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
use std::collections::HashMap;
use std::io::{self, IsTerminal};
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct Outcome {
    pub composition: Composition,
    pub target_id: Option<i64>,
    pub parent_id: Option<i64>,
}

enum Target {
    New {
        parent_id: Option<i64>,
    },
    Task {
        id: i64,
        description: String,
        status: String,
        draft: Draft,
    },
}

enum Confirmation {
    Exit,
    Switch { target: Target, focus_editor: bool },
    Action { action: TaskAction, dirty: bool },
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

enum ActionUi {
    Menu {
        id: i64,
        archived: bool,
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

fn action_lines(ui: &ActionUi, width: usize, height: usize) -> Vec<String> {
    match ui {
        ActionUi::Menu { id, archived } => vec![
            format!("Task actions #{id}"),
            "c Complete".to_owned(),
            "r Retry error".to_owned(),
            "o Reopen".to_owned(),
            format!("a {}", if *archived { "Unarchive" } else { "Archive" }),
            "p Priority".to_owned(),
            "d Parent".to_owned(),
            "Esc cancel".to_owned(),
        ],
        ActionUi::Input {
            id,
            kind,
            value,
            error,
        } => {
            let mut lines = match kind {
                ActionInputKind::Priority => {
                    vec![format!("Priority task #{id}"), "Enter -100..100".to_owned()]
                }
                ActionInputKind::Parent => {
                    vec![format!("Parent task #{id}"), "ID / none".to_owned()]
                }
            };
            lines.push(format!("> {value}"));
            if !error.is_empty() {
                lines.extend(wrap_modal(error, width));
            }
            lines.push("Enter apply  Esc cancel".to_owned());
            lines
        }
        ActionUi::Error { text, top } => {
            let wrapped = wrap_modal(text, width);
            let available = height.saturating_sub(2);
            let start = (*top).min(wrapped.len().saturating_sub(available));
            let mut lines = vec!["Action error".to_owned()];
            lines.extend(wrapped.into_iter().skip(start).take(available));
            lines.push("Up/Down Esc".to_owned());
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

fn task_target(db: &crate::db::Db, id: i64, description: String, status: String) -> Result<Target> {
    let draft = Draft::from_saved(&description, id, &db.image_references(id)?)?;
    Ok(Target::Task {
        id,
        description,
        status,
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
        Some((id, description, status)) => Some(task_target(db, id, description, status)?),
        None if !older => Some(Target::New { parent_id: None }),
        None => None,
    })
}

fn load_target(
    target: Target,
    draft: &mut Draft,
    target_id: &mut Option<i64>,
    target_status: &mut Option<String>,
    draft_parent_id: &mut Option<i64>,
    baseline: &mut String,
    top: &mut usize,
) {
    match target {
        Target::New { parent_id } => {
            *target_id = None;
            *target_status = None;
            *draft_parent_id = parent_id;
            baseline.clear();
            *draft = Draft::new("");
        }
        Target::Task {
            id,
            description,
            status,
            draft: loaded,
        } => {
            *target_id = Some(id);
            *target_status = Some(status);
            *draft_parent_id = None;
            *baseline = description;
            *draft = loaded;
        }
    }
    draft.set_cursor(0);
    *top = 0;
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
            Self::Continuous { db, .. } => Some(db),
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
    task_target(db, task.id, task.description, task.status)
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
    description: &str,
    task_id: i64,
    references: &[crate::images::ImageReference],
) -> Result<Outcome> {
    let draft = Draft::from_saved(description, task_id, references)?;
    compose_inner(description, Mode::Single(None), Some(draft))
        .map(|saved| saved.expect("single editor returns saved composition"))
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
    initial_draft: Option<Draft>,
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
        Mode::Single(_) => AfterSaveNew::default(),
    };
    let terminal = TerminalGuard::enter(dashboard)?;
    let mut dashboard_terminal = if dashboard {
        Some(Terminal::new(CrosstermBackend::new(io::stderr()))?)
    } else {
        None
    };
    let mut draft = initial_draft.unwrap_or_else(|| Draft::new(description));
    let mut baseline = description.to_owned();
    let mut target_id = None;
    let mut target_status = None;
    let mut draft_parent_id = None;
    let mut top = 0;
    let mut list_top = 0;
    let mut list_follow_selected = true;
    let mut editor_follow_cursor = true;
    let mut filter_query = String::new();
    let mut filter_focused = false;
    let mut message = String::new();
    let mut message_is_error = false;
    let mut confirmation: Option<Confirmation> = None;
    let mut action_ui: Option<ActionUi> = None;
    let mut saved_any = false;
    loop {
        let size = terminal::size()?;
        let fragments = draft.fragments();
        let image_mask = draft.image_mask();
        let paste_mask = draft.paste_mask();
        let layout = if paste_mask.contains(&true) {
            render::Layout::with_paste(&fragments, &image_mask, &paste_mask, size.0 as usize)
        } else {
            render::Layout::new(&fragments, &image_mask, size.0 as usize)
        };
        let (dashboard_tasks, list_version) = if dashboard {
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
            }
            if !include_archived {
                tasks.retain(|task| !task.archived);
            }
            (Some(tasks), Some(version))
        } else {
            (None, None)
        };
        let footer = match &confirmation {
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
        let (title, status_start) = match (mode.db().is_some(), target_id) {
            (true, Some(id)) => {
                let prefix = format!("qqq task editor - task #{id} ");
                let label = crate::output::status_label(
                    target_status.as_deref().expect("selected task has status"),
                );
                (format!("{prefix}({label})"), Some(prefix.len()))
            }
            (true, None) => (
                match draft_parent_id {
                    Some(parent) => format!("qqq task editor - new task (parent #{parent})"),
                    None => "qqq task editor - new task".to_owned(),
                },
                None,
            ),
            (false, _) => ("qqq task editor".to_owned(), None),
        };
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
            keys: if filter_focused {
                render::FILTER_KEYS
            } else if dashboard {
                render::DASHBOARD_KEYS
            } else {
                match &mode {
                    Mode::Continuous { .. } => render::ADD_KEYS,
                    Mode::Single(Some(_)) => render::NAV_KEYS,
                    Mode::Single(None) => render::KEYS,
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
            rows = if !filter_query.is_empty() && displayed.is_empty() {
                Vec::new()
            } else {
                let tree = crate::output::render(
                    crate::output::Format::Tasks,
                    &serde_json::json!(displayed),
                    false,
                    Some(usize::from(size.0).saturating_sub(2)),
                );
                panel::rows(&tree, usize::from(size.0).saturating_sub(2))
            };
            list_row_count = rows.len();
            visible_ids = Some(panel::visible_ids(&rows));
            let modal_lines = action_ui
                .as_ref()
                .map(|ui| action_lines(ui, usize::from(size.0), usize::from(size.1)))
                .or_else(|| match &confirmation {
                    Some(Confirmation::Action { action, dirty }) => Some(vec![
                        format!("{} task #{}?", action.label(), action.id()),
                        if *dirty { "Lose draft?" } else { "" }.to_owned(),
                        "y confirm  n/Esc cancel".to_owned(),
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
                        dashboard::ListView {
                            query: &filter_query,
                            focused: filter_focused,
                            top: &mut list_top,
                            follow_selected: list_follow_selected,
                            modal_lines: modal_lines.as_deref(),
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
                    && match mouse.kind {
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                            dashboard::wheel_area(
                                size,
                                target_id.is_some(),
                                mouse.column,
                                mouse.row,
                            )
                            .is_some()
                        }
                        MouseEventKind::Down(MouseButton::Left) => dashboard::click_target(
                            size,
                            mouse.column,
                            mouse.row,
                            dashboard::HitState {
                                selected: target_id.is_some(),
                                rows: &rows,
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
            Event::Paste(text) if confirmation.is_none() => {
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
                match dashboard::wheel_area(size, target_id.is_some(), mouse.column, mouse.row) {
                    Some(dashboard::WheelArea::List(height)) => {
                        list_follow_selected = false;
                        list_top = panel::wheel_top(list_top, list_row_count, height, down);
                    }
                    Some(dashboard::WheelArea::Editor(height)) => {
                        editor_follow_cursor = false;
                        top = panel::wheel_top(top, layout.rows.len(), height, down);
                    }
                    Some(dashboard::WheelArea::Details(_)) => (),
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
                        selected: target_id.is_some(),
                        rows: &rows,
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
                        let target = db.task(id).and_then(|task| {
                            task_target(db, task.id, task.description, task.status)
                        });
                        match target {
                            Ok(target) if draft.is_dirty_against(&baseline) => {
                                confirmation = Some(Confirmation::Switch {
                                    target,
                                    focus_editor: true,
                                });
                            }
                            Ok(target) => {
                                load_target(
                                    target,
                                    &mut draft,
                                    &mut target_id,
                                    &mut target_status,
                                    &mut draft_parent_id,
                                    &mut baseline,
                                    &mut top,
                                );
                                list_follow_selected = true;
                                editor_follow_cursor = true;
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
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                let cancel_key = control && key.code == KeyCode::Char('c');
                if dashboard
                    && target_id.is_none()
                    && cancel_key
                    && draft.is_dirty_against(&baseline)
                {
                    confirmation = Some(Confirmation::Exit);
                    action_ui = None;
                    filter_focused = false;
                    continue;
                }
                let editor_escape = key.code == KeyCode::Esc
                    && confirmation.is_none()
                    && action_ui.is_none()
                    && !filter_focused;
                if dashboard && target_id.is_some() && (cancel_key || editor_escape) {
                    if !cancel_key && draft.is_dirty_against(&baseline) {
                        confirmation = Some(Confirmation::Switch {
                            target: Target::New { parent_id: None },
                            focus_editor: true,
                        });
                    } else {
                        load_target(
                            Target::New { parent_id: None },
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
                        editor_follow_cursor = true;
                        message.clear();
                        message_is_error = false;
                    }
                    continue;
                }
                if let Some(pending) = confirmation.take() {
                    if control && key.code == KeyCode::Char('c') {
                        return cancel(saved_any, dashboard);
                    }
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                    {
                        match key.code {
                            KeyCode::Char('y' | 'Y') => match pending {
                                Confirmation::Exit => return cancel(saved_any, dashboard),
                                Confirmation::Switch {
                                    target,
                                    focus_editor,
                                } => {
                                    load_target(
                                        target,
                                        &mut draft,
                                        &mut target_id,
                                        &mut target_status,
                                        &mut draft_parent_id,
                                        &mut baseline,
                                        &mut top,
                                    );
                                    list_follow_selected = true;
                                    editor_follow_cursor = true;
                                    if focus_editor {
                                        filter_focused = false;
                                    }
                                    message.clear();
                                    message_is_error = false;
                                }
                                Confirmation::Action { action, .. } => {
                                    match run_action(&mut mode, action, include_archived) {
                                        Ok(target) => {
                                            load_target(
                                                target,
                                                &mut draft,
                                                &mut target_id,
                                                &mut target_status,
                                                &mut draft_parent_id,
                                                &mut baseline,
                                                &mut top,
                                            );
                                            list_follow_selected = true;
                                            editor_follow_cursor = true;
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
                if let Some(ui) = action_ui.take() {
                    if control && key.code == KeyCode::Char('c') {
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
                        ActionUi::Menu { id, archived } => {
                            let chosen = match key.code {
                                KeyCode::Char('c') => Some(TaskAction::Complete(id)),
                                KeyCode::Char('r') => Some(TaskAction::Retry(id)),
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
                                    action_ui = Some(ActionUi::Menu { id, archived });
                                    None
                                }
                            };
                            if let Some(action) = chosen {
                                confirmation = Some(Confirmation::Action {
                                    action,
                                    dirty: draft.is_dirty_against(&baseline),
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
                                Ok(action) if draft.is_dirty_against(&baseline) => {
                                    confirmation = Some(Confirmation::Action {
                                        action,
                                        dirty: true,
                                    });
                                }
                                Ok(action) => match run_action(&mut mode, action, include_archived)
                                {
                                    Ok(target) => {
                                        load_target(
                                            target,
                                            &mut draft,
                                            &mut target_id,
                                            &mut target_status,
                                            &mut draft_parent_id,
                                            &mut baseline,
                                            &mut top,
                                        );
                                        list_follow_selected = true;
                                        editor_follow_cursor = true;
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
                    && key.code == KeyCode::Enter
                    && key.modifiers.contains(KeyModifiers::SHIFT)
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                {
                    if let Some(parent) = target_id {
                        let target = Target::New {
                            parent_id: Some(parent),
                        };
                        if draft.is_dirty_against(&baseline) {
                            confirmation = Some(Confirmation::Switch {
                                target,
                                focus_editor: true,
                            });
                        } else {
                            load_target(
                                target,
                                &mut draft,
                                &mut target_id,
                                &mut target_status,
                                &mut draft_parent_id,
                                &mut baseline,
                                &mut top,
                            );
                            filter_focused = false;
                            list_follow_selected = true;
                            editor_follow_cursor = true;
                            message.clear();
                            message_is_error = false;
                        }
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
                            Ok(Some(target)) if draft.is_dirty_against(&baseline) => {
                                confirmation = Some(Confirmation::Switch {
                                    target,
                                    focus_editor: false,
                                });
                            }
                            Ok(Some(target)) => {
                                load_target(
                                    target,
                                    &mut draft,
                                    &mut target_id,
                                    &mut target_status,
                                    &mut draft_parent_id,
                                    &mut baseline,
                                    &mut top,
                                );
                                list_follow_selected = true;
                                editor_follow_cursor = true;
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
                    if control && key.code == KeyCode::Char('c') {
                        return cancel(saved_any, dashboard);
                    }
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
                            KeyCode::Esc if filter_query.is_empty() => filter_focused = false,
                            KeyCode::Esc => {
                                filter_query.clear();
                                list_top = 0;
                                list_follow_selected = true;
                            }
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
                                };
                                match &mut mode {
                                    Mode::Single(_) => return Ok(Some(outcome)),
                                    Mode::Continuous { db, save, .. } => match save(db, outcome)
                                        .and_then(|id| {
                                            let target = if keep_saved {
                                                let task = db.task(id)?;
                                                task_target(db, id, task.description, task.status)?
                                            } else {
                                                Target::New { parent_id: None }
                                            };
                                            Ok((id, target))
                                        }) {
                                        Ok((id, target)) => {
                                            saved_any = true;
                                            load_target(
                                                target,
                                                &mut draft,
                                                &mut target_id,
                                                &mut target_status,
                                                &mut draft_parent_id,
                                                &mut baseline,
                                                &mut top,
                                            );
                                            list_follow_selected = true;
                                            editor_follow_cursor = true;
                                            message = if keep_saved {
                                                format!("Saved #{id}")
                                            } else {
                                                format!("Saved #{id}. New task")
                                            };
                                            message_is_error = false;
                                        }
                                        Err(error) => {
                                            message = format!("{error:#}");
                                            message_is_error = true;
                                        }
                                    },
                                }
                            }
                            Err(error) => {
                                message = error.to_string();
                                message_is_error = true;
                            }
                        },
                        KeyCode::Char('c') => return cancel(saved_any, dashboard),
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
            {"id":1,"description":"ABCDEFGHIJKLMN","status":"new","parent_id":null},
            {"id":2,"description":"Child first\nChild second\nChild third\nHidden","status":"new","parent_id":1}
        ]);
        for width in [24, 20, 10] {
            let tree = render(Format::Tasks, &tasks, false, Some(width));
            let rows = super::panel::rows(&tree, width);
            for id in [1, 2] {
                let preview: Vec<_> = rows.iter().filter(|row| row.task_id == Some(id)).collect();
                assert_eq!(preview.len(), 3, "task {id}, width {width}");
                assert!(
                    preview[2].text.ends_with("..."),
                    "task {id}, width {width}: {}",
                    preview[2].text
                );
                assert!(unicode_width::UnicodeWidthStr::width(preview[2].text.as_str()) <= width);
            }
        }
    }
}
