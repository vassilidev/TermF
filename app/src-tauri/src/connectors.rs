//! A project's connectors in its Configuration (`termf_library::connectors`), the Claude account's connectors to
//! give them from, read once through `claude mcp list` and kept, and the vault's keys of the project's servers.

use std::sync::Mutex;

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_agents::Given;
use termf_claude::cli::{self, Health};
use termf_claude::mask::{MASK, mask_text};
use termf_library::connectors::{self, Access};
use termf_library::{Kind, Scope};
use termf_vault::{Actor, Secret, SecretId, Vault};

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
            target: target(server),
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

/// The server's command or URL as the page shows it: masked, and a URL that holds a key cut after its host.
fn target(server: &Value) -> String {
    match server["url"].as_str() {
        Some(url) if connectors::secret_url(url) => {
            let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
            let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
            let host = host.rsplit('@').next().unwrap_or_default();
            format!("{scheme}://{host}/{MASK}")
        }
        Some(url) => url.to_owned(),
        None => {
            let args = server["args"].as_array().into_iter().flatten();
            let words = std::iter::once(&server["command"]).chain(args);
            let line = words
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            mask_text(&line)
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

/// Keeps a new server's keys in the vault, as the project's dev secrets of their variables' names (never in
/// `[agents.secrets]`: only the sessions of the roles the server is given to get them, in its config).
fn store(
    vault: &Vault,
    project: &str,
    name: &str,
    keys: Vec<(String, String)>,
    password: Option<&Secret>,
) -> Result<()> {
    ensure!(
        vault.initialized(),
        "create the vault first: the server's keys are kept there"
    );
    let human = Actor::human();
    if !vault.status()?.unlocked {
        let password = password.ok_or(termf_vault::Locked)?;
        vault.unlock(&human, Some(password.expose()), None, None)?;
    }
    let purpose = format!("MCP server {name}");
    for (variable, value) in keys {
        let id = SecretId::new(&variable, termf_vault::Scope::Dev, Some(project))?;
        vault.set(&human, &id, Secret::new(value), Some(&purpose))?;
    }
    Ok(())
}

/// The project's own dev secrets named `wanted` (its servers' `${MCP_<NAME>_…}`), each read as the agent, which
/// the audit records; one the vault does not give is left out, and the server that needs it with it.
pub fn keys(vault: &Vault, project: &str, wanted: &[String], actor: &Actor) -> Given {
    if !vault.status().is_ok_and(|s| s.unlocked) {
        return Given::default();
    }
    let vars = wanted
        .iter()
        .filter_map(|name| {
            let id = SecretId::new(name, termf_vault::Scope::Dev, Some(project)).ok()?;
            let value = vault.value(actor, &id, termf_vault::Access::Use).ok()?;
            Some((name.clone(), value.expose().to_owned()))
        })
        .collect();
    Given { vars, note: None }
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
        let access = Access { roles, write };
        let mut server = server;
        if let Some(server) = server.as_mut() {
            connectors::check_name(&name)?;
            let taken = connectors::take_secrets(&name, server)?;
            connectors::check(config_dir, &project, &name, Some(server), &access)?;
            if !taken.is_empty() {
                let vault = crate::vault::shared(&app);
                let was_open = vault.status().is_ok_and(|s| s.unlocked);
                let stored = store(&vault, &project, &name, taken, password.as_ref());
                if let Ok(status) = vault.status()
                    && status.unlocked
                    && !was_open
                {
                    let _ = app.emit("vault-status", status);
                }
                stored.context("the server's keys go to the project's vault")?;
            }
        }
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
        assert_eq!(target(&stdio), "npx srv --api-key ••••••");
        for url in [
            "https://u:p@mcp.test/x?token=abc",
            "https://mcp.test/s/sk-0123456789abcdefghijklmnop/mcp",
            "https://mcp.test/mcp?key=abc",
        ] {
            let http = serde_json::json!({ "type": "http", "url": url });
            assert_eq!(target(&http), "https://mcp.test/••••••", "{url}");
        }
        let plain = serde_json::json!({ "type": "http", "url": "https://mcp.test/mcp" });
        assert_eq!(target(&plain), "https://mcp.test/mcp");
    }
}
