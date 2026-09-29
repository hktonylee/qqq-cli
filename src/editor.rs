use anyhow::{Context, Result, ensure};
use std::{
    fs,
    process::{Command, Stdio},
};

pub fn compose(title: &str, description: &str) -> Result<(String, String)> {
    let editor = std::env::var("EDITOR")
        .context("Set EDITOR to compose or edit a task, or provide fields inline")?;
    ensure!(
        !editor.trim().is_empty(),
        "Set EDITOR to compose or edit a task, or provide fields inline"
    );
    let draft_dir = tempfile::Builder::new()
        .prefix("qqq-edit-")
        .tempdir()
        .context("Failed to create task draft directory")?;
    let draft = draft_dir.path().join("task.txt");
    fs::write(&draft, format!("{title}\n\n{description}\n"))
        .context("Failed to write task draft")?;

    // EDITOR is a user-configured shell command, allowing flags and quoted paths.
    // Pass the draft separately so its path is never interpreted as shell code.
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("exec {editor} \"$1\""))
        .arg("qqq-editor")
        .arg(&draft)
        .stdin(Stdio::inherit())
        .stdout(Stdio::from(std::io::stderr()))
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to start EDITOR")?;
    ensure!(
        status.success(),
        "Editor exited unsuccessfully ({status}); task not saved"
    );
    // Read the path again: editors may replace the file on save.
    let content = fs::read_to_string(&draft).context("Failed to read edited task")?;
    let content = content.replace("\r\n", "\n");
    let (title, description) = content.split_once('\n').unwrap_or((&content, ""));
    ensure!(
        !title.trim().is_empty(),
        "Task title cannot be empty; task not saved"
    );
    Ok((title.trim().to_owned(), description.trim().to_owned()))
}
