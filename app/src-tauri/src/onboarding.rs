use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tauri::State;
use termf_claude::trust::{self, RepoTrust};
use termf_onboarding::{self as onboarding, Decision, Step};

use crate::library::{blocking, claude};
use crate::{AppState, CmdResult, err};

fn data_dir() -> anyhow::Result<PathBuf> {
    termf_core::default_data_dir()
}

/// For a project being created: the assistant opens by itself once.
pub(crate) fn created(config_dir: &Path, project: &str) {
    if let Err(e) = data_dir().and_then(|data| onboarding::created(config_dir, &data, project)) {
        eprintln!("termf: onboarding of {project}: {e:#}");
    }
}

#[tauri::command]
pub async fn onboarding_state(
    state: State<'_, AppState>,
    project: String,
) -> CmdResult<onboarding::State> {
    onboarding::load(&state.config_dir, &data_dir().map_err(err)?, &project).map_err(err)
}

/// The survey, then Claude Code read-only for its findings and questions (`lang`: `fr` or `en`).
#[tauri::command]
pub async fn onboarding_analyze(
    state: State<'_, AppState>,
    project: String,
    lang: String,
) -> CmdResult<onboarding::State> {
    blocking(&state, move |dir| {
        onboarding::analyze(dir, &data_dir()?, &project, &lang, &claude(dir))
    })
    .await
}

/// `replace`: the onboarding convention was edited in the library since, and the owner chose to replace it.
#[tauri::command]
pub async fn onboarding_answer(
    state: State<'_, AppState>,
    project: String,
    answers: BTreeMap<String, Vec<String>>,
    replace: Option<bool>,
) -> CmdResult<onboarding::State> {
    onboarding::answer(
        &state.config_dir,
        &data_dir().map_err(err)?,
        &project,
        answers,
        replace.unwrap_or(false),
    )
    .map_err(err)
}

#[tauri::command]
pub async fn onboarding_step(
    state: State<'_, AppState>,
    project: String,
    step: Step,
) -> CmdResult<onboarding::State> {
    onboarding::step(&state.config_dir, &data_dir().map_err(err)?, &project, step).map_err(err)
}

#[tauri::command]
pub async fn onboarding_seen(
    state: State<'_, AppState>,
    project: String,
) -> CmdResult<onboarding::State> {
    onboarding::seen(&state.config_dir, &data_dir().map_err(err)?, &project).map_err(err)
}

/// `proposal`: `tickets`, `worktree`, `ci` or `team`, once applied or refused with its own command.
#[tauri::command]
pub async fn onboarding_decide(
    state: State<'_, AppState>,
    project: String,
    proposal: String,
    decision: Decision,
    detail: String,
) -> CmdResult<onboarding::State> {
    onboarding::decide(
        &state.config_dir,
        &data_dir().map_err(err)?,
        &project,
        &proposal,
        decision,
        &detail,
    )
    .map_err(err)
}

#[tauri::command]
pub async fn claude_trust(
    state: State<'_, AppState>,
    project: String,
) -> CmdResult<Vec<RepoTrust>> {
    blocking(&state, move |dir| trust::project(dir, &project)).await
}

/// Trusts (or stops trusting) every repository of the project in Claude Code: only on the owner's explicit
/// choice, recorded in the assistant's journal.
#[tauri::command]
pub async fn claude_trust_set(
    state: State<'_, AppState>,
    project: String,
    on: bool,
) -> CmdResult<Vec<RepoTrust>> {
    blocking(&state, move |dir| {
        let repos = trust::set_project(dir, &project, on)?;
        let roots: Vec<&str> = repos.iter().map(|r| r.root.as_str()).collect();
        let decision = if on {
            Decision::Accepted
        } else {
            Decision::Rejected
        };
        onboarding::decide(
            dir,
            &data_dir()?,
            &project,
            "trust",
            decision,
            &roots.join(", "),
        )?;
        Ok(repos)
    })
    .await
}

/// `choices`: what the card changed; `fresh` (by default): read the repositories again rather than reuse the
/// last survey.
#[tauri::command]
pub async fn onboarding_team(
    state: State<'_, AppState>,
    project: String,
    choices: Option<onboarding::Choices>,
    fresh: Option<bool>,
) -> CmdResult<onboarding::Team> {
    blocking(&state, move |dir| {
        onboarding::team(
            dir,
            &data_dir()?,
            &project,
            choices.unwrap_or_default(),
            fresh.unwrap_or(true),
        )
    })
    .await
}

/// The diff `edit` makes to the project's file, before it is written.
#[tauri::command]
pub async fn onboarding_team_preview(
    state: State<'_, AppState>,
    project: String,
    edit: onboarding::Edit,
) -> CmdResult<onboarding::Preview> {
    blocking(&state, move |dir| {
        onboarding::team_preview(dir, &project, &edit)
    })
    .await
}

/// Writes the accepted parts of the team; `replace`: the owner chose to replace what was changed by hand.
#[tauri::command]
pub async fn onboarding_team_save(
    state: State<'_, AppState>,
    project: String,
    edit: onboarding::Edit,
    replace: Option<bool>,
) -> CmdResult<onboarding::State> {
    blocking(&state, move |dir| {
        onboarding::team_save(dir, &data_dir()?, &project, &edit, replace.unwrap_or(false))
    })
    .await
}
