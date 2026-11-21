//! Tickets and their runs, for the UI and, through the local API, for the CLI. A run's session is an ordinary
//! terminal session, opened in the run's worktree. Every change to a run emits `run-updated` with the run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tauri::{AppHandle, Emitter, Manager};
use termf_core::claude;
use termf_tickets::{Resolution, Run, RunEvent, TicketSummary, Tickets};

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

fn updated(app: &AppHandle, run: &Run) {
    let _ = app.emit("run-updated", run);
}

/// Resolves and prepares the run, then opens its Claude Code session with the ticket as first message.
pub fn start(
    app: &AppHandle,
    input: &str,
    project: Option<&str>,
    repo: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<Run> {
    let state = app.state::<AppState>();
    let settings = termf_core::load_settings(&state.config_dir)?;
    let agent = settings
        .agents
        .iter()
        .find(|(_, command)| claude::is_claude(command))
        .map(|(name, _)| name.clone())
        .context("no agent of termf.toml [agents] runs Claude Code")?;
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
            prompt: &launch.prompt,
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
    let run = tickets.session_opened(run.id, session.id)?;
    updated(app, &run);
    Ok(run)
}

pub fn deliver(app: &AppHandle, id: i64) -> Result<Run> {
    let tickets = tickets(app);
    let delivered = tickets.deliver(id);
    // Even a failed delivery may have pushed or opened the pull request.
    if let Ok(run) = tickets.run(id) {
        updated(app, &run);
    }
    delivered
}

/// Closes the run, then ends its session, whose folder is gone.
pub fn close(app: &AppHandle, id: i64, force: bool) -> Result<Run> {
    let run = tickets(app).close(id, force)?;
    let session = app
        .state::<TicketsState>()
        .sessions
        .lock()
        .unwrap()
        .remove(&id);
    if let Some(session) = session {
        let state = app.state::<AppState>();
        state.api.forget(session);
        let _ = state.sessions.kill(session);
        state.keep_awake.update(app);
    }
    updated(app, &run);
    Ok(run)
}

#[tauri::command]
pub async fn tickets_inbox(
    app: AppHandle,
    project: Option<String>,
) -> CmdResult<Vec<TicketSummary>> {
    blocking(app, move |app| tickets(app).inbox(project.as_deref())).await
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
    blocking(app, |app| tickets(app).runs()).await
}

#[tauri::command]
pub async fn tickets_events(app: AppHandle, id: i64) -> CmdResult<Vec<RunEvent>> {
    blocking(app, move |app| tickets(app).events(id)).await
}

#[tauri::command]
pub async fn tickets_deliver(app: AppHandle, id: i64) -> CmdResult<Run> {
    blocking(app, move |app| deliver(app, id)).await
}

#[tauri::command]
pub async fn tickets_close(app: AppHandle, id: i64, force: Option<bool>) -> CmdResult<Run> {
    blocking(app, move |app| close(app, id, force.unwrap_or(false))).await
}
