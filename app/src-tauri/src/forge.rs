//! Laravel Forge in Ops: the project's account (its token in the vault, global, prod), the import of its servers and
//! sites as Ops connections and deploy targets, and a site's deployments.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use termf_ops::forge::{self, Account, Deployment, Forge, Server, Site};
use termf_vault::{Actor, Scope, Secret, SecretId, Vault};

use crate::CmdResult;
use crate::ops::{OpsState, blocking};

/// The account's token, from the vault.
fn token(vault: &Vault, account: &Account) -> Result<Secret> {
    ensure!(
        vault.initialized(),
        "there is no vault yet: create it in the Vault view, the Forge token goes there"
    );
    let id = SecretId::new(&account.token, Scope::Prod, None)?;
    vault.value(&Actor::human(), &id, termf_vault::Access::Use)
}

/// The project's account and its API.
fn api(state: &OpsState, vault: &Vault, project: &str) -> Result<(Account, Forge, Secret)> {
    let account = forge::account(&state.config_dir, project)?
        .context("the project has no Forge account yet")?;
    let secret = token(vault, &account)?;
    let curl = state
        .runner
        .find("curl")
        .context("curl is missing: TermF reaches Forge through it")?;
    let api = Forge::new(secret.expose().to_owned(), curl);
    Ok((account, api, secret))
}

/// What Forge said, the token taken out should it come back.
fn hide(text: String, secret: &Secret) -> String {
    text.replace(secret.expose(), "••••••")
}

#[tauri::command]
pub async fn forge_account(app: AppHandle, project: String) -> CmdResult<Option<Account>> {
    blocking(app, move |_, state, _| {
        forge::account(&state.config_dir, &project)
    })
    .await
}

/// Names the project's account; `token`, when given, goes to the vault (global, prod) under the account's name.
#[tauri::command]
pub async fn forge_account_save(
    app: AppHandle,
    project: String,
    label: String,
    token: Option<String>,
) -> CmdResult<Account> {
    blocking(app, move |_, state, vault| {
        let account = Account {
            token: forge::token_name(&label),
            label: label.trim().to_owned(),
        };
        if let Some(value) = token.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()) {
            ensure!(
                vault.initialized(),
                "there is no vault yet: create it in the Vault view, the Forge token goes there"
            );
            let id = SecretId::new(&account.token, Scope::Prod, None)?;
            let about = format!("Laravel Forge API token of the account {}", account.label);
            vault.set(&Actor::human(), &id, Secret::new(value), Some(&about))?;
        }
        forge::save_account(&state.config_dir, &project, Some(&account))?;
        Ok(account)
    })
    .await
}

/// Forgets the project's account; its connections and deploy targets stay. `forget_token` also deletes the token
/// from the vault, unless other projects' accounts use it: their names come back, and the token stays.
#[tauri::command]
pub async fn forge_account_delete(
    app: AppHandle,
    project: String,
    forget_token: bool,
) -> CmdResult<Vec<String>> {
    blocking(app, move |_, state, vault| {
        let account = forge::account(&state.config_dir, &project)?
            .context("the project has no Forge account")?;
        let sharing = forge::sharing(&state.config_dir, &account.token, &project)?;
        if forget_token && sharing.is_empty() && vault.initialized() {
            let id = SecretId::new(&account.token, Scope::Prod, None)?;
            if vault
                .list(Some(Scope::Prod), None)?
                .iter()
                .any(|s| s.name == account.token && s.project.is_none())
            {
                vault.delete(&Actor::human(), &id)?;
            }
        }
        forge::save_account(&state.config_dir, &project, None)?;
        Ok(if forget_token { sharing } else { Vec::new() })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    server: Server,
    sites: Vec<Site>,
}

/// The account's servers and their sites, for the user to tick.
#[tauri::command]
pub async fn forge_sites(app: AppHandle, project: String) -> CmdResult<Vec<Found>> {
    blocking(app, move |_, state, vault| {
        let (_, api, secret) = api(state, vault, &project)?;
        let found = api.servers().and_then(|servers| {
            servers
                .into_iter()
                .map(|server| {
                    Ok(Found {
                        sites: api.sites(server.id)?,
                        server,
                    })
                })
                .collect::<Result<Vec<_>>>()
        });
        found.map_err(|e| anyhow::anyhow!(hide(format!("{e:#}"), &secret)))
    })
    .await
}

#[derive(Deserialize)]
pub struct Pick {
    server: u64,
    site: u64,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Import {
    connections: Vec<String>,
    targets: Vec<String>,
    /// What was there already, or could not be made, and why.
    skipped: Vec<String>,
}

/// Imports the ticked sites: an SSH connection per server (`forge@ip`, the site's folder when it is the only one
/// there), each site's database through it (its password to the vault, never the project), and a deploy target per
/// site, tied to the project's repository of the same name.
#[tauri::command]
pub async fn forge_import(app: AppHandle, project: String, picks: Vec<Pick>) -> CmdResult<Import> {
    blocking(app, move |_, state, vault| {
        let (_, api, secret) = api(state, vault, &project)?;
        let config = &state.config_dir;
        let repos: Vec<String> = termf_core::load_projects(config)?
            .into_iter()
            .find(|p| p.id == project)
            .map(|p| p.repos.into_iter().map(|r| r.path).collect())
            .unwrap_or_default();
        let servers = api.servers()?;
        let mut out = Import::default();
        let mut by_server: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for p in &picks {
            by_server.entry(p.server).or_default().push(p.site);
        }
        for (server_id, site_ids) in by_server {
            let server = servers
                .iter()
                .find(|s| s.id == server_id)
                .context("no such Forge server")?;
            let sites: Vec<Site> = api
                .sites(server_id)?
                .into_iter()
                .filter(|s| site_ids.contains(&s.id))
                .collect();
            let only = (sites.len() == 1)
                .then(|| sites[0].directory.clone())
                .flatten();
            let ssh = forge::server_connection(server, only.as_deref())?;
            let existing = termf_ops::connections(config, &project)?;
            if existing.iter().any(|c| c.name == ssh.name) {
                out.skipped
                    .push(format!("{}: already a connection", ssh.name));
            } else {
                termf_ops::save_connection(config, &project, &ssh, None)?;
                out.connections.push(ssh.name.clone());
            }
            for site in &sites {
                let env = termf_ops::env::parse(
                    &api.env(server_id, site.id)
                        .map_err(|e| anyhow::anyhow!(hide(format!("{e:#}"), &secret)))?,
                );
                let made = forge::site_database(site, &env.into_iter().collect(), &ssh.name)?;
                for (name, value) in made.secrets {
                    ensure!(
                        vault.initialized(),
                        "there is no vault yet: create it in the Vault view, the passwords go there"
                    );
                    let id = SecretId::new(&name, Scope::Prod, Some(&project))?;
                    vault.set(
                        &Actor::human(),
                        &id,
                        Secret::new(value),
                        Some(&format!("Database password of {} (Forge)", site.name)),
                    )?;
                }
                for db in made.connections {
                    if termf_ops::connections(config, &project)?
                        .iter()
                        .any(|c| c.name == db.name)
                    {
                        out.skipped
                            .push(format!("{}: already a connection", db.name));
                        continue;
                    }
                    termf_ops::save_connection(config, &project, &db, None)?;
                    out.connections.push(db.name);
                }
                let repo = site
                    .repository
                    .as_deref()
                    .and_then(|r| r.rsplit('/').next())
                    .and_then(|name| {
                        repos.iter().find(|p| {
                            Path::new(p)
                                .file_name()
                                .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(name))
                        })
                    });
                let target = termf_ci::deploy::Target {
                    name: site.name.clone(),
                    tool: termf_ci::deploy::Tool::Forge,
                    target: site.name.clone(),
                    scope: Some(server.name.clone()),
                    service: None,
                    repo: repo.cloned(),
                    token: None,
                    env: "prod".into(),
                };
                match termf_ci::deploy::save_target(config, &project, &target, None) {
                    Ok(_) => out.targets.push(target.name),
                    Err(e) => out.skipped.push(format!("{}: {e:#}", target.name)),
                }
            }
        }
        Ok(out)
    })
    .await
}

#[tauri::command]
pub async fn forge_deployments(
    app: AppHandle,
    project: String,
    server: u64,
    site: u64,
) -> CmdResult<Vec<Deployment>> {
    blocking(app, move |_, state, vault| {
        let (_, api, secret) = api(state, vault, &project)?;
        api.deployments(server, site)
            .map_err(|e| anyhow::anyhow!(hide(format!("{e:#}"), &secret)))
    })
    .await
}

#[tauri::command]
pub async fn forge_output(
    app: AppHandle,
    project: String,
    server: u64,
    site: u64,
    deployment: u64,
) -> CmdResult<String> {
    blocking(app, move |_, state, vault| {
        let (_, api, secret) = api(state, vault, &project)?;
        let text = hide(api.output(server, site, deployment)?, &secret);
        // A deployment's script may print a value of the vault.
        Ok(match vault.masker() {
            Ok(m) => m.mask(&text).text,
            Err(_) => text,
        })
    })
    .await
}

/// Redeploys a site, once the user (or an action's card) approved it.
#[tauri::command]
pub async fn forge_deploy(
    app: AppHandle,
    project: String,
    server: u64,
    site: u64,
) -> CmdResult<()> {
    blocking(app, move |_, state, vault| {
        let (_, api, secret) = api(state, vault, &project)?;
        api.deploy(server, site)
            .map_err(|e| anyhow::anyhow!(hide(format!("{e:#}"), &secret)))
    })
    .await
}

/// The token of a Forge deploy target that names none: the project's account's, when it has one.
pub fn account_token(config_dir: &Path, vault: &Vault, project: &str) -> Result<Option<Secret>> {
    match forge::account(config_dir, project)? {
        Some(account) => token(vault, &account).map(Some),
        None => Ok(None),
    }
}
