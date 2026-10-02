use crate::{
    db::Db,
    list_filter::{self, ListStatus},
    output::{self, Format},
};
use anyhow::Result;
use serde_json::json;
use std::{
    io::{self, IsTerminal, Write},
    thread,
    time::Duration,
};

pub fn run(
    json_output: bool,
    max_completed: Option<i64>,
    include_archived: bool,
    display_limited: bool,
    query: Option<&str>,
    statuses: &[ListStatus],
) -> Result<()> {
    let (db, _) = Db::open(false)?;
    let stdout = io::stdout();
    let stdout_terminal = stdout.is_terminal();
    let terminal = stdout_terminal && std::env::var_os("TERM").is_none_or(|term| term != "dumb");
    let color = output::color_enabled(terminal);
    let filtered = query.is_some() || !statuses.is_empty();
    let mut stdout = stdout.lock();
    // Read the counter before the first list, so a concurrent commit cannot be missed.
    let mut version = db.data_version()?;
    loop {
        let tasks = json!(list_filter::filter_tasks(
            db.list_with_archived(max_completed, include_archived)?,
            query,
            statuses
        ));
        let snapshot = if json_output {
            serde_json::to_string(&tasks)?
        } else if filtered && tasks.as_array().is_some_and(Vec::is_empty) {
            "No matching tasks.".to_owned()
        } else if display_limited && tasks.as_array().is_some_and(Vec::is_empty) {
            "No tasks to display.".to_owned()
        } else {
            output::render(
                Format::PriorityTasks,
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
