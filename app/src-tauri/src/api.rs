//! Local API socket for the `termf` CLI, and the status agents report through their hooks.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use interprocess::local_socket::{GenericFilePath, ListenerOptions, Stream, prelude::*};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use termf_core::claude;
use termf_core::ipc::{self, Call, Reply, Request, SessionSummary, Status, StatusEvent};
use termf_term::{SessionInfo, Sessions, SpawnSpec, shell_command, shell_quote};

use crate::AppState;

struct Meta {
    title: String,
    status: Option<Status>,
    remote_control: bool,
    agent_session_id: Option<String>,
    /// Removed with the session.
    dir: Option<PathBuf>,
}

pub struct Api {
    data_dir: PathBuf,
    socket: String,
    token: String,
    meta: Mutex<HashMap<u32, Meta>>,
    watchers: Mutex<Vec<mpsc::Sender<StatusEvent>>>,
}

impl Api {
    /// Writes a fresh token to a file only the user can read. No session outlives the app, so old session folders go.
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        let run = ipc::run_dir(&data_dir);
        fs::create_dir_all(&run)?;
        let _ = fs::remove_dir_all(data_dir.join("sessions"));
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| anyhow!("no randomness for the API token: {e}"))?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

        let file = ipc::token_file(&data_dir);
        let _ = fs::remove_file(&file);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            fs::set_permissions(&run, fs::Permissions::from_mode(0o700))?;
            options.mode(0o600);
        }
        options
            .open(&file)
            .and_then(|mut f| f.write_all(token.as_bytes()))
            .with_context(|| format!("cannot write {}", file.display()))?;

        Ok(Self {
            socket: ipc::socket_name(&data_dir),
            data_dir,
            token,
            meta: Mutex::default(),
            watchers: Mutex::default(),
        })
    }

    /// A session registry whose processes can reach this API.
    pub fn sessions(&self) -> Sessions {
        let (socket, token) = (self.socket.clone(), self.token.clone());
        Sessions::with_env(move |id| {
            vec![
                (ipc::ENV_SOCKET.into(), socket.clone()),
                (ipc::ENV_TOKEN.into(), token.clone()),
                (ipc::ENV_SESSION.into(), id.to_string()),
            ]
        })
    }

    pub fn list(&self, sessions: &Sessions) -> Vec<SessionSummary> {
        sessions
            .list()
            .into_iter()
            .map(|s| self.summary(s))
            .collect()
    }

    fn summary(&self, info: SessionInfo) -> SessionSummary {
        let meta = self.meta.lock().unwrap();
        let meta = meta.get(&info.id);
        SessionSummary {
            status: meta
                .and_then(|m| m.status)
                .map(|s| if info.running { s } else { Status::Done }),
            remote_control: meta.is_some_and(|m| m.remote_control),
            agent_session_id: meta.and_then(|m| m.agent_session_id.clone()),
            id: info.id,
            title: info.title,
            cwd: info.cwd,
            agent: info.agent,
            running: info.running,
            exit_code: info.exit_code,
        }
    }

    fn set_status(&self, app: &AppHandle, id: u32, status: Status) {
        let event = {
            let mut meta = self.meta.lock().unwrap();
            let Some(m) = meta.get_mut(&id).filter(|m| m.status != Some(status)) else {
                return;
            };
            m.status = Some(status);
            StatusEvent {
                id,
                title: m.title.clone(),
                status,
            }
        };
        let _ = app.emit("session-status", &event);
        self.watchers
            .lock()
            .unwrap()
            .retain(|w| w.send(event.clone()).is_ok());
    }

    fn hook(&self, app: &AppHandle, id: u32, event: &str, payload: &Value) {
        if let Some(agent_session) = payload.get("session_id").and_then(Value::as_str)
            && let Some(m) = self.meta.lock().unwrap().get_mut(&id)
        {
            m.agent_session_id = Some(agent_session.into());
        }
        if let Some(status) = ipc::hook_status(event, payload) {
            self.set_status(app, id, status);
        }
    }

    /// The process ended: an agent that reports its status is done.
    pub fn exited(&self, app: &AppHandle, id: u32) {
        let tracked = self
            .meta
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|m| m.status.is_some());
        if tracked {
            self.set_status(app, id, Status::Done);
        }
    }

    pub fn forget(&self, id: u32) {
        if let Some(Meta { dir: Some(dir), .. }) = self.meta.lock().unwrap().remove(&id) {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// Starts a session. A Claude Code agent gets an ephemeral plugin whose hooks report its status,
/// and Remote Control when asked; other agents and shells get neither.
pub fn spawn(
    state: &AppState,
    agent: Option<&str>,
    cwd: PathBuf,
    title: String,
    cols: u16,
    rows: u16,
    remote_control: bool,
) -> Result<SessionSummary> {
    let command = match agent {
        Some(name) => {
            let settings = termf_core::load_settings(&state.config_dir)?;
            let command = settings.agents.get(name).cloned();
            Some(command.ok_or_else(|| anyhow!("unknown agent {name}"))?)
        }
        None => None,
    };
    let is_claude = command.as_deref().is_some_and(claude::is_claude);
    let remote_control = remote_control && is_claude;
    let termf = is_claude.then(termf_binary).flatten();

    let cwd_text = cwd.to_string_lossy().into_owned();
    let mut new_id = None;
    let spawned = state.sessions.spawn_with(|id| {
        new_id = Some(id);
        let dir = state.api.data_dir.join("sessions").join(id.to_string());
        state.api.meta.lock().unwrap().insert(
            id,
            Meta {
                title: title.clone(),
                status: termf.is_some().then_some(Status::Starting),
                remote_control,
                agent_session_id: None,
                dir: termf.is_some().then(|| dir.clone()),
            },
        );
        let mut command = command;
        if let (Some(command), Some(termf)) = (command.as_mut(), &termf) {
            let plugin = dir.join("plugin");
            claude::write_plugin(&plugin, termf)?;
            command.push_str(" --plugin-dir ");
            command.push_str(&shell_quote(&plugin.to_string_lossy()));
        }
        if let Some(command) = command.as_mut().filter(|_| remote_control) {
            command.push_str(" --remote-control ");
            command.push_str(&shell_quote(&title));
        }
        let (program, args) = shell_command(command.as_deref());
        Ok(SpawnSpec {
            title: title.clone(),
            program,
            args,
            cwd,
            agent: agent.map(str::to_owned),
            cols,
            rows,
        })
    });
    if spawned.is_err()
        && let Some(id) = new_id
    {
        state.api.forget(id);
    }
    Ok(state.api.summary(SessionInfo {
        id: spawned?,
        title,
        cwd: cwd_text,
        agent: agent.map(str::to_owned),
        running: true,
        exit_code: None,
    }))
}

/// The CLI ships next to the app executable (both in `target/<profile>/` during development).
fn termf_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.with_file_name(format!("termf{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Listens on the local socket, one thread per connection.
pub fn serve(app: AppHandle) -> Result<()> {
    let socket = app.state::<AppState>().api.socket.clone();
    // Replaces the socket of a TermF that crashed; the newest instance wins.
    let listener = ListenerOptions::new()
        .name(socket.as_str().to_fs_name::<GenericFilePath>()?)
        .try_overwrite(true)
        .create_sync()
        .with_context(|| format!("cannot listen on {socket}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
    }
    std::thread::Builder::new()
        .name("api".into())
        .spawn(move || {
            for conn in listener.incoming().filter_map(Result::ok) {
                let app = app.clone();
                let _ = std::thread::Builder::new()
                    .name("api-conn".into())
                    .spawn(move || handle(&app, conn));
            }
        })?;
    Ok(())
}

fn handle(app: &AppHandle, conn: Stream) -> Result<()> {
    let state = app.state::<AppState>();
    let mut conn = BufReader::new(conn);
    let mut line = String::new();
    while conn.read_line(&mut line)? > 0 {
        let request = serde_json::from_str::<Request>(&line);
        line.clear();
        let request = match request {
            Ok(r) if ipc::token_matches(&r.token, &state.api.token) => r,
            Ok(_) => return reply(&mut conn, &Reply::Error("invalid token".into())),
            Err(e) => return reply(&mut conn, &Reply::Error(format!("invalid request: {e}"))),
        };
        if request.call == Call::Watch {
            return watch(&state.api, conn);
        }
        let result = match call(app, &state, request.call) {
            Ok(value) => Reply::Ok(value),
            Err(e) => Reply::Error(format!("{e:#}")),
        };
        reply(&mut conn, &result)?;
    }
    Ok(())
}

fn reply(conn: &mut BufReader<Stream>, reply: &Reply) -> Result<()> {
    conn.get_mut().write_all(ipc::encode(reply).as_bytes())?;
    Ok(())
}

/// Streams status events until the client goes away.
// shortcut: a watcher that disconnects is only noticed at the next event, fine for a handful of terminals.
fn watch(api: &Api, mut conn: BufReader<Stream>) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    api.watchers.lock().unwrap().push(tx);
    reply(&mut conn, &Reply::Ok(Value::Null))?;
    for event in rx {
        reply(&mut conn, &Reply::Event(event))?;
    }
    Ok(())
}

fn call(app: &AppHandle, state: &AppState, call: Call) -> Result<Value> {
    Ok(match call {
        Call::List => serde_json::to_value(state.api.list(&state.sessions))?,
        Call::Open { path, agent } => {
            let cwd = PathBuf::from(path);
            if !cwd.is_dir() {
                bail!("not a folder: {}", cwd.display());
            }
            let name = cwd.file_name().unwrap_or(cwd.as_os_str()).to_string_lossy();
            let title = format!("{name} · {}", agent.as_deref().unwrap_or("shell"));
            let session = spawn(state, agent.as_deref(), cwd, title, 120, 32, false)?;
            app.emit("session-opened", &session)?;
            serde_json::to_value(session)?
        }
        Call::Say { session, text } => {
            let id = ipc::find_session(&state.api.list(&state.sessions), &session)?;
            state.sessions.write(id, text.as_bytes())?;
            // Sent apart so a TUI that detects pastes, like Claude Code, reads Enter as a key press.
            std::thread::sleep(Duration::from_millis(100));
            state.sessions.write(id, b"\r")?;
            Value::Null
        }
        Call::Hook {
            session,
            event,
            payload,
        } => {
            state.api.hook(app, session, &event, &payload);
            Value::Null
        }
        Call::Watch => unreachable!("handled by the connection loop"),
    })
}
