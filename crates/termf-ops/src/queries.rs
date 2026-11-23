//! The SQL console's saved queries: in the project's file (`[[ops.queries]]`), so synced with it. One can become
//! a recipe of the project that runs that query only, for the agents (through their approval cards).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table};

use crate::{Kind, Recipe, connections, edit, named, project_recipes, valid_name};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The database it was written for; any of the project's otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    pub sql: String,
}

fn text(what: &str, value: &str, max: usize) -> Result<()> {
    ensure!(
        value.chars().count() <= max && !value.chars().any(char::is_control),
        "{what}: {max} characters at most, on one line"
    );
    Ok(())
}

impl Query {
    pub fn check(&self) -> Result<()> {
        ensure!(!self.name.trim().is_empty(), "a saved query has a name");
        text("its name", &self.name, 100)?;
        text("its description", &self.description, 300)?;
        text("its folder", &self.folder, 60)?;
        ensure!(self.tags.len() <= 10, "10 tags at most");
        for tag in &self.tags {
            ensure!(!tag.trim().is_empty(), "a tag is not empty");
            text("a tag", tag, 30)?;
        }
        ensure!(!self.sql.trim().is_empty(), "the query is empty");
        ensure!(
            self.sql.chars().count() <= 10_000
                && !self
                    .sql
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')),
            "the query: 10 000 characters at most, without control characters"
        );
        Ok(())
    }
}

/// The project's saved queries, as its file lists them, read apart from its connections: one that does not read is
/// left out and said in the errors, never the others nor the connections.
pub fn queries(config_dir: &Path, project: &str) -> Result<(Vec<Query>, Vec<String>)> {
    let file = termf_core::project_file(config_dir, project)?;
    let doc: toml::Table = toml::from_str(&fs::read_to_string(&file)?)
        .with_context(|| format!("invalid {}", file.display()))?;
    let list = doc
        .get("ops")
        .and_then(|o| o.get("queries"))
        .and_then(|q| q.as_array())
        .cloned()
        .unwrap_or_default();
    let (mut found, mut errors) = (Vec::new(), Vec::new());
    for (i, item) in list.into_iter().enumerate() {
        let read = item
            .try_into::<Query>()
            .map_err(anyhow::Error::from)
            .and_then(|q| q.check().map(|()| q));
        match read {
            Ok(q) => found.push(q),
            Err(e) => errors.push(format!("[[ops.queries]] #{}: {e:#}", i + 1)),
        }
    }
    Ok((found, errors))
}

/// Adds a saved query, or replaces the one named `previous` (its name may change).
pub fn save_query(
    config_dir: &Path,
    project: &str,
    query: &Query,
    previous: Option<&str>,
) -> Result<Vec<Query>> {
    query.check()?;
    let taken = queries(config_dir, project)?
        .0
        .iter()
        .any(|q| q.name == query.name && Some(q.name.as_str()) != previous);
    ensure!(!taken, "a saved query is already named {}", query.name);
    if let Some(name) = &query.connection {
        ensure!(
            connections(config_dir, project)?
                .iter()
                .any(|c| &c.name == name && c.kind.is_db()),
            "{name} is not a database of the project"
        );
    }
    edit(config_dir, project, "queries", |list| {
        let mut table: Table = toml::to_string(query)?
            .parse::<DocumentMut>()?
            .as_table()
            .clone();
        table.set_implicit(false);
        match previous.and_then(|p| list.iter().position(|t| named(t, p))) {
            Some(i) => *list.get_mut(i).expect("found") = table,
            None => {
                ensure!(
                    previous.is_none(),
                    "no saved query named {}",
                    previous.unwrap_or_default()
                );
                list.push(table);
            }
        }
        Ok(())
    })?;
    Ok(queries(config_dir, project)?.0)
}

pub fn delete_query(config_dir: &Path, project: &str, name: &str) -> Result<Vec<Query>> {
    edit(config_dir, project, "queries", |list| {
        let before = list.len();
        list.retain(|t| !named(t, name));
        ensure!(list.len() < before, "no saved query named {name}");
        Ok(())
    })?;
    Ok(queries(config_dir, project)?.0)
}

/// A recipe name from a query's: `query-` and its words.
fn recipe_name(name: &str) -> String {
    let words: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let words: Vec<&str> = words.split('-').filter(|w| !w.is_empty()).collect();
    format!("query-{}", words.join("-"))
        .chars()
        .take(100)
        .collect()
}

/// Writes the project's recipe that runs this saved query, and only it, read-only, each run approved by the user
/// (`approval = "always"`), personal data masked for an agent: its name.
pub fn query_recipe(config_dir: &Path, project: &str, name: &str) -> Result<String> {
    let query = queries(config_dir, project)?
        .0
        .into_iter()
        .find(|q| q.name == name)
        .with_context(|| format!("no saved query named {name}"))?;
    let recipe = recipe_name(&query.name);
    ensure!(valid_name(&recipe), "{name} gives no recipe name");
    let dir = project_recipes(config_dir, project);
    let file = dir.join(format!("{recipe}.toml"));
    ensure!(!file.exists(), "the project already has a recipe {recipe}");
    let kinds: Vec<Kind> = match &query.connection {
        Some(c) => connections(config_dir, project)?
            .into_iter()
            .filter(|x| &x.name == c)
            .map(|x| x.kind)
            .collect(),
        None => vec![
            Kind::Postgres,
            Kind::Mysql,
            Kind::Mariadb,
            Kind::Sqlite,
            Kind::Clickhouse,
        ],
    };
    ensure!(
        !kinds.is_empty(),
        "its database is no longer in the project"
    );
    let mut doc = DocumentMut::new();
    doc["name"] = toml_edit::value(&recipe);
    let description = if query.description.is_empty() {
        format!("The saved query {}", query.name)
    } else {
        query.description.clone()
    };
    doc["description"] = toml_edit::value(description);
    if let Some(c) = &query.connection {
        doc["connection"] = toml_edit::value(c.as_str());
    }
    doc["mode"] = toml_edit::value("read");
    doc["ai_access"] = toml_edit::value("redacted");
    doc["approval"] = toml_edit::value("always");
    let mut redact = toml_edit::Array::new();
    for r in ["email", "phone", "iban", "jwt", "vault_values"] {
        redact.push(r);
    }
    doc["redact"] = toml_edit::value(redact);
    let mut param = Table::new();
    param["type"] = toml_edit::value("sql");
    param["description"] = toml_edit::value(format!("The saved query {}", query.name));
    param["required"] = toml_edit::value(true);
    param["default"] = toml_edit::value(query.sql.trim());
    let mut values = toml_edit::Array::new();
    values.push(query.sql.trim());
    param["values"] = toml_edit::value(values);
    let mut params = Table::new();
    params.set_implicit(true);
    params.insert("query", Item::Table(param));
    doc["params"] = Item::Table(params);
    let mut limits = Table::new();
    limits["rows"] = toml_edit::value(500);
    limits["lines"] = toml_edit::value(10_000);
    limits["bytes"] = toml_edit::value(1 << 20);
    limits["timeout_s"] = toml_edit::value(15);
    doc["limits"] = Item::Table(limits);
    let mut run = toml_edit::ArrayOfTables::new();
    for kind in kinds {
        let mut v = Table::new();
        v["kind"] = toml_edit::value(serde_json::to_value(kind)?.as_str().unwrap_or_default());
        let mut args = toml_edit::Array::new();
        args.push("{query}");
        v["args"] = toml_edit::value(args);
        run.push(v);
    }
    doc["run"] = Item::ArrayOfTables(run);
    let text = doc.to_string();
    Recipe::parse(&text, "project").context("the recipe it would make")?;
    fs::create_dir_all(&dir)?;
    termf_core::write_atomic(&file, &text)?;
    Ok(recipe)
}
