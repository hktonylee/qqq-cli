use super::ExternalDraft;
use crate::db::{ContentConflict, ContentSnapshot, Db, ParentChange, Task};
use crate::images::ImageInput;
use crate::tui::draft::Composition;
use anyhow::{Context, Result};
use serde_json::json;
use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::Path,
};

pub struct ExternalEdit<'a> {
    pub description: Option<&'a str>,
    pub images: &'a [ImageInput],
    pub parent: Option<ParentChange>,
    pub priority: Option<i64>,
}

fn input(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_ascii_lowercase())
}

fn confirm(prompt: &str) -> Result<bool> {
    Ok(matches!(input(prompt)?.as_str(), "y" | "yes"))
}

fn recovery_files(
    draft: &ExternalDraft,
    composition: &Composition,
    current: Option<&str>,
) -> Result<()> {
    if let Some(current) = current {
        fs::write(draft.directory().join("current.txt"), current)?;
    } else {
        let path = draft.directory().join("current.txt");
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    let mut attachments = Vec::new();
    for (index, image) in composition.images.iter().enumerate() {
        let directory = draft.directory().join(format!("image-{}", index + 1));
        fs::create_dir_all(&directory)?;
        let name = Path::new(&image.name)
            .file_name()
            .context("Invalid pending image name")?;
        let path = directory.join(name);
        fs::write(&path, &image.data)?;
        attachments.push(json!({"name": image.name, "path": path}));
    }
    fs::write(
        draft.directory().join("attachments.json"),
        serde_json::to_vec_pretty(&attachments)?,
    )?;
    Ok(())
}

fn recovery(draft: &ExternalDraft) -> crate::errors::Recovery {
    let current = draft.directory().join("current.txt");
    let attachments = draft.directory().join("attachments.json");
    crate::errors::Recovery {
        local_draft: draft.path.clone(),
        current_text: current.exists().then_some(current),
        attachments_manifest: attachments.exists().then_some(attachments),
    }
}

pub fn edit_external(
    db: &mut Db,
    snapshot: ContentSnapshot,
    request: ExternalEdit<'_>,
) -> Result<Task> {
    let id = snapshot.task.id;
    let mut expected_revision = snapshot.task.content_revision;
    let mut draft = ExternalDraft::new(request.description.unwrap_or(&snapshot.task.description))?;
    let mut composition = Composition {
        description: draft.edit()?,
        images: request.images.to_vec(),
        image_spans: Vec::new(),
    };
    loop {
        match db.edit_composition_guarded(
            id,
            &composition,
            request.parent,
            request.priority,
            Some(expected_revision),
        ) {
            Ok(task) => {
                draft.cleanup();
                return Ok(task);
            }
            Err(error) => {
                draft.keep();
                let conflict = error.downcast_ref::<ContentConflict>();
                let current = conflict.and_then(|conflict| conflict.current.as_ref());
                recovery_files(
                    &draft,
                    &composition,
                    current.map(|current| current.description.as_str()),
                )
                .context("Failed to preserve editor recovery files")
                .with_context(|| recovery(&draft))?;
                if !crate::errors::json_output() || io::stdin().is_terminal() {
                    eprintln!("{error:#}\nLocal draft: {}", draft.path.display());
                    if let Some(current) = current {
                        eprintln!(
                            "Current DB text (revision {}): {}",
                            current.revision,
                            draft.directory().join("current.txt").display()
                        );
                    }
                    if !composition.images.is_empty() {
                        eprintln!(
                            "Pending image bytes: {}",
                            draft.directory().join("attachments.json").display()
                        );
                    }
                }
                if conflict.is_none() || current.is_none() || !io::stdin().is_terminal() {
                    return Err(error.context(recovery(&draft)));
                }
                let current = current.expect("checked current content");
                loop {
                    match input(
                        "[r] Reload DB text  [o] Overwrite DB text  [k] Keep draft and exit: ",
                    )
                    .with_context(|| recovery(&draft))?
                    .as_str()
                    {
                        "r" => {
                            if !confirm(
                                "Discard local text and pending images, then reload? (y/N): ",
                            )
                            .with_context(|| recovery(&draft))?
                            {
                                continue;
                            }
                            let snapshot = match db.content_snapshot(id) {
                                Ok(snapshot) => snapshot,
                                Err(error) => {
                                    return Err(error.context(recovery(&draft)));
                                }
                            };
                            expected_revision = snapshot.task.content_revision;
                            fs::write(&draft.path, &snapshot.task.description)
                                .with_context(|| recovery(&draft))?;
                            composition.description =
                                draft.edit().with_context(|| recovery(&draft))?;
                            composition.images.clear();
                            break;
                        }
                        "o" => {
                            if !confirm(&format!(
                                "Replace DB text at revision {} with local draft and pending images? (y/N): ",
                                current.revision
                            )).with_context(|| recovery(&draft))? {
                                continue;
                            }
                            expected_revision = current.revision;
                            break;
                        }
                        "k" | "" => {
                            return Err(error.context(recovery(&draft)));
                        }
                        _ => eprintln!("Choose r, o, or k."),
                    }
                }
            }
        }
    }
}
