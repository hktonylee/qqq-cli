use crate::{
    db::Db,
    list_filter::{self, ListStatus},
    output::{self, Format},
    sql_filter::CompiledFilter,
};
use anyhow::Result;
use serde_json::json;
use std::{
    io::{self, IsTerminal, Write},
    thread,
    time::Duration,
};

pub struct WatchOptions<'a> {
    pub json_output: bool,
    pub max_completed: Option<i64>,
    pub include_archived: bool,
    pub oneline: bool,
    pub display_limited: bool,
    pub query: Option<&'a str>,
    pub statuses: &'a [ListStatus],
    pub filter: Option<&'a CompiledFilter>,
}
pub fn run(options: WatchOptions<'_>) -> Result<()> {
    let WatchOptions {
        json_output,
        max_completed,
        include_archived,
        oneline,
        display_limited,
        query,
        statuses,
        filter,
    } = options;
    let (db, _) = Db::open(false)?;
    let stdout = io::stdout();
    let stdout_terminal = stdout.is_terminal();
    let terminal = stdout_terminal && std::env::var_os("TERM").is_none_or(|term| term != "dumb");
    let color = output::color_enabled(terminal);
    let filtered = query.is_some() || !statuses.is_empty() || filter.is_some();
    let mut stdout = stdout.lock();
    // Read the counter before the first list, so a concurrent commit cannot be missed.
    let mut version = db.data_version()?;
    loop {
        let (tasks, matches) = db.list_filtered(max_completed, include_archived, filter)?;
        let tasks = json!(list_filter::filter_tasks(
            tasks,
            query,
            statuses,
            matches.as_ref()
        ));
        let snapshot = if json_output {
            serde_json::to_string(&tasks)?
        } else if filtered && tasks.as_array().is_some_and(Vec::is_empty) {
            "No matching tasks.".to_owned()
        } else if display_limited && tasks.as_array().is_some_and(Vec::is_empty) {
            "No tasks to display.".to_owned()
        } else {
            output::render(
                if oneline {
                    Format::OnelinePriorityTasks
                } else {
                    Format::PriorityTasks
                },
                &tasks,
                color,
                output::terminal_columns(stdout_terminal),
            )
        };
        let written = (|| -> io::Result<()> {
            if terminal && !json_output {
                write!(stdout, "\x1b[H\x1b[2J")?;
            }
            writeln!(stdout, "{snapshot}")?;
            stdout.flush()
        })();
        if let Err(error) = written {
            if error.kind() == io::ErrorKind::BrokenPipe {
                return Ok(());
            }
            return Err(error.into());
        }
        loop {
            thread::sleep(Duration::from_millis(250));
            let latest = db.data_version()?;
            if latest != version {
                version = latest;
                break;
            }
        }
    }
}
