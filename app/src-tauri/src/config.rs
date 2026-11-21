use tauri::State;
use termf_core::{Layout, Project};

use crate::{AppState, CmdResult, err};

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
