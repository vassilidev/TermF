use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use termf_claude::trust::{self, RepoTrust};
use termf_onboarding::{self as onboarding, Decision, Step, greenfield, setup};

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
    app: AppHandle,
    state: State<'_, AppState>,
    project: String,
    lang: String,
) -> CmdResult<onboarding::State> {
    blocking(&state, move |dir| {
        let progress = &mut |p: termf_library::conventions::Progress| {
            let read = Read {
                project: &project,
                tool: p.tool,
                target: p.target,
            };
            let _ = app.emit("onboarding-progress", read);
        };
        onboarding::analyze_showing(dir, &data_dir()?, &project, &lang, &claude(dir), progress)
    })
    .await
}

/// `start_onboarding` of the CTO's conversation: the analysis the assistant starts with, which the user then goes
/// through there.
pub(crate) fn mcp_analyze(
    app: &AppHandle,
    project: &str,
    lang: Option<String>,
) -> anyhow::Result<serde_json::Value> {
    use anyhow::Context as _;
    use tauri::Manager as _;
    let dir = app.state::<AppState>().config_dir.clone();
    let progress = &mut |p: termf_library::conventions::Progress| {
        let read = Read {
            project,
            tool: p.tool,
            target: p.target,
        };
        let _ = app.emit("onboarding-progress", read);
    };
    let lang = lang.unwrap_or_else(|| "en".into());
    let state = onboarding::analyze_showing(&dir, &data_dir()?, project, &lang, &claude(&dir), progress)?;
    let analysis = state.analysis.context("the analysis gave nothing")?;
    Ok(serde_json::json!({
        "findings": analysis.findings,
        "questions": analysis.questions.len(),
        "next": "The user answers its questions and applies what it proposes in TermF's setup assistant."
    }))
}

/// A file Claude Code reads, or a search it makes, during a project's analysis.
#[derive(Serialize, Clone)]
struct Read<'a> {
    project: &'a str,
    tool: String,
    target: String,
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

/// The new-project setup: each repository's card (git state, remote, stack), read off the window's thread.
#[tauri::command]
pub async fn setup_repos(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CmdResult<Vec<setup::Card>> {
    blocking(&state, move |_| Ok(setup::cards(&paths))).await
}

/// What TermF finds by itself in one repository chosen for a new project.
#[tauri::command]
pub async fn setup_detect(state: State<'_, AppState>, path: String) -> CmdResult<setup::Detected> {
    blocking(&state, move |dir| setup::detect(dir, &path)).await
}

#[derive(serde::Serialize)]
pub struct SetupDraft {
    #[serde(flatten)]
    draft: setup::Draft,
    /// Claude Code's trust in each repository, and what trusting it loads.
    trust: Vec<RepoTrust>,
}

/// What TermF proposes for the repositories of a project not created yet.
#[tauri::command]
pub async fn setup_draft(state: State<'_, AppState>, repos: Vec<String>) -> CmdResult<SetupDraft> {
    blocking(&state, move |dir| {
        let named: Vec<(String, PathBuf)> = repos
            .iter()
            .map(|p| {
                let path = PathBuf::from(p);
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                (name, path)
            })
            .collect();
        Ok(SetupDraft {
            draft: setup::draft(dir, &repos)?,
            trust: trust::state(&trust::file(), &named)?,
        })
    })
    .await
}

/// The project file creating the project with `team` and `trackers` would write.
#[tauri::command]
pub async fn setup_preview(
    name: String,
    repos: Vec<String>,
    team: Option<onboarding::Edit>,
    trackers: Option<termf_tickets::Trackers>,
    checks: Option<BTreeMap<String, String>>,
) -> CmdResult<String> {
    let checks = checks.unwrap_or_default();
    setup::preview(&name, &repos, team.as_ref(), trackers.as_ref(), &checks).map_err(err)
}

/// What creating a project from an idea will do: its folder, its files and every command, before any runs.
#[tauri::command]
pub async fn greenfield_plan(
    state: State<'_, AppState>,
    spec: greenfield::Spec,
) -> CmdResult<greenfield::Plan> {
    blocking(&state, move |_| greenfield::plan(&spec, &greenfield::env())).await
}

/// Creates the project from an idea, as `greenfield_plan` said: its repository, then the project.
#[tauri::command]
pub async fn greenfield_create(
    state: State<'_, AppState>,
    spec: greenfield::Spec,
) -> CmdResult<greenfield::Created> {
    blocking(&state, move |dir| {
        let made = greenfield::create(dir, &spec, &greenfield::env())?;
        created(dir, &made.project);
        Ok(made)
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
