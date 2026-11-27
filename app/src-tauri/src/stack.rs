//! The Stack view: the map of known repositories read from their files, then, per repository, what the registries,
//! OSV.dev and endoflife.date say of its versions (kept a day in `<data>/stack-cache.json`), off the main thread.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;
use termf_files::Files;
use termf_stack::{Check, StackMap};

use crate::{CmdResult, err};

#[tauri::command]
pub async fn stack_map(files: State<'_, Arc<Files>>, repos: Vec<String>) -> CmdResult<StackMap> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for repo in repos {
        let (root, _) = files.resolve(&repo, ".").map_err(err)?;
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        found.push((name, root));
    }
    tauri::async_runtime::spawn_blocking(move || termf_stack::map(&found))
        .await
        .map_err(|e| e.to_string())
}

/// `refresh`: asks again what is younger than a day.
#[tauri::command]
pub async fn stack_check(
    files: State<'_, Arc<Files>>,
    repo: String,
    refresh: bool,
) -> CmdResult<Check> {
    let (root, _) = files.resolve(&repo, ".").map_err(err)?;
    let cache = termf_core::default_data_dir()
        .map_err(err)?
        .join("stack-cache.json");
    tauri::async_runtime::spawn_blocking(move || {
        termf_stack::check(&root, &cache, refresh, &termf_stack::curl)
    })
    .await
    .map_err(|e| e.to_string())
}
