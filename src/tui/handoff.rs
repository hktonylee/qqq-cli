use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    io,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn project_root(db: &crate::db::Db) -> Result<PathBuf> {
    let path = db.conn.path().context("Database has no project path")?;
    std::path::Path::new(path)
        .parent()
        .and_then(|directory| directory.parent())
        .context("Database path has no project directory")?
        .canonicalize()
        .context("Cannot resolve queue project directory")
}

fn server_running() -> Result<bool> {
    #[derive(Deserialize)]
    struct Status {
        status: String,
    }
    let output = Command::new("herdr")
        .args(["status", "server", "--json"])
        .output()
        .context("Cannot check Herdr server")?;
    ensure!(
        output.status.success(),
        "Cannot check Herdr server: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let status: Status =
        serde_json::from_slice(&output.stdout).context("Invalid Herdr server status")?;
    ensure!(
        matches!(status.status.as_str(), "running" | "not_running"),
        "Unknown Herdr server status: {}",
        status.status
    );
    Ok(status.status == "running")
}

fn ensure_server() -> Result<()> {
    if server_running()? {
        return Ok(());
    }
    let mut server = Command::new("herdr")
        .arg("server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Cannot start Herdr server")?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if server_running()? {
            return Ok(());
        }
        if let Some(status) = server.try_wait()? {
            ensure!(status.success(), "Herdr server startup failed: {status}");
        }
        ensure!(
            Instant::now() < deadline,
            "Herdr server did not become ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(super) fn create(db: &crate::db::Db, prompt: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct Panes {
        panes: Vec<crate::herdr::Pane>,
    }
    #[derive(Deserialize)]
    struct Created {
        root_pane: crate::herdr::Pane,
    }
    #[derive(Deserialize)]
    struct Agent {
        agent: crate::herdr::Pane,
    }
    crate::db::nonempty(prompt, "Prompt")?;
    let root = project_root(db)?;
    let cwd = root.to_str().context("Project directory is not UTF-8")?;
    ensure_server()?;
    let panes: Panes = crate::herdr::call(None, &["pane", "list"])?;
    let workspace = panes.panes.iter().find(|pane| {
        pane.cwd
            .as_deref()
            .is_some_and(|cwd| cwd.is_absolute() && cwd.canonicalize().is_ok_and(|cwd| cwd == root))
    });
    let created: Created = match workspace {
        Some(pane) => crate::herdr::call(
            None,
            &[
                "tab",
                "create",
                "--workspace",
                &pane.workspace_id,
                "--cwd",
                cwd,
                "--no-focus",
            ],
        )?,
        None => crate::herdr::call(
            None,
            &[
                "workspace",
                "create",
                "--cwd",
                cwd,
                "--label",
                root.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(cwd),
                "--no-focus",
            ],
        )?,
    };
    let pane = created.root_pane;
    crate::db::nonempty(&pane.pane_id, "Created pane ID")?;
    let prepare = || -> Result<()> {
        if let Some(workspace) = workspace {
            ensure!(
                pane.workspace_id == workspace.workspace_id,
                "Created tab is in a different workspace"
            );
        }
        let name = format!("qqq-worker-{}", pane.pane_id.replace(':', "-"));
        let started: Result<serde_json::Value> = crate::herdr::call(
            None,
            &[
                "agent",
                "start",
                &name,
                "--kind",
                "codex",
                "--pane",
                &pane.pane_id,
            ],
        );
        if let Err(error) = started {
            // Some interactive shell profiles start Codex automatically.
            let current: Result<Agent> = crate::herdr::call(None, &["agent", "get", &pane.pane_id]);
            let already_started = current.is_ok_and(|current| {
                current.agent.pane_id == pane.pane_id
                    && (current.agent.agent.as_deref() == Some("codex")
                        || current
                            .agent
                            .agent_session
                            .as_ref()
                            .is_some_and(|identity| identity.agent == "codex"))
            });
            if !already_started {
                return Err(error);
            }
            let _: serde_json::Value = crate::herdr::call(
                None,
                &[
                    "agent",
                    "wait",
                    &pane.pane_id,
                    "--until",
                    "idle",
                    "--timeout",
                    "30000",
                ],
            )?;
        }
        let _: serde_json::Value =
            crate::herdr::call(None, &["agent", "prompt", &pane.pane_id, prompt])
                .context("Prompt may have been delivered; inspect the agent before trying again")?;
        let _: serde_json::Value = crate::herdr::call(None, &["agent", "focus", &pane.pane_id])?;
        Ok(())
    };
    prepare().with_context(|| {
        format!(
            "Created Herdr pane {}; inspect it before retrying",
            pane.pane_id
        )
    })?;
    Ok(pane.pane_id)
}

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
