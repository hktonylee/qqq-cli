use anyhow::{Context, Result};
use serde::{Deserialize, de::DeserializeOwned};
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

#[derive(Default, Deserialize)]
pub struct DisplayConfig {
    #[serde(default)]
    pub display: Display,
}

#[derive(Default, Deserialize)]
pub struct Display {
    #[serde(rename = "max-completed")]
    pub max_completed: Option<u64>,
}

#[derive(Default, Deserialize)]
pub struct TuiConfig {
    #[serde(default)]
    pub tui: Tui,
}

#[derive(Default, Deserialize)]
pub struct Tui {
    #[serde(default)]
    pub after_save_new: AfterSaveNew,
}

#[derive(Clone, Copy, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AfterSaveNew {
    #[default]
    OpenSaved,
    OpenNew,
}

pub fn path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".config/qqq/config.toml"))
}

pub fn load() -> Result<Config> {
    load_section()
}
pub fn load_display() -> Result<DisplayConfig> {
    load_section()
}
pub fn load_tui() -> Result<TuiConfig> {
    load_section()
}

// Commands load their own settings independently of unrelated sections.
fn load_section<T: Default + DeserializeOwned>() -> Result<T> {
    let Some(path) = path() else {
        return Ok(T::default());
    };
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(T::default()),
        Err(error) => return Err(error).with_context(|| format!("Cannot read {}", path.display())),
    };
    toml::from_str(&content).with_context(|| format!("Invalid config {}", path.display()))
}
