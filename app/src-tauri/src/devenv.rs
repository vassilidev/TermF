//! My environment: the machine's development environment (`termf_core::devenv`) for its screen. Detection streams
//! each source as it ends (`env-scan-started`, then `env-scanned`); the install script runs in a terminal tile.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use termf_core::devenv::{self, Entry, Event, Found, Key, Machine};
use termf_core::ipc::SessionSummary;

use crate::{AppState, CmdResult, api, err};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    entries: Vec<Entry>,
    /// The named scripts kept beside it.
    scripts: Vec<String>,
    file: String,
}

fn manifest(config_dir: &Path) -> Result<Manifest> {
    Ok(Manifest {
        entries: devenv::load(config_dir)?,
        scripts: devenv::scripts(config_dir),
        file: config_dir.join(devenv::FILE).to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn env_manifest(state: State<AppState>) -> CmdResult<Manifest> {
    manifest(&state.config_dir).map_err(err)
}

static SCANNING: AtomicBool = AtomicBool::new(false);

/// Only reads: each manager present lists what it installed.
#[tauri::command]
pub async fn env_scan(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Vec<Found>> {
    if SCANNING.swap(true, Ordering::SeqCst) {
        return Err("TermF is already looking at what this machine has".into());
    }
    let config_dir = state.config_dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let manifest = devenv::load(&config_dir).unwrap_or_default();
        devenv::scan(&Machine::current(), &manifest, &|e| match e {
            Event::Started(sources) => {
                let _ = app.emit("env-scan-started", sources);
            }
            Event::Scanned(scanned) => {
                let _ = app.emit("env-scanned", scanned);
            }
        })
    })
    .await
    .map_err(|e| e.to_string());
    SCANNING.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
pub fn env_add(state: State<AppState>, entries: Vec<Entry>) -> CmdResult<Manifest> {
    devenv::add(&state.config_dir, entries).map_err(err)?;
    manifest(&state.config_dir).map_err(err)
}

#[tauri::command]
pub fn env_update(state: State<AppState>, previous: Key, entry: Entry) -> CmdResult<Manifest> {
    devenv::update(&state.config_dir, &previous, entry).map_err(err)?;
    manifest(&state.config_dir).map_err(err)
}

#[tauri::command]
pub fn env_remove(state: State<AppState>, keys: Vec<Key>) -> CmdResult<Manifest> {
    devenv::remove(&state.config_dir, &keys).map_err(err)?;
    manifest(&state.config_dir).map_err(err)
}

/// The script written from the manifest, or a named one.
#[tauri::command]
pub fn env_script(state: State<AppState>, name: Option<String>) -> CmdResult<String> {
    script(&state.config_dir, name.as_deref()).map_err(err)
}

fn script(config_dir: &Path, name: Option<&str>) -> Result<String> {
    match name {
        Some(name) => devenv::read_script(config_dir, name),
        None => Ok(devenv::script(&devenv::load(config_dir)?)),
    }
}

#[tauri::command]
pub fn env_script_save(state: State<AppState>, name: String, text: String) -> CmdResult<Manifest> {
    devenv::save_script(&state.config_dir, &name, &text).map_err(err)?;
    manifest(&state.config_dir).map_err(err)
}

#[tauri::command]
pub fn env_script_delete(state: State<AppState>, name: String) -> CmdResult<Manifest> {
    devenv::delete_script(&state.config_dir, &name).map_err(err)?;
    manifest(&state.config_dir).map_err(err)
}

/// Runs the script (the one written from the manifest, or a named one) in a terminal tile, from the home folder.
#[tauri::command]
pub fn env_install(state: State<AppState>, name: Option<String>) -> CmdResult<SessionSummary> {
    if cfg!(windows) {
        return Err("the install script is for macOS and Linux".into());
    }
    let text = script(&state.config_dir, name.as_deref()).map_err(err)?;
    let file = termf_core::default_data_dir()
        .map_err(err)?
        .join("env-install.sh");
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    let command = format!("sh {}", termf_term::shell_quote(&file.to_string_lossy()));
    let title = match &name {
        Some(name) => format!("termf env install {name}"),
        None => "termf env install".into(),
    };
    api::spawn_command(&state, termf_core::expand_tilde("~"), title, &command).map_err(err)
}
