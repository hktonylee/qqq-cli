use crate::{
    errors::{Code, Info},
    selection::{self, Prepared, Selectors, Visibility},
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 1_048_576;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedView {
    pub name: String,
    #[serde(default)]
    pub criteria: Selectors,
    #[serde(default)]
    pub include_archived: bool,
    pub max_completed: Option<i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub views: Vec<SavedView>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            version: 1,
            views: Vec::new(),
        }
    }
}

fn valid_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty() && name.trim() == name && !name.chars().any(char::is_control),
        Info::invalid_argument(
            "NAME",
            "View names must be nonempty without controls or surrounding whitespace"
        )
    );
    Ok(())
}

fn validate(view: &mut SavedView) -> Result<()> {
    valid_name(&view.name)?;
    ensure!(
        view.max_completed.is_none_or(|limit| limit >= 0),
        Info::invalid_argument("--max-completed", "Completed limit must be nonnegative")
    );
    selection::prepare(None, &view.criteria, Visibility::default())?;
    view.criteria.tags = crate::tags::normalize(&view.criteria.tags)?;
    Ok(())
}

pub fn path() -> Result<PathBuf> {
    let db = crate::db::database_path(false)?;
    let project = db
        .parent()
        .and_then(Path::parent)
        .context("Missing project root")?;
    Ok(project.join(".qqq-views.json"))
}

pub fn load(path: &Path) -> Result<Catalog> {
    let loaded = (|| -> Result<Catalog> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Catalog::default());
            }
            Err(error) => return Err(error.into()),
        };
        let mut source = String::new();
        file.take(MAX_BYTES + 1).read_to_string(&mut source)?;
        ensure!(
            source.len() as u64 <= MAX_BYTES,
            "View catalog exceeds 1 MiB"
        );
        let mut catalog: Catalog = serde_json::from_str(&source)?;
        ensure!(
            catalog.version == 1,
            "Unsupported view version {}; expected 1",
            catalog.version
        );
        let mut names = HashSet::new();
        for view in &mut catalog.views {
            validate(view)?;
            ensure!(
                names.insert(view.name.clone()),
                "Duplicate view name {}",
                view.name
            );
        }
        catalog.views.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(catalog)
    })();
    loaded.context(
        Info::new(Code::ConfigError, "Invalid project views file")
            .detail("path", path.display().to_string())
            .detail("reason", "invalid_views"),
    )
}

fn unknown(name: &str) -> Info {
    Info::invalid_argument("--view", format!("Unknown view {name:?}"))
        .detail("view", name)
        .detail("reason", "unknown_view")
}

pub fn find(catalog: &Catalog, name: &str) -> Result<SavedView> {
    catalog
        .views
        .iter()
        .find(|view| view.name == name)
        .cloned()
        .context(unknown(name))
}

fn update(
    path: &Path,
    change: impl FnOnce(&mut Catalog) -> Result<SavedView>,
) -> Result<SavedView> {
    let project = path.parent().context("Missing views directory")?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(project.join(".qqq/views.lock"))?;
    lock.lock_exclusive()?;
    let mut catalog = load(path)?;
    let changed = change(&mut catalog)?;
    catalog.views.sort_by(|a, b| a.name.cmp(&b.name));
    let bytes = serde_json::to_vec_pretty(&catalog)?;
    ensure!(
        (bytes.len() as u64) < MAX_BYTES,
        Info::invalid_argument("view", "View catalog exceeds 1 MiB")
    );
    let mut temporary = tempfile::NamedTempFile::new_in(project)?;
    if let Ok(metadata) = std::fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.write_all(&bytes)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| anyhow::Error::new(error.error))?;
    #[cfg(unix)]
    File::open(project)?.sync_all()?;
    Ok(changed)
}

pub fn save(path: &Path, mut view: SavedView) -> Result<SavedView> {
    validate(&mut view)?;
    update(path, |catalog| {
        if let Some(existing) = catalog
            .views
            .iter_mut()
            .find(|existing| existing.name == view.name)
        {
            *existing = view.clone();
        } else {
            catalog.views.push(view.clone());
        }
        Ok(view)
    })
}

pub fn remove(path: &Path, name: &str) -> Result<SavedView> {
    valid_name(name)?;
    update(path, |catalog| {
        let index = catalog
            .views
            .iter()
            .position(|view| view.name == name)
            .context(unknown(name))?;
        Ok(catalog.views.remove(index))
    })
}

#[derive(Clone)]
pub struct ViewContext {
    pub path: PathBuf,
    pub selectors: Selectors,
    pub visibility: Visibility,
    pub active: Option<SavedView>,
    pub prepared: Prepared,
}

impl ViewContext {
    pub fn new(
        path: PathBuf,
        selectors: Selectors,
        visibility: Visibility,
        active: Option<SavedView>,
    ) -> Result<Self> {
        let prepared = prepare(active.as_ref(), &selectors, visibility)?;
        Ok(Self {
            path,
            selectors,
            visibility,
            active,
            prepared,
        })
    }
    pub fn select(&mut self, active: Option<SavedView>) -> Result<()> {
        let prepared = prepare(active.as_ref(), &self.selectors, self.visibility)?;
        self.active = active;
        self.prepared = prepared;
        Ok(())
    }
}

pub fn prepare(
    view: Option<&SavedView>,
    explicit: &Selectors,
    mut visibility: Visibility,
) -> Result<Prepared> {
    if let Some(view) = view {
        visibility.include_archived = visibility.include_archived.or(Some(view.include_archived));
        visibility.max_completed = visibility.max_completed.or(Some(view.max_completed));
    }
    selection::prepare(view.map(|view| &view.criteria), explicit, visibility)
}

pub fn describe(view: &SavedView) -> Vec<String> {
    let criteria = &view.criteria;
    vec![
        format!("View: {}", view.name),
        format!(
            "Tags (all): {}",
            if criteria.tags.is_empty() {
                "Any".to_owned()
            } else {
                criteria.tags.join(", ")
            }
        ),
        format!(
            "Filter: {}",
            criteria
                .filter
                .as_deref()
                .map(crate::output::clean)
                .unwrap_or_else(|| "Any".to_owned())
        ),
        format!(
            "Query: {}",
            criteria
                .query
                .as_deref()
                .map(crate::output::clean)
                .unwrap_or_else(|| "Any".to_owned())
        ),
        format!(
            "Statuses (any): {}",
            if criteria.statuses.is_empty() {
                "Any".to_owned()
            } else {
                criteria
                    .statuses
                    .iter()
                    .map(|status| status.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ),
        format!(
            "Readiness: {}",
            match criteria.readiness {
                None => "Any",
                Some(selection::Readiness::Ready) => "Ready",
                Some(selection::Readiness::Blocked) => "Blocked",
            }
        ),
        format!(
            "Archived: {}",
            if view.include_archived {
                "Shown"
            } else {
                "Hidden"
            }
        ),
        format!(
            "Completed limit: {}",
            view.max_completed
                .map_or_else(|| "All".to_owned(), |limit| limit.to_string())
        ),
    ]
}

pub fn render(value: &serde_json::Value) -> String {
    if let Some(views) = value.as_array() {
        if views.is_empty() {
            return "No saved views.".to_owned();
        }
        return views
            .iter()
            .filter_map(|view| view["name"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    let view: SavedView = serde_json::from_value(value.clone()).expect("view output is typed");
    describe(&view).join("\n")
}
