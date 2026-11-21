//! What only Claude Code's own commands tell: MCP servers, plugins, the account, the version, and the tools
//! around it. Their text output is parsed leniently: a line that does not fit is skipped, never an error.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::{Env, mask};

/// `claude mcp list` checks every server's health, which can take a while.
const SLOW: Duration = Duration::from_secs(60);
const QUICK: Duration = Duration::from_secs(20);

pub struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(env: &Env, program: &str, args: &[&str], timeout: Duration) -> Result<Output> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(env.cwd());
    if let Some(path) = &env.path {
        cmd.env("PATH", path);
    }
    let (status, stdout, stderr) = termf_core::process::run(cmd, b"", timeout)?;
    Ok(Output {
        ok: status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

/// Through its resolved path: on Windows `claude` is often a `claude.cmd` that `Command` would not find.
fn claude(env: &Env, args: &[&str], timeout: Duration) -> Result<Output> {
    let program = which(env, &env.claude)
        .map_or_else(|| env.claude.clone(), |p| p.to_string_lossy().into_owned());
    run(env, &program, args, timeout)
}

fn failure(out: &Output) -> String {
    let text = if out.stderr.trim().is_empty() {
        &out.stdout
    } else {
        &out.stderr
    };
    mask::mask_text(&termf_core::process::tail(text))
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Connected,
    Failed,
    Pending,
    NeedsAuth,
    Unknown,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub name: String,
    /// The command or URL, masked.
    pub target: String,
    pub status: String,
    pub health: Health,
    /// `user`, `project` or `local`, from `claude mcp get`.
    pub scope: Option<String>,
    /// What `claude mcp get` prints, as keys and values, masked.
    pub details: Option<Value>,
}

#[derive(Serialize, Debug, Default)]
pub struct Mcp {
    pub servers: Vec<McpServer>,
    pub error: Option<String>,
}

/// `name: target - status` lines; names may hold spaces and colons (`claude.ai Gmail`, `plugin:x:y`).
pub fn parse_mcp_list(text: &str) -> Vec<McpServer> {
    text.lines()
        .filter_map(|line| {
            let (name, rest) = line.trim().split_once(": ")?;
            let (target, status) = rest.rsplit_once(" - ").unwrap_or((rest, ""));
            let lower = status.to_lowercase();
            let health = if lower.contains("failed") || status.contains(['✘', '✗']) {
                Health::Failed
            } else if lower.contains("pending") {
                Health::Pending
            } else if lower.contains("auth") {
                Health::NeedsAuth
            } else if lower.contains("connected") || status.contains(['✓', '✔']) {
                Health::Connected
            } else {
                Health::Unknown
            };
            Some(McpServer {
                name: name.to_owned(),
                target: mask::mask_text(target),
                status: status.to_owned(),
                health,
                scope: None,
                details: None,
            })
        })
        .collect()
}

/// `  Key: value` lines, and `Environment:` / `Headers:` blocks of more indented `KEY=value` or `Key: value`.
pub fn parse_mcp_get(text: &str) -> (Map<String, Value>, Option<String>) {
    let mut details = Map::new();
    let mut block: Option<String> = None;
    let mut scope = None;
    for line in text.lines() {
        if let Some(hint) = line.trim().strip_prefix("To remove this server, run:") {
            scope = hint
                .split_once(" -s ")
                .map(|(_, s)| s.split_whitespace().next().unwrap_or_default().to_owned());
            continue;
        }
        let depth = line.len() - line.trim_start().len();
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if depth >= 4
            && let Some(name) = &block
        {
            let (k, v) = line
                .split_once('=')
                .or_else(|| line.split_once(": "))
                .unwrap_or((line, ""));
            if let Some(Value::Object(map)) = details.get_mut(name) {
                map.insert(k.trim().to_owned(), v.trim().into());
            }
            continue;
        }
        if depth == 0 {
            continue;
        }
        match line.split_once(':') {
            Some((key, value)) if value.trim().is_empty() => {
                details.insert(key.to_owned(), Value::Object(Map::new()));
                block = Some(key.to_owned());
            }
            Some((key, value)) => {
                details.insert(key.to_owned(), value.trim().into());
                block = None;
            }
            None => {}
        }
    }
    (details, scope)
}

pub fn mcp(env: &Env) -> Mcp {
    let out = match claude(env, &["mcp", "list"], SLOW) {
        Ok(out) if out.ok => out,
        Ok(out) => {
            return Mcp {
                servers: Vec::new(),
                error: Some(failure(&out)),
            };
        }
        Err(e) => {
            return Mcp {
                servers: Vec::new(),
                error: Some(format!("{e:#}")),
            };
        }
    };
    let mut servers = parse_mcp_list(&out.stdout);
    // `claude mcp get` checks the server again: a few at a time.
    for chunk in servers.chunks_mut(4) {
        std::thread::scope(|s| {
            for server in chunk {
                s.spawn(|| {
                    if let Ok(out) = claude(env, &["mcp", "get", &server.name], SLOW)
                        && out.ok
                    {
                        let (details, scope) = parse_mcp_get(&out.stdout);
                        let mut details = Value::Object(details);
                        mask::mask(&mut details);
                        server.details = Some(details);
                        server.scope = scope;
                    }
                });
            }
        });
    }
    Mcp {
        servers,
        error: None,
    }
}

#[derive(Serialize, Debug, Default)]
pub struct Plugins {
    /// `claude plugin list --json`, masked.
    pub plugins: Vec<Value>,
    /// `claude plugin marketplace list --json`, masked.
    pub marketplaces: Vec<Value>,
    pub error: Option<String>,
}

fn json_list(env: &Env, args: &[&str]) -> Result<Vec<Value>> {
    let out = claude(env, args, QUICK)?;
    if !out.ok {
        bail!("{}", failure(&out));
    }
    let mut list: Vec<Value> = serde_json::from_str(out.stdout.trim())?;
    list.iter_mut().for_each(mask::mask);
    Ok(list)
}

pub fn plugins(env: &Env) -> Plugins {
    let (plugins, marketplaces) = std::thread::scope(|s| {
        let plugins = s.spawn(|| json_list(env, &["plugin", "list", "--json"]));
        let markets = json_list(env, &["plugin", "marketplace", "list", "--json"]);
        (
            plugins.join().unwrap_or_else(|_| bail!("panicked")),
            markets,
        )
    });
    let error = [&plugins, &marketplaces]
        .iter()
        .find_map(|r| r.as_ref().err().map(|e| format!("{e:#}")));
    Plugins {
        plugins: plugins.unwrap_or_default(),
        marketplaces: marketplaces.unwrap_or_default(),
        error,
    }
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// `claude auth status` (JSON), masked.
    pub auth: Option<Value>,
    pub auth_error: Option<String>,
    pub version: Option<String>,
    /// `native`, `npm`, `homebrew` or `other`.
    pub install: Option<String>,
    pub path: Option<String>,
}

/// `claude auth status` exits with 1 when logged out, and still prints its JSON.
pub fn parse_auth(stdout: &str) -> Option<Value> {
    let mut value: Value = serde_json::from_str(stdout.trim()).ok()?;
    if !value.is_object() {
        return None;
    }
    mask::mask(&mut value);
    Some(value)
}

pub fn install_method(path: &Path) -> &'static str {
    let path = fs_canonical(path);
    let text = path.to_string_lossy().replace('\\', "/");
    if text.contains("/node_modules/") {
        "npm"
    } else if text.contains("/Caskroom/")
        || text.contains("/homebrew/")
        || text.contains("/Cellar/")
    {
        "homebrew"
    } else if text.contains("/.local/share/claude/") || text.contains("/.claude/local/") {
        "native"
    } else {
        "other"
    }
}

fn fs_canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn account(env: &Env) -> Account {
    let (auth, version) = std::thread::scope(|s| {
        let auth = s.spawn(|| claude(env, &["auth", "status"], QUICK));
        let version = claude(env, &["--version"], QUICK);
        (auth.join().unwrap_or_else(|_| bail!("panicked")), version)
    });
    let (auth, auth_error) = match auth {
        Ok(out) => match parse_auth(&out.stdout) {
            Some(v) => (Some(v), None),
            None => (None, Some(failure(&out))),
        },
        Err(e) => (None, Some(format!("{e:#}"))),
    };
    let path = which(env, &env.claude);
    Account {
        auth,
        auth_error,
        version: version
            .ok()
            .filter(|o| o.ok)
            .and_then(|o| first_version(&o.stdout)),
        install: path.as_deref().map(|p| install_method(p).to_owned()),
        path: path.map(|p| p.to_string_lossy().into_owned()),
    }
}

/// The first word that looks like a version in the first line: `gh version 2.83.0 (2026-01-01)` → `2.83.0`.
pub fn first_version(text: &str) -> Option<String> {
    text.lines().next()?.split_whitespace().find_map(|w| {
        let v = w.strip_prefix('v').unwrap_or(w);
        v.starts_with(|c: char| c.is_ascii_digit())
            .then(|| v.trim_end_matches([',', ';']).to_owned())
    })
}

/// The program `PATH` resolves, the way the commands above find it.
pub fn which(env: &Env, program: &str) -> Option<PathBuf> {
    if Path::new(program).components().count() > 1 {
        return Some(PathBuf::from(program)).filter(|p| p.is_file());
    }
    let path = env
        .path
        .clone()
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter()
            .map(|ext| dir.join(format!("{program}{ext}")))
            .find(|p| p.is_file())
    })
}

#[derive(Serialize, Debug)]
pub struct Tool {
    pub name: String,
    pub version: Option<String>,
    pub path: Option<String>,
}

/// The tools Claude Code's config leans on: GitHub CLI, git, Node and its package managers.
pub const TOOLS: &[&str] = &["gh", "git", "node", "npm", "pnpm", "yarn", "bun"];

pub fn tools(env: &Env) -> Vec<Tool> {
    std::thread::scope(|s| {
        let handles: Vec<_> = TOOLS
            .iter()
            .map(|name| {
                s.spawn(move || {
                    let path = which(env, name);
                    let version = path.as_ref().and_then(|p| {
                        let out = run(env, &p.to_string_lossy(), &["--version"], QUICK).ok()?;
                        first_version(&out.stdout)
                    });
                    Tool {
                        name: (*name).to_owned(),
                        version,
                        path: path.map(|p| p.to_string_lossy().into_owned()),
                    }
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}
