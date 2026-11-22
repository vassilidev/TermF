//! What a repository is built with, read from its files alone: tags such as `react` or `django`, with the version
//! its lockfiles, installed packages or manifests give. A shipped skill whose `metadata.termf-stacks` names one
//! of them starts on.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use serde_json::Value;

/// Every tag detection can give: a skill's `termf-stacks` names some of them.
pub const TAGS: &[&str] = &[
    "android",
    "angular",
    "aspnetcore",
    "aws",
    "azure",
    "compose",
    "django",
    "docker",
    "dotnet",
    "echo",
    "expo",
    "fastapi",
    "flask",
    "flutter",
    "gcp",
    "gin",
    "github",
    "github-actions",
    "go",
    "helm",
    "ios",
    "java",
    "jest",
    "junit",
    "kotlin",
    "kubernetes",
    "laravel",
    "make",
    "minitest",
    "mongodb",
    "mysql",
    "nextjs",
    "node",
    "nuxt",
    "opentofu",
    "pest",
    "php",
    "phpunit",
    "playwright",
    "postgres",
    "prisma",
    "pytest",
    "python",
    "rails",
    "react",
    "react-native",
    "redis",
    "rspec",
    "ruby",
    "rust",
    "spring-boot",
    "sql",
    "sqlalchemy",
    "svelte",
    "sveltekit",
    "swift",
    "symfony",
    "tailwind",
    "terraform",
    "typescript",
    "valkey",
    "vitest",
    "vue",
    "web",
];

/// Folders never read: installed dependencies, build output, virtualenvs (hidden ones are skipped too) and test
/// data, which is not the project's stack.
const SKIPPED: [&str; 12] = [
    "node_modules",
    "vendor",
    "target",
    "venv",
    "dist",
    "build",
    "out",
    "coverage",
    "__pycache__",
    "fixtures",
    "testdata",
    "Pods",
];
const MAX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 6;
/// A monorepo's packages are read down to this depth, up to `MAX_MANIFESTS` of them.
const MANIFEST_DEPTH: usize = 3;
const MAX_MANIFESTS: usize = 50;
/// YAML and Terraform files read for Kubernetes manifests, versions and cloud providers.
const MAX_READS: usize = 200;

/// npm packages and the tag each gives, with its version.
const NODE: [(&str, &str); 24] = [
    ("typescript", "typescript"),
    ("react", "react"),
    ("react-native", "react-native"),
    ("expo", "expo"),
    ("next", "nextjs"),
    ("svelte", "svelte"),
    ("@sveltejs/kit", "sveltekit"),
    ("vue", "vue"),
    ("nuxt", "nuxt"),
    ("@angular/core", "angular"),
    ("tailwindcss", "tailwind"),
    ("vitest", "vitest"),
    ("jest", "jest"),
    ("@playwright/test", "playwright"),
    ("playwright", "playwright"),
    ("prisma", "prisma"),
    ("@prisma/client", "prisma"),
    ("mongodb", "mongodb"),
    ("mongoose", "mongodb"),
    ("redis", "redis"),
    ("ioredis", "redis"),
    ("pg", "postgres"),
    ("postgres", "postgres"),
    ("mysql2", "mysql"),
];
/// Server frameworks that make a package a Node.js server.
const NODE_SERVERS: [&str; 4] = ["express", "fastify", "@nestjs/core", "hono"];
/// PyPI distributions (normalized) and their tag.
const PYTHON: [(&str, &str); 17] = [
    ("django", "django"),
    ("fastapi", "fastapi"),
    ("flask", "flask"),
    ("pytest", "pytest"),
    ("sqlalchemy", "sqlalchemy"),
    ("psycopg", "postgres"),
    ("psycopg2", "postgres"),
    ("psycopg2-binary", "postgres"),
    ("asyncpg", "postgres"),
    ("redis", "redis"),
    ("pymongo", "mongodb"),
    ("motor", "mongodb"),
    ("mysqlclient", "mysql"),
    ("pymysql", "mysql"),
    ("mysql-connector-python", "mysql"),
    ("boto3", "aws"),
    ("botocore", "aws"),
];
/// Go module prefixes and their tag.
const GO: [(&str, &str); 10] = [
    ("github.com/gin-gonic/gin", "gin"),
    ("github.com/labstack/echo", "echo"),
    ("github.com/jackc/pgx", "postgres"),
    ("github.com/lib/pq", "postgres"),
    ("github.com/redis/go-redis", "redis"),
    ("go.mongodb.org/mongo-driver", "mongodb"),
    ("github.com/go-sql-driver/mysql", "mysql"),
    ("github.com/aws/aws-sdk-go", "aws"),
    ("cloud.google.com/go", "gcp"),
    ("github.com/Azure/azure-sdk-for-go", "azure"),
];
/// Images of a Compose service (the last part of their name) and the database they run.
const IMAGES: [(&str, &str); 7] = [
    ("postgres", "postgres"),
    ("postgresql", "postgres"),
    ("mysql", "mysql"),
    ("mariadb", "mysql"),
    ("mongo", "mongodb"),
    ("redis", "redis"),
    ("valkey", "valkey"),
];
const WEB: [&str; 7] = ["html", "htm", "tsx", "jsx", "vue", "svelte", "astro"];

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// The installed or locked version, else the lowest one the manifest accepts.
    pub version: Option<String>,
    /// The file it was read from, relative to the repository.
    pub source: String,
}

/// What detection read of a repository: its tags by name, and the manifests that list its dependencies.
#[derive(Clone, Debug, Default)]
struct Survey {
    tags: Vec<Tag>,
    /// `package.json`, `pyproject.toml`, `requirements*.txt` and `composer.json`, relative to the repository.
    manifests: Vec<String>,
}

/// The repository's tags, by name.
pub fn stack_tags(repo: &Path) -> Vec<Tag> {
    survey(repo, head(repo).as_deref()).tags
}

/// How long the survey of a folder outside git is kept.
const NO_HEAD_FOR: Duration = Duration::from_secs(10);

/// Read again when the repository's `HEAD` or its shared config (its remotes) changes; a folder outside git, after a
/// few seconds.
// shortcut: a manifest changed but not committed is seen after the next commit or restart; key on the files' times
// too if that bites.
fn survey(repo: &Path, head: Option<&str>) -> Survey {
    type Entry = (Option<String>, Instant, Survey);
    static CACHE: Mutex<BTreeMap<PathBuf, Entry>> = Mutex::new(BTreeMap::new());
    if let Some((at, when, survey)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(repo)
        && at.as_deref() == head
        && (head.is_some() || when.elapsed() < NO_HEAD_FOR)
    {
        return survey.clone();
    }
    let survey = detect(repo);
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).insert(
        repo.to_path_buf(),
        (head.map(str::to_owned), Instant::now(), survey.clone()),
    );
    survey
}

/// The repository's `HEAD`, followed by the config shared by its worktrees, where `git remote add` writes.
fn head(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD", "--git-common-dir"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let mut lines = text.lines().map(str::trim);
    let head = lines.next().unwrap_or_default();
    if !out.status.success() || head.is_empty() {
        return None;
    }
    let config = lines
        .next()
        .and_then(|common| fs::read_to_string(repo.join(common).join("config")).ok());
    Some(format!("{head}\n{}", config.unwrap_or_default()))
}

#[derive(Default)]
struct Found {
    tags: BTreeMap<String, Tag>,
    /// Lockfiles read once per detection, by relative path.
    locks: HashMap<String, Option<Value>>,
    texts: HashMap<String, Option<String>>,
}

impl Found {
    /// The first place a tag is seen gives it, unless only a later one has its version.
    fn add(&mut self, name: &str, version: Option<String>, source: &str) {
        match self.tags.get_mut(name) {
            Some(tag) if tag.version.is_none() && version.is_some() => {
                tag.version = version;
                tag.source = source.into();
            }
            Some(_) => {}
            None => {
                self.tags.insert(
                    name.into(),
                    Tag {
                        name: name.into(),
                        version,
                        source: source.into(),
                    },
                );
            }
        }
    }

    fn text(&mut self, repo: &Path, rel: &str) -> Option<String> {
        self.texts
            .entry(rel.into())
            .or_insert_with(|| fs::read_to_string(repo.join(rel)).ok())
            .clone()
    }

    /// A JSON, TOML or YAML lockfile, as JSON.
    fn lock(&mut self, repo: &Path, rel: &str) -> Option<Value> {
        if let Some(value) = self.locks.get(rel) {
            return value.clone();
        }
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let value = fs::read_to_string(repo.join(rel))
            .ok()
            .and_then(|text| match name {
                _ if name.ends_with(".json") || name == "composer.lock" => {
                    serde_json::from_str(&text).ok()
                }
                _ if name.ends_with(".toml") || name.ends_with(".lock") => {
                    toml::from_str::<toml::Table>(&text)
                        .ok()
                        .and_then(|t| serde_json::to_value(t).ok())
                }
                _ => yaml_rust2::YamlLoader::load_from_str(&text)
                    .ok()?
                    .into_iter()
                    .next()
                    .map(crate::json),
            });
        self.locks.insert(rel.into(), value.clone());
        value
    }
}

/// `rel` under `dir`, both relative to the repository with `/`.
fn join(dir: &str, rel: &str) -> String {
    match dir {
        "" => rel.to_owned(),
        dir => format!("{dir}/{rel}"),
    }
}

/// Files and folders of the repository, relative with `/`, shallowest first.
fn walk(repo: &Path) -> Vec<(String, bool)> {
    let mut found = Vec::new();
    let mut queue = VecDeque::from([(repo.to_path_buf(), String::new(), 0)]);
    while let Some((dir, rel, depth)) = queue.pop_front() {
        let mut entries: Vec<_> = fs::read_dir(&dir).into_iter().flatten().flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if found.len() >= MAX_ENTRIES {
                return found;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = join(&rel, &name);
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let hidden = name.starts_with('.') && name != ".github";
                found.push((path.clone(), true));
                if !hidden && !SKIPPED.contains(&name.as_str()) && depth < MAX_DEPTH {
                    queue.push_back((entry.path(), path, depth + 1));
                }
            } else if kind.is_file() {
                found.push((path, false));
            }
        }
    }
    found
}

fn detect(repo: &Path) -> Survey {
    let mut found = Found::default();
    let mut lists = Vec::new();
    let (mut manifests, mut reads) = (0, 0);
    for (rel, dir) in walk(repo) {
        let (parent, name) = rel.rsplit_once('/').unwrap_or(("", &rel));
        let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
        if dir {
            if ext == "xcodeproj" || ext == "xcworkspace" {
                found.add("swift", None, &rel);
                let pbx = fs::read_to_string(repo.join(&rel).join("project.pbxproj"));
                if pbx.is_ok_and(|t| t.contains("iphoneos")) {
                    found.add("ios", None, &rel);
                }
            }
            continue;
        }
        let manifest = rel.matches('/').count() < MANIFEST_DEPTH && manifests < MAX_MANIFESTS;
        let mut counted = true;
        match name {
            "package.json" if manifest => node(repo, parent, &mut found),
            "pyproject.toml" if manifest => python(repo, parent, &rel, &mut found),
            _ if manifest && name.starts_with("requirements") && ext == "txt" => {
                python(repo, parent, &rel, &mut found)
            }
            "go.mod" if manifest => go(repo, &rel, &mut found),
            "Cargo.toml" if manifest => rust(repo, &rel, &mut found),
            "pom.xml" if manifest => maven(repo, &rel, &mut found),
            "build.gradle" | "build.gradle.kts" if manifest => gradle(repo, &rel, &mut found),
            "Package.swift" if manifest => swift(repo, &rel, &mut found),
            "pubspec.yaml" if manifest => flutter(repo, &rel, &mut found),
            "global.json" if manifest => dotnet_sdk(repo, &rel, &mut found),
            "composer.json" if manifest => php(repo, parent, &rel, &mut found),
            "Gemfile" if manifest => ruby(repo, parent, &rel, &mut found),
            _ if manifest && matches!(ext, "csproj" | "fsproj" | "vbproj") => {
                csproj(repo, &rel, &mut found)
            }
            _ => counted = false,
        }
        if counted {
            manifests += 1;
            if matches!(name, "package.json" | "pyproject.toml" | "composer.json") || ext == "txt" {
                lists.push(rel.clone());
            }
            continue;
        }
        let compose = (name.starts_with("compose.") || name.starts_with("docker-compose"))
            && matches!(ext, "yml" | "yaml");
        match name {
            "tsconfig.json" => found.add("typescript", None, &rel),
            "Chart.yaml" => {
                found.add("helm", None, &rel);
                found.add("kubernetes", None, &rel);
            }
            "kustomization.yaml" | "kustomization.yml" => found.add("kubernetes", None, &rel),
            "Makefile" | "makefile" | "GNUmakefile" => found.add("make", None, &rel),
            ".terraform.lock.hcl" => found.add("terraform", None, &rel),
            _ if ext == "sln" => found.add("dotnet", None, &rel),
            _ if name == "Dockerfile"
                || name == "Containerfile"
                || name.starts_with("Dockerfile.")
                || ext == "dockerfile" =>
            {
                found.add("docker", None, &rel)
            }
            _ if compose => compose_file(repo, &rel, &mut found),
            _ if rel.starts_with(".github/workflows/") && matches!(ext, "yml" | "yaml") => {
                found.add("github-actions", None, &rel)
            }
            _ if ext == "tf" || ext == "tofu" => {
                found.add(
                    if ext == "tf" { "terraform" } else { "opentofu" },
                    None,
                    &rel,
                );
                if reads < MAX_READS {
                    reads += 1;
                    terraform(repo, &rel, ext, &mut found);
                }
            }
            _ if matches!(ext, "yml" | "yaml")
                && !rel.starts_with(".github/")
                && !name.contains("lock")
                && reads < MAX_READS =>
            {
                reads += 1;
                let text = fs::read_to_string(repo.join(&rel)).unwrap_or_default();
                let top = |key: &str| text.lines().any(|l| l.starts_with(key));
                if top("apiVersion:") && top("kind:") {
                    found.add("kubernetes", None, &rel);
                }
            }
            _ if WEB.contains(&ext) => found.add("web", None, &rel),
            _ if ext == "sql" => found.add("sql", None, &rel),
            _ => {}
        }
    }
    for db in ["postgres", "mysql"] {
        if let Some(source) = found.tags.get(db).map(|t| t.source.clone()) {
            found.add("sql", None, &source);
        }
    }
    if github_remote(repo) {
        found.add("github", None, ".git/config");
    }
    Survey {
        tags: found.tags.into_values().collect(),
        manifests: lists,
    }
}

const GITHUB_HOSTS: [&str; 3] = ["github.com", "www.github.com", "ssh.github.com"];

/// Whether a remote of the repository (a worktree's included) is on github.com.
// shortcut: GitHub Enterprise hosts and SSH host aliases are not seen; share termf-tickets' `remote_slug` (which
// reads `~/.ssh/config`) through a common crate if they matter.
fn github_remote(repo: &Path) -> bool {
    repo.join(".git").exists()
        && Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["remote", "-v"])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .is_ok_and(|out| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|line| remote_host(line.split_whitespace().nth(1)?))
                    .any(|host| GITHUB_HOSTS.iter().any(|g| host.eq_ignore_ascii_case(g)))
            })
}

/// The host of a remote's URL: `https://user@host:443/…`, `ssh://git@host/…`, or `[user@]host:path` as git reads
/// it (a colon before any slash).
fn remote_host(url: &str) -> Option<&str> {
    let authority = match url.split_once("://") {
        Some((_, rest)) => rest.split('/').next()?,
        None => {
            url.split_once(':')
                .filter(|(a, _)| !a.contains(['/', '\\']))?
                .0
        }
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    host.split(':').next()
}

/// The version a manifest's range starts from: `^16.1.0` → `16.1.0`, `>=5.0,<6` → `5.0`. `None` for what is not
/// a version (`workspace:*`, a path, a URL, `latest`).
pub fn clean(spec: &str) -> Option<String> {
    let spec = spec.trim();
    if spec.contains(':') || spec.contains('/') {
        return None;
    }
    let start = spec.find(|c: char| c.is_ascii_digit())?;
    if !spec[..start]
        .chars()
        .all(|c| "^~=<>!v ".contains(c) || c == '*')
    {
        return None;
    }
    let version: String = spec[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        .collect();
    let mut version = version.as_str();
    while let Some(v) = version
        .strip_suffix(".x")
        .or_else(|| version.strip_suffix(['.', '-']))
    {
        version = v;
    }
    (!version.is_empty()).then(|| version.to_owned())
}

fn node(repo: &Path, dir: &str, found: &mut Found) {
    let rel = join(dir, "package.json");
    let Some(pkg) = found
        .text(repo, &rel)
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return;
    };
    let mut deps: BTreeMap<&str, &str> = BTreeMap::new();
    for key in ["devDependencies", "dependencies"] {
        for (name, spec) in pkg[key].as_object().into_iter().flatten() {
            deps.insert(name, spec.as_str().unwrap_or_default());
        }
    }
    for (dep, tag) in NODE {
        if let Some(spec) = deps.get(dep) {
            let (version, source) = node_version(repo, dir, dep, found)
                .map_or_else(|| (clean(spec), rel.clone()), |(v, s)| (Some(v), s));
            found.add(tag, version, &source);
        }
    }
    if let Some(engine) = pkg["engines"]["node"].as_str() {
        found.add("node", clean(engine), &rel);
    } else if NODE_SERVERS.iter().any(|s| deps.contains_key(s)) {
        found.add("node", None, &rel);
    }
    for (prefix, tag) in [
        ("@aws-sdk/", "aws"),
        ("aws-cdk-lib", "aws"),
        ("@google-cloud/", "gcp"),
        ("@azure/", "azure"),
    ] {
        if deps.keys().any(|d| d.starts_with(prefix)) {
            found.add(tag, None, &rel);
        }
    }
}

/// What is installed, else what a lockfile holds, for the npm package `dep` of the package in `dir`.
fn node_version(repo: &Path, dir: &str, dep: &str, found: &mut Found) -> Option<(String, String)> {
    let bases: &[&str] = if dir.is_empty() { &[""] } else { &[dir, ""] };
    for base in bases {
        let rel = join(base, &format!("node_modules/{dep}/package.json"));
        if let Some(version) = found
            .text(repo, &rel)
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["version"].as_str().map(str::to_owned))
        {
            return Some((version, rel));
        }
    }
    for base in bases {
        let rel = join(base, "package-lock.json");
        let Some(lock) = found.lock(repo, &rel) else {
            continue;
        };
        // A workspace's lockfile is at its root, with a package's own copy under its folder.
        let nested = join(
            dir[base.len()..].trim_start_matches('/'),
            &format!("node_modules/{dep}"),
        );
        for key in [nested, format!("node_modules/{dep}")] {
            if let Some(v) = lock["packages"][&key]["version"].as_str() {
                return Some((v.into(), rel));
            }
        }
    }
    if let Some(lock) = found.lock(repo, "pnpm-lock.yaml") {
        let importer = &lock["importers"][if dir.is_empty() { "." } else { dir }];
        for key in ["dependencies", "devDependencies"] {
            let entry = &importer[key][dep];
            let version = entry["version"].as_str().or(entry.as_str());
            if let Some(v) = version.and_then(|v| clean(v.split('(').next().unwrap_or(v))) {
                return Some((v, "pnpm-lock.yaml".into()));
            }
        }
    }
    let yarn = found.text(repo, "yarn.lock")?;
    let mut header = false;
    for line in yarn.lines() {
        if !line.starts_with(' ') {
            header = line
                .split(", ")
                .any(|h| h.trim_matches('"').starts_with(&format!("{dep}@")));
        } else if header && let Some(v) = line.trim().strip_prefix("version") {
            return clean(v.trim_matches([':', ' ', '"'])).map(|v| (v, "yarn.lock".into()));
        }
    }
    None
}

/// A PEP 508 requirement: its normalized name and what follows it.
fn requirement(line: &str) -> Option<(String, String)> {
    let line = line.split(';').next()?.trim();
    let end = line
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(line.len());
    let name = line[..end].to_ascii_lowercase().replace(['_', '.'], "-");
    let rest = line[end..].trim_start();
    let rest = match rest.strip_prefix('[') {
        Some(extras) => extras.split_once(']').map_or("", |(_, r)| r),
        None => rest,
    };
    (!name.is_empty()).then(|| (name, rest.trim().to_owned()))
}

/// The requirements of a `pyproject.toml` or `requirements*.txt`, by normalized name, and the Python versions it
/// accepts.
fn python_deps(text: &str, rel: &str) -> (Vec<(String, String)>, Option<String>) {
    let mut deps: Vec<(String, String)> = Vec::new();
    let mut python = None;
    if rel.ends_with("pyproject.toml") {
        let Ok(doc) = toml::from_str::<toml::Table>(text) else {
            return (deps, None);
        };
        let doc = serde_json::to_value(doc).unwrap_or_default();
        let project = &doc["project"];
        python = project["requires-python"].as_str().map(str::to_owned);
        let mut lists = vec![&project["dependencies"]];
        lists.extend(
            project["optional-dependencies"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.values()),
        );
        lists.extend(
            doc["dependency-groups"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.values()),
        );
        for list in lists {
            deps.extend(
                list.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| requirement(d.as_str()?)),
            );
        }
        let poetry = &doc["tool"]["poetry"];
        let mut tables = vec![&poetry["dependencies"], &poetry["dev-dependencies"]];
        tables.extend(
            poetry["group"]
                .as_object()
                .into_iter()
                .flat_map(|g| g.values().map(|g| &g["dependencies"])),
        );
        for table in tables {
            for (name, spec) in table.as_object().into_iter().flatten() {
                let spec = spec
                    .as_str()
                    .or(spec["version"].as_str())
                    .unwrap_or_default();
                if name == "python" {
                    python = python.or(Some(spec.to_owned()));
                } else {
                    deps.push((
                        name.to_ascii_lowercase().replace(['_', '.'], "-"),
                        spec.into(),
                    ));
                }
            }
        }
    } else {
        deps.extend(
            text.lines()
                .map(|l| l.split(" #").next().unwrap_or_default().trim())
                .filter(|l| !l.is_empty() && !l.starts_with(['#', '-']))
                .filter_map(requirement),
        );
    }
    (deps, python)
}

fn python(repo: &Path, dir: &str, rel: &str, found: &mut Found) {
    let text = found.text(repo, rel).unwrap_or_default();
    let (deps, python) = python_deps(&text, rel);
    let pinned = [join(dir, ".python-version"), ".python-version".into()]
        .iter()
        .find_map(|f| {
            found
                .text(repo, f)
                .map(|t| (t.trim().to_owned(), f.clone()))
        });
    match (python.as_deref().and_then(clean), pinned) {
        (_, Some((v, file))) if clean(&v).is_some() => found.add("python", clean(&v), &file),
        (version, _) => found.add("python", version, rel),
    }
    for (name, spec) in deps {
        let Some(tag) = PYTHON
            .iter()
            .find(|(d, _)| *d == name)
            .map(|(_, t)| *t)
            .or_else(|| name.starts_with("google-cloud-").then_some("gcp"))
            .or_else(|| name.starts_with("azure-").then_some("azure"))
        else {
            continue;
        };
        let locked = [
            join(dir, "uv.lock"),
            join(dir, "poetry.lock"),
            "uv.lock".into(),
            "poetry.lock".into(),
        ]
        .into_iter()
        .find_map(|lock| {
            let value = found.lock(repo, &lock)?;
            let v = value["package"].as_array()?.iter().find_map(|p| {
                let n = p["name"]
                    .as_str()?
                    .to_ascii_lowercase()
                    .replace(['_', '.'], "-");
                (n == name).then(|| p["version"].as_str().map(str::to_owned))?
            })?;
            Some((v, lock))
        });
        match locked {
            Some((v, lock)) => found.add(tag, Some(v), &lock),
            None => found.add(tag, clean(&spec), rel),
        }
    }
}

fn go(repo: &Path, rel: &str, found: &mut Found) {
    let text = found.text(repo, rel).unwrap_or_default();
    let mut version = None;
    for line in text
        .lines()
        .map(|l| l.split("//").next().unwrap_or_default().trim())
    {
        if let Some(v) = line.strip_prefix("go ") {
            version = clean(v);
            continue;
        }
        let line = line.strip_prefix("require ").unwrap_or(line).trim();
        let mut parts = line.split_whitespace();
        let (Some(module), Some(v)) = (parts.next(), parts.next()) else {
            continue;
        };
        if let Some((_, tag)) = GO.iter().find(|(prefix, _)| module.starts_with(prefix)) {
            found.add(tag, clean(v), rel);
        }
    }
    found.add("go", version, rel);
}

fn rust(repo: &Path, rel: &str, found: &mut Found) {
    let doc = found
        .text(repo, rel)
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .and_then(|t| serde_json::to_value(t).ok())
        .unwrap_or_default();
    let version = doc["package"]["rust-version"]
        .as_str()
        .or(doc["workspace"]["package"]["rust-version"].as_str())
        .map(str::to_owned);
    found.add("rust", version, rel);
    for (dep, tag) in [
        ("tokio-postgres", "postgres"),
        ("postgres", "postgres"),
        ("redis", "redis"),
        ("mongodb", "mongodb"),
    ] {
        if doc["dependencies"].get(dep).is_some() {
            found.add(tag, None, rel);
        }
    }
}

/// The text between `open` and `close` after `marker`.
fn after<'a>(text: &'a str, marker: &str, open: &str, close: &str) -> Option<&'a str> {
    let rest = &text[text.find(marker)? + marker.len()..];
    let rest = &rest[rest.find(open)? + open.len()..];
    Some(rest[..rest.find(close)?].trim())
}

fn maven(repo: &Path, rel: &str, found: &mut Found) {
    let text = found.text(repo, rel).unwrap_or_default();
    let java = [
        "<java.version>",
        "<maven.compiler.release>",
        "<maven.compiler.target>",
    ]
    .iter()
    .find_map(|tag| after(&text, tag, "", "<"))
    .and_then(clean);
    found.add("java", java, rel);
    if text.contains("spring-boot") {
        let version = after(
            &text,
            "spring-boot-starter-parent</artifactId>",
            "<version>",
            "<",
        )
        .and_then(clean);
        found.add("spring-boot", version, rel);
    }
    if text.contains("kotlin-maven-plugin") || text.contains("kotlin-stdlib") {
        found.add("kotlin", None, rel);
    }
    // The version of a dependency's own `<version>`, not a later one's.
    let declared = |marker: &str| {
        let rest = &text[text.find(marker)?..];
        let block = &rest[..rest.find("</dependency>").unwrap_or(rest.len())];
        after(block, "<version>", "", "<").and_then(clean)
    };
    // JUnit 5 and later only: the `junit` skill is not written for JUnit 4.
    for marker in [
        "<groupId>org.junit.jupiter</groupId>",
        "<groupId>org.junit.platform</groupId>",
        "<artifactId>spring-boot-starter-test</artifactId>",
    ] {
        if text.contains(marker) {
            let version = declared(marker).filter(|_| !marker.contains("spring-boot"));
            found.add("junit", version, rel);
        }
    }
}

/// The version of the first `group:artifact:version` coordinate that starts with one of `prefixes`.
fn coordinate(text: &str, prefixes: &[&str]) -> Option<Option<String>> {
    prefixes.iter().find_map(|prefix| {
        let at = text.find(prefix)?;
        let coordinate: String = text[at..]
            .chars()
            .take_while(|c| !matches!(c, '"' | '\'' | ')' | '\n'))
            .collect();
        Some(coordinate.split(':').nth(2).and_then(clean))
    })
}

/// The version a Gradle plugin is applied with: `id("x") version "1.2"` or `id 'x' version '1.2'`.
fn plugin_version(text: &str, id: &str) -> Option<String> {
    let line = text
        .lines()
        .find(|l| l.contains(id) && l.contains("version"))?;
    let rest = &line[line.find("version")? + "version".len()..];
    clean(rest.trim().trim_matches(['"', '\'', ' ', '(', ')']))
}

fn gradle(repo: &Path, rel: &str, found: &mut Found) {
    let text = found.text(repo, rel).unwrap_or_default();
    let android = [
        "com.android.application",
        "com.android.library",
        "android.application",
        "android.library",
    ]
    .iter()
    .any(|p| text.contains(p));
    let spring = text.contains("org.springframework.boot");
    if android {
        found.add("android", plugin_version(&text, "com.android."), rel);
    }
    if !android || spring {
        found.add("java", None, rel);
    }
    if spring {
        found.add(
            "spring-boot",
            plugin_version(&text, "org.springframework.boot"),
            rel,
        );
    }
    if text.contains("org.jetbrains.kotlin")
        || text.contains("kotlin(\"")
        || text.contains("plugins.kotlin")
    {
        found.add("kotlin", plugin_version(&text, "kotlin"), rel);
    }
    // JUnit 5 and later only: `junit:junit` is JUnit 4.
    let junit = coordinate(
        &text,
        &[
            "org.junit.jupiter:",
            "org.junit.platform:",
            "org.junit:junit-bom:",
        ],
    );
    if let Some(version) = junit {
        found.add("junit", version, rel);
    } else if text.contains("spring-boot-starter-test") {
        found.add("junit", None, rel);
    }
}

fn swift(repo: &Path, rel: &str, found: &mut Found) {
    let text = found.text(repo, rel).unwrap_or_default();
    let version = text
        .lines()
        .next()
        .and_then(|l| l.split_once("swift-tools-version"))
        .and_then(|(_, v)| clean(v.trim_start_matches([':', ' '])));
    found.add("swift", version, rel);
}

/// A Dart package is a Flutter app when it depends on the Flutter SDK; its version: `environment.flutter`.
fn flutter(repo: &Path, rel: &str, found: &mut Found) {
    let Some(pubspec) = found
        .text(repo, rel)
        .and_then(|t| yaml_rust2::YamlLoader::load_from_str(&t).ok())
        .and_then(|docs| docs.into_iter().next())
        .map(crate::json)
    else {
        return;
    };
    if pubspec["dependencies"]["flutter"]["sdk"] == "flutter" {
        let version = pubspec["environment"]["flutter"].as_str().and_then(clean);
        found.add("flutter", version, rel);
    }
}

fn dotnet_sdk(repo: &Path, rel: &str, found: &mut Found) {
    let version = found
        .text(repo, rel)
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["sdk"]["version"].as_str().and_then(clean));
    if version.is_some() {
        found.add("dotnet", version, rel);
    }
}

fn csproj(repo: &Path, rel: &str, found: &mut Found) {
    let text = found.text(repo, rel).unwrap_or_default();
    let framework = ["<TargetFramework>", "<TargetFrameworks>"]
        .iter()
        .find_map(|tag| after(&text, tag, "", "<"))
        .and_then(|f| f.split(';').next())
        .and_then(|f| f.strip_prefix("net"))
        .and_then(clean);
    found.add("dotnet", framework, rel);
    if text.contains("Microsoft.NET.Sdk.Web") {
        found.add("aspnetcore", None, rel);
    }
}

fn php(repo: &Path, dir: &str, rel: &str, found: &mut Found) {
    let Some(doc) = found
        .text(repo, rel)
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return;
    };
    let mut deps: BTreeMap<String, String> = BTreeMap::new();
    for key in ["require-dev", "require"] {
        for (name, spec) in doc[key].as_object().into_iter().flatten() {
            deps.insert(name.to_owned(), spec.as_str().unwrap_or_default().into());
        }
    }
    found.add("php", deps.get("php").and_then(|s| clean(s)), rel);
    let lock = join(dir, "composer.lock");
    let locked = found.lock(repo, &lock).unwrap_or_default();
    let version = |name: &str| -> (Option<String>, String) {
        let from_lock = ["packages", "packages-dev"].iter().find_map(|key| {
            locked[*key].as_array()?.iter().find_map(|p| {
                (p["name"].as_str()? == name).then(|| p["version"].as_str().and_then(clean))?
            })
        });
        match from_lock {
            Some(v) => (Some(v), lock.clone()),
            None => (deps.get(name).and_then(|s| clean(s)), rel.to_owned()),
        }
    };
    let laravel = deps
        .contains_key("laravel/framework")
        .then(|| version("laravel/framework"));
    let symfony = ["symfony/framework-bundle", "symfony/symfony"]
        .iter()
        .find(|d| deps.contains_key(**d))
        .map(|d| version(d));
    let mut tags = vec![("laravel", laravel), ("symfony", symfony)];
    for (dep, tag) in [
        ("pestphp/pest", "pest"),
        ("phpunit/phpunit", "phpunit"),
        ("predis/predis", "redis"),
    ] {
        tags.push((tag, deps.contains_key(dep).then(|| version(dep))));
    }
    for (tag, found_as) in tags {
        if let Some((v, source)) = found_as {
            // predis' version is not Redis'.
            found.add(tag, v.filter(|_| tag != "redis"), &source);
        }
    }
}

fn ruby(repo: &Path, dir: &str, rel: &str, found: &mut Found) {
    let text = found.text(repo, rel).unwrap_or_default();
    let gems: Vec<String> = text
        .lines()
        .filter_map(|l| l.trim().strip_prefix("gem "))
        .filter_map(|l| l.split(',').next())
        .map(|g| g.trim().trim_matches(['"', '\'']).to_owned())
        .collect();
    let lock_rel = join(dir, "Gemfile.lock");
    let lock = found.text(repo, &lock_rel).unwrap_or_default();
    let locked = |gem: &str| {
        lock.lines()
            .find_map(|l| l.strip_prefix(&format!("    {gem} (")))
            .and_then(|v| clean(v.trim_end_matches(')')))
    };
    let ruby = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("ruby "))
        .and_then(|v| clean(v.trim_matches(['"', '\''])));
    let pinned = found
        .text(repo, &join(dir, ".ruby-version"))
        .and_then(|v| clean(v.trim()));
    found.add("ruby", ruby.or(pinned), rel);
    for (gem, tag) in [
        ("rails", "rails"),
        ("pg", "postgres"),
        ("mysql2", "mysql"),
        ("redis", "redis"),
        ("rspec-rails", "rspec"),
        ("rspec", "rspec"),
        ("minitest", "minitest"),
    ] {
        if gems.iter().any(|g| g == gem) {
            match locked(gem) {
                Some(v) => found.add(tag, Some(v), &lock_rel),
                None => found.add(tag, None, rel),
            }
        }
    }
}

fn compose_file(repo: &Path, rel: &str, found: &mut Found) {
    found.add("docker", None, rel);
    found.add("compose", None, rel);
    let Some(doc) = found.lock(repo, rel) else {
        return;
    };
    for service in doc["services"]
        .as_object()
        .into_iter()
        .flat_map(|s| s.values())
    {
        let Some(image) = service["image"].as_str() else {
            continue;
        };
        // `registry.local:5000/postgres:16`: the registry's port is not the tag.
        let last = image.rsplit('/').next().unwrap_or(image);
        let (name, tag) = last.split_once(':').unwrap_or((last, ""));
        if let Some((_, db)) = IMAGES.iter().find(|(i, _)| *i == name) {
            // `17-alpine`: the variant is not part of the version.
            found.add(db, clean(tag.split('-').next().unwrap_or(tag)), rel);
        }
    }
}

fn terraform(repo: &Path, rel: &str, ext: &str, found: &mut Found) {
    let text = fs::read_to_string(repo.join(rel)).unwrap_or_default();
    let tool = if ext == "tf" { "terraform" } else { "opentofu" };
    if let Some(line) = text
        .lines()
        .find(|l| l.trim_start().starts_with("required_version"))
    {
        let value = line
            .split_once('=')
            .map_or("", |(_, v)| v)
            .trim()
            .trim_matches('"');
        found.add(tool, clean(value), rel);
    }
    for (provider, tag) in [
        ("hashicorp/aws", "aws"),
        ("hashicorp/google", "gcp"),
        ("hashicorp/azurerm", "azure"),
    ] {
        if text.contains(provider) {
            found.add(tag, None, rel);
        }
    }
}

/// Whether `version` is in `range`, as a skill's `termf-versions` writes it: comparisons that must all hold
/// (`>=15 <17`), a bare version for its whole series (`16`, `16.x`), `||` between alternatives. `None` when one
/// of them cannot be read.
pub fn covers(range: &str, version: &str) -> Option<bool> {
    let parse = |v: &str| -> Option<Vec<u64>> {
        let parts: Vec<u64> = v
            .split('.')
            .map_while(|p| {
                let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
            .collect();
        (!parts.is_empty()).then_some(parts)
    };
    let have = parse(&clean(version)?)?;
    let compare = |want: &[u64]| {
        let n = want.len().max(have.len());
        let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
        (0..n)
            .map(|i| at(&have, i).cmp(&at(want, i)))
            .find(|o| o.is_ne())
            .unwrap_or(std::cmp::Ordering::Equal)
    };
    let mut any = false;
    for alternative in range.split("||") {
        let mut all = true;
        for token in alternative.split_whitespace() {
            let op_end = token.find(|c: char| c.is_ascii_digit())?;
            let want = parse(token[op_end..].trim_end_matches(".x"))?;
            let order = compare(&want);
            all &= match &token[..op_end] {
                ">=" => order.is_ge(),
                ">" => order.is_gt(),
                "<=" => order.is_le(),
                "<" => order.is_lt(),
                "=" | "" => have.starts_with(&want),
                _ => return None,
            };
        }
        any |= all;
    }
    Some(any)
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: Option<String>,
}

/// A skill an installed package ships, served as it is from where the package manager put it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PackageSkill {
    /// Its folder's name, which names it in the library.
    pub name: String,
    pub path: PathBuf,
    pub package: Package,
}

/// Docs or guidelines an installed package ships for agents, to read when a task needs them.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Guide {
    pub package: Package,
    /// Relative to the repository.
    pub path: String,
}

/// What a repository is built with, and what its installed packages ship for agents.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct RepoStack {
    pub name: String,
    pub path: PathBuf,
    pub tags: Vec<Tag>,
    pub skills: Vec<PackageSkill>,
    pub guides: Vec<Guide>,
    /// Claude Code trusts the repository: the skills and guides of its packages are loaded like its own
    /// `.claude/skills`. Otherwise they are offered, off until switched on.
    pub trusted: bool,
    /// The language servers of its stack, installed or not; `read` leaves them to the caller.
    pub servers: Vec<LanguageServer>,
}

impl RepoStack {
    /// The repository at `path` as it is now, `trusted` false.
    /// Its packages are looked at again only when a manifest, a lockfile or a package folder changed.
    pub fn read(name: &str, path: &Path) -> RepoStack {
        type Entry = (Option<String>, Vec<Option<SystemTime>>, RepoStack);
        static CACHE: Mutex<BTreeMap<PathBuf, Entry>> = Mutex::new(BTreeMap::new());
        let head = head(path);
        let survey = survey(path, head.as_deref());
        let times = installed_times(path, &survey.manifests);
        if let Some((at, when, stack)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(path)
            && *at == head
            && *when == times
        {
            return RepoStack {
                name: name.into(),
                ..stack.clone()
            };
        }
        let (skills, guides) = installed(path, &survey.manifests);
        let stack = RepoStack {
            name: name.into(),
            path: path.into(),
            tags: survey.tags,
            skills,
            guides,
            trusted: false,
            servers: Vec::new(),
        };
        CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.into(), (head, times, stack.clone()));
        stack
    }
}

/// What changes when packages are installed: the manifests, the lockfiles and the package folders of the
/// repository and of each folder with a manifest.
fn installed_times(repo: &Path, manifests: &[String]) -> Vec<Option<SystemTime>> {
    const FILES: [&str; 12] = [
        "package.json",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "bun.lock",
        "node_modules",
        "composer.json",
        "composer.lock",
        "vendor",
        "pyproject.toml",
        "uv.lock",
        "poetry.lock",
    ];
    let mut dirs = vec![repo.to_path_buf()];
    for manifest in manifests {
        let dir = repo
            .join(manifest)
            .parent()
            .map_or(repo.into(), Path::to_path_buf);
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    let time = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut times = Vec::new();
    for dir in dirs {
        times.extend(FILES.iter().map(|f| time(&dir.join(f))));
        times.extend(site_packages(&dir).iter().map(|s| time(s)));
    }
    times
}

/// Skill folders under `dir/sub`: `sub/<name>/SKILL.md`, and one level deeper for a package that groups them.
fn skill_dirs(dir: &Path, sub: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level: Vec<PathBuf> = vec![dir.join(sub)];
    for _ in 0..2 {
        let mut next = Vec::new();
        for parent in level {
            let mut entries: Vec<PathBuf> = fs::read_dir(&parent)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            entries.sort();
            for entry in entries {
                if entry.join("SKILL.md").is_file() {
                    found.push(entry);
                } else {
                    next.push(entry);
                }
            }
        }
        level = next;
    }
    found
}

/// Where npm, Composer or a virtualenv put the direct dependency `name` of the manifest in `dir`, and its version.
fn package_root(
    repo: &Path,
    dir: &str,
    manager: &str,
    name: &str,
) -> Option<(PathBuf, Option<String>)> {
    let bases: &[&str] = if dir.is_empty() { &[""] } else { &[dir, ""] };
    for base in bases {
        let root = repo.join(base);
        match manager {
            "npm" => {
                let path = root.join("node_modules").join(name);
                if path.is_dir() {
                    let version = fs::read_to_string(path.join("package.json"))
                        .ok()
                        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                        .and_then(|v| v["version"].as_str().map(str::to_owned));
                    return Some((path, version));
                }
            }
            "composer" => {
                let path = root.join("vendor").join(name);
                if path.is_dir() {
                    let lock = fs::read_to_string(root.join("composer.lock"))
                        .ok()
                        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                        .unwrap_or_default();
                    let version = ["packages", "packages-dev"].iter().find_map(|key| {
                        lock[*key].as_array()?.iter().find_map(|p| {
                            (p["name"].as_str()? == name)
                                .then(|| p["version"].as_str().and_then(clean))?
                        })
                    });
                    return Some((path, version));
                }
            }
            _ => {
                let module = name.replace('-', "_");
                for site in site_packages(&root) {
                    let path = site.join(&module);
                    if path.is_dir() {
                        let prefix = format!("{module}-");
                        let version = fs::read_dir(&site)
                            .into_iter()
                            .flatten()
                            .flatten()
                            .find_map(|e| {
                                let file = e.file_name().to_string_lossy().to_ascii_lowercase();
                                let rest =
                                    file.strip_prefix(&prefix)?.strip_suffix(".dist-info")?;
                                Some(rest.to_owned())
                            });
                        return Some((path, version));
                    }
                }
            }
        }
    }
    None
}

/// The `site-packages` of the virtualenvs in `root`.
fn site_packages(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for venv in [".venv", "venv"] {
        let lib = root.join(venv).join("lib");
        for entry in fs::read_dir(&lib).into_iter().flatten().flatten() {
            let site = entry.path().join("site-packages");
            if site.is_dir() {
                found.push(site);
            }
        }
        // Windows puts it in `Lib`, without the Python version.
        let site = root.join(venv).join("Lib").join("site-packages");
        if site.is_dir() {
            found.push(site);
        }
    }
    found
}

/// The skills and guides the installed direct dependencies of `manifests` ship.
fn installed(repo: &Path, manifests: &[String]) -> (Vec<PackageSkill>, Vec<Guide>) {
    let mut skills: Vec<PackageSkill> = Vec::new();
    let mut guides: Vec<Guide> = Vec::new();
    let rel = |path: &Path| {
        path.strip_prefix(repo)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    };
    for manifest in manifests {
        let (dir, file) = manifest.rsplit_once('/').unwrap_or(("", manifest));
        let text = fs::read_to_string(repo.join(manifest)).unwrap_or_default();
        let (manager, deps): (&str, Vec<String>) = match file {
            "package.json" | "composer.json" => {
                let doc: Value = serde_json::from_str(&text).unwrap_or_default();
                let keys: &[&str] = if file == "package.json" {
                    &[
                        "dependencies",
                        "devDependencies",
                        "optionalDependencies",
                        "peerDependencies",
                    ]
                } else {
                    &["require", "require-dev"]
                };
                let deps = keys
                    .iter()
                    .flat_map(|k| {
                        doc[*k]
                            .as_object()
                            .into_iter()
                            .flatten()
                            .map(|(n, _)| n.clone())
                    })
                    .collect();
                (
                    if file == "package.json" {
                        "npm"
                    } else {
                        "composer"
                    },
                    deps,
                )
            }
            _ => (
                "python",
                python_deps(&text, manifest)
                    .0
                    .into_iter()
                    .map(|(n, _)| n)
                    .collect(),
            ),
        };
        for dep in deps {
            // Playwright's skills come with playwright-core, which its packages depend on.
            let (name, subs): (&str, &[&str]) = match (manager, dep.as_str()) {
                ("npm", "playwright" | "@playwright/test") => {
                    ("playwright-core", &["lib/tools/skills"])
                }
                ("npm", _) => (&dep, &["skills", "dist/skills"]),
                ("composer", _) => (&dep, &["resources/boost/skills"]),
                _ => (&dep, &[".agents/skills"]),
            };
            let Some((root, version)) = package_root(repo, dir, manager, name) else {
                continue;
            };
            let package = Package {
                name: name.into(),
                version,
            };
            for sub in subs {
                for path in skill_dirs(&root, sub) {
                    let skill = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    if !skills.iter().any(|s| s.name == skill) {
                        skills.push(PackageSkill {
                            name: skill,
                            path,
                            package: package.clone(),
                        });
                    }
                }
            }
            let guide = match (manager, name) {
                ("npm", "next") => Some(root.join("dist/docs")),
                ("npm", "@angular/core") => fs::read_to_string(root.join("package.json"))
                    .ok()
                    .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                    .and_then(|v| {
                        let path = v["angular"]["bestPractices"]["path"].as_str()?;
                        (!path.contains("..")).then(|| root.join(path.trim_start_matches("./")))
                    }),
                ("composer", _) => Some(root.join("resources/boost/guidelines")),
                _ => None,
            };
            if let Some(path) = guide.filter(|p| p.exists()) {
                let path = rel(&path);
                if !guides.iter().any(|g| g.path == path) {
                    guides.push(Guide { package, path });
                }
            }
        }
    }
    (skills, guides)
}

/// A framework's MCP server, as `claude mcp add-json` takes it: offered, never added without the owner.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpOffer {
    pub name: &'static str,
    /// The tag (or the package) it is offered for.
    pub r#for: String,
    pub server: Value,
    pub docs: &'static str,
    /// It runs the project's code (boots the app, starts a dev server or a build).
    pub runs_code: bool,
}

/// Official skills a vendor publishes, to install from their repository: TermF redistributes none.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SkillSource {
    /// `owner/repo`, or the package that installs them.
    pub repo: &'static str,
    pub r#for: String,
    pub url: String,
    pub command: String,
    /// SPDX, as the repository or its skills declare it; empty when they declare none.
    pub license: &'static str,
}

/// Servers by tag, from each vendor's docs (checked 2026-11-22).
const MCP: [(&str, &str, &str, &str, bool); 9] = [
    (
        "nextjs",
        "next-devtools",
        "npx -y next-devtools-mcp@latest",
        "https://nextjs.org/docs/app/guides/mcp",
        false,
    ),
    (
        "angular",
        "angular-cli",
        "npx -y @angular/cli mcp",
        "https://angular.dev/ai/mcp",
        true,
    ),
    (
        "svelte",
        "svelte",
        "https://mcp.svelte.dev/mcp",
        "https://svelte.dev/docs/mcp",
        false,
    ),
    (
        "nuxt",
        "nuxt",
        "https://nuxt.com/mcp",
        "https://nuxt.com/docs/4.x/guide/ai/mcp",
        false,
    ),
    (
        "go",
        "gopls",
        "gopls mcp",
        "https://go.dev/gopls/features/mcp",
        false,
    ),
    (
        "playwright",
        "playwright",
        "npx @playwright/mcp@latest",
        "https://github.com/microsoft/playwright-mcp",
        false,
    ),
    (
        "ios",
        "xcode",
        "xcrun mcpbridge",
        "https://developer.apple.com/documentation/xcode/giving-external-agents-access-to-xcode",
        true,
    ),
    (
        "dotnet",
        "microsoft-learn",
        "https://learn.microsoft.com/api/mcp",
        "https://learn.microsoft.com/en-us/training/support/mcp",
        false,
    ),
    (
        "terraform",
        "terraform",
        "docker run -i --rm hashicorp/terraform-mcp-server",
        "https://github.com/hashicorp/terraform-mcp-server",
        false,
    ),
];

/// Vendor repositories by tag: a Claude Code marketplace when they have one, else the `skills` CLI.
const SOURCES: [(&str, &str, bool, &str); 16] = [
    ("angular", "angular/skills", false, "MIT"),
    ("dotnet", "dotnet/skills", true, "MIT"),
    ("terraform", "hashicorp/agent-skills", true, "MPL-2.0"),
    ("android", "android/skills", true, "Apache-2.0"),
    ("kotlin", "Kotlin/kotlin-agent-skills", true, "Apache-2.0"),
    ("redis", "redis/agent-skills", true, "MIT"),
    ("mongodb", "mongodb/agent-skills", true, "Apache-2.0"),
    ("postgres", "supabase/agent-skills", true, "MIT"),
    ("react", "vercel-labs/agent-skills", false, "MIT"),
    ("nextjs", "vercel/next.js", true, "MIT"),
    ("docker", "docker/skills", true, "Apache-2.0"),
    ("gcp", "google/skills", true, "Apache-2.0"),
    ("azure", "microsoft/skills", true, "MIT"),
    ("svelte", "sveltejs/ai-tools", true, "MIT"),
    ("prisma", "prisma/skills", false, "MIT"),
    ("laravel", "laravel/agent-skills", true, ""),
];

/// What the vendors of the repository's stack offer: MCP servers, and skills to install from their repositories.
pub fn offers(stack: &RepoStack) -> (Vec<McpOffer>, Vec<SkillSource>) {
    let has = |tag: &str| stack.tags.iter().any(|t| t.name == tag);
    let mut mcp: Vec<McpOffer> = MCP
        .iter()
        .filter(|(tag, ..)| has(tag))
        .map(|(tag, name, target, docs, runs_code)| {
            let server = if target.starts_with("https://") {
                serde_json::json!({ "type": "http", "url": target })
            } else {
                let mut words = target.split(' ');
                let command = words.next().unwrap_or_default();
                serde_json::json!({ "type": "stdio", "command": command, "args": words.collect::<Vec<_>>() })
            };
            McpOffer {
                name,
                r#for: (*tag).into(),
                server,
                docs,
                runs_code: *runs_code,
            }
        })
        .collect();
    let mut sources: Vec<SkillSource> = SOURCES
        .iter()
        .filter(|(tag, ..)| has(tag))
        .map(|(tag, repo, marketplace, license)| SkillSource {
            repo,
            r#for: (*tag).into(),
            url: format!("https://github.com/{repo}"),
            command: if *marketplace {
                format!("claude plugin marketplace add {repo}")
            } else {
                format!("npx skills add {repo} -a claude-code")
            },
            license,
        })
        .collect();
    // Laravel Boost renders its guidelines and skills itself, into the project, and serves its own MCP server.
    let boost = stack
        .path
        .join("vendor/laravel/boost")
        .is_dir()
        .then_some("laravel/boost");
    if let Some(boost) = boost {
        mcp.push(McpOffer {
            name: "laravel-boost",
            r#for: boost.into(),
            server: serde_json::json!({ "type": "stdio", "command": "php", "args": ["artisan", "boost:mcp"] }),
            docs: "https://laravel.com/docs/13.x/boost",
            runs_code: true,
        });
        if !stack.path.join("boost.json").exists() {
            sources.insert(
                0,
                SkillSource {
                    repo: boost,
                    r#for: "laravel".into(),
                    url: "https://laravel.com/docs/13.x/boost".into(),
                    command: "php artisan boost:install".into(),
                    license: "MIT",
                },
            );
        }
    }
    (mcp, sources)
}

/// A language server Claude Code can start for the stack, so that the agent sees the errors of its edits and
/// finds code by symbol. TermF declares it in the session's plugin only when it is installed.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageServer {
    pub name: &'static str,
    pub r#for: String,
    pub command: &'static str,
    pub installed: bool,
    pub docs: String,
}

struct ServerDef {
    tags: &'static [&'static str],
    /// Servers of the same language are alternatives: the first one installed is declared.
    language: &'static str,
    name: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    extensions: &'static [(&'static str, &'static str)],
    startup_timeout: Option<u64>,
    docs: &'static str,
    /// Found is not enough: rustup puts a `rust-analyzer` proxy in `~/.cargo/bin` that fails until the component
    /// is added, so `--version` must succeed.
    probe: bool,
}

const OFFICIAL: &str = "https://github.com/anthropics/claude-plugins-official/tree/main/plugins/";

/// The configurations of Anthropic's code intelligence plugins (claude-plugins-official, checked 2026-11-22),
/// and basedpyright as an alternative to pyright.
const SERVERS: [ServerDef; 11] = [
    ServerDef {
        tags: &["rust"],
        language: "rust",
        name: "rust-analyzer",
        command: "rust-analyzer",
        args: &[],
        extensions: &[(".rs", "rust")],
        startup_timeout: None,
        docs: "rust-analyzer-lsp",
        probe: true,
    },
    ServerDef {
        tags: &["typescript", "node"],
        language: "typescript",
        name: "typescript",
        command: "typescript-language-server",
        args: &["--stdio"],
        extensions: &[
            (".ts", "typescript"),
            (".tsx", "typescriptreact"),
            (".js", "javascript"),
            (".jsx", "javascriptreact"),
            (".mts", "typescript"),
            (".cts", "typescript"),
            (".mjs", "javascript"),
            (".cjs", "javascript"),
        ],
        startup_timeout: None,
        docs: "typescript-lsp",
        probe: false,
    },
    ServerDef {
        tags: &["python"],
        language: "python",
        name: "pyright",
        command: "pyright-langserver",
        args: &["--stdio"],
        extensions: &[(".py", "python"), (".pyi", "python")],
        startup_timeout: None,
        docs: "pyright-lsp",
        probe: false,
    },
    ServerDef {
        tags: &["python"],
        language: "python",
        name: "basedpyright",
        command: "basedpyright-langserver",
        args: &["--stdio"],
        extensions: &[(".py", "python"), (".pyi", "python")],
        startup_timeout: None,
        docs: "https://docs.basedpyright.com/latest/installation/command-line-and-language-server/",
        probe: false,
    },
    ServerDef {
        tags: &["go"],
        language: "go",
        name: "gopls",
        command: "gopls",
        args: &[],
        extensions: &[(".go", "go")],
        startup_timeout: None,
        docs: "gopls-lsp",
        probe: false,
    },
    ServerDef {
        tags: &["php"],
        language: "php",
        name: "intelephense",
        command: "intelephense",
        args: &["--stdio"],
        extensions: &[(".php", "php")],
        startup_timeout: None,
        docs: "php-lsp",
        probe: false,
    },
    ServerDef {
        tags: &["ruby"],
        language: "ruby",
        name: "ruby-lsp",
        command: "ruby-lsp",
        args: &[],
        extensions: &[
            (".rb", "ruby"),
            (".rake", "ruby"),
            (".gemspec", "ruby"),
            (".ru", "ruby"),
            (".erb", "erb"),
        ],
        startup_timeout: None,
        docs: "ruby-lsp",
        probe: false,
    },
    ServerDef {
        tags: &["java"],
        language: "java",
        name: "jdtls",
        command: "jdtls",
        args: &[],
        extensions: &[(".java", "java")],
        startup_timeout: Some(120_000),
        docs: "jdtls-lsp",
        probe: false,
    },
    ServerDef {
        tags: &["kotlin"],
        language: "kotlin",
        name: "kotlin-lsp",
        command: "kotlin-lsp",
        args: &["--stdio"],
        extensions: &[(".kt", "kotlin"), (".kts", "kotlin")],
        startup_timeout: Some(120_000),
        docs: "kotlin-lsp",
        probe: false,
    },
    ServerDef {
        tags: &["swift", "ios"],
        language: "swift",
        name: "sourcekit-lsp",
        command: "sourcekit-lsp",
        args: &[],
        extensions: &[(".swift", "swift")],
        startup_timeout: None,
        docs: "swift-lsp",
        probe: false,
    },
    ServerDef {
        tags: &["dotnet"],
        language: "csharp",
        name: "csharp-ls",
        command: "csharp-ls",
        args: &[],
        extensions: &[(".cs", "csharp")],
        startup_timeout: None,
        docs: "csharp-lsp",
        probe: false,
    },
];

/// Whether `bin --version` succeeds in `repo`, whose `rust-toolchain.toml` picks rustup's toolchain, within 3 s.
/// Kept a minute per binary and repository, so that listing the library or starting a session does not wait on
/// rustup each time.
fn answers(bin: &Path, repo: &Path) -> bool {
    type Entry = (Option<SystemTime>, Instant, bool);
    static CACHE: Mutex<BTreeMap<(PathBuf, PathBuf), Entry>> = Mutex::new(BTreeMap::new());
    const KEEP: Duration = Duration::from_secs(60);
    const WAIT: Duration = Duration::from_secs(3);
    let key = (bin.to_path_buf(), repo.to_path_buf());
    let modified = fs::metadata(bin).and_then(|m| m.modified()).ok();
    if let Some((at, when, ok)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(&key)
        && *at == modified
        && when.elapsed() < KEEP
    {
        return *ok;
    }
    let ok = Command::new(bin)
        .arg("--version")
        .current_dir(repo)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok_and(|mut child| {
            let start = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => return status.success(),
                    Ok(None) if start.elapsed() < WAIT => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return false;
                    }
                }
            }
        });
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, (modified, Instant::now(), ok));
    ok
}

/// The language servers of `tags`, one a language, found or not in `path` (the `PATH` the sessions get) for the
/// repository at `repo`.
pub fn language_servers(tags: &[Tag], path: &std::ffi::OsStr, repo: &Path) -> Vec<LanguageServer> {
    let language_of = |name: &str| SERVERS.iter().find(|d| d.name == name).map(|d| d.language);
    let mut servers: Vec<LanguageServer> = Vec::new();
    for def in &SERVERS {
        let Some(tag) = tags.iter().find(|t| def.tags.contains(&t.name.as_str())) else {
            continue;
        };
        let server = LanguageServer {
            name: def.name,
            r#for: tag.name.clone(),
            command: def.command,
            installed: termf_claude::cli::which_in(path, def.command)
                .is_some_and(|bin| !def.probe || answers(&bin, repo)),
            docs: match def.docs.starts_with("https://") {
                true => def.docs.to_owned(),
                false => format!("{OFFICIAL}{}", def.docs),
            },
        };
        match servers
            .iter_mut()
            .find(|s| language_of(s.name) == Some(def.language))
        {
            Some(s) if !s.installed && server.installed => *s = server,
            Some(_) => {}
            None => servers.push(server),
        }
    }
    servers
}

/// `.lsp.json` for the installed servers of `servers`, each once.
pub fn lsp_json(servers: &[LanguageServer]) -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    for server in servers.iter().filter(|s| s.installed) {
        let Some(def) = SERVERS.iter().find(|d| d.name == server.name) else {
            continue;
        };
        let mut config = serde_json::json!({
            "command": def.command,
            "extensionToLanguage": def
                .extensions
                .iter()
                .map(|(ext, lang)| ((*ext).to_owned(), Value::from(*lang)))
                .collect::<serde_json::Map<_, _>>(),
        });
        if !def.args.is_empty() {
            config["args"] = def.args.into();
        }
        if let Some(ms) = def.startup_timeout {
            config["startupTimeout"] = ms.into();
        }
        map.insert(def.name.to_owned(), config);
    }
    map
}

/// The checks a repository configures without a script to run them: its typecheck and its lint, each a command
/// to run in the repository, when its tool is installed there (`node_modules/.bin`) or in a folder of `path`.
pub fn checks(repo: &Path, path: &std::ffi::OsStr) -> Vec<String> {
    let has = |file: &str| repo.join(file).is_file();
    let text = |file: &str| fs::read_to_string(repo.join(file)).unwrap_or_default();
    let exe = |name: &str| {
        if cfg!(windows) {
            vec![format!("{name}.cmd"), format!("{name}.exe")]
        } else {
            vec![name.to_owned()]
        }
    };
    let local = |name: &str| {
        exe(name)
            .into_iter()
            .map(|n| Path::new("node_modules").join(".bin").join(n))
            .find(|p| repo.join(p).is_file())
            .map(|p| p.to_string_lossy().into_owned())
    };
    let global = |name: &str| {
        std::env::split_paths(path)
            .any(|dir| exe(name).iter().any(|n| dir.join(n).is_file()))
            .then(|| name.to_owned())
    };
    let (pyproject, setup) = (text("pyproject.toml"), text("setup.cfg"));
    let mut found = Vec::new();
    if has("tsconfig.json")
        && let Some(tsc) = local("tsc").or_else(|| global("tsc"))
    {
        found.push(format!("{tsc} --noEmit -p tsconfig.json"));
    }
    let eslint = [
        "eslint.config.js",
        "eslint.config.mjs",
        "eslint.config.cjs",
        "eslint.config.ts",
        ".eslintrc",
        ".eslintrc.js",
        ".eslintrc.cjs",
        ".eslintrc.json",
        ".eslintrc.yml",
        ".eslintrc.yaml",
    ];
    if eslint.iter().any(|f| has(f))
        && let Some(bin) = local("eslint")
    {
        found.push(format!("{bin} ."));
    }
    let mypy = has("mypy.ini")
        || has(".mypy.ini")
        || pyproject.contains("[tool.mypy]")
        || setup.contains("[mypy");
    if mypy && let Some(bin) = global("mypy") {
        found.push(format!("{bin} ."));
    }
    if (has("pyrightconfig.json") || pyproject.contains("[tool.pyright]"))
        && let Some(bin) = global("pyright")
    {
        found.push(bin);
    }
    if (has("ruff.toml") || has(".ruff.toml") || pyproject.contains("[tool.ruff]"))
        && let Some(bin) = global("ruff")
    {
        found.push(format!("{bin} check ."));
    }
    if has("Cargo.toml")
        && let Some(cargo) = global("cargo")
    {
        found.push(format!("{cargo} clippy --all-targets --quiet"));
    }
    found
}

#[cfg(test)]
mod tests;
