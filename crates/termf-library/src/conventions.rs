//! Project conventions written in plain language, turned by Claude Code into library items (§11.1).
//!
//! `<config>/projects/<id>/conventions/<slug>/` holds `source.md` (the owner's text), `draft.toml` (a proposal
//! under review) and `pack.toml` (what was applied, and which sentence each item implements). Applied items live
//! in the project's library, so the usual activation switches them on and off. The model's answer is untrusted:
//! it is validated, its paths are checked, and nothing it proposes runs while proposing.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    Item, Kind, Layer, Scope, library_dir, resolve, set_toggle, with_frontmatter, write_file,
};

const DEFAULT_MODEL: &str = "opus";
const DEFAULT_EFFORT: &str = "high";
const MAX_ITEMS: usize = 40;
const MAX_TEXT: usize = 64 << 10;
const MAX_REPO_FILE: usize = 256 << 10;
const PROPOSE_TIMEOUT: Duration = Duration::from_secs(20 * 60);
#[cfg(target_os = "macos")]
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(target_os = "macos")]
const FINDINGS_SHOWN: usize = 5;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    /// Instructions, and a check script when the convention can be verified.
    Rule,
    /// A Claude Code hook: a script run on an event, such as a formatter after each edit.
    Hook,
    Preference,
    Skill,
    Command,
    /// Text added to an existing role, or a new role.
    Role,
    /// A change to a tool's config in a repository, as the file's full new content.
    Repo,
}

impl ItemKind {
    fn library(self) -> Option<Kind> {
        match self {
            ItemKind::Rule | ItemKind::Hook => Some(Kind::Rule),
            ItemKind::Preference => Some(Kind::Preference),
            ItemKind::Skill => Some(Kind::Skill),
            ItemKind::Command => Some(Kind::Command),
            ItemKind::Role => Some(Kind::Role),
            ItemKind::Repo => None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    Advise,
    Warn,
    Block,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Review {
    #[default]
    Pending,
    Accepted,
    Rejected,
}

/// One generated item. Fields that do not apply to its kind are empty.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Proposal {
    pub kind: ItemKind,
    pub name: String,
    /// The sentence of the owner's text it implements.
    pub sentence: String,
    pub description: String,
    /// How it is verified, in words.
    pub how: String,
    pub level: Level,
    /// Roles it applies to; none means all.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Instructions, or what is added to a role.
    #[serde(default)]
    pub body: String,
    /// A rule's check script or a hook's script.
    #[serde(default)]
    pub script: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub matcher: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub content: String,
    /// The repository file when it was proposed, `None` for a new file. Set by TermF, never by the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default)]
    pub review: Review,
}

impl Proposal {
    /// Only a rule that runs a check can warn or block.
    fn checked(&self) -> bool {
        self.kind == ItemKind::Rule && !self.script.is_empty()
    }

    fn same_content(&self, other: &Proposal) -> bool {
        let strip = |p: &Proposal| Proposal {
            review: Review::Pending,
            before: None,
            ..p.clone()
        };
        strip(self) == strip(other)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Answer {
    pub question: String,
    pub answer: String,
}

/// `draft.toml` and `pack.toml`.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Pack {
    pub model: String,
    pub effort: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    /// What the model asked before proposing anything.
    #[serde(default)]
    pub questions: Vec<String>,
    /// Items of the model's answer that failed validation, with the reason.
    #[serde(default)]
    pub dropped: Vec<String>,
    #[serde(default)]
    pub answers: Vec<Answer>,
    #[serde(default)]
    pub items: Vec<Proposal>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Changed,
    Unchanged,
    Removed,
}

/// A draft item against the applied pack.
#[derive(Serialize, Debug, Clone)]
pub struct Diff {
    pub name: String,
    pub change: Change,
    pub previous: Option<Proposal>,
}

#[derive(Serialize, Debug)]
pub struct Convention {
    pub slug: String,
    pub source: String,
    pub pack: Option<Pack>,
    pub draft: Option<Pack>,
    /// The draft against the pack, so a regeneration shows what it would replace.
    pub diff: Vec<Diff>,
    /// Applied items that are switched on in the project.
    pub active: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct Conventions {
    pub model: String,
    pub effort: String,
    pub list: Vec<Convention>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Measure {
    Done {
        count: usize,
        findings: Vec<String>,
    },
    /// No read-only sandbox on this system, so nothing ran.
    Unsupported,
    Failed {
        message: String,
    },
}

#[derive(Serialize, Debug)]
pub struct Impact {
    pub name: String,
    pub repo: String,
    #[serde(flatten)]
    pub measure: Measure,
}

/// `[conventions] model` and `effort` in `termf.toml`.
pub fn settings(config_dir: &Path) -> (String, String) {
    let text = fs::read_to_string(config_dir.join("termf.toml")).unwrap_or_default();
    let table = toml::from_str::<toml::Table>(&text).unwrap_or_default();
    let get = |key: &str, default: &str| {
        table
            .get("conventions")
            .and_then(|c| c.get(key))
            .and_then(toml::Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(default)
            .to_owned()
    };
    (get("model", DEFAULT_MODEL), get("effort", DEFAULT_EFFORT))
}

/// How to start Claude Code from a GUI app: through the login shell, like sessions, so the PATH of the shell
/// profile applies; `program` stays a separate word.
pub fn launcher(program: &str) -> Vec<String> {
    if cfg!(windows) {
        // shortcut: run directly, so an npm `claude.cmd` is not found; go through `cmd /c` when that matters.
        vec![program.into()]
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        [&shell, "-l", "-i", "-c", r#"exec "$0" "$@""#, program]
            .map(String::from)
            .to_vec()
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn dir(config_dir: &Path, project: &str, slug: &str) -> Result<PathBuf> {
    ensure!(valid_name(slug), "invalid convention name {slug:?}");
    termf_core::project_file(config_dir, project)?;
    Ok(config_dir
        .join("projects")
        .join(project)
        .join("conventions")
        .join(slug))
}

fn read_pack(file: &Path) -> Result<Option<Pack>> {
    if !file.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(file)?;
    Ok(Some(
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?,
    ))
}

fn write_pack(file: &Path, pack: &Pack) -> Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    termf_core::write_atomic(file, &toml::to_string(pack)?)
}

pub fn list(config_dir: &Path, project: &str) -> Result<Conventions> {
    termf_core::project_file(config_dir, project)?;
    let root = config_dir
        .join("projects")
        .join(project)
        .join("conventions");
    let mut slugs: Vec<String> = fs::read_dir(&root)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|s| valid_name(s))
                .collect()
        })
        .unwrap_or_default();
    slugs.sort();
    let (model, effort) = settings(config_dir);
    Ok(Conventions {
        model,
        effort,
        list: slugs
            .iter()
            .map(|slug| load(config_dir, project, slug))
            .collect::<Result<_>>()?,
    })
}

pub fn load(config_dir: &Path, project: &str, slug: &str) -> Result<Convention> {
    let dir = dir(config_dir, project, slug)?;
    let source = fs::read_to_string(dir.join("source.md")).unwrap_or_default();
    let pack = read_pack(&dir.join("pack.toml"))?;
    let draft = read_pack(&dir.join("draft.toml"))?;
    let mut diff = Vec::new();
    if let (Some(pack), Some(draft)) = (&pack, &draft)
        && !draft.items.is_empty()
    {
        for item in &draft.items {
            let previous = pack.items.iter().find(|p| p.name == item.name);
            let change = match previous {
                None => Change::Added,
                Some(p) if p.same_content(item) => Change::Unchanged,
                Some(_) => Change::Changed,
            };
            diff.push(Diff {
                name: item.name.clone(),
                change,
                previous: previous.cloned(),
            });
        }
        for p in &pack.items {
            if !draft.items.iter().any(|i| i.name == p.name) {
                diff.push(Diff {
                    name: p.name.clone(),
                    change: Change::Removed,
                    previous: Some(p.clone()),
                });
            }
        }
    }
    let mut active = Vec::new();
    if let Some(pack) = &pack {
        let scope = Scope {
            project: Some(project.into()),
            ..Scope::default()
        };
        let items = resolve(config_dir, &scope)?;
        for p in &pack.items {
            let on = p.kind.library().is_some_and(|kind| {
                items.iter().any(|i| {
                    i.kind == kind && i.name == p.name && i.layer == Layer::Project && i.active
                })
            });
            if on {
                active.push(p.name.clone());
            }
        }
    }
    Ok(Convention {
        slug: slug.into(),
        source,
        pack,
        draft,
        diff,
        active,
    })
}

/// Writes the owner's text, creating the convention if needed.
pub fn save_source(config_dir: &Path, project: &str, slug: &str, text: &str) -> Result<Convention> {
    ensure!(text.len() <= MAX_TEXT, "the text is too long");
    let dir = dir(config_dir, project, slug)?;
    fs::create_dir_all(&dir)?;
    termf_core::write_atomic(&dir.join("source.md"), text)?;
    load(config_dir, project, slug)
}

/// Removes the convention and the library items it generated; repository files it changed stay as they are.
pub fn delete(config_dir: &Path, project: &str, slug: &str) -> Result<()> {
    let dir = dir(config_dir, project, slug)?;
    if let Some(pack) = read_pack(&dir.join("pack.toml"))? {
        let library = library_dir(config_dir, Some(project))?;
        for item in &pack.items {
            remove_item(&library, item)?;
        }
    }
    match fs::remove_dir_all(&dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Asks Claude Code, read-only in the project's repositories, to turn the text into items, or to ask its
/// questions first. `answers` add to those already given. The result becomes the draft.
pub fn propose(
    config_dir: &Path,
    project: &str,
    slug: &str,
    answers: Vec<Answer>,
    claude: &[String],
) -> Result<Convention> {
    let dir = dir(config_dir, project, slug)?;
    let source = fs::read_to_string(dir.join("source.md")).unwrap_or_default();
    ensure!(!source.trim().is_empty(), "write the conventions first");
    let pack = read_pack(&dir.join("pack.toml"))?;
    let draft = read_pack(&dir.join("draft.toml"))?;
    let mut given = draft
        .as_ref()
        .or(pack.as_ref())
        .map(|p| p.answers.clone())
        .unwrap_or_default();
    given.extend(
        answers
            .into_iter()
            .filter(|a| !a.answer.trim().is_empty() && a.answer.len() <= MAX_TEXT),
    );
    let context = Grounding::new(config_dir, project, pack.as_ref())?;
    let (model, effort) = settings(config_dir);
    let prompt = context.prompt(&source, &given, None);
    let (answer, cost) = ask(claude, &context.repos, &model, &effort, &prompt)?;

    let mut next = Pack {
        model,
        effort,
        cost,
        answers: given,
        ..Pack::default()
    };
    let questions = answer
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|q| q.trim().to_owned())
        .filter(|q| !q.is_empty() && q.len() <= 2000)
        .take(10)
        .collect::<Vec<_>>();
    if questions.is_empty() {
        let raw = answer.get("items").and_then(Value::as_array);
        let mut seen = BTreeSet::new();
        for (i, raw) in raw.into_iter().flatten().enumerate() {
            if i == MAX_ITEMS {
                next.dropped
                    .push(format!("only the first {MAX_ITEMS} items are kept"));
                break;
            }
            match context.validate(raw.clone(), &mut seen) {
                Ok(item) => next.items.push(item),
                Err(why) => next.dropped.push(why),
            }
        }
        ensure!(
            !next.items.is_empty() || !next.dropped.is_empty(),
            "Claude Code proposed nothing"
        );
    } else {
        next.questions = questions;
    }
    write_pack(&dir.join("draft.toml"), &next)?;
    load(config_dir, project, slug)
}

/// Regenerates one draft item following the owner's comment; the others stay as they are.
pub fn revise(
    config_dir: &Path,
    project: &str,
    slug: &str,
    name: &str,
    comment: &str,
    claude: &[String],
) -> Result<Convention> {
    ensure!(!comment.trim().is_empty(), "say what to change");
    let dir = dir(config_dir, project, slug)?;
    let source = fs::read_to_string(dir.join("source.md")).unwrap_or_default();
    let mut draft = read_pack(&dir.join("draft.toml"))?.context("no proposal to revise")?;
    let index = draft
        .items
        .iter()
        .position(|i| i.name == name)
        .with_context(|| format!("no proposed item {name}"))?;
    let pack = read_pack(&dir.join("pack.toml"))?;
    let context = Grounding::new(config_dir, project, pack.as_ref())?;
    let prompt = context.prompt(
        &source,
        &draft.answers,
        Some((&draft.items[index], comment.trim())),
    );
    let (answer, cost) = ask(claude, &context.repos, &draft.model, &draft.effort, &prompt)?;
    let raw = answer
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .cloned();
    let Some(raw) = raw else {
        let asked = answer["questions"].as_array().into_iter().flatten();
        let asked: Vec<&str> = asked.filter_map(Value::as_str).collect();
        bail!("Claude Code did not revise the item: {}", asked.join(" "));
    };
    let mut seen: BTreeSet<String> = draft
        .items
        .iter()
        .filter(|i| i.name != name)
        .map(|i| i.name.clone())
        .collect();
    draft.items[index] = context
        .validate(raw, &mut seen)
        .map_err(anyhow::Error::msg)?;
    draft.cost = match (draft.cost, cost) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    };
    write_pack(&dir.join("draft.toml"), &draft)?;
    load(config_dir, project, slug)
}

/// Accepts or rejects a draft item, or changes its level.
pub fn review(
    config_dir: &Path,
    project: &str,
    slug: &str,
    name: &str,
    review: Option<Review>,
    level: Option<Level>,
) -> Result<Convention> {
    let dir = dir(config_dir, project, slug)?;
    let mut draft = read_pack(&dir.join("draft.toml"))?.context("no proposal to review")?;
    let item = draft
        .items
        .iter_mut()
        .find(|i| i.name == name)
        .with_context(|| format!("no proposed item {name}"))?;
    if let Some(review) = review {
        item.review = review;
    }
    if let Some(level) = level {
        ensure!(
            item.checked() || level == Level::Advise,
            "only a rule with a check can warn or block"
        );
        item.level = level;
    }
    write_pack(&dir.join("draft.toml"), &draft)?;
    load(config_dir, project, slug)
}

pub fn discard(config_dir: &Path, project: &str, slug: &str) -> Result<Convention> {
    let dir = dir(config_dir, project, slug)?;
    match fs::remove_file(dir.join("draft.toml")) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    load(config_dir, project, slug)
}

/// Writes the accepted items into the project's library, and the accepted repository changes into the
/// repositories' working trees (never committed), then makes the draft the pack. Every item is checked before
/// anything is written: a repository file changed since the proposal stops the whole apply.
pub fn apply(config_dir: &Path, project: &str, slug: &str) -> Result<Convention> {
    let dir = dir(config_dir, project, slug)?;
    let draft = read_pack(&dir.join("draft.toml"))?.context("no proposal to apply")?;
    ensure!(draft.questions.is_empty(), "answer the questions first");
    ensure!(
        draft.items.iter().all(|i| i.review != Review::Pending),
        "accept or reject every item first"
    );
    let pack = read_pack(&dir.join("pack.toml"))?;
    let context = Grounding::new(config_dir, project, pack.as_ref())?;
    let library = library_dir(config_dir, Some(project))?;
    let accepted: Vec<Proposal> = draft
        .items
        .iter()
        .filter(|i| i.review == Review::Accepted)
        .cloned()
        .collect();

    let mut writes = Vec::new();
    for item in &accepted {
        if item.kind == ItemKind::Repo {
            let path = context.repo_file(item)?;
            let now = read_text(&path)?;
            ensure!(
                now == item.before,
                "{} changed since the proposal: propose again",
                item.path
            );
            continue;
        }
        context.check_owner(item)?;
        writes.push((item, context.files(item)));
    }

    if let Some(pack) = &pack {
        for old in &pack.items {
            if !accepted
                .iter()
                .any(|i| i.name == old.name && i.kind == old.kind)
            {
                remove_item(&library, old)?;
            }
        }
    }
    for (item, files) in writes {
        remove_item(&library, item)?;
        for (rel, text, script) in files {
            let path = library.join(rel);
            write_file(&path, text.as_bytes())?;
            if script {
                make_executable(&path)?;
            }
        }
    }
    for item in accepted.iter().filter(|i| i.kind == ItemKind::Repo) {
        write_file(&context.repo_file(item)?, item.content.as_bytes())?;
    }
    let applied = Pack {
        items: accepted,
        dropped: Vec::new(),
        questions: Vec::new(),
        ..draft
    };
    write_pack(&dir.join("pack.toml"), &applied)?;
    fs::remove_file(dir.join("draft.toml"))?;
    load(config_dir, project, slug)
}

/// Switches on or off, or sets the level of, the applied items that implement `sentence`.
pub fn switch(
    config_dir: &Path,
    project: &str,
    slug: &str,
    sentence: &str,
    enabled: Option<bool>,
    level: Option<Level>,
) -> Result<Convention> {
    let dir = dir(config_dir, project, slug)?;
    let mut pack = read_pack(&dir.join("pack.toml"))?.context("nothing applied yet")?;
    ensure!(
        pack.items.iter().any(|i| i.sentence == sentence),
        "no item implements {sentence:?}"
    );
    if let Some(on) = enabled {
        for item in pack.items.iter().filter(|i| i.sentence == sentence) {
            if let Some(kind) = item.kind.library() {
                set_toggle(config_dir, Some(project), kind, &item.name, Some(on))?;
            }
        }
    }
    if let Some(level) = level {
        let context = Grounding::new(config_dir, project, Some(&pack))?;
        let library = library_dir(config_dir, Some(project))?;
        for item in pack
            .items
            .iter_mut()
            .filter(|i| i.sentence == sentence && i.checked())
        {
            item.level = level;
            for (rel, text, _) in context.files(item).into_iter().filter(|f| !f.2) {
                write_file(&library.join(rel), text.as_bytes())?;
            }
        }
        write_pack(&dir.join("pack.toml"), &pack)?;
    }
    load(config_dir, project, slug)
}

/// Runs the check scripts of the draft's accepted rules (else of the applied ones) on each repository, in a
/// read-only sandbox, and counts what they report.
pub fn impact(config_dir: &Path, project: &str, slug: &str) -> Result<Vec<Impact>> {
    let dir = dir(config_dir, project, slug)?;
    let items = match read_pack(&dir.join("draft.toml"))? {
        Some(draft) if !draft.items.is_empty() => draft
            .items
            .into_iter()
            .filter(|i| i.review == Review::Accepted)
            .collect(),
        _ => read_pack(&dir.join("pack.toml"))?
            .map(|p| p.items)
            .unwrap_or_default(),
    };
    let repos = project_repos(config_dir, project)?;
    let mut results = Vec::new();
    for item in items.iter().filter(|i| i.checked()) {
        for (name, path) in &repos {
            results.push(Impact {
                name: item.name.clone(),
                repo: name.clone(),
                measure: check(&item.script, path),
            });
        }
    }
    Ok(results)
}

fn project_repos(config_dir: &Path, project: &str) -> Result<Vec<(String, PathBuf)>> {
    let project = termf_core::load_projects(config_dir)?
        .into_iter()
        .find(|p| p.id == project)
        .with_context(|| format!("unknown project {project}"))?;
    Ok(project
        .repos
        .into_iter()
        .map(|r| (r.name, PathBuf::from(r.path)))
        .collect())
}

/// What validation and file generation need to know about the project.
struct Grounding {
    project: String,
    repos: Vec<(String, PathBuf)>,
    /// The library as the project sees it.
    items: Vec<Item>,
    /// The library without the project's layer, which a role adjustment adds to.
    base: Vec<Item>,
    /// Names of the items the applied pack generated, which it may rewrite.
    owned: Vec<String>,
}

impl Grounding {
    fn new(config_dir: &Path, project: &str, pack: Option<&Pack>) -> Result<Grounding> {
        let repos = project_repos(config_dir, project)?;
        ensure!(!repos.is_empty(), "this project has no repository");
        let scope = Scope {
            project: Some(project.into()),
            ..Scope::default()
        };
        Ok(Grounding {
            project: project.into(),
            repos,
            items: resolve(config_dir, &scope)?,
            base: resolve(config_dir, &Scope::default())?,
            owned: pack
                .map(|p| p.items.iter().map(|i| i.name.clone()).collect())
                .unwrap_or_default(),
        })
    }

    fn prompt(
        &self,
        source: &str,
        answers: &[Answer],
        revise: Option<(&Proposal, &str)>,
    ) -> String {
        let mut p = String::from(
            "You turn a software project's conventions, written in plain language by its owner, into concrete items for TermF, \
             a tool that runs Claude Code agents on the project's repositories.\n\n",
        );
        p += "Repositories (read-only: use Read, Grep and Glob; never modify anything):\n";
        for (i, (name, path)) in self.repos.iter().enumerate() {
            let here = if i == 0 { " (current directory)" } else { "" };
            p += &format!("- {name}: {}{here}\n", path.display());
        }
        p += "\nGround every item in what the repositories already use: read the manifests (package.json, Cargo.toml, \
               pyproject.toml…), the lint and format configs (eslint, prettier, biome, stylelint, editorconfig, rustfmt, \
               clippy, ruff…), CLAUDE.md or AGENTS.md, and a few representative source files.\n\n";
        p += &format!(
            "The conventions, as the owner wrote them:\n<<<\n{}\n>>>\n\n",
            source.trim()
        );
        if !answers.is_empty() {
            p += "The owner's answers to earlier questions:\n";
            for a in answers {
                p += &format!("- Q: {}\n  A: {}\n", a.question.trim(), a.answer.trim());
            }
            p += "\n";
        }
        p += "How to answer:\n\
              1. Scope: implement only what the text says. If a sentence is ambiguous or incomplete in a way that changes what \
              you would generate, return only `questions` (and no items): ask before proposing.\n\
              2. Prefer the most deterministic implementation: a rule of a tool the repository already uses (kind `repo`) over a \
              check script (kind `rule` with a `script`), over a hook (kind `hook`), over instructions only (kind `preference`, \
              or `rule` without script). Several items may implement one sentence.\n\
              3. Kinds and the fields they use:\n\
              - `rule`: `body` = instructions for the agents; `script` = optional check script; `level` = `advise` without \
              script, else `warn` or `block`; `roles` = the roles it applies to, empty for all.\n\
              - `hook`: a Claude Code hook; `event` (PostToolUse, PreToolUse, Stop…), `matcher` (tool names such as \
              `Edit|Write`, or empty), `script` = POSIX sh run on the event, `body` = what it does.\n\
              - `preference`: `body` = coding-style instructions; `roles` as for a rule.\n\
              - `skill`: `body` = a Claude Code skill (how to do a task the conventional way).\n\
              - `command`: `body` = a Claude Code slash command prompt (may use $ARGUMENTS).\n\
              - `role`: `name` = an existing role to adjust, `body` = the text added to it.\n\
              - `repo`: `repo` = a repository name from the list, `path` = a file relative to it, `content` = the file's \
              complete new content (read the current file first and keep everything else).\n\
              4. A check script is POSIX sh, run at the repository root, reading the paths to check on stdin, one per line \
              (every tracked file when measuring the current state). It must not modify anything nor use the network. It \
              prints one finding per line as `path:line: message` and exits 0 when clean, 1 when it found violations.\n\
              5. Every item gives the `sentence` of the text it implements (verbatim), a one-line `description`, and `how` it \
              is verified.\n\
              6. `name`: lowercase letters, digits and dashes, unique in your answer.\n\
              7. Write descriptions, instructions and questions in the language of the conventions text.\n\
              8. Fields that do not apply to an item are empty strings or empty lists.\n\n";
        let roles: Vec<String> = self
            .items
            .iter()
            .filter(|i| i.kind == Kind::Role)
            .map(|i| format!("{} ({})", i.name, i.description.as_deref().unwrap_or("")))
            .collect();
        p += &format!("Roles: {}\n", roles.join("; "));
        let taken: Vec<String> = self
            .items
            .iter()
            .filter(|i| i.kind != Kind::Role && !self.owned.contains(&i.name))
            .map(|i| i.name.clone())
            .collect();
        p += &format!("Names already taken, not to reuse: {}\n", taken.join(", "));
        if !self.owned.is_empty() {
            p += &format!(
                "Names of the current version's items, to keep when an item still applies: {}\n",
                self.owned.join(", ")
            );
        }
        if let Some((item, comment)) = revise {
            let mut shown = item.clone();
            shown.before = None;
            let shown = serde_json::to_value(&shown).unwrap_or_default();
            p += &format!(
                "\nRevise only this item, following the owner's request. Return exactly one item in `items` and no \
                 questions; keep its name unless the request says otherwise.\nItem: {shown}\nRequest: {comment}\n"
            );
        }
        p
    }

    /// Checks one item of the model's answer and normalizes it to its kind. The error says why it was dropped.
    fn validate(&self, raw: Value, seen: &mut BTreeSet<String>) -> Result<Proposal, String> {
        let label = raw
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .chars()
            .take(64)
            .collect::<String>();
        let fail = |why: &str| Err(format!("{label}: {why}"));
        let Ok(mut item) = serde_json::from_value::<Proposal>(raw) else {
            return fail("not a valid item");
        };
        item.before = None;
        item.review = Review::Pending;
        for text in [
            &mut item.name,
            &mut item.sentence,
            &mut item.how,
            &mut item.event,
            &mut item.matcher,
            &mut item.repo,
            &mut item.path,
        ] {
            *text = text.trim().to_owned();
        }
        item.description = item
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !valid_name(&item.name) {
            return fail("invalid name");
        }
        if !seen.insert(item.name.clone()) {
            return fail("name used twice");
        }
        if item.sentence.is_empty() || item.description.is_empty() {
            return fail("no sentence or description");
        }
        if [&item.sentence, &item.description, &item.how, &item.matcher]
            .iter()
            .any(|t| t.len() > 2000)
            || [&item.body, &item.script]
                .iter()
                .any(|t| t.len() > MAX_TEXT)
            || item.content.len() > MAX_REPO_FILE
        {
            return fail("too long");
        }
        if item.roles.iter().any(|r| !valid_name(r)) {
            return fail("invalid role name");
        }
        let keep_script = matches!(item.kind, ItemKind::Rule | ItemKind::Hook);
        let is_hook = item.kind == ItemKind::Hook;
        let is_repo = item.kind == ItemKind::Repo;
        if !keep_script {
            item.script.clear();
        }
        if !is_hook {
            item.event.clear();
            item.matcher.clear();
        }
        if !is_repo {
            item.repo.clear();
            item.path.clear();
            item.content.clear();
        }
        if !matches!(item.kind, ItemKind::Rule | ItemKind::Preference) {
            item.roles.clear();
        }
        if !item.checked() && item.kind != ItemKind::Hook {
            item.level = Level::Advise;
        }
        match item.kind {
            ItemKind::Hook => {
                if item.script.trim().is_empty() {
                    return fail("a hook needs a script");
                }
                if !termf_core::claude::HOOK_EVENTS.contains(&item.event.as_str())
                    && !["SubagentStop", "PreCompact"].contains(&item.event.as_str())
                {
                    return fail("unknown hook event");
                }
            }
            ItemKind::Repo => {
                if item.content.is_empty() {
                    return fail("no content");
                }
                let path = self
                    .repo_file(&item)
                    .map_err(|e| format!("{label}: {e:#}"))?;
                item.before = read_text(&path).map_err(|e| format!("{label}: {e:#}"))?;
                if item.before.as_deref() == Some(item.content.as_str()) {
                    return fail("changes nothing");
                }
            }
            _ if item.body.trim().is_empty() => return fail("no instructions"),
            _ => {}
        }
        self.check_owner(&item)
            .map_err(|e| format!("{label}: {e:#}"))?;
        Ok(item)
    }

    /// A generated item may only replace one this convention generated, or add to an existing role.
    fn check_owner(&self, item: &Proposal) -> Result<()> {
        let Some(kind) = item.kind.library() else {
            return Ok(());
        };
        let Some(existing) = self
            .items
            .iter()
            .find(|i| i.kind == kind && i.name == item.name)
        else {
            return Ok(());
        };
        let owned = existing.layer == Layer::Project && self.owned.contains(&item.name);
        let adjusts = kind == Kind::Role && existing.layer < Layer::Project;
        ensure!(
            owned || adjusts,
            "{kind} {} already exists in the {} library",
            item.name,
            existing.layer
        );
        Ok(())
    }

    /// The file a repository change writes, refusing anything outside the repository or in `.git`.
    fn repo_file(&self, item: &Proposal) -> Result<PathBuf> {
        let (_, repo) = self
            .repos
            .iter()
            .find(|(name, _)| *name == item.repo)
            .with_context(|| format!("{} is not a repository of {}", item.repo, self.project))?;
        safe_join(repo, &item.path)
    }

    /// The item's files in the project library: path relative to it, content, and whether it is a script.
    fn files(&self, item: &Proposal) -> Vec<(String, String, bool)> {
        let name = &item.name;
        let mut meta = Map::new();
        meta.insert("name".into(), name.as_str().into());
        meta.insert("description".into(), item.description.as_str().into());
        if !item.roles.is_empty() {
            meta.insert("applies_to".into(), json!(item.roles));
        }
        let body = format!("{}\n", item.body.trim_end());
        let script = format!("{}\n", item.script.trim_end());
        match item.kind {
            ItemKind::Rule => {
                meta.insert("level".into(), json!(item.level));
                let mut files = Vec::new();
                if item.checked() {
                    meta.insert("check".into(), json!({ "run": "check.sh" }));
                    files.push((format!("rules/{name}/check.sh"), script, true));
                }
                files.push((
                    format!("rules/{name}/rule.md"),
                    with_frontmatter(&meta, &body),
                    false,
                ));
                files
            }
            ItemKind::Hook => {
                meta.insert("level".into(), json!(item.level));
                let mut hook = json!({ "event": item.event, "run": "hook.sh" });
                if !item.matcher.is_empty() {
                    hook["matcher"] = item.matcher.as_str().into();
                }
                meta.insert("hook".into(), hook);
                vec![
                    (
                        format!("rules/{name}/rule.md"),
                        with_frontmatter(&meta, &body),
                        false,
                    ),
                    (format!("rules/{name}/hook.sh"), script, true),
                ]
            }
            ItemKind::Preference => vec![(
                format!("preferences/{name}.md"),
                with_frontmatter(&meta, &body),
                false,
            )],
            ItemKind::Skill => vec![(
                format!("skills/{name}/SKILL.md"),
                with_frontmatter(&meta, &body),
                false,
            )],
            ItemKind::Command => {
                meta.remove("name");
                vec![(
                    format!("commands/{name}.md"),
                    with_frontmatter(&meta, &body),
                    false,
                )]
            }
            ItemKind::Role => {
                let base = self
                    .base
                    .iter()
                    .find(|i| i.kind == Kind::Role && i.name == *name);
                let text = match base {
                    Some(base) => with_frontmatter(
                        &base.meta,
                        &format!("{}\n\n{}", base.body.trim_end(), body),
                    ),
                    None => with_frontmatter(&meta, &body),
                };
                vec![(format!("roles/{name}.md"), text, false)]
            }
            ItemKind::Repo => Vec::new(),
        }
    }
}

fn remove_item(library: &Path, item: &Proposal) -> Result<()> {
    let path = match item.kind.library() {
        Some(Kind::Rule) => library.join("rules").join(&item.name),
        Some(Kind::Skill) => library.join("skills").join(&item.name),
        Some(kind) => library.join(kind.dir()).join(format!("{}.md", item.name)),
        None => return Ok(()),
    };
    let result = if path.is_dir() {
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    };
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// `rel` inside `root`: relative, no `..`, no `.git`, and never through a symbolic link.
fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let path = Path::new(rel);
    ensure!(
        !rel.is_empty() && path.is_relative(),
        "invalid path {rel:?}"
    );
    let mut out = root.to_path_buf();
    for part in path.components() {
        match part {
            // Case-insensitive file systems and Windows' trailing dots would still reach `.git`.
            Component::Normal(name)
                if name
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .trim_end_matches(['.', ' '])
                    != ".git" =>
            {
                out.push(name);
                ensure!(!out.is_symlink(), "{rel} goes through a link");
            }
            _ => bail!("invalid path {rel:?}"),
        }
    }
    Ok(out)
}

/// A text file's content, `None` when it does not exist.
fn read_text(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(meta) => {
            ensure!(meta.is_file(), "{} is not a file", path.display());
            ensure!(
                meta.len() as usize <= MAX_REPO_FILE,
                "{} is too large",
                path.display()
            );
            Ok(Some(fs::read_to_string(path).with_context(|| {
                format!("{} is not text", path.display())
            })?))
        }
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::set_permissions(
        path,
        fs::Permissions::from_mode(0o755),
    )?)
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

/// The structured answer the model must give.
fn schema() -> Value {
    let text = json!({ "type": "string" });
    let item = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "name", "sentence", "description", "how", "level", "roles", "body", "script",
                     "event", "matcher", "repo", "path", "content"],
        "properties": {
            "kind": { "enum": ["rule", "hook", "preference", "skill", "command", "role", "repo"] },
            "name": text, "sentence": text, "description": text, "how": text,
            "level": { "enum": ["advise", "warn", "block"] },
            "roles": { "type": "array", "items": text },
            "body": text, "script": text, "event": text, "matcher": text,
            "repo": text, "path": text, "content": text,
        },
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["questions", "items"],
        "properties": {
            "questions": { "type": "array", "items": text },
            "items": { "type": "array", "items": item },
        },
    })
}

/// Runs Claude Code headless: read-only tools, no project settings, hooks, MCP servers or plugins, nothing saved.
/// Returns the structured answer and what it cost.
fn ask(
    claude: &[String],
    repos: &[(String, PathBuf)],
    model: &str,
    effort: &str,
    prompt: &str,
) -> Result<(Value, Option<f64>)> {
    let (program, lead) = claude.split_first().context("no Claude Code command")?;
    let mut cmd = Command::new(program);
    cmd.args(lead)
        .args(["-p", "--output-format", "json", "--json-schema"])
        .arg(schema().to_string())
        .args(["--model", model, "--effort", effort])
        .args(["--tools", "Read,Grep,Glob", "--permission-mode", "dontAsk"])
        .args(["--restricted", "--safe-mode", "--strict-mcp-config"])
        .arg("--no-session-persistence")
        .current_dir(&repos[0].1);
    for (_, path) in &repos[1..] {
        cmd.arg("--add-dir").arg(path);
    }
    let (status, stdout, stderr) =
        run(cmd, prompt.as_bytes(), PROPOSE_TIMEOUT).context("cannot run Claude Code")?;
    let text = String::from_utf8_lossy(&stdout);
    // A shell profile may print before Claude Code: its answer is the last JSON line.
    let Some(result) = text.lines().rev().find_map(|l| {
        serde_json::from_str::<Value>(l.trim())
            .ok()
            .filter(Value::is_object)
    }) else {
        bail!(
            "Claude Code gave no answer ({status}): {}",
            tail(&String::from_utf8_lossy(&stderr))
        );
    };
    let cost = result["total_cost_usd"].as_f64();
    match result.get("structured_output") {
        Some(answer) if result["is_error"] != true && answer.is_object() => {
            Ok((answer.clone(), cost))
        }
        _ => bail!(
            "Claude Code: {}",
            result["result"].as_str().unwrap_or("no structured answer")
        ),
    }
}

fn tail(text: &str) -> String {
    let text = text.trim();
    let start = text.char_indices().rev().nth(500).map_or(0, |(i, _)| i);
    text[start..].into()
}

/// Runs `cmd` with `input` on stdin. Output goes to files so a chatty child never blocks; after `timeout` the
/// child and what it started are killed.
fn run(
    mut cmd: Command,
    input: &[u8],
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let scratch = scratch_dir()?;
    let result = (|| {
        let (stdin, stdout, stderr) =
            (scratch.join("in"), scratch.join("out"), scratch.join("err"));
        fs::write(&stdin, input)?;
        cmd.stdin(fs::File::open(&stdin)?)
            .stdout(fs::File::create(&stdout)?)
            .stderr(fs::File::create(&stderr)?);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd.spawn()?;
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() > deadline {
                kill_group(&mut child);
                bail!("timed out after {} s", timeout.as_secs());
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        Ok((status, fs::read(&stdout)?, fs::read(&stderr)?))
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{}", child.id())])
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

fn scratch_dir() -> Result<PathBuf> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "termf-conventions-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Writes are denied everywhere but a private temporary folder, and so is the network.
#[cfg(target_os = "macos")]
const SANDBOX: &str = r#"(version 1)(allow default)(deny file-write*)(deny network*)
(allow file-write* (literal "/dev/null") (regex #"^/dev/(fd/|tty)") (subpath (param "TMP")))"#;

/// Runs a check script on a repository's tracked files without letting it change anything.
fn check(script: &str, repo: &Path) -> Measure {
    // shortcut: only macOS has a read-only sandbox built in (sandbox-exec); elsewhere the impact is not measured
    // until TermF sandboxes scripts there too (bubblewrap, a job object…).
    if !cfg!(target_os = "macos") {
        return Measure::Unsupported;
    }
    match measure(script, repo) {
        Ok(measure) => measure,
        Err(e) => Measure::Failed {
            message: format!("{e:#}"),
        },
    }
}

#[cfg(not(target_os = "macos"))]
fn measure(_: &str, _: &Path) -> Result<Measure> {
    Ok(Measure::Unsupported)
}

#[cfg(target_os = "macos")]
fn measure(script: &str, repo: &Path) -> Result<Measure> {
    let tracked = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z"])
        .output()
        .context("cannot list the repository's files")?;
    ensure!(
        tracked.status.success(),
        "{} is not a git repository",
        repo.display()
    );
    let files: Vec<u8> = tracked
        .stdout
        .into_iter()
        .map(|b| if b == 0 { b'\n' } else { b })
        .collect();
    let scratch = scratch_dir()?;
    let result = (|| {
        let tmp = scratch.join("tmp");
        fs::create_dir_all(&tmp)?;
        let file = scratch.join("check.sh");
        fs::write(&file, script)?;
        let mut cmd = Command::new("sandbox-exec");
        cmd.arg("-D")
            .arg(format!("TMP={}", tmp.canonicalize()?.display()))
            .args(["-p", SANDBOX, "env"])
            .arg(format!("TMPDIR={}/", tmp.display()))
            .arg("/bin/sh")
            .arg(&file)
            .current_dir(repo);
        let (status, stdout, stderr) = run(cmd, &files, CHECK_TIMEOUT)?;
        let findings: Vec<String> = String::from_utf8_lossy(&stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        Ok(match status.code() {
            Some(0 | 1) => Measure::Done {
                count: findings.len(),
                findings: findings.into_iter().take(FINDINGS_SHOWN).collect(),
            },
            _ => Measure::Failed {
                message: format!("{status}: {}", tail(&String::from_utf8_lossy(&stderr))),
            },
        })
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{put, temp_dir};

    /// A config with project `acme` and its repository `api`, a git repository with a tracked `src/App.tsx`.
    fn setup(name: &str) -> (PathBuf, PathBuf) {
        let dir = temp_dir(name);
        let config = dir.join("config");
        let repo = dir.join("api");
        put(
            &repo.join("src/App.tsx"),
            "const a = <div style={{color: 'red'}} />;\n",
        );
        put(&repo.join("package.json"), "{ \"name\": \"api\" }\n");
        let git = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        put(
            &config.join("projects/acme.toml"),
            &format!(
                "name = \"Acme\"\n[[repos]]\npath = {:?}\n",
                repo.to_string_lossy()
            ),
        );
        (config, repo)
    }

    /// A stand-in for Claude Code that records its arguments and stdin, then prints `answer` as Claude Code would.
    #[cfg(unix)]
    fn fake_claude(dir: &Path, answer: &Value) -> Vec<String> {
        let out = json!({ "type": "result", "is_error": false, "total_cost_usd": 0.25, "structured_output": answer });
        let script = dir.join("fake-claude.sh");
        put(
            &script,
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{0}/args.txt\"\ncat > \"{0}/prompt.txt\"\necho 'profile noise'\ncat <<'EOF'\n{1}\nEOF\n",
                dir.display(),
                out
            ),
        );
        vec!["/bin/sh".into(), script.to_string_lossy().into_owned()]
    }

    #[cfg(unix)]
    fn item(kind: &str, name: &str, sentence: &str) -> Value {
        json!({
            "kind": kind, "name": name, "sentence": sentence, "description": format!("{name} does it"),
            "how": "checked", "level": "block", "roles": [], "body": "Do it.", "script": "", "event": "",
            "matcher": "", "repo": "", "path": "", "content": "",
        })
    }

    #[cfg(unix)]
    const TEXT: &str = "Pas de CSS inline. Composants en PascalCase.";

    #[cfg(unix)]
    fn accept_all(config: &Path) {
        let draft = load(config, "acme", "front").unwrap().draft.unwrap();
        for i in &draft.items {
            review(
                config,
                "acme",
                "front",
                &i.name,
                Some(Review::Accepted),
                None,
            )
            .unwrap();
        }
    }

    #[test]
    fn writes_frontmatter_that_reads_back() {
        let mut meta = Map::new();
        meta.insert(
            "description".into(),
            "Pas de « style » : jamais\n\"vraiment\"".into(),
        );
        meta.insert("applies_to".into(), json!(["front", "back"]));
        meta.insert("check".into(), json!({ "run": "check.sh" }));
        meta.insert("level".into(), "block".into());
        let text = with_frontmatter(&meta, "Body\n");
        let item = crate::parse(
            Kind::Rule,
            "x".into(),
            &text,
            Layer::Global,
            None,
            String::new(),
        );
        assert_eq!(item.error, None, "{text}");
        assert_eq!(item.meta, meta);
        assert_eq!(item.body, "Body\n");
    }

    // The stand-in for Claude Code is a shell script.
    #[cfg(unix)]
    #[test]
    fn proposes_validates_and_applies_into_the_project_library() {
        use crate::{explain, materialize};
        let (config, repo) = setup("conventions-apply");
        let dir = config.parent().unwrap().to_path_buf();
        save_source(&config, "acme", "front", TEXT).unwrap();

        let mut rule = item("rule", "no-inline-css", "Pas de CSS inline.");
        rule["script"] = "#!/bin/sh\ngrep -n 'style={{' $(cat) && exit 1\nexit 0\n".into();
        rule["roles"] = json!(["front"]);
        let mut eslint = item("repo", "eslint-no-style", "Pas de CSS inline.");
        eslint["repo"] = "api".into();
        eslint["path"] = ".eslintrc.json".into();
        eslint["content"] = "{ \"rules\": {} }\n".into();
        let mut hook = item("hook", "format", "Composants en PascalCase.");
        hook["event"] = "PostToolUse".into();
        hook["matcher"] = "Edit|Write".into();
        hook["script"] = "#!/bin/sh\nexit 0\n".into();
        let mut role = item("role", "front", "Composants en PascalCase.");
        role["body"] = "Name components in PascalCase.".into();
        let mut prompt_only = item("rule", "pascal-case", "Composants en PascalCase.");
        prompt_only["level"] = "block".into();
        let mut injected = item("preference", "sneaky", "x");
        injected["before"] = "forged".into();
        injected["review"] = "accepted".into();
        let mut bad_path = item("repo", "escape", "x");
        bad_path["repo"] = "api".into();
        bad_path["path"] = "../outside.txt".into();
        bad_path["content"] = "x".into();
        let mut git_hook = bad_path.clone();
        git_hook["name"] = "git-hook".into();
        git_hook["path"] = ".git/hooks/pre-commit".into();
        let mut other_repo = bad_path.clone();
        other_repo["name"] = "elsewhere".into();
        other_repo["repo"] = "web".into();
        other_repo["path"] = "x.txt".into();
        let answer = json!({ "questions": [], "items": [
            rule, eslint, hook, role, prompt_only, injected,
            item("preference", "canary", "x"),
            item("skill", "Bad Name", "x"),
            item("command", "no-inline-css", "x"),
            bad_path, git_hook, other_repo,
            json!({ "kind": "virus" }),
        ]});
        let claude = fake_claude(&dir, &answer);

        let convention = propose(&config, "acme", "front", Vec::new(), &claude).unwrap();
        let args = fs::read_to_string(dir.join("args.txt")).unwrap();
        for flag in [
            "--tools\nRead,Grep,Glob\n",
            "--restricted",
            "--safe-mode",
            "--strict-mcp-config",
            "--json-schema",
            "--model\nopus\n",
            "--effort\nhigh\n",
            "dontAsk",
        ] {
            assert!(args.contains(flag), "{flag} missing in {args}");
        }
        let prompt = fs::read_to_string(dir.join("prompt.txt")).unwrap();
        assert!(prompt.contains(TEXT) && prompt.contains(&repo.display().to_string()));

        let draft = convention.draft.unwrap();
        assert_eq!(draft.cost, Some(0.25));
        let names: Vec<_> = draft.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "no-inline-css",
                "eslint-no-style",
                "format",
                "front",
                "pascal-case",
                "sneaky"
            ]
        );
        assert_eq!(draft.dropped.len(), 7, "{:?}", draft.dropped);
        for why in [
            "canary: preference canary already exists in the shipped library",
            "Bad Name: invalid name",
            "no-inline-css: name used twice",
            "escape: invalid path",
            "git-hook: invalid path",
            "elsewhere: web is not a repository of acme",
            "?: not a valid item",
        ] {
            assert!(
                draft.dropped.iter().any(|d| d.starts_with(why)),
                "{why} not in {:?}",
                draft.dropped
            );
        }
        let sneaky = draft.items.iter().find(|i| i.name == "sneaky").unwrap();
        assert_eq!(
            (sneaky.before.as_deref(), sneaky.review),
            (None, Review::Pending),
            "TermF's fields are not the model's"
        );
        let pascal = draft
            .items
            .iter()
            .find(|i| i.name == "pascal-case")
            .unwrap();
        assert_eq!(pascal.level, Level::Advise, "no check, no blocking");
        assert_eq!(draft.items[1].before, None, "a new file");

        assert!(
            apply(&config, "acme", "front")
                .unwrap_err()
                .to_string()
                .contains("accept or reject")
        );
        assert!(
            review(
                &config,
                "acme",
                "front",
                "pascal-case",
                None,
                Some(Level::Block)
            )
            .is_err()
        );
        accept_all(&config);
        review(
            &config,
            "acme",
            "front",
            "sneaky",
            Some(Review::Rejected),
            None,
        )
        .unwrap();
        review(
            &config,
            "acme",
            "front",
            "no-inline-css",
            None,
            Some(Level::Warn),
        )
        .unwrap();
        let measured = impact(&config, "acme", "front").unwrap();
        assert_eq!(measured.len(), 1);
        if cfg!(target_os = "macos") {
            assert_eq!(
                measured[0].measure,
                Measure::Done {
                    count: 1,
                    findings: vec![
                        "src/App.tsx:1:const a = <div style={{color: 'red'}} />;".into()
                    ]
                }
            );
        } else {
            assert_eq!(measured[0].measure, Measure::Unsupported);
        }

        let applied = apply(&config, "acme", "front").unwrap();
        assert!(applied.draft.is_none());
        let pack = applied.pack.unwrap();
        assert_eq!(pack.items.len(), 5);
        assert_eq!(
            applied.active,
            ["no-inline-css", "format", "front", "pascal-case"]
        );
        assert_eq!(
            fs::read_to_string(repo.join(".eslintrc.json")).unwrap(),
            "{ \"rules\": {} }\n"
        );
        let status = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["log", "--oneline"])
            .output()
            .unwrap();
        assert!(
            !status.status.success() || status.stdout.is_empty(),
            "nothing committed"
        );

        let library = config.join("projects/acme/library");
        let scope = Scope {
            project: Some("acme".into()),
            ..Scope::default()
        };
        let rule = explain(&config, &scope, Kind::Rule, "no-inline-css").unwrap();
        assert_eq!((rule.layer, rule.active), (Layer::Project, true));
        assert_eq!(rule.meta["level"], "warn");
        assert_eq!(rule.meta["check"]["run"], "check.sh");
        assert_eq!(rule.applies_to, ["front"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(library.join("rules/no-inline-css/check.sh"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let front = explain(&config, &scope, Kind::Role, "front").unwrap();
        assert_eq!(front.shadows, [Layer::Shipped]);
        assert!(
            front
                .body
                .trim_end()
                .ends_with("Name components in PascalCase.")
        );
        assert_eq!(
            front.meta["name"], "front",
            "the shipped role is kept and added to"
        );
        assert!(!library.join("preferences/sneaky.md").exists(), "rejected");

        let session = dir.join("session");
        put(
            &session.join("plugin/hooks/hooks.json"),
            "{\"hooks\":{\"Stop\":[{\"hooks\":[]}]}}",
        );
        let prompt = materialize(&config, &scope, &session).unwrap().unwrap();
        assert!(
            fs::read_to_string(prompt)
                .unwrap()
                .contains("## no-inline-css")
        );
        let hooks: Value = serde_json::from_str(
            &fs::read_to_string(session.join("plugin/hooks/hooks.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            hooks["hooks"]["Stop"].as_array().unwrap().len(),
            1,
            "status hooks kept"
        );
        let format = &hooks["hooks"]["PostToolUse"][0];
        assert_eq!(format["matcher"], "Edit|Write");
        let script = PathBuf::from(format["hooks"][0]["args"][0].as_str().unwrap());
        assert!(
            script.starts_with(session.join("plugin")) && script.is_file(),
            "{script:?}"
        );

        switch(
            &config,
            "acme",
            "front",
            "Pas de CSS inline.",
            Some(false),
            Some(Level::Block),
        )
        .unwrap();
        let rule = explain(&config, &scope, Kind::Rule, "no-inline-css").unwrap();
        assert!(!rule.active);
        assert_eq!(rule.meta["level"], "block");
        let after = load(&config, "acme", "front").unwrap();
        assert_eq!(after.active, ["format", "front", "pascal-case"]);
        assert!(
            after
                .pack
                .unwrap()
                .items
                .iter()
                .any(|i| i.name == "no-inline-css" && i.level == Level::Block)
        );

        delete(&config, "acme", "front").unwrap();
        assert!(
            !library.join("rules/no-inline-css").exists()
                && !library.join("roles/front.md").exists()
        );
        assert!(
            repo.join(".eslintrc.json").exists(),
            "repository files stay"
        );
    }

    #[cfg(unix)]
    #[test]
    fn asks_before_proposing_and_keeps_the_answers() {
        let (config, _) = setup("conventions-questions");
        let dir = config.parent().unwrap().to_path_buf();
        save_source(&config, "acme", "front", TEXT).unwrap();
        let question = "Les tests sont-ils obligatoires pour les composants visuels ?";
        let claude = fake_claude(
            &dir,
            &json!({ "questions": [question], "items": [item("preference", "ignored", "x")] }),
        );
        let draft = propose(&config, "acme", "front", Vec::new(), &claude)
            .unwrap()
            .draft
            .unwrap();
        assert_eq!(draft.questions, [question]);
        assert!(draft.items.is_empty(), "questions first, nothing proposed");
        assert!(apply(&config, "acme", "front").is_err());

        let claude = fake_claude(
            &dir,
            &json!({ "questions": [], "items": [item("preference", "pascal", "Composants en PascalCase.")] }),
        );
        let answer = Answer {
            question: question.into(),
            answer: "Non.".into(),
        };
        let draft = propose(&config, "acme", "front", vec![answer.clone()], &claude)
            .unwrap()
            .draft
            .unwrap();
        assert_eq!(draft.answers, std::slice::from_ref(&answer));
        assert!(
            fs::read_to_string(dir.join("prompt.txt"))
                .unwrap()
                .contains("A: Non.")
        );
        accept_all(&config);
        let pack = apply(&config, "acme", "front").unwrap().pack.unwrap();
        assert_eq!(pack.answers, [answer], "kept for the next regeneration");

        let claude = fake_claude(&dir, &json!({ "questions": [], "items": [] }));
        assert!(
            propose(&config, "acme", "front", Vec::new(), &claude).is_err(),
            "an empty answer is an error"
        );
        let broken = vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo '{\"type\":\"result\",\"is_error\":true,\"result\":\"Not logged in\"}'".into(),
        ];
        let err = propose(&config, "acme", "front", Vec::new(), &broken)
            .unwrap_err()
            .to_string();
        assert!(err.contains("Not logged in"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn regenerating_shows_a_diff_and_replaces_only_what_it_owns() {
        let (config, repo) = setup("conventions-regenerate");
        let dir = config.parent().unwrap().to_path_buf();
        save_source(&config, "acme", "front", TEXT).unwrap();
        put(
            &config.join("projects/acme/library/skills/mine/SKILL.md"),
            "Mine.\n",
        );
        let first = json!({ "questions": [], "items": [
            item("preference", "keep", "Pas de CSS inline."),
            item("preference", "edit", "Pas de CSS inline."),
            item("command", "gone", "Composants en PascalCase."),
            item("skill", "mine", "x"),
        ]});
        propose(
            &config,
            "acme",
            "front",
            Vec::new(),
            &fake_claude(&dir, &first),
        )
        .unwrap();
        accept_all(&config);
        apply(&config, "acme", "front").unwrap();
        let library = config.join("projects/acme/library");
        assert_eq!(
            fs::read_to_string(library.join("skills/mine/SKILL.md")).unwrap(),
            "Mine.\n",
            "not the convention's"
        );

        let mut edited = item("preference", "edit", "Pas de CSS inline.");
        edited["body"] = "Do it better.".into();
        let mut config_change = item("repo", "tsconfig", "Composants en PascalCase.");
        config_change["repo"] = "api".into();
        config_change["path"] = "package.json".into();
        config_change["content"] = "{ \"name\": \"api\", \"private\": true }\n".into();
        let second = json!({ "questions": [], "items": [
            item("preference", "keep", "Pas de CSS inline."), edited, item("skill", "new-one", "x"), config_change,
        ]});
        let convention = propose(
            &config,
            "acme",
            "front",
            Vec::new(),
            &fake_claude(&dir, &second),
        )
        .unwrap();
        let prompt = fs::read_to_string(dir.join("prompt.txt")).unwrap();
        assert!(
            prompt.contains("keep, edit, gone"),
            "the current names are offered to keep"
        );
        let changes: Vec<_> = convention
            .diff
            .iter()
            .map(|d| (d.name.as_str(), d.change))
            .collect();
        assert_eq!(
            changes,
            [
                ("keep", Change::Unchanged),
                ("edit", Change::Changed),
                ("new-one", Change::Added),
                ("tsconfig", Change::Added),
                ("gone", Change::Removed)
            ]
        );
        assert_eq!(convention.diff[1].previous.as_ref().unwrap().body, "Do it.");
        assert!(
            library.join("commands/gone.md").exists(),
            "nothing replaced before applying"
        );
        let draft = convention.draft.unwrap();
        assert_eq!(
            draft.items[3].before.as_deref(),
            Some("{ \"name\": \"api\" }\n")
        );

        accept_all(&config);
        put(&repo.join("package.json"), "{ \"name\": \"renamed\" }\n");
        let err = apply(&config, "acme", "front").unwrap_err().to_string();
        assert!(
            err.contains("package.json changed since the proposal"),
            "{err}"
        );
        assert!(
            library.join("commands/gone.md").exists(),
            "a refused apply writes nothing"
        );

        review(
            &config,
            "acme",
            "front",
            "tsconfig",
            Some(Review::Rejected),
            None,
        )
        .unwrap();
        apply(&config, "acme", "front").unwrap();
        assert!(
            !library.join("commands/gone.md").exists(),
            "removed by the new version"
        );
        assert_eq!(
            fs::read_to_string(library.join("preferences/edit.md"))
                .unwrap()
                .lines()
                .last(),
            Some("Do it better.")
        );
        assert!(library.join("skills/new-one/SKILL.md").is_file());
        assert_eq!(
            fs::read_to_string(repo.join("package.json")).unwrap(),
            "{ \"name\": \"renamed\" }\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn revises_one_item_only() {
        let (config, _) = setup("conventions-revise");
        let dir = config.parent().unwrap().to_path_buf();
        save_source(&config, "acme", "front", TEXT).unwrap();
        let first = json!({ "questions": [], "items": [item("preference", "a", "x"), item("preference", "b", "y")] });
        propose(
            &config,
            "acme",
            "front",
            Vec::new(),
            &fake_claude(&dir, &first),
        )
        .unwrap();
        review(&config, "acme", "front", "a", Some(Review::Accepted), None).unwrap();
        review(&config, "acme", "front", "b", Some(Review::Accepted), None).unwrap();

        let mut revised = item("preference", "b", "y");
        revised["body"] = "Shorter.".into();
        let draft = revise(
            &config,
            "acme",
            "front",
            "b",
            "Plus court",
            &fake_claude(&dir, &json!({ "questions": [], "items": [revised] })),
        )
        .unwrap()
        .draft
        .unwrap();
        assert_eq!(draft.items[0].review, Review::Accepted, "the others stay");
        assert_eq!(
            (draft.items[1].body.as_str(), draft.items[1].review),
            ("Shorter.", Review::Pending)
        );
        assert_eq!(draft.cost, Some(0.5));
        let prompt = fs::read_to_string(dir.join("prompt.txt")).unwrap();
        assert!(prompt.contains("Request: Plus court") && prompt.contains("\"name\":\"b\""));

        let clash = json!({ "questions": [], "items": [item("preference", "a", "y")] });
        assert!(
            revise(
                &config,
                "acme",
                "front",
                "b",
                "x",
                &fake_claude(&dir, &clash)
            )
            .is_err(),
            "cannot take another item's name"
        );
    }

    #[test]
    fn guards_names_paths_and_settings() {
        let (config, repo) = setup("conventions-guards");
        for bad in ["", "../x", "A", "-x", "a/b"] {
            assert!(save_source(&config, "acme", bad, "x").is_err(), "{bad:?}");
        }
        assert!(save_source(&config, "missing", "front", "x").is_err());
        assert!(safe_join(&repo, "a/../../b").is_err());
        for git in [".git/config", ".GIT/hooks/pre-commit", "sub/.Git./x"] {
            assert!(safe_join(&repo, git).is_err(), "{git}");
        }
        assert!(safe_join(&repo, "/etc/passwd").is_err());
        assert!(safe_join(&repo, "").is_err());
        assert_eq!(safe_join(&repo, "src/x.ts").unwrap(), repo.join("src/x.ts"));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/tmp", repo.join("link")).unwrap();
            assert!(safe_join(&repo, "link/x").is_err());
        }
        assert_eq!(settings(&config), ("opus".into(), "high".into()));
        put(
            &config.join("termf.toml"),
            "[conventions]\nmodel = \"sonnet\"\neffort = \"medium\"\n",
        );
        assert_eq!(settings(&config), ("sonnet".into(), "medium".into()));
        assert_eq!(list(&config, "acme").unwrap().model, "sonnet");
    }
}
