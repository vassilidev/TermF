use std::collections::BTreeMap;

use serde::Serialize;
use tauri::{AppHandle, State};
use termf_core::{Layout, Project, QUICK_TERMINAL_GLOBAL};

use crate::{AppState, CmdResult, err, quick_terminal};

/// Creates (`id: None`) or updates a project, then returns every project. `checks`: the repositories' check
/// commands by path, an empty one removed.
#[tauri::command]
pub fn project_save(
    state: State<AppState>,
    id: Option<String>,
    name: String,
    repos: Vec<String>,
    checks: Option<BTreeMap<String, String>>,
) -> CmdResult<Vec<Project>> {
    let saved =
        termf_core::save_project(&state.config_dir, id.as_deref(), &name, &repos).map_err(err)?;
    if let Some(checks) = checks {
        termf_core::save_checks(&state.config_dir, &saved, &checks).map_err(err)?;
    }
    if id.is_none() {
        crate::onboarding::created(&state.config_dir, &saved);
    }
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

/// Also follows a system-wide shortcut that changed in termf.toml behind the app's back (sync, reload).
#[tauri::command]
pub fn keys(app: AppHandle, state: State<AppState>) -> CmdResult<Keys> {
    let settings = termf_core::load_settings(&state.config_dir).map_err(err)?;
    quick_terminal::follow(&app, &settings.global_shortcut());
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
    let saved = termf_core::load_settings(&state.config_dir)
        .map_err(err)?
        .global_shortcut();
    let after = keys.get(QUICK_TERMINAL_GLOBAL).map_or("", |s| s.trim());
    // Only when it changed: a shortcut the system already refused must not block saving the others.
    let previous = if saved != after {
        Some(quick_terminal::replace(&app, after).map_err(err)?)
    } else {
        None
    };
    if let Err(e) = termf_core::save_keys(&state.config_dir, &keys) {
        if let Some(previous) = previous {
            let _ = quick_terminal::replace(&app, &previous);
        }
        return Err(err(e));
    }
    self::keys(app, state)
}
