use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Context as _;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};
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

/// Whether the project's sessions get the language servers of its stack, after writing `enabled` when given.
#[tauri::command]
pub fn library_lsp(
    state: State<AppState>,
    project: String,
    enabled: Option<bool>,
) -> CmdResult<bool> {
    if let Some(on) = enabled {
        termf_library::set_language_servers(&state.config_dir, &project, on).map_err(err)?;
    }
    Ok(termf_library::language_servers_on(
        &state.config_dir,
        Some(&project),
    ))
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

/// A copy of the item in the project's library (the global one without `project`) to edit there; its path.
#[tauri::command]
pub async fn library_copy(
    state: State<'_, AppState>,
    kind: Kind,
    name: String,
    project: Option<String>,
) -> CmdResult<PathBuf> {
    let scope = scope(&state, None, project)?;
    termf_library::copy_here(&state.config_dir, &scope, kind, &name).map_err(err)
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

/// The vault in a config export: sealed by its own password, the whole vault or one project's part, once the
/// master password is typed again.
#[derive(serde::Deserialize)]
pub struct VaultPart {
    password: String,
    project: Option<String>,
    master: String,
}

#[tauri::command]
pub async fn config_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    vault: Option<VaultPart>,
) -> CmdResult<Exported> {
    let Some(part) = vault else {
        return archive::export(&state.config_dir, Path::new(&path)).map_err(err);
    };
    let password = termf_vault::Secret::new(part.password);
    let master = termf_vault::Secret::new(part.master);
    let (config_dir, project) = (state.config_dir.clone(), part.project);
    // The vault's export entry is written once the archive is.
    tauri::async_runtime::spawn_blocking(move || {
        let mut exported = None;
        let result = crate::vault::sealed(
            &app,
            project.as_deref(),
            password.expose(),
            master.expose(),
            &mut |bytes| {
                exported = Some(archive::export_with_vault(
                    &config_dir,
                    Path::new(&path),
                    Some(bytes),
                )?);
                Ok(())
            },
        );
        if result.is_err() && exported.is_some() {
            let _ = std::fs::remove_file(&path);
        }
        result?;
        exported.ok_or_else(|| anyhow::anyhow!("nothing exported"))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
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

/// What Claude Code reads while it proposes comes as `conventions-progress` events.
fn progress(app: AppHandle, project: &str, slug: &str) -> impl FnMut(conventions::Progress) {
    let (project, slug) = (project.to_owned(), slug.to_owned());
    move |p| {
        let _ = app.emit(
            "conventions-progress",
            serde_json::json!({"project": project, "slug": slug, "tool": p.tool, "target": p.target}),
        );
    }
}

#[tauri::command]
pub async fn conventions_propose(
    app: AppHandle,
    state: State<'_, AppState>,
    project: String,
    slug: String,
    answers: Vec<Answer>,
) -> CmdResult<Convention> {
    blocking(&state, move |dir| {
        let mut progress = progress(app, &project, &slug);
        conventions::propose_with(dir, &project, &slug, answers, &claude(dir), &mut progress)
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
pub async fn conventions_restore(
    state: State<'_, AppState>,
    project: String,
    slug: String,
) -> CmdResult<Convention> {
    conventions::restore(&state.config_dir, &project, &slug).map_err(err)
}

#[tauri::command]
pub async fn conventions_apply(
    state: State<'_, AppState>,
    project: String,
    slug: String,
    repos: Option<bool>,
) -> CmdResult<Convention> {
    conventions::apply_with(&state.config_dir, &project, &slug, repos.unwrap_or(true)).map_err(err)
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

/// What stops the scout's launch in progress: one at a time, from the Library or the CTO, so a cancel never
/// stops another.
static SCOUT_CANCEL: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Frees the launch slot however the launch ends.
struct Launching;

impl Drop for Launching {
    fn drop(&mut self) {
        *SCOUT_CANCEL.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// A launch of the scout, which `scout_cancel` stops; refused while another runs. What Claude Code searches and
/// reads comes as `scout-progress` events.
pub(crate) fn launch_scout(
    app: &AppHandle,
    dir: &Path,
    project: Option<&str>,
    items: &[ItemRef],
) -> anyhow::Result<scout::Draft> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = SCOUT_CANCEL.lock().unwrap_or_else(|e| e.into_inner());
        anyhow::ensure!(
            slot.is_none(),
            "the watch is already running: wait for it, or cancel it"
        );
        *slot = Some(cancel.clone());
    }
    let _launching = Launching;
    let options = scout::Options {
        cancel: Some(cancel),
        ..scout::Options::default()
    };
    let progress = &mut |p: scout::Progress| {
        let _ = app.emit("scout-progress", p);
    };
    scout::run(dir, project, items, &options, &claude(dir), progress)
}

/// `run_scout` of the CTO's conversation: the project's watch, as the Library launches it.
pub(crate) fn mcp_scout(app: &AppHandle, project: &str, items: &[String]) -> anyhow::Result<Value> {
    let dir = app.state::<AppState>().config_dir.clone();
    let refs = items
        .iter()
        .map(|i| {
            let (kind, name) = i
                .split_once('/')
                .with_context(|| format!("{i}: give kind/name"))?;
            Ok(ItemRef {
                kind: serde_json::from_value(json!(kind))?,
                name: name.into(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    // Results the user has not decided on yet are theirs: never replaced from here.
    if let Some(draft) = scout::state(&dir, Some(project))?.draft {
        let waiting = scout::would_replace(&draft, &refs);
        anyhow::ensure!(
            waiting == 0,
            "{waiting} results of the last watch still wait for the user's decision in the Library, Watch tab: \
             a new launch would replace them, so the user decides on them first"
        );
    }
    let draft = launch_scout(app, &dir, Some(project), &refs)?;
    let _ = app.emit("library-changed", ());
    let results: Vec<Value> = draft
        .entries
        .iter()
        .map(|e| {
            json!({ "kind": e.kind, "name": e.name, "flaws": e.critique.iter().map(|f| &f.flaw).collect::<Vec<_>>(),
                    "proposes": !e.content.is_empty() || !e.script.is_empty(), "summary": e.summary })
        })
        .collect();
    Ok(json!({ "results": results, "cost": draft.cost,
               "next": "The user reviews each result in the Library, Watch tab: accept, reject (remembered) or ask for a change, then apply." }))
}

/// `propose_conventions` of the CTO's conversation: always a new convention, never over one the user wrote.
pub(crate) fn mcp_propose(
    app: &AppHandle,
    project: &str,
    text: Option<String>,
    name: Option<String>,
) -> anyhow::Result<Value> {
    let dir = app.state::<AppState>().config_dir.clone();
    let text = text
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| conventions::FROM_CODE.to_owned());
    let mut base: String = name
        .unwrap_or_default()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    base.truncate(48);
    if base.is_empty() {
        base = "from-code".into();
    }
    let taken: Vec<String> = conventions::list(&dir, project)?
        .list
        .into_iter()
        .map(|c| c.slug)
        .collect();
    let slug = (1..)
        .map(|n| match n {
            1 => base.clone(),
            n => format!("{base}-{n}"),
        })
        .find(|s| !taken.contains(s))
        .unwrap_or(base);
    conventions::save_source(&dir, project, &slug, &text)?;
    let mut progress = progress(app.clone(), project, &slug);
    let proposed = conventions::propose_with(
        &dir,
        project,
        &slug,
        Vec::new(),
        &claude(&dir),
        &mut progress,
    )?;
    let _ = app.emit("library-changed", ());
    let draft = proposed.draft.unwrap_or_default();
    let items: Vec<Value> = draft
        .items
        .iter()
        .map(|i| json!({ "kind": i.kind, "name": i.name, "description": i.description }))
        .collect();
    Ok(
        json!({ "convention": slug, "questions": draft.questions, "items": items, "cost": draft.cost,
               "next": "The user answers the questions, or reviews each item, then applies them, in the Library, Conventions tab." }),
    )
}

/// `items` empty: the whole library.
#[tauri::command]
pub async fn scout_run(
    app: AppHandle,
    state: State<'_, AppState>,
    project: Option<String>,
    items: Vec<ItemRef>,
) -> CmdResult<Scout> {
    blocking(&state, move |dir| {
        launch_scout(&app, dir, project.as_deref(), &items)?;
        scout::state(dir, project.as_deref())
    })
    .await
}

#[tauri::command]
pub fn scout_cancel() {
    if let Some(cancel) = SCOUT_CANCEL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        cancel.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn scout_restore(
    state: State<'_, AppState>,
    project: Option<String>,
) -> CmdResult<Scout> {
    scout::restore(&state.config_dir, project.as_deref()).map_err(err)
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
