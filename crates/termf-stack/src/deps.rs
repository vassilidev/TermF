//! A repository's direct dependencies, read from its manifests and lockfiles only: nothing is run.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use termf_library::stack::clean;
use yaml_rust2::{Yaml, YamlLoader};

/// As OSV names them.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ecosystem {
    #[serde(rename = "npm")]
    Npm,
    Packagist,
    #[serde(rename = "crates.io")]
    Crates,
    PyPI,
    Go,
    RubyGems,
}

impl Ecosystem {
    pub fn osv(self) -> &'static str {
        match self {
            Ecosystem::Npm => "npm",
            Ecosystem::Packagist => "Packagist",
            Ecosystem::Crates => "crates.io",
            Ecosystem::PyPI => "PyPI",
            Ecosystem::Go => "Go",
            Ecosystem::RubyGems => "RubyGems",
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Dep {
    pub name: String,
    pub ecosystem: Ecosystem,
    /// npm, pnpm, yarn, bun, composer, cargo, uv, poetry, pip, go or bundler.
    pub manager: String,
    /// As the manifest asks for it: `^18.2.0`, `>=2,<3`, `~> 7.1`; empty when it names no version.
    pub range: String,
    /// What the lockfile holds, else the lowest version the range accepts; `None` for a path, a workspace or a git
    /// dependency.
    pub version: Option<String>,
    /// Read from a lockfile (or pinned exactly).
    pub locked: bool,
    /// Only for development or tests.
    pub dev: bool,
    /// The manifest, relative to the repository.
    pub manifest: String,
}

const SKIPPED: [&str; 14] = [
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "venv",
    "__pycache__",
    "fixtures",
    "testdata",
    "Pods",
    "bower_components",
    "site-packages",
];
const MANIFEST_DEPTH: usize = 3;
const MAX_MANIFESTS: usize = 50;
const MANIFESTS: [&str; 7] = [
    "package.json",
    "composer.json",
    "Cargo.toml",
    "pyproject.toml",
    "requirements.txt",
    "go.mod",
    "Gemfile",
];

/// The manifests of `repo`, relative with `/`, shallowest first (a monorepo's packages too).
pub fn manifests(repo: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut queue = VecDeque::from([(repo.to_path_buf(), String::new(), 0)]);
    while let Some((dir, rel, depth)) = queue.pop_front() {
        let mut entries: Vec<_> = fs::read_dir(&dir).into_iter().flatten().flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
            if is_dir {
                if depth < MANIFEST_DEPTH
                    && !name.starts_with('.')
                    && !SKIPPED.contains(&name.as_str())
                {
                    queue.push_back((entry.path(), path, depth + 1));
                }
            } else if MANIFESTS.contains(&name.as_str())
                || (name.starts_with("requirements") && name.ends_with(".txt"))
            {
                found.push(path);
                if found.len() >= MAX_MANIFESTS {
                    return found;
                }
            }
        }
    }
    found
}

/// The lockfiles read once per repository, by relative path.
#[derive(Default)]
struct Locks {
    texts: HashMap<String, Option<String>>,
    json: HashMap<String, Option<Value>>,
    yaml: HashMap<String, Option<Yaml>>,
}

impl Locks {
    fn text(&mut self, repo: &Path, rel: &str) -> Option<String> {
        self.texts
            .entry(rel.into())
            .or_insert_with(|| fs::read_to_string(repo.join(rel)).ok())
            .clone()
    }

    /// A JSON or TOML file, as JSON.
    fn json(&mut self, repo: &Path, rel: &str) -> Option<Value> {
        if let Some(v) = self.json.get(rel) {
            return v.clone();
        }
        let value = self.text(repo, rel).and_then(|text| {
            if rel.ends_with(".json") || rel.ends_with("composer.lock") {
                serde_json::from_str(&text).ok()
            } else {
                toml::from_str::<toml::Table>(&text)
                    .ok()
                    .and_then(|t| serde_json::to_value(t).ok())
            }
        });
        self.json.insert(rel.into(), value.clone());
        value
    }

    fn yaml(&mut self, repo: &Path, rel: &str) -> Option<Yaml> {
        if let Some(v) = self.yaml.get(rel) {
            return v.clone();
        }
        let value = self
            .text(repo, rel)
            .and_then(|t| YamlLoader::load_from_str(&t).ok())
            .and_then(|docs| docs.into_iter().next());
        self.yaml.insert(rel.into(), value.clone());
        value
    }
}

fn dir_of(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(d, _)| d)
}

fn join(dir: &str, file: &str) -> String {
    if dir.is_empty() {
        file.into()
    } else {
        format!("{dir}/{file}")
    }
}

/// The first of `files` in `dir` or one of its parents up to the repository: a workspace's lockfile is at its root.
fn nearest(repo: &Path, dir: &str, files: &[&str]) -> Option<(String, String)> {
    let mut dir = dir.to_owned();
    loop {
        for file in files {
            let rel = join(&dir, file);
            if repo.join(&rel).is_file() {
                return Some((dir, rel));
            }
        }
        if dir.is_empty() {
            return None;
        }
        dir = dir_of(&dir).to_owned();
    }
}

/// The direct dependencies of every manifest of `repo`, each once: the same package at the same version in several
/// manifests is one dependency, from the first of them, for development only when it is so everywhere.
pub fn read(repo: &Path) -> Vec<Dep> {
    let mut locks = Locks::default();
    let mut all: Vec<Dep> = Vec::new();
    for rel in manifests(repo) {
        let file = rel.rsplit('/').next().unwrap_or(&rel);
        let found = match file {
            "package.json" => node(repo, &rel, &mut locks),
            "composer.json" => php(repo, &rel, &mut locks),
            "Cargo.toml" => rust(repo, &rel, &mut locks),
            "go.mod" => go(repo, &rel, &mut locks),
            "Gemfile" => ruby(repo, &rel, &mut locks),
            _ => python(repo, &rel, &mut locks),
        };
        for dep in found {
            match all.iter_mut().find(|d| {
                d.ecosystem == dep.ecosystem && d.name == dep.name && d.version == dep.version
            }) {
                Some(known) => known.dev &= dep.dev,
                None => all.push(dep),
            }
        }
    }
    all
}

fn node(repo: &Path, rel: &str, locks: &mut Locks) -> Vec<Dep> {
    let Some(pkg) = locks.json(repo, rel) else {
        return Vec::new();
    };
    let dir = dir_of(rel);
    let lock = nearest(
        repo,
        dir,
        &[
            "pnpm-lock.yaml",
            "bun.lock",
            "bun.lockb",
            "yarn.lock",
            "package-lock.json",
        ],
    );
    let manager = match lock
        .as_ref()
        .map(|(_, l)| l.rsplit('/').next().unwrap_or(l))
    {
        Some("pnpm-lock.yaml") => "pnpm",
        Some("bun.lock" | "bun.lockb") => "bun",
        Some("yarn.lock") => "yarn",
        Some(_) => "npm",
        None => match pkg["packageManager"]
            .as_str()
            .unwrap_or_default()
            .split('@')
            .next()
        {
            Some("pnpm") => "pnpm",
            Some("yarn") => "yarn",
            Some("bun") => "bun",
            _ => "npm",
        },
    };
    let mut deps = Vec::new();
    for (key, dev) in [
        ("dependencies", false),
        ("optionalDependencies", false),
        ("devDependencies", true),
    ] {
        for (name, spec) in pkg[key].as_object().into_iter().flatten() {
            let spec = spec.as_str().unwrap_or_default();
            // Another package of the workspace, a folder, git or an alias: no registry version to compare.
            if spec.contains(':') || spec.contains('/') {
                continue;
            }
            let locked = lock
                .as_ref()
                .and_then(|(at, file)| node_locked(repo, at, file, dir, name, spec, locks))
                .or_else(|| {
                    let installed = join(dir, &format!("node_modules/{name}/package.json"));
                    locks.json(repo, &installed)?["version"]
                        .as_str()
                        .map(str::to_owned)
                });
            deps.push(Dep {
                name: name.clone(),
                ecosystem: Ecosystem::Npm,
                manager: manager.into(),
                range: spec.into(),
                locked: locked.is_some(),
                version: locked.or_else(|| clean(spec)),
                dev,
                manifest: rel.into(),
            });
        }
    }
    deps
}

/// The version a Node lockfile at `at` holds for `name` of the package in `dir`.
fn node_locked(
    repo: &Path,
    at: &str,
    file: &str,
    dir: &str,
    name: &str,
    spec: &str,
    locks: &mut Locks,
) -> Option<String> {
    let sub = dir[at.len()..].trim_start_matches('/');
    match file.rsplit('/').next().unwrap_or(file) {
        "package-lock.json" => {
            let lock = locks.json(repo, file)?;
            [
                join(sub, &format!("node_modules/{name}")),
                format!("node_modules/{name}"),
            ]
            .iter()
            .find_map(|key| lock["packages"][key]["version"].as_str())
            .or(lock["dependencies"][name]["version"].as_str())
            .map(str::to_owned)
        }
        "pnpm-lock.yaml" => {
            let lock = locks.yaml(repo, file)?;
            let importer = &lock["importers"][if sub.is_empty() { "." } else { sub }];
            ["dependencies", "devDependencies", "optionalDependencies"]
                .iter()
                .find_map(|key| {
                    let entry = &importer[*key][name];
                    let v = entry["version"].as_str().or(entry.as_str())?;
                    clean(v.split('(').next().unwrap_or(v))
                })
        }
        "yarn.lock" => {
            let text = locks.text(repo, file)?;
            let mut chosen: Option<(bool, String)> = None;
            let mut header: Option<bool> = None;
            for line in text.lines() {
                if !line.starts_with(' ') && line.ends_with(':') {
                    let descriptors: Vec<&str> = line
                        .trim_end_matches(':')
                        .split(", ")
                        .map(|d| d.trim_matches('"'))
                        .collect();
                    header = descriptors
                        .iter()
                        .any(|d| d.starts_with(&format!("{name}@")))
                        .then(|| {
                            descriptors.iter().any(|d| {
                                *d == format!("{name}@{spec}") || *d == format!("{name}@npm:{spec}")
                            })
                        });
                } else if let Some(exact) = header
                    && let Some(v) = line.trim().strip_prefix("version")
                {
                    let v = v.trim_matches([':', ' ', '"']).to_owned();
                    if exact {
                        return Some(v);
                    }
                    chosen.get_or_insert((exact, v));
                    header = None;
                }
            }
            chosen.map(|(_, v)| v)
        }
        "bun.lock" => {
            let text = locks.text(repo, file)?;
            let marker = format!("\"{name}\": [\"{name}@");
            let at = text.find(&marker)? + marker.len();
            Some(text[at..].split('"').next()?.to_owned())
        }
        _ => None,
    }
}

fn php(repo: &Path, rel: &str, locks: &mut Locks) -> Vec<Dep> {
    let Some(doc) = locks.json(repo, rel) else {
        return Vec::new();
    };
    let lock = locks
        .json(repo, &join(dir_of(rel), "composer.lock"))
        .unwrap_or_default();
    let mut deps = Vec::new();
    for (key, dev) in [("require", false), ("require-dev", true)] {
        for (name, spec) in doc[key].as_object().into_iter().flatten() {
            // The platform: PHP itself and its extensions.
            if !name.contains('/') {
                continue;
            }
            let spec = spec.as_str().unwrap_or_default();
            let locked = ["packages", "packages-dev"].iter().find_map(|k| {
                lock[*k].as_array()?.iter().find_map(|p| {
                    (p["name"].as_str()?.eq_ignore_ascii_case(name))
                        .then(|| p["version"].as_str().and_then(clean))?
                })
            });
            deps.push(Dep {
                name: name.to_ascii_lowercase(),
                ecosystem: Ecosystem::Packagist,
                manager: "composer".into(),
                range: spec.into(),
                locked: locked.is_some(),
                version: locked.or_else(|| clean(spec)),
                dev,
                manifest: rel.into(),
            });
        }
    }
    deps
}

fn rust(repo: &Path, rel: &str, locks: &mut Locks) -> Vec<Dep> {
    let Some(doc) = locks.json(repo, rel) else {
        return Vec::new();
    };
    let lock = nearest(repo, dir_of(rel), &["Cargo.lock"])
        .and_then(|(_, file)| locks.json(repo, &file))
        .unwrap_or_default();
    let mut tables: Vec<(&Value, bool)> = vec![
        (&doc["dependencies"], false),
        (&doc["build-dependencies"], false),
        (&doc["dev-dependencies"], true),
        (&doc["workspace"]["dependencies"], false),
    ];
    for target in doc["target"]
        .as_object()
        .into_iter()
        .flat_map(|t| t.values())
    {
        tables.push((&target["dependencies"], false));
        tables.push((&target["dev-dependencies"], true));
    }
    let mut deps = Vec::new();
    for (table, dev) in tables {
        for (key, spec) in table.as_object().into_iter().flatten() {
            // A path or git dependency, or one the workspace's table gives (read there).
            if spec.get("path").is_some()
                || spec.get("git").is_some()
                || spec.get("workspace").is_some()
            {
                continue;
            }
            let name = spec["package"].as_str().unwrap_or(key);
            let range = spec
                .as_str()
                .or(spec["version"].as_str())
                .unwrap_or_default();
            let wanted = clean(range);
            let mut versions: Vec<&str> = lock["package"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| {
                    p["name"].as_str() == Some(name)
                        && p["source"]
                            .as_str()
                            .is_some_and(|s| s.starts_with("registry+"))
                })
                .filter_map(|p| p["version"].as_str())
                .collect();
            // Several versions locked: the one this requirement reaches.
            versions
                .sort_by(|a, b| crate::version::compare(a, b).unwrap_or(std::cmp::Ordering::Equal));
            let locked = versions
                .iter()
                .rev()
                .find(|v| {
                    wanted.as_deref().is_none_or(|w| {
                        !matches!(
                            crate::version::bump(w, v),
                            Some(crate::version::Bump::Major)
                        ) && crate::version::compare(v, w).is_some_and(|o| o.is_ge())
                    })
                })
                .or(versions.last())
                .map(|v| v.to_string());
            deps.push(Dep {
                name: name.into(),
                ecosystem: Ecosystem::Crates,
                manager: "cargo".into(),
                range: range.into(),
                locked: locked.is_some(),
                version: locked.or(wanted),
                dev,
                manifest: rel.into(),
            });
        }
    }
    deps
}

/// PEP 503: `Flask_Login` and `flask.login` are `flask-login`.
fn normalized(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if matches!(c, '-' | '_' | '.') {
            if !out.ends_with('-') {
                out.push('-');
            }
        } else {
            out.push(c.to_ascii_lowercase());
        }
    }
    out
}

/// A PEP 508 requirement: its name and what it asks of the version.
fn requirement(line: &str) -> Option<(String, String)> {
    let line = line.split(';').next()?.trim();
    let end = line
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(line.len());
    let rest = line[end..].trim_start();
    let rest = match rest.strip_prefix('[') {
        Some(extras) => extras.split_once(']').map_or("", |(_, r)| r),
        None => rest,
    };
    if rest.starts_with('@') || end == 0 {
        return None;
    }
    Some((normalized(&line[..end]), rest.trim().to_owned()))
}

fn python(repo: &Path, rel: &str, locks: &mut Locks) -> Vec<Dep> {
    let dir = dir_of(rel);
    let mut wanted: Vec<(String, String, bool)> = Vec::new();
    let mut poetry = false;
    if rel.ends_with("pyproject.toml") {
        let Some(doc) = locks.json(repo, rel) else {
            return Vec::new();
        };
        let project = &doc["project"];
        let mut lists = vec![(&project["dependencies"], false)];
        lists.extend(
            project["optional-dependencies"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.values().map(|v| (v, false))),
        );
        lists.extend(
            doc["dependency-groups"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.values().map(|v| (v, true))),
        );
        lists.push((&doc["tool"]["uv"]["dev-dependencies"], true));
        for (list, dev) in lists {
            for (name, spec) in list
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| requirement(d.as_str()?))
            {
                wanted.push((name, spec, dev));
            }
        }
        let tool = &doc["tool"]["poetry"];
        poetry = tool.is_object();
        let mut tables = vec![
            (&tool["dependencies"], false),
            (&tool["dev-dependencies"], true),
        ];
        tables.extend(
            tool["group"]
                .as_object()
                .into_iter()
                .flat_map(|g| g.values().map(|g| (&g["dependencies"], true))),
        );
        for (table, dev) in tables {
            for (name, spec) in table.as_object().into_iter().flatten() {
                if name == "python" || spec.get("path").is_some() || spec.get("git").is_some() {
                    continue;
                }
                let spec = spec
                    .as_str()
                    .or(spec["version"].as_str())
                    .unwrap_or_default();
                wanted.push((normalized(name), spec.into(), dev));
            }
        }
    } else {
        let file = rel.rsplit('/').next().unwrap_or(rel);
        let dev = file.contains("dev") || file.contains("test");
        let text = locks.text(repo, rel).unwrap_or_default();
        for line in text.lines() {
            let line = line.split(" #").next().unwrap_or_default().trim();
            if line.is_empty() || line.starts_with(['#', '-']) || line.contains("://") {
                continue;
            }
            if let Some((name, spec)) = requirement(line) {
                wanted.push((name, spec, dev));
            }
        }
    }
    let lock = nearest(repo, dir, &["uv.lock", "poetry.lock"]);
    let manager = match lock.as_ref().map(|(_, l)| l.ends_with("uv.lock")) {
        Some(true) => "uv",
        Some(false) => "poetry",
        None if poetry => "poetry",
        None => "pip",
    };
    let lock = lock
        .and_then(|(_, file)| locks.json(repo, &file))
        .unwrap_or_default();
    wanted
        .into_iter()
        .map(|(name, spec, dev)| {
            let locked = lock["package"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|p| {
                    (normalized(p["name"].as_str()?) == name)
                        .then(|| p["version"].as_str().map(str::to_owned))?
                });
            let pinned = spec
                .strip_prefix("==")
                .filter(|v| !v.contains([',', '*']))
                .map(|v| v.trim().to_owned());
            let locked = locked.or(pinned);
            Dep {
                name,
                ecosystem: Ecosystem::PyPI,
                manager: manager.into(),
                locked: locked.is_some(),
                version: locked.or_else(|| clean(&spec)),
                range: spec,
                dev,
                manifest: rel.into(),
            }
        })
        .collect()
}

fn go(repo: &Path, rel: &str, locks: &mut Locks) -> Vec<Dep> {
    let text = locks.text(repo, rel).unwrap_or_default();
    let mut deps = Vec::new();
    let mut block = false;
    let replaced: Vec<&str> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("replace "))
        .filter(|l| l.contains("=> ./") || l.contains("=> ../"))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("require (") {
            block = true;
            continue;
        }
        if block && line.starts_with(')') {
            block = false;
            continue;
        }
        let entry = match line.strip_prefix("require ") {
            Some(e) => e,
            None if block => line,
            None => continue,
        };
        if entry.contains("// indirect") {
            continue;
        }
        let mut parts = entry.split_whitespace();
        let (Some(module), Some(version)) = (parts.next(), parts.next()) else {
            continue;
        };
        if replaced.contains(&module) {
            continue;
        }
        deps.push(Dep {
            name: module.into(),
            ecosystem: Ecosystem::Go,
            manager: "go".into(),
            range: version.into(),
            version: Some(version.into()),
            locked: true,
            dev: false,
            manifest: rel.into(),
        });
    }
    deps
}

fn ruby(repo: &Path, rel: &str, locks: &mut Locks) -> Vec<Dep> {
    let text = locks.text(repo, rel).unwrap_or_default();
    let lock = locks
        .text(repo, &join(dir_of(rel), "Gemfile.lock"))
        .unwrap_or_default();
    let mut specs = BTreeMap::new();
    let mut in_gems = false;
    for line in lock.lines() {
        if !line.starts_with(' ') {
            in_gems = line == "GEM";
        } else if in_gems
            && let Some(spec) = line.strip_prefix("    ")
            && !spec.starts_with(' ')
            && let Some((name, version)) = spec.split_once(" (")
        {
            specs.insert(name.to_owned(), version.trim_end_matches(')').to_owned());
        }
    }
    let mut deps = Vec::new();
    let mut groups: Vec<bool> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with("group ") && line.ends_with(" do") {
            groups.push(line.contains(":development") || line.contains(":test"));
            continue;
        }
        if line.ends_with(" do") || line == "do" {
            groups.push(groups.last().copied().unwrap_or(false));
            continue;
        }
        if line == "end" {
            groups.pop();
            continue;
        }
        let Some(rest) = line.strip_prefix("gem ") else {
            continue;
        };
        let mut args = rest.split(',').map(|a| a.trim().trim_matches(['"', '\'']));
        let Some(name) = args.next() else { continue };
        if rest.contains("path:") || rest.contains("git:") || rest.contains("github:") {
            continue;
        }
        let range: Vec<&str> = args
            .take_while(|a| a.starts_with(|c: char| c.is_ascii_digit() || "~<>=!".contains(c)))
            .collect();
        let range = range.join(", ");
        let dev = groups.last().copied().unwrap_or(false)
            || rest.contains(":development")
            || rest.contains(":test");
        let locked = specs
            .get(name)
            .and_then(|v| clean(v.split('-').next().unwrap_or(v)));
        deps.push(Dep {
            name: name.into(),
            ecosystem: Ecosystem::RubyGems,
            manager: "bundler".into(),
            locked: locked.is_some(),
            version: locked.or_else(|| clean(&range)),
            range,
            dev,
            manifest: rel.into(),
        });
    }
    deps
}

/// The names a repository publishes, which another repository may depend on.
pub fn published(repo: &Path) -> Vec<(Ecosystem, String)> {
    let mut locks = Locks::default();
    let mut names = Vec::new();
    for rel in manifests(repo) {
        let file = rel.rsplit('/').next().unwrap_or(&rel);
        let name = match file {
            "package.json" => locks
                .json(repo, &rel)
                .and_then(|v| Some((Ecosystem::Npm, v["name"].as_str()?.to_owned()))),
            "composer.json" => locks.json(repo, &rel).and_then(|v| {
                Some((
                    Ecosystem::Packagist,
                    v["name"].as_str()?.to_ascii_lowercase(),
                ))
            }),
            "Cargo.toml" => locks
                .json(repo, &rel)
                .and_then(|v| Some((Ecosystem::Crates, v["package"]["name"].as_str()?.to_owned()))),
            "pyproject.toml" => locks.json(repo, &rel).and_then(|v| {
                let name = v["project"]["name"]
                    .as_str()
                    .or(v["tool"]["poetry"]["name"].as_str())?;
                Some((Ecosystem::PyPI, normalized(name)))
            }),
            "go.mod" => locks.text(repo, &rel).and_then(|t| {
                let module = t.lines().find_map(|l| l.trim().strip_prefix("module "))?;
                Some((Ecosystem::Go, module.trim().trim_matches('"').to_owned()))
            }),
            _ => None,
        };
        if let Some(name) = name.filter(|n| !n.1.is_empty() && !names.contains(n)) {
            names.push(name);
        }
    }
    names
}
