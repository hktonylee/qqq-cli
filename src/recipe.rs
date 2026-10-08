//! Declarative, project-local recipes expanded as literal task data.
use crate::{db, errors::Info, import};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    version: u32,
    #[serde(default)]
    parameters: Vec<Parameter>,
    tasks: Vec<import::InputTask>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameter {
    name: String,
    #[serde(default, deserialize_with = "string_default")]
    default: Option<String>,
}

fn string_default<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn recipe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn values(parameters: &[Parameter], assignments: &[String]) -> Result<BTreeMap<String, String>> {
    let mut declared = BTreeMap::new();
    for parameter in parameters {
        ensure!(
            identifier(&parameter.name),
            Info::invalid_argument("template", "Invalid recipe parameter name")
                .detail("parameter", parameter.name.clone())
        );
        ensure!(
            declared
                .insert(parameter.name.as_str(), parameter)
                .is_none(),
            Info::invalid_argument("template", "Duplicate recipe parameter declaration")
                .detail("parameter", parameter.name.clone())
        );
    }
    let mut values = BTreeMap::new();
    for assignment in assignments {
        let (name, value) = assignment.split_once('=').context(
            Info::invalid_argument("var", "Recipe parameter must use NAME=VALUE")
                .detail("reason", "invalid_recipe_assignment"),
        )?;
        ensure!(
            identifier(name),
            Info::invalid_argument("var", "Invalid recipe parameter name")
                .detail("parameter", name)
        );
        ensure!(
            declared.contains_key(name),
            Info::invalid_argument("var", "Unknown recipe parameter").detail("parameter", name)
        );
        ensure!(
            values.insert(name.to_owned(), value.to_owned()).is_none(),
            Info::invalid_argument("var", "Duplicate recipe parameter assignment")
                .detail("parameter", name)
        );
    }
    for (name, parameter) in declared {
        if !values.contains_key(name) {
            let default = parameter.default.as_ref().with_context(|| {
                Info::invalid_argument("var", "Missing required recipe parameter")
                    .detail("parameter", name)
            })?;
            values.insert(name.to_owned(), default.clone());
        }
    }
    Ok(values)
}

fn expand(text: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let mut result = String::new();
    let mut rest = text;
    while let Some(index) = rest.find('$') {
        result.push_str(&rest[..index]);
        rest = &rest[index..];
        if let Some(tail) = rest.strip_prefix("$$") {
            result.push('$');
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("${") {
            let end = tail.find('}').context(Info::invalid_argument(
                "template",
                "Unterminated recipe parameter placeholder",
            ))?;
            let name = &tail[..end];
            ensure!(
                identifier(name),
                Info::invalid_argument("template", "Invalid recipe parameter placeholder")
            );
            result.push_str(values.get(name).with_context(|| {
                Info::invalid_argument("template", "Unknown recipe parameter placeholder")
                    .detail("parameter", name)
            })?);
            rest = &tail[end + 1..];
        } else {
            result.push('$');
            rest = &rest[1..];
        }
    }
    result.push_str(rest);
    Ok(result)
}

pub(crate) fn read(name: &str, assignments: &[String]) -> Result<import::ValidatedBatch> {
    ensure!(
        recipe_name(name),
        Info::invalid_argument(
            "template",
            "Recipe name must contain only ASCII letters, digits, underscores or hyphens"
        )
        .detail("reason", "invalid_recipe_name")
    );
    let database = db::database_path(false)?;
    let project = database
        .parent()
        .and_then(|parent| parent.parent())
        .context("Database path has no project directory")?;
    let path = project.join(".qqq-recipes").join(format!("{name}.json"));
    let content = std::fs::read_to_string(&path).with_context(|| {
        Info::invalid_argument("template", "Cannot read project recipe as UTF-8")
            .detail("recipe", name)
            .detail("path", path.display().to_string())
            .human(format!(
                "Cannot read recipe {name:?} at {} as UTF-8",
                path.display()
            ))
    })?;
    let mut recipe: Recipe = serde_json::from_str(&content).map_err(|error| {
        let info = Info::invalid_argument("template", "Invalid recipe JSON")
            .detail("reason", "invalid_recipe_json")
            .detail("line", error.line())
            .detail("column", error.column());
        anyhow::Error::new(error).context(info)
    })?;
    ensure!(
        recipe.version == 1,
        Info::invalid_argument("version", "Unsupported recipe version; expected 1")
            .detail("reason", "unsupported_recipe_version")
            .detail("expected_version", 1)
            .detail("actual_version", recipe.version)
    );
    let values = values(&recipe.parameters, assignments)?;
    for task in &mut recipe.tasks {
        task.description = expand(&task.description, &values)?;
        for tag in &mut task.tags {
            *tag = expand(tag, &values)?;
        }
    }
    import::prepare(recipe.version, recipe.tasks)
}
