use crate::{
    db::{Db, Task, nonempty},
    herdr::{self, Link, Pane},
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Created {
    root_pane: Pane,
}
#[derive(Deserialize)]
struct Agent {
    agent: Pane,
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn next(db: &mut Db, caller: Option<&str>) -> Result<Option<Task>> {
    let caller = match caller {
        Some(session) => {
            nonempty(session, "Session")?;
            session.to_owned()
        }
        None => herdr::dispatch_caller()?,
    };
    if let Some(task) = db.owned(&caller)? {
        return Ok(Some(task));
    }
    if !db.has_ready()? {
        return Ok(None);
    }
    ensure!(
        std::env::var("HERDR_ENV").as_deref() == Ok("1"),
        "Herdr dispatch requires HERDR_ENV=1; use next --local outside Herdr"
    );
    let workspace = std::env::var("HERDR_WORKSPACE_ID")
        .context("HERDR_WORKSPACE_ID missing; use next --local")?;
    nonempty(&workspace, "HERDR_WORKSPACE_ID")?;
    let cwd = std::env::current_dir()?.canonicalize()?;
    let cwd = cwd
        .to_str()
        .context("Herdr dispatch requires a UTF-8 working directory")?;
    let executable = std::env::current_exe()?;
    let executable = executable
        .to_str()
        .context("qqq executable path is not UTF-8")?;
    let unique = tempfile::Builder::new().prefix("qqq-dispatch-").tempdir()?;
    let name = unique
        .path()
        .file_name()
        .context("Missing dispatch name")?
        .to_string_lossy()
        .to_ascii_lowercase();
    let name = format!("{name}-{:x}", std::process::id());
    let Some(task) = db.next(&name, None)? else {
        return Ok(None);
    };
    let mut location = "before tab creation".to_owned();
    let prepared = (|| -> Result<Pane> {
        let created: Created = herdr::call(
            None,
            &[
                "tab",
                "create",
                "--workspace",
                &workspace,
                "--cwd",
                cwd,
                "--no-focus",
                "--env",
                &format!("QQQ_SESSION={name}"),
            ],
        )?;
        let pane = created.root_pane;
        location = format!("tab {}, pane {}", pane.tab_id, pane.pane_id);
        nonempty(&pane.pane_id, "Created pane ID")?;
        nonempty(&pane.tab_id, "Created tab ID")?;
        ensure!(
            pane.workspace_id == workspace,
            "Created tab belongs to a different workspace"
        );
        let _: Value = herdr::call(
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
        )?;
        let current: Agent = herdr::call(None, &["agent", "get", &pane.pane_id])?;
        ensure!(
            current.agent.pane_id == pane.pane_id,
            "Herdr returned a different agent pane"
        );
        let identity = herdr::dispatch_identity(&current.agent)?;
        ensure!(identity.agent == "codex", "Started agent is not Codex");
        db.set_link(
            task.id,
            &Link {
                server: None,
                identity,
                pane: current.agent.clone(),
            },
        )?;
        Ok(current.agent)
    })();
    let pane = match prepared {
        Ok(pane) => pane,
        Err(error) => {
            db.edit(task.id, None, None, Some(&name)).with_context(|| {
                format!(
                    "Dispatch failed ({error:#}); could not release task {} assigned to {name}",
                    task.id
                )
            })?;
            return Err(error).with_context(|| format!("Task {} returned to pending; dispatch failed at {location}. Any created tab remains for inspection", task.id));
        }
    };
    let prompt = format!(
        "Work on qqq task #{id} in {cwd}. Task is already assigned to session {session}. Do not call qqq next or dispatch another agent. Run {bin} show {id} --json to read task, messages and attachments. Complete requested work, verify changes, record progress using {bin} message {id} '<progress>' --session {session}, then run {bin} complete {id} --session {session} only when finished. If blocked, leave task claimed and record blocker. Use next --local only if you need to retrieve this existing claim.",
        id = task.id,
        cwd = quote(cwd),
        session = quote(&name),
        bin = quote(executable)
    );
    let _: Value = herdr::call(None, &["agent", "prompt", &pane.pane_id, &prompt])
        .with_context(|| format!("Prompt may have been delivered. Task {} remains assigned to {name}, linked to {location}; inspect agent before retrying. Recovery: qqq edit {} --set-status pending --session {}", task.id, task.id, quote(&name)))?;
    Ok(Some(task))
}
