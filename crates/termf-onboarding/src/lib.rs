//! Project onboarding assistant (ARCHITECTURE §13.7): a read-only analysis of the project's repositories,
//! deterministic first, then Claude Code headless for what the files alone do not say and for the questions only
//! the owner can answer. The answers become the `onboarding` convention, whose proposals go through the
//! conventions' review and journaled apply (§11.1). The assistant's progress is this machine's, in
//! `<data>/onboarding/<project>.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use termf_library::conventions;
use termf_library::{Layer, Scope};

mod team;
pub use team::{
    Choices, Edit, Part, Preview, Team, preview as team_preview, save as team_save, team,
};

/// The convention the answers become.
pub const SLUG: &str = "onboarding";
/// What one analysis may spend, in USD.
const BUDGET: &str = "2";
const MAX_QUESTIONS: usize = 10;
const MAX_FINDINGS: usize = 8;
const MAX_OPTIONS: usize = 8;
const MAX_ANSWER: usize = 500;
/// The proposals outside the library that the assistant records a decision for.
const PROPOSALS: [&str; 5] = ["tickets", "worktree", "ci", "trust", "team"];

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    #[default]
    Analysis,
    Questions,
    Proposals,
    Trust,
    Done,
}

/// What TermF finds in a repository by itself.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Facts {
    pub name: String,
    pub path: String,
    /// As worktree setup detects it: `node:pnpm`, `rust:cargo`, `laravel`…
    pub stack: Vec<String>,
    /// File names under `.github/workflows`.
    pub workflows: Vec<String>,
    /// Instructions for agents already there: `CLAUDE.md`, `AGENTS.md`…
    pub instructions: Vec<String>,
    /// Claude Code config the repository brings: `.claude/settings.json`, `.mcp.json`, `.claude/skills`…
    pub claude: Vec<String>,
    /// The monorepo tool whose workspace lists packages: `pnpm`, `npm`, `yarn`, `cargo`, `go`.
    pub workspace: Option<String>,
    /// Its code, by part: the packages, the first-level folders with a manifest, the repository itself.
    pub parts: Vec<Part>,
    /// The test globs (qa's `writes`) that match files of the repository.
    pub tests: Vec<String>,
    /// Its CI and infrastructure, for devops; a folder ends with `/`.
    pub infra: Vec<String>,
}

/// What Claude Code found that matters to agents, such as the test command.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Finding {
    pub repo: String,
    pub label: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Quality,
    Tests,
    Workflow,
    Agents,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// One of `options`.
    Choice,
    /// Any of `options`.
    Multi,
    /// `yes` or `no`.
    Bool,
    Text,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Question {
    pub id: String,
    pub theme: Theme,
    pub text: String,
    pub kind: Kind,
    pub options: Vec<String>,
    /// The proposed answer, in the shape of an answer.
    pub default: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Analysis {
    /// Seconds since the Unix epoch.
    pub at: u64,
    /// `fr` or `en`: the language of the questions and of the convention.
    pub lang: String,
    pub repos: Vec<Facts>,
    /// `owner/name` of the project's GitHub repositories.
    pub github: Vec<String>,
    /// The ticket sources its inbox reads now.
    pub tickets: Vec<String>,
    /// Items switched on in the project's own library, as `<kind> <name>`.
    pub library: Vec<String>,
    /// Whether the project has a saved worktree setup recipe.
    pub recipe: bool,
    /// The project's other conventions.
    pub conventions: Vec<String>,
    /// Whether Claude Code read the repositories too.
    pub asked: bool,
    pub findings: Vec<Finding>,
    pub questions: Vec<Question>,
    /// What was dropped from Claude Code's answer, and why.
    pub dropped: Vec<String>,
    pub model: String,
    pub cost: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Accepted,
    Rejected,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Logged {
    pub at: u64,
    /// `answers`, or the proposal decided on.
    pub what: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
    pub detail: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    /// Opens the assistant by itself, once: set when the project is created.
    pub auto_open: bool,
    pub step: Step,
    pub analysis: Option<Analysis>,
    /// By question id.
    pub answers: BTreeMap<String, Vec<String>>,
    /// The decision on each proposal outside the library (`tickets`, `worktree`, `ci`, `trust`, `team`).
    pub decisions: BTreeMap<String, Decision>,
    /// What the assistant applied, oldest first.
    pub log: Vec<Logged>,
    /// The convention text the answers last became.
    pub source: String,
    /// What the assistant last wrote of each part of the team (`team`, `workflow`, `roles.<name>`) in the
    /// project's file, to tell a hand edit from its own.
    pub team: BTreeMap<String, Value>,
    /// The `onboarding` convention's text is no longer the one the assistant wrote: someone edited it in the
    /// library, and new answers would replace it.
    #[serde(skip_deserializing)]
    pub edited: bool,
    /// An analysis of the project is running.
    #[serde(skip_deserializing)]
    pub analyzing: bool,
}

/// The projects (by state file) whose analysis is running in this process: a second one would pay twice for
/// the same answers.
static RUNNING: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

struct Running(PathBuf);

impl Running {
    fn start(file: PathBuf) -> Result<Running> {
        let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        ensure!(
            running.insert(file.clone()),
            "an analysis of this project is already running"
        );
        Ok(Running(file))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

fn convention_text(config_dir: &Path, project: &str) -> String {
    let file = config_dir
        .join("projects")
        .join(project)
        .join("conventions")
        .join(SLUG)
        .join("source.md");
    fs::read_to_string(file).unwrap_or_default()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn file(config_dir: &Path, data_dir: &Path, project: &str) -> Result<PathBuf> {
    termf_core::project_file(config_dir, project)?;
    Ok(data_dir.join("onboarding").join(format!("{project}.json")))
}

pub fn load(config_dir: &Path, data_dir: &Path, project: &str) -> Result<State> {
    let file = file(config_dir, data_dir, project)?;
    let mut state: State = match fs::read_to_string(&file) {
        Ok(text) => {
            serde_json::from_str(&text).with_context(|| format!("invalid {}", file.display()))?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", file.display())),
    };
    let text = convention_text(config_dir, project);
    state.edited = !text.trim().is_empty() && text != state.source;
    state.analyzing = RUNNING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&file);
    Ok(state)
}

fn save(config_dir: &Path, data_dir: &Path, project: &str, state: &State) -> Result<()> {
    let file = file(config_dir, data_dir, project)?;
    fs::create_dir_all(file.parent().context("no parent folder")?)?;
    termf_core::write_atomic(&file, &serde_json::to_string_pretty(state)?)
}

fn change(
    config_dir: &Path,
    data_dir: &Path,
    project: &str,
    edit: impl FnOnce(&mut State) -> Result<()>,
) -> Result<State> {
    let mut state = load(config_dir, data_dir, project)?;
    edit(&mut state)?;
    save(config_dir, data_dir, project, &state)?;
    // What the edit changed decides `edited` and `analyzing` too.
    load(config_dir, data_dir, project)
}

/// A new project opens the assistant once, from scratch: a deleted project of the same id leaves nothing behind.
pub fn created(config_dir: &Path, data_dir: &Path, project: &str) -> Result<()> {
    let state = State {
        auto_open: true,
        ..State::default()
    };
    save(config_dir, data_dir, project, &state)
}

/// The assistant opened: it no longer opens by itself.
pub fn seen(config_dir: &Path, data_dir: &Path, project: &str) -> Result<State> {
    change(config_dir, data_dir, project, |s| {
        s.auto_open = false;
        Ok(())
    })
}

pub fn step(config_dir: &Path, data_dir: &Path, project: &str, step: Step) -> Result<State> {
    change(config_dir, data_dir, project, |s| {
        s.step = step;
        Ok(())
    })
}

/// Records what the owner decided on a proposal outside the library, and what was applied.
pub fn decide(
    config_dir: &Path,
    data_dir: &Path,
    project: &str,
    proposal: &str,
    decision: Decision,
    detail: &str,
) -> Result<State> {
    ensure!(
        PROPOSALS.contains(&proposal),
        "unknown proposal {proposal:?}"
    );
    change(config_dir, data_dir, project, |s| {
        s.decisions.insert(proposal.into(), decision);
        s.log.push(Logged {
            at: now(),
            what: proposal.into(),
            decision: Some(decision),
            detail: detail.chars().take(MAX_ANSWER).collect(),
        });
        Ok(())
    })
}

fn listed(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| keep(n))
        .collect();
    names.sort();
    names
}

fn facts(name: &str, path: &Path, tests: &[String]) -> Facts {
    let present = |files: &[&str]| -> Vec<String> {
        files
            .iter()
            .filter(|f| path.join(f).exists())
            .map(|f| f.to_string())
            .collect()
    };
    let mut facts = Facts {
        name: name.into(),
        path: path.to_string_lossy().into_owned(),
        stack: termf_worktree::detect(path, &[]).stack,
        workflows: listed(&path.join(".github/workflows"), |n| {
            n.ends_with(".yml") || n.ends_with(".yaml")
        }),
        instructions: present(&[
            "CLAUDE.md",
            ".claude/CLAUDE.md",
            "CLAUDE.local.md",
            "AGENTS.md",
        ]),
        claude: present(&[
            ".claude/settings.json",
            ".mcp.json",
            ".claude/skills",
            ".claude/commands",
            ".claude/agents",
            ".claude/rules",
        ]),
        ..Facts::default()
    };
    team::survey(path, &mut facts, tests);
    facts
}

/// The facts of each of the project's repositories; `tests`: the test globs to look for.
fn repos(config_dir: &Path, project: &str, tests: &[String]) -> Result<Vec<Facts>> {
    let found = termf_core::load_projects(config_dir)?
        .into_iter()
        .find(|p| p.id == project)
        .with_context(|| format!("unknown project {project}"))?;
    ensure!(!found.repos.is_empty(), "this project has no repository");
    Ok(found
        .repos
        .iter()
        .map(|r| facts(&r.name, Path::new(&r.path), tests))
        .collect())
}

/// What TermF finds by itself, reading files and the project's config only.
pub fn survey(config_dir: &Path, data_dir: &Path, project: &str) -> Result<Analysis> {
    termf_core::project_file(config_dir, project)?;
    let tickets = termf_tickets::Tickets::new(config_dir.into(), data_dir.into());
    let scope = Scope {
        project: Some(project.into()),
        ..Scope::default()
    };
    let items = termf_library::resolve(config_dir, &scope)?;
    let library = items
        .iter()
        .filter(|i| i.layer == Layer::Project && i.active)
        .map(|i| format!("{} {}", i.kind, i.name))
        .collect();
    Ok(Analysis {
        at: now(),
        repos: repos(config_dir, project, &team::test_globs(&items))?,
        github: tickets.sources(project)?.github_repos,
        tickets: tickets
            .configured(Some(project))?
            .into_iter()
            .flat_map(|c| c.sources)
            .collect(),
        library,
        recipe: termf_worktree::view(config_dir, project)?.saved,
        conventions: conventions::list(config_dir, project)?
            .list
            .into_iter()
            .map(|c| c.slug)
            .filter(|s| s != SLUG)
            .collect(),
        ..Analysis::default()
    })
}

/// The deterministic survey, then Claude Code, read-only in the repositories, for its findings and questions.
/// When Claude Code fails, a first analysis keeps the survey alone, so the assistant can go on without
/// questions; an analysis run again keeps the last one and its answers.
pub fn analyze(
    config_dir: &Path,
    data_dir: &Path,
    project: &str,
    lang: &str,
    claude: &[String],
) -> Result<State> {
    let running = Running::start(file(config_dir, data_dir, project)?)?;
    let mut analysis = survey(config_dir, data_dir, project)?;
    analysis.lang = if lang == "fr" { "fr" } else { "en" }.into();
    let (model, effort) = conventions::settings(config_dir);
    analysis.model = model.clone();
    let asked = team::questions(&analysis.repos);
    analysis.questions = asked.clone();
    change(config_dir, data_dir, project, |s| {
        if s.analysis.is_none() {
            s.analysis = Some(analysis.clone());
            s.answers.clear();
        }
        Ok(())
    })?;
    let repos: Vec<(String, PathBuf)> = analysis
        .repos
        .iter()
        .map(|r| (r.name.clone(), PathBuf::from(&r.path)))
        .collect();
    // Options before `-p` reach Claude Code like any other: the cap needs no change to the shared runner.
    let mut command = claude.to_vec();
    command.extend(["--max-budget-usd".into(), BUDGET.into()]);
    let (answer, cost) = conventions::ask(
        &command,
        &repos,
        &model,
        &effort,
        &prompt(&analysis),
        &schema(),
    )?;
    let names: Vec<String> = analysis.repos.iter().map(|r| r.name.clone()).collect();
    let (findings, questions, dropped) = validate(&answer, &names);
    analysis.asked = true;
    analysis.cost = cost;
    analysis.findings = findings;
    analysis.dropped = dropped;
    analysis.questions = questions.into_iter().chain(asked).collect();
    drop(running);
    change(config_dir, data_dir, project, |s| {
        s.answers = analysis
            .questions
            .iter()
            .map(|q| (q.id.clone(), q.default.clone()))
            .collect();
        s.analysis = Some(analysis);
        s.step = Step::Questions;
        Ok(())
    })
}

fn prompt(analysis: &Analysis) -> String {
    let language = if analysis.lang == "fr" {
        "French"
    } else {
        "English"
    };
    let mut p = String::from(
        "You help set up a software project in TermF, a tool that runs Claude Code agents on the project's \
         repositories. Read the repositories (read-only: use Read, Grep and Glob; never modify anything) for what \
         the facts below cannot tell.\n\nRepositories and what TermF already found:\n",
    );
    for (i, r) in analysis.repos.iter().enumerate() {
        let here = if i == 0 { " (current directory)" } else { "" };
        p += &format!(
            "- {}: {}{here}; stack: {}; CI workflows: {}; agent instructions: {}; Claude Code config: {}\n",
            r.name,
            r.path,
            or_none(&r.stack),
            or_none(&r.workflows),
            or_none(&r.instructions),
            or_none(&r.claude),
        );
    }
    p += &format!(
        "Project library items already on: {}\nOther conventions already written: {}\n\n",
        or_none(&analysis.library),
        or_none(&analysis.conventions)
    );
    p += &format!(
        "Return:\n\
         1. `findings`: short facts per repository that agents working on it need and the facts above do not \
         give: the commands to test, lint, format, type-check and build; the main frameworks; conventions stated \
         in CLAUDE.md, AGENTS.md or the tools' configs. At most {MAX_FINDINGS} per repository; `repo` is a \
         repository name from the list, `label` a few words, `value` one line. Never copy a secret.\n\
         2. `questions`: only what the repositories cannot tell and the owner must decide for TermF to set up the \
         agents' rules, skills, commands and roles, such as which checks must pass before a change is delivered, \
         whether every change needs tests, branch and commit conventions, what agents may change without asking. \
         Never ask what the files already answer, nor about ticket trackers, worktree setup or CI notifications: \
         TermF sets those up apart. Never ask about the agents' team either (its roles, squads, size or \
         models): TermF asks that itself. At most {MAX_QUESTIONS}. Each has a unique `id` (lowercase letters, digits \
         and dashes), a `theme` (quality, tests, workflow, agents, other), a short `text`, a `kind` and a \
         proposed answer in `default`, the most likely one given the repositories: `choice` takes one of \
         `options` (2 to {MAX_OPTIONS}) and `default` is that one option; `multi` takes any of `options` and \
         `default` lists some; `bool` has no options and `default` is [\"yes\"] or [\"no\"]; `text` has no \
         options and `default` is one short text or nothing. Prefer choice, multi and bool to text.\n\
         Write labels, values, questions and options in {language}.\n"
    );
    p
}

fn or_none(list: &[String]) -> String {
    if list.is_empty() {
        "none".into()
    } else {
        list.join(", ")
    }
}

/// The structured answer Claude Code must give.
fn schema() -> Value {
    let text = json!({ "type": "string" });
    let texts = json!({ "type": "array", "items": text });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["findings", "questions"],
        "properties": {
            "findings": { "type": "array", "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["repo", "label", "value"],
                "properties": { "repo": text, "label": text, "value": text },
            }},
            "questions": { "type": "array", "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "theme", "text", "kind", "options", "default"],
                "properties": {
                    "id": text,
                    "theme": { "enum": ["quality", "tests", "workflow", "agents", "other"] },
                    "text": text,
                    "kind": { "enum": ["choice", "multi", "bool", "text"] },
                    "options": texts,
                    "default": texts,
                },
            }},
        },
    })
}

/// One line, spaces collapsed.
fn line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fits(text: &str, max: usize) -> bool {
    !text.is_empty() && text.chars().count() <= max
}

fn valid_id(id: &str) -> bool {
    fits(id, 40)
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether `value` is an answer `question` accepts.
fn accepts(q: &Question, value: &[String]) -> bool {
    let known = |v: &String| q.options.contains(v);
    let unique = value.iter().collect::<BTreeSet<_>>().len() == value.len();
    match q.kind {
        Kind::Choice => value.len() == 1 && known(&value[0]),
        Kind::Multi => unique && value.iter().all(known),
        Kind::Bool => value == ["yes"] || value == ["no"],
        Kind::Text => value.len() <= 1 && value.iter().all(|v| v.chars().count() <= MAX_ANSWER),
    }
}

/// Keeps what is well-formed in Claude Code's answer; the rest is dropped, each with its reason.
fn validate(answer: &Value, repos: &[String]) -> (Vec<Finding>, Vec<Question>, Vec<String>) {
    let mut dropped = Vec::new();
    let mut findings: Vec<Finding> = Vec::new();
    for raw in answer["findings"].as_array().into_iter().flatten() {
        let Ok(f) = serde_json::from_value::<Finding>(raw.clone()) else {
            dropped.push("a finding: not valid".into());
            continue;
        };
        let f = Finding {
            repo: f.repo.trim().into(),
            label: line(&f.label),
            value: line(&f.value),
        };
        let why = if !repos.contains(&f.repo) {
            "not a repository of the project"
        } else if !fits(&f.label, 60) || !fits(&f.value, 300) {
            "empty or too long"
        } else if findings.iter().filter(|g| g.repo == f.repo).count() == MAX_FINDINGS {
            "too many for this repository"
        } else {
            findings.push(f);
            continue;
        };
        dropped.push(format!(
            "finding {:?}: {why}",
            f.label.chars().take(60).collect::<String>()
        ));
    }

    let mut questions: Vec<Question> = Vec::new();
    for raw in answer["questions"].as_array().into_iter().flatten() {
        let label: String = raw["id"].as_str().unwrap_or("?").chars().take(40).collect();
        let why = match serde_json::from_value::<Question>(raw.clone()) {
            Err(_) => "not a valid question",
            Ok(mut q) => {
                q.id = q.id.trim().into();
                q.text = line(&q.text);
                q.options = q.options.iter().map(|o| line(o)).collect();
                q.default = q.default.iter().map(|d| line(d)).collect();
                let options_ok = match q.kind {
                    Kind::Choice | Kind::Multi => {
                        (2..=MAX_OPTIONS).contains(&q.options.len())
                            && q.options.iter().all(|o| fits(o, 80))
                            && q.options.iter().collect::<BTreeSet<_>>().len() == q.options.len()
                    }
                    Kind::Bool | Kind::Text => q.options.is_empty(),
                };
                if q.kind == Kind::Text {
                    q.default.retain(|d| !d.is_empty());
                }
                if questions.len() == MAX_QUESTIONS {
                    "too many questions"
                } else if !valid_id(&q.id) || questions.iter().any(|o| o.id == q.id) {
                    "invalid or repeated id"
                } else if q.id.starts_with("team-") {
                    "an id kept for TermF's own questions"
                } else if !fits(&q.text, 300) {
                    "empty or too long"
                } else if !options_ok {
                    "invalid options"
                } else if !accepts(&q, &q.default) {
                    "its proposed answer is not one it accepts"
                } else {
                    questions.push(q);
                    continue;
                }
            }
        };
        dropped.push(format!("question {label}: {why}"));
    }
    (findings, questions, dropped)
}

/// Saves the answers (a question left out keeps its proposed answer) and writes them, with the analysis, as the
/// source of the `onboarding` convention, ready for `conventions::propose`. A convention edited in the library
/// since is only replaced with `replace`.
pub fn answer(
    config_dir: &Path,
    data_dir: &Path,
    project: &str,
    answers: BTreeMap<String, Vec<String>>,
    replace: bool,
) -> Result<State> {
    let state = load(config_dir, data_dir, project)?;
    let analysis = state.analysis.context("analyze the project first")?;
    let mut given = BTreeMap::new();
    for (id, value) in &answers {
        let q = analysis
            .questions
            .iter()
            .find(|q| q.id == *id)
            .with_context(|| format!("no question {id:?}"))?;
        let value: Vec<String> = value
            .iter()
            .map(|v| v.trim().to_owned())
            .filter(|v| !(q.kind == Kind::Text && v.is_empty()))
            .collect();
        if !accepts(q, &value) {
            bail!("not an answer to {:?}", q.text);
        }
        given.insert(id.clone(), value);
    }
    for q in &analysis.questions {
        given
            .entry(q.id.clone())
            .or_insert_with(|| q.default.clone());
    }
    ensure!(
        replace || !state.edited,
        "the onboarding convention was edited in the library since: replace it to go on"
    );
    let scope = Scope {
        project: Some(project.into()),
        ..Scope::default()
    };
    let items = termf_library::resolve(config_dir, &scope)?;
    let folders = team::folders(&items, &analysis.repos, &analysis.questions, &given);
    let text = source(&analysis, &given, &folders);
    if convention_text(config_dir, project) != text {
        conventions::save_source(config_dir, project, SLUG, &text)?;
    }
    change(config_dir, data_dir, project, |s| {
        s.source = text;
        s.answers = given;
        s.step = Step::Proposals;
        s.log.push(Logged {
            at: now(),
            what: "answers".into(),
            decision: None,
            detail: analysis.questions.len().to_string(),
        });
        Ok(())
    })
}

/// The convention's text: what the repositories hold, what the owner decided, then the team's roles that have
/// folders of their own, one sentence per line.
fn source(
    analysis: &Analysis,
    answers: &BTreeMap<String, Vec<String>>,
    folders: &[(String, Vec<String>)],
) -> String {
    let fr = analysis.lang == "fr";
    let pick = |fr_text: &'static str, en_text: &'static str| if fr { fr_text } else { en_text };
    let mut text = format!(
        "# {}\n\n{}\n",
        pick(
            "Mise en route du projet (assistant de TermF)",
            "Project setup (TermF's onboarding assistant)"
        ),
        pick(
            "Ce que contiennent les dépôts :",
            "What the repositories hold:"
        )
    );
    for r in analysis.repos.iter().filter(|r| !r.stack.is_empty()) {
        text += &format!("- {}: {}\n", r.name, r.stack.join(", "));
    }
    for f in &analysis.findings {
        text += &format!("- {}: {}: {}\n", f.repo, f.label, f.value);
    }
    let decided: Vec<String> = analysis
        .questions
        .iter()
        .filter(|q| !q.id.starts_with("team-"))
        .filter_map(|q| {
            let value = answers.get(&q.id)?;
            let shown = match q.kind {
                Kind::Bool if value == &["yes"] => pick("oui", "yes").to_owned(),
                Kind::Bool => pick("non", "no").to_owned(),
                Kind::Multi if value.is_empty() => pick("aucun", "none").to_owned(),
                _ => value.join(", "),
            };
            (!shown.is_empty()).then(|| format!("- {} {shown}\n", q.text))
        })
        .collect();
    if !decided.is_empty() {
        text += &format!(
            "\n{}\n",
            pick(
                "Ce que le propriétaire a décidé :",
                "What the owner decided:"
            )
        );
        text += &decided.concat();
    }
    if !folders.is_empty() {
        text += &format!(
            "\n{}\n",
            pick(
                "L'équipe d'agents (proposée par l'assistant) :",
                "The agents' team (proposed by the assistant):"
            )
        );
        for (role, globs) in folders {
            text += &format!("- {role}: {}\n", globs.join(", "));
        }
        text += pick(
            "Chaque rôle reçoit les consignes du projet qui le concernent.\n",
            "Each role gets the project's instructions that concern it.\n",
        );
    }
    text
}

#[cfg(test)]
mod tests;
