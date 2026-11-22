//! The vault for the UI, and the loop that locks it once idle or after a sleep. A value reaches the webview only
//! through `vault_reveal`, when the user asks; the local API never gives one (see `call`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_core::ipc::VaultCall;
use termf_sync::OsKeyStore;
use termf_vault::{
    Access, Actor, Entry, KEYCHAIN, Scope, Secret, SecretId, SecretInfo, Status, SystemClock,
    Vault, Verification,
};

use crate::{CmdResult, err};

/// How soon the key leaves memory once idle or after a sleep. Any use checks first, so this is not what
/// protects it: a slow tick saves energy.
const TICK: Duration = Duration::from_secs(15);

pub struct VaultState {
    vault: Arc<Vault>,
    config_dir: PathBuf,
}

pub fn start(app: &AppHandle, config_dir: PathBuf, data_dir: &Path, minutes: u32) -> VaultState {
    termf_vault::harden_process();
    let vault = Vault::new(
        data_dir,
        Box::new(OsKeyStore(KEYCHAIN)),
        Box::new(SystemClock::default()),
    );
    let _ = vault.set_auto_lock(minutes);
    let vault = Arc::new(vault);
    let (app, background) = (app.clone(), vault.clone());
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(TICK);
            if background.check() {
                emit(&app, &background);
            }
        }
    });
    VaultState { vault, config_dir }
}

fn emit(app: &AppHandle, vault: &Vault) {
    if let Ok(status) = vault.status() {
        let _ = app.emit("vault-status", status);
    }
}

/// The local API: the CLI's `termf vault status | unlock | lock`. Never a value nor a name: an agent can read
/// the user's token (STATUS §4), and the password it would need to unlock is the user's.
pub fn call(app: &AppHandle, call: VaultCall) -> anyhow::Result<Value> {
    let vault = &app.state::<VaultState>().vault;
    let status = match call {
        VaultCall::Status => vault.status()?,
        VaultCall::Unlock { password } => {
            let password = Secret::new(password);
            vault.unlock(&Actor::human(), Some(password.expose()), None)?
        }
        VaultCall::Lock => vault.lock(&Actor::human())?,
    };
    emit(app, vault);
    Ok(serde_json::to_value(status)?)
}

/// Argon2id and the external managers' CLIs take a while: never on the main thread.
async fn blocking<T: Send + 'static>(
    state: &VaultState,
    f: impl FnOnce(&Vault) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let vault = state.vault.clone();
    tauri::async_runtime::spawn_blocking(move || f(&vault))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

fn id(name: &str, scope: Scope, project: Option<&str>) -> anyhow::Result<SecretId> {
    SecretId::new(name, scope, project)
}

#[tauri::command]
pub async fn vault_status(state: State<'_, VaultState>) -> CmdResult<Status> {
    blocking(&state, |v| v.status()).await
}

#[tauri::command]
pub async fn vault_init(
    state: State<'_, VaultState>,
    password: String,
    remember: bool,
) -> CmdResult<Status> {
    let password = Secret::new(password);
    blocking(&state, move |v| {
        v.init(&Actor::human(), password.expose(), remember)
    })
    .await
}

#[tauri::command]
pub async fn vault_unlock(
    state: State<'_, VaultState>,
    password: Option<String>,
    remember: Option<bool>,
) -> CmdResult<Status> {
    let password = password.map(Secret::new);
    blocking(&state, move |v| {
        v.unlock(
            &Actor::human(),
            password.as_ref().map(Secret::expose),
            remember,
        )
    })
    .await
}

#[tauri::command]
pub async fn vault_lock(state: State<'_, VaultState>) -> CmdResult<Status> {
    blocking(&state, |v| v.lock(&Actor::human())).await
}

#[tauri::command]
pub async fn vault_reauth(state: State<'_, VaultState>, password: String) -> CmdResult<Status> {
    let password = Secret::new(password);
    blocking(&state, move |v| {
        v.reauth(&Actor::human(), password.expose())
    })
    .await
}

#[tauri::command]
pub async fn vault_list(
    state: State<'_, VaultState>,
    scope: Option<Scope>,
    project: Option<String>,
) -> CmdResult<Vec<SecretInfo>> {
    blocking(&state, move |v| v.list(scope, project.as_deref())).await
}

#[tauri::command]
pub async fn vault_set(
    state: State<'_, VaultState>,
    name: String,
    scope: Scope,
    project: Option<String>,
    value: String,
    description: Option<String>,
) -> CmdResult<SecretInfo> {
    let value = Secret::new(value);
    blocking(&state, move |v| {
        let id = id(&name, scope, project.as_deref())?;
        v.set(&Actor::human(), &id, value, description.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn vault_ref(
    state: State<'_, VaultState>,
    name: String,
    scope: Scope,
    project: Option<String>,
    uri: String,
    description: Option<String>,
) -> CmdResult<SecretInfo> {
    blocking(&state, move |v| {
        let id = id(&name, scope, project.as_deref())?;
        v.set_reference(&Actor::human(), &id, &uri, description.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn vault_reveal(
    state: State<'_, VaultState>,
    name: String,
    scope: Scope,
    project: Option<String>,
) -> CmdResult<String> {
    blocking(&state, move |v| {
        let id = id(&name, scope, project.as_deref())?;
        Ok(v.value(&Actor::human(), &id, Access::Reveal)?
            .expose()
            .to_owned())
    })
    .await
}

#[tauri::command]
pub async fn vault_delete(
    state: State<'_, VaultState>,
    name: String,
    scope: Scope,
    project: Option<String>,
) -> CmdResult<()> {
    blocking(&state, move |v| {
        v.delete(&Actor::human(), &id(&name, scope, project.as_deref())?)
    })
    .await
}

#[tauri::command]
pub async fn vault_audit(
    state: State<'_, VaultState>,
    limit: Option<usize>,
) -> CmdResult<Vec<Entry>> {
    blocking(&state, move |v| v.audit(limit.unwrap_or(500))).await
}

#[tauri::command]
pub async fn vault_audit_verify(state: State<'_, VaultState>) -> CmdResult<Verification> {
    blocking(&state, |v| v.verify()).await
}

#[tauri::command]
pub async fn vault_audit_export(state: State<'_, VaultState>, path: String) -> CmdResult<u64> {
    blocking(&state, move |v| v.export(&mut fs::File::create(path)?)).await
}

#[tauri::command]
pub async fn vault_auto_lock_set(state: State<'_, VaultState>, minutes: u32) -> CmdResult<Status> {
    let config_dir = state.config_dir.clone();
    blocking(&state, move |v| {
        v.set_auto_lock(minutes)?;
        termf_core::save_vault_auto_lock(&config_dir, minutes)?;
        v.status()
    })
    .await
}
