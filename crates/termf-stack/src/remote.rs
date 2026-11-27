//! What the outside says of a repository's versions, read only and kept on disk for a day: the versions each
//! registry has, the known vulnerabilities from OSV.dev (one batch query for every ecosystem), and the support of
//! runtimes and frameworks from endoflife.date. Only package names and versions leave the machine.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use termf_library::stack::Tag;

use crate::deps::{Dep, Ecosystem};
use crate::version::{self, Bump};

/// How long an answer is kept before it is asked again.
pub const KEEP_FOR: u64 = 24 * 3600;
const THREADS: usize = 8;
const OSV_BATCH: usize = 1000;

/// One HTTP request: a GET, or a POST of `body` as JSON.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub url: String,
    pub body: Option<String>,
    pub accept: Option<&'static str>,
}

/// What a request gets: the body, `Ok(None)` when the server answers an error status (an unknown package), `Err`
/// when it cannot be reached.
pub type Fetch<'a> = dyn Fn(&Request) -> Result<Option<Vec<u8>>> + Sync + 'a;

/// The system's curl, HTTPS only.
pub fn curl(request: &Request) -> Result<Option<Vec<u8>>> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-q",
        "-fsSL",
        "-g",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
    ])
    .args([
        "--max-redirs",
        "3",
        "--connect-timeout",
        "5",
        "--max-time",
        "25",
    ])
    .args(["--max-filesize", "67108864"]);
    if let Some(accept) = request.accept {
        cmd.args(["-H", &format!("Accept: {accept}")]);
    }
    if request.body.is_some() {
        cmd.args([
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
        ]);
    }
    let mut child = cmd
        .arg(&request.url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run curl: is it installed?")?;
    if let Some(body) = &request.body {
        use std::io::Write;
        child
            .stdin
            .take()
            .context("no stdin")?
            .write_all(body.as_bytes())?;
    }
    drop(child.stdin.take());
    let out = child.wait_with_output()?;
    match out.status.code() {
        Some(0) => Ok(Some(out.stdout)),
        // The server answered with an error status.
        Some(22) => Ok(None),
        _ => bail!(
            "cannot reach {}: {}",
            host(&request.url),
            String::from_utf8_lossy(&out.stderr).trim()
        ),
    }
}

fn host(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .unwrap_or(url)
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Entry {
    at: u64,
    value: Value,
}

/// Answers by key (`npm:react`, `osv:npm:react@18.2.0`, `vuln:GHSA-…`, `eol:nodejs`), each with its time.
#[derive(Serialize, Deserialize, Default)]
struct Store {
    entries: BTreeMap<String, Entry>,
}

/// Each cache file, read once and shared by the checks running at the same time.
static STORES: Mutex<BTreeMap<PathBuf, Store>> = Mutex::new(BTreeMap::new());

fn store<'a>(stores: &'a mut BTreeMap<PathBuf, Store>, file: &Path) -> &'a mut Store {
    stores.entry(file.to_path_buf()).or_insert_with(|| {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    })
}

/// `key`'s answer when younger than a day and not `refresh`; with `stale`, of any age.
fn cached(file: &Path, key: &str, refresh: bool, stale: bool) -> Option<Entry> {
    let mut stores = STORES.lock().unwrap_or_else(|e| e.into_inner());
    let entry = store(&mut stores, file).entries.get(key)?.clone();
    (stale || (!refresh && now().saturating_sub(entry.at) < KEEP_FOR)).then_some(entry)
}

fn keep(file: &Path, found: Vec<(String, Value)>) -> Result<()> {
    if found.is_empty() {
        return Ok(());
    }
    let mut stores = STORES.lock().unwrap_or_else(|e| e.into_inner());
    let store = store(&mut stores, file);
    let at = now();
    // Older than a week: dropped, so the file does not grow forever.
    store
        .entries
        .retain(|_, e| at.saturating_sub(e.at) < 7 * KEEP_FOR);
    for (key, value) in found {
        store.entries.insert(key, Entry { at, value });
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    termf_core::write_atomic(file, &serde_json::to_string(store)?)
}

/// Asks for every key missing from the cache, a few at a time; once the network fails, the rest is not asked.
struct Asker<'a> {
    fetch: &'a Fetch<'a>,
    cache: &'a Path,
    refresh: bool,
    offline: AtomicBool,
    errors: Mutex<Vec<String>>,
    oldest: Mutex<u64>,
}

impl Asker<'_> {
    fn seen(&self, at: u64) {
        let mut oldest = self.oldest.lock().unwrap();
        *oldest = (*oldest).min(at);
    }

    /// The value of each key: from the cache, else from its request read by `parse` (given the key); `Value::Null`
    /// for none. What the network cannot give, an older answer does.
    fn ask(
        &self,
        keys: Vec<(String, Request)>,
        parse: &(dyn Fn(&str, &[u8]) -> Value + Sync),
    ) -> HashMap<String, Value> {
        let mut out = HashMap::new();
        let mut missing = Vec::new();
        for (key, request) in keys {
            match cached(self.cache, &key, self.refresh, false) {
                Some(entry) => {
                    self.seen(entry.at);
                    out.insert(key, entry.value);
                }
                None => missing.push((key, request)),
            }
        }
        let next = AtomicUsize::new(0);
        let found = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..THREADS.min(missing.len()) {
                s.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some((key, request)) = missing.get(i) else {
                            break;
                        };
                        if self.offline.load(Ordering::Relaxed) {
                            continue;
                        }
                        match (self.fetch)(request) {
                            Ok(body) => {
                                let value = body.map_or(Value::Null, |b| parse(key, &b));
                                found.lock().unwrap().push((key.clone(), value));
                            }
                            Err(e) => {
                                if !self.offline.swap(true, Ordering::Relaxed) {
                                    self.errors.lock().unwrap().push(format!("{e:#}"));
                                }
                            }
                        }
                    }
                });
            }
        });
        let found = found.into_inner().unwrap();
        out.extend(found.iter().cloned());
        if let Err(e) = keep(self.cache, found) {
            self.errors.lock().unwrap().push(format!("{e:#}"));
        }
        for (key, _) in missing {
            if !out.contains_key(&key)
                && let Some(entry) = cached(self.cache, &key, true, true)
            {
                self.seen(entry.at);
                out.insert(key, entry.value);
            }
        }
        out
    }
}

const ECOSYSTEMS: [Ecosystem; 6] = [
    Ecosystem::Npm,
    Ecosystem::Packagist,
    Ecosystem::Crates,
    Ecosystem::PyPI,
    Ecosystem::Go,
    Ecosystem::RubyGems,
];

/// Where each registry lists a package's versions.
fn registry(eco: Ecosystem, name: &str) -> Request {
    let (url, accept) = match eco {
        Ecosystem::Npm => (
            format!("https://registry.npmjs.org/{}", name.replace('/', "%2f")),
            Some("application/vnd.npm.install-v1+json"),
        ),
        Ecosystem::Packagist => (format!("https://repo.packagist.org/p2/{name}.json"), None),
        Ecosystem::Crates => {
            let lower = name.to_ascii_lowercase();
            let prefix = match lower.len() {
                1 => "1".to_owned(),
                2 => "2".to_owned(),
                3 => format!("3/{}", &lower[..1]),
                _ => format!("{}/{}", &lower[..2], &lower[2..4]),
            };
            (format!("https://index.crates.io/{prefix}/{lower}"), None)
        }
        Ecosystem::PyPI => (format!("https://pypi.org/pypi/{name}/json"), None),
        Ecosystem::Go => {
            // The module proxy's case encoding: `Azure` is `!azure`.
            let escaped: String = name
                .chars()
                .flat_map(|c| {
                    if c.is_ascii_uppercase() {
                        vec!['!', c.to_ascii_lowercase()]
                    } else {
                        vec![c]
                    }
                })
                .collect();
            (format!("https://proxy.golang.org/{escaped}/@v/list"), None)
        }
        Ecosystem::RubyGems => (
            format!("https://rubygems.org/api/v1/versions/{name}.json"),
            None,
        ),
    };
    Request {
        url,
        body: None,
        accept,
    }
}

/// The versions a registry's answer lists, yanked ones left out.
pub fn versions(eco: Ecosystem, name: &str, body: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(body);
    let json = || serde_json::from_str::<Value>(&text).unwrap_or_default();
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect()
    };
    match eco {
        Ecosystem::Npm => json()["versions"]
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default(),
        Ecosystem::Packagist => strings(&Value::Array(
            json()["packages"][name]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| p["version"].clone())
                .collect(),
        )),
        Ecosystem::Crates => text
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v["yanked"] != true)
            .filter_map(|v| v["vers"].as_str().map(str::to_owned))
            .collect(),
        Ecosystem::PyPI => json()["releases"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(_, files)| {
                files
                    .as_array()
                    .is_none_or(|f| f.is_empty() || f.iter().any(|f| f["yanked"] != true))
            })
            .map(|(v, _)| v.clone())
            .collect(),
        Ecosystem::Go => text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect(),
        Ecosystem::RubyGems => json()
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["number"].as_str().map(str::to_owned))
            .collect(),
    }
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Unknown,
    Low,
    Moderate,
    High,
    Critical,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Vuln {
    pub id: String,
    /// Its other names (`CVE-…`, `GHSA-…`), the same vulnerability under several databases.
    pub aliases: Vec<String>,
    pub summary: String,
    pub severity: Severity,
    /// The first version that fixes it after the one used; `None` when none does yet.
    pub fixed: Option<String>,
    pub url: String,
}

/// GitHub's level, else a CVSS v3 score's band; `Unknown` without either.
fn severity(vuln: &Value) -> Severity {
    let level = vuln["database_specific"]["severity"]
        .as_str()
        .or_else(|| {
            vuln["affected"]
                .as_array()?
                .iter()
                .find_map(|a| a["database_specific"]["severity"].as_str())
        })
        .unwrap_or_default()
        .to_ascii_uppercase();
    match level.as_str() {
        "CRITICAL" => Severity::Critical,
        "HIGH" => Severity::High,
        "MODERATE" | "MEDIUM" => Severity::Moderate,
        "LOW" => Severity::Low,
        _ => Severity::Unknown,
    }
}

/// The first version after `used` that fixes `vuln` for the package: the end of the range `used` is in, else the
/// lowest fix above it.
fn fixed(vuln: &Value, dep: &Dep, used: &str) -> Option<String> {
    let mut fixes: Vec<String> = Vec::new();
    for affected in vuln["affected"].as_array().into_iter().flatten() {
        let package = &affected["package"];
        let same = package["ecosystem"].as_str() == Some(dep.ecosystem.osv())
            && package["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(&dep.name));
        if !same {
            continue;
        }
        for range in affected["ranges"].as_array().into_iter().flatten() {
            if range["type"] == "GIT" {
                continue;
            }
            let mut introduced: Option<&str> = None;
            for event in range["events"].as_array().into_iter().flatten() {
                if let Some(v) = event["introduced"].as_str() {
                    introduced = Some(v);
                } else if let Some(v) = event["fixed"].as_str() {
                    let inside = introduced.is_none_or(|i| {
                        i == "0" || version::compare(used, i).is_some_and(|o| o.is_ge())
                    }) && version::compare(used, v).is_some_and(|o| o.is_lt());
                    if inside {
                        return Some(v.to_owned());
                    }
                    fixes.push(v.to_owned());
                }
            }
        }
    }
    fixes
        .into_iter()
        .filter(|f| version::compare(f, used).is_some_and(|o| o.is_gt()))
        .min_by(|a, b| version::compare(a, b).unwrap_or(std::cmp::Ordering::Equal))
}

/// OSV's batch answer matched back to the dependencies queried (in the same order), with each vulnerability's
/// details by id: one entry per vulnerability, its aliases under another database folded into it (the most severe
/// level kept).
pub fn match_osv(
    queried: &[&Dep],
    batch: &Value,
    details: &HashMap<String, Value>,
) -> Vec<Vec<Vuln>> {
    let results = batch["results"].as_array().cloned().unwrap_or_default();
    queried
        .iter()
        .enumerate()
        .map(|(i, dep)| {
            let used = dep.version.as_deref().unwrap_or_default();
            let mut vulns: Vec<Vuln> = Vec::new();
            let ids = results
                .get(i)
                .and_then(|r| r["vulns"].as_array().cloned())
                .unwrap_or_default();
            for id in ids.iter().filter_map(|v| v["id"].as_str()) {
                let detail = details.get(id).cloned().unwrap_or(Value::Null);
                let aliases: Vec<String> = detail["aliases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| a.as_str().map(str::to_owned))
                    .collect();
                let vuln = Vuln {
                    id: id.to_owned(),
                    summary: detail["summary"]
                        .as_str()
                        .or(detail["details"].as_str().and_then(|d| d.lines().next()))
                        .unwrap_or_default()
                        .to_owned(),
                    severity: severity(&detail),
                    fixed: fixed(&detail, dep, used),
                    url: format!("https://osv.dev/vulnerability/{id}"),
                    aliases,
                };
                let same = vulns.iter_mut().find(|v| {
                    v.aliases.contains(&vuln.id)
                        || vuln.aliases.contains(&v.id)
                        || v.aliases
                            .iter()
                            .any(|a| a.starts_with("CVE-") && vuln.aliases.contains(a))
                });
                match same {
                    Some(known) => {
                        // GitHub's advisory names it best: its id first.
                        if vuln.id.starts_with("GHSA-") && !known.id.starts_with("GHSA-") {
                            let older = std::mem::replace(known, vuln.clone());
                            known.aliases.push(older.id);
                        } else {
                            known.aliases.push(vuln.id.clone());
                        }
                        known.severity = known.severity.clone().max(vuln.severity);
                        if known.summary.is_empty() {
                            known.summary = vuln.summary;
                        }
                        known.fixed = known.fixed.take().or(vuln.fixed);
                    }
                    None => vulns.push(vuln),
                }
            }
            vulns.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.id.cmp(&b.id)));
            vulns
        })
        .collect()
}

/// The endoflife.date product of a stack tag, and the dependency it is, when it is one.
pub const PRODUCTS: [(&str, &str, &str); 24] = [
    ("node", "nodejs", ""),
    ("php", "php", ""),
    ("python", "python", ""),
    ("go", "go", ""),
    ("ruby", "ruby", ""),
    ("rust", "rust", ""),
    ("java", "java", ""),
    ("dotnet", "dotnet", ""),
    ("kotlin", "kotlin", ""),
    ("laravel", "laravel", "laravel/framework"),
    ("symfony", "symfony", "symfony/framework-bundle"),
    ("django", "django", "django"),
    ("rails", "rails", "rails"),
    ("react", "react", "react"),
    ("angular", "angular", "@angular/core"),
    ("vue", "vue", "vue"),
    ("nuxt", "nuxt", "nuxt"),
    ("nextjs", "nextjs", "next"),
    ("spring-boot", "spring-boot", ""),
    ("postgres", "postgresql", ""),
    ("mysql", "mysql", ""),
    ("redis", "redis", ""),
    ("mongodb", "mongodb", ""),
    ("tailwind", "tailwind-css", "tailwindcss"),
];

/// A runtime's or a framework's support: its cycle and when that cycle ends, and the current LTS.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Support {
    pub tag: String,
    pub product: String,
    /// The dependency it is, for a framework (`laravel/framework`).
    pub dep: Option<String>,
    pub version: String,
    pub cycle: Option<String>,
    /// The end of the cycle's support, `YYYY-MM-DD`; `None` when not announced.
    pub eol: Option<String>,
    /// Its support has ended.
    pub ended: bool,
    pub lts: bool,
    /// The newest release of its cycle, and of all.
    pub latest_in_cycle: Option<String>,
    pub latest: Option<String>,
    /// The newest cycle still supported that is an LTS.
    pub current_lts: Option<String>,
}

/// The support of `version` read from endoflife.date's cycles (newest first), on `today`.
pub fn support(cycles: &Value, version: &str, today: &str) -> Option<(Support, Option<String>)> {
    let cycles = cycles.as_array()?;
    let parts: Vec<&str> = version.split('.').collect();
    let date_or = |v: &Value| v.as_str().map(str::to_owned);
    let passed = |v: &Value| match v {
        Value::Bool(b) => *b,
        Value::String(d) => d.as_str() <= today,
        _ => false,
    };
    let lts = |c: &Value| match &c["lts"] {
        Value::Bool(b) => *b,
        Value::String(d) => d.as_str() <= today,
        _ => false,
    };
    let current_lts = cycles
        .iter()
        .find(|c| lts(c) && !passed(&c["eol"]))
        .and_then(|c| {
            c["cycle"]
                .as_str()
                .map(str::to_owned)
                .or(c["cycle"].as_u64().map(|n| n.to_string()))
        });
    let latest = cycles.first().and_then(|c| date_or(&c["latest"]));
    let cycle_name = |c: &Value| {
        c["cycle"]
            .as_str()
            .map(str::to_owned)
            .or(c["cycle"].as_u64().map(|n| n.to_string()))
    };
    // The longest cycle that starts the version: `3.12` for Python 3.12.1, `20` for Node 20.11.
    let found = cycles
        .iter()
        .filter_map(|c| Some((cycle_name(c)?, c)))
        .filter(|(name, _)| {
            let want: Vec<&str> = name.split('.').collect();
            want.len() <= parts.len() && want.iter().zip(&parts).all(|(a, b)| a == b)
        })
        .max_by_key(|(name, _)| name.split('.').count());
    let support = match found {
        Some((name, c)) => Support {
            tag: String::new(),
            product: String::new(),
            dep: None,
            version: version.into(),
            cycle: Some(name),
            eol: date_or(&c["eol"]),
            ended: passed(&c["eol"]),
            lts: lts(c),
            latest_in_cycle: date_or(&c["latest"]),
            latest,
            current_lts: current_lts.clone(),
        },
        None => Support {
            tag: String::new(),
            product: String::new(),
            dep: None,
            version: version.into(),
            cycle: None,
            eol: None,
            ended: false,
            lts: false,
            latest_in_cycle: None,
            latest,
            current_lts: current_lts.clone(),
        },
    };
    Some((support, current_lts))
}

/// A dependency as the outside sees it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DepCheck {
    pub name: String,
    pub ecosystem: Ecosystem,
    pub version: Option<String>,
    pub latest: Option<String>,
    /// The newest version without a major, when newer than the one used.
    pub safe: Option<String>,
    /// How far going to `latest` goes, and to `safe`.
    pub bump: Option<Bump>,
    pub safe_bump: Option<Bump>,
    pub vulns: Vec<Vuln>,
    /// The vulnerabilities the safe update fixes, by id.
    pub safe_fixes: Vec<String>,
    /// The package manager's command for the safe update, typed in a terminal of the repository.
    pub command: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub deps: Vec<DepCheck>,
    pub support: Vec<Support>,
    /// What could not be asked: the map shows what is local, and says so.
    pub errors: Vec<String>,
    pub offline: bool,
    /// When the oldest answer shown was asked, in seconds since the epoch.
    pub checked: u64,
}

/// Safe to type in a shell: no quote, space or operator.
fn plain(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with('-')
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@/._-".contains(c))
}

/// The package manager's command that takes `dep` to `to`, kept within its major; `None` for a manager whose files
/// it does not update (pip's requirements).
pub fn update_command(dep: &Dep, to: &str) -> Option<String> {
    if !plain(&dep.name) || !plain(to) {
        return None;
    }
    let (name, dev) = (&dep.name, dep.dev);
    Some(match dep.manager.as_str() {
        "npm" => format!(
            "npm install {name}@{to}{}",
            if dev { " --save-dev" } else { "" }
        ),
        "pnpm" => format!(
            "pnpm add {name}@{to}{}",
            if dev { " --save-dev" } else { "" }
        ),
        "yarn" => format!("yarn add {name}@{to}{}", if dev { " --dev" } else { "" }),
        "bun" => format!("bun add {name}@{to}{}", if dev { " --dev" } else { "" }),
        // Quoted: `^` is a glob in zsh's extended globbing.
        "composer" => format!(
            "composer require '{name}:^{to}' --with-all-dependencies{}",
            if dev { " --dev" } else { "" }
        ),
        "cargo" => {
            let from = dep.version.as_deref().filter(|v| plain(v));
            match from {
                Some(from) => format!("cargo update -p {name}@{from} --precise {to}"),
                None => format!("cargo update -p {name} --precise {to}"),
            }
        }
        "uv" => format!("uv lock --upgrade-package '{name}=={to}' && uv sync"),
        "poetry" => format!("poetry update {name}"),
        "go" => format!(
            "go get {name}@v{} && go mod tidy",
            to.trim_start_matches('v')
        ),
        "bundler" => format!("bundle update {name} --conservative"),
        _ => return None,
    })
}

/// What the registries, OSV and endoflife.date say of `deps` and the runtimes and frameworks of `tags`, from the
/// cache in `cache` (a file) unless older than a day or `refresh`.
pub fn check(
    deps: &[Dep],
    tags: &[Tag],
    cache: &Path,
    refresh: bool,
    fetch: &Fetch<'_>,
    today: &str,
) -> Check {
    let asker = Asker {
        fetch,
        cache,
        refresh,
        offline: AtomicBool::new(false),
        errors: Mutex::new(Vec::new()),
        oldest: Mutex::new(now()),
    };
    let queried: Vec<&Dep> = deps.iter().filter(|d| d.version.is_some()).collect();

    let mut names: Vec<(Ecosystem, &str)> = queried
        .iter()
        .map(|d| (d.ecosystem, d.name.as_str()))
        .collect();
    names.sort();
    names.dedup();
    let registry_values = asker.ask(
        names
            .iter()
            .map(|(eco, name)| (format!("{}:{name}", eco.osv()), registry(*eco, name)))
            .collect(),
        &|key, body| {
            let (eco, name) = key.split_once(':').unwrap_or_default();
            let eco = ECOSYSTEMS
                .into_iter()
                .find(|e| e.osv() == eco)
                .unwrap_or(Ecosystem::Npm);
            json!(versions(eco, name, body))
        },
    );

    // OSV: one batch of the ids, then each vulnerability once.
    let osv_keys: Vec<String> = queried
        .iter()
        .map(|d| {
            format!(
                "osv:{}:{}@{}",
                d.ecosystem.osv(),
                d.name,
                d.version.as_deref().unwrap_or_default()
            )
        })
        .collect();
    let mut ids: HashMap<String, Value> = HashMap::new();
    let mut ask_osv: Vec<usize> = Vec::new();
    for (i, key) in osv_keys.iter().enumerate() {
        match cached(cache, key, refresh, false) {
            Some(entry) => {
                asker.seen(entry.at);
                ids.insert(key.clone(), entry.value);
            }
            None => ask_osv.push(i),
        }
    }
    for chunk in ask_osv.chunks(OSV_BATCH) {
        if asker.offline.load(Ordering::Relaxed) {
            break;
        }
        let queries: Vec<Value> = chunk
            .iter()
            .map(|i| {
                let d = queried[*i];
                json!({"package": {"ecosystem": d.ecosystem.osv(), "name": d.name}, "version": d.version})
            })
            .collect();
        let request = Request {
            url: "https://api.osv.dev/v1/querybatch".into(),
            body: Some(json!({ "queries": queries }).to_string()),
            accept: None,
        };
        match fetch(&request) {
            Ok(Some(body)) => {
                let batch: Value = serde_json::from_slice(&body).unwrap_or_default();
                let mut found = Vec::new();
                for (n, i) in chunk.iter().enumerate() {
                    let vulns = batch["results"][n]["vulns"].clone();
                    let vulns = if vulns.is_array() { vulns } else { json!([]) };
                    ids.insert(osv_keys[*i].clone(), vulns.clone());
                    found.push((osv_keys[*i].clone(), vulns));
                }
                let _ = keep(cache, found);
            }
            Ok(None) => asker
                .errors
                .lock()
                .unwrap()
                .push("api.osv.dev refused the query".into()),
            Err(e) => {
                if !asker.offline.swap(true, Ordering::Relaxed) {
                    asker.errors.lock().unwrap().push(format!("{e:#}"));
                }
            }
        }
    }
    for i in ask_osv {
        if !ids.contains_key(&osv_keys[i])
            && let Some(entry) = cached(cache, &osv_keys[i], true, true)
        {
            asker.seen(entry.at);
            ids.insert(osv_keys[i].clone(), entry.value);
        }
    }
    let batch = json!({
        "results": osv_keys.iter().map(|k| json!({"vulns": ids.get(k).cloned().unwrap_or(json!([]))})).collect::<Vec<_>>()
    });
    let mut vuln_ids: Vec<String> = ids
        .values()
        .flat_map(|v| v.as_array().cloned().unwrap_or_default())
        .filter_map(|v| v["id"].as_str().map(str::to_owned))
        .collect();
    vuln_ids.sort();
    vuln_ids.dedup();
    let details: HashMap<String, Value> = asker
        .ask(
            vuln_ids
                .iter()
                .map(|id| {
                    (
                        format!("vuln:{id}"),
                        Request {
                            url: format!("https://api.osv.dev/v1/vulns/{id}"),
                            body: None,
                            accept: None,
                        },
                    )
                })
                .collect(),
            &|_, b| compact(serde_json::from_slice(b).unwrap_or_default()),
        )
        .into_iter()
        .map(|(k, v)| (k.trim_start_matches("vuln:").to_owned(), v))
        .collect();
    let matched = match_osv(&queried, &batch, &details);

    let mut out = Check::default();
    for (dep, vulns) in queried.iter().zip(matched) {
        let key = format!("{}:{}", dep.ecosystem.osv(), dep.name);
        let listed: Vec<String> = registry_values
            .get(&key)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        let used = dep.version.as_deref();
        let (latest, safe) = version::best(listed.iter().map(String::as_str), used);
        let bump = latest.as_deref().and_then(|l| version::bump(used?, l));
        let safe_bump = safe.as_deref().and_then(|s| version::bump(used?, s));
        let safe_fixes = vulns
            .iter()
            .filter(|v| {
                let (Some(fix), Some(to)) = (v.fixed.as_deref(), safe.as_deref()) else {
                    return false;
                };
                version::compare(to, fix).is_some_and(|o| o.is_ge())
            })
            .map(|v| v.id.clone())
            .collect();
        out.deps.push(DepCheck {
            name: dep.name.clone(),
            ecosystem: dep.ecosystem,
            version: dep.version.clone(),
            command: safe.as_deref().and_then(|to| update_command(dep, to)),
            latest,
            safe,
            bump,
            safe_bump,
            vulns,
            safe_fixes,
        });
    }

    // endoflife.date, for each tag with a product and a version.
    let wanted: Vec<(&Tag, &str, &str)> = tags
        .iter()
        .filter_map(|t| {
            let (_, product, dep) = PRODUCTS.iter().find(|(tag, _, _)| *tag == t.name)?;
            t.version.as_ref()?;
            Some((t, *product, *dep))
        })
        .collect();
    let cycles = asker.ask(
        wanted
            .iter()
            .map(|(_, product, _)| {
                (
                    format!("eol:{product}"),
                    Request {
                        url: format!("https://endoflife.date/api/{product}.json"),
                        body: None,
                        accept: None,
                    },
                )
            })
            .collect(),
        &|_, b| serde_json::from_slice(b).unwrap_or_default(),
    );
    for (tag, product, dep) in wanted {
        let version = tag.version.as_deref().unwrap_or_default();
        // A framework's version is the one locked, when the dependency is there.
        let version = deps
            .iter()
            .find(|d| !dep.is_empty() && d.name == dep)
            .and_then(|d| d.version.as_deref())
            .unwrap_or(version);
        let Some(found) = cycles.get(&format!("eol:{product}")) else {
            continue;
        };
        if let Some((mut s, _)) = support(found, version, today) {
            s.tag = tag.name.clone();
            s.product = product.into();
            s.dep = (!dep.is_empty()).then(|| dep.to_owned());
            out.support.push(s);
        }
    }

    out.offline = asker.offline.load(Ordering::Relaxed);
    out.errors = asker.errors.into_inner().unwrap();
    out.checked = asker.oldest.into_inner().unwrap();
    out
}

/// What is kept of a vulnerability: enough to show it and find its fix.
fn compact(v: Value) -> Value {
    json!({
        "id": v["id"],
        "summary": v["summary"],
        "details": v["details"].as_str().map(|d| d.lines().next().unwrap_or_default().chars().take(300).collect::<String>()),
        "aliases": v["aliases"],
        "database_specific": {"severity": v["database_specific"]["severity"]},
        "affected": v["affected"].as_array().into_iter().flatten().map(|a| json!({
            "package": a["package"],
            "ranges": a["ranges"].as_array().into_iter().flatten().map(|r| json!({"type": r["type"], "events": r["events"]})).collect::<Vec<_>>(),
            "database_specific": {"severity": a["database_specific"]["severity"]},
        })).collect::<Vec<_>>(),
    })
}
