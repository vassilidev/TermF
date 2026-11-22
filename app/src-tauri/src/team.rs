//! The agent team of a ticket run, for the UI and, through the local API, for the CLI. Every change of a team
//! run goes out as `team-updated` (a `TeamRun`) and every event as `team-event` (a `TeamEvent`); its agents'
//! sessions are ordinary headless sessions (`agent-updated`, `agent-event`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Result, bail};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use termf_core::ipc::TeamCall;
use termf_team::route::WorkflowView;
use termf_team::{
    Inbox, Level, Options, Request, Routing, Target, Team, TeamEvent, TeamRun, TeamUpdate,
};

use crate::agents::AgentsState;
use crate::tickets::{blocking, tickets, updated};
use crate::{AppState, CmdResult, api};

pub struct TeamState {
    team: Team,
    config_dir: PathBuf,
    data_dir: PathBuf,
}

pub fn state(app: &AppHandle, config_dir: PathBuf, data_dir: PathBuf, inbox: Inbox) -> TeamState {
    let handle = app.clone();
    let agents = app.state::<AgentsState>().agents();
    let team = Team::new(
        config_dir.clone(),
        data_dir.clone(),
        agents,
        inbox,
        move |update| {
            let _ = match update {
                TeamUpdate::Run(run) => handle.emit("team-updated", run),
                TeamUpdate::Event(event) => handle.emit("team-event", event),
            };
        },
    );
    TeamState {
        team,
        config_dir,
        data_dir,
    }
}

fn state_of(app: &AppHandle) -> &TeamState {
    app.state::<TeamState>().inner()
}

/// The app quits: nothing more starts, before its agents are killed.
pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<TeamState>() {
        state.team.shutdown();
    }
}

/// The team of a ticket run, or of a request in a new run of its own (prepared without a session, as a ticket
/// run for a team is): its triage starts at once.
fn start(
    app: &AppHandle,
    run: Option<i64>,
    request: Option<Request>,
    options: Options,
) -> Result<TeamRun> {
    let state = state_of(app);
    let run = match (run, request) {
        (Some(run), None) => tickets(app).run(run)?,
        (None, Some(r)) => {
            api::run_guard(&app.state::<AppState>())?;
            let run = tickets(app).start_request(&r.text, &r.project, r.repo.as_deref())?;
            updated(app, &run);
            run
        }
        _ => bail!("give the team a run, or a request"),
    };
    let target = Target::of(&run, &state.config_dir, &state.data_dir)?;
    state.team.start(target, options)
}

fn levels(rules: impl IntoIterator<Item = (String, String)>) -> Result<BTreeMap<String, Level>> {
    rules
        .into_iter()
        .map(|(name, level)| Ok((name, Level::parse(&level)?)))
        .collect()
}

/// What the CLI asks through the local API.
pub fn call(app: &AppHandle, call: TeamCall) -> Result<Value> {
    let team = &state_of(app).team;
    Ok(match call {
        TeamCall::Start {
            run,
            request,
            options,
        } => {
            let request = request.map(serde_json::from_value).transpose()?;
            serde_json::to_value(start(app, run, request, serde_json::from_value(options)?)?)?
        }
        TeamCall::Answer {
            run,
            step,
            answers,
            note,
        } => {
            serde_json::to_value(team.answer(run, &step, answers.into_iter().collect(), &note)?)?
        }
        TeamCall::Approve { run, step } => serde_json::to_value(team.approve(run, &step)?)?,
        TeamCall::Revise {
            run,
            step,
            feedback,
        } => serde_json::to_value(team.revise(run, &step, &feedback)?)?,
        TeamCall::Stop { run } => serde_json::to_value(team.stop(run)?)?,
        TeamCall::Rules { run, task, rules } => {
            serde_json::to_value(team.set_rules(run, task.as_deref(), levels(rules)?)?)?
        }
    })
}

// Starting sessions, git and the database: off the UI thread.

#[tauri::command]
pub async fn team_start(
    app: AppHandle,
    run: Option<i64>,
    request: Option<Request>,
    options: Option<Options>,
) -> CmdResult<TeamRun> {
    blocking(app, move |app| {
        start(app, run, request, options.unwrap_or_default())
    })
    .await
}

#[tauri::command]
pub async fn team_route(app: AppHandle, run: i64, routing: Routing) -> CmdResult<TeamRun> {
    blocking(app, move |app| state_of(app).team.route(run, routing)).await
}

#[tauri::command]
pub async fn team_answer(
    app: AppHandle,
    run: i64,
    step: String,
    answers: BTreeMap<String, Vec<String>>,
    note: Option<String>,
) -> CmdResult<TeamRun> {
    blocking(app, move |app| {
        state_of(app)
            .team
            .answer(run, &step, answers, note.as_deref().unwrap_or_default())
    })
    .await
}

#[tauri::command]
pub async fn team_workflows(app: AppHandle, run: i64) -> CmdResult<Vec<WorkflowView>> {
    blocking(app, move |app| state_of(app).team.workflows(run)).await
}

#[tauri::command]
pub async fn team_approve(app: AppHandle, run: i64, step: String) -> CmdResult<TeamRun> {
    blocking(app, move |app| state_of(app).team.approve(run, &step)).await
}

#[tauri::command]
pub async fn team_revise(
    app: AppHandle,
    run: i64,
    step: String,
    feedback: String,
) -> CmdResult<TeamRun> {
    blocking(app, move |app| {
        state_of(app).team.revise(run, &step, &feedback)
    })
    .await
}

#[tauri::command]
pub async fn team_stop(app: AppHandle, run: Option<i64>) -> CmdResult<Vec<TeamRun>> {
    blocking(app, move |app| state_of(app).team.stop(run)).await
}

#[tauri::command]
pub async fn team_rules(
    app: AppHandle,
    run: i64,
    task: Option<String>,
    rules: BTreeMap<String, String>,
) -> CmdResult<TeamRun> {
    blocking(app, move |app| {
        state_of(app)
            .team
            .set_rules(run, task.as_deref(), levels(rules)?)
    })
    .await
}

#[tauri::command]
pub async fn team_runs(app: AppHandle) -> CmdResult<Vec<TeamRun>> {
    blocking(app, |app| state_of(app).team.runs()).await
}

#[tauri::command]
pub async fn team_run(app: AppHandle, run: i64) -> CmdResult<TeamRun> {
    blocking(app, move |app| state_of(app).team.run(run)).await
}

#[tauri::command]
pub async fn team_events(
    app: AppHandle,
    run: i64,
    after: Option<i64>,
) -> CmdResult<Vec<TeamEvent>> {
    blocking(app, move |app| {
        state_of(app).team.events(run, after.unwrap_or(0))
    })
    .await
}

#[tauri::command]
pub async fn team_squads(app: AppHandle, project: String) -> CmdResult<termf_team::squads::View> {
    blocking(app, move |app| {
        termf_team::squads::view(&state_of(app).config_dir, &project)
    })
    .await
}
