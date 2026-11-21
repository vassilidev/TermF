use std::collections::BTreeMap;

use serde::Serialize;
use tauri::{AppHandle, State};
use termf_core::{Layout, Project, QUICK_TERMINAL_GLOBAL};

use crate::{AppState, CmdResult, err, quick_terminal};

/// Creates (`id: None`) or updates a project, then returns every project.
#[tauri::command]
pub fn project_save(
    state: State<AppState>,
    id: Option<String>,
    name: String,
    repos: Vec<String>,
) -> CmdResult<Vec<Project>> {
    termf_core::save_project(&state.config_dir, id.as_deref(), &name, &repos).map_err(err)?;
    termf_core::load_projects(&state.config_dir).map_err(err)
}

#[tauri::command]
pub fn project_delete(state: State<AppState>, id: String) -> CmdResult<Vec<Project>> {
    termf_core::delete_project(&state.config_dir, &id).map_err(err)?;
    termf_core::load_projects(&state.config_dir).map_err(err)
}

#[tauri::command]
pub fn layouts(state: State<AppState>) -> CmdResult<Vec<Layout>> {
    termf_core::load_layouts(&state.config_dir).map_err(err)
}

#[tauri::command]
pub fn layout_save(state: State<AppState>, layout: Layout) -> CmdResult<Vec<Layout>> {
    termf_core::save_layout(&state.config_dir, layout).map_err(err)?;
    layouts(state)
}

#[tauri::command]
pub fn layout_delete(state: State<AppState>, name: String) -> CmdResult<Vec<Layout>> {
    termf_core::delete_layout(&state.config_dir, &name).map_err(err)?;
    layouts(state)
}

/// The shortcuts changed in the UI (`[keys]`), and why the system-wide one is not registered.
#[derive(Serialize)]
pub struct Keys {
    keys: BTreeMap<String, String>,
    error: Option<String>,
}

#[tauri::command]
pub fn keys(state: State<AppState>) -> CmdResult<Keys> {
    let settings = termf_core::load_settings(&state.config_dir).map_err(err)?;
    Ok(Keys {
        keys: settings.key_overrides(),
        error: state.quick_terminal.error.lock().unwrap().clone(),
    })
}

/// Saves every changed shortcut. A new system-wide shortcut is registered first: if the system refuses it,
/// nothing is saved and the error says why.
#[tauri::command]
pub fn keys_set(
    app: AppHandle,
    state: State<AppState>,
    keys: BTreeMap<String, String>,
) -> CmdResult<Keys> {
    let before = termf_core::load_settings(&state.config_dir)
        .map_err(err)?
        .global_shortcut();
    let after = keys.get(QUICK_TERMINAL_GLOBAL).map_or("", |s| s.trim());
    if before != after {
        quick_terminal::replace(&app, &before, after).map_err(err)?;
    }
    if let Err(e) = termf_core::save_keys(&state.config_dir, &keys) {
        if before != after {
            let _ = quick_terminal::replace(&app, after, &before);
        }
        return Err(err(e));
    }
    self::keys(state)
}
