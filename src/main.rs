mod aliases;
mod config;
mod config_cli;
mod db;
mod delete;
mod dispatch;
mod doctor;
mod editor;
mod herdr;
mod identity;
mod images;
mod list_filter;
mod output;
mod session;
mod snapshot;
mod sql_filter;
mod tui;
mod watch;
use anyhow::{Context, Result, ensure};
use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{io::IsTerminal, path::PathBuf, thread, time::Duration};

#[derive(Parser)]
#[command(
    name = "qqq",
    version,
    about = "Local-first task queue for agent sessions. Project DB: .qqq/qqq.db."
)]
struct Cli {
    /// Print JSON for scripts and agents instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,
    /// Stable owner identity. Falls back to exact Herdr pane, Codex session, then unique Herdr agent at project root.
    #[arg(long, global = true, env = "QQQ_SESSION")]
    session: Option<String>,
    /// Override coding harness name (for example codex).
    #[arg(long, global = true)]
    harness_name: Option<String>,
    /// Override displayed harness session; also supplies ownership input without --session.
    #[arg(long, global = true)]
    harness_session: Option<String>,
    /// Override orchestrator name (for example herdr).
    #[arg(long, global = true)]
    orchestrator_name: Option<String>,
    /// Override orchestrator server session.
    #[arg(long, global = true)]
    orchestrator_session: Option<String>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Clone, Copy, ValueEnum)]
enum EditStatus {
    New,
    Error,
}

#[derive(Subcommand)]
enum Commands {
    /// Read or edit ~/.config/qqq/config.toml; no project database required.
    #[command(group(ArgGroup::new("action").required(true).args(["list", "get", "unset", "key"])))]
    Config {
        /// List explicitly stored values as dotted keys.
        #[arg(long)]
        list: bool,
        /// Read a dotted key (also supported as a positional key).
        #[arg(long)]
        get: Option<String>,
        /// Remove a dotted key.
        #[arg(long)]
        unset: Option<String>,
        /// Dotted key to read, or set when VALUE is supplied.
        key: Option<String>,
        /// New value. Alias values are strings; other values accept TOML literals.
        #[arg(requires = "key")]
        value: Option<String>,
    },
    /// Create .qqq/qqq.db in current directory (safe to repeat).
    Init,
    /// Create tasks; built-in editor can browse and edit existing tasks.
    Add {
        /// Whole task description. Omit to compose interactively.
        #[arg(conflicts_with = "description")]
        text: Option<String>,
        /// Whole task description; alternative to positional TEXT.
        #[arg(short, long)]
        description: Option<String>,
        /// Use $EDITOR with supplied description prefilled.
        #[arg(short, long)]
        edit: bool,
        /// Existing task that must complete before this task can be claimed.
        #[arg(long)]
        parent: Option<i64>,
        /// Claim order: -100 through 100, higher first; defaults to 0.
        #[arg(long, default_value_t = 0, allow_hyphen_values = true, value_parser = clap::value_parser!(i64).range(-100..=100))]
        priority: i64,
        /// Attach image file bytes. Repeat for multiple images.
        #[arg(long = "image", value_name = "PATH")]
        images: Vec<PathBuf>,
    },
    /// Browse tasks above a continuous interactive editor.
    Tui {
        /// Include archived tasks in dashboard browsing.
        #[arg(long)]
        include_archived: bool,
    },
    /// List tasks as a dependency tree; JSON preserves whole text.
    List {
        /// Include archived tasks; default list hides them.
        #[arg(long)]
        include_archived: bool,
        /// Match a Luau expression compiled to SQLite; see docs/filter.md.
        #[arg(long, value_name = "EXPR", allow_hyphen_values = true)]
        filter: Option<String>,
        /// Match text anywhere in the full description, ignoring Unicode case.
        #[arg(long, value_name = "TEXT", allow_hyphen_values = true)]
        query: Option<String>,
        /// Include a status; repeat to match any supplied status.
        #[arg(long = "status", value_enum, value_name = "STATUS")]
        statuses: Vec<list_filter::ListStatus>,
        /// Maximum completed tasks to show; overrides human display default. 0 hides completed tasks.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        max_completed: Option<i64>,
        /// Keep watching database commits and refresh the task list. Ctrl-C stops.
        #[arg(long)]
        watch: bool,
        /// Show first description line only in human terminal output.
        #[arg(long)]
        oneline: bool,
        /// Show all completed tasks, bypassing display.max-completed.
        #[arg(short = 'a', long, conflicts_with = "max_completed")]
        all: bool,
    },
    /// Show task, messages, image metadata, ownership history and Herdr link.
    Show {
        /// Task ID, or negative creation index: -1 is newest, -2 second newest.
        #[arg(allow_negative_numbers = true)]
        id: i64,
        /// Export an image belonging to this task; requires --output.
        #[arg(long, requires = "output")]
        export_image: Option<i64>,
        /// New destination file for exported image bytes.
        #[arg(long, requires = "export_image", value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Edit whole description interactively, or update supplied fields directly.
    Edit {
        /// Task ID, or negative creation index: -1 is newest, -2 second newest.
        #[arg(allow_negative_numbers = true)]
        id: i64,
        #[arg(short, long)]
        description: Option<String>,
        /// Reject save if description or attachments changed since this content revision.
        #[arg(long, value_parser = clap::value_parser!(i64).range(1..))]
        expected_revision: Option<i64>,
        /// Force $EDITOR with supplied fields prefilled, including attachment edits.
        #[arg(short, long, conflicts_with_all = ["set_status", "set_pending"])]
        edit: bool,
        /// Return owned task/error to new, or mark owned task error with --reason. Skips editor.
        #[arg(long, value_enum)]
        set_status: Option<EditStatus>,
        /// Return owned task/error to execution queue (new); same as --set-status new.
        #[arg(long, conflicts_with = "set_status")]
        set_pending: bool,
        /// Return active/error task to new without session discovery or owner matching.
        #[arg(long, requires = "set_status")]
        force: bool,
        /// Failure details, required and valid only with --set-status error.
        #[arg(long, requires = "set_status")]
        reason: Option<String>,
        /// Change dependency to a task ID, or use none to clear it. Skips editor unless --edit.
        #[arg(long, value_name = "ID|none")]
        set_parent: Option<db::ParentChange>,
        /// Claim order: -100 through 100, higher first.
        #[arg(long, allow_hyphen_values = true, value_parser = clap::value_parser!(i64).range(-100..=100))]
        priority: Option<i64>,
        /// Append image file bytes without opening editor. Repeat for multiple images.
        #[arg(long = "image", value_name = "PATH")]
        images: Vec<PathBuf>,
    },
    /// Hide a task from default lists and claims while preserving its data.
    Archive {
        /// Task ID, or negative creation index: -1 is newest.
        #[arg(allow_negative_numbers = true)]
        id: i64,
    },
    /// Restore an archived task to default lists and claims.
    Unarchive {
        /// Task ID, or negative creation index: -1 is newest.
        #[arg(allow_negative_numbers = true)]
        id: i64,
    },
    /// Preview or permanently delete an archived task without dependent children.
    Delete {
        /// Positive task ID; relative creation indexes are unsafe for permanent deletion.
        #[arg(value_parser = clap::value_parser!(i64).range(1..))]
        id: i64,
        /// Confirm permanent deletion after reviewing preview and backup.
        #[arg(long)]
        yes: bool,
    },
    /// Return owned task or atomically claim highest-priority ready task (oldest ID on ties).
    Next {
        /// Filter queued candidates with Luau; normal next still returns owned task. See docs/filter.md.
        #[arg(long, value_name = "EXPR", allow_hyphen_values = true)]
        filter: Option<String>,
        /// Wait until a task is available to claim or preview.
        #[arg(long)]
        wait: bool,
        /// Preview queued candidate without claiming or returning an owned task.
        #[arg(long)]
        dry_run: bool,
        /// Claim locally even when Herdr new-agent dispatch is configured.
        #[arg(long)]
        local: bool,
    },
    /// Mark task completed; supplied or discovered session ID must match recorded owner.
    Complete { id: i64 },
    /// Return completed, unarchived task to new while keeping its history.
    Reopen {
        /// Task ID, or negative creation index: -1 is newest.
        #[arg(allow_negative_numbers = true)]
        id: i64,
    },
    /// Write consistent project snapshot with database and attachments.
    Backup { destination: PathBuf },
    /// Restore snapshot into new or empty project location.
    Restore { source: PathBuf },
    /// Check project database and stored images without changing files.
    Doctor,
    /// Append message; session, when supplied, is recorded as author.
    Message { id: i64, body: String },
    /// Link tasks to exact Herdr agent sessions, find their live panes.
    Herdr {
        #[command(subcommand)]
        command: HerdrCommand,
    },
}
#[derive(Subcommand)]
enum HerdrCommand {
    /// Link to caller, unique agent at project root, or explicit agent session.
    Link {
        task_id: i64,
        #[arg(long, requires = "agent_session")]
        agent: Option<String>,
        #[arg(long, requires = "agent")]
        agent_session: Option<String>,
        /// Named Herdr server for explicit identity lookup.
        #[arg(long, requires = "agent")]
        server: Option<String>,
    },
    /// Find live pane by stored agent session identity.
    Find { task_id: i64 },
}
fn local_owner(cli: &Cli, project_dir: &std::path::Path, db: &db::Db) -> Result<session::Owner> {
    if let Some(session) = cli.session.as_deref() {
        return session::owner(Some(session), project_dir, db);
    }
    if let Some(session) = cli.harness_session.as_deref() {
        if db
            .owned_with_name(session, cli.harness_name.as_deref())?
            .is_some()
        {
            return session::owner(Some(session), project_dir, db);
        }
        // Override public session while retaining auto-fill; local explicit input
        // remains usable when Herdr is unavailable or ambiguous.
        return session::owner(None, project_dir, db)
            .or_else(|_| session::owner(Some(session), project_dir, db));
    }
    session::owner(None, project_dir, db)
}
fn execute(
    cli: Cli,
    display_limit: Option<i64>,
    filter: Option<&sql_filter::CompiledFilter>,
) -> Result<Value> {
    if let Commands::Config {
        list,
        get,
        unset,
        key,
        value,
    } = &cli.command
    {
        return config_cli::execute(
            *list,
            get.as_deref(),
            unset.as_deref(),
            key.as_deref(),
            value.as_deref(),
        );
    }
    if matches!(&cli.command, Commands::Doctor) {
        return doctor::run();
    }
    let overrides = identity::Identity {
        harness_name: cli.harness_name.clone(),
        harness_session: cli.harness_session.clone(),
        orchestrator_name: cli.orchestrator_name.clone(),
        orchestrator_session: cli.orchestrator_session.clone(),
    };
    overrides.validate()?;
    if let Commands::Restore { source } = &cli.command {
        return snapshot::restore::run(source);
    }
    if let Commands::Delete { id, yes: false } = &cli.command {
        return Ok(json!(delete::preview_cli(*id)?));
    }
    let session_input = cli.session.as_deref().or(cli.harness_session.as_deref());
    let (mut db, path) = db::Db::open(matches!(cli.command, Commands::Init))?;
    delete::recover(&mut db.conn, &path)?;
    let project_dir = path
        .parent()
        .and_then(|directory| directory.parent())
        .context("Database path has no project directory")?;
    let next_owner = match &cli.command {
        Commands::Next {
            local,
            dry_run: false,
            ..
        } if *local || !config::load()?.herdr.next_to_new_agent => {
            Some(local_owner(&cli, project_dir, &db)?)
        }
        _ => None,
    };
    Ok(match cli.command {
        Commands::Config { .. } => unreachable!("config was handled before database lookup"),
        Commands::Init => json!({"database":path}),
        Commands::Add {
            text,
            description,
            edit,
            parent,
            priority,
            images,
        } => {
            if let Some(id) = parent {
                db.task(id)?;
            }
            let images = images
                .iter()
                .map(|path| images::ImageInput::read(path))
                .collect::<Result<Vec<_>>>()?;
            match text.or(description) {
                Some(description) if !edit => {
                    json!(db.add_with_priority(&description, parent, &images, priority)?)
                }
                None if editor::uses_builtin(edit) => {
                    let mut saved = Vec::new();
                    let mut first_images = images;
                    tui::compose_continuously(&mut db, &mut |db, mut outcome| {
                        outcome
                            .composition
                            .images
                            .extend(first_images.iter().cloned());
                        let task = db.save_composition_with_priority(
                            outcome.target_id,
                            parent,
                            &outcome.composition,
                            priority,
                        )?;
                        first_images.clear();
                        let id = task.id;
                        saved.push(task);
                        Ok(id)
                    })?;
                    json!(saved)
                }
                description => {
                    let mut outcome =
                        editor::compose(description.as_deref().unwrap_or(""), edit, Some(&db))?;
                    outcome.composition.images.extend(images);
                    json!(db.save_composition_with_priority(
                        outcome.target_id,
                        parent,
                        &outcome.composition,
                        priority
                    )?)
                }
            }
        }
        Commands::Tui { include_archived } => {
            let settings = config::load_tui()?;
            tui::compose_dashboard(
                &mut db,
                include_archived,
                settings.tui.after_save_new,
                &mut |db, outcome| {
                    let task = db.save_composition(
                        outcome.target_id,
                        outcome.parent_id,
                        &outcome.composition,
                    )?;
                    Ok(task.id)
                },
                &mut |db, action| match action {
                    tui::TaskAction::Complete(id) => {
                        let owner = session::owner(session_input, project_dir, db)?;
                        db.complete(id, &owner.key, overrides.harness_name.as_deref())
                    }
                    tui::TaskAction::Retry(id) => db.edit_with_priority(
                        id,
                        None,
                        Some(db::EditTransition::RetryError(
                            session_input.unwrap_or("manual"),
                        )),
                        &[],
                        None,
                        None,
                    ),
                    tui::TaskAction::Reopen(id) => db.reopen(id, session_input.unwrap_or("cli")),
                    tui::TaskAction::SetArchived(id, archived) => {
                        db.set_archived(id, archived, session_input.unwrap_or("cli"))
                    }
                    tui::TaskAction::Priority(id, priority) => {
                        db.edit_with_priority(id, None, None, &[], None, Some(priority))
                    }
                    tui::TaskAction::Parent(id, parent) => {
                        db.edit_with_priority(id, None, None, &[], Some(parent), None)
                    }
                },
            )?;
            Value::Null
        }
        Commands::List {
            max_completed,
            query,
            statuses,
            include_archived,
            ..
        } => {
            let (tasks, matches) =
                db.list_filtered(max_completed.or(display_limit), include_archived, filter)?;
            json!(list_filter::filter_tasks(
                tasks,
                query.as_deref(),
                &statuses,
                matches.as_ref()
            ))
        }
        Commands::Show {
            id,
            export_image,
            output,
        } => {
            let id = db.resolve_task_id(id)?;
            let mut detail = db.show(id)?;
            if let (Some(image), Some(path)) = (export_image, output) {
                detail["export"] = db.image_export(id, image, &path)?;
            }
            detail
        }
        Commands::Edit {
            id,
            description,
            expected_revision,
            edit,
            set_status,
            set_pending,
            force,
            images,
            reason,
            set_parent,
            priority,
        } => {
            ensure!(
                !force || matches!(set_status, Some(EditStatus::New)),
                "--force is only valid with --set-status new"
            );
            let set_status = if set_pending {
                Some(EditStatus::New)
            } else {
                set_status
            };
            let id = db.resolve_task_id(id)?;
            let task = db.task(id)?;
            let images = images
                .iter()
                .map(|path| images::ImageInput::read(path))
                .collect::<Result<Vec<_>>>()?;
            if edit
                || (description.is_none()
                    && set_status.is_none()
                    && set_parent.is_none()
                    && priority.is_none()
                    && images.is_empty())
            {
                let snapshot = db.content_snapshot(id)?;
                if let Some(expected) = expected_revision {
                    if expected != snapshot.task.content_revision {
                        return Err(db::ContentConflict {
                            task_id: id,
                            expected_revision: expected,
                            current: Some(db::CurrentContent {
                                description: snapshot.task.description,
                                revision: snapshot.task.content_revision,
                            }),
                        }
                        .into());
                    }
                }
                if editor::uses_builtin(edit) {
                    let description = description.as_deref().unwrap_or(&snapshot.task.description);
                    let mut outcome = tui::compose_existing(description, id, &snapshot.references)?;
                    outcome.composition.images.extend(images);
                    json!(db.edit_composition_guarded(
                        id,
                        &outcome.composition,
                        set_parent,
                        priority,
                        Some(snapshot.task.content_revision),
                    )?)
                } else {
                    json!(editor::edit_external(
                        &mut db,
                        snapshot,
                        editor::ExternalEdit {
                            description: description.as_deref(),
                            images: &images,
                            parent: set_parent,
                            priority,
                        }
                    )?)
                }
            } else {
                ensure!(
                    reason.is_none() || matches!(set_status, Some(EditStatus::Error)),
                    "--reason is only valid with --set-status error"
                );
                if matches!(set_status, Some(EditStatus::Error)) {
                    db::nonempty(
                        reason
                            .as_deref()
                            .context("--set-status error requires --reason")?,
                        "Error reason",
                    )?;
                }
                let session = match set_status {
                    Some(EditStatus::New) if force => {
                        Some(session_input.unwrap_or("manual").to_owned())
                    }
                    Some(EditStatus::New) if task.status == "error" => {
                        Some(session_input.unwrap_or("manual").to_owned())
                    }
                    Some(EditStatus::New | EditStatus::Error) => {
                        Some(session::owner(session_input, project_dir, &db)?.key)
                    }
                    None => None,
                };
                let transition = match set_status {
                    Some(EditStatus::New) if force => {
                        Some(db::EditTransition::ForceNew(session.as_deref().unwrap()))
                    }
                    Some(EditStatus::New) if task.status == "error" => {
                        Some(db::EditTransition::RetryError(session.as_deref().unwrap()))
                    }
                    Some(EditStatus::New) => Some(db::EditTransition::New {
                        session: session.as_deref().unwrap(),
                        harness_name: overrides.harness_name.as_deref(),
                    }),
                    Some(EditStatus::Error) => Some(db::EditTransition::Error {
                        session: session.as_deref().unwrap(),
                        reason: reason.as_deref().unwrap(),
                        harness_name: overrides.harness_name.as_deref(),
                    }),
                    None => None,
                };
                json!(db.edit_guarded(
                    id,
                    description.as_deref(),
                    &images,
                    db::EditOptions {
                        transition,
                        parent: set_parent,
                        priority,
                        expected_revision,
                    }
                )?)
            }
        }
        Commands::Archive { id } => {
            let id = db.resolve_task_id(id)?;
            json!(db.set_archived(id, true, session_input.unwrap_or("cli"))?)
        }
        Commands::Unarchive { id } => {
            let id = db.resolve_task_id(id)?;
            json!(db.set_archived(id, false, session_input.unwrap_or("cli"))?)
        }
        Commands::Delete { id, yes: true } => {
            json!(delete::run(&mut db, &path, id)?)
        }
        Commands::Delete { yes: false, .. } => {
            unreachable!("delete preview handled before database open")
        }
        Commands::Next { wait, dry_run, .. } => {
            loop {
                let task = if dry_run {
                    db.peek_next_filtered(filter)?
                } else {
                    match &next_owner {
                        Some(owner) => db.next_with_identity_filtered(
                            &owner.key,
                            owner.link.as_ref(),
                            owner.metadata.as_ref(),
                            &overrides,
                            filter,
                        )?,
                        None => dispatch::next(&mut db, session_input, &overrides, filter)?,
                    }
                };
                if task.is_some() || !wait {
                    break json!(task);
                }
                // Claim/read transactions end before waiting; no lock spans the sleep.
                thread::sleep(Duration::from_millis(250));
            }
        }
        Commands::Complete { id } => {
            let owner = session::owner(session_input, project_dir, &db)?;
            json!(db.complete(id, &owner.key, overrides.harness_name.as_deref())?)
        }
        Commands::Reopen { id } => {
            let id = db.resolve_task_id(id)?;
            json!(db.reopen(id, session_input.unwrap_or("cli"))?)
        }
        Commands::Backup { destination } => snapshot::backup::run(&mut db, &path, &destination)?,
        Commands::Restore { .. } => unreachable!("restore handled before database open"),
        Commands::Doctor => unreachable!("doctor handled before database open"),
        Commands::Message { id, body } => db.message(id, &body, session_input)?,
        Commands::Herdr { command } => match command {
            HerdrCommand::Link {
                task_id,
                agent,
                agent_session,
                server,
            } => {
                db.task(task_id)?;
                let link = match (agent, agent_session) {
                    (Some(agent), Some(session)) => herdr::explicit(agent, session, server)?,
                    _ => herdr::owner(None, project_dir, &db)?
                        .1
                        .context("Missing automatically discovered Herdr link")?,
                };
                db.set_link_with_identity(task_id, &link, &overrides, None)?;
                json!(link)
            }
            HerdrCommand::Find { task_id } => {
                db.task(task_id)?;
                let link = db
                    .link(task_id)?
                    .context("Task has no Herdr link; run qqq herdr link")?;
                json!(herdr::find(&link.identity, link.server.as_deref())?)
            }
        },
    })
}
fn run() -> Result<(Option<String>, bool)> {
    let cli = Cli::parse_from(aliases::expand(std::env::args_os().collect())?);
    let filter = match &cli.command {
        Commands::List { filter, .. } | Commands::Next { filter, .. } => {
            filter.as_deref().map(sql_filter::compile).transpose()?
        }
        _ => None,
    };
    let is_tui = matches!(&cli.command, Commands::Tui { .. });
    let filtered_list = match &cli.command {
        Commands::List {
            query, statuses, ..
        } => query.is_some() || !statuses.is_empty() || filter.is_some(),
        _ => false,
    };
    let display_limit = match &cli.command {
        Commands::List {
            all: false,
            max_completed: None,
            ..
        } if !cli.json => config::load_display()?
            .display
            .max_completed
            .map(|limit| i64::try_from(limit).context("display.max-completed is too large"))
            .transpose()?,
        _ => None,
    };
    if let Commands::List {
        watch: true,
        max_completed,
        include_archived,
        oneline,
        query,
        statuses,
        ..
    } = &cli.command
    {
        watch::run(watch::WatchOptions {
            json_output: cli.json,
            max_completed: max_completed.or(display_limit),
            include_archived: *include_archived,
            oneline: *oneline,
            display_limited: display_limit.is_some(),
            query: query.as_deref(),
            statuses,
            filter: filter.as_ref(),
        })?;
        return Ok((None, false));
    }
    let json = cli.json;
    let is_doctor = matches!(&cli.command, Commands::Doctor);
    let format = output::Format::from(&cli.command);
    let value = execute(cli, display_limit, filter.as_ref())?;
    if is_tui {
        return Ok((None, false));
    }
    let unhealthy = is_doctor && value["ok"] == false;
    Ok((
        Some(if json {
            serde_json::to_string_pretty(&value).expect("JSON value is serializable")
        } else if filtered_list && value.as_array().is_some_and(Vec::is_empty) {
            "No matching tasks.".to_owned()
        } else if display_limit.is_some() && value.as_array().is_some_and(Vec::is_empty) {
            "No tasks to display.".to_owned()
        } else {
            let terminal = std::io::stdout().is_terminal();
            let color = output::color_enabled(terminal);
            let columns = if matches!(
                &format,
                output::Format::Tasks
                    | output::Format::PriorityTasks
                    | output::Format::OnelinePriorityTasks
            ) {
                output::terminal_columns(terminal)
            } else {
                None
            };
            output::render(format, &value, color, columns)
        }),
        unhealthy,
    ))
}
fn main() {
    match run() {
        Ok((Some(output), unhealthy)) => {
            println!("{output}");
            if unhealthy {
                std::process::exit(1);
            }
        }
        Ok((None, _)) => (),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    }
}
