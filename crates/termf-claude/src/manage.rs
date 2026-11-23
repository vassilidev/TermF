//! Changing MCP servers and plugins, only through Claude Code's own commands (`claude mcp …`, `claude plugin …`):
//! TermF never edits `~/.claude.json` nor `installed_plugins.json`. A repository's `.mcp.json` is the one file it
//! writes itself, after showing the diff.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::cli::{claude_with, failure};
use crate::settings::{self, Preview};
use crate::{Env, mask};

const QUICK: Duration = Duration::from_secs(30);
/// Cloning a marketplace or fetching a plugin.
const FETCH: Duration = Duration::from_secs(180);
/// `claude mcp login` waits while you sign in in the browser.
const SIGN_IN: Duration = Duration::from_secs(300);

#[derive(Deserialize, Debug)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum McpAction {
    /// `server`: `{type: "stdio", command, args?, env?}` or `{type: "http" | "sse", url, headers?}`.
    Add {
        name: String,
        scope: String,
        server: Value,
    },
    /// Without `scope`, from whichever scope has it.
    Remove {
        name: String,
        scope: Option<String>,
    },
    Login {
        name: String,
    },
    Logout {
        name: String,
    },
    /// Forgets which `.mcp.json` servers you approved or rejected in the repository.
    Reset,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum PluginAction {
    /// `accept`: the sha256 of a command the marketplace runs to install it, once you saw it.
    Install {
        plugin: String,
        scope: String,
        accept: Option<String>,
    },
    Uninstall {
        plugin: String,
        scope: String,
    },
    Enable {
        plugin: String,
        scope: Option<String>,
    },
    Disable {
        plugin: String,
        scope: Option<String>,
    },
    Update {
        plugin: String,
        scope: Option<String>,
        accept: Option<String>,
    },
    MarketplaceAdd {
        source: String,
        scope: String,
    },
    MarketplaceRemove {
        name: String,
        scope: Option<String>,
    },
    MarketplaceUpdate {
        name: String,
    },
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    /// What Claude Code said, masked.
    pub message: String,
    /// A command the marketplace runs to install or update the plugin (`shownCommand`), which Claude Code
    /// runs only once accepted by its `sha256`: shown as is, since you decide on exactly this text.
    pub shown: Option<Value>,
}

/// A value handed to `claude` as an argument: never read as one of its options.
fn arg<'a>(value: &'a str, what: &str) -> Result<&'a str> {
    ensure!(
        !value.trim().is_empty() && !value.starts_with('-') && !value.contains(char::is_control),
        "{what} cannot be empty, start with - or hold control characters"
    );
    Ok(value)
}

fn scope<'a>(value: &'a str, allowed: &[&str]) -> Result<&'a str> {
    ensure!(
        allowed.contains(&value),
        "the scope is one of {}",
        allowed.join(", ")
    );
    Ok(value)
}

const SCOPES: &[&str] = &["user", "project", "local"];

fn needs_repo(env: &Env, scope: &str) -> Result<()> {
    ensure!(
        scope == "user" || env.repo.as_ref().is_some_and(|r| r.is_dir()),
        "choose a repository for the {scope} scope"
    );
    Ok(())
}

/// What `claude` printed, or its error as it gave it.
fn outcome(env: &Env, args: &[&str], input: &[u8], timeout: Duration) -> Result<String> {
    let out = claude_with(env, args, input, timeout).map_err(|e| {
        if e.to_string().contains("timed out") {
            e.context(format!("claude {} did not finish", args[..2].join(" ")))
        } else {
            e
        }
    })?;
    if !out.ok {
        bail!("{}", failure(&out));
    }
    Ok(mask::mask_text(out.stdout.trim()))
}

/// A server as `claude mcp add-json` takes it, checked before Claude Code sees it.
pub fn check_server(name: &str, server: &Value) -> Result<()> {
    let s = server
        .as_object()
        .with_context(|| format!("{name}: a server is a JSON object"))?;
    let strings = |key: &str| -> Result<()> {
        match s.get(key) {
            None => Ok(()),
            Some(Value::Object(map)) if map.values().all(Value::is_string) => Ok(()),
            Some(_) => bail!("{name}: {key} maps names to text values"),
        }
    };
    let kind = s
        .get("type")
        .map(|t| t.as_str().unwrap_or_default())
        .unwrap_or(if s.contains_key("url") { "" } else { "stdio" });
    match kind {
        "stdio" => {
            ensure!(
                s.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| !c.trim().is_empty()),
                "{name}: a stdio server needs a command"
            );
            ensure!(
                s.get("args")
                    .is_none_or(|a| a.as_array().is_some_and(|a| a.iter().all(Value::is_string))),
                "{name}: args is a list of text"
            );
            strings("env")
        }
        "http" | "sse" | "streamable-http" | "ws" => {
            ensure!(
                s.get("url").and_then(Value::as_str).is_some_and(|u| {
                    ["http://", "https://", "ws://", "wss://", "${"]
                        .iter()
                        .any(|p| u.starts_with(p))
                }),
                "{name}: a {kind} server needs an http(s) URL"
            );
            strings("headers")
        }
        _ => bail!("{name}: the transport is stdio, http or sse"),
    }
}

fn no_secret_shared(name: &str, server: &Value) -> Result<()> {
    ensure!(
        !settings::secret(name, server),
        "{name} holds a secret, and .mcp.json is shared with everyone who clones the repository: use the \
         local or user scope, or ${{VARIABLE}} in its place"
    );
    Ok(())
}

pub fn mcp(env: &Env, action: &McpAction) -> Result<String> {
    match action {
        McpAction::Add {
            name,
            scope: s,
            server,
        } => {
            let name = arg(name, "the server name")?;
            let s = scope(s, SCOPES)?;
            needs_repo(env, s)?;
            check_server(name, server)?;
            ensure!(
                !server.to_string().contains(mask::MASK),
                "{name} still holds a hidden value: type it again"
            );
            if s == "project" {
                no_secret_shared(name, server)?;
            }
            // On stdin: the variables and headers never show in the process list.
            let json = server.to_string();
            outcome(
                env,
                &["mcp", "add-json", "--scope", s, name, "-"],
                json.as_bytes(),
                QUICK,
            )
        }
        McpAction::Remove { name, scope: s } => {
            let name = arg(name, "the server name")?;
            let mut args = vec!["mcp", "remove", name];
            if let Some(s) = s {
                args.extend(["--scope", scope(s, SCOPES)?]);
            }
            outcome(env, &args, b"", QUICK)
        }
        McpAction::Login { name } => outcome(
            env,
            &["mcp", "login", arg(name, "the server name")?],
            b"",
            SIGN_IN,
        ),
        McpAction::Logout { name } => outcome(
            env,
            &["mcp", "logout", arg(name, "the server name")?],
            b"",
            QUICK,
        ),
        McpAction::Reset => {
            needs_repo(env, "project")?;
            outcome(env, &["mcp", "reset-project-choices"], b"", QUICK)
        }
    }
}

fn sha(value: &str) -> Result<&str> {
    ensure!(
        value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()),
        "an accepted command is named by its sha256"
    );
    Ok(value)
}

pub fn plugin(env: &Env, action: &PluginAction) -> Result<Done> {
    use PluginAction::*;
    let mut args: Vec<&str> = vec!["plugin"];
    let (timeout, scope_needs_repo) = match action {
        Install {
            plugin,
            scope: s,
            accept,
        } => {
            args.extend([
                "install",
                "--json",
                "--scope",
                scope(s, SCOPES)?,
                arg(plugin, "the plugin")?,
            ]);
            if let Some(a) = accept {
                args.extend(["--accept-command", sha(a)?]);
            }
            (FETCH, Some(s))
        }
        Uninstall { plugin, scope: s } => {
            args.extend([
                "uninstall",
                "--json",
                "--scope",
                scope(s, SCOPES)?,
                arg(plugin, "the plugin")?,
            ]);
            (QUICK, Some(s))
        }
        Enable { plugin, scope: s } | Disable { plugin, scope: s } => {
            args.extend([
                if matches!(action, Enable { .. }) {
                    "enable"
                } else {
                    "disable"
                },
                "--json",
                arg(plugin, "the plugin")?,
            ]);
            if let Some(s) = s {
                args.extend(["--scope", scope(s, SCOPES)?]);
            }
            (QUICK, s.as_ref())
        }
        Update {
            plugin,
            scope: s,
            accept,
        } => {
            args.extend(["update", "--json", arg(plugin, "the plugin")?]);
            if let Some(s) = s {
                args.extend([
                    "--scope",
                    scope(s, &["user", "project", "local", "managed"])?,
                ]);
            }
            if let Some(a) = accept {
                args.extend(["--accept-command", sha(a)?]);
            }
            (FETCH, None)
        }
        MarketplaceAdd { source, scope: s } => {
            args.extend([
                "marketplace",
                "add",
                "--json",
                "--scope",
                scope(s, SCOPES)?,
                arg(source, "the source")?,
            ]);
            (FETCH, Some(s))
        }
        MarketplaceRemove { name, scope: s } => {
            args.extend([
                "marketplace",
                "remove",
                "--json",
                arg(name, "the marketplace")?,
            ]);
            if let Some(s) = s {
                args.extend(["--scope", scope(s, SCOPES)?]);
            }
            (QUICK, s.as_ref())
        }
        MarketplaceUpdate { name } => {
            args.extend([
                "marketplace",
                "update",
                "--json",
                arg(name, "the marketplace")?,
            ]);
            (FETCH, None)
        }
    };
    if let Some(s) = scope_needs_repo {
        needs_repo(env, s)?;
    }
    let out = claude_with(env, &args, b"", timeout)?;
    result(out.ok, &out.stdout).unwrap_or_else(|| {
        if out.ok {
            Ok(Done {
                message: mask::mask_text(out.stdout.trim()),
                shown: None,
            })
        } else {
            bail!("{}", failure(&out))
        }
    })
}

/// The JSON result line `--json` prints last: `{command, outcome, message, shownCommand?, alreadyInGoalState?}`.
pub fn result(ok: bool, stdout: &str) -> Option<Result<Done>> {
    let line = stdout.lines().rev().find(|l| !l.trim().is_empty())?;
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let message = mask::mask_text(value["message"].as_str().unwrap_or_default());
    let shown = value.get("shownCommand").filter(|s| s.is_object()).cloned();
    Some(match value["outcome"].as_str() {
        Some("ok") if ok => Ok(Done {
            message,
            shown: None,
        }),
        _ if shown.is_some() => Ok(Done { message, shown }),
        _ if value["alreadyInGoalState"] == true => Ok(Done {
            message,
            shown: None,
        }),
        _ => Err(anyhow::anyhow!(if message.is_empty() {
            "claude did not succeed".to_owned()
        } else {
            message
        })),
    })
}

pub fn mcp_json_path(env: &Env) -> Result<PathBuf> {
    Ok(env
        .repo
        .as_ref()
        .context("choose a repository: .mcp.json is a file of the repository")?
        .join(".mcp.json"))
}

/// The repository's `.mcp.json` as it is: a file of the repository, opened because you asked to edit it.
pub fn mcp_json(env: &Env) -> Result<(PathBuf, String)> {
    let path = mcp_json_path(env)?;
    let text = settings::read_text(&path)?;
    Ok((path, text))
}

/// What saving `text` as `.mcp.json` would change; it gets this text exactly.
pub fn mcp_json_preview(env: &Env, text: &str) -> Result<Preview> {
    let (path, before) = mcp_json(env)?;
    let value: Value = serde_json::from_str(text).context(".mcp.json is not valid JSON")?;
    let root = value.as_object().context(".mcp.json is a JSON object")?;
    let old: Value = serde_json::from_str(&before).unwrap_or_default();
    let servers = match root.get("mcpServers") {
        None => &Map::new(),
        Some(Value::Object(map)) => map,
        Some(_) => bail!("mcpServers maps server names to servers"),
    };
    for (name, server) in servers {
        check_server(name, server)?;
        // A secret already there is yours to move out; a new one is refused.
        if old["mcpServers"].get(name) != Some(server) {
            no_secret_shared(name, server)?;
        }
    }
    let text = if text.ends_with('\n') {
        text.to_owned()
    } else {
        format!("{text}\n")
    };
    Ok(Preview {
        path: path.to_string_lossy().into_owned(),
        diff: settings::diff(&before, &text),
        hash: settings::fingerprint(&before),
        text,
        tracked: false,
    })
}

/// Refused when the file changed since the preview of `hash`.
pub fn mcp_json_save(env: &Env, text: &str, hash: Option<&str>) -> Result<Preview> {
    let preview = mcp_json_preview(env, text)?;
    if let Some(hash) = hash {
        ensure!(
            hash == preview.hash,
            "{} changed since its preview: refresh and look at the change again",
            preview.path
        );
    }
    settings::write_atomic(Path::new(&preview.path), &preview.text)?;
    Ok(preview)
}
