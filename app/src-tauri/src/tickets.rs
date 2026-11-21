//! Tickets and their runs, for the UI and, through the local API, for the CLI. A run's session is an ordinary
//! terminal session, opened in the run's worktree. Every change to a run emits `run-updated` with the run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use termf_tickets::{
    Configured, Delivery, DeliveryPreview, Inbox, McpServer, Resolution, Run, RunEvent, Sources,
    TicketSummary, Tickets, Trackers, Unpushed,
};

use crate::{AppState, CmdResult, api, err};

pub struct TicketsState {
    tickets: Arc<Tickets>,
    /// Run → its session, for this launch only: session ids start over with the app.
    sessions: Mutex<HashMap<i64, u32>>,
}

impl TicketsState {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            tickets: Arc::new(Tickets::new(config_dir, data_dir)),
            sessions: Mutex::default(),
        }
    }
}

fn tickets(app: &AppHandle) -> Arc<Tickets> {
    app.state::<TicketsState>().tickets.clone()
}

/// git and gh reach the network: never on the UI thread.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle) -> Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

/// The run with its session, while that session still exists.
fn with_session(app: &AppHandle, mut run: Run) -> Run {
    let session = app
        .state::<TicketsState>()
        .sessions
        .lock()
        .unwrap()
        .get(&run.id)
        .copied();
    let live = app.state::<AppState>().sessions.list();
    run.session_id = session.filter(|id| live.iter().any(|s| s.id == *id));
    run
}

fn updated(app: &AppHandle, run: &Run) {
    let _ = app.emit("run-updated", with_session(app, run.clone()));
}

/// Resolves and prepares the run, then opens its Claude Code session, pointed at the file with its task.
pub fn start(
    app: &AppHandle,
    input: &str,
    project: Option<&str>,
    repo: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<Run> {
    let state = app.state::<AppState>();
    let agent = termf_core::load_settings(&state.config_dir)?.ticket_agent()?;
    api::run_guard(&state)?;
    let tickets = tickets(app);
    let launch = tickets.start(input, project, repo)?;
    let run = &launch.run;
    let worktree = run.worktree.clone().context("the run has no worktree")?;
    let session = api::spawn(
        &state,
        Some(&agent),
        worktree.into(),
        format!("{} · {}", run.ticket.key, run.title),
        cols,
        rows,
        false,
        Some(api::RunSession {
            project: &run.project,
            brief: &launch.brief,
        }),
    );
    let session = match session {
        Ok(session) => session,
        Err(e) => {
            updated(app, &tickets.fail(run.id, &format!("{e:#}"))?);
            return Err(e);
        }
    };
    let sessions = &app.state::<TicketsState>().sessions;
    sessions.lock().unwrap().insert(run.id, session.id);
    state.keep_awake.update(app);
    let _ = app.emit("session-opened", &session);
    let run = with_session(app, tickets.session_opened(run.id, session.id)?);
    updated(app, &run);
    Ok(run)
}

pub fn deliver(app: &AppHandle, id: i64, confirm: &str) -> Result<Delivery> {
    let tickets = tickets(app);
    let delivered = tickets.deliver(id, confirm);
    // Even a failed delivery may have pushed or opened the pull request.
    if let Ok(run) = tickets.run(id) {
        updated(app, &run);
    }
    let mut delivered = delivered?;
    delivered.run = with_session(app, delivered.run);
    Ok(delivered)
}

/// Closes the run; its session ends first, so no agent is left working in a deleted folder.
pub fn close(app: &AppHandle, id: i64, force: bool) -> Result<Run> {
    let run = tickets(app).close(id, force, || {
        let sessions = &app.state::<TicketsState>().sessions;
        let session = sessions.lock().unwrap().remove(&id);
        if let Some(session) = session {
            let state = app.state::<AppState>();
            state.api.forget(session);
            let _ = state.sessions.kill(session);
            state.keep_awake.update(app);
        }
    })?;
    updated(app, &run);
    Ok(with_session(app, run))
}

/// `refresh` asks the project's MCP sources again instead of reading their last answer.
#[tauri::command]
pub async fn tickets_inbox(
    app: AppHandle,
    project: Option<String>,
    refresh: Option<bool>,
) -> CmdResult<Inbox> {
    blocking(app, move |app| {
        tickets(app).inbox(project.as_deref(), refresh.unwrap_or(false))
    })
    .await
}

/// Whether the keychain holds an API token for this Jira site and e-mail.
#[tauri::command]
pub async fn tickets_jira_token(app: AppHandle, site: String, email: String) -> CmdResult<bool> {
    blocking(app, move |app| tickets(app).has_jira_token(&site, &email)).await
}

#[tauri::command]
pub async fn tickets_sources(app: AppHandle, project: String) -> CmdResult<Sources> {
    blocking(app, move |app| tickets(app).sources(&project)).await
}

#[tauri::command]
pub async fn tickets_sources_save(
    app: AppHandle,
    project: String,
    trackers: Trackers,
) -> CmdResult<Sources> {
    blocking(app, move |app| {
        tickets(app).save_sources(&project, &trackers)
    })
    .await
}

/// A few tickets of one source (`github`, `githubProject`, `jira` or `mcp:<name>`), with the sources as the
/// editor holds them, saved or not.
#[tauri::command]
pub async fn tickets_source_test(
    app: AppHandle,
    project: String,
    trackers: Trackers,
    source: String,
) -> CmdResult<Vec<TicketSummary>> {
    blocking(app, move |app| {
        tickets(app).test_source(&project, trackers, &source)
    })
    .await
}

#[tauri::command]
pub async fn tickets_jira_token_save(
    app: AppHandle,
    site: String,
    email: String,
    token: String,
) -> CmdResult<()> {
    blocking(app, move |app| {
        tickets(app).save_jira_token(&site, &email, &token)
    })
    .await
}

#[tauri::command]
pub async fn tickets_jira_token_forget(
    app: AppHandle,
    site: String,
    email: String,
) -> CmdResult<()> {
    blocking(app, move |app| {
        tickets(app).forget_jira_token(&site, &email)
    })
    .await
}

/// The MCP servers of the user's Claude Code (`claude mcp list`, which checks each one: slow).
#[tauri::command]
pub async fn tickets_mcp_servers(
    app: AppHandle,
    project: Option<String>,
) -> CmdResult<Vec<McpServer>> {
    blocking(app, move |app| tickets(app).mcp_servers(project.as_deref())).await
}

#[tauri::command]
pub async fn tickets_resolve(
    app: AppHandle,
    input: String,
    project: Option<String>,
) -> CmdResult<Resolution> {
    blocking(app, move |app| {
        tickets(app).resolve(&input, project.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn tickets_run(
    app: AppHandle,
    input: String,
    project: Option<String>,
    repo: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> CmdResult<Run> {
    blocking(app, move |app| {
        let (cols, rows) = (cols.unwrap_or(120), rows.unwrap_or(32));
        start(app, &input, project.as_deref(), repo.as_deref(), cols, rows)
    })
    .await
}

#[tauri::command]
pub async fn tickets_runs(app: AppHandle) -> CmdResult<Vec<Run>> {
    blocking(app, |app| {
        let runs = tickets(app).runs()?;
        Ok(runs.into_iter().map(|r| with_session(app, r)).collect())
    })
    .await
}

#[tauri::command]
pub async fn tickets_events(app: AppHandle, id: i64) -> CmdResult<Vec<RunEvent>> {
    blocking(app, move |app| tickets(app).events(id)).await
}

#[tauri::command]
pub async fn tickets_deliver_preview(app: AppHandle, id: i64) -> CmdResult<DeliveryPreview> {
    blocking(app, move |app| tickets(app).deliver_preview(id)).await
}

#[tauri::command]
pub async fn tickets_deliver(app: AppHandle, id: i64, confirm: String) -> CmdResult<Delivery> {
    blocking(app, move |app| deliver(app, id, &confirm)).await
}

/// Why a run was not closed. `uncommitted` or `unpushed`: closing would lose work (`uncommitted`: changes not
/// committed; `commits`: commits not pushed), which `force` drops. `error`: anything else.
#[derive(Serialize)]
pub struct CloseError {
    kind: &'static str,
    message: String,
    uncommitted: bool,
    commits: u32,
}

#[tauri::command]
pub async fn tickets_close(
    app: AppHandle,
    id: i64,
    force: Option<bool>,
) -> Result<Run, CloseError> {
    let closed =
        tauri::async_runtime::spawn_blocking(move || close(&app, id, force.unwrap_or(false)))
            .await
            .map_err(anyhow::Error::from)
            .and_then(|closed| closed);
    closed.map_err(|e| match e.downcast_ref::<Unpushed>() {
        Some(left) => CloseError {
            kind: if left.uncommitted {
                "uncommitted"
            } else {
                "unpushed"
            },
            message: left.to_string(),
            uncommitted: left.uncommitted,
            commits: left.commits,
        },
        None => CloseError {
            kind: "error",
            message: err(e),
            uncommitted: false,
            commits: 0,
        },
    })
}

/// For each project (or only `project`), whether its inbox has a source, and which.
#[tauri::command]
pub async fn tickets_configured(
    app: AppHandle,
    project: Option<String>,
) -> CmdResult<Vec<Configured>> {
    blocking(app, move |app| tickets(app).configured(project.as_deref())).await
}
