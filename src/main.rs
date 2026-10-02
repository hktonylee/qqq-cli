mod aliases;
mod config;
mod config_cli;
mod db;
mod dispatch;
mod editor;
mod herdr;
mod identity;
mod images;
mod output;
mod session;
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
        /// Attach image file bytes. Repeat for multiple images.
        #[arg(long = "image", value_name = "PATH")]
        images: Vec<PathBuf>,
    },
    /// List first description lines as a dependency tree; JSON preserves whole text.
    List {
        /// Maximum completed tasks to show; overrides human display default. 0 hides completed tasks.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        max_completed: Option<i64>,
        /// Keep watching database commits and refresh the task list. Ctrl-C stops.
        #[arg(long)]
        watch: bool,
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
        /// Append image file bytes without opening editor. Repeat for multiple images.
        #[arg(long = "image", value_name = "PATH")]
        images: Vec<PathBuf>,
    },
    /// Return owned task or atomically claim oldest ready task.
    Next {
        /// Wait until a task is available; concurrent sessions claim each task once.
        #[arg(long)]
        wait: bool,
        /// Claim locally even when Herdr new-agent dispatch is configured.
        #[arg(long)]
        local: bool,
    },
    /// Mark task completed; supplied or discovered session ID must match recorded owner.
    Complete { id: i64 },
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
fn execute(cli: Cli, display_limit: Option<i64>) -> Result<Value> {
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
    let overrides = identity::Identity {
        harness_name: cli.harness_name.clone(),
        harness_session: cli.harness_session.clone(),
        orchestrator_name: cli.orchestrator_name.clone(),
        orchestrator_session: cli.orchestrator_session.clone(),
    };
    overrides.validate()?;
    let session_input = cli.session.as_deref().or(cli.harness_session.as_deref());
    let (mut db, path) = db::Db::open(matches!(cli.command, Commands::Init))?;
    let project_dir = path
        .parent()
        .and_then(|directory| directory.parent())
        .context("Database path has no project directory")?;
    let next_owner = match &cli.command {
        Commands::Next { local, .. } if *local || !config::load()?.herdr.next_to_new_agent => {
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
                Some(description) if !edit => json!(db.add(&description, parent, &images)?),
                None if editor::uses_builtin(edit) => {
                    let mut saved = Vec::new();
                    let mut first_images = images;
                    tui::compose_continuously(&mut db, &mut |db, mut outcome| {
                        outcome
                            .composition
                            .images
                            .extend(first_images.iter().cloned());
                        let task =
                            db.save_composition(outcome.target_id, parent, &outcome.composition)?;
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
                    json!(db.save_composition(outcome.target_id, parent, &outcome.composition)?)
                }
            }
        }
        Commands::List { max_completed, .. } => json!(db.list(max_completed.or(display_limit))?),
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
            edit,
            set_status,
            set_pending,
            force,
            images,
            reason,
            set_parent,
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
                    && images.is_empty())
            {
                let description = description.as_deref().unwrap_or(&task.description);
                let mut outcome = editor::compose(description, edit, None)?;
                outcome.composition.images.extend(images);
                json!(db.edit(
                    id,
                    Some(&outcome.composition.description),
                    None,
                    &outcome.composition.images,
                    set_parent
                )?)
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
                json!(db.edit(id, description.as_deref(), transition, &images, set_parent)?)
            }
        }
        Commands::Next { wait, .. } => {
            loop {
                let task = match &next_owner {
                    Some(owner) => db.next_with_identity(
                        &owner.key,
                        owner.link.as_ref(),
                        owner.metadata.as_ref(),
                        &overrides,
                    )?,
                    None => dispatch::next(&mut db, session_input, &overrides)?,
                };
                if task.is_some() || !wait {
                    break json!(task);
                }
                // Claims commit before waiting; no write lock spans the sleep.
                thread::sleep(Duration::from_millis(250));
            }
        }
        Commands::Complete { id } => {
            let owner = session::owner(session_input, project_dir, &db)?;
            json!(db.complete(id, &owner.key, overrides.harness_name.as_deref())?)
        }
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
fn run() -> Result<Option<String>> {
    let cli = Cli::parse_from(aliases::expand(std::env::args_os().collect())?);
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
        ..
    } = &cli.command
    {
        watch::run(
            cli.json,
            max_completed.or(display_limit),
            display_limit.is_some(),
        )?;
        return Ok(None);
    }
    let json = cli.json;
    let format = output::Format::from(&cli.command);
    let value = execute(cli, display_limit)?;
    Ok(Some(if json {
        serde_json::to_string_pretty(&value).expect("JSON value is serializable")
    } else if display_limit.is_some() && value.as_array().is_some_and(Vec::is_empty) {
        "No tasks to display.".to_owned()
    } else {
        let color = output::color_enabled(std::io::stdout().is_terminal());
        output::render(format, &value, color, None)
    }))
}
fn main() {
    match run() {
        Ok(Some(output)) => println!("{output}"),
        Ok(None) => (),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    }
}
