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

pub(super) fn create(db: &mut crate::db::Db, id: i64, prompt: &str) -> Result<String> {
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
    #[derive(Deserialize)]
    struct Current {
        pane: crate::herdr::Pane,
    }
    crate::db::nonempty(prompt, "Prompt")?;
    db.task(id)?;
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
    let mut prepare = || -> Result<()> {
        if let Some(workspace) = workspace {
            ensure!(
                pane.workspace_id == workspace.workspace_id,
                "Created tab is in a different workspace"
            );
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let current: Current = crate::herdr::call(None, &["pane", "get", &pane.pane_id])?;
            ensure!(
                current.pane.pane_id == pane.pane_id,
                "Herdr returned a different pane"
            );
            if crate::herdr::pane_identity(&current.pane)
                .is_ok_and(|identity| identity.agent == "codex")
            {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "Created pane did not start Codex within 30 seconds"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        let timeout = deadline
            .saturating_duration_since(Instant::now())
            .as_millis()
            .max(1)
            .to_string();
        let _: serde_json::Value = crate::herdr::call(
            None,
            &[
                "agent",
                "wait",
                &pane.pane_id,
                "--until",
                "idle",
                "--timeout",
                &timeout,
            ],
        )?;
        let current: Agent = crate::herdr::call(None, &["agent", "get", &pane.pane_id])?;
        ensure!(
            current.agent.pane_id == pane.pane_id,
            "Herdr returned a different agent pane"
        );
        let identity = crate::herdr::pane_identity(&current.agent)?;
        ensure!(
            identity.agent == "codex",
            "Created pane did not start Codex"
        );
        let link = crate::herdr::Link {
            server: crate::herdr::server_session(),
            identity,
            pane: current.agent,
        };
        db.set_created_agent_link(id, &link)?;
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
