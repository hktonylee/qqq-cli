use anyhow::{Context, Result, bail, ensure};
use std::{io::Read, path::Path};

pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

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
