use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, State};
use termf_library::archive::{self, Exported, Preview};
use termf_library::conventions::{self, Answer, Convention, Conventions, Impact, Level, Review};
use termf_library::scout::{self, ItemRef, Scout};
use termf_library::{Imported, Item, Kind, Scope, stack};

use crate::{AppState, CmdResult, err};

/// `repo` without `project` takes the first project that lists the repository.
fn scope(state: &AppState, repo: Option<&str>, project: Option<String>) -> CmdResult<Scope> {
    Scope::new(&state.config_dir, repo.map(Path::new), project).map_err(err)
}

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

#[derive(serde::Serialize)]
pub struct StackView {
    #[serde(flatten)]
    repo: stack::RepoStack,
    mcp: Vec<stack::McpOffer>,
    sources: Vec<stack::SkillSource>,
}

/// Each repository of the project: what it is built with, what its packages ship, and what its vendors offer.
#[tauri::command]
pub async fn library_stack(
    state: State<'_, AppState>,
    project: String,
) -> CmdResult<Vec<StackView>> {
    let scope = scope(&state, None, Some(project))?;
    Ok(scope
        .stack
        .into_iter()
        .map(|repo| {
            let (mcp, sources) = stack::offers(&repo);
            StackView { repo, mcp, sources }
        })
        .collect())
}

/// `enabled: null` removes the toggle, so the item inherits.
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

/// The vault in a config export: sealed by its own password, the whole vault or one project's part.
#[derive(serde::Deserialize)]
pub struct VaultPart {
    password: String,
    project: Option<String>,
}

#[tauri::command]
pub async fn config_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    vault: Option<VaultPart>,
) -> CmdResult<Exported> {
    let sealed = match vault {
        Some(part) => {
            let password = termf_vault::Secret::new(part.password);
            let (app, project) = (app.clone(), part.project);
            let sealed = tauri::async_runtime::spawn_blocking(move || {
                crate::vault::sealed(&app, project.as_deref(), password.expose())
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(err)?;
            Some(sealed)
        }
        None => None,
    };
    archive::export_with_vault(&state.config_dir, Path::new(&path), sealed.as_deref()).map_err(err)
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

/// The program of the first Claude Code agent in the settings, else `claude`. It runs directly, with the login
/// shell's PATH (see `conventions::propose`).
pub(crate) fn claude(config_dir: &Path) -> Vec<String> {
    let settings = termf_core::load_settings(config_dir).unwrap_or_default();
    // shortcut: on Windows an npm `claude.cmd` is not found this way; go through `cmd /c` when that matters.
    vec![settings.claude_program()]
}

/// Runs a slow call (Claude Code, check scripts) off the async runtime.
pub(crate) async fn blocking<T: Send + 'static>(
    state: &AppState,
    call: impl FnOnce(&Path) -> anyhow::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let config_dir = state.config_dir.clone();
    tauri::async_runtime::spawn_blocking(move || call(&config_dir))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[tauri::command]
pub async fn conventions_list(
    state: State<'_, AppState>,
    project: String,
) -> CmdResult<Conventions> {
    conventions::list(&state.config_dir, &project).map_err(err)
}

#[tauri::command]
pub async fn conventions_save(
    state: State<'_, AppState>,
    project: String,
    slug: String,
    source: String,
) -> CmdResult<Convention> {
    conventions::save_source(&state.config_dir, &project, &slug, &source).map_err(err)
}

#[tauri::command]
pub async fn conventions_delete(
    state: State<'_, AppState>,
    project: String,
    slug: String,
) -> CmdResult<()> {
    conventions::delete(&state.config_dir, &project, &slug).map_err(err)
}

#[tauri::command]
pub async fn conventions_propose(
    state: State<'_, AppState>,
    project: String,
    slug: String,
    answers: Vec<Answer>,
) -> CmdResult<Convention> {
    blocking(&state, move |dir| {
        conventions::propose(dir, &project, &slug, answers, &claude(dir))
    })
    .await
}

#[tauri::command]
pub async fn conventions_revise(
    state: State<'_, AppState>,
    project: String,
    slug: String,
    name: String,
    comment: String,
) -> CmdResult<Convention> {
    blocking(&state, move |dir| {
        conventions::revise(dir, &project, &slug, &name, &comment, &claude(dir))
    })
    .await
}

#[tauri::command]
pub async fn conventions_review(
    state: State<'_, AppState>,
    project: String,
    slug: String,
    name: String,
    review: Option<Review>,
    level: Option<Level>,
) -> CmdResult<Convention> {
    conventions::review(&state.config_dir, &project, &slug, &name, review, level).map_err(err)
}

#[tauri::command]
pub async fn conventions_discard(
    state: State<'_, AppState>,
    project: String,
    slug: String,
) -> CmdResult<Convention> {
    conventions::discard(&state.config_dir, &project, &slug).map_err(err)
}

#[tauri::command]
pub async fn conventions_apply(
    state: State<'_, AppState>,
    project: String,
    slug: String,
) -> CmdResult<Convention> {
    conventions::apply(&state.config_dir, &project, &slug).map_err(err)
}

#[tauri::command]
pub async fn conventions_switch(
    state: State<'_, AppState>,
    project: String,
    slug: String,
    sentence: String,
    enabled: Option<bool>,
    level: Option<Level>,
) -> CmdResult<Convention> {
    conventions::switch(
        &state.config_dir,
        &project,
        &slug,
        &sentence,
        enabled,
        level,
    )
    .map_err(err)
}

#[tauri::command]
pub async fn conventions_impact(
    state: State<'_, AppState>,
    project: String,
    slug: String,
) -> CmdResult<Vec<Impact>> {
    blocking(&state, move |dir| conventions::impact(dir, &project, &slug)).await
}

#[tauri::command]
pub async fn scout_state(state: State<'_, AppState>, project: Option<String>) -> CmdResult<Scout> {
    scout::state(&state.config_dir, project.as_deref()).map_err(err)
}

/// `items` empty: the whole library. What Claude Code searches and reads comes as `scout-progress` events.
#[tauri::command]
pub async fn scout_run(
    app: AppHandle,
    state: State<'_, AppState>,
    project: Option<String>,
    items: Vec<ItemRef>,
) -> CmdResult<Scout> {
    blocking(&state, move |dir| {
        let options = scout::Options::default();
        let progress = &mut |p: scout::Progress| {
            let _ = app.emit("scout-progress", p);
        };
        scout::run(
            dir,
            project.as_deref(),
            &items,
            &options,
            &claude(dir),
            progress,
        )?;
        scout::state(dir, project.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn scout_revise(
    app: AppHandle,
    state: State<'_, AppState>,
    project: Option<String>,
    item: ItemRef,
    comment: String,
) -> CmdResult<Scout> {
    blocking(&state, move |dir| {
        let progress = &mut |p: scout::Progress| {
            let _ = app.emit("scout-progress", p);
        };
        scout::revise(
            dir,
            project.as_deref(),
            &item,
            &comment,
            &claude(dir),
            progress,
        )
    })
    .await
}

/// `confirm`: the owner read the script or hook of the entry and accepts it.
#[tauri::command]
pub async fn scout_review(
    state: State<'_, AppState>,
    project: Option<String>,
    item: ItemRef,
    review: Review,
    confirm: bool,
) -> CmdResult<Scout> {
    scout::review(
        &state.config_dir,
        project.as_deref(),
        &item,
        review,
        confirm,
    )
    .map_err(err)
}

#[tauri::command]
pub async fn scout_apply(state: State<'_, AppState>, project: Option<String>) -> CmdResult<Scout> {
    scout::apply(&state.config_dir, project.as_deref()).map_err(err)
}

#[tauri::command]
pub async fn scout_discard(
    state: State<'_, AppState>,
    project: Option<String>,
) -> CmdResult<Scout> {
    scout::discard(&state.config_dir, project.as_deref()).map_err(err)
}
