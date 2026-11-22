//! What a project says about its visual review, in its TOML: `[visual]` (routes, viewports, themes, masks, the
//! test account's login, PR images) and each repository's `dev` command (and `storybook`), next to its
//! `checks`. Only the user's own config is read, never a file of the repository: like the worktree setup, the
//! commands run for a repository the user put in a project.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Settings {
    pub routes: Vec<RouteEntry>,
    pub viewports: Vec<String>,
    pub themes: Vec<String>,
    /// CSS selectors of what changes on its own (dates, avatars): hidden in both captures.
    pub mask: Vec<String>,
    pub threshold: Option<u8>,
    pub timeout: Option<u64>,
    pub login: Option<Login>,
    pub pr_images: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum RouteEntry {
    Path(String),
    Full(Route),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Route {
    pub path: String,
    pub state: Option<String>,
    pub viewports: Vec<String>,
    pub repo: Option<String>,
}

impl RouteEntry {
    pub fn route(&self) -> Route {
        match self {
            RouteEntry::Path(path) => Route {
                path: path.clone(),
                ..Route::default()
            },
            RouteEntry::Full(route) => route.clone(),
        }
    }
}

/// Signing in to a test account of the local dev server: never a production one.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Login {
    pub url: String,
    /// CSS selector → value; `env:NAME` reads the variable `NAME` of TermF's environment.
    pub fields: BTreeMap<String, String>,
    pub submit: Option<String>,
    pub wait: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub project: String,
    pub repo: String,
    pub path: PathBuf,
    pub dev: Option<String>,
    pub storybook: Option<String>,
    pub settings: Settings,
}

pub fn load(config_dir: &Path, repo: &Path) -> Result<Config> {
    let (project, name) = termf_core::load_projects(config_dir)?
        .into_iter()
        .find_map(|p| {
            let r = p.repos.iter().find(|r| same(Path::new(&r.path), repo))?;
            Some((p.id.clone(), r.name.clone()))
        })
        .with_context(|| {
            format!(
                "{} is in no project: TermF reviews the repositories of its projects only",
                repo.display()
            )
        })?;
    let file = termf_core::project_file(config_dir, &project)?;
    let table: toml::Table = toml::from_str(&fs::read_to_string(&file)?)
        .with_context(|| format!("invalid {}", file.display()))?;
    let entry = table
        .get("repos")
        .and_then(|r| r.as_array())
        .into_iter()
        .flatten()
        .find(|r| {
            r.get("path")
                .and_then(|p| p.as_str())
                .is_some_and(|p| same(&termf_core::expand_tilde(p), repo))
        });
    let command = |key: &str| {
        entry
            .and_then(|r| r.get(key)?.as_str())
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_owned)
    };
    let settings = match table.get("visual") {
        Some(visual) => visual
            .clone()
            .try_into()
            .with_context(|| format!("invalid [visual] in {}", file.display()))?,
        None => Settings::default(),
    };
    Ok(Config {
        project,
        repo: name,
        path: repo.to_path_buf(),
        dev: command("dev"),
        storybook: command("storybook"),
        settings,
    })
}

pub fn field_value(value: &str) -> Result<String> {
    match value.strip_prefix("env:") {
        Some(name) => std::env::var(name.trim())
            .with_context(|| format!("the login needs the variable {name}, which is not set")),
        None => Ok(value.to_owned()),
    }
}

pub(crate) fn same(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
