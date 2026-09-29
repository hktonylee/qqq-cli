use crate::config;
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, Key, Table};

pub fn execute(
    list: bool,
    get: Option<&str>,
    unset: Option<&str>,
    key: Option<&str>,
    value: Option<&str>,
) -> Result<Value> {
    let path = config::path().context("HOME must be set to access qqq config")?;
    if list {
        let content = read(&path)?;
        let values: toml::Value = toml::from_str(&content)
            .with_context(|| format!("Invalid config {}", path.display()))?;
        let mut result = Map::new();
        flatten(&values, &[], &mut result)?;
        return Ok(Value::Object(result));
    }
    let key = get
        .or(unset)
        .or(key)
        .expect("Clap requires a config action");
    let keys = Key::parse(key).with_context(|| format!("Invalid config key '{key}'"))?;
    if unset.is_some() || value.is_some() {
        return mutate(&path, key, &keys, value);
    }
    lookup(&read(&path)?, key, &keys)
}

fn read(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("Cannot read {}", path.display())),
    }
}

fn lookup(content: &str, key: &str, keys: &[Key]) -> Result<Value> {
    let document: toml::Value = toml::from_str(content).context("Invalid config TOML")?;
    let mut value = &document;
    for segment in keys {
        value = value
            .get(segment.get())
            .with_context(|| format!("Config key '{key}' is not set"))?;
    }
    serde_json::to_value(value).context("Cannot represent config value as JSON")
}

fn flatten(value: &toml::Value, path: &[String], result: &mut Map<String, Value>) -> Result<()> {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                let mut child = path.to_vec();
                child.push(Key::new(key).to_string());
                flatten(value, &child, result)?;
            }
        }
        _ => {
            result.insert(path.join("."), serde_json::to_value(value)?);
        }
    }
    Ok(())
}

fn parse_value(keys: &[Key], value: &str) -> Result<toml_edit::Value> {
    if keys.len() == 2 && keys[0].get() == "alias" {
        return Ok(toml_edit::Value::from(value));
    }
    if keys.len() == 2 && keys[0].get() == "herdr" && keys[1].get() == "next-to-new-agent" {
        ensure!(
            matches!(value, "true" | "false"),
            "herdr.next-to-new-agent requires true or false"
        );
    }
    Ok(value
        .parse()
        .unwrap_or_else(|_| toml_edit::Value::from(value)))
}

fn edit(
    item: &mut Item,
    keys: &[Key],
    value: Option<toml_edit::Value>,
    full_key: &str,
) -> Result<()> {
    let table = item
        .as_table_like_mut()
        .with_context(|| format!("Config key '{full_key}' traverses a non-table value"))?;
    let first = keys.first().context("Config key cannot be empty")?.get();
    if keys.len() > 1 {
        if !table.contains_key(first) && value.is_some() {
            let mut child = Table::new();
            child.set_implicit(true);
            table.insert(first, Item::Table(child));
        }
        let child = table
            .get_mut(first)
            .with_context(|| format!("Config key '{full_key}' is not set"))?;
        return edit(child, &keys[1..], value, full_key);
    }
    match value {
        Some(mut value) => {
            if let Some(slot) = table.get_mut(first) {
                if let Some(old) = slot.as_value() {
                    *value.decor_mut() = old.decor().clone();
                }
                *slot = Item::Value(value);
            } else {
                table.insert(first, Item::Value(value));
            }
        }
        None => {
            table
                .remove(first)
                .with_context(|| format!("Config key '{full_key}' is not set"))?;
        }
    }
    Ok(())
}

// Resolve existing symlinks so atomic replacement updates their target, not the link.
fn target(path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(_) => path
            .canonicalize()
            .with_context(|| format!("Cannot resolve {}", path.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let parent = path.parent().context("Config path has no parent")?;
            fs::create_dir_all(parent)
                .with_context(|| format!("Cannot create {}", parent.display()))?;
            Ok(parent
                .canonicalize()?
                .join(path.file_name().context("Config path has no filename")?))
        }
        Err(error) => Err(error).with_context(|| format!("Cannot access {}", path.display())),
    }
}

fn mutate(path: &Path, key: &str, keys: &[Key], value: Option<&str>) -> Result<Value> {
    let value = value.map(|value| parse_value(keys, value)).transpose()?;
    let path = target(path)?;
    let mut lock_name = path.as_os_str().to_owned();
    lock_name.push(".lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(lock_name))
        .context("Cannot open config lock")?;
    // Keep sidecar stable across atomic replacements; reread only after acquiring lock.
    fs2::FileExt::lock_exclusive(&lock).context("Cannot lock config")?;
    let mut document: DocumentMut = read(&path)?
        .parse()
        .with_context(|| format!("Invalid config {}", path.display()))?;
    let setting = value.is_some();
    edit(document.as_item_mut(), keys, value, key)?;
    let content = document.to_string();
    toml::from_str::<config::Config>(&content).context("Invalid config value")?;
    let display: config::DisplayConfig =
        toml::from_str(&content).context("Invalid display config value")?;
    if let Some(limit) = display.display.max_completed {
        i64::try_from(limit).context("display.max-completed is too large")?;
    }
    let result = if setting {
        lookup(&content, key, keys)?
    } else {
        Value::Null
    };
    let parent = path.parent().context("Config path has no parent")?;
    let mut draft =
        tempfile::NamedTempFile::new_in(parent).context("Cannot create config draft")?;
    match fs::metadata(&path) {
        Ok(metadata) => draft.as_file().set_permissions(metadata.permissions())?,
        Err(error) if error.kind() == ErrorKind::NotFound => (),
        Err(error) => return Err(error).context("Cannot read config permissions"),
    }
    draft
        .write_all(content.as_bytes())
        .context("Cannot write config draft")?;
    draft
        .as_file()
        .sync_all()
        .context("Cannot sync config draft")?;
    draft
        .persist(&path)
        .with_context(|| format!("Cannot replace {}", path.display()))?;
    Ok(json!({"key":key,"value":result}))
}
