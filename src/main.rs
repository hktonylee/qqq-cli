mod aliases;
mod archive;
mod bulk;
mod cli_error;
mod config;
mod config_cli;
mod db;
mod delete;
mod dependencies;
mod dispatch;
mod doctor;
mod editor;
mod errors;
mod graph;
mod herdr;
mod identity;
mod images;
mod import;
mod list_filter;
mod output;
mod preflight;
mod process;
mod queue;
mod recipe;
mod selection;
mod session;
mod snapshot;
mod sql_filter;
mod tags;
mod tui;
mod views;
mod watch;
use anyhow::{Context, Result, ensure};
use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{
    io::{IsTerminal, Read},
    path::PathBuf,
    thread,
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "qqq",
    version,
    about = "Local-first task queue for agent sessions. Project DB: .qqq/qqq.db."
)]
struct Cli {
    /// Print JSON; default for recognized agent callers.
    #[arg(long, global = true)]
    json: bool,
    /// Print human-readable text, overriding automatic agent JSON output.
    #[arg(long, global = true, conflicts_with = "json")]
    human: bool,
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
    /// Inspect dependency blockers and hypothetical downstream completion impact without writes.
    Graph {
        #[arg(allow_negative_numbers = true)]
        id: i64,
        #[command(flatten)]
        options: graph::Options,
    },
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
        /// Read full UTF-8 description from stdin without opening an editor.
        #[arg(long, conflicts_with_all = ["text", "description", "edit"])]
        stdin: bool,
        /// Existing task that must complete before this task can be claimed.
        #[arg(long)]
        parent: Option<i64>,
        /// Additional prerequisite task ID; repeat for multiple prerequisites.
        #[arg(long, value_name = "ID", value_parser = clap::value_parser!(i64).range(1..))]
        depends_on: Vec<i64>,
        /// Claim order: -100 through 100, higher first; defaults to 0.
        #[arg(long, default_value_t = 0, allow_hyphen_values = true, value_parser = clap::value_parser!(i64).range(-100..=100))]
        priority: i64,
        /// Attach image file bytes. Repeat for multiple images.
        #[arg(long = "image", value_name = "PATH")]
        images: Vec<PathBuf>,
        /// Tag label; repeat for multiple tags. Surrounding spaces are trimmed.
        #[arg(long = "tag", value_name = "LABEL")]
        tags: Vec<String>,
        /// Apply project-local .qqq-recipes/NAME.json instead of ordinary add.
        #[arg(long, value_name = "NAME", conflicts_with_all = ["text", "description", "edit", "stdin", "parent", "depends_on", "priority", "images", "tags"])]
        template: Option<String>,
        /// Literal recipe parameter; repeat for multiple parameters.
        #[arg(long = "var", value_name = "NAME=VALUE", requires = "template")]
        vars: Vec<String>,
        /// Expand and validate recipe without creating tasks or reserving IDs.
        #[arg(long, requires = "template")]
        dry_run: bool,
    },
    /// Atomically import a versioned JSON task batch; use - for stdin.
    Import {
        path: PathBuf,
        /// Validate and preview without creating tasks or reserving IDs.
        #[arg(long)]
        dry_run: bool,
    },
    /// Preview metadata changes; explicitly apply exact saved JSON preview.
    Bulk(bulk::Args),
    /// Browse tasks above a continuous interactive editor.
    Tui {
        /// Include archived tasks in dashboard browsing.
        #[arg(long)]
        include_archived: bool,
        /// Hide archived tasks, overriding a saved view.
        #[arg(long, conflicts_with = "include_archived")]
        hide_archived: bool,
        /// Select project saved view; Ctrl-B switches views interactively.
        #[arg(long, value_name = "NAME")]
        view: Option<String>,
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        max_completed: Option<i64>,
        #[arg(short = 'a', long, conflicts_with = "max_completed")]
        all: bool,
        #[command(flatten)]
        selectors: selection::Selectors,
    },
    /// List tasks as a dependency tree; JSON preserves whole text.
    List {
        /// Include archived tasks; default list hides them.
        #[arg(long)]
        include_archived: bool,
        /// Hide archived tasks, overriding a saved view.
        #[arg(long, conflicts_with = "include_archived")]
        hide_archived: bool,
        /// Select project saved view; extra selectors use AND.
        #[arg(long, value_name = "NAME")]
        view: Option<String>,
        #[command(flatten)]
        selectors: selection::Selectors,
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
        /// Add prerequisite task ID. Repeating an existing edge is an error.
        #[arg(long, value_name = "ID", conflicts_with = "edit", value_parser = clap::value_parser!(i64).range(1..))]
        depends_on: Vec<i64>,
        /// Remove prerequisite task ID; repeat for multiple removals.
        #[arg(long, value_name = "ID", conflicts_with_all = ["edit", "clear_depends_on"], value_parser = clap::value_parser!(i64).range(1..))]
        remove_depends_on: Vec<i64>,
        /// Remove all extra prerequisites; combine with --depends-on to replace.
        #[arg(long, conflicts_with = "edit")]
        clear_depends_on: bool,
        /// Replace comma-separated tags; an empty string clears tags. Skips editor.
        #[arg(long, value_name = "TAGS", conflicts_with = "edit")]
        set_tags: Option<String>,
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
    /// Summarize queue readiness, owners, blockers and latest activity.
    Status {
        /// Include archived tasks in diagnostic rows and counts.
        #[arg(long)]
        include_archived: bool,
    },
    /// Return owned task or atomically claim highest-priority ready task (oldest ID on ties).
    Next {
        #[command(flatten)]
        selectors: selection::Selectors,
        /// Select project saved view; owned task still returns first.
        #[arg(long, value_name = "NAME")]
        view: Option<String>,
        /// Wait until a task is available to claim or preview.
        #[arg(long)]
        wait: bool,
        /// Preview queued candidate without claiming or returning an owned task.
        #[arg(long)]
        dry_run: bool,
        /// Explain selection without claiming, dispatching or discovering a session.
        #[arg(long, conflicts_with_all = ["wait", "dry_run"])]
        explain: bool,
        /// Include archived diagnostic rows; archived tasks remain ineligible.
        #[arg(long, requires = "explain")]
        include_archived: bool,
        /// Hide archived diagnostic rows, overriding saved visibility.
        #[arg(long, requires = "explain", conflicts_with = "include_archived")]
        hide_archived: bool,
        /// Claim locally even when Herdr new-agent dispatch is configured.
        #[arg(long)]
        local: bool,
    },
    /// Save, browse, inspect or remove project-local named views.
    View {
        #[command(subcommand)]
        command: ViewCommand,
    },
    /// Mark task completed; supplied or discovered session ID must match recorded owner.
    Complete { id: i64 },
    /// Return completed/auto-failed task or active task with absent Herdr owner to new; keep history.
    Reopen {
        /// Task ID, or negative creation index: -1 is newest.
        #[arg(allow_negative_numbers = true)]
        id: i64,
    },
    /// Write consistent project snapshot with database and attachments.
    Backup { destination: PathBuf },
    /// Restore snapshot into new or empty project location.
    Restore {
        source: PathBuf,
        /// Recover original schema from an automatic pre-upgrade snapshot.
        #[arg(long)]
        recovery: bool,
    },
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
enum ViewCommand {
    /// Save or replace selection criteria and visibility under a name.
    Save {
        name: String,
        #[command(flatten)]
        selectors: selection::Selectors,
        #[arg(long)]
        include_archived: bool,
        #[arg(long, conflicts_with = "include_archived")]
        hide_archived: bool,
        #[arg(long, value_parser = clap::value_parser!(i64).range(0..))]
        max_completed: Option<i64>,
        #[arg(short = 'a', long, conflicts_with = "max_completed")]
        all: bool,
    },
    /// List saved definitions in name order.
    List,
    /// Inspect one saved definition.
    Show { name: String },
    /// Remove one saved definition.
    Remove { name: String },
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
fn resolved_owner(
    explicit: Option<&str>,
    project: &std::path::Path,
    db: &db::Db,
) -> Result<session::Owner> {
    session::owner(explicit, project, db).map_err(cli_error::session_error)
}
fn local_owner(cli: &Cli, project_dir: &std::path::Path, db: &db::Db) -> Result<session::Owner> {
    if let Some(session) = cli.session.as_deref() {
        return resolved_owner(Some(session), project_dir, db);
    }
    if let Some(session) = cli.harness_session.as_deref() {
        if db
            .owned_with_name(session, cli.harness_name.as_deref())?
            .is_some()
        {
            return resolved_owner(Some(session), project_dir, db);
        }
        // Override public session while retaining auto-fill; local explicit input
        // remains usable when Herdr is unavailable or ambiguous.
        return resolved_owner(None, project_dir, db)
            .or_else(|_| resolved_owner(Some(session), project_dir, db));
    }
    resolved_owner(None, project_dir, db)
}
fn execute(
    cli: Cli,
    selection: &selection::Prepared,
    view_context: Option<views::ViewContext>,
) -> Result<Value> {
    let filter = selection.filter.as_ref();
    if let Commands::Graph { id, options } = &cli.command {
        let (db, _) = db::Db::open_read_only()?;
        return Ok(json!(db.graph(*id, *options)?));
    }
    if let Commands::View { command } = &cli.command {
        let path = views::path()?;
        return Ok(match command {
            ViewCommand::Save {
                name,
                selectors,
                include_archived,
                max_completed,
                ..
            } => json!(views::save(
                &path,
                views::SavedView {
                    name: name.clone(),
                    criteria: selectors.clone(),
                    include_archived: *include_archived,
                    max_completed: *max_completed,
                }
            )?),
            ViewCommand::List => json!(views::load(&path)?.views),
            ViewCommand::Show { name } => json!(views::find(&views::load(&path)?, name)?),
            ViewCommand::Remove { name } => json!(views::remove(&path, name)?),
        });
    }
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
        )
        .map_err(cli_error::config_error);
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
    if let Commands::Bulk(args) = &cli.command {
        return if let Some(path) = &args.apply {
            let report = bulk::read(path)?;
            let (mut db, _) = db::Db::open(false)?;
            Ok(json!(bulk::apply(
                &mut db,
                &report,
                cli.session
                    .as_deref()
                    .or(cli.harness_session.as_deref())
                    .unwrap_or("cli")
            )?))
        } else {
            let actions = args.actions()?;
            let (db, _) = db::Db::open_read_only()?;
            Ok(json!(bulk::preview(&db, args.selection(filter), actions)?))
        };
    }
    if let Commands::Restore { source, recovery } = &cli.command {
        return snapshot::restore::run(source, *recovery);
    }
    if let Commands::Delete { id, yes: false } = &cli.command {
        return Ok(json!(delete::preview_cli(*id)?));
    }
    let stdin_description = if matches!(&cli.command, Commands::Add { stdin: true, .. }) {
        let mut description = String::new();
        std::io::stdin()
            .read_to_string(&mut description)
            .context("Failed to read stdin description as UTF-8")?;
        db::validate_description(&description)?;
        Some(description)
    } else {
        None
    };
    let import_batch = match &cli.command {
        Commands::Import { path, .. } => Some(import::read(path)?),
        Commands::Add {
            template: Some(name),
            vars,
            ..
        } => Some(recipe::read(name, vars)?),
        _ => None,
    };
    if matches!(
        &cli.command,
        Commands::Import { dry_run: true, .. }
            | Commands::Add {
                template: Some(_),
                dry_run: true,
                ..
            }
    ) {
        let (mut db, _) = db::Db::open_read_only()?;
        return Ok(json!(import::run(
            &mut db.conn,
            import_batch.as_ref().expect("import input prepared"),
            true
        )?));
    }
    if let Commands::Next {
        dry_run: true,
        wait,
        ..
    } = &cli.command
    {
        let (mut db, _) = db::Db::open_read_only()?;
        loop {
            let task = db.peek_next_filtered(filter)?;
            if task.is_some() || !wait {
                return Ok(json!(task));
            }
            thread::sleep(Duration::from_millis(250));
        }
    }
    let session_input = cli.session.as_deref().or(cli.harness_session.as_deref());
    let diagnostics = match &cli.command {
        Commands::Status { include_archived } => Some((*include_archived, false)),
        Commands::Next {
            explain: true,
            include_archived,
            ..
        } => Some((*include_archived, true)),
        _ => None,
    };
    if let Some((include_archived, explain)) = diagnostics {
        let (mut db, _) = db::Db::open_read_only()?;
        return Ok(json!(queue::report(
            &mut db.conn,
            queue::Options {
                include_archived: if explain {
                    selection.include_archived
                } else {
                    include_archived
                },
                filter,
                explain,
                owner: session_input,
                harness_name: cli.harness_name.as_deref(),
            },
        )?));
    }
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
            explain: false,
            ..
        } if *local
            || !config::load()
                .map_err(cli_error::config_error)?
                .herdr
                .next_to_new_agent =>
        {
            Some(local_owner(&cli, project_dir, &db)?)
        }
        _ => None,
    };
    Ok(match cli.command {
        Commands::Config { .. } => unreachable!("config was handled before database lookup"),
        Commands::View { .. } => unreachable!("views handled before database lookup"),
        Commands::Init => json!({"database":path}),
        Commands::Status { .. } => unreachable!("diagnostics handled before database writes"),
        Commands::Bulk(_) => unreachable!("bulk handled before writable database open"),
        Commands::Import { .. } => json!(import::run(
            &mut db.conn,
            import_batch.as_ref().expect("import input prepared"),
            false
        )?),
        Commands::Add {
            template: Some(_), ..
        } => json!(import::run(
            &mut db.conn,
            import_batch.as_ref().expect("recipe input prepared"),
            false
        )?),
        Commands::Add {
            text,
            description,
            edit,
            parent,
            priority,
            images,
            stdin: _,
            depends_on,
            tags,
            ..
        } => {
            let tags = tags::normalize(&tags)?;
            if let Some(id) = parent {
                db.task(id)?;
            }
            dependencies::validate_add(&db.conn, parent, &depends_on, true)?;
            let images = images
                .iter()
                .map(|path| images::ImageInput::read(path).map_err(cli_error::image_error))
                .collect::<Result<Vec<_>>>()?;
            match stdin_description.or(text).or(description) {
                Some(description) if !edit => {
                    json!(if tags.is_empty() {
                        db.add_with_dependencies(
                            &description,
                            parent,
                            &images,
                            priority,
                            &depends_on,
                        )?
                    } else {
                        db.add_with_options(
                            &description,
                            &images,
                            db::AddOptions {
                                parent,
                                priority,
                                prerequisites: &depends_on,
                                tags: &tags,
                            },
                        )?
                    })
                }
                None if editor::uses_builtin(edit) => {
                    let mut saved = Vec::new();
                    let mut first_images = images;
                    tui::compose_continuously(&mut db, &mut |db, mut outcome| {
                        outcome
                            .composition
                            .images
                            .extend(first_images.iter().cloned());
                        let task = match outcome.target_id {
                            Some(id) => db.edit_composition_guarded(
                                id,
                                &outcome.composition,
                                None,
                                None,
                                outcome.expected_revision,
                            ),
                            None => db.save_composition_with_options(
                                None,
                                &outcome.composition,
                                db::AddOptions {
                                    parent,
                                    priority,
                                    prerequisites: &depends_on,
                                    tags: &tags,
                                },
                            ),
                        }?;
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
                    json!(match outcome.target_id {
                        Some(id) => db.edit_composition_guarded(
                            id,
                            &outcome.composition,
                            None,
                            None,
                            outcome.expected_revision
                        )?,
                        None => db.save_composition_with_options(
                            None,
                            &outcome.composition,
                            db::AddOptions {
                                parent,
                                priority,
                                prerequisites: &depends_on,
                                tags: &tags
                            }
                        )?,
                    })
                }
            }
        }
        Commands::Tui {
            include_archived, ..
        } => {
            let settings = config::load_tui().map_err(cli_error::config_error)?;
            tui::compose_dashboard(
                &mut db,
                include_archived,
                settings.tui.after_save_new,
                view_context.expect("dashboard view context prepared"),
                &mut |db, outcome| {
                    let task = match outcome.target_id {
                        Some(id) => db.edit_composition_guarded(
                            id,
                            &outcome.composition,
                            None,
                            None,
                            outcome.expected_revision,
                        ),
                        None if outcome.tags.is_empty() => {
                            db.save_composition(None, outcome.parent_id, &outcome.composition)
                        }
                        None => db.save_composition_with_options(
                            None,
                            &outcome.composition,
                            db::AddOptions {
                                parent: outcome.parent_id,
                                tags: &outcome.tags,
                                ..Default::default()
                            },
                        ),
                    }?;
                    Ok(task.id)
                },
                &mut |db, id| {
                    let owner = match resolved_owner(session_input, project_dir, db) {
                        Ok(owner) => Some(owner),
                        Err(error) if tui::completion::discovery_unavailable(&error) => None,
                        Err(error) => return Err(error),
                    };
                    let owned = owner
                        .as_ref()
                        .map(|owner| {
                            db.owned_with_name(&owner.key, overrides.harness_name.as_deref())
                        })
                        .transpose()?
                        .flatten();
                    Ok(if owned.is_some_and(|task| task.id == id) {
                        tui::TaskAction::Complete(id)
                    } else {
                        tui::TaskAction::ForceComplete(id)
                    })
                },
                &mut |db, action| match action {
                    tui::TaskAction::Complete(id) => {
                        let owner = resolved_owner(session_input, project_dir, db)?;
                        db.complete(id, &owner.key, overrides.harness_name.as_deref())
                    }
                    tui::TaskAction::MarkError(id, reason) => {
                        let owner = resolved_owner(session_input, project_dir, db)?;
                        db.edit_with_priority(
                            id,
                            None,
                            Some(db::EditTransition::Error {
                                session: &owner.key,
                                reason: &reason,
                                harness_name: overrides.harness_name.as_deref(),
                            }),
                            &[],
                            None,
                            None,
                        )
                    }
                    tui::TaskAction::ForceComplete(id) => {
                        db.force_complete(id, session_input.unwrap_or("manual"))
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
                    tui::TaskAction::ForceReopen(id) => {
                        db.force_reopen(id, session_input.unwrap_or("manual"))
                    }
                    tui::TaskAction::SetArchived(id, archived) => {
                        db.set_archived(id, archived, session_input.unwrap_or("cli"))
                    }
                    tui::TaskAction::Tags(id, tags) => db.edit_guarded(
                        id,
                        None,
                        &[],
                        db::EditOptions {
                            tags: Some(&tags),
                            ..Default::default()
                        },
                    ),
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
        Commands::List { .. } => json!(selection::list(&db, selection, None)?),
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
            depends_on,
            remove_depends_on,
            clear_depends_on,
            set_tags,
        } => {
            let tags = set_tags.as_deref().map(tags::parse).transpose()?;
            ensure!(
                !force || matches!(set_status, Some(EditStatus::New)),
                errors::Info::invalid_argument(
                    "--force",
                    "--force is only valid with --set-status new"
                )
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
                .map(|path| images::ImageInput::read(path).map_err(cli_error::image_error))
                .collect::<Result<Vec<_>>>()?;
            if edit
                || (description.is_none()
                    && set_status.is_none()
                    && set_parent.is_none()
                    && priority.is_none()
                    && tags.is_none()
                    && depends_on.is_empty()
                    && remove_depends_on.is_empty()
                    && !clear_depends_on
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
                    let mut saved = None;
                    tui::compose_existing(
                        &mut db,
                        snapshot,
                        description.as_deref(),
                        &mut |db, mut outcome| {
                            outcome.composition.images.extend(images.iter().cloned());
                            let task = db.edit_composition_guarded(
                                id,
                                &outcome.composition,
                                set_parent,
                                priority,
                                outcome.expected_revision,
                            )?;
                            let id = task.id;
                            saved = Some(task);
                            Ok(id)
                        },
                    )?;
                    json!(saved.context("Editor cancelled; task not saved")?)
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
                    errors::Info::invalid_argument(
                        "--reason",
                        "--reason is only valid with --set-status error"
                    )
                );
                if matches!(set_status, Some(EditStatus::Error)) {
                    db::nonempty(
                        reason.as_deref().context(errors::Info::invalid_argument(
                            "--reason",
                            "--set-status error requires --reason",
                        ))?,
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
                        Some(resolved_owner(session_input, project_dir, &db)?.key)
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
                        tags: tags.as_deref(),
                        dependencies: dependencies::Changes {
                            add: &depends_on,
                            remove: &remove_depends_on,
                            clear: clear_depends_on
                        },
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
                        None => dispatch::next(&mut db, session_input, &overrides, filter)
                            .map_err(|error| {
                                cli_error::annotate(
                                    error,
                                    errors::Info::new(
                                        errors::Code::DispatchError,
                                        "Agent dispatch failed; inspect assignment before retrying",
                                    ),
                                )
                            })?,
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
            let owner = resolved_owner(session_input, project_dir, &db)?;
            json!(db.complete(id, &owner.key, overrides.harness_name.as_deref())?)
        }
        Commands::Reopen { id } => {
            let id = db.resolve_task_id(id)?;
            json!(db.reopen(id, session_input.unwrap_or("cli"))?)
        }
        Commands::Backup { destination } => snapshot::backup::run(&mut db, &path, &destination)?,
        Commands::Restore { .. } => unreachable!("restore handled before database open"),
        Commands::Doctor => unreachable!("doctor handled before database open"),
        Commands::Graph { .. } => unreachable!("graph handled before writable database open"),
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
                let link = db.link(task_id)?.context(
                    errors::Info::new(
                        errors::Code::InvalidTransition,
                        "Task has no Herdr link; run qqq herdr link",
                    )
                    .detail("task_id", task_id)
                    .detail("actual_link", false)
                    .detail("expected_link", true),
                )?;
                json!(herdr::find(&link.identity, link.server.as_deref())?)
            }
        },
    })
}
fn run(cli: Cli) -> Result<(Option<String>, bool)> {
    let archive = |include: bool, hide: bool| {
        if include {
            Some(true)
        } else if hide {
            Some(false)
        } else {
            None
        }
    };
    let inputs = match &cli.command {
        Commands::List {
            selectors,
            include_archived,
            hide_archived,
            max_completed,
            all,
            view,
            ..
        }
        | Commands::Tui {
            selectors,
            include_archived,
            hide_archived,
            max_completed,
            all,
            view,
            ..
        } => Some((
            selectors,
            view.as_deref(),
            selection::Visibility {
                include_archived: archive(*include_archived, *hide_archived),
                max_completed: if *all {
                    Some(None)
                } else {
                    max_completed.map(Some)
                },
                ..Default::default()
            },
        )),
        Commands::Next {
            selectors,
            include_archived,
            hide_archived,
            view,
            ..
        } => Some((
            selectors,
            view.as_deref(),
            selection::Visibility {
                include_archived: archive(*include_archived, *hide_archived),
                ..Default::default()
            },
        )),
        Commands::View {
            command: ViewCommand::Save { selectors, .. },
        } => Some((selectors, None, selection::Visibility::default())),
        _ => None,
    };
    // Validate explicit selectors before project discovery, catalog loading or preflight.
    let mut prepared = inputs
        .map(|(selectors, _, visibility)| selection::prepare(None, selectors, visibility))
        .transpose()?
        .unwrap_or_default();
    if let Commands::Bulk(args) = &cli.command {
        prepared = selection::prepare(
            None,
            &selection::Selectors {
                tags: args.tags.clone(),
                filter: args.filter.clone(),
                ..Default::default()
            },
            selection::Visibility::default(),
        )?;
    }
    let mut view_context = None;
    if let Some((selectors, name, visibility)) = inputs {
        if name.is_some() || matches!(&cli.command, Commands::Tui { .. }) {
            let path = views::path()?;
            let catalog = views::load(&path)?;
            let active = name.map(|name| views::find(&catalog, name)).transpose()?;
            let context = views::ViewContext::new(path, selectors.clone(), visibility, active)?;
            prepared = context.prepared.clone();
            view_context = Some(context);
        }
    }
    let is_tui = matches!(&cli.command, Commands::Tui { .. });
    let filtered_list = matches!(&cli.command, Commands::List { .. }) && prepared.filter.is_some();
    let display_limit = match &cli.command {
        Commands::List {
            all: false,
            max_completed: None,
            view: None,
            ..
        } if !cli.json => config::load_display()
            .map_err(cli_error::config_error)?
            .display
            .max_completed
            .map(|limit| i64::try_from(limit).context("display.max-completed is too large"))
            .transpose()?,
        _ => None,
    };
    if matches!(&cli.command, Commands::List { .. }) && prepared.max_completed.is_none() {
        prepared.max_completed = display_limit;
    }
    if let Commands::List {
        watch: true,
        oneline,
        ..
    } = &cli.command
    {
        preflight::current_project()?;
        watch::run(watch::WatchOptions {
            json_output: cli.json,
            max_completed: prepared.max_completed,
            include_archived: prepared.include_archived,
            oneline: *oneline,
            display_limited: display_limit.is_some(),
            query: None,
            statuses: &[],
            filter: prepared.filter.as_ref(),
        })?;
        return Ok((None, false));
    }
    let json = cli.json;
    // Init/restore target current directory, rather than nearest parent project.
    // Init's DB open checks its own active owners; restore has no live target DB.
    if !matches!(
        &cli.command,
        Commands::Init | Commands::Restore { .. } | Commands::View { .. } | Commands::Graph { .. }
    ) {
        preflight::current_project()?;
    }
    let is_doctor = matches!(&cli.command, Commands::Doctor);
    let format = output::Format::from(&cli.command);
    let value = execute(cli, &prepared, view_context)?;
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
                    | output::Format::Graph
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
    let arguments: Vec<_> = std::env::args_os().collect();
    let mut agent_caller = None;
    let arguments = match aliases::expand(arguments, &mut agent_caller) {
        Ok(arguments) => arguments,
        Err(error) => {
            let error = cli_error::annotate(
                error,
                errors::Info::invalid_argument("alias", "Invalid command alias"),
            );
            if errors::json_output() {
                cli_error::emit(&cli_error::payload(&error, None));
            } else {
                eprintln!("error: {error:#}");
            }
            std::process::exit(1);
        }
    };
    let requested_json = cli_error::requests_json(&arguments, &mut agent_caller);
    let mut cli = match Cli::try_parse_from(arguments) {
        Ok(cli) => cli,
        Err(error) if requested_json && error.use_stderr() => {
            cli_error::emit(&cli_error::parser_payload(&error));
            std::process::exit(error.exit_code());
        }
        Err(error) => error.exit(),
    };
    cli.json = cli.json || (requested_json && !cli.human);
    let json_output = cli.json;
    errors::set_json_output(json_output);
    let command = cli_error::command_name(&cli.command);
    match run(cli) {
        Ok((Some(output), unhealthy)) => {
            println!("{output}");
            if unhealthy {
                std::process::exit(1);
            }
        }
        Ok((None, _)) => (),
        Err(error) => {
            if json_output {
                cli_error::emit(&cli_error::payload(&error, Some(command)));
            } else {
                eprintln!("error: {error:#}");
            }
            std::process::exit(1);
        }
    }
}
