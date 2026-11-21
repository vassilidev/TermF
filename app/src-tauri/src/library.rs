use std::path::{Path, PathBuf};

use tauri::State;
use termf_library::archive::{self, Preview};
use termf_library::{Imported, Item, Kind, Scope};

use crate::{AppState, CmdResult, err};

/// `repo` without `project` takes the first project that lists the repository.
fn scope(state: &AppState, repo: Option<&str>, project: Option<String>) -> CmdResult<Scope> {
    Scope::new(&state.config_dir, repo.map(Path::new), project).map_err(err)
}

/// Every item in the layers of `repo` / `project`, each with the layer that provides it and its state.
#[tauri::command]
pub async fn library_list(
    state: State<'_, AppState>,
    kind: Option<Kind>,
    repo: Option<String>,
    project: Option<String>,
) -> CmdResult<Vec<Item>> {
    let scope = scope(&state, repo.as_deref(), project)?;
    let items = termf_library::resolve(&state.config_dir, &scope).map_err(err)?;
    Ok(items
        .into_iter()
        .filter(|i| kind.is_none_or(|k| i.kind == k))
        .collect())
}

/// Switches an item on (`enabled: true`), off, or back to inheriting (`null`), globally or for `project`.
#[tauri::command]
pub fn library_toggle(
    state: State<AppState>,
    kind: Kind,
    name: String,
    project: Option<String>,
    enabled: Option<bool>,
) -> CmdResult<()> {
    termf_library::set_toggle(&state.config_dir, project.as_deref(), kind, &name, enabled)
        .map_err(err)
}

#[tauri::command]
pub async fn library_explain(
    state: State<'_, AppState>,
    kind: Kind,
    name: String,
    repo: Option<String>,
    project: Option<String>,
) -> CmdResult<Item> {
    let scope = scope(&state, repo.as_deref(), project)?;
    termf_library::explain(&state.config_dir, &scope, kind, &name).map_err(err)
}

/// Imports `~/.claude` (or `<repo>/.claude`) into the global library (or `project`'s).
#[tauri::command]
pub async fn library_import_claude(
    state: State<'_, AppState>,
    repo: Option<String>,
    project: Option<String>,
    overwrite: bool,
) -> CmdResult<Vec<Imported>> {
    let from = match repo {
        Some(repo) => PathBuf::from(repo).join(".claude"),
        None => termf_core::expand_tilde("~/.claude"),
    };
    let library = termf_library::library_dir(&state.config_dir, project.as_deref()).map_err(err)?;
    termf_library::import_claude(&from, &library, overwrite).map_err(err)
}

/// Writes the config and the library to `path`; returns the number of files.
#[tauri::command]
pub async fn config_export(state: State<'_, AppState>, path: String) -> CmdResult<usize> {
    archive::export(&state.config_dir, Path::new(&path)).map_err(err)
}

#[tauri::command]
pub async fn config_import_preview(state: State<'_, AppState>, path: String) -> CmdResult<Preview> {
    archive::preview(Path::new(&path), &state.config_dir).map_err(err)
}

/// Items in `replace` take the archive's version; the others are merged.
#[tauri::command]
pub async fn config_import_apply(
    state: State<'_, AppState>,
    path: String,
    replace: Vec<String>,
) -> CmdResult<Preview> {
    archive::apply(Path::new(&path), &state.config_dir, &replace).map_err(err)
}
