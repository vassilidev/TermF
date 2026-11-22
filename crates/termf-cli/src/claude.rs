//! `termf claude settings | mcp | plugin`: Claude Code's settings by scope, its MCP servers and its plugins, with
//! the checks of the app's Claude view. TermF does not need to run.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use termf_claude::Env;
use termf_claude::manage::{self, McpAction, PluginAction};
use termf_claude::settings::{self, Edit, Scope};

use crate::take_opt;

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let scope = take_opt(&mut args, "--scope")?;
    let accept = take_opt(&mut args, "--accept-command")?;
    let repo_arg = take_opt(&mut args, "--repo")?;
    let repo = match &repo_arg {
        Some(repo) => std::path::absolute(repo)?,
        None => std::env::current_dir()?,
    };
    if let Some(flag) = args.iter().find(|a| a.starts_with("--")) {
        bail!("termf claude does not take {flag}\n{}", crate::USAGE);
    }
    let env = Env::current(Some(repo), "claude".into());
    match args.as_slice() {
        ["settings", "get", key] => get(&env, key, scope.as_deref()),
        ["settings", action @ ("set" | "unset"), key, rest @ ..] => {
            let value = match (*action, rest) {
                ("set", [value]) => Some(
                    serde_json::from_str(value)
                        .unwrap_or_else(|_| Value::String(value.to_string())),
                ),
                ("unset", []) => None,
                _ => bail!(crate::USAGE),
            };
            let scope = parse_scope(scope.as_deref().unwrap_or("user"))?;
            let edit = Edit::Set {
                key: key.to_string(),
                value,
            };
            let saved = settings::save(&env, scope, &edit, None)?;
            if saved.diff.is_empty() {
                println!("{} already says so", saved.path);
            } else {
                print!("{}", saved.diff);
                println!("saved {}", saved.path);
            }
            Ok(())
        }
        ["mcp", "add", name, server] => {
            let server: Value = serde_json::from_str(server).context(
                "the server is JSON: {\"command\": …} or {\"type\": \"http\", \"url\": …}",
            )?;
            let action = McpAction::Add {
                name: name.to_string(),
                scope: scope.unwrap_or_else(|| "local".into()),
                server,
            };
            println!("{}", manage::mcp(&env, &action)?);
            Ok(())
        }
        ["mcp", "remove", name] => {
            let action = McpAction::Remove {
                name: name.to_string(),
                scope,
            };
            println!("{}", manage::mcp(&env, &action)?);
            Ok(())
        }
        ["plugin", "install", plugin] => {
            let scope = scope.unwrap_or_else(|| "user".into());
            let action = PluginAction::Install {
                plugin: plugin.to_string(),
                scope: scope.clone(),
                accept,
            };
            let done = manage::plugin(&env, &action)?;
            if let Some(shown) = done.shown {
                let sha = shown["sha256"].as_str().unwrap_or_default();
                bail!(
                    "{}\nthe marketplace runs this command to install it:\n  {}\nonce you trust it: {}",
                    done.message,
                    shown["command"].as_str().unwrap_or_default(),
                    accept_hint(plugin, &scope, repo_arg.as_deref(), sha)
                );
            }
            println!("{}", done.message);
            Ok(())
        }
        ["plugin", action @ ("remove" | "enable" | "disable"), plugin] => {
            if accept.is_some() {
                bail!("--accept-command goes with install only");
            }
            let plugin = plugin.to_string();
            let action = match *action {
                "remove" => PluginAction::Uninstall {
                    plugin,
                    scope: scope.unwrap_or_else(|| "user".into()),
                },
                "enable" => PluginAction::Enable { plugin, scope },
                _ => PluginAction::Disable { plugin, scope },
            };
            println!("{}", manage::plugin(&env, &action)?.message);
            Ok(())
        }
        _ => bail!(crate::USAGE),
    }
}

fn parse_scope(scope: &str) -> Result<Scope> {
    serde_json::from_value(json!(scope))
        .with_context(|| format!("--scope takes user, project, local or managed, not {scope}"))
}

/// The key in each scope's file, masked, and the scope that wins.
fn get(env: &Env, key: &str, scope: Option<&str>) -> Result<()> {
    let only = scope.map(parse_scope).transpose()?;
    let read = settings::masked(settings::read(env));
    for file in read
        .files
        .iter()
        .filter(|f| only.is_none_or(|s| s == f.scope))
    {
        let value = file
            .value
            .as_ref()
            .and_then(|v| key.split('.').try_fold(v, |v, part| v.get(part)));
        let scope = serde_json::to_value(file.scope)?;
        let scope = scope.as_str().unwrap_or_default();
        match value {
            Some(v) => println!("{scope:<8}  {v}"),
            None => println!("{scope:<8}  -"),
        }
    }
    if let Some(e) = read.effective.iter().find(|e| e.key == key) {
        let scope = serde_json::to_value(e.scope)?;
        println!(
            "{} by {}",
            if e.merged { "merged" } else { "decided" },
            scope.as_str().unwrap_or_default()
        );
    }
    Ok(())
}

/// The same install, with what it needs to run the command the marketplace showed.
fn accept_hint(plugin: &str, scope: &str, repo: Option<&str>, sha: &str) -> String {
    let repo = repo.map(|r| format!(" --repo {r}")).unwrap_or_default();
    format!("termf claude plugin install {plugin} --scope {scope}{repo} --accept-command {sha}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_accept_hint_keeps_scope_and_repository() {
        assert_eq!(
            super::accept_hint("db@acme", "project", Some("../api"), "ab12"),
            "termf claude plugin install db@acme --scope project --repo ../api --accept-command ab12"
        );
        assert!(!super::accept_hint("db@acme", "user", None, "ab12").contains("--repo"));
    }
}
