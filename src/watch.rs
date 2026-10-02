use crate::{
    db::Db,
    output::{self, Format},
};
use anyhow::Result;
use serde_json::json;
use std::{
    io::{self, IsTerminal, Write},
    thread,
    time::Duration,
};

pub fn run(json_output: bool, max_completed: Option<i64>, display_limited: bool) -> Result<()> {
    let (db, _) = Db::open(false)?;
    let stdout = io::stdout();
    let terminal =
        stdout.is_terminal() && std::env::var_os("TERM").is_none_or(|term| term != "dumb");
    let color = output::color_enabled(terminal);
    let mut stdout = stdout.lock();
    // Read the counter before the first list, so a concurrent commit cannot be missed.
    let mut version = db.data_version()?;
    loop {
        let tasks = json!(db.list(max_completed)?);
        let snapshot = if json_output {
            serde_json::to_string(&tasks)?
        } else if display_limited && tasks.as_array().is_some_and(Vec::is_empty) {
            "No tasks to display.".to_owned()
        } else {
            output::render(Format::Tasks, &tasks, color, None)
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
