mod db;
mod herdr;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Local-first task queue for agent sessions. JSON output; project DB: qqq.sqlite."
)]
struct Cli {
    /// Stable owner identity. Falls back to exact Herdr pane agent session.
    #[arg(long, global = true, env = "QQQ_SESSION")]
    session: Option<String>,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Create qqq.sqlite in current directory (safe to repeat).
    Init,
    /// Create a pending task.
    Add {
        title: String,
        #[arg(short, long, default_value = "")]
        description: String,
    },
    /// List tasks in creation order.
    List,
    /// Show task, messages, image metadata, ownership history and Herdr link.
    Show { id: i64 },
    /// Replace description.
    Describe { id: i64, description: String },
    /// Return owned task or atomically claim oldest pending task. Empty queue: null.
    Next,
    /// Complete task owned by this session.
    Complete { id: i64 },
    /// Release owned task back to queue.
    Release { id: i64 },
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
    /// Link to current pane, or exact live --agent / --agent-session pair.
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
    Ok(match cli.command {
        Commands::Init => json!({"database":path}),
        Commands::Add { title, description } => json!(db.add(&title, &description)?),
        Commands::List => json!(db.list()?),
        Commands::Show { id } => db.show(id)?,
        Commands::Describe { id, description } => json!(db.describe(id, &description)?),
        Commands::Next => {
            let (session, link) = herdr::owner(cli.session.as_deref())?;
            json!(db.next(&session, link.as_ref())?)
        }
        Commands::Complete { id } => {
            let (session, _) = herdr::owner(cli.session.as_deref())?;
            json!(db.transition(id, &session, true)?)
        }
        Commands::Release { id } => {
            let (session, _) = herdr::owner(cli.session.as_deref())?;
            json!(db.transition(id, &session, false)?)
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
                    _ => herdr::current()?,
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
fn main() {
    match execute(Cli::parse()) {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("JSON value is serializable")
        ),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    }
}
