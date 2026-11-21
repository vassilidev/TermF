//! A project's worktree setup recipe (ARCHITECTURE §9), seen and edited in the library, and running it again on
//! a run's worktree.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};
use termf_tickets::{Event, Run, RunStatus};
use termf_worktree::{Recipe, RecipeView, Report, Step};

use crate::library::{blocking, claude};
use crate::tickets::{tickets, updated};
use crate::{AppState, CmdResult, err};

/// A run's worktree and its last setup.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSetup {
    run: Run,
    /// When it last ran, in seconds since the Unix epoch; `None` before any setup.
    at: Option<i64>,
    steps: Vec<Step>,
    saved: bool,
}

#[tauri::command]
pub async fn worktree_recipe(state: State<'_, AppState>, project: String) -> CmdResult<RecipeView> {
    blocking(&state, move |dir| termf_worktree::view(dir, &project)).await
}

#[tauri::command]
pub async fn worktree_recipe_save(
    state: State<'_, AppState>,
    project: String,
    recipe: Recipe,
) -> CmdResult<RecipeView> {
    blocking(&state, move |dir| {
        termf_worktree::save(dir, &project, recipe)
    })
    .await
}

#[tauri::command]
pub async fn worktree_recipe_propose(
    state: State<'_, AppState>,
    project: String,
) -> CmdResult<RecipeView> {
    blocking(&state, move |dir| {
        termf_worktree::propose(dir, &project, &claude(dir))
    })
    .await
}

#[tauri::command]
pub async fn worktree_recipe_discard(
    state: State<'_, AppState>,
    project: String,
) -> CmdResult<RecipeView> {
    blocking(&state, move |dir| termf_worktree::discard(dir, &project)).await
}

/// The project's runs that still have their worktree, newest first.
#[tauri::command]
pub async fn worktree_runs(app: AppHandle, project: String) -> CmdResult<Vec<RunSetup>> {
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Vec<RunSetup>> {
        let tickets = tickets(&app);
        let mut found = Vec::new();
        for run in tickets.runs()? {
            let open = matches!(run.status, RunStatus::Running | RunStatus::Delivered);
            if run.project != project || !open || run.worktree.is_none() {
                continue;
            }
            let last = tickets
                .events(run.id)?
                .into_iter()
                .rev()
                .find_map(|e| match e.event {
                    Event::Setup { steps, saved } => Some((e.at, steps, saved)),
                    _ => None,
                });
            let (at, steps, saved) = match last {
                Some((at, steps, saved)) => (Some(at), steps, saved),
                None => (None, Vec::new(), false),
            };
            found.push(RunSetup {
                run,
                at,
                steps,
                saved,
            });
        }
        Ok(found)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}

/// Runs the setup again in the worktree at `path`; a run's journal records it.
#[tauri::command]
pub async fn worktree_setup(app: AppHandle, path: String) -> CmdResult<Report> {
    tauri::async_runtime::spawn_blocking(move || {
        let (run, report) = tickets(&app).prepare(&PathBuf::from(path))?;
        if let Some(run) = run {
            updated(&app, &run);
        }
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}
