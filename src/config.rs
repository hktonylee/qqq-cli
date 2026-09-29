use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::HashMap, fs, io::ErrorKind, path::PathBuf};

#[derive(Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub alias: HashMap<String, String>,
    #[serde(default)]
    pub herdr: Herdr,
}

#[derive(Default, Deserialize)]
pub struct Herdr {
    #[serde(default, rename = "next-to-new-agent")]
    pub next_to_new_agent: bool,
}

pub fn load() -> Result<Config> {
    let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) else {
        return Ok(Config::default());
    };
    let path = PathBuf::from(home).join(".config/qqq/config.toml");
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => return Err(error).with_context(|| format!("Cannot read {}", path.display())),
    };
    toml::from_str(&content).with_context(|| format!("Invalid config {}", path.display()))
}
