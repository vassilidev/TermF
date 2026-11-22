//! The Design system view: a known repository's tokens, components, inconsistencies and brand guidelines, read
//! off the main thread; and the guidelines' PDF export.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, ensure};
use tauri::State;
use termf_design::Design;
use termf_files::Files;

use crate::{CmdResult, err};

#[tauri::command]
pub async fn design_scan(files: State<'_, Arc<Files>>, repo: String) -> CmdResult<Design> {
    let (root, _) = files.resolve(&repo, ".").map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || termf_design::scan(&root))
        .await
        .map_err(|e| e.to_string())
}

/// `path`: where the user chose to save it.
#[tauri::command]
pub async fn design_pdf(files: State<'_, Arc<Files>>, repo: String, path: String) -> CmdResult<()> {
    let (root, _) = files.resolve(&repo, ".").map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        ensure!(
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf")),
            "the export is a .pdf file"
        );
        let pdf = termf_design::brand::pdf(&root)?;
        let tmp = path.with_extension("pdf.tmp");
        fs::write(&tmp, pdf)
            .and_then(|()| fs::rename(&tmp, &path))
            .with_context(|| format!("cannot write {}", path.display()))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}
