//! Encrypted sync between machines: the commands for the UI, and the background loop that pushes a few seconds
//! after the config changes, and pulls at startup, when the window gets focus and every 5 minutes. No file
//! system polling: the loop sleeps until one of those happens.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::Invoke;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_sync::{
    Device, Engine, OsKeyStore, RemoteSpec, Report, Resolution, Scope, Setup, Status, Undone,
};

use crate::{AppState, CmdResult, err, quick_terminal};

const DEBOUNCE: Duration = Duration::from_secs(5);
const PERIOD: Duration = Duration::from_secs(5 * 60);
/// Switching windows back and forth does not fetch each time.
const FOCUS_GAP: Duration = Duration::from_secs(30);

/// Commands that write the config: a push follows them.
pub const WRITES: &[&str] = &[
    "project_save",
    "project_delete",
    "layout_save",
    "layout_delete",
    "keep_awake_set",
    "context_settings_set",
    "keys_set",
    "library_toggle",
    "library_import_claude",
    "config_import_apply",
    "conventions_save",
    "conventions_delete",
    "conventions_propose",
    "conventions_revise",
    "conventions_review",
    "conventions_discard",
    "conventions_restore",
    "conventions_apply",
    "conventions_switch",
    "vault_auto_lock_set",
    // The vault's file, when its scope is synced.
    "vault_init",
    "vault_set",
    "vault_ref",
    "vault_delete",
    "vault_item_save",
    "vault_item_delete",
    "vault_key_add",
    "vault_key_remove",
    "vault_password_change",
    "vault_recovery_new",
    "vault_recover",
    "vault_resolve",
    "vault_join",
    "vault_import_apply",
    "vault_import_foreign",
    "env_add",
    "env_update",
    "env_remove",
    "env_script_save",
    "env_script_delete",
];

enum Wake {
    Changed,
    Focused,
}

pub struct SyncState {
    engine: Arc<Engine>,
    wake: Mutex<Sender<Wake>>,
}

pub fn start(app: &AppHandle, config_dir: PathBuf, vault_dir: PathBuf) -> SyncState {
    let mut engine = Engine::new(config_dir, Box::new(OsKeyStore::SYNC)).with_vault(vault_dir);
    let home = termf_core::expand_tilde("~");
    if home.is_absolute() {
        engine = engine.with_home(home);
    }
    let engine = Arc::new(engine);
    let (tx, rx) = mpsc::channel();
    let (app, background) = (app.clone(), engine.clone());
    std::thread::spawn(move || run(app, background, rx));
    SyncState {
        engine,
        wake: Mutex::new(tx),
    }
}

pub fn hook(
    handler: impl Fn(Invoke) -> bool + Send + Sync + 'static,
) -> impl Fn(Invoke) -> bool + Send + Sync + 'static {
    move |invoke| {
        if WRITES.contains(&invoke.message.command()) {
            wake(invoke.message.webview_ref().app_handle(), Wake::Changed);
        }
        handler(invoke)
    }
}

pub fn focused(app: &AppHandle) {
    wake(app, Wake::Focused);
}

fn wake(app: &AppHandle, wake: Wake) {
    if let Some(state) = app.try_state::<SyncState>() {
        let _ = state.wake.lock().unwrap().send(wake);
    }
}

fn run(app: AppHandle, engine: Arc<Engine>, wake: Receiver<Wake>) {
    let mut due = Some(Instant::now());
    let mut last = Instant::now();
    loop {
        let next = due.unwrap_or(last + PERIOD);
        match wake.recv_timeout(next.saturating_duration_since(Instant::now())) {
            Ok(Wake::Changed) => due = Some(Instant::now() + DEBOUNCE),
            Ok(Wake::Focused) if due.is_none() && last.elapsed() >= FOCUS_GAP => {
                due = Some(Instant::now())
            }
            Ok(Wake::Focused) => {}
            Err(RecvTimeoutError::Timeout) => {
                due = None;
                last = Instant::now();
                // Locked: the UI asks for the password; trying would only fail again.
                if engine.status().is_ok_and(|s| s.enabled && s.unlocked) {
                    if let Ok(report) = engine.sync(true) {
                        applied(&app, &report);
                        merged(&app, &engine);
                    }
                    if let Ok(status) = engine.status() {
                        let _ = app.emit("sync-status", status);
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// Another machine's vault came in (by this sync or the CLI's): merged now when the vault is open, then pushed,
/// so the remote holds both.
fn merged(app: &AppHandle, engine: &Engine) {
    if crate::vault::synced(app) {
        let _ = engine.sync(true);
    }
}

/// What a pull brought that does not apply at once, for the UI to say (`sync-pulled`).
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Pulled {
    paths: Vec<String>,
    /// Settings read once at startup that changed: they apply when TermF starts again.
    restart: Vec<&'static str>,
    /// Agents already running keep the library and the conventions they started with.
    sessions: bool,
}

fn effects(paths: &[String], confirm_cli_changed: bool, agents_running: bool) -> Pulled {
    let settings = paths.iter().any(|p| p == "termf.toml");
    let launched_with = paths
        .iter()
        .any(|p| p.starts_with("library/") || p.starts_with("projects/"));
    Pulled {
        paths: paths.to_vec(),
        restart: if settings && confirm_cli_changed {
            vec!["confirmCli"]
        } else {
            Vec::new()
        },
        sessions: agents_running && launched_with,
    }
}

/// Applies what the backend holds of a pulled `termf.toml` (keep awake, the system-wide shortcut), then tells
/// the UI, which reloads the rest (projects, shortcuts, library, layouts…).
fn applied(app: &AppHandle, report: &Report) {
    if report.pulled.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    let settings = termf_core::load_settings(&state.config_dir).ok();
    if let Some(s) = settings.as_ref()
        && report.pulled.iter().any(|p| p == "termf.toml")
    {
        state
            .keep_awake
            .configure(app, s.keep_awake, s.keep_display_on);
        quick_terminal::follow(app, &s.global_shortcut());
    }
    let confirm_cli_changed =
        settings.is_some_and(|s| s.api.confirm_cli != state.api.confirm_cli());
    let agents_running = state
        .sessions
        .list()
        .iter()
        .any(|s| s.running && s.agent.is_some());
    let _ = app.emit(
        "sync-pulled",
        effects(&report.pulled, confirm_cli_changed, agents_running),
    );
}

/// Every command runs on the blocking pool: they reach the network, or derive a key from a password.
async fn blocking<T: Send + 'static>(
    state: &SyncState,
    f: impl FnOnce(&Engine) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let engine = state.engine.clone();
    tauri::async_runtime::spawn_blocking(move || f(&engine))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
pub async fn sync_status(state: State<'_, SyncState>) -> CmdResult<Status> {
    blocking(&state, |e| e.status()).await
}

#[tauri::command]
pub async fn sync_probe(state: State<'_, SyncState>, remote: RemoteSpec) -> CmdResult<bool> {
    blocking(&state, move |e| e.probe(&remote)).await
}

#[tauri::command]
pub async fn sync_setup(
    app: AppHandle,
    state: State<'_, SyncState>,
    remote: RemoteSpec,
    password: String,
    scopes: Vec<Scope>,
    device_name: Option<String>,
) -> CmdResult<Setup> {
    blocking(&state, move |e| {
        let setup = e.setup(remote, &password, &scopes, device_name.as_deref())?;
        applied(&app, &setup.report);
        Ok(setup)
    })
    .await
}

/// What joining `remote` would do here: nothing is applied.
#[tauri::command]
pub async fn sync_preview(
    state: State<'_, SyncState>,
    remote: RemoteSpec,
    password: String,
    scopes: Vec<Scope>,
) -> CmdResult<Report> {
    blocking(&state, move |e| e.preview(remote, &password, &scopes)).await
}

/// Puts back what the last sync changed here; what it put back waits as conflicts.
#[tauri::command]
pub async fn sync_undo(app: AppHandle, state: State<'_, SyncState>) -> CmdResult<Undone> {
    blocking(&state, move |e| {
        let undone = e.undo()?;
        let report = Report {
            pulled: undone.restored.clone(),
            ..Default::default()
        };
        applied(&app, &report);
        Ok(undone)
    })
    .await
}

#[tauri::command]
pub async fn sync_unlock(state: State<'_, SyncState>, password: String) -> CmdResult<()> {
    blocking(&state, move |e| e.unlock(&password)).await
}

#[tauri::command]
pub async fn sync_recover(
    state: State<'_, SyncState>,
    recovery_key: String,
    password: String,
) -> CmdResult<String> {
    blocking(&state, move |e| e.recover(&recovery_key, &password)).await
}

#[tauri::command]
pub async fn sync_push(app: AppHandle, state: State<'_, SyncState>) -> CmdResult<Report> {
    blocking(&state, move |e| {
        let report = e.sync(true)?;
        applied(&app, &report);
        merged(&app, e);
        Ok(report)
    })
    .await
}

#[tauri::command]
pub async fn sync_pull(app: AppHandle, state: State<'_, SyncState>) -> CmdResult<Report> {
    blocking(&state, move |e| {
        let report = e.sync(false)?;
        applied(&app, &report);
        crate::vault::synced(&app);
        Ok(report)
    })
    .await
}

#[tauri::command]
pub async fn sync_resolve(
    app: AppHandle,
    state: State<'_, SyncState>,
    path: String,
    resolution: Resolution,
) -> CmdResult<Report> {
    blocking(&state, move |e| {
        let report = e.resolve(&path, resolution)?;
        applied(&app, &report);
        Ok(report)
    })
    .await
}

/// Returns the new recovery key: the data key changed.
#[tauri::command]
pub async fn sync_forget(
    state: State<'_, SyncState>,
    id: String,
    password: String,
) -> CmdResult<String> {
    blocking(&state, move |e| e.forget(&id, &password)).await
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CloneLine {
    project: String,
    path: String,
    line: String,
}

/// Clones a missing repository into `target` (absolute); git's progress comes as `sync-clone` events.
#[tauri::command]
pub async fn sync_clone(
    app: AppHandle,
    state: State<'_, SyncState>,
    project: String,
    path: String,
    target: String,
) -> CmdResult<Status> {
    blocking(&state, move |e| {
        let target = PathBuf::from(target.trim());
        anyhow::ensure!(target.is_absolute(), "choose a full path to clone into");
        e.clone_repo(&project, &path, &target, &mut |line| {
            let _ = app.emit(
                "sync-clone",
                CloneLine {
                    project: project.clone(),
                    path: path.clone(),
                    line: line.to_owned(),
                },
            );
        })
    })
    .await
}

#[tauri::command]
pub async fn sync_devices(state: State<'_, SyncState>) -> CmdResult<Vec<Device>> {
    blocking(&state, |e| e.devices()).await
}

#[tauri::command]
pub async fn sync_change_password(
    state: State<'_, SyncState>,
    current: String,
    password: String,
) -> CmdResult<String> {
    blocking(&state, move |e| e.change_password(&current, &password)).await
}

#[tauri::command]
pub async fn sync_scopes(state: State<'_, SyncState>, scopes: Vec<Scope>) -> CmdResult<Status> {
    blocking(&state, move |e| {
        e.set_scopes(&scopes)?;
        e.status()
    })
    .await
}

#[tauri::command]
pub async fn sync_disable(state: State<'_, SyncState>) -> CmdResult<()> {
    blocking(&state, |e| e.disable()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_a_pull_cannot_apply_at_once() {
        let paths = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let quiet = effects(&paths(&["termf.toml", "layouts.toml"]), false, true);
        assert!(quiet.restart.is_empty() && !quiet.sessions);
        assert_eq!(
            effects(&paths(&["termf.toml"]), true, false).restart,
            ["confirmCli"]
        );
        assert!(
            effects(&paths(&["layouts.toml"]), true, false)
                .restart
                .is_empty()
        );
        let library = paths(&["library/skills/s/SKILL.md"]);
        assert!(effects(&library, false, true).sessions);
        assert!(!effects(&library, false, false).sessions);
        assert!(effects(&paths(&["projects/acme.toml"]), false, true).sessions);
    }
}
