use anyhow::{Context, Result, bail, ensure};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

pub struct ImageStore {
    root: PathBuf,
}

pub struct PendingFiles {
    created: Vec<PathBuf>,
    active: bool,
}

impl PendingFiles {
    pub fn new() -> Self {
        Self {
            created: Vec::new(),
            active: true,
        }
    }

    pub fn keep(&mut self) {
        self.active = false;
    }
}

impl Drop for PendingFiles {
    fn drop(&mut self) {
        if self.active {
            for path in self.created.iter().rev() {
                let _ = fs::remove_file(path);
            }
        }
    }
}

impl ImageStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn path(&self, task_id: i64, image_id: i64, media_type: &str) -> Result<PathBuf> {
        ensure!(task_id > 0 && image_id > 0, "Invalid image identity");
        let ext = match media_type {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            _ => bail!("Unsupported stored image media type: {media_type}"),
        };
        Ok(self
            .root
            .join(task_id.to_string())
            .join(format!("{image_id}.{ext}")))
    }

    pub fn write(
        &self,
        pending: &mut PendingFiles,
        task_id: i64,
        image_id: i64,
        media_type: &str,
        data: &[u8],
    ) -> Result<()> {
        let path = self.path(task_id, image_id, media_type)?;
        let directory = path.parent().context("Missing image directory")?;
        fs::create_dir_all(directory)
            .with_context(|| format!("Cannot create {}", directory.display()))?;
        if let Some(parent) = self.root.parent() {
            sync_directory(parent)?;
        }
        sync_directory(&self.root)?;
        let mut temporary = NamedTempFile::new_in(directory)?;
        temporary.write_all(data)?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(&path) {
            Ok(_) => {
                pending.created.push(path.clone());
                sync_directory(directory)?;
                Ok(())
            }
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = self
                    .read(task_id, image_id, media_type, data.len() as i64)
                    .context("Stored image path conflicts with existing bytes")?;
                ensure!(
                    existing == data,
                    "Stored image path conflicts with existing bytes"
                );
                Ok(())
            }
            Err(error) => {
                Err(error.error).with_context(|| format!("Cannot store {}", path.display()))
            }
        }
    }

    pub fn read(
        &self,
        task_id: i64,
        image_id: i64,
        media_type: &str,
        expected_bytes: i64,
    ) -> Result<Vec<u8>> {
        let path = self.path(task_id, image_id, media_type)?;
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("Cannot read stored image {}", path.display()))?;
        ensure!(
            metadata.file_type().is_file(),
            "Stored image is not a regular file"
        );
        let data = fs::read(&path)
            .with_context(|| format!("Cannot read stored image {}", path.display()))?;
        ensure!(
            expected_bytes >= 0 && data.len() as i64 == expected_bytes,
            "Stored image byte count differs from database"
        );
        Ok(data)
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ImageInput {
    pub name: String,
    pub data: Vec<u8>,
}

impl ImageInput {
    pub fn read(path: &Path) -> Result<Self> {
        // Reject directories and FIFOs before opening; opening a FIFO can block.
        let metadata =
            std::fs::metadata(path).with_context(|| format!("Cannot read {}", path.display()))?;
        ensure!(metadata.is_file(), "Image must be a regular file");
        let file =
            std::fs::File::open(path).with_context(|| format!("Cannot read {}", path.display()))?;
        ensure!(file.metadata()?.is_file(), "Image must be a regular file");
        let mut data = Vec::new();
        file.take(MAX_IMAGE_BYTES as u64 + 1)
            .read_to_end(&mut data)?;
        let input = Self {
            name: path
                .file_name()
                .context("Missing image filename")?
                .to_string_lossy()
                .into_owned(),
            data,
        };
        input.media_type()?;
        Ok(input)
    }

    pub fn media_type(&self) -> Result<&'static str> {
        ensure!(
            self.data.len() <= MAX_IMAGE_BYTES,
            "Image exceeds 20 MiB limit"
        );
        if self.data.starts_with(b"\x89PNG\r\n\x1a\n") {
            Ok("image/png")
        } else if self.data.starts_with(b"\xff\xd8\xff") {
            Ok("image/jpeg")
        } else if self.data.starts_with(b"GIF87a") || self.data.starts_with(b"GIF89a") {
            Ok("image/gif")
        } else if self.data.starts_with(b"RIFF") && self.data.get(8..12) == Some(b"WEBP") {
            Ok("image/webp")
        } else {
            bail!("Unsupported image signature; expected PNG, JPEG, GIF or WebP")
        }
    }
}
