//! Encrypted sync between machines: the commands for the UI, and the background loop that pushes a few seconds
//! after the config changes, and pulls at startup, when the window gets focus and every 5 minutes. No file
//! system polling: the loop sleeps until one of those happens.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::ipc::Invoke;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_sync::{
    Device, Engine, OsKeyStore, RemoteSpec, Report, Resolution, Scope, Setup, Status,
};

use crate::{CmdResult, err};

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
    "library_toggle",
    "library_import_claude",
    "config_import_apply",
];

enum Wake {
    Changed,
    Focused,
}

pub struct SyncState {
    engine: Arc<Engine>,
    wake: Mutex<Sender<Wake>>,
}

pub fn start(app: &AppHandle, config_dir: PathBuf) -> SyncState {
    let engine = Arc::new(Engine::new(config_dir, Box::new(OsKeyStore)));
    let (tx, rx) = mpsc::channel();
    let (app, background) = (app.clone(), engine.clone());
    std::thread::spawn(move || run(app, background, rx));
    SyncState {
        engine,
        wake: Mutex::new(tx),
    }
}

/// Wraps the command handler: a push follows the commands that write the config.
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
                    let _ = engine.sync(true);
                    if let Ok(status) = engine.status() {
                        let _ = app.emit("sync-status", status);
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
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

/// Whether the remote already holds a TermF sync: setting up then joins it with its password.
#[tauri::command]
pub async fn sync_probe(state: State<'_, SyncState>, remote: RemoteSpec) -> CmdResult<bool> {
    blocking(&state, move |e| e.probe(&remote)).await
}

#[tauri::command]
pub async fn sync_setup(
    state: State<'_, SyncState>,
    remote: RemoteSpec,
    password: String,
    scopes: Vec<Scope>,
    device_name: Option<String>,
) -> CmdResult<Setup> {
    blocking(&state, move |e| {
        e.setup(remote, &password, &scopes, device_name.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn sync_unlock(state: State<'_, SyncState>, password: String) -> CmdResult<()> {
    blocking(&state, move |e| e.unlock(&password)).await
}

/// Returns the new recovery key.
#[tauri::command]
pub async fn sync_recover(
    state: State<'_, SyncState>,
    recovery_key: String,
    password: String,
) -> CmdResult<String> {
    blocking(&state, move |e| e.recover(&recovery_key, &password)).await
}

#[tauri::command]
pub async fn sync_push(state: State<'_, SyncState>) -> CmdResult<Report> {
    blocking(&state, |e| e.sync(true)).await
}

#[tauri::command]
pub async fn sync_pull(state: State<'_, SyncState>) -> CmdResult<Report> {
    blocking(&state, |e| e.sync(false)).await
}

#[tauri::command]
pub async fn sync_resolve(
    state: State<'_, SyncState>,
    path: String,
    resolution: Resolution,
) -> CmdResult<Report> {
    blocking(&state, move |e| e.resolve(&path, resolution)).await
}

#[tauri::command]
pub async fn sync_devices(state: State<'_, SyncState>) -> CmdResult<Vec<Device>> {
    blocking(&state, |e| e.devices()).await
}

/// Returns the new recovery key.
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
