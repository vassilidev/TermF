//! The project's team as the assistant proposes it (ARCHITECTURE §7): what the survey finds of the code (its
//! parts, test files, CI and infrastructure), the roles that serve and those left out, then `[[squads]]`,
//! `[team] staff`, `[workflows] default` and `[roles.<name>]` in the project's file. Instructions for a role are
//! never written there: they come from the conventions, as preferences with `applies_to`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
/// The dependencies that call for a specialist, by kind of signal, in npm, PyPI, Composer, RubyGems, Go and
/// crates.io names.
const DEPS: [(&str, &[&str]); 4] = [
    (
        "data",
        &[
            "prisma",
            "@prisma/client",
            "drizzle-orm",
            "typeorm",
            "sequelize",
            "knex",
            "@mikro-orm/core",
            "mongoose",
            "sqlalchemy",
            "alembic",
            "django",
            "peewee",
            "tortoise-orm",
            "rails",
            "activerecord",
            "laravel/framework",
            "doctrine/orm",
            "doctrine/migrations",
            "gorm.io/gorm",
            "entgo.io/ent",
            "github.com/golang-migrate/migrate",
            "github.com/pressly/goose",
            "diesel",
            "sqlx",
            "sea-orm",
        ],
    ),
    (
        "llm",
        &[
            "openai",
            "@anthropic-ai/sdk",
            "anthropic",
            "@ai-sdk/openai",
            "@ai-sdk/anthropic",
            "@ai-sdk/google",
            "@google/genai",
            "google-genai",
            "google-generativeai",
            "@mistralai/mistralai",
            "mistralai",
            "cohere-ai",
            "cohere",
            "langchain",
            "@langchain/core",
            "llamaindex",
            "llama-index",
            "litellm",
            "ollama",
            "groq-sdk",
            "async-openai",
            "github.com/openai/openai-go",
            "github.com/anthropics/anthropic-sdk-go",
            "github.com/sashabaranov/go-openai",
            "openai-php/client",
            "ruby-openai",
        ],
    ),
    (
        "analytics",
        &[
            "posthog-js",
            "posthog-node",
            "posthog",
            "@segment/analytics-next",
            "@segment/analytics-node",
            "analytics-node",
            "segment-analytics-python",
            "@amplitude/analytics-browser",
            "@amplitude/analytics-node",
            "amplitude-analytics",
            "mixpanel-browser",
            "mixpanel",
            "@vercel/analytics",
            "plausible-tracker",
            "react-ga4",
            "@rudderstack/analytics-js",
            "@snowplow/browser-tracker",
        ],
    ),
    (
        "integrations",
        &[
            "stripe",
            "@stripe/stripe-js",
            "stripe/stripe-php",
            "github.com/stripe/stripe-go",
            "async-stripe",
            "@paypal/paypal-js",
            "braintree",
            "@adyen/api-library",
            "@mollie/api-client",
            "mollie/mollie-api-php",
            "@paddle/paddle-node-sdk",
            "openid-client",
            "next-auth",
            "@auth/core",
            "@node-saml/passport-saml",
            "@node-saml/node-saml",
            "passport-saml",
            "samlify",
            "python3-saml",
            "pysaml2",
            "authlib",
            "omniauth-saml",
            "omniauth_openid_connect",
            "ruby-saml",
            "onelogin/php-saml",
            "laravel/socialite",
            "github.com/coreos/go-oidc",
            "github.com/crewjam/saml",
            "openidconnect",
            "@workos-inc/node",
            "svix",
            "@octokit/webhooks",
            "standardwebhooks",
        ],
    ),
];
/// Manifests read for those dependencies, and the most markers kept of one kind.
const MAX_MANIFEST_READS: usize = 50;
const MAX_SIGNALS: usize = 6;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Part {
    /// Relative to the repository, `""` for the whole of it.
    pub path: String,
    pub name: String,
    /// `front` (a user interface) or `back` (anything else: a server, a library, a tool).
    pub role: String,
    /// A public website, which `seo` serves; a desktop app's interface is not one.
    pub web: bool,
}

pub(crate) fn survey(repo: &Path, facts: &mut Facts, tests: &[String]) {
    let files = files(repo);
    (facts.workspace, facts.parts) = parts(repo);
    facts.tests = tests
        .iter()
        .filter(|g| files.iter().any(|f| termf_team::rules::glob(g, f)))
        .cloned()
        .collect();
    facts.infra = infra(&files);
    facts.signals = signals(repo, &files);
}

/// What calls for a specialist: the dependencies of `DEPS` its manifests list, its migrations and schema files
/// (`data`), its docs folder, API specs and changelog (`docs`).
fn signals(repo: &Path, files: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let manifests = files.iter().filter(|f| {
        let (dir, name) = f.rsplit_once('/').unwrap_or(("", f));
        let requirements = name.ends_with(".txt")
            && (name.starts_with("requirements") || dir.rsplit('/').next() == Some("requirements"));
        MANIFESTS.contains(&name) || requirements
    });
    for file in manifests.take(MAX_MANIFEST_READS) {
        let Ok(text) = fs::read_to_string(repo.join(file)) else {
            continue;
        };
        let listed = normal(&dependencies(file, &text));
        for (kind, deps) in DEPS {
            for dep in deps.iter().filter(|d| names(&listed, d)) {
                found
                    .entry(kind.into())
                    .or_default()
                    .insert(dep.to_string());
            }
        }
    }
    for f in files {
        let dirs: Vec<&str> = f.split('/').collect();
        let (name, folders) = dirs.split_last().unwrap_or((&"", &[]));
        // Folder names in any case: Entity Framework writes `Migrations/`.
        let lower: Vec<String> = folders.iter().map(|d| d.to_lowercase()).collect();
        let upto = |i: usize| format!("{}/", folders[..=i].join("/"));
        let at = |dir: &[&str]| lower.windows(dir.len()).position(|w| w == dir);
        let data = at(&["migrations"])
            .map(upto)
            .or_else(|| at(&["db", "migrate"]).map(|i| upto(i + 1)))
            .or_else(|| at(&["alembic", "versions"]).map(|i| upto(i + 1)))
            .or_else(|| name.ends_with(".prisma").then(|| f.clone()));
        if let Some(marker) = data {
            found.entry("data".into()).or_default().insert(marker);
        }
        let spec = ["openapi.", "swagger."].iter().any(|p| name.starts_with(p))
            && [".yaml", ".yml", ".json"].iter().any(|e| name.ends_with(e));
        let docs = if lower.first().is_some_and(|d| d == "docs") {
            Some(format!("{}/", folders[0]))
        } else if spec || (folders.is_empty() && *name == "CHANGELOG.md") {
            Some(f.clone())
        } else {
            None
        };
        if let Some(marker) = docs {
            found.entry("docs".into()).or_default().insert(marker);
        }
    }
    found
        .into_iter()
        .map(|(kind, set)| (kind, set.into_iter().take(MAX_SIGNALS).collect()))
        .collect()
}

/// The dependencies a manifest lists, one a line: the keys of a JSON manifest's dependency tables, the packages of
/// a TOML one's, a Gemfile's `gem` lines, a `go.mod` or a requirements file without comments. A name or a
/// description is no dependency.
fn dependencies(file: &str, text: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    let uncommented = |mark: &str| -> Vec<String> {
        text.lines()
            .map(|l| l.split(mark).next().unwrap_or_default().trim().to_owned())
            .filter(|l| !l.is_empty())
            .collect()
    };
    let list: Vec<String> = match name {
        _ if name.ends_with(".json") => {
            let Ok(json) = serde_json::from_str::<Value>(text) else {
                return String::new();
            };
            [
                "dependencies",
                "devDependencies",
                "peerDependencies",
                "optionalDependencies",
                "require",
                "require-dev",
            ]
            .iter()
            .filter_map(|k| json[*k].as_object())
            .flat_map(|o| o.keys().cloned())
            .collect()
        }
        "pyproject.toml" | "Cargo.toml" => {
            let mut list = Vec::new();
            if let Ok(table) = toml::from_str::<toml::Table>(text) {
                toml_dependencies(&table, &mut list);
            }
            list
        }
        "Gemfile" => uncommented("#")
            .into_iter()
            .filter(|l| l.starts_with("gem ") || l.starts_with("gem("))
            .collect(),
        "go.mod" => uncommented("//")
            .into_iter()
            .filter(|l| !l.starts_with("module "))
            .collect(),
        _ => uncommented("#"),
    };
    list.join("\n")
}

/// The packages of the dependency tables of a Cargo or Poetry manifest (renamed ones by their `package` too),
/// and the requirements of PEP 621 and PEP 735 lists, at any depth (`[workspace.dependencies]`,
/// `[tool.poetry.group.dev.dependencies]`, `[target.'cfg(…)'.dependencies]`).
fn toml_dependencies(table: &toml::Table, list: &mut Vec<String>) {
    let strings = |items: &[toml::Value]| -> Vec<String> {
        items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    };
    for (key, value) in table {
        let deps = key.ends_with("dependencies") || key == "dependency-groups";
        match value {
            toml::Value::Array(items) if deps => list.extend(strings(items)),
            toml::Value::Table(packages) if deps => {
                for (package, spec) in packages {
                    match spec {
                        // An extra or a group: its requirements.
                        toml::Value::Array(items) => list.extend(strings(items)),
                        spec => {
                            list.push(package.clone());
                            if let Some(real) = spec.get("package").and_then(|p| p.as_str()) {
                                list.push(real.to_owned());
                            }
                        }
                    }
                }
            }
            toml::Value::Table(inner) => toml_dependencies(inner, list),
            _ => {}
        }
    }
}

/// A package name as registries compare them: PyPI and Composer ignore case, and PyPI takes `_`, `-` and `.` for
/// one another (`llama_index` is `llama-index`).
fn normal(name: &str) -> String {
    name.to_lowercase().replace(['_', '.'], "-")
}

/// Whether `text`, made `normal`, names the package `name` whole: `stripe` in `gem "stripe"` or `stripe==9`, not
/// in `stripe-mock`; a Go module also before its major version (`github.com/stripe/stripe-go/v82`).
fn names(text: &str, name: &str) -> bool {
    let edge =
        |c: Option<char>| c.is_none_or(|c| c.is_whitespace() || "\"'=<>~!^,[](){}:;#".contains(c));
    let go = name.contains('.') && name.contains('/');
    let name = normal(name);
    text.match_indices(&name).any(|(i, _)| {
        let after = text[i + name.len()..].chars().next();
        edge(text[..i].chars().next_back()) && (edge(after) || (go && after == Some('/')))
    })
}

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

/// The repository itself counts unless its manifest only holds a workspace's tooling.
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

/// Enough YAML for a `pnpm-workspace.yaml`: `key: [..]`, or `- item` lines under `key:`.
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

pub(crate) struct RoleInfo {
    name: String,
    plannable: bool,
}

pub(crate) fn roles(items: &[Item]) -> Vec<RoleInfo> {
    items
        .iter()
        .filter(|i| i.kind == ItemKind::Role && (i.layer != Layer::Repo || i.active))
        .filter(|i| canonical_role(&i.name) == i.name)
        .map(|i| RoleInfo {
            name: i.name.clone(),
            plannable: termf_library::plannable(i),
        })
        .collect()
}

/// From the frontmatter, not the layered settings: those hold what the assistant narrowed last time.
pub(crate) fn test_globs(items: &[Item]) -> Vec<String> {
    items
        .iter()
        .find(|i| i.kind == ItemKind::Role && i.name == "qa")
        .map(|i| termf_library::list_of(i.meta.get("writes")))
        .unwrap_or_default()
}

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

/// What only the owner can say of the team. Their text is a key of the interface's translations and their
/// options are codes: the interface words them, like the rest of it.
pub(crate) fn questions(repos: &[Facts]) -> Vec<Question> {
    let large = large(repos);
    let question = |id: &str, text: &str, options: &[&str], default: usize| Question {
        id: id.into(),
        theme: Theme::Agents,
        text: text.into(),
        kind: Kind::Choice,
        options: options.iter().map(|o| o.to_string()).collect(),
        default: vec![options[default].into()],
    };
    let mut asked = vec![question(
        SIZE,
        "obTeamQSize",
        &["1", "2", "3"],
        usize::from(large),
    )];
    if splittable(&units(repos)) {
        asked.push(question(
            SPLIT,
            "obTeamQSquads",
            &["one", "split"],
            usize::from(large),
        ));
    }
    asked.push(question(
        MODEL,
        "obTeamQModel",
        &["own", "sonnet", "opus"],
        0,
    ));
    asked
}

/// What the owner chose of the team: in the answers to the assistant's questions, then in the card.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Choices {
    /// Against what the survey proposes.
    pub keep: BTreeMap<String, bool>,
    pub split: Option<bool>,
    /// Sessions at once of each code role (front, back).
    pub size: Option<usize>,
    /// `""` keeps each role's own.
    pub model: Option<String>,
}

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
    /// A code the interface words, with `detail`: `own` is a role of the user's library.
    pub why: String,
    pub detail: Vec<String>,
    /// Kept or left out by the owner, against the survey.
    pub chosen: bool,
}

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

/// Only the parts the owner accepted.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Edit {
    pub team: Option<Composition>,
    pub workflow: Option<String>,
    pub roles: BTreeMap<String, Settings>,
}

impl Edit {
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
    New,
    Same,
    /// The file has what the assistant wrote last time: replacing it loses nothing.
    Changed,
    /// The file has something else, written by hand: only replaced once asked to.
    Edited,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    /// Each filled in.
    pub choices: Choices,
    pub splittable: bool,
    /// The roles that take tasks.
    pub roles: Vec<RoleChoice>,
    /// The roles that are workflow steps rather than tasks.
    pub steps: Vec<String>,
    pub proposal: Edit,
    pub status: BTreeMap<String, Status>,
    /// The parts the file holds as written by hand, `same` ones too: writing them takes `replace`.
    pub hand: Vec<String>,
    pub path: String,
}

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
    // A file or a folder, as `labels` names a part: with its repository's name when there are several.
    let found = |kind: &str| -> Vec<String> {
        let dependency = |m: &str| DEPS.iter().any(|(_, list)| list.contains(&m));
        unique(
            repos
                .iter()
                .flat_map(|r| {
                    let markers = r.signals.get(kind).cloned().unwrap_or_default();
                    markers.into_iter().map(|m| match many && !dependency(&m) {
                        true => format!("{}/{m}", r.name),
                        false => m,
                    })
                })
                .collect(),
        )
    };
    let signal = |kind: &'static str, none: &'static str| match found(kind) {
        list if list.is_empty() => (false, none, list),
        list => (true, kind, list),
    };
    // Payments, sign-in and analytics handle personal data, which the law frames even without a public site.
    let personal = unique([found("integrations"), found("analytics")].concat());

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
            "ux" | "brand" | "copy" if !front.is_empty() => (true, "ui", labels(&front)),
            "ux" | "brand" | "copy" => (false, "no-ui", Vec::new()),
            "seo" | "motion" if !web.is_empty() => (true, "web", labels(&web)),
            "seo" | "motion" => (false, "no-web", Vec::new()),
            "a11y" if !front.is_empty() => (true, "ui", labels(&front)),
            "a11y" => (false, "no-ui", Vec::new()),
            "dba" => signal("data", "no-data"),
            "ai" => signal("llm", "no-llm"),
            "analytics" => signal("analytics", "no-analytics"),
            "integrations" => signal("integrations", "no-integrations"),
            "docs" => signal("docs", "no-docs"),
            "sre" if !back.is_empty() && !infra.is_empty() => (true, "service", infra.clone()),
            "sre" => (false, "no-service", Vec::new()),
            "legal" if !web.is_empty() => (true, "web", labels(&web)),
            "legal" if !personal.is_empty() => (true, "personal", personal.clone()),
            "legal" => (false, "no-public", Vec::new()),
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
        // Even a role whose frontmatter has that model: a global `[roles.<name>]` could set another.
        for role in roles.iter().filter(|r| !r.plannable || kept(&r.name)) {
            settings.entry(role.name.clone()).or_default().model = Some(model.clone());
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

/// The last survey of each project (by state file), so that a choice in the card re-proposes without reading
/// every repository again.
static SURVEYS: Mutex<BTreeMap<PathBuf, Vec<Facts>>> = Mutex::new(BTreeMap::new());

/// The proposal from the library's roles, the owner's answers, then what the card changed (`card`), on a new
/// survey when `fresh`, else on the last one. A setting the assistant wrote and no longer proposes becomes a
/// part that removes it.
pub fn team(
    config_dir: &Path,
    data_dir: &Path,
    project: &str,
    card: Choices,
    fresh: bool,
) -> Result<Team> {
    let state = crate::load(config_dir, data_dir, project)?;
    let items = library(config_dir, project)?;
    let roles = roles(&items);
    let key = crate::file(config_dir, data_dir, project)?;
    let kept = match fresh {
        true => None,
        false => SURVEYS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned(),
    };
    let repos = match kept {
        Some(repos) => repos,
        None => {
            let repos = crate::repos(config_dir, project, &test_globs(&items))?;
            SURVEYS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(key, repos.clone());
            repos
        }
    };
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
    let (choices, role_choices, mut proposal) = propose(&repos, &roles, &given);
    let file = termf_core::project_file(config_dir, project)?;
    let text = fs::read_to_string(&file)?;
    let before = table(&text)?;
    for role in state.team.keys().filter_map(|k| k.strip_prefix("roles.")) {
        if !held(&before, &format!("roles.{role}")).is_null() {
            proposal.roles.entry(role.to_owned()).or_default();
        }
    }
    let after = table(&render(&text, &proposal)?)?;
    let (mut status, mut hand) = (BTreeMap::new(), Vec::new());
    for key in proposal.keys() {
        let (now, then) = (held(&before, &key), held(&after, &key));
        let by_hand = !now.is_null() && state.team.get(&key) != Some(&now);
        let found = if now == then {
            Status::Same
        } else if now.is_null() {
            Status::New
        } else if by_hand {
            Status::Edited
        } else {
            Status::Changed
        };
        if by_hand {
            hand.push(key.clone());
        }
        status.insert(key, found);
    }
    Ok(Team {
        hand,
        choices,
        splittable: splittable(&units(&repos)),
        roles: role_choices,
        // The expert answers questions about the project: no workflow runs it.
        steps: roles
            .iter()
            .filter(|r| !r.plannable && r.name != "expert")
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

/// Checked against the library, and read back as a team before anything is written.
fn written(config_dir: &Path, project: &str, edit: &Edit) -> Result<(PathBuf, String, String)> {
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

/// A part changed by hand since the assistant last wrote it is only replaced with `replace`.
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
