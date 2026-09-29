mod aliases;
mod config;
mod config_cli;
mod db;
mod dispatch;
mod editor;
mod herdr;
mod images;
mod output;
mod tui;
mod watch;
use anyhow::{Context, Result, ensure};
use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{io::IsTerminal, path::PathBuf, thread, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "Local-first task queue for agent sessions. Project DB: qqq.db."
)]
struct Cli {
    /// Print JSON for scripts and agents instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,
    /// Stable owner identity. Falls back to Herdr caller, then unique agent at database directory.
    #[arg(long, global = true, env = "QQQ_SESSION")]
    session: Option<String>,
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
    /// Create qqq.db in current directory (safe to repeat).
    Init,
    /// Create a new task.
    Add {
        /// Task title. Omit to compose title and description interactively.
        title: Option<String>,
        #[arg(short, long, default_value = "")]
        description: String,
        /// Use $EDITOR with the supplied title and description prefilled.
        #[arg(short, long)]
        edit: bool,
        /// Existing task that must complete before this task can be claimed.
        #[arg(long)]
        parent: Option<i64>,
        /// Attach image file bytes. Repeat for multiple images.
        #[arg(long = "image", value_name = "PATH")]
        images: Vec<PathBuf>,
    },
    /// List tasks as a dependency tree; JSON lists tasks in creation order.
    List {
        /// Maximum completed tasks to show; overrides human display default. 0 hides completed tasks.
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        max_completed: Option<i64>,
        /// Keep watching database commits and refresh the task list. Ctrl-C stops.
        #[arg(long)]
        watch: bool,
        /// Show all completed tasks, bypassing display.max-completed.
        #[arg(long, conflicts_with = "max_completed")]
        all: bool,
    },
    /// Show task, messages, image metadata, ownership history and Herdr link.
    Show {
        id: i64,
        /// Export an image belonging to this task; requires --output.
        #[arg(long, requires = "output")]
        export_image: Option<i64>,
        /// New destination file for exported image bytes.
        #[arg(long, requires = "export_image", value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Edit title and description interactively, or update supplied fields directly.
    Edit {
        /// Task ID, or negative creation index: -1 is newest, -2 second newest.
        #[arg(allow_negative_numbers = true)]
        id: i64,
        #[arg(long)]
        title: Option<String>,
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
    /// Link to caller, unique agent at DB directory, or explicit agent session.
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
    let (mut db, path) = db::Db::open(matches!(cli.command, Commands::Init))?;
    let project_dir = path
        .parent()
        .context("Database path has no parent directory")?;
    Ok(match cli.command {
        Commands::Config { .. } => unreachable!("config was handled before database lookup"),
        Commands::Init => json!({"database":path}),
        Commands::Add {
            title,
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
            match title {
                Some(title) if !edit => json!(db.add(&title, &description, parent, &images)?),
                title => {
                    let mut draft =
                        editor::compose(title.as_deref().unwrap_or(""), &description, edit)?;
                    draft.images.extend(images);
                    json!(db.save_composition(None, parent, &draft)?)
                }
            }
        }
        Commands::List { max_completed, .. } => json!(db.list(max_completed.or(display_limit))?),
        Commands::Show {
            id,
            export_image,
            output,
        } => {
            let mut detail = db.show(id)?;
            if let (Some(image), Some(path)) = (export_image, output) {
                detail["export"] = db.image_export(id, image, &path)?;
            }
            detail
        }
        Commands::Edit {
            id,
            title,
            description,
            edit,
            set_status,
            set_pending,
            images,
            reason,
            set_parent,
        } => {
            let set_status = if set_pending {
                Some(EditStatus::New)
            } else {
                set_status
            };
            let id = db.resolve_edit_id(id)?;
            let task = db.task(id)?;
            let images = images
                .iter()
                .map(|path| images::ImageInput::read(path))
                .collect::<Result<Vec<_>>>()?;
            if edit
                || (title.is_none()
                    && description.is_none()
                    && set_status.is_none()
                    && set_parent.is_none()
                    && images.is_empty())
            {
                let title = title.as_deref().unwrap_or(&task.title);
                let description = description.as_deref().unwrap_or(&task.description);
                ensure!(
                    !title.contains(['\n', '\r']),
                    "Cannot edit a multiline title in EDITOR; use --title or --description"
                );
                let mut draft = editor::compose(title, description, edit)?;
                draft.images.extend(images);
                json!(db.edit(
                    id,
                    Some(&draft.title),
                    Some(&draft.description),
                    None,
                    &draft.images,
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
                    Some(EditStatus::New) if task.status == "error" => {
                        Some(cli.session.clone().unwrap_or_else(|| "manual".to_owned()))
                    }
                    Some(EditStatus::New) => {
                        Some(herdr::owner(cli.session.as_deref(), project_dir, &db)?.0)
                    }
                    Some(EditStatus::Error) => {
                        Some(herdr::owner(cli.session.as_deref(), project_dir, &db)?.0)
                    }
                    None => None,
                };
                let transition = match set_status {
                    Some(EditStatus::New) if task.status == "error" => {
                        Some(db::EditTransition::RetryError(session.as_deref().unwrap()))
                    }
                    Some(EditStatus::New) => {
                        Some(db::EditTransition::New(session.as_deref().unwrap()))
                    }
                    Some(EditStatus::Error) => Some(db::EditTransition::Error {
                        session: session.as_deref().unwrap(),
                        reason: reason.as_deref().unwrap(),
                    }),
                    None => None,
                };
                json!(db.edit(
                    id,
                    title.as_deref(),
                    description.as_deref(),
                    transition,
                    &images,
                    set_parent
                )?)
            }
        }
        Commands::Next { local, wait } => {
            let dispatch = !local && config::load()?.herdr.next_to_new_agent;
            let local_owner = if dispatch {
                None
            } else {
                Some(herdr::owner(cli.session.as_deref(), project_dir, &db)?)
            };
            loop {
                let task = match &local_owner {
                    Some((session, link)) => db.next(session, link.as_ref())?,
                    None => dispatch::next(&mut db, cli.session.as_deref())?,
                };
                if task.is_some() || !wait {
                    break json!(task);
                }
                // Claims commit before waiting; no write lock spans the sleep.
                thread::sleep(Duration::from_millis(250));
            }
        }
        Commands::Complete { id } => {
            let (session, _) = herdr::owner(cli.session.as_deref(), project_dir, &db)?;
            json!(db.complete(id, &session)?)
        }
        Commands::Message { id, body } => db.message(id, &body, cli.session.as_deref())?,
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
                db.set_link(task_id, &link)?;
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
        output::render(format, &value, color)
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
