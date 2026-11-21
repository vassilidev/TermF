//! Context reset settings and the reset button (ARCHITECTURE §6.5).

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{AppState, CmdResult, err};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSettings {
    on: bool,
    reset_at: u8,
    warning_at: u8,
}

#[tauri::command]
pub fn context_settings(state: State<AppState>) -> CmdResult<ContextSettings> {
    let settings = termf_core::load_settings(&state.config_dir).map_err(err)?;
    Ok(ContextSettings {
        on: settings.context_reset,
        reset_at: settings.context_reset_at,
        warning_at: settings.context_warning_at,
    })
}

#[tauri::command]
pub fn context_settings_set(
    state: State<AppState>,
    on: bool,
    reset_at: u8,
    warning_at: u8,
) -> CmdResult<ContextSettings> {
    termf_core::save_context_reset(&state.config_dir, on, reset_at, warning_at).map_err(err)?;
    context_settings(state)
}

#[tauri::command]
pub fn context_reset(app: AppHandle, state: State<AppState>, id: u32) -> CmdResult<()> {
    state.api.request_reset(&app, id).map_err(err)
}
