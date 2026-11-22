//! The vault for the UI, and the loop that locks it once idle or after a sleep. A value reaches the webview only
//! through `vault_reveal`, when the user asks; the local API never gives one (see `call`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_core::ipc::VaultCall;
use termf_sync::OsKeyStore;
use termf_vault::{
    Access, Actor, ConflictInfo, Entry, Field, ForeignChoice, ForeignPreview, ItemDraft, ItemInfo,
    KEYCHAIN, KeyInfo, Merged, Recipe, Scope, Secret, SecretId, SecretInfo, Status, SystemClock,
    Vault, Verification, Zeroizing, digest,
};

use crate::biometry;
use crate::{CmdResult, err};

/// How soon the key leaves memory once idle or after a sleep. Any use checks first, so this is not what
/// protects it: a slow tick saves energy.
const TICK: Duration = Duration::from_secs(15);

pub struct VaultState {
    vault: Arc<Vault>,
    config_dir: PathBuf,
    /// The other manager's file last imported, and its digest: the only one TermF offers to delete.
    imported: Arc<Mutex<Option<(PathBuf, String)>>>,
}

pub fn start(app: &AppHandle, config_dir: PathBuf, data_dir: &Path) -> VaultState {
    termf_vault::harden_process();
    let told = app.clone();
    let vault = Vault::new(
        data_dir,
        Box::new(OsKeyStore(KEYCHAIN)),
        Box::new(SystemClock::default()),
    )
    .with_keys(termf_vault::fido::system())
    .with_quick()
    .on_lock(move |status| {
        let _ = told.emit("vault-status", status);
    });
    let vault = Arc::new(vault);
    let (background, config) = (vault.clone(), config_dir.clone());
    std::thread::spawn(move || {
        // `[vault]` changes by hand or through the sync too: applied whenever termf.toml does.
        let mut seen = None;
        loop {
            let file = config.join("termf.toml");
            let modified = fs::metadata(&file).and_then(|m| m.modified()).ok();
            if modified != seen {
                seen = modified;
                if let Ok(settings) = termf_core::load_settings(&config) {
                    let _ = background.set_auto_lock(settings.vault.auto_lock_minutes);
                }
            }
            background.check();
            std::thread::sleep(TICK);
        }
    });
    VaultState {
        vault,
        config_dir,
        imported: Arc::default(),
    }
}

/// Another machine's version of the vault waits beside it: merged now if it is open (it locks instead when the
/// way it opens changed there), else when it opens. Returns whether something was merged.
pub fn synced(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<VaultState>() else {
        return false;
    };
    if state.vault.status().is_ok_and(|s| s.synced.is_none()) {
        return false;
    }
    let merged = state.vault.merge_synced(&Actor::termf());
    emit(app, &state.vault);
    match merged {
        Ok(Some(rows)) => {
            let _ = app.emit("vault-merged", rows);
            true
        }
        _ => false,
    }
}

/// A vault export sealed by its own password, handed to `write` (the config archive): the vault open, its
/// master password typed again. Audited once written.
pub fn sealed(
    app: &AppHandle,
    project: Option<&str>,
    password: &str,
    master: &str,
    write: termf_vault::ExportWrite,
) -> anyhow::Result<()> {
    let state = app.try_state::<VaultState>().ok_or(termf_vault::Locked)?;
    state.vault.reauth(&Actor::human(), master)?;
    state
        .vault
        .export_sealed(&Actor::human(), project, password, write)
}

/// Writes an export, audited by `export` once written: a file left by an export that failed after is removed.
fn export_to(
    path: &str,
    export: impl FnOnce(termf_vault::ExportWrite) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut written = false;
    let result = export(&mut |bytes| {
        write_private(path, bytes)?;
        written = true;
        Ok(())
    });
    if result.is_err() && written {
        let _ = fs::remove_file(path);
    }
    result
}

/// A file only the user reads (Unix), written whole.
fn write_private(path: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    file.write_all(bytes)?;
    Ok(file.sync_all()?)
}

/// A file to import: a TermF vault export, alone or inside a config archive, or another manager's export.
fn import_bytes(path: &str) -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let bytes = Zeroizing::new(fs::read(path)?);
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let vault = termf_library::archive::vault(Path::new(path))?;
        return Ok(Zeroizing::new(anyhow::Context::context(
            vault,
            "this configuration export holds no vault",
        )?));
    }
    Ok(bytes)
}

/// The vault's values to hide from what TermF stores or hands an agent; none while it is locked.
pub fn masker(app: &AppHandle) -> Option<termf_vault::Masker> {
    app.try_state::<VaultState>()?.vault.masker().ok()
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
        VaultCall::Unlock { password, pin } => {
            let password = Secret::new(password);
            let pin = pin.map(Secret::new);
            vault.unlock(
                &Actor::human(),
                Some(password.expose()),
                None,
                pin.as_ref().map(Secret::expose),
            )?
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
    pin: Option<String>,
) -> CmdResult<Status> {
    let password = password.map(Secret::new);
    let pin = pin.map(Secret::new);
    blocking(&state, move |v| {
        v.unlock(
            &Actor::human(),
            password.as_ref().map(Secret::expose),
            remember,
            pin.as_ref().map(Secret::expose),
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

#[tauri::command]
pub async fn vault_items(
    state: State<'_, VaultState>,
    project: Option<String>,
) -> CmdResult<Vec<ItemInfo>> {
    blocking(&state, move |v| v.items(project.as_deref())).await
}

#[tauri::command]
pub async fn vault_item_save(state: State<'_, VaultState>, item: ItemDraft) -> CmdResult<ItemInfo> {
    blocking(&state, move |v| v.save_item(&Actor::human(), item)).await
}

#[tauri::command]
pub async fn vault_item_delete(state: State<'_, VaultState>, id: String) -> CmdResult<()> {
    blocking(&state, move |v| v.delete_item(&Actor::human(), &id)).await
}

/// A password, a note, a TOTP secret or the code valid now, which the user asked to see or copy.
#[tauri::command]
pub async fn vault_item_reveal(
    state: State<'_, VaultState>,
    id: String,
    field: Field,
) -> CmdResult<String> {
    blocking(&state, move |v| {
        Ok(v.item_value(&Actor::human(), &id, field, Access::Reveal)?
            .expose()
            .to_owned())
    })
    .await
}

#[tauri::command]
pub async fn vault_generate(recipe: Recipe) -> CmdResult<String> {
    termf_vault::generate(&recipe)
        .map(|s| s.expose().to_owned())
        .map_err(err)
}

/// How this machine can open the vault besides the password.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Methods {
    /// TermF asks for a security key's PIN (macOS, Linux); Windows asks in its own window.
    key_pin: bool,
    /// Touch ID or Windows Hello, when set up here; `None` on Linux.
    biometry: Option<&'static str>,
}

#[tauri::command]
pub async fn vault_methods(state: State<'_, VaultState>) -> CmdResult<Methods> {
    blocking(&state, |v| {
        Ok(Methods {
            key_pin: v.asks_pin(),
            biometry: biometry::available(),
        })
    })
    .await
}

/// `reason`: what the system's window says, in the user's language.
#[tauri::command]
pub async fn vault_unlock_biometry(
    state: State<'_, VaultState>,
    reason: String,
) -> CmdResult<Status> {
    blocking(&state, move |v| {
        // The system asks only when it would open the vault.
        v.can_unlock_quick()?;
        biometry::verify(&reason)?;
        v.unlock_quick(&Actor::human())
    })
    .await
}

#[tauri::command]
pub async fn vault_reauth_key(
    state: State<'_, VaultState>,
    pin: Option<String>,
) -> CmdResult<Status> {
    let pin = pin.map(Secret::new);
    blocking(&state, move |v| {
        v.reauth_key(&Actor::human(), pin.as_ref().map(Secret::expose))
    })
    .await
}

#[tauri::command]
pub async fn vault_reauth_biometry(
    state: State<'_, VaultState>,
    reason: String,
) -> CmdResult<Status> {
    blocking(&state, move |v| {
        anyhow::ensure!(v.status()?.unlocked, termf_vault::Locked);
        v.can_unlock_quick()?;
        biometry::verify(&reason)?;
        v.reauth_quick(&Actor::human())
    })
    .await
}

#[tauri::command]
pub async fn vault_keys(state: State<'_, VaultState>) -> CmdResult<Vec<KeyInfo>> {
    blocking(&state, |v| v.keys()).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Added {
    key: KeyInfo,
    /// The first key turned the second factor on: a new recovery key, to show once.
    recovery_key: Option<String>,
}

#[tauri::command]
pub async fn vault_key_add(
    state: State<'_, VaultState>,
    name: String,
    pin: Option<String>,
) -> CmdResult<Added> {
    let pin = pin.map(Secret::new);
    blocking(&state, move |v| {
        let (key, recovery) =
            v.add_key(&Actor::human(), &name, pin.as_ref().map(Secret::expose))?;
        Ok(Added {
            key,
            recovery_key: recovery.map(|r| r.expose().to_owned()),
        })
    })
    .await
}

#[tauri::command]
pub async fn vault_key_remove(state: State<'_, VaultState>, id: String) -> CmdResult<Status> {
    blocking(&state, move |v| v.remove_key(&Actor::human(), &id)).await
}

#[tauri::command]
pub async fn vault_password_change(
    state: State<'_, VaultState>,
    password: String,
) -> CmdResult<Status> {
    let password = Secret::new(password);
    blocking(&state, move |v| {
        v.change_password(&Actor::human(), password.expose())
    })
    .await
}

#[tauri::command]
pub async fn vault_recovery_new(state: State<'_, VaultState>) -> CmdResult<String> {
    blocking(&state, |v| {
        Ok(v.new_recovery(&Actor::human())?.expose().to_owned())
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recovered {
    status: Status,
    /// The one used stops working: this one replaces it, to show once.
    recovery_key: String,
}

#[tauri::command]
pub async fn vault_recover(
    state: State<'_, VaultState>,
    recovery_key: String,
    password: String,
) -> CmdResult<Recovered> {
    let (recovery_key, password) = (Secret::new(recovery_key), Secret::new(password));
    blocking(&state, move |v| {
        let (status, new) = v.recover(&Actor::human(), recovery_key.expose(), password.expose())?;
        Ok(Recovered {
            status,
            recovery_key: new.expose().to_owned(),
        })
    })
    .await
}

#[tauri::command]
pub async fn vault_join(state: State<'_, VaultState>) -> CmdResult<Status> {
    blocking(&state, |v| v.join_synced(&Actor::human())).await
}

#[tauri::command]
pub async fn vault_conflicts(
    state: State<'_, VaultState>,
    key: String,
) -> CmdResult<Vec<ConflictInfo>> {
    blocking(&state, move |v| v.conflicts(&key)).await
}

/// `keep`: the other version's id; absent: the current one.
#[tauri::command]
pub async fn vault_resolve(
    state: State<'_, VaultState>,
    key: String,
    keep: Option<String>,
) -> CmdResult<()> {
    blocking(&state, move |v| {
        v.resolve(&Actor::human(), &key, keep.as_deref())
    })
    .await
}

/// The vault, or one project's part, sealed by its own export password.
#[tauri::command]
pub async fn vault_export(
    state: State<'_, VaultState>,
    path: String,
    project: Option<String>,
    password: String,
) -> CmdResult<()> {
    let password = Secret::new(password);
    blocking(&state, move |v| {
        export_to(&path, |write| {
            v.export_sealed(
                &Actor::human(),
                project.as_deref(),
                password.expose(),
                write,
            )
        })
    })
    .await
}

/// In clear: only when the user asked, past a warning.
#[tauri::command]
pub async fn vault_export_csv(
    state: State<'_, VaultState>,
    path: String,
    project: Option<String>,
) -> CmdResult<()> {
    blocking(&state, move |v| {
        export_to(&path, |write| {
            v.export_csv(&Actor::human(), project.as_deref(), write)
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ImportFile {
    /// A TermF vault export: its password first.
    Termf,
    /// Another manager's export, in clear.
    Foreign { preview: ForeignPreview },
}

#[tauri::command]
pub async fn vault_import_read(
    state: State<'_, VaultState>,
    path: String,
) -> CmdResult<ImportFile> {
    blocking(&state, move |v| {
        let bytes = import_bytes(&path)?;
        if termf_vault::is_export(&bytes) {
            return Ok(ImportFile::Termf);
        }
        Ok(ImportFile::Foreign {
            preview: v.preview_foreign(&bytes)?,
        })
    })
    .await
}

#[tauri::command]
pub async fn vault_import_preview(
    state: State<'_, VaultState>,
    path: String,
    password: String,
) -> CmdResult<Vec<Merged>> {
    let password = Secret::new(password);
    blocking(&state, move |v| {
        v.preview_import(&import_bytes(&path)?, password.expose())
    })
    .await
}

#[tauri::command]
pub async fn vault_import_apply(
    state: State<'_, VaultState>,
    path: String,
    password: String,
) -> CmdResult<Vec<Merged>> {
    let password = Secret::new(password);
    blocking(&state, move |v| {
        v.import_sealed(&Actor::human(), &import_bytes(&path)?, password.expose())
    })
    .await
}

#[tauri::command]
pub async fn vault_import_foreign(
    state: State<'_, VaultState>,
    path: String,
    choice: ForeignChoice,
) -> CmdResult<usize> {
    let imported = state.imported.clone();
    blocking(&state, move |v| {
        let bytes = import_bytes(&path)?;
        let count = v.import_foreign(&Actor::human(), &bytes, &choice)?;
        *imported.lock().unwrap() = Some((PathBuf::from(&path), digest(&bytes)));
        Ok(count)
    })
    .await
}

/// Deletes the other manager's export just imported, which holds every password in clear: only that file,
/// unchanged, and only when the user asks.
#[tauri::command]
pub async fn vault_import_delete(state: State<'_, VaultState>, path: String) -> CmdResult<()> {
    let imported = state.imported.clone();
    blocking(&state, move |_| {
        let mut last = imported.lock().unwrap();
        let same = last.as_ref().is_some_and(|(p, d)| {
            *p == Path::new(&path) && fs::read(p).is_ok_and(|b| digest(&Zeroizing::new(b)) == *d)
        });
        anyhow::ensure!(
            same,
            "TermF deletes only the file it just imported, unchanged"
        );
        fs::remove_file(&path)?;
        *last = None;
        Ok(())
    })
    .await
}

/// `days: None` turns it off. Turning it on asks the system once, so it is known to work here.
#[tauri::command]
pub async fn vault_biometry_set(
    state: State<'_, VaultState>,
    days: Option<u32>,
    reason: String,
) -> CmdResult<Status> {
    blocking(&state, move |v| {
        if days.is_some() {
            v.can_change()?;
            biometry::verify(&reason)?;
        }
        v.set_quick(&Actor::human(), days)
    })
    .await
}
