//! What a repository is made of, read from its files alone: its package managers and how to install from their
//! cache, the frameworks that matter for its setup, Docker Compose, its local database and its local files.

use std::fs;
use std::path::Path;

use yaml_rust2::{Yaml, YamlLoader};

use crate::{Dep, RepoRecipe, database, ports};

/// Local settings a worktree needs when git ignores them.
const LOCAL_FILES: [&str; 7] = [
    ".env",
    ".env.local",
    ".env.development.local",
    ".env.test.local",
    "auth.json",
    "config/master.key",
    ".bundle/config",
];
const COMPOSE_FILES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
];

/// The recipe detection proposes for a repository. `env` goes to git, which tells the ignored files.
pub fn detect(repo: &Path, env: &[(String, String)]) -> RepoRecipe {
    let has = |file: &str| repo.join(file).exists();
    let mut r = RepoRecipe::default();
    let mut dep = |dir: &str, clone: bool, install: &[&str]| {
        r.deps.push(Dep {
            dir: dir.into(),
            clone,
            install: install.iter().map(|c| c.to_string()).collect(),
        })
    };
    let mut stack = Vec::new();

    if has("composer.json") {
        stack.push("php:composer");
        if has("artisan") {
            stack.push("laravel");
        }
        dep(
            "vendor",
            true,
            &[
                "COMPOSER_DISABLE_NETWORK=1 composer install --no-interaction --no-progress",
                "composer install --no-interaction --no-progress",
            ],
        );
    }
    if has("package.json") {
        let (manager, locked) = node_manager(repo);
        stack.push(match manager {
            "pnpm" => "node:pnpm",
            "yarn" => "node:yarn",
            "bun" => "node:bun",
            _ => "node:npm",
        });
        let install: &[&str] = match (manager, locked) {
            ("pnpm", true) => &[
                "pnpm install --frozen-lockfile --offline",
                "pnpm install --frozen-lockfile",
            ],
            ("pnpm", false) => &["pnpm install --prefer-offline --no-lockfile"],
            // Yarn 2+ installs from its own cache, offline when it has everything.
            ("yarn", true) if has(".yarnrc.yml") => &["yarn install --immutable"],
            ("yarn", true) => &[
                "yarn install --frozen-lockfile --offline",
                "yarn install --frozen-lockfile",
            ],
            ("yarn", false) => &["yarn install --prefer-offline --no-lockfile"],
            ("bun", _) => &["bun install --frozen-lockfile"],
            (_, true) => &[
                "npm ci --offline --no-audit --no-fund",
                "npm ci --prefer-offline --no-audit --no-fund",
            ],
            // Without a lockfile, none is created: it would end up in the run's pull request.
            _ => &["npm install --prefer-offline --no-audit --no-fund --no-package-lock"],
        };
        dep("node_modules", true, install);
        if ["js", "ts", "mjs", "mts", "cjs"]
            .iter()
            .any(|ext| has(&format!("vite.config.{ext}")))
        {
            stack.push("vite");
        }
    }
    // A virtualenv holds absolute paths: it is installed from uv's or pip's cache, never cloned.
    if has("uv.lock") {
        stack.push("python:uv");
        dep(
            ".venv",
            false,
            &["uv sync --frozen --offline", "uv sync --frozen"],
        );
    } else if has("poetry.lock") {
        stack.push("python:poetry");
        dep("", false, &["poetry install --no-interaction"]);
    } else if has("requirements.txt") {
        stack.push("python:pip");
        dep(
            ".venv",
            false,
            &["python3 -m venv .venv && .venv/bin/pip install -r requirements.txt"],
        );
    }
    if has("Gemfile") {
        stack.push("ruby:bundler");
        if has("config/application.rb") {
            stack.push("rails");
        }
        let local = fs::read_to_string(repo.join(".bundle/config"))
            .is_ok_and(|c| c.contains("vendor/bundle"));
        dep(
            if local { "vendor/bundle" } else { "" },
            local,
            &["bundle install --local", "bundle install"],
        );
    }
    if has("go.mod") {
        stack.push("go");
        dep("", false, &["go mod download"]);
    }
    if has("Cargo.toml") {
        stack.push("rust");
        // Cargo builds what is missing from its shared registry: no install step.
        dep("target", true, &[]);
    }
    if let Some(compose) = compose(repo) {
        stack.push("compose");
        r.compose = true;
        r.ports = compose.published.iter().map(|s| ports::name(s)).collect();
        r.ports.retain(|p| !p.is_empty() && p != "port");
    }
    if let Some(from) = database::find(repo) {
        if let Ok(conn) = database::read(repo, &from) {
            stack.push(conn.engine.label());
        }
        r.database = Some(from);
    }
    r.stack = stack.into_iter().map(str::to_owned).collect();

    let mut include: Vec<String> = fs::read_to_string(repo.join(".worktreeinclude"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();
    for file in LOCAL_FILES {
        if has(file) && ignored(repo, file, env) && !include.iter().any(|i| i == file) {
            include.push(file.into());
        }
    }
    r.include = include;
    r
}

/// The package manager and whether its lockfile is there.
fn node_manager(repo: &Path) -> (&'static str, bool) {
    let locks = [
        ("pnpm-lock.yaml", "pnpm"),
        ("bun.lock", "bun"),
        ("bun.lockb", "bun"),
        ("yarn.lock", "yarn"),
        ("package-lock.json", "npm"),
    ];
    if let Some((_, manager)) = locks.iter().find(|(lock, _)| repo.join(lock).exists()) {
        return (manager, true);
    }
    let declared = fs::read_to_string(repo.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["packageManager"].as_str().map(str::to_owned))
        .unwrap_or_default();
    let manager = match declared.split('@').next().unwrap_or_default() {
        "pnpm" => "pnpm",
        "yarn" => "yarn",
        "bun" => "bun",
        _ => "npm",
    };
    (manager, false)
}

pub(crate) fn ignored(repo: &Path, file: &str, env: &[(String, String)]) -> bool {
    crate::command("git")
        .arg("-C")
        .arg(repo)
        .args(["check-ignore", "-q", "--", file])
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A repository's Compose file, read for its services.
pub(crate) struct Compose {
    pub services: Vec<String>,
    /// Services that publish a port.
    pub published: Vec<String>,
}

pub(crate) fn compose(repo: &Path) -> Option<Compose> {
    let text = COMPOSE_FILES
        .iter()
        .find_map(|f| fs::read_to_string(repo.join(f)).ok())?;
    let doc = YamlLoader::load_from_str(&text).ok()?.into_iter().next()?;
    let mut found = Compose {
        services: Vec::new(),
        published: Vec::new(),
    };
    if let Yaml::Hash(services) = &doc["services"] {
        for (name, service) in services {
            let Some(name) = name.as_str() else { continue };
            found.services.push(name.to_owned());
            if service["ports"].as_vec().is_some_and(|p| !p.is_empty()) {
                found.published.push(name.to_owned());
            }
        }
    }
    Some(found)
}
