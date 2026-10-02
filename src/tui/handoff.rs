use anyhow::{Context, Result, ensure};
use std::{io, process::Command};

pub(super) fn focus(db: &crate::db::Db, id: i64) -> Result<Option<String>> {
    let link = db
        .link(id)?
        .context("Task has no Herdr link; run qqq herdr link")?;
    let pane = crate::herdr::find(&link.identity, link.server.as_deref())?;
    let _: serde_json::Value =
        crate::herdr::call(link.server.as_deref(), &["agent", "focus", &pane.pane_id])?;
    Ok(link.server)
}

pub(super) fn attach(server: Option<&str>) -> Result<()> {
    let mut command = Command::new("herdr");
    if let Some(server) = server {
        crate::db::nonempty(server, "Herdr server")?;
        command.args(["--session", server]);
    }
    let status = command
        .stdout(io::stderr())
        .status()
        .context("Cannot open Herdr client")?;
    ensure!(status.success(), "Herdr client failed: {status}");
    Ok(())
}
