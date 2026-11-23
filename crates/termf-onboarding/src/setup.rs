//! A new project's setup, before anything is written: the repositories of the machine as cards (git state,
//! remote, stack), what TermF finds in the ones chosen, the team and the skills it proposes, and the project file
//! creating it would write.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use termf_library::stack::{self, RepoStack, Tag};
use termf_library::{Item, Kind as ItemKind, Layer, Scope};
use termf_worktree::{RepoRecipe, Script, scripts};
use toml_edit::DocumentMut;

use crate::Facts;
use crate::team::{self, Choices, Edit, RoleChoice};

/// A remote's forge and the repository on it: `github.com` and `acme/api`.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Remote {
    pub host: String,
    pub slug: String,
}

/// A repository as the setup lists it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub name: String,
    pub path: String,
    pub modified: Option<u64>,
    /// `None` when HEAD is detached or the repository has no commit.
    pub branch: Option<String>,
    /// `origin`, else the first remote.
    pub remote: Option<Remote>,
    /// Files changed, staged or not, and untracked ones.
    pub changes: usize,
    /// The stack's tags, as the library reads them.
    pub stack: Vec<String>,
}

/// What TermF finds by itself in a repository.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub facts: Facts,
    /// With their version.
    pub tags: Vec<Tag>,
    pub scripts: Vec<Script>,
    /// How its worktrees would be prepared.
    pub recipe: RepoRecipe,
    pub remote: Option<Remote>,
    /// Ticket keys' prefixes the recent commits cite (`ACME` for `ACME-142`): a tracker such as Jira or Linear.
    pub keys: Vec<String>,
}

/// A skill the chosen repositories switch on: by their stack (`by`), or shipped by one of their packages.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SkillOn {
    pub name: String,
    pub by: Vec<String>,
    pub package: Option<String>,
    /// What it is for, in one short sentence (the start of its description).
    pub description: String,
}

/// A framework's MCP server the chosen repositories call for, and the ones they are.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpFor {
    #[serde(flatten)]
    pub offer: termf_library::stack::McpOffer,
    pub repos: Vec<String>,
}

/// What the setup writes in the project's file besides its repositories, team and trackers: the MCP servers its
/// sessions get (`[mcp.<name>]`), each repository's dev server (`[[repos]] dev`, for the visual review) and the
/// pages the visual review captures (`[visual] routes`).
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Extras {
    pub mcp: serde_json::Map<String, serde_json::Value>,
    pub dev: BTreeMap<String, String>,
    pub routes: Vec<String>,
}

/// What TermF proposes for the chosen repositories, as the assistant would once the project exists.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub choices: Choices,
    /// Whether the front and the back can each have their squad.
    pub splittable: bool,
    pub roles: Vec<RoleChoice>,
    /// The roles that are workflow steps rather than tasks.
    pub steps: Vec<String>,
    /// The team as the project's file would get it.
    pub team: Edit,
    pub skills: Vec<SkillOn>,
    pub mcp: Vec<McpFor>,
}

const KEYS_READ: &str = "-200";
/// Words shaped like a ticket key that name a standard: `UTF-8`, `ISO-8601`, `SHA-256`…
const NOT_KEYS: [&str; 8] = ["UTF", "ISO", "SHA", "CVE", "RFC", "PEP", "ES", "HTTP"];
const MAX_KEYS: usize = 3;
/// How long a repository's facts are kept between the detection and the proposal.
const FACTS_FOR: Duration = Duration::from_secs(120);

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    // No optional locks: reading a repository must never hold `index.lock` while an agent commits there.
    let out = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The forge and `owner/name` of a remote's URL (HTTPS, SSH or `[user@]host:path`); credentials never come back.
pub fn remote(url: &str) -> Option<Remote> {
    let url = url.trim();
    let (authority, path) = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?,
        // scp-like, as git reads it: a colon before any slash. One letter is a Windows drive.
        None => url
            .split_once(':')
            .filter(|(a, _)| a.len() > 1 && !a.contains(['/', '\\']))?,
    };
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = path.trim_matches('/');
    let slug = path.strip_suffix(".git").unwrap_or(path);
    (!host.is_empty() && !slug.is_empty()).then(|| Remote {
        host,
        slug: slug.to_owned(),
    })
}

fn remote_of(repo: &Path) -> Option<Remote> {
    let urls = git(repo, &["config", "--get-regexp", r"^remote\..*\.url$"])?;
    let urls: Vec<(&str, &str)> = urls.lines().filter_map(|l| l.split_once(' ')).collect();
    let (_, url) = urls
        .iter()
        .find(|(key, _)| *key == "remote.origin.url")
        .or(urls.first())?;
    remote(url)
}

/// The branch checked out and how many files changed, from `git status --porcelain=v2 --branch`.
fn status(text: &str) -> (Option<String>, usize) {
    let branch = text
        .lines()
        .find_map(|l| l.strip_prefix("# branch.head "))
        .filter(|b| *b != "(detached)")
        .map(str::to_owned);
    let changes = text.lines().filter(|l| !l.starts_with('#')).count();
    (branch, changes)
}

fn card(path: &str) -> Card {
    let repo = termf_core::Repo::at(PathBuf::from(path), None);
    let dir = Path::new(path);
    let (branch, changes) = git(dir, &["status", "--porcelain=v2", "--branch"])
        .map(|s| status(&s))
        .unwrap_or_default();
    let mut tags: Vec<String> = Vec::new();
    for tag in stack::stack_tags(dir) {
        if !tags.contains(&tag.name) {
            tags.push(tag.name);
        }
    }
    Card {
        name: repo.name,
        path: repo.path,
        modified: repo.modified,
        branch,
        remote: remote_of(dir),
        changes,
        stack: tags,
    }
}

/// Each repository's card, read a few at a time: the stack's survey is kept by `HEAD`, so a second look is quick.
pub fn cards(paths: &[String]) -> Vec<Card> {
    let next = AtomicUsize::new(0);
    let found: Mutex<Vec<(usize, Card)>> = Mutex::new(Vec::with_capacity(paths.len()));
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8)
        .min(paths.len().max(1));
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(i) else { break };
                    let card = card(path);
                    found
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push((i, card));
                }
            });
        }
    });
    let mut found = found.into_inner().unwrap_or_else(|e| e.into_inner());
    found.sort_by_key(|(i, _)| *i);
    found.into_iter().map(|(_, c)| c).collect()
}

/// The prefixes of the ticket keys (`ACME-142`) cited at least twice in `subjects`, the most cited first.
fn ticket_keys(subjects: &str) -> Vec<String> {
    let mut count: BTreeMap<String, usize> = BTreeMap::new();
    for word in subjects.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        // `ACME-142-fix-login`, a branch name in a subject, counts as `ACME-142`.
        let mut parts = word.split('-');
        let (Some(prefix), Some(number)) = (parts.next(), parts.next()) else {
            continue;
        };
        let key = prefix.len() >= 2
            && prefix.len() <= 10
            && prefix.starts_with(|c: char| c.is_ascii_uppercase())
            && prefix
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && !number.is_empty()
            && number.chars().all(|c| c.is_ascii_digit())
            && !NOT_KEYS.contains(&prefix);
        if key {
            *count.entry(prefix.to_owned()).or_default() += 1;
        }
    }
    let mut keys: Vec<(String, usize)> = count.into_iter().filter(|(_, n)| *n >= 2).collect();
    keys.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    keys.into_iter().take(MAX_KEYS).map(|(k, _)| k).collect()
}

/// The library as a project without layer of its own sees it, the chosen repositories' stack switching skills on.
fn library(config_dir: &Path, stack: Vec<RepoStack>) -> Result<Vec<Item>> {
    let scope = Scope {
        stack,
        ..Scope::default()
    };
    termf_library::resolve(config_dir, &scope)
}

type FactsKey = (PathBuf, Vec<String>);
static FACTS: Mutex<BTreeMap<FactsKey, (Instant, Facts)>> = Mutex::new(BTreeMap::new());

fn facts_of(path: &Path, tests: &[String]) -> Facts {
    let key = (path.to_path_buf(), tests.to_vec());
    if let Some((at, facts)) = FACTS.lock().unwrap_or_else(|e| e.into_inner()).get(&key)
        && at.elapsed() < FACTS_FOR
    {
        return facts.clone();
    }
    let name = path
        .file_name()
        .map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy())
        .into_owned();
    let facts = crate::facts(&name, path, tests);
    let mut kept = FACTS.lock().unwrap_or_else(|e| e.into_inner());
    kept.retain(|_, (at, _)| at.elapsed() < FACTS_FOR);
    kept.insert(key, (Instant::now(), facts.clone()));
    facts
}

/// What TermF finds in the repository at `path`, reading its files and git only.
pub fn detect(config_dir: &Path, path: &str) -> Result<Detected> {
    let dir = Path::new(path);
    anyhow::ensure!(dir.is_dir(), "{path} is not a folder");
    let tests = team::test_globs(&library(config_dir, Vec::new())?);
    let facts = facts_of(dir, &tests);
    let recipe = termf_worktree::detect(dir, &[]);
    Ok(Detected {
        tags: stack::stack_tags(dir),
        scripts: scripts(dir, &recipe),
        remote: remote_of(dir),
        keys: ticket_keys(&git(dir, &["log", KEYS_READ, "--format=%s"]).unwrap_or_default()),
        recipe,
        facts,
    })
}

/// What TermF finds by itself in the repositories at `paths`, before they are a project: their facts and GitHub
/// repositories, nothing of a project's own (library, tickets, recipe, conventions).
pub(crate) fn survey(config_dir: &Path, paths: &[String]) -> Result<crate::Analysis> {
    anyhow::ensure!(!paths.is_empty(), "no repository chosen");
    let tests = team::test_globs(&library(config_dir, Vec::new())?);
    let mut github: Vec<String> = Vec::new();
    for p in paths {
        let dir = Path::new(p);
        anyhow::ensure!(dir.is_dir(), "{p} is not a folder");
        if let Some(r) = remote_of(dir).filter(|r| r.host == "github.com") {
            let slug = r.slug.to_lowercase();
            if !github.contains(&slug) {
                github.push(slug);
            }
        }
    }
    Ok(crate::Analysis {
        at: crate::now(),
        repos: paths
            .iter()
            .map(|p| facts_of(Path::new(p), &tests))
            .collect(),
        github,
        ..crate::Analysis::default()
    })
}

/// The team and skills TermF proposes for the repositories at `paths`, as the assistant would for a project
/// that has only them: the library without a project layer, `given` the owner's choices on the team so far.
pub fn draft(config_dir: &Path, paths: &[String], given: &Choices) -> Result<Draft> {
    let stack: Vec<RepoStack> = paths
        .iter()
        .map(|p| {
            let dir = Path::new(p);
            let name = dir.file_name().unwrap_or_default().to_string_lossy();
            RepoStack::read(&name, dir)
        })
        .collect();
    let items = library(config_dir, stack.clone())?;
    let tests = team::test_globs(&items);
    let repos: Vec<Facts> = paths
        .iter()
        .map(|p| facts_of(Path::new(p), &tests))
        .collect();
    let roles = team::roles(&items);
    let (choices, roles_chosen, edit) = team::propose(&repos, &roles, given);
    let skills = items
        .iter()
        .filter(|i| i.kind == ItemKind::Skill && i.active)
        .filter(|i| {
            i.layer == Layer::Package
                || (!i.activated_by.is_empty() && !i.activated_by.iter().any(|t| t == "*"))
        })
        .map(|i| SkillOn {
            name: i.name.clone(),
            by: i.activated_by.clone(),
            package: i.package.as_ref().map(|p| p.name.clone()),
            description: short(i.description.as_deref().unwrap_or_default()),
        })
        .collect();
    let mut mcp: Vec<McpFor> = Vec::new();
    for repo in &stack {
        for offer in stack::offers(repo).0 {
            match mcp.iter_mut().find(|m| m.offer.name == offer.name) {
                Some(seen) => seen.repos.push(repo.name.clone()),
                None => mcp.push(McpFor {
                    offer,
                    repos: vec![repo.name.clone()],
                }),
            }
        }
    }
    Ok(Draft {
        choices,
        splittable: team::can_split(&repos),
        roles: roles_chosen,
        steps: roles
            .iter()
            .filter(|r| !r.plannable && r.name != "expert")
            .map(|r| r.name.clone())
            .collect(),
        team: edit,
        skills,
        mcp,
    })
}

/// The project file creating the project then writing `team` and `trackers` would leave, as those writes do it.
pub fn preview(
    name: &str,
    repos: &[String],
    team: Option<&Edit>,
    trackers: Option<&termf_tickets::Trackers>,
    checks: &BTreeMap<String, String>,
    extras: &Extras,
) -> Result<String> {
    let mut doc: DocumentMut = termf_core::project_text(name, repos)?.parse()?;
    termf_core::set_checks(&mut doc, checks);
    put_extras(&mut doc, extras)?;
    let mut text = doc.to_string();
    if let Some(edit) = team {
        text = team::render(&text, edit)?;
    }
    if let Some(trackers) = trackers {
        let mut doc: DocumentMut = text.parse()?;
        termf_tickets::write_trackers(&mut doc, trackers)?;
        text = doc.to_string();
    }
    Ok(text)
}

fn put_extras(doc: &mut DocumentMut, extras: &Extras) -> Result<()> {
    termf_core::set_repo_field(doc, "dev", &extras.dev);
    let routes: Vec<&str> = extras
        .routes
        .iter()
        .map(|r| r.trim())
        .filter(|r| r.starts_with('/'))
        .collect();
    if !routes.is_empty() {
        let visual = doc
            .entry("visual")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut()
            .context("`visual` must be a table")?;
        visual.insert(
            "routes",
            toml_edit::value(routes.into_iter().collect::<toml_edit::Array>()),
        );
    }
    termf_library::set_mcp_servers(doc, &extras.mcp)
}

/// Writes `extras` in the project's file, keeping the rest of it.
pub fn write_extras(config_dir: &Path, project: &str, extras: &Extras) -> Result<()> {
    let file = termf_core::project_file(config_dir, project)?;
    let mut doc: DocumentMut = fs::read_to_string(&file)?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    put_extras(&mut doc, extras)?;
    termf_core::write_atomic(&file, &doc.to_string())
}

/// The first sentence of a description, 120 characters at most.
fn short(text: &str) -> String {
    let first = text
        .split(". ")
        .next()
        .unwrap_or(text)
        .trim()
        .trim_end_matches('.');
    if first.chars().count() <= 120 {
        return first.to_owned();
    }
    let cut: String = first.chars().take(119).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests;
