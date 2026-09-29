use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::IsTerminal,
    process::{Command, Stdio},
};

pub fn compose(description: &str, external: bool) -> Result<crate::tui::draft::Composition> {
    if !external && std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        return crate::tui::compose(description);
    }
    let description = compose_external(description)?;
    Ok(crate::tui::draft::Composition {
        description,
        images: Vec::new(),
    })
}
fn compose_external(description: &str) -> Result<String> {
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
    fs::write(&draft, description).context("Failed to write task draft")?;

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
    ensure!(
        !content.trim().is_empty(),
        "Task description cannot be empty; task not saved"
    );
    Ok(content)
}
