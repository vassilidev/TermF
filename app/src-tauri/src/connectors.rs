//! A project's connectors in its Configuration (`termf_library::connectors`), and the Claude account's connectors
//! to give them from, read once through `claude mcp list` and kept.

use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_claude::cli::{self, Health};
use termf_claude::mask::mask_text;
use termf_core::agent_env;
use termf_library::connectors::{self, Access};
use termf_library::{Kind, Scope};
use termf_vault::Secret;

use crate::{AppState, CmdResult, err};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connector {
    name: String,
    account: bool,
    /// The command or URL, masked.
    target: String,
    /// `None`: a server set up before connectors, which every session but the CTO's and the expert's gets.
    access: Option<Access>,
    /// It reads keys of the vault (`${…}`), which the CTO and the expert never get.
    keys: bool,
}

#[derive(Serialize)]
pub struct Role {
    name: String,
    /// It may read prod (Ops): a connector it gets waits for the user at each call.
    prod: bool,
}

#[derive(Serialize)]
pub struct Listing {
    connectors: Vec<Connector>,
    roles: Vec<Role>,
}

#[derive(Serialize, Clone)]
pub struct AccountConnector {
    name: String,
    health: Health,
}

#[derive(Serialize, Clone, Default)]
pub struct Account {
    connectors: Vec<AccountConnector>,
    error: Option<String>,
}

/// `claude mcp list` checks every server it knows: once, then again on demand.
static ACCOUNT: Mutex<Option<Account>> = Mutex::new(None);

fn listing(state: &AppState, project: &str) -> Result<Listing> {
    let config_dir = &state.config_dir;
    let configured = connectors::access(config_dir, project);
    let servers = termf_library::mcp_servers(config_dir, project);
    let mut list: Vec<Connector> = servers
        .iter()
        .map(|(name, server)| Connector {
            name: name.clone(),
            account: false,
            target: mask_text(&target(server)),
            access: configured.get(name).cloned(),
            keys: server.to_string().contains("${"),
        })
        .collect();
    list.extend(
        configured
            .iter()
            .filter(|(name, _)| connectors::is_account(name))
            .map(|(name, access)| Connector {
                name: name.clone(),
                account: true,
                target: String::new(),
                access: Some(access.clone()),
                keys: false,
            }),
    );
    let scope = Scope::new(config_dir, None, Some(project.to_owned()))?;
    let roles = termf_library::resolve(config_dir, &scope)?
        .into_iter()
        .filter(|i| i.kind == Kind::Role)
        .map(|role| {
            let tools = termf_library::list_of(role.meta.get("allowedTools"));
            Role {
                prod: role.name == "cto" || tools.iter().any(|t| t.starts_with("mcp__termf__ops")),
                name: role.name,
            }
        })
        .collect();
    Ok(Listing {
        connectors: list,
        roles,
    })
}

fn target(server: &Value) -> String {
    match server["url"].as_str() {
        Some(url) => url.to_owned(),
        None => {
            let args = server["args"].as_array().into_iter().flatten();
            let words = std::iter::once(&server["command"]).chain(args);
            words
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

#[tauri::command]
pub fn connectors_list(state: State<AppState>, project: String) -> CmdResult<Listing> {
    listing(&state, &project).map_err(err)
}

/// The account's connectors (`claude.ai …`) Claude Code fetches in every session but those in
/// `--strict-mcp-config`, with their health; kept until `refresh`.
#[tauri::command]
pub async fn connectors_account(state: State<'_, AppState>, refresh: bool) -> CmdResult<Account> {
    if !refresh && let Some(known) = ACCOUNT.lock().unwrap().clone() {
        return Ok(known);
    }
    let env = crate::claude::env(&state.config_dir, None);
    let account = tauri::async_runtime::spawn_blocking(move || match cli::mcp_list(&env) {
        Ok(list) => Account {
            connectors: list
                .into_iter()
                .filter(|s| connectors::is_account(&s.name))
                .map(|s| AccountConnector {
                    name: s.name,
                    health: s.health,
                })
                .collect(),
            error: None,
        },
        Err(e) => Account {
            connectors: Vec::new(),
            error: Some(mask_text(&format!("{e:#}"))),
        },
    })
    .await
    .map_err(|e| e.to_string())?;
    *ACCOUNT.lock().unwrap() = Some(account.clone());
    Ok(account)
}

/// Adds or changes a connector. A new server's values that look like secrets go to the project's vault (a locked
/// vault opens with `password`) and the file names them only (`${MCP_<NAME>_<FIELD>}`).
#[tauri::command]
pub async fn connectors_save(
    app: AppHandle,
    project: String,
    name: String,
    server: Option<Value>,
    roles: Vec<String>,
    write: bool,
    password: Option<String>,
) -> CmdResult<Listing> {
    let password = password.map(Secret::new);
    tauri::async_runtime::spawn_blocking(move || -> Result<Listing> {
        let state = app.state::<AppState>();
        let config_dir = &state.config_dir;
        let mut server = server;
        if let Some(server) = server.as_mut() {
            connectors::check_name(&name)?;
            connectors::check_server(server)?;
            let taken = connectors::take_secrets(&name, server);
            if !taken.is_empty() {
                let vault = crate::vault::shared(&app);
                let was_open = vault.status().is_ok_and(|s| s.unlocked);
                let purpose = format!("MCP server {name}");
                let stored = taken.into_iter().try_for_each(|(variable, value)| {
                    agent_env::check_variable(&variable)?;
                    let value = Secret::new(value);
                    crate::agentenv::give_key(
                        &vault,
                        config_dir,
                        &project,
                        &variable,
                        &purpose,
                        &value,
                        password.as_ref(),
                    )
                });
                if let Ok(status) = vault.status()
                    && status.unlocked
                    && !was_open
                {
                    let _ = app.emit("vault-status", status);
                }
                stored.context("the server's keys go to the project's vault")?;
            }
        }
        let access = Access { roles, write };
        connectors::save(config_dir, &project, &name, server.as_ref(), &access)?;
        listing(&state, &project)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}

/// Removes a connector, and the project's own server; its keys stay in the vault.
#[tauri::command]
pub fn connectors_remove(
    state: State<AppState>,
    project: String,
    name: String,
) -> CmdResult<Listing> {
    connectors::remove(&state.config_dir, &project, &name)
        .and_then(|()| listing(&state, &project))
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_a_server_without_its_keys() {
        let stdio = serde_json::json!({ "command": "npx", "args": ["srv", "--api-key", "abc"] });
        assert_eq!(mask_text(&target(&stdio)), "npx srv --api-key ••••••");
        let http = serde_json::json!({ "type": "http", "url": "https://u:p@mcp.test/x?token=abc" });
        assert_eq!(
            mask_text(&target(&http)),
            "https://u:••••••@mcp.test/x?token=••••••"
        );
    }
}
