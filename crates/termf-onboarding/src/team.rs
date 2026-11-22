//! The project's team as the assistant proposes it (ARCHITECTURE §7): what the survey finds of the code (its
//! parts, test files, CI and infrastructure), the roles that serve and those left out, then `[[squads]]`,
//! `[team] staff`, `[workflows] default` and `[roles.<name>]` in the project's file. Instructions for a role are
//! never written there: they come from the conventions, as preferences with `applies_to`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use termf_core::squads::canonical_role;
use termf_library::{Item, Kind as ItemKind, Layer, Scope};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item as TomlItem, Table, value};

use crate::{Decision, Facts, Kind, Logged, Question, State, Theme};

pub const SIZE: &str = "team-size";
pub const SPLIT: &str = "team-squads";
pub const MODEL: &str = "team-model";
const MODELS: [&str; 3] = ["", "sonnet", "opus"];
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// The settings of `[roles.<name>]` the assistant proposes; the others stay as they are.
const SETTINGS: [&str; 3] = ["writes", "model", "effort"];
/// The roles that work in a squad; the other roles that take tasks are on the staff.
const CODE: [&str; 4] = ["front", "back", "qa", "devops"];

const MAX_ENTRIES: usize = 50_000;
const SKIPPED: [&str; 8] = [
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "__pycache__",
];
const DOT_KEPT: [&str; 3] = [".github", ".circleci", ".buildkite"];
const MANIFESTS: [&str; 7] = [
    "package.json",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "composer.json",
    "Gemfile",
];
const FRONT_DEPS: [&str; 20] = [
    "react",
    "vue",
    "svelte",
    "@sveltejs/kit",
    "next",
    "nuxt",
    "astro",
    "@angular/core",
    "solid-js",
    "preact",
    "lit",
    "gatsby",
    "@remix-run/react",
    "@builder.io/qwik",
    "ember-source",
    "react-native",
    "expo",
    "vitepress",
    "@docusaurus/core",
    "@11ty/eleventy",
];
/// Frameworks for websites, whose pages search engines read.
const WEB_DEPS: [&str; 10] = [
    "next",
    "nuxt",
    "astro",
    "gatsby",
    "@remix-run/react",
    "@sveltejs/kit",
    "@builder.io/qwik-city",
    "vitepress",
    "@docusaurus/core",
    "@11ty/eleventy",
];
const DESKTOP_DEPS: [&str; 2] = ["@tauri-apps/api", "electron"];
const RUST_FRONT: [&str; 4] = ["leptos", "yew", "dioxus", "sycamore"];
const INFRA_DIRS: [&str; 12] = [
    "infra",
    "infrastructure",
    "deploy",
    "deployment",
    "k8s",
    "kubernetes",
    "helm",
    "charts",
    "terraform",
    "ansible",
    ".circleci",
    ".buildkite",
];
const INFRA_FILES: [&str; 13] = [
    ".gitlab-ci.yml",
    "Jenkinsfile",
    "azure-pipelines.yml",
    "bitbucket-pipelines.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
    "fly.toml",
    "vercel.json",
    "netlify.toml",
    "render.yaml",
    "Procfile",
];
const MAX_INFRA: usize = 12;

/// A part of a repository's code: a package of a monorepo, a folder with its own manifest, or the repository.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Part {
    /// Relative to the repository, `""` for the whole of it.
    pub path: String,
    /// The package's name, else the folder's.
    pub name: String,
    /// `front` (a user interface) or `back` (anything else: a server, a library, a tool).
    pub role: String,
    /// A public website, which `seo` serves; a desktop app's interface is not one.
    pub web: bool,
}

/// What the team needs from a repository: its monorepo tool and parts, the globs of `tests` that match its
/// files, its CI and infrastructure.
pub(crate) fn survey(repo: &Path, facts: &mut Facts, tests: &[String]) {
    let files = files(repo);
    (facts.workspace, facts.parts) = parts(repo);
    facts.tests = tests
        .iter()
        .filter(|g| files.iter().any(|f| termf_team::rules::glob(g, f)))
        .cloned()
        .collect();
    facts.infra = infra(&files);
}

/// The repository's files, relative and with `/`, leaving out dependency, build and hidden folders.
fn files(repo: &Path) -> Vec<String> {
    let (mut found, mut seen) = (Vec::new(), 0);
    let mut queue = VecDeque::from([(repo.to_path_buf(), String::new())]);
    while let Some((dir, rel)) = queue.pop_front() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return found;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = match rel.as_str() {
                "" => name.clone(),
                rel => format!("{rel}/{name}"),
            };
            match entry.file_type() {
                Ok(t) if t.is_dir() => {
                    let hidden = name.starts_with('.') && !DOT_KEPT.contains(&name.as_str());
                    if !hidden && !SKIPPED.contains(&name.as_str()) {
                        queue.push_back((entry.path(), path));
                    }
                }
                Ok(t) if t.is_file() => found.push(path),
                _ => {}
            }
        }
    }
    found
}

/// CI and infrastructure: a folder ends with `/`.
fn infra(files: &[String]) -> Vec<String> {
    let mut found = BTreeSet::new();
    for f in files {
        let (first, rest) = f.split_once('/').unwrap_or((f, ""));
        let name = f.rsplit('/').next().unwrap_or(f);
        let marker = if f.starts_with(".github/workflows/") {
            ".github/workflows/".to_owned()
        } else if !rest.is_empty() && INFRA_DIRS.contains(&first) {
            format!("{first}/")
        } else if (rest.is_empty() && INFRA_FILES.contains(&name))
            || name == "Dockerfile"
            || name.starts_with("Dockerfile.")
        {
            f.clone()
        } else if name.ends_with(".tf") {
            f.rsplit_once('/')
                .map_or_else(|| f.clone(), |(dir, _)| format!("{dir}/"))
        } else {
            continue;
        };
        found.insert(marker);
    }
    found.into_iter().take(MAX_INFRA).collect()
}

/// The parts of a repository: its workspace's packages, its first-level folders with a manifest, and the
/// repository itself unless its manifest only holds a workspace's tooling.
fn parts(repo: &Path) -> (Option<String>, Vec<Part>) {
    let (tool, members) = workspace(repo).map_or((None, Vec::new()), |(t, m)| (Some(t), m));
    let mut rels = vec![String::new()];
    rels.extend(members);
    let mut firsts: Vec<String> = fs::read_dir(repo)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.') && !SKIPPED.contains(&n.as_str()))
        .filter(|n| has_manifest(&repo.join(n)))
        .collect();
    firsts.sort();
    for first in firsts {
        if !rels.contains(&first) {
            rels.push(first);
        }
    }
    let mut parts: Vec<Part> = rels.iter().filter_map(|r| part(repo, r)).collect();
    // A Cargo package may have members; other workspace roots only hold tooling.
    if tool.is_some_and(|t| t != "cargo") {
        parts.retain(|p| !p.path.is_empty());
    }
    // Laravel and Rails keep their interface in the server's folder.
    let mut views = Vec::new();
    for p in parts.iter().filter(|p| p.role == "back") {
        let dir = repo.join(&p.path);
        for (marker, folder) in [("resources/views", "resources"), ("app/views", "app/views")] {
            if dir.join(marker).is_dir() {
                views.push(Part {
                    path: join(&p.path, folder),
                    name: p.name.clone(),
                    role: "front".into(),
                    web: dir.join("public/robots.txt").is_file(),
                });
            }
        }
    }
    parts.extend(views);
    (tool.map(str::to_owned), parts)
}

fn join(dir: &str, rel: &str) -> String {
    match dir {
        "" => rel.to_owned(),
        dir => format!("{dir}/{rel}"),
    }
}

fn has_manifest(dir: &Path) -> bool {
    MANIFESTS.iter().any(|m| dir.join(m).is_file())
}

/// The monorepo tool and the folders its workspace lists.
fn workspace(repo: &Path) -> Option<(&'static str, Vec<String>)> {
    let read = |f: &str| fs::read_to_string(repo.join(f)).ok();
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect()
    };
    let npm = read("package.json")
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| v["workspaces"].clone())
        .filter(|w| !w.is_null());
    let cargo = read("Cargo.toml")
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| {
            let members = t.get("workspace")?.get("members")?.as_array()?;
            Some(
                members
                    .iter()
                    .filter_map(|m| m.as_str().map(str::to_owned))
                    .collect(),
            )
        });
    let (tool, patterns) = if let Some(text) = read("pnpm-workspace.yaml") {
        ("pnpm", yaml_list(&text, "packages"))
    } else if let Some(w) = npm {
        let tool = if repo.join("yarn.lock").exists() {
            "yarn"
        } else {
            "npm"
        };
        (
            tool,
            if w.is_array() {
                strings(&w)
            } else {
                strings(&w["packages"])
            },
        )
    } else if let Some(members) = cargo {
        ("cargo", members)
    } else {
        ("go", go_uses(&read("go.work")?))
    };
    Some((tool, expand(repo, &patterns)))
}

/// The strings of `key: [..]`, or of the `- item` lines under `key:`, in a simple YAML file.
fn yaml_list(text: &str, key: &str) -> Vec<String> {
    let clean = |s: &str| {
        let s = s.split(" #").next().unwrap_or_default().trim();
        s.trim_matches(['\'', '"']).to_owned()
    };
    let mut lines = text
        .lines()
        .skip_while(|l| !l.starts_with(&format!("{key}:")));
    let Some(first) = lines.next() else {
        return Vec::new();
    };
    if let Some(list) = first[key.len() + 1..].trim().strip_prefix('[') {
        return list
            .trim_end_matches(']')
            .split(',')
            .map(clean)
            .filter(|s| !s.is_empty())
            .collect();
    }
    lines
        .take_while(|l| l.trim().is_empty() || l.starts_with([' ', '\t', '-', '#']))
        .filter_map(|l| l.trim().strip_prefix('-').map(clean))
        .filter(|s| !s.is_empty())
        .collect()
}

/// The folders of a `go.work`: `use ./api`, or a `use ( … )` block.
fn go_uses(text: &str) -> Vec<String> {
    let (mut found, mut block) = (Vec::new(), false);
    for line in text
        .lines()
        .map(|l| l.split("//").next().unwrap_or_default().trim())
    {
        if block {
            match line {
                ")" => block = false,
                "" => {}
                dir => found.push(dir.to_owned()),
            }
        } else if let Some(rest) = line
            .strip_prefix("use")
            .filter(|r| r.starts_with([' ', '\t', '(']))
        {
            match rest.trim() {
                "(" => block = true,
                dir => found.push(dir.to_owned()),
            }
        }
    }
    found
}

/// A workspace's folders: literal ones, and those under `dir/*` (or `dir/**`) that hold a manifest.
fn expand(repo: &Path, patterns: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for pattern in patterns.iter().filter(|p| !p.starts_with('!')) {
        let p = pattern.trim_start_matches("./").trim_end_matches('/');
        if p.split('/').any(|c| c == "..") {
            continue;
        }
        let dirs = match p.strip_suffix("/**").or_else(|| p.strip_suffix("/*")) {
            Some(parent) if !parent.contains('*') => {
                let mut subs: Vec<String> = fs::read_dir(repo.join(parent))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    .map(|e| format!("{parent}/{}", e.file_name().to_string_lossy()))
                    .filter(|d| has_manifest(&repo.join(d)))
                    .collect();
                subs.sort();
                subs
            }
            _ if p.is_empty() || p == "." || p.contains('*') => Vec::new(),
            _ => vec![p.to_owned()],
        };
        for dir in dirs {
            if !found.contains(&dir) && repo.join(&dir).is_dir() {
                found.push(dir);
            }
        }
    }
    found
}

/// The part at `rel`, by its manifest; `None` without one, or for a root that only lists a workspace.
fn part(repo: &Path, rel: &str) -> Option<Part> {
    let dir = repo.join(rel);
    let read = |f: &str| fs::read_to_string(dir.join(f)).ok();
    let robots = ["public/robots.txt", "static/robots.txt"]
        .iter()
        .any(|f| dir.join(f).is_file());
    let mut part = Part {
        path: rel.to_owned(),
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        role: "back".into(),
        web: false,
    };
    if let Some(pkg) = read("package.json").and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        if !pkg["workspaces"].is_null() {
            return None;
        }
        let deps: BTreeSet<&str> = ["dependencies", "devDependencies", "peerDependencies"]
            .iter()
            .filter_map(|k| pkg[*k].as_object())
            .flat_map(|o| o.keys().map(String::as_str))
            .collect();
        let has = |list: &[&str]| list.iter().any(|d| deps.contains(d));
        if let Some(name) = pkg["name"].as_str() {
            part.name = name.to_owned();
        }
        if has(&FRONT_DEPS) {
            part.role = "front".into();
            part.web = !has(&DESKTOP_DEPS) && (has(&WEB_DEPS) || robots);
        }
        return Some(part);
    }
    if let Some(cargo) = read("Cargo.toml").and_then(|t| toml::from_str::<toml::Table>(&t).ok()) {
        let package = cargo.get("package")?;
        if let Some(name) = package.get("name").and_then(|n| n.as_str()) {
            part.name = name.to_owned();
        }
        let deps = cargo.get("dependencies").and_then(|d| d.as_table());
        if deps.is_some_and(|d| RUST_FRONT.iter().any(|f| d.contains_key(*f))) {
            part.role = "front".into();
        }
        return Some(part);
    }
    if has_manifest(&dir) {
        return Some(part);
    }
    if ["index.html", "hugo.toml", "hugo.yaml"]
        .iter()
        .any(|f| dir.join(f).is_file())
    {
        part.role = "front".into();
        part.web = true;
        return Some(part);
    }
    None
}

/// A role a session may play, as the team needs it.
pub(crate) struct RoleInfo {
    name: String,
    plannable: bool,
    model: Option<String>,
}

pub(crate) fn roles(items: &[Item]) -> Vec<RoleInfo> {
    items
        .iter()
        .filter(|i| i.kind == ItemKind::Role && (i.layer != Layer::Repo || i.active))
        .filter(|i| canonical_role(&i.name) == i.name)
        .map(|i| RoleInfo {
            name: i.name.clone(),
            plannable: termf_library::plannable(i),
            model: i
                .meta
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .collect()
}

/// qa's globs, as its frontmatter gives them: the test files the survey looks for.
pub(crate) fn test_globs(items: &[Item]) -> Vec<String> {
    items
        .iter()
        .find(|i| i.kind == ItemKind::Role && i.name == "qa")
        .map(|i| termf_library::list_of(i.meta.get("writes")))
        .unwrap_or_default()
}

/// Each repository's parts, with its name.
fn units(repos: &[Facts]) -> Vec<(&str, &Part)> {
    repos
        .iter()
        .flat_map(|r| r.parts.iter().map(move |p| (r.name.as_str(), p)))
        .collect()
}

/// Whether the front and the back can each have their squad: they are apart, in folders or repositories.
fn splittable(units: &[(&str, &Part)]) -> bool {
    let has = |role: &str| units.iter().any(|(_, p)| p.role == role);
    has("front")
        && has("back")
        && !units.iter().any(|(repo, p)| {
            p.path.is_empty()
                && units
                    .iter()
                    .any(|(other, q)| other == repo && q.role != p.role)
        })
}

/// A monorepo, or several repositories with code: worth a squad per part and two agents per code role.
fn large(repos: &[Facts]) -> bool {
    repos
        .iter()
        .any(|r| r.workspace.is_some() && r.parts.len() > 1)
        || repos.iter().filter(|r| !r.parts.is_empty()).count() > 1
}

/// What only the owner can say of the team, asked with Claude Code's questions.
pub(crate) fn questions(repos: &[Facts], lang: &str) -> Vec<Question> {
    let fr = lang == "fr";
    let text = |fr_text: &str, en_text: &str| if fr { fr_text } else { en_text }.to_owned();
    let large = large(repos);
    let question = |id: &str, text: String, options: Vec<String>, default: usize| Question {
        id: id.into(),
        theme: Theme::Agents,
        text,
        kind: Kind::Choice,
        default: vec![options[default].clone()],
        options,
    };
    let mut asked = vec![question(
        SIZE,
        text(
            "Combien d'agents à la fois par rôle de code (front, back) ?",
            "How many agents at once per code role (front, back)?",
        ),
        ["1", "2", "3"].map(str::to_owned).to_vec(),
        usize::from(large),
    )];
    if splittable(&units(repos)) {
        asked.push(question(
            SPLIT,
            text(
                "Comment organiser l'équipe d'agents ?",
                "How should the agents' team be organized?",
            ),
            vec![
                text("Une seule squad", "One squad"),
                text(
                    "Une squad par partie : web et api",
                    "A squad per part: web and api",
                ),
            ],
            usize::from(large),
        ));
    }
    asked.push(question(
        MODEL,
        text(
            "Quel modèle pour les agents ?",
            "Which model for the agents?",
        ),
        vec![
            text("Celui de chaque rôle", "Each role's own"),
            text("Économe : sonnet partout", "Thrifty: sonnet everywhere"),
            text("Qualité : opus partout", "Quality: opus everywhere"),
        ],
        0,
    ));
    asked
}

/// What the owner chose of the team: in the answers to the assistant's questions, then in the card.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Choices {
    /// Roles kept (`true`) or left out (`false`) against what the survey proposes.
    pub keep: BTreeMap<String, bool>,
    /// A squad per part rather than one.
    pub split: Option<bool>,
    /// Sessions at once of each code role (front, back).
    pub size: Option<usize>,
    /// `sonnet` or `opus` for every role; `""` keeps each role's own.
    pub model: Option<String>,
}

/// The choices the answers make.
fn answered(questions: &[Question], answers: &BTreeMap<String, Vec<String>>) -> Choices {
    let index = |id: &str| {
        let q = questions.iter().find(|q| q.id == id)?;
        let a = answers.get(id)?.first()?;
        q.options.iter().position(|o| o == a)
    };
    Choices {
        keep: BTreeMap::new(),
        size: index(SIZE).map(|i| i + 1),
        split: index(SPLIT).map(|i| i == 1),
        model: index(MODEL).map(|i| MODELS[i].to_owned()),
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoleChoice {
    pub role: String,
    pub keep: bool,
    /// Why, as a code the interface words: `front`, `back`, `tests`, `no-tests`, `infra`, `ui`, `web`, `design`,
    /// `own` (a role of the user's library), or `no-front`, `no-back`, `no-infra`, `no-ui`, `no-web`.
    pub why: String,
    /// What the survey found for it: folders, globs, files.
    pub detail: Vec<String>,
    /// Kept or left out by the owner, against the survey.
    pub chosen: bool,
}

/// A `[[squads]]` entry, as the assistant writes it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Squad {
    pub id: String,
    pub lead: String,
    pub roles: BTreeMap<String, usize>,
    pub repos: Vec<String>,
    pub paths: Vec<String>,
    pub domains: Vec<String>,
}

/// `[[squads]]` and `[team] staff`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Composition {
    pub squads: Vec<Squad>,
    pub staff: Vec<String>,
}

/// The settings of `[roles.<name>]` the assistant manages; one left out is removed.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

/// What the assistant writes in the project's file: each part only once accepted.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Edit {
    pub team: Option<Composition>,
    pub workflow: Option<String>,
    pub roles: BTreeMap<String, Settings>,
}

impl Edit {
    /// Its parts: `team`, `workflow`, `roles.<name>`.
    fn keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        if self.team.is_some() {
            keys.push("team".to_owned());
        }
        if self.workflow.is_some() {
            keys.push("workflow".to_owned());
        }
        keys.extend(self.roles.keys().map(|r| format!("roles.{r}")));
        keys
    }
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The file has nothing of it yet.
    New,
    /// The file already says the same.
    Same,
    /// The file has what the assistant wrote last time: replacing it loses nothing.
    Changed,
    /// The file has something else, written by hand: only replaced once asked to.
    Edited,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    /// The choices the proposal follows, each filled in.
    pub choices: Choices,
    /// Whether the front and the back can each have their squad.
    pub splittable: bool,
    /// The roles that take tasks, kept or left out, and why.
    pub roles: Vec<RoleChoice>,
    /// The roles that are workflow steps rather than tasks (`po`, `lead`, `secops`…).
    pub steps: Vec<String>,
    pub proposal: Edit,
    /// By part of the proposal (`team`, `workflow`, `roles.<name>`).
    pub status: BTreeMap<String, Status>,
    /// The project's file.
    pub path: String,
}

/// A label for a part: its folder, or its repository when it is the whole of it.
fn label(repo: &str, part: &Part, many: bool) -> String {
    match (part.path.as_str(), many) {
        ("", _) => repo.to_owned(),
        (path, true) => format!("{repo}/{path}"),
        (path, false) => path.to_owned(),
    }
}

/// `dir/**` for each folder, or for their parent when every part under it is one of them.
fn globs(mine: &[&str], others: &[&str]) -> Vec<String> {
    let under = |list: &[&str], dir: &str| {
        list.iter()
            .filter(|p| p.starts_with(&format!("{dir}/")))
            .count()
    };
    let found: BTreeSet<String> = mine
        .iter()
        .map(|p| match p.rsplit_once('/') {
            Some((dir, _)) if under(mine, dir) > 1 && under(others, dir) == 0 => {
                format!("{dir}/**")
            }
            _ => format!("{p}/**"),
        })
        .collect();
    found.into_iter().collect()
}

/// The proposal: the roles that serve and why, then what to write for them.
fn propose(
    repos: &[Facts],
    roles: &[RoleInfo],
    given: &Choices,
) -> (Choices, Vec<RoleChoice>, Edit) {
    let units = units(repos);
    let many = repos.len() > 1;
    let of = |role: &str| -> Vec<(&str, &Part)> {
        units
            .iter()
            .copied()
            .filter(|(_, p)| p.role == role)
            .collect()
    };
    let (front, back) = (of("front"), of("back"));
    let web: Vec<_> = front.iter().copied().filter(|(_, p)| p.web).collect();
    let labels = |list: &[(&str, &Part)]| -> Vec<String> {
        let found: BTreeSet<String> = list.iter().map(|(r, p)| label(r, p, many)).collect();
        found.into_iter().collect()
    };
    // In the order found: qa's globs stay in its own order.
    let unique = |list: Vec<String>| -> Vec<String> {
        let mut seen = BTreeSet::new();
        list.into_iter()
            .filter(|s| seen.insert(s.clone()))
            .collect()
    };
    let tests = unique(repos.iter().flat_map(|r| r.tests.clone()).collect());
    let infra = unique(repos.iter().flat_map(|r| r.infra.clone()).collect());

    let mut choices = Vec::new();
    for role in roles.iter().filter(|r| r.plannable) {
        let (keep, why, detail) = match role.name.as_str() {
            "front" if !front.is_empty() => (true, "front", labels(&front)),
            "front" => (false, "no-front", Vec::new()),
            "back" if !back.is_empty() => (true, "back", labels(&back)),
            "back" => (false, "no-back", Vec::new()),
            "qa" if !tests.is_empty() => (true, "tests", tests.clone()),
            "qa" => (true, "no-tests", Vec::new()),
            "devops" if !infra.is_empty() => (true, "infra", infra.clone()),
            "devops" => (false, "no-infra", Vec::new()),
            "ux" if !front.is_empty() => (true, "ui", labels(&front)),
            "ux" => (false, "no-ui", Vec::new()),
            "seo" if !web.is_empty() => (true, "web", labels(&web)),
            "seo" => (false, "no-web", Vec::new()),
            "architect" => (true, "design", Vec::new()),
            _ => (true, "own", Vec::new()),
        };
        let chosen = given.keep.get(&role.name).is_some_and(|k| *k != keep);
        choices.push(RoleChoice {
            role: role.name.clone(),
            keep: keep != chosen,
            why: why.into(),
            detail,
            chosen,
        });
    }
    let kept = |role: &str| choices.iter().any(|c| c.role == role && c.keep);

    let splittable = splittable(&units);
    let large = large(repos);
    let effective = Choices {
        keep: given.keep.clone(),
        split: Some(given.split.unwrap_or(large) && splittable),
        size: Some(given.size.unwrap_or(if large { 2 } else { 1 }).clamp(1, 3)),
        model: Some(given.model.clone().unwrap_or_default()),
    };
    let size = effective.size.unwrap_or(1);
    let count = |role: &str| {
        if ["front", "back"].contains(&role) {
            size
        } else {
            1
        }
    };
    let paths = |list: &[(&str, &Part)], others: &[(&str, &Part)]| -> Vec<String> {
        if list.iter().any(|(_, p)| p.path.is_empty()) {
            return Vec::new();
        }
        let mine: Vec<&str> = list.iter().map(|(_, p)| p.path.as_str()).collect();
        let others: Vec<&str> = others.iter().map(|(_, p)| p.path.as_str()).collect();
        globs(&mine, &others)
    };
    let devops_writes: Vec<String> = infra
        .iter()
        .map(|m| match m.strip_suffix('/') {
            Some(dir) => format!("{dir}/**"),
            None => m.clone(),
        })
        .collect();

    let mut squads = Vec::new();
    if effective.split == Some(true) {
        for (id, role, list, others) in [
            ("web", "front", &front, &back),
            ("api", "back", &back, &front),
        ] {
            if !kept(role) {
                continue;
            }
            let mut members = BTreeMap::from([(role.to_owned(), size)]);
            if kept("qa") {
                members.insert("qa".into(), 1);
            }
            let mut owned = paths(list, others);
            if id == "api" && kept("devops") {
                members.insert("devops".into(), 1);
                if !owned.is_empty() {
                    owned.extend(devops_writes.iter().cloned());
                }
            }
            squads.push(Squad {
                id: id.into(),
                lead: "lead".into(),
                roles: members,
                repos: if many {
                    unique(list.iter().map(|(r, _)| r.to_string()).collect())
                } else {
                    Vec::new()
                },
                paths: owned,
                domains: unique(list.iter().map(|(_, p)| p.name.clone()).collect()),
            });
        }
        if kept("devops")
            && !squads.iter().any(|s| s.id == "api")
            && let Some(web) = squads.first_mut()
        {
            web.roles.insert("devops".into(), 1);
        }
    }
    if squads.is_empty() {
        let members: BTreeMap<String, usize> = CODE
            .iter()
            .filter(|r| kept(r))
            .map(|r| (r.to_string(), count(r)))
            .collect();
        if !members.is_empty() {
            squads.push(Squad {
                id: termf_core::squads::MAIN.into(),
                lead: "lead".into(),
                roles: members,
                ..Squad::default()
            });
        }
    }
    let staff: Vec<String> = choices
        .iter()
        .filter(|c| c.keep && !CODE.contains(&c.role.as_str()))
        .map(|c| c.role.clone())
        .collect();

    let mut settings: BTreeMap<String, Settings> = BTreeMap::new();
    let mut writes = |role: &str, list: Vec<String>| {
        if kept(role) && !list.is_empty() {
            settings.entry(role.into()).or_default().writes = Some(list);
        }
    };
    writes("qa", tests);
    writes(
        "front",
        if front.is_empty() {
            Vec::new()
        } else {
            paths(&front, &back)
        },
    );
    writes(
        "back",
        if back.is_empty() {
            Vec::new()
        } else {
            paths(&back, &front)
        },
    );
    writes("devops", devops_writes);
    let model = effective.model.clone().unwrap_or_default();
    if !model.is_empty() {
        for role in roles.iter().filter(|r| !r.plannable || kept(&r.name)) {
            if role.model.as_deref() != Some(&model) {
                settings.entry(role.name.clone()).or_default().model = Some(model.clone());
            }
        }
    }
    let workflow = if squads.len() > 1 {
        "squads"
    } else {
        "default"
    };
    let edit = Edit {
        team: Some(Composition { squads, staff }),
        workflow: Some(workflow.into()),
        roles: settings,
    };
    (effective, choices, edit)
}

/// The kept roles that have folders of their own, for the onboarding convention: each gets the project's
/// instructions that concern it through the conventions, as a preference for that role.
pub(crate) fn folders(
    items: &[Item],
    repos: &[Facts],
    questions: &[Question],
    answers: &BTreeMap<String, Vec<String>>,
) -> Vec<(String, Vec<String>)> {
    let (_, _, edit) = propose(repos, &roles(items), &answered(questions, answers));
    edit.roles
        .into_iter()
        .filter_map(|(role, s)| Some((role, s.writes?)))
        .collect()
}

fn library(config_dir: &Path, project: &str) -> Result<Vec<Item>> {
    let scope = Scope {
        project: Some(project.into()),
        ..Scope::default()
    };
    termf_library::resolve(config_dir, &scope)
}

/// The proposal for the project as it is now: a new survey, the library's roles, the owner's answers, then what
/// the card changed (`card`), each part with where the project's file stands.
pub fn team(config_dir: &Path, data_dir: &Path, project: &str, card: Choices) -> Result<Team> {
    let state = crate::load(config_dir, data_dir, project)?;
    let items = library(config_dir, project)?;
    let roles = roles(&items);
    let repos = crate::repos(config_dir, project, &test_globs(&items))?;
    let answers = match &state.analysis {
        Some(a) => answered(&a.questions, &state.answers),
        None => Choices::default(),
    };
    let given = Choices {
        keep: card.keep,
        split: card.split.or(answers.split),
        size: card.size.or(answers.size),
        model: card.model.or(answers.model),
    };
    let (choices, role_choices, proposal) = propose(&repos, &roles, &given);
    let file = termf_core::project_file(config_dir, project)?;
    let text = fs::read_to_string(&file)?;
    let before = table(&text)?;
    let after = table(&render(&text, &proposal)?)?;
    let status = proposal
        .keys()
        .into_iter()
        .map(|key| {
            let (now, then) = (held(&before, &key), held(&after, &key));
            let status = if now == then {
                Status::Same
            } else if now.is_null() {
                Status::New
            } else if state.team.get(&key) == Some(&now) {
                Status::Changed
            } else {
                Status::Edited
            };
            (key, status)
        })
        .collect();
    Ok(Team {
        choices,
        splittable: splittable(&units(&repos)),
        roles: role_choices,
        steps: roles
            .iter()
            .filter(|r| !r.plannable)
            .map(|r| r.name.clone())
            .collect(),
        proposal,
        status,
        path: file.to_string_lossy().into_owned(),
    })
}

fn table(text: &str) -> Result<toml::Table> {
    toml::from_str(text).context("invalid project file")
}

/// What the project's file holds of a part, in the shape it is compared in.
fn held(file: &toml::Table, key: &str) -> Value {
    let json = |v: Option<&toml::Value>| {
        v.and_then(|v| serde_json::to_value(v).ok())
            .unwrap_or(Value::Null)
    };
    match key {
        "team" => {
            let squads = json(file.get("squads"));
            let staff = json(file.get("team").and_then(|t| t.get("staff")));
            if squads.is_null() && staff.is_null() {
                Value::Null
            } else {
                json!({ "squads": squads, "staff": staff })
            }
        }
        "workflow" => json(file.get("workflows").and_then(|w| w.get("default"))),
        key => {
            let role = key.strip_prefix("roles.").unwrap_or(key);
            let Some(table) = file
                .get("roles")
                .and_then(|r| r.get(role))
                .and_then(|t| t.as_table())
            else {
                return Value::Null;
            };
            let kept: serde_json::Map<String, Value> = SETTINGS
                .iter()
                .filter_map(|k| Some((k.to_string(), json(Some(table.get(*k)?)))))
                .collect();
            if kept.is_empty() {
                Value::Null
            } else {
                Value::Object(kept)
            }
        }
    }
}

fn strings(list: &[String]) -> Array {
    list.iter().map(String::as_str).collect()
}

/// `edit` written into the project's file, keeping the rest of it.
fn render(text: &str, edit: &Edit) -> Result<String> {
    let mut doc: DocumentMut = text.parse().context("invalid project file")?;
    if let Some(workflow) = &edit.workflow {
        termf_core::table_in(doc.as_table_mut(), "workflows").insert("default", value(workflow));
    }
    if let Some(team) = &edit.team {
        let staff = termf_core::table_in(doc.as_table_mut(), "team");
        if team.staff.is_empty() {
            staff.remove("staff");
        } else {
            staff.insert("staff", value(strings(&team.staff)));
        }
        if staff.is_empty() {
            doc.remove("team");
        }
        let mut squads = ArrayOfTables::new();
        for s in &team.squads {
            let mut t = Table::new();
            t.insert("id", value(&s.id));
            t.insert("lead", value(&s.lead));
            if !s.roles.is_empty() {
                let mut roles = InlineTable::new();
                for (role, n) in &s.roles {
                    roles.insert(role, (*n as i64).into());
                }
                t.insert("roles", value(roles));
            }
            for (key, list) in [
                ("repos", &s.repos),
                ("paths", &s.paths),
                ("domains", &s.domains),
            ] {
                if !list.is_empty() {
                    t.insert(key, value(strings(list)));
                }
            }
            squads.push(t);
        }
        if squads.is_empty() {
            doc.remove("squads");
        } else {
            doc.insert("squads", TomlItem::ArrayOfTables(squads));
        }
    }
    for (role, s) in &edit.roles {
        let roles = termf_core::table_in(doc.as_table_mut(), "roles");
        roles.set_implicit(true);
        let table = termf_core::table_in(roles, role);
        let values = [
            ("writes", s.writes.as_deref().map(|w| value(strings(w)))),
            ("model", s.model.as_deref().map(value)),
            ("effort", s.effort.as_deref().map(value)),
        ];
        for (key, item) in values {
            match item {
                Some(item) => {
                    table.insert(key, item);
                }
                None => {
                    table.remove(key);
                }
            }
        }
        if table.is_empty() {
            roles.remove(role);
        }
        if roles.is_empty() {
            doc.remove("roles");
        }
    }
    Ok(doc.to_string())
}

/// `edit` checked against the library, written into the project's file and read back as a team: the file
/// and its text before and after.
fn written(
    config_dir: &Path,
    project: &str,
    edit: &Edit,
) -> Result<(std::path::PathBuf, String, String)> {
    let items = library(config_dir, project)?;
    let known: Vec<String> = roles(&items).into_iter().map(|r| r.name).collect();
    let role = |name: &str| {
        ensure!(
            known.iter().any(|k| k == canonical_role(name)),
            "no role named {name}"
        );
        Ok(())
    };
    if let Some(team) = &edit.team {
        let names: Vec<String> = termf_core::load_projects(config_dir)?
            .into_iter()
            .filter(|p| p.id == project)
            .flat_map(|p| p.repos.into_iter().map(|r| r.name))
            .collect();
        for s in &team.squads {
            role(&s.lead)?;
            for r in s.roles.keys() {
                role(r)?;
            }
            for repo in &s.repos {
                ensure!(
                    names.contains(repo),
                    "squad {}: no repository {repo} in the project",
                    s.id
                );
            }
        }
        for r in &team.staff {
            role(r)?;
        }
    }
    if let Some(w) = &edit.workflow {
        ensure!(
            items
                .iter()
                .any(|i| i.kind == ItemKind::Workflow && i.name == *w),
            "no workflow named {w}"
        );
    }
    for (r, s) in &edit.roles {
        role(r)?;
        if let Some(effort) = &s.effort {
            ensure!(
                EFFORTS.contains(&effort.as_str()),
                "{r}: no effort {effort:?}"
            );
        }
        if let Some(model) = &s.model {
            ensure!(
                !model.is_empty() && model.len() <= 80 && !model.contains(char::is_whitespace),
                "{r}: {model:?} is not a model"
            );
        }
        for glob in s.writes.iter().flatten() {
            let rooted = glob.starts_with(['/', '\\']) || Path::new(glob).is_absolute();
            ensure!(
                !glob.trim().is_empty() && !rooted && !glob.split(['/', '\\']).any(|c| c == ".."),
                "{r}: {glob:?} is not a path inside a repository"
            );
        }
    }
    let file = termf_core::project_file(config_dir, project)?;
    let before = fs::read_to_string(&file)?;
    let after = render(&before, edit)?;
    termf_core::squads::parse(&after).context("the team would not read back")?;
    Ok((file, before, after))
}

/// The change `edit` makes to the project's file, before it is written.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Preview {
    pub path: String,
    pub diff: String,
}

pub fn preview(config_dir: &Path, project: &str, edit: &Edit) -> Result<Preview> {
    let (file, before, after) = written(config_dir, project, edit)?;
    Ok(Preview {
        path: file.to_string_lossy().into_owned(),
        diff: termf_core::diff(&before, &after),
    })
}

/// Writes `edit` into the project's file and records it. A part changed by hand since the assistant last wrote
/// it is only replaced with `replace`.
pub fn save(
    config_dir: &Path,
    data_dir: &Path,
    project: &str,
    edit: &Edit,
    replace: bool,
) -> Result<State> {
    let state = crate::load(config_dir, data_dir, project)?;
    let keys = edit.keys();
    ensure!(!keys.is_empty(), "nothing to write");
    let (file, before, after) = written(config_dir, project, edit)?;
    let (old, new) = (table(&before)?, table(&after)?);
    for key in &keys {
        let now = held(&old, key);
        ensure!(
            replace || now.is_null() || now == held(&new, key) || state.team.get(key) == Some(&now),
            "{key} in the project's file was changed by hand since: replace it to go on"
        );
    }
    if before != after {
        termf_core::write_atomic(&file, &after)?;
    }
    crate::change(config_dir, data_dir, project, |s| {
        for key in &keys {
            s.team.insert(key.clone(), held(&new, key));
        }
        s.decisions.insert("team".into(), Decision::Accepted);
        s.log.push(Logged {
            at: crate::now(),
            what: "team".into(),
            decision: Some(Decision::Accepted),
            detail: keys.join(", "),
        });
        Ok(())
    })
}

#[cfg(test)]
mod tests;
