use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;
use termf_files::{Content, Entry, Files, Opened, Saved, SearchResult};

use crate::{AppState, CmdResult, err};

/// Runs disk-heavy work on the blocking pool so neither the main thread nor the async workers stall.
async fn blocking<T: Send + 'static>(
    files: &Arc<Files>,
    f: impl FnOnce(&Files) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let files = files.clone();
    tauri::async_runtime::spawn_blocking(move || f(&files))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
pub async fn files_list(
    files: State<'_, Arc<Files>>,
    repo: String,
    dir: String,
) -> CmdResult<Vec<Entry>> {
    blocking(&files, move |f| f.list(&repo, &dir)).await
}

#[tauri::command]
pub async fn files_status(
    files: State<'_, Arc<Files>>,
    repo: String,
) -> CmdResult<HashMap<String, char>> {
    blocking(&files, move |f| f.status(&repo)).await
}

#[tauri::command]
pub async fn files_find(
    files: State<'_, Arc<Files>>,
    repo: String,
    query: String,
    fresh: bool,
) -> CmdResult<Vec<String>> {
    blocking(&files, move |f| f.find(&repo, &query, fresh)).await
}

#[tauri::command]
pub async fn files_search(
    files: State<'_, Arc<Files>>,
    repo: String,
    query: String,
    regex: bool,
) -> CmdResult<SearchResult> {
    blocking(&files, move |f| f.search(&repo, &query, regex)).await
}

#[tauri::command]
pub async fn files_read(
    files: State<'_, Arc<Files>>,
    repo: String,
    path: String,
) -> CmdResult<Opened> {
    blocking(&files, move |f| f.read(&repo, &path)).await
}

#[tauri::command]
pub async fn files_write(
    files: State<'_, Arc<Files>>,
    repo: String,
    path: String,
    text: String,
    mtime: Option<u64>,
) -> CmdResult<Saved> {
    blocking(&files, move |f| f.write(&repo, &path, &text, mtime)).await
}

#[tauri::command]
pub async fn files_head(
    files: State<'_, Arc<Files>>,
    repo: String,
    path: String,
) -> CmdResult<Option<Content>> {
    blocking(&files, move |f| f.head(&repo, &path)).await
}

#[tauri::command]
pub async fn files_open_external(
    state: State<'_, AppState>,
    files: State<'_, Arc<Files>>,
    repo: String,
    path: String,
) -> CmdResult<()> {
    let editor = termf_core::load_settings(&state.config_dir)
        .map_err(err)?
        .editor;
    blocking(&files, move |f| {
        f.open_external(&repo, &path, editor.as_deref())
    })
    .await
}
