mod aliases;
mod db;
mod editor;
mod herdr;
mod output;
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::path::PathBuf;

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
    Pending,
}

#[derive(Subcommand)]
enum Commands {
    /// Create qqq.db in current directory (safe to repeat).
    Init,
    /// Create a pending task.
    Add {
        /// Task title. Omit to compose title and description in $EDITOR.
        title: Option<String>,
        #[arg(short, long, default_value = "")]
        description: String,
        /// Open $EDITOR with the supplied title and description prefilled.
        #[arg(short, long)]
        edit: bool,
        /// Existing task that must complete before this task can be claimed.
        #[arg(long)]
        parent: Option<i64>,
    },
    /// List tasks in creation order.
    List,
    /// Show task, messages, image metadata, ownership history and Herdr link.
    Show { id: i64 },
    /// Edit title and description in $EDITOR, or update supplied fields directly.
    Edit {
        /// Task ID, or negative creation index: -1 is newest, -2 second newest.
        #[arg(allow_negative_numbers = true)]
        id: i64,
        #[arg(long)]
        title: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
        /// Return claimed task to pending; session ID must match recorded owner. Skips editor.
        #[arg(long, value_enum)]
        set_status: Option<EditStatus>,
    },
    /// Return owned task or atomically claim oldest ready task.
    Next,
    /// Mark task completed; supplied or discovered session ID must match recorded owner.
    Complete { id: i64 },
    /// Append message; session, when supplied, is recorded as author.
    Message { id: i64, body: String },
    /// Store or export image attachments.
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
    /// Link tasks to exact Herdr agent sessions, find their live panes.
    Herdr {
        #[command(subcommand)]
        command: HerdrCommand,
    },
}
#[derive(Subcommand)]
enum ImageCommand {
    Add { task_id: i64, path: PathBuf },
    Export { image_id: i64, path: PathBuf },
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
fn execute(cli: Cli) -> Result<Value> {
    let (mut db, path) = db::Db::open(matches!(cli.command, Commands::Init))?;
    let project_dir = path
        .parent()
        .context("Database path has no parent directory")?;
    Ok(match cli.command {
        Commands::Init => json!({"database":path}),
        Commands::Add {
            title,
            description,
            edit,
            parent,
        } => {
            if let Some(id) = parent {
                db.task(id)?;
            }
            let (title, description) = match title {
                Some(title) if !edit => (title, description),
                title => editor::compose(title.as_deref().unwrap_or(""), &description)?,
            };
            json!(db.add(&title, &description, parent)?)
        }
        Commands::List => json!(db.list()?),
        Commands::Show { id } => db.show(id)?,
        Commands::Edit {
            id,
            title,
            description,
            set_status,
        } => {
            let id = db.resolve_edit_id(id)?;
            if title.is_none() && description.is_none() && set_status.is_none() {
                let task = db.task(id)?;
                ensure!(
                    !task.title.contains(['\n', '\r']),
                    "Cannot edit a multiline title in EDITOR; use --title or --description"
                );
                let (title, description) = editor::compose(&task.title, &task.description)?;
                json!(db.edit(id, Some(&title), Some(&description), None)?)
            } else {
                let release_session = match set_status {
                    Some(EditStatus::Pending) => {
                        Some(herdr::owner(cli.session.as_deref(), project_dir)?.0)
                    }
                    None => None,
                };
                json!(db.edit(
                    id,
                    title.as_deref(),
                    description.as_deref(),
                    release_session.as_deref()
                )?)
            }
        }
        Commands::Next => {
            let (session, link) = herdr::owner(cli.session.as_deref(), project_dir)?;
            json!(db.next(&session, link.as_ref())?)
        }
        Commands::Complete { id } => {
            let (session, _) = herdr::owner(cli.session.as_deref(), project_dir)?;
            json!(db.complete(id, &session)?)
        }
        Commands::Message { id, body } => db.message(id, &body, cli.session.as_deref())?,
        Commands::Image { command } => match command {
            ImageCommand::Add { task_id, path } => db.image_add(task_id, &path)?,
            ImageCommand::Export { image_id, path } => db.image_export(image_id, &path)?,
        },
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
                    _ => herdr::discover(project_dir)?,
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
fn run() -> Result<String> {
    let cli = Cli::parse_from(aliases::expand(std::env::args_os().collect())?);
    let json = cli.json;
    let format = output::Format::from(&cli.command);
    let value = execute(cli)?;
    Ok(if json {
        serde_json::to_string_pretty(&value).expect("JSON value is serializable")
    } else {
        output::render(format, &value)
    })
}
fn main() {
    match run() {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    }
}
