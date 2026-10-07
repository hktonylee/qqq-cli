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
        tags: Vec::new(),
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
    let editor = std::env::var("EDITOR").context(crate::errors::Info::new(
        crate::errors::Code::EditorError,
        "Set EDITOR to compose or edit a task, or provide fields inline",
    ))?;
    ensure!(
        !editor.trim().is_empty(),
        crate::errors::Info::new(
            crate::errors::Code::EditorError,
            "Set EDITOR to compose or edit a task, or provide fields inline"
        )
    );
    // EDITOR is a user-configured shell command, allowing flags and quoted paths.
    // Pass the draft separately so its path is never interpreted as shell code.
    let quiet = crate::errors::json_output() && !std::io::stdin().is_terminal();
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("exec {editor} \"$1\""))
        .arg("qqq-editor")
        .arg(draft)
        .stdin(Stdio::inherit())
        .stdout(if quiet {
            Stdio::null()
        } else {
            Stdio::from(std::io::stderr())
        })
        .stderr(if quiet {
            Stdio::null()
        } else {
            Stdio::inherit()
        })
        .status()
        .context(crate::errors::Info::new(
            crate::errors::Code::EditorError,
            "Failed to start EDITOR",
        ))?;
    ensure!(
        status.success(),
        crate::errors::Info::new(
            crate::errors::Code::EditorError,
            format!("Editor exited unsuccessfully ({status}); task not saved")
        )
        .detail("exit_code", status.code())
    );
    // Read the path again: editors may replace the file on save.
    let content = fs::read_to_string(draft).context(
        crate::errors::Info::new(crate::errors::Code::IoError, "Failed to read edited task")
            .detail("reason", "edited_draft_unreadable"),
    )?;
    ensure!(
        !content.trim().is_empty(),
        crate::errors::Info::invalid_argument(
            "description",
            "Task description cannot be empty; task not saved"
        )
    );
    Ok(content)
}
