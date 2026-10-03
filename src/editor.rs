mod recovery;
pub use recovery::{ExternalEdit, edit_external};

use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::IsTerminal,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub fn uses_builtin(external: bool) -> bool {
    !external && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

pub fn compose(
    description: &str,
    external: bool,
    navigation: Option<&crate::db::Db>,
) -> Result<crate::tui::Outcome> {
    if uses_builtin(external) {
        return crate::tui::compose(description, navigation);
    }
    compose_external_outcome(description)
}

fn compose_external_outcome(description: &str) -> Result<crate::tui::Outcome> {
    let description = compose_external(description)?;
    Ok(crate::tui::Outcome {
        composition: crate::tui::draft::Composition {
            description,
            images: Vec::new(),
            image_spans: Vec::new(),
        },
        target_id: None,
        parent_id: None,
        expected_revision: None,
    })
}
fn compose_external(description: &str) -> Result<String> {
    ExternalDraft::new(description)?.edit()
}

struct ExternalDraft {
    directory: PathBuf,
    temporary: Option<tempfile::TempDir>,
    path: PathBuf,
}

impl ExternalDraft {
    fn new(description: &str) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("qqq-edit-")
            .tempdir()
            .context("Failed to create task draft directory")?;
        let path = directory.path().join("task.txt");
        fs::write(&path, description).context("Failed to write task draft")?;
        Ok(Self {
            directory: directory.path().to_owned(),
            temporary: Some(directory),
            path,
        })
    }

    fn edit(&self) -> Result<String> {
        run_external_editor(&self.path)
    }

    fn directory(&self) -> &Path {
        &self.directory
    }

    fn keep(&mut self) {
        if let Some(directory) = self.temporary.take() {
            let _path = directory.keep();
        }
    }

    fn cleanup(&self) {
        if self.temporary.is_none() {
            if let Err(error) = fs::remove_dir_all(&self.directory) {
                eprintln!(
                    "Saved task; cannot remove recovery directory {}: {error}",
                    self.directory.display()
                );
            }
        }
    }
}

fn run_external_editor(draft: &Path) -> Result<String> {
    let editor = std::env::var("EDITOR")
        .context("Set EDITOR to compose or edit a task, or provide fields inline")?;
    ensure!(
        !editor.trim().is_empty(),
        "Set EDITOR to compose or edit a task, or provide fields inline"
    );
    // EDITOR is a user-configured shell command, allowing flags and quoted paths.
    // Pass the draft separately so its path is never interpreted as shell code.
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("exec {editor} \"$1\""))
        .arg("qqq-editor")
        .arg(draft)
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
    let content = fs::read_to_string(draft).context("Failed to read edited task")?;
    ensure!(
        !content.trim().is_empty(),
        "Task description cannot be empty; task not saved"
    );
    Ok(content)
}
