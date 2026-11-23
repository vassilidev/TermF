//! The deployments of a project through the deploy tools' own CLIs, never their SDKs: Laravel Forge (`forge`),
//! Vercel (`vercel`), Netlify (`netlify`), Fly.io (`fly`) and Railway (`railway`). A project's targets live in its
//! file (`[[deploy.targets]]`), their token as the name of a vault secret, which the caller resolves and hands in;
//! without one, the CLI's own sign-in serves.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

const LIST_TIMEOUT: Duration = Duration::from_secs(60);
/// `forge deploy` waits for the deployment to end.
const DEPLOY_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// Lines of a log kept, from its end.
const LOG_LINES: usize = 5000;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Forge,
    Vercel,
    Netlify,
    Fly,
    Railway,
}

impl Tool {
    pub const ALL: [Tool; 5] = [
        Tool::Forge,
        Tool::Vercel,
        Tool::Netlify,
        Tool::Fly,
        Tool::Railway,
    ];

    /// The CLI's names, the first one found wins.
    fn programs(self) -> &'static [&'static str] {
        match self {
            Tool::Forge => &["forge"],
            Tool::Vercel => &["vercel"],
            Tool::Netlify => &["netlify"],
            Tool::Fly => &["fly", "flyctl"],
            Tool::Railway => &["railway"],
        }
    }

    /// Where the CLI reads a token from, so that none has to be saved by its `login`.
    pub fn token_var(self) -> &'static str {
        match self {
            Tool::Forge => "FORGE_API_TOKEN",
            Tool::Vercel => "VERCEL_TOKEN",
            Tool::Netlify => "NETLIFY_AUTH_TOKEN",
            Tool::Fly => "FLY_API_TOKEN",
            Tool::Railway => "RAILWAY_API_TOKEN",
        }
    }

    pub fn install(self) -> &'static str {
        match self {
            Tool::Forge => "composer global require laravel/forge-cli",
            Tool::Vercel => "npm i -g vercel",
            Tool::Netlify => "npm install -g netlify-cli",
            Tool::Fly => "brew install flyctl",
            Tool::Railway => "npm i -g @railway/cli",
        }
    }
}

/// Where a project deploys, as `[[deploy.targets]]` keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Its name in TermF: `production`, `preview`…
    pub name: String,
    pub tool: Tool,
    /// The Vercel project, Netlify site id, Fly app, Railway project or Forge site.
    pub target: String,
    /// The Vercel team, Railway environment or Forge server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// The Railway service.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// The repository it deploys, by path: its deployments are matched with that repository's runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The vault's secret that holds the CLI's token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The vault's scope of that secret: `dev`, `staging` or `prod`.
    #[serde(default = "dev")]
    pub env: String,
}

fn dev() -> String {
    "dev".into()
}

/// A deployment as each tool tells it, in the same shape.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Deployment {
    pub id: String,
    /// `running`, `queued`, `passed`, `failed`, `stopped`, or `unknown` when the tool does not say (Forge).
    pub state: String,
    /// The tool's own word: `READY`, `error`, `complete`, `SUCCESS`…
    pub status: String,
    pub url: Option<String>,
    /// `production`, `preview`, `deploy-preview`…
    pub environment: Option<String>,
    pub sha: Option<String>,
    pub branch: Option<String>,
    pub pr: Option<u64>,
    /// ISO 8601.
    pub created: Option<String>,
    pub by: Option<String>,
    pub description: Option<String>,
    /// Fly: the image a redeploy takes again.
    #[serde(skip)]
    pub image: Option<String>,
}

/// A deploy tool's CLI on this machine.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub tool: Tool,
    /// Where it is; none when it is not installed.
    pub path: Option<String>,
    pub install: String,
    pub token_var: String,
}

#[derive(Deserialize, Default)]
struct ProjectDeploy {
    #[serde(default)]
    deploy: Section,
}

#[derive(Deserialize, Default)]
struct Section {
    #[serde(default)]
    targets: Vec<Target>,
}

pub fn targets(config_dir: &Path, project: &str) -> Result<Vec<Target>> {
    let file = termf_core::project_file(config_dir, project)?;
    let text = fs::read_to_string(&file)?;
    let found: ProjectDeploy =
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
    Ok(found.deploy.targets)
}

/// Adds a target, or replaces the one named `previous`. Comments and the rest of the file stay as they are.
pub fn save_target(
    config_dir: &Path,
    project: &str,
    target: &Target,
    previous: Option<&str>,
) -> Result<Vec<Target>> {
    check(target)?;
    let taken = targets(config_dir, project)?
        .iter()
        .any(|t| t.name == target.name && Some(t.name.as_str()) != previous);
    ensure!(
        !taken,
        "the project already deploys to a target named {}",
        target.name
    );
    edit(config_dir, project, |list| {
        let mut table: Table = toml::to_string(target)?
            .parse::<DocumentMut>()?
            .as_table()
            .clone();
        table.set_implicit(false);
        match previous.and_then(|p| list.iter().position(|t| named(t, p))) {
            Some(at) => *list.get_mut(at).context("no such target")? = table,
            None if previous.is_some() => bail!("no target named {}", previous.unwrap_or_default()),
            None => list.push(table),
        }
        Ok(())
    })?;
    targets(config_dir, project)
}

pub fn delete_target(config_dir: &Path, project: &str, name: &str) -> Result<Vec<Target>> {
    edit(config_dir, project, |list| {
        let before = list.len();
        list.retain(|t| !named(t, name));
        ensure!(list.len() < before, "no target named {name}");
        Ok(())
    })?;
    targets(config_dir, project)
}

fn named(table: &Table, name: &str) -> bool {
    table.get("name").and_then(Item::as_str) == Some(name)
}

fn edit(
    config_dir: &Path,
    project: &str,
    change: impl FnOnce(&mut ArrayOfTables) -> Result<()>,
) -> Result<()> {
    let file = termf_core::project_file(config_dir, project)?;
    let mut doc: DocumentMut = fs::read_to_string(&file)?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    let deploy = doc
        .entry("deploy")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .with_context(|| format!("`deploy` in {} must be a table", file.display()))?;
    deploy.set_implicit(true);
    let list = deploy
        .entry("targets")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .with_context(|| {
            format!(
                "`deploy.targets` in {} must be [[deploy.targets]]",
                file.display()
            )
        })?;
    change(list)?;
    termf_core::write_atomic(&file, &doc.to_string())
}

/// What goes on a command line: one word, never an option.
fn word(what: &str, value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 256
            && !value.starts_with('-')
            && !value.chars().any(|c| c.is_whitespace() || c.is_control()),
        "{what} must be one word, without a leading -: {value:?}"
    );
    Ok(())
}

pub fn check(target: &Target) -> Result<()> {
    ensure!(
        !target.name.trim().is_empty() && target.name.len() <= 64,
        "name the target"
    );
    word("the target", &target.target)?;
    for value in [&target.scope, &target.service].into_iter().flatten() {
        word("each field", value)?;
    }
    if let Some(token) = &target.token {
        word("the token's secret", token)?;
    }
    ensure!(
        ["dev", "staging", "prod"].contains(&target.env.as_str()),
        "the token's scope is dev, staging or prod"
    );
    Ok(())
}

/// How TermF runs the tools: the `PATH` it looks in (the login shell's by default).
#[derive(Default)]
pub struct Deploy {
    pub path: Option<OsString>,
}

/// Tells a deployment's state from the tool's word for it.
fn state(tool: Tool, status: &str, running: bool) -> &'static str {
    let s = status.to_ascii_lowercase();
    let s = s.as_str();
    if running {
        return "running";
    }
    match tool {
        Tool::Vercel => match s {
            "ready" => "passed",
            "error" | "blocked" => "failed",
            "building" | "initializing" => "running",
            "queued" => "queued",
            "canceled" => "stopped",
            _ => "unknown",
        },
        Tool::Netlify => match s {
            "ready" => "passed",
            "error" => "failed",
            "building" | "processing" | "uploading" | "uploaded" | "preparing" | "prepared"
            | "retrying" => "running",
            "new" | "enqueued" | "pending_review" => "queued",
            "skipped" | "canceled" | "cancelled" | "rejected" => "stopped",
            _ => "unknown",
        },
        Tool::Fly => match s {
            "complete" | "succeeded" | "successful" => "passed",
            "failed" | "interrupted" => "failed",
            "running" | "in_progress" => "running",
            "pending" => "queued",
            "cancelled" | "canceled" => "stopped",
            _ => "unknown",
        },
        Tool::Railway => match s {
            "success" => "passed",
            "failed" | "crashed" => "failed",
            "building" | "deploying" | "initializing" => "running",
            "queued" | "waiting" => "queued",
            "removed" | "removing" | "skipped" | "sleeping" => "stopped",
            _ => "unknown",
        },
        Tool::Forge => "unknown",
    }
}

fn text(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

/// The JSON a CLI printed, past any line of chatter before it.
fn json(out: &str) -> Result<Value> {
    let at = out.find(['{', '[']).context("the CLI printed no JSON")?;
    serde_json::from_str(&out[at..]).context("the CLI printed no JSON")
}

/// The deployments a tool's listing printed, newest first.
pub fn parse_list(tool: Tool, out: &str) -> Result<Vec<Deployment>> {
    if tool == Tool::Forge {
        return Ok(vec![Deployment {
            id: "latest".into(),
            state: "unknown".into(),
            ..Deployment::default()
        }]);
    }
    let found = json(out)?;
    let items = match tool {
        Tool::Vercel => found["deployments"].as_array(),
        _ => found.as_array(),
    };
    let items = items.into_iter().flatten();
    Ok(items
        .map(|d| match tool {
            Tool::Vercel => {
                let status = text(&d["state"])
                    .or_else(|| text(&d["readyState"]))
                    .unwrap_or_default();
                Deployment {
                    id: text(&d["id"])
                        .or_else(|| text(&d["uid"]))
                        .unwrap_or_default(),
                    state: state(tool, &status, false).into(),
                    url: text(&d["url"]).map(|u| {
                        if u.contains("://") {
                            u
                        } else {
                            format!("https://{u}")
                        }
                    }),
                    environment: Some(text(&d["target"]).unwrap_or_else(|| "preview".into())),
                    sha: text(&d["meta"]["githubCommitSha"]),
                    branch: text(&d["meta"]["githubCommitRef"]),
                    pr: d["meta"]["githubPrId"]
                        .as_u64()
                        .or_else(|| d["meta"]["githubPrId"].as_str()?.parse().ok()),
                    created: d["createdAt"]
                        .as_i64()
                        .and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
                        .map(|t| t.to_string()),
                    by: text(&d["creator"]["username"]),
                    description: text(&d["meta"]["githubCommitMessage"]),
                    status,
                    image: None,
                }
            }
            Tool::Netlify => {
                let status = text(&d["state"]).unwrap_or_default();
                Deployment {
                    id: text(&d["id"]).unwrap_or_default(),
                    state: state(tool, &status, false).into(),
                    url: text(&d["deploy_ssl_url"]).or_else(|| text(&d["ssl_url"])),
                    environment: text(&d["context"]),
                    sha: text(&d["commit_ref"]),
                    branch: text(&d["branch"]),
                    pr: d["review_id"].as_u64(),
                    created: text(&d["created_at"]),
                    by: text(&d["committer"]),
                    description: text(&d["title"]).or_else(|| text(&d["error_message"])),
                    status,
                    image: None,
                }
            }
            Tool::Fly => {
                let status = text(&d["Status"]).unwrap_or_default();
                let version = d["Version"].as_u64().map(|v| format!("v{v}"));
                let said = text(&d["Description"]);
                Deployment {
                    id: text(&d["ID"]).unwrap_or_default(),
                    state: state(tool, &status, d["InProgress"].as_bool() == Some(true)).into(),
                    created: text(&d["CreatedAt"]),
                    by: text(&d["User"]["Email"]).or_else(|| text(&d["User"]["Name"])),
                    description: match (version, said) {
                        (Some(v), Some(s)) => Some(format!("{v} · {s}")),
                        (v, s) => v.or(s),
                    },
                    image: text(&d["ImageRef"]),
                    status,
                    ..Deployment::default()
                }
            }
            Tool::Railway | Tool::Forge => {
                let status = text(&d["status"]).unwrap_or_default();
                let meta = &d["meta"];
                Deployment {
                    id: text(&d["id"]).unwrap_or_default(),
                    state: state(tool, &status, false).into(),
                    url: text(&d["staticUrl"]).map(|u| {
                        if u.contains("://") {
                            u
                        } else {
                            format!("https://{u}")
                        }
                    }),
                    sha: text(&meta["commitHash"]),
                    branch: text(&meta["branch"]),
                    pr: meta["prNumber"].as_u64(),
                    created: text(&d["createdAt"]),
                    by: text(&meta["commitAuthor"]),
                    description: text(&meta["commitMessage"]),
                    status,
                    ..Deployment::default()
                }
            }
        })
        .filter(|d| !d.id.is_empty())
        .collect())
}

/// What a deployment is about to the CLI: the arguments of a listing, a log or a redeploy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action<'a> {
    List,
    Logs(&'a str),
    /// The deployment, and for Fly the image it ran.
    Redeploy(&'a str, Option<&'a str>),
}

/// The arguments the tool takes for `action`, and the command to run before it (Forge selects its server first).
pub fn args(target: &Target, action: Action) -> Result<(Vec<String>, Option<Vec<String>>)> {
    let t = target.target.clone();
    let opt = |flag: &str, value: &Option<String>| {
        value
            .iter()
            .flat_map(|v| [flag.to_owned(), v.clone()])
            .collect::<Vec<_>>()
    };
    let words = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    if let Action::Logs(id) | Action::Redeploy(id, _) = action {
        word("the deployment", id)?;
    }
    // Forge's server is the CLI's global state: switched only to act, never to list (which runs nothing).
    let before = (target.tool == Tool::Forge && action != Action::List)
        .then(|| {
            target
                .scope
                .as_ref()
                .map(|s| vec!["server:switch".into(), s.clone()])
        })
        .flatten();
    let site = |data: &str| format!("{{\"site_id\":{}{data}}}", Value::String(t.clone()));
    let args = match (target.tool, action) {
        (Tool::Forge, Action::List) => Vec::new(),
        (Tool::Forge, Action::Logs(_)) => vec!["deploy:logs".into(), t],
        (Tool::Forge, Action::Redeploy(..)) => vec!["deploy".into(), t],
        (Tool::Vercel, Action::List) => [
            words(&["list", &t, "--json", "--limit", "20"]),
            opt("--scope", &target.scope),
        ]
        .concat(),
        (Tool::Vercel, Action::Logs(id)) => [
            words(&["inspect", id, "--logs"]),
            opt("--scope", &target.scope),
        ]
        .concat(),
        (Tool::Vercel, Action::Redeploy(id, _)) => [
            words(&["redeploy", id, "--no-wait"]),
            opt("--scope", &target.scope),
        ]
        .concat(),
        (Tool::Netlify, Action::List) => words(&[
            "api",
            "listSiteDeploys",
            "--data",
            &site(",\"per_page\":20"),
        ]),
        (Tool::Netlify, Action::Logs(id)) => words(&[
            "logs",
            "--source",
            "deploy",
            "--deploy-id",
            id,
            "--since",
            "24h",
        ]),
        (Tool::Netlify, Action::Redeploy(..)) => {
            words(&["api", "createSiteBuild", "--data", &site("")])
        }
        (Tool::Fly, Action::List) => words(&["releases", "-a", &t, "--json"]),
        (Tool::Fly, Action::Logs(_)) => words(&["logs", "-a", &t, "--no-tail"]),
        (Tool::Fly, Action::Redeploy(_, image)) => {
            let image =
                image.context("Fly gave no image for this release: deploy it from its folder")?;
            word("the image", image)?;
            words(&["deploy", "-a", &t, "--image", image, "--yes", "--detach"])
        }
        (Tool::Railway, action) => {
            let at = [
                opt("-p", &Some(t)),
                opt("-s", &target.service),
                opt("-e", &target.scope),
            ]
            .concat();
            match action {
                Action::List => [
                    words(&["deployment", "list"]),
                    at,
                    words(&["--limit", "20", "--json"]),
                ]
                .concat(),
                Action::Logs(id) => [words(&["logs", id, "--build", "-n", "500"]), at].concat(),
                Action::Redeploy(..) => [words(&["redeploy"]), at, words(&["--yes"])].concat(),
            }
        }
    };
    Ok((args, before))
}

impl Deploy {
    fn path(&self) -> Option<OsString> {
        self.path
            .clone()
            .or_else(|| termf_core::shell::login_path().map(OsString::from))
            .or_else(|| std::env::var_os("PATH"))
    }

    pub fn find(&self, tool: Tool) -> Option<PathBuf> {
        let exts: &[&str] = if cfg!(windows) {
            &[".exe", ".cmd", ".bat", ""]
        } else {
            &[""]
        };
        let path = self.path()?;
        tool.programs().iter().find_map(|program| {
            std::env::split_paths(&path).find_map(|dir| {
                exts.iter()
                    .map(|ext| dir.join(format!("{program}{ext}")))
                    .find(|p| p.is_file())
            })
        })
    }

    pub fn tools(&self) -> Vec<ToolInfo> {
        Tool::ALL
            .iter()
            .map(|&tool| ToolInfo {
                tool,
                path: self.find(tool).map(|p| p.to_string_lossy().into_owned()),
                install: tool.install().into(),
                token_var: tool.token_var().into(),
            })
            .collect()
    }

    /// Runs the tool with `args`, its token in its variable when there is one; what it printed, or why it failed.
    fn run(
        &self,
        target: &Target,
        args: &[String],
        token: Option<&str>,
        timeout: Duration,
    ) -> Result<String> {
        let tool = target.tool;
        let program = self.find(tool).ok_or_else(|| {
            anyhow!(
                "{} is not installed or not on the PATH: install it with `{}`",
                tool.programs()[0],
                tool.install()
            )
        })?;
        let mut cmd = Command::new(&program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        if let Some(path) = self.path() {
            cmd.env("PATH", path);
        }
        if let Some(token) = token {
            cmd.env(tool.token_var(), token);
        }
        // Netlify's logs know their site from it.
        if tool == Tool::Netlify {
            cmd.env("NETLIFY_SITE_ID", &target.target);
        }
        cmd.args(args)
            .env("CI", "1")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null());
        let (status, out, err) = termf_core::process::run(cmd, b"", timeout)?;
        let (out, err) = (
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err),
        );
        if !status.success() {
            let said = termf_core::process::tail(if err.trim().is_empty() { &out } else { &err });
            bail!(
                "{} {} failed: {}",
                tool.programs()[0],
                args.join(" "),
                said.trim()
            );
        }
        Ok(out)
    }

    fn with(
        &self,
        target: &Target,
        action: Action,
        token: Option<&str>,
        timeout: Duration,
    ) -> Result<String> {
        let (args, before) = args(target, action)?;
        if args.is_empty() {
            return Ok(String::new());
        }
        if let Some(before) = before {
            self.run(target, &before, token, LIST_TIMEOUT)?;
        }
        self.run(target, &args, token, timeout)
    }

    /// The target's recent deployments, newest first. Forge's CLI tells only of its latest one, without its state.
    pub fn list(&self, target: &Target, token: Option<&str>) -> Result<Vec<Deployment>> {
        parse_list(
            target.tool,
            &self.with(target, Action::List, token, LIST_TIMEOUT)?,
        )
    }

    /// A deployment's log, its last lines. Fly has none per release: its app's recent logs stand in.
    pub fn logs(&self, target: &Target, id: &str, token: Option<&str>) -> Result<Vec<String>> {
        let out = self.with(target, Action::Logs(id), token, LIST_TIMEOUT)?;
        let lines: Vec<String> = out.lines().map(str::to_owned).collect();
        Ok(lines[lines.len().saturating_sub(LOG_LINES)..].to_vec())
    }

    /// Deploys again: the given deployment where the tool can (Vercel, Fly), else the target's latest.
    pub fn redeploy(&self, target: &Target, id: &str, token: Option<&str>) -> Result<String> {
        let image = if target.tool == Tool::Fly {
            let found = self.list(target, token)?.into_iter().find(|d| d.id == id);
            Some(
                found
                    .and_then(|d| d.image)
                    .context("Fly no longer lists this release")?,
            )
        } else {
            None
        };
        let out = self.with(
            target,
            Action::Redeploy(id, image.as_deref()),
            token,
            DEPLOY_TIMEOUT,
        )?;
        Ok(termf_core::process::tail(&out).trim().to_owned())
    }
}
