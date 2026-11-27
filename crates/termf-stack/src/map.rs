//! What each repository of a project is, read from its files (nothing is run), and how the repositories are linked:
//! which one calls another (a URL of its env or Compose files names it, or a port it publishes), which one depends on
//! a package another publishes, and which ones share a database.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use termf_library::stack::{Tag, stack_tags};
use yaml_rust2::{Yaml, YamlLoader};

use crate::deps::{self, Dep, Ecosystem};

const COMPOSE_FILES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
];
/// Where a repository says which variables it expects, in this order.
const ENV_TEMPLATES: [&str; 6] = [
    ".env.example",
    ".env.sample",
    ".env.dist",
    ".env.template",
    ".env.local.example",
    ".env.development",
];
/// Read for the hosts of the URLs only: no value leaves this module.
const ENV_FILES: [&str; 3] = [".env", ".env.local", ".env.development.local"];
const PORT_KEYS: [&str; 4] = ["PORT", "APP_PORT", "HTTP_PORT", "SERVER_PORT"];
const LOCAL_HOSTS: [&str; 5] = [
    "localhost",
    "127.0.0.1",
    "0.0.0.0",
    "host.docker.internal",
    "::1",
];

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub name: String,
    pub image: Option<String>,
    /// The ports it publishes on the host.
    pub ports: Vec<u16>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Database {
    /// `postgres`, `mysql`, `sqlite`, `redis`…
    pub engine: String,
    /// Its name, or a SQLite file's name.
    pub name: String,
    pub host: String,
    pub port: Option<u16>,
    /// Where it is declared: `.env:DATABASE_URL`, `compose.yaml:db`…
    pub source: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub file: String,
    pub name: String,
    pub jobs: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepoMap {
    pub name: String,
    pub path: String,
    pub tags: Vec<Tag>,
    pub deps: Vec<Dep>,
    pub services: Vec<Service>,
    pub databases: Vec<Database>,
    pub ports: Vec<u16>,
    /// The variables it expects, by name, and the file that lists them.
    pub env: Vec<String>,
    pub env_file: Option<String>,
    pub workflows: Vec<Workflow>,
    /// The packages it publishes: `@acme/sdk`, `acme/shared`.
    pub publishes: Vec<String>,
    #[serde(skip)]
    published: Vec<(Ecosystem, String)>,
    /// The URLs of its env and Compose files: the variable, the host and the port.
    #[serde(skip)]
    calls: Vec<(String, String, Option<u16>)>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum LinkKind {
    Calls,
    Package,
    Database,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    /// Repositories, by path: `from` calls or depends on `to`; a shared database has no direction.
    pub from: String,
    pub to: String,
    pub kind: LinkKind,
    /// What says so: the variables, the packages, the databases.
    pub via: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StackMap {
    pub repos: Vec<RepoMap>,
    pub links: Vec<Link>,
}

/// `KEY=value` pairs of an env file, `export` and quotes taken off.
fn env_pairs(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let l = l.strip_prefix("export ").unwrap_or(l);
            let (key, value) = l.split_once('=')?;
            let key = key.trim();
            let ok = !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            ok.then(|| {
                (
                    key.to_owned(),
                    value.trim().trim_matches(['"', '\'']).to_owned(),
                )
            })
        })
        .collect()
}

/// Compose's variables by their default: `${API_PORT:-3001}:3000` → `3001:3000`.
fn defaults(text: &str) -> String {
    let mut text = text.trim().to_owned();
    while let Some(start) = text.find("${") {
        let Some(len) = text[start..].find('}') else {
            break;
        };
        let inner = &text[start + 2..start + len];
        let default = inner.split_once(":-").map_or("", |(_, d)| d).to_owned();
        text.replace_range(start..=start + len, &default);
    }
    text
}

fn port_of(text: &str) -> Option<u16> {
    defaults(text).parse().ok()
}

/// The host ports of a Compose service: `"3001:3000"`, `"127.0.0.1:3001:3000/tcp"`, `{published: 3001}`.
fn published_ports(service: &Yaml) -> Vec<u16> {
    service["ports"]
        .as_vec()
        .into_iter()
        .flatten()
        .filter_map(|p| match p {
            Yaml::Hash(_) => match &p["published"] {
                Yaml::Integer(n) => u16::try_from(*n).ok(),
                Yaml::String(s) => port_of(s),
                _ => None,
            },
            Yaml::Integer(n) => u16::try_from(*n).ok(),
            Yaml::String(s) => {
                let s = defaults(s);
                let s = s.split('/').next().unwrap_or_default();
                let parts: Vec<&str> = s.rsplitn(3, ':').collect();
                port_of(parts.get(1).unwrap_or(&parts[0]))
            }
            _ => None,
        })
        .collect()
}

/// What a URL in an env value points to, for a call: not a database's.
fn call_target(value: &str) -> Option<(String, Option<u16>)> {
    let url = termf_worktree::parse_url(value).ok()?;
    matches!(
        url.scheme.as_str(),
        "http" | "https" | "ws" | "wss" | "grpc" | "grpcs"
    )
    .then(|| (url.host.to_ascii_lowercase(), url.port))
    .filter(|(host, _)| !host.is_empty())
}

/// The stack's tags, Node's version read from `.nvmrc` too: endoflife.date needs it.
pub(crate) fn tags(repo: &Path) -> Vec<Tag> {
    let mut tags = stack_tags(repo);
    let pinned = [".nvmrc", ".node-version"]
        .iter()
        .find_map(|f| fs::read_to_string(repo.join(f)).ok())
        .and_then(|v| termf_library::stack::clean(v.trim().trim_start_matches("lts/")));
    if let Some(v) = pinned {
        match tags.iter_mut().find(|t| t.name == "node") {
            Some(tag) if tag.version.is_none() => tag.version = Some(v),
            Some(_) => {}
            None => tags.push(Tag {
                name: "node".into(),
                version: Some(v),
                source: ".nvmrc".into(),
            }),
        }
    }
    tags
}

/// What `repo` is made of.
pub fn read(name: &str, repo: &Path) -> RepoMap {
    let tags = tags(repo);
    let mut calls = Vec::new();
    let mut ports: Vec<u16> = Vec::new();
    let mut services = Vec::new();
    let compose = COMPOSE_FILES.iter().find_map(|f| {
        let text = fs::read_to_string(repo.join(f)).ok()?;
        YamlLoader::load_from_str(&text).ok()?.into_iter().next()
    });
    if let Some(Yaml::Hash(list)) = compose.as_ref().map(|c| &c["services"]) {
        for (service_name, service) in list {
            let Some(service_name) = service_name.as_str() else {
                continue;
            };
            let published = published_ports(service);
            ports.extend(&published);
            let environment: Vec<(String, String)> = match &service["environment"] {
                Yaml::Hash(h) => h
                    .iter()
                    .filter_map(|(k, v)| {
                        let value = v
                            .as_str()
                            .map(str::to_owned)
                            .or_else(|| v.as_i64().map(|n| n.to_string()))?;
                        Some((k.as_str()?.to_owned(), value))
                    })
                    .collect(),
                Yaml::Array(a) => env_pairs(
                    &a.iter()
                        .filter_map(Yaml::as_str)
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                _ => Vec::new(),
            };
            for (key, value) in environment {
                if let Some((host, port)) = call_target(&value) {
                    calls.push((key, host, port));
                }
            }
            services.push(Service {
                name: service_name.to_owned(),
                image: service["image"].as_str().map(str::to_owned),
                ports: published,
            });
        }
    }

    let template = ENV_TEMPLATES
        .iter()
        .find_map(|f| Some((f.to_string(), fs::read_to_string(repo.join(f)).ok()?)));
    let env: Vec<String> = template
        .as_ref()
        .map(|(_, text)| env_pairs(text).into_iter().map(|(k, _)| k).collect())
        .unwrap_or_default();
    let texts = template.iter().map(|(_, t)| t.clone()).chain(
        ENV_FILES
            .iter()
            .filter_map(|f| fs::read_to_string(repo.join(f)).ok()),
    );
    for text in texts {
        for (key, value) in env_pairs(&text) {
            if PORT_KEYS.contains(&key.as_str())
                && let Some(port) = port_of(&value)
            {
                ports.push(port);
            }
            if let Some((host, port)) = call_target(&value)
                && !calls
                    .iter()
                    .any(|(k, h, p)| *k == key && *h == host && *p == port)
            {
                calls.push((key, host, port));
            }
        }
    }
    ports.sort();
    ports.dedup();

    let mut databases: Vec<Database> = termf_worktree::declared(repo)
        .into_iter()
        .map(|d| Database {
            name: if d.url.scheme == "sqlite" {
                Path::new(&d.url.database)
                    .file_name()
                    .map_or(d.url.database.clone(), |n| n.to_string_lossy().into_owned())
            } else {
                d.url.database.clone()
            },
            engine: d.url.scheme,
            host: d.url.host,
            port: d.url.port,
            source: d.source,
        })
        .collect();
    // The template is what the repository commits: Ops' detection reads the real files only.
    if let Some((file, text)) = &template {
        for (key, value) in env_pairs(text) {
            let named =
                key.contains("DATABASE") || key.starts_with("DB_") || key.ends_with("_DB_URL");
            let Some(url) = named
                .then(|| termf_worktree::parse_url(&value).ok())
                .flatten()
                .filter(|u| !u.database.is_empty() && value.contains("://"))
            else {
                continue;
            };
            let engine = match url.scheme.as_str() {
                "postgresql" | "pgsql" | "postgis" => "postgres",
                "mysql2" | "trilogy" => "mysql",
                "rediss" => "redis",
                "mongodb+srv" => "mongodb",
                other => other,
            };
            let db = Database {
                engine: engine.into(),
                name: url.database,
                host: url.host,
                port: url.port,
                source: format!("{file}:{key}"),
            };
            if !databases.iter().any(|d| {
                (&d.engine, &d.host, d.port, &d.name) == (&db.engine, &db.host, db.port, &db.name)
            }) {
                databases.push(db);
            }
        }
    }

    let mut workflows: Vec<Workflow> = fs::read_dir(repo.join(".github/workflows"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().into_owned();
            if !(file.ends_with(".yml") || file.ends_with(".yaml")) {
                return None;
            }
            let text = fs::read_to_string(entry.path()).ok()?;
            let name = YamlLoader::load_from_str(&text)
                .ok()
                .and_then(|d| d.into_iter().next())
                .and_then(|d| d["name"].as_str().map(str::to_owned))
                .unwrap_or_else(|| file.clone());
            Some(Workflow {
                jobs: termf_ci::parse_graph(&text).map_or(0, |j| j.len()),
                file: format!(".github/workflows/{file}"),
                name,
            })
        })
        .collect();
    if repo.join(".gitlab-ci.yml").is_file() {
        workflows.push(Workflow {
            file: ".gitlab-ci.yml".into(),
            name: "GitLab CI".into(),
            jobs: 0,
        });
    }
    workflows.sort_by(|a, b| a.file.cmp(&b.file));

    let published = deps::published(repo);
    RepoMap {
        name: name.into(),
        path: repo.to_string_lossy().into_owned(),
        tags,
        deps: deps::read(repo),
        services,
        databases,
        ports,
        env,
        env_file: template.map(|(f, _)| f),
        workflows,
        publishes: published.iter().map(|(_, n)| n.clone()).collect(),
        published,
        calls,
    }
}

/// A database's identity across repositories: its engine, where it runs and its name.
fn db_key(db: &Database) -> Option<(String, String, Option<u16>, String)> {
    if db.name.is_empty() || db.engine == "sqlite" {
        return None;
    }
    let host = if LOCAL_HOSTS.contains(&db.host.as_str()) || db.host.is_empty() {
        "local".to_owned()
    } else {
        db.host.to_ascii_lowercase()
    };
    Some((db.engine.clone(), host, db.port, db.name.clone()))
}

fn add(links: &mut Vec<Link>, from: &str, to: &str, kind: LinkKind, via: String) {
    match links
        .iter_mut()
        .find(|l| l.from == from && l.to == to && l.kind == kind)
    {
        Some(link) if !link.via.contains(&via) => link.via.push(via),
        Some(_) => {}
        None => links.push(Link {
            from: from.into(),
            to: to.into(),
            kind,
            via: vec![via],
        }),
    }
}

/// How the repositories are linked.
pub fn links(repos: &[RepoMap]) -> Vec<Link> {
    let mut links = Vec::new();
    for a in repos {
        for b in repos.iter().filter(|b| b.path != a.path) {
            // `a` calls `b`: a URL names `b` (or one of its packages, or services), or a port only `b` publishes.
            let mut names: Vec<String> = vec![b.name.to_ascii_lowercase()];
            names.extend(
                b.services
                    .iter()
                    .filter(|s| s.image.is_none())
                    .map(|s| s.name.to_ascii_lowercase()),
            );
            for (key, host, port) in &a.calls {
                let named = names.contains(host);
                let local = LOCAL_HOSTS.contains(&host.as_str())
                    && port.is_some_and(|p| b.ports.contains(&p) && !a.ports.contains(&p));
                if named || local {
                    add(&mut links, &a.path, &b.path, LinkKind::Calls, key.clone());
                }
            }
            for dep in &a.deps {
                if b.published
                    .iter()
                    .any(|(eco, name)| *eco == dep.ecosystem && *name == dep.name)
                {
                    add(
                        &mut links,
                        &a.path,
                        &b.path,
                        LinkKind::Package,
                        dep.name.clone(),
                    );
                }
            }
            if a.path < b.path {
                for db in &a.databases {
                    if db_key(db)
                        .is_some_and(|k| b.databases.iter().any(|o| db_key(o).as_ref() == Some(&k)))
                    {
                        add(
                            &mut links,
                            &a.path,
                            &b.path,
                            LinkKind::Database,
                            db.name.clone(),
                        );
                    }
                }
            }
        }
    }
    links
}

/// The map of `repos` (name, path).
pub fn map(repos: &[(String, PathBuf)]) -> StackMap {
    let repos: Vec<RepoMap> = std::thread::scope(|s| {
        let handles: Vec<_> = repos
            .iter()
            .map(|(name, path)| s.spawn(move || read(name, path)))
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    let links = links(&repos);
    StackMap { repos, links }
}
