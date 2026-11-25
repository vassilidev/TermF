//! Headless agent sessions, for the UI and, through the local API, for the CLI. Every event goes out as
//! `agent-event` (a `StoredEvent`, id 0 for streamed text, which is never stored) and every change of a session
//! as `agent-updated`. Taking a session over opens it in an ordinary tile.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use termf_agents::{
    AgentEvent, AgentSession, Agents, IMAGE_MAX, Image, SessionSpec, StoredEvent, Update,
};
use termf_core::ipc::{AgentCall, SessionSummary};

use crate::tickets::blocking;
use crate::{AppState, CmdResult, api};

pub struct AgentsState(Agents);

impl AgentsState {
    pub fn agents(&self) -> Agents {
        self.0.clone()
    }
}

/// `team` hears of every change of a session, its steps being sessions.
pub fn state(
    app: &AppHandle,
    config_dir: PathBuf,
    data_dir: PathBuf,
    team: termf_team::Notifier,
) -> AgentsState {
    let handle = app.clone();
    let config = config_dir.clone();
    let agents = Agents::new(config_dir, data_dir, move |update| {
        let _ = match update {
            Update::Event(event) => {
                if let AgentEvent::Limit {
                    status,
                    window,
                    resets_at,
                    used,
                } = &event.event
                {
                    crate::claude_native::rate_limit(
                        &handle,
                        event.session,
                        status,
                        window.as_deref(),
                        *resets_at,
                        *used,
                    );
                }
                handle.emit("agent-event", event)
            }
            Update::Session(session) => {
                crate::memory::delivering(&handle, &session);
                team.session(session.id);
                if let Some(state) = handle.try_state::<AppState>() {
                    let working = session.state == termf_agents::State::Working;
                    state.keep_awake.agent(&handle, session.id, working);
                }
                handle.emit("agent-updated", session)
            }
        };
    });
    let (app, gone, secrets) = (app.clone(), app.clone(), app.clone());
    if let Some(termf) = api::termf_binary() {
        agents.serve_guard(termf);
    }
    agents.serve_secrets(move |session, project| {
        if secrets.try_state::<crate::vault::VaultState>().is_none() {
            return termf_agents::Given::default();
        }
        let actor = termf_vault::Actor::agent(
            crate::ops::run_of(&secrets, session),
            Some(session.to_string()),
        );
        crate::agentenv::given(&crate::vault::shared(&secrets), &config, project, &actor)
    });
    agents.serve_mcp(
        move |session| {
            let state = app.try_state::<AppState>().context("TermF is starting")?;
            state.api.mcp_server(session)
        },
        move |session| {
            if let Some(state) = gone.try_state::<AppState>() {
                state.api.revoke_mcp(session);
            }
        },
    );
    AgentsState(agents)
}

pub(crate) fn agents(app: &AppHandle) -> &Agents {
    &app.state::<AgentsState>().inner().0
}

/// The app quits: no headless process may outlive it.
pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<AgentsState>() {
        state.0.shutdown();
    }
}

/// What a permission request of `session` asks, as the user sees it before allowing it from elsewhere: the
/// session, the tool, its input.
pub fn permission(app: &AppHandle, session: i64, request: &str) -> Vec<(&'static str, String)> {
    let agents = agents(app);
    let mut about = Vec::new();
    if let Ok(s) = agents.session(session) {
        about.push(("title", s.spec.title));
    }
    let asked = agents.events(session, 0).unwrap_or_default();
    if let Some((tool, input, description)) = asked.into_iter().rev().find_map(|e| match e.event {
        AgentEvent::Permission {
            request: r,
            tool,
            input,
            description,
        } if r == request => Some((tool, input, description)),
        _ => None,
    }) {
        about.push(("tool", tool));
        about.extend(description.map(|d| ("description", d)));
        about.push((
            "input",
            serde_json::to_string_pretty(&input).unwrap_or_default(),
        ));
    }
    about
}

/// A terminal session ended: it may hold a conversation taken over.
pub fn tile_exited(app: &AppHandle, id: u32) {
    agents(app).tile_exited(id);
}

fn takeover(app: &AppHandle, id: i64, cols: u16, rows: u16) -> Result<SessionSummary> {
    let agents = agents(app);
    let takeover = agents.takeover(id)?;
    let spec = agents.session(id).map(|s| s.spec).unwrap_or_default();
    let remote = || api::Remote {
        project: spec.project.as_deref(),
        role: spec.role.as_deref(),
        ..api::Remote::default()
    };
    let state = app.state::<AppState>();
    let spawned = match takeover.guard {
        true => api::spawn_guarded(
            &state,
            &takeover.agent,
            takeover.cwd,
            takeover.title,
            (cols, rows),
            remote(),
            &takeover.args,
        ),
        false => api::spawn(
            &state,
            Some(&takeover.agent),
            takeover.cwd,
            takeover.title,
            cols,
            rows,
            remote(),
            None,
            &takeover.args,
        ),
    };
    let tile = match spawned {
        Ok(tile) => tile,
        Err(e) => {
            let _ = agents.attach_tile(id, None);
            return Err(e);
        }
    };
    agents.attach_tile(id, Some(tile.id))?;
    // It may have ended before it was attached.
    if !state
        .sessions
        .list()
        .iter()
        .any(|s| s.id == tile.id && s.running)
    {
        agents.tile_exited(tile.id);
    }
    state.keep_awake.update(app);
    app.emit("session-opened", &tile)?;
    Ok(tile)
}

fn handback(app: &AppHandle, id: i64) -> Result<AgentSession> {
    let state = app.state::<AppState>();
    // The tile stays, showing that its process ended.
    agents(app).handback(id, |tile| {
        let _ = state.sessions.terminate(tile);
    })
}

/// What the CLI asks through the local API.
pub fn call(app: &AppHandle, call: AgentCall) -> Result<Value> {
    let agents = agents(app);
    Ok(match call {
        AgentCall::Start { spec, prompt } => {
            serde_json::to_value(agents.start(serde_json::from_value(spec)?, prompt.as_deref())?)?
        }
        AgentCall::Send { session, text } => {
            crate::memory::before_expert(app, &agents.session(session)?, &text);
            agents.send(session, &text)?;
            Value::Null
        }
        AgentCall::Interrupt { session } => {
            agents.interrupt(session)?;
            Value::Null
        }
        AgentCall::Answer {
            session,
            request,
            allow,
            message,
        } => {
            agents.answer(session, &request, allow, message.as_deref())?;
            Value::Null
        }
        AgentCall::Stop { session } => serde_json::to_value(agents.stop(session)?)?,
        AgentCall::Takeover { session } => serde_json::to_value(takeover(app, session, 120, 32)?)?,
        AgentCall::Handback { session } => serde_json::to_value(handback(app, session)?)?,
    })
}

// The database, the login shell that starts a process and the wait for one to end: off the UI thread.

#[tauri::command]
pub async fn agent_start(
    app: AppHandle,
    spec: SessionSpec,
    prompt: Option<String>,
) -> CmdResult<AgentSession> {
    blocking(app, move |app| agents(app).start(spec, prompt.as_deref())).await
}

#[tauri::command]
pub async fn agent_send(
    app: AppHandle,
    id: i64,
    text: String,
    images: Option<Vec<Image>>,
) -> CmdResult<()> {
    blocking(app, move |app| {
        let agents = agents(app);
        crate::memory::before_expert(app, &agents.session(id)?, &text);
        agents.send_with(id, &text, &images.unwrap_or_default())
    })
    .await
}

/// An image file dropped on a conversation: its bytes, for the message only.
#[tauri::command]
pub async fn agent_image(app: AppHandle, path: String) -> CmdResult<tauri::ipc::Response> {
    blocking(app, move |_| {
        let size = std::fs::metadata(&path)?.len();
        // Claude's limit is on the base64 of the file, a third longer.
        anyhow::ensure!(
            size <= (IMAGE_MAX / 4 * 3) as u64,
            "this image is over 7.5 MB: Claude refuses it"
        );
        Ok(tauri::ipc::Response::new(std::fs::read(&path)?))
    })
    .await
}

/// Sends a message now, or keeps it for the end of the agent's turn while it works (`true`).
#[tauri::command]
pub async fn agent_send_or_queue(app: AppHandle, id: i64, text: String) -> CmdResult<bool> {
    blocking(app, move |app| agents(app).send_or_queue(id, &text)).await
}

#[tauri::command]
pub async fn agent_interrupt(app: AppHandle, id: i64) -> CmdResult<()> {
    blocking(app, move |app| agents(app).interrupt(id)).await
}

#[tauri::command]
pub async fn agent_answer(
    app: AppHandle,
    id: i64,
    request: String,
    allow: bool,
    message: Option<String>,
) -> CmdResult<()> {
    blocking(app, move |app| {
        agents(app).answer(id, &request, allow, message.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn agent_stop(app: AppHandle, id: i64) -> CmdResult<AgentSession> {
    blocking(app, move |app| agents(app).stop(id)).await
}

#[tauri::command]
pub async fn agent_takeover(
    app: AppHandle,
    id: i64,
    cols: Option<u16>,
    rows: Option<u16>,
) -> CmdResult<SessionSummary> {
    blocking(app, move |app| {
        takeover(app, id, cols.unwrap_or(120), rows.unwrap_or(32))
    })
    .await
}

#[tauri::command]
pub async fn agent_handback(app: AppHandle, id: i64) -> CmdResult<AgentSession> {
    blocking(app, move |app| handback(app, id)).await
}

#[tauri::command]
pub async fn agent_sessions(app: AppHandle) -> CmdResult<Vec<AgentSession>> {
    blocking(app, |app| agents(app).sessions()).await
}

#[tauri::command]
pub async fn agent_events(
    app: AppHandle,
    id: i64,
    after: Option<i64>,
) -> CmdResult<Vec<StoredEvent>> {
    blocking(app, move |app| agents(app).events(id, after.unwrap_or(0))).await
}
