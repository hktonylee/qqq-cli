use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, io::Read};

pub const MANIFEST_NAME: &str = "manifest.json";
pub const DATABASE_NAME: &str = "qqq.db";
pub const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileMeta {
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ImageMeta {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub database: FileMeta,
    pub images: Vec<ImageMeta>,
}

impl Manifest {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported snapshot format version");
        ensure!(self.database.bytes > 0, "Snapshot database is empty");
        validate_hash(&self.database.sha256)?;
        let mut seen = HashSet::new();
        for image in &self.images {
            validate_image_path(&image.path)?;
            validate_hash(&image.sha256)?;
            ensure!(seen.insert(&image.path), "Duplicate image path in manifest");
        }
        Ok(())
    }
}

fn validate_hash(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "Invalid SHA-256 in snapshot manifest"
    );
    Ok(())
}

pub fn validate_image_path(path: &str) -> Result<(i64, i64, &str)> {
    let parts: Vec<_> = path.split('/').collect();
    ensure!(
        parts.len() == 3 && parts[0] == "images",
        "Invalid image path"
    );
    let task = canonical_id(parts[1])?;
    let (id, extension) = parts[2]
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("Invalid image path"))?;
    let image = canonical_id(id)?;
    ensure!(
        matches!(extension, "png" | "jpg" | "gif" | "webp"),
        "Invalid image extension"
    );
    Ok((task, image, extension))
}

fn canonical_id(value: &str) -> Result<i64> {
    let id: i64 = value.parse()?;
    ensure!(id > 0 && id.to_string() == value, "Invalid image identity");
    Ok(id)
}

pub fn hash_reader(mut reader: impl Read) -> Result<FileMeta> {
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .ok_or_else(|| anyhow::anyhow!("Snapshot size overflow"))?;
        hash.update(&chunk[..count]);
    }
    Ok(FileMeta {
        bytes,
        sha256: format!("{:x}", hash.finalize()),
    })
}
