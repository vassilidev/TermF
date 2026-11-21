//! Local API socket for the `termf` CLI, and the status agents report through their hooks.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use interprocess::local_socket::{GenericFilePath, Listener, ListenerOptions, Stream, prelude::*};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use termf_core::claude;
use termf_core::ipc::{self, Call, Reply, Request, SessionSummary, Status, StatusEvent};
use termf_term::{SessionInfo, Sessions, SpawnSpec, shell_command, shell_quote};

use crate::{AppState, tickets};

struct Meta {
    title: String,
    status: Option<Status>,
    remote_control: bool,
    agent_session_id: Option<String>,
    /// Removed with the session.
    dir: Option<PathBuf>,
}

struct Endpoint {
    data_dir: PathBuf,
    socket: String,
    /// The user's token, also in the token file.
    token: String,
    /// Session id → the token its hooks report with.
    session_tokens: Arc<Mutex<HashMap<u32, String>>>,
}

impl Endpoint {
    fn allows(&self, request: &Request) -> bool {
        let sessions = self.session_tokens.lock().unwrap();
        ipc::access(&request.token, &self.token, &sessions).is_some_and(|a| a.allows(&request.call))
    }
}

pub struct Api {
    /// `None` when the local API could not be set up: sessions still run, without the CLI or status.
    endpoint: Option<Endpoint>,
    /// Why the local API is off.
    pub error: Option<String>,
    meta: Mutex<HashMap<u32, Meta>>,
    watchers: Mutex<Vec<mpsc::Sender<StatusEvent>>>,
}

impl Api {
    /// Sets up the local API, or one that is off (and says why) when that fails.
    pub fn new() -> (Self, Option<Listener>) {
        let (endpoint, listener, error) = match open() {
            Ok((endpoint, listener)) => (Some(endpoint), Some(listener), None),
            Err(e) => {
                eprintln!("termf: local API disabled: {e:#}");
                (None, None, Some(format!("{e:#}")))
            }
        };
        let api = Self {
            endpoint,
            error,
            meta: Mutex::default(),
            watchers: Mutex::default(),
        };
        (api, listener)
    }

    /// A session registry whose processes can report their own hooks, and whose exits end their status.
    pub fn sessions(&self, app: AppHandle) -> Sessions {
        let sessions = match &self.endpoint {
            Some(endpoint) => {
                let socket = endpoint.socket.clone();
                let tokens = endpoint.session_tokens.clone();
                Sessions::with_env(move |id| {
                    let Ok(token) = random_token() else {
                        return Vec::new();
                    };
                    tokens.lock().unwrap().insert(id, token.clone());
                    vec![
                        (ipc::ENV_SOCKET.into(), socket.clone()),
                        (ipc::ENV_TOKEN.into(), token),
                        (ipc::ENV_SESSION.into(), id.to_string()),
                    ]
                })
            }
            None => Sessions::default(),
        };
        sessions.on_exit(move |id| app.state::<AppState>().api.exited(&app, id))
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
        app.state::<AppState>().keep_awake.update(app);
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

    /// Typing into a waiting session answers it (e.g. approves a tool), which no hook reports right away.
    pub fn input(&self, app: &AppHandle, id: u32, input: &[u8]) {
        let waiting = self
            .meta
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|m| m.status == Some(Status::Waiting));
        if waiting && ipc::answers_prompt(input) {
            self.set_status(app, id, Status::Working);
        }
    }

    /// The process ended: an agent that reports its status is done.
    fn exited(&self, app: &AppHandle, id: u32) {
        let tracked = self
            .meta
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|m| m.status.is_some());
        if tracked {
            self.set_status(app, id, Status::Done);
        } else {
            // Agents without a status keep the computer awake while they run.
            app.state::<AppState>().keep_awake.update(app);
        }
    }

    pub fn forget(&self, id: u32) {
        if let Some(endpoint) = &self.endpoint {
            endpoint.session_tokens.lock().unwrap().remove(&id);
        }
        if let Some(Meta { dir: Some(dir), .. }) = self.meta.lock().unwrap().remove(&id) {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

/// Writes a fresh user token to a file only the user can read and listens on the socket.
/// No session outlives the app, so old session folders go.
fn open() -> Result<(Endpoint, Listener)> {
    let data_dir = termf_core::default_data_dir()?;
    let run = ipc::run_dir(&data_dir);
    let socket = ipc::socket_name(&data_dir);
    #[cfg(unix)]
    for dir in [
        run.as_path(),
        std::path::Path::new(&socket).parent().unwrap_or(&run),
    ] {
        ipc::private_dir(dir)?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(&run)?;
    let _ = fs::remove_dir_all(data_dir.join("sessions"));
    let token = random_token()?;
    let file = ipc::token_file(&data_dir);
    let _ = fs::remove_file(&file);

    // Replaces the socket of a TermF that crashed; a second instance never gets here.
    let listener = ListenerOptions::new()
        .name(socket.as_str().to_fs_name::<GenericFilePath>()?)
        .try_overwrite(true)
        .create_sync()
        .with_context(|| format!("cannot listen on {socket}"))?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        options.mode(0o600);
    }
    options
        .open(&file)
        .and_then(|mut f| f.write_all(token.as_bytes()))
        .with_context(|| format!("cannot write {}", file.display()))?;

    let endpoint = Endpoint {
        data_dir,
        socket,
        token,
        session_tokens: Arc::default(),
    };
    Ok((endpoint, listener))
}

fn random_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("no randomness for an API token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// What a ticket run adds to its Claude Code session: the project whose library applies, and the first message.
pub struct RunSession<'a> {
    pub project: &'a str,
    pub prompt: &'a str,
}

/// Starts a session. A Claude Code agent gets an ephemeral plugin whose hooks report its status,
/// and Remote Control when asked; other agents and shells get neither.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    state: &AppState,
    agent: Option<&str>,
    cwd: PathBuf,
    title: String,
    cols: u16,
    rows: u16,
    remote_control: bool,
    run: Option<RunSession>,
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
    // Session folders live with the local API, which clears them at startup.
    let sessions_dir = match &state.api.endpoint {
        Some(endpoint) if is_claude => Some(endpoint.data_dir.join("sessions")),
        _ => None,
    };
    // Status needs the local API and the CLI the hooks run.
    let hooks = sessions_dir.as_ref().and_then(|_| termf_binary());

    let cwd_text = cwd.to_string_lossy().into_owned();
    let mut new_id = None;
    let spawned = state.sessions.spawn_with(|id| {
        new_id = Some(id);
        let dir = sessions_dir.as_ref().map(|s| s.join(id.to_string()));
        state.api.meta.lock().unwrap().insert(
            id,
            Meta {
                title: title.clone(),
                status: hooks.is_some().then_some(Status::Starting),
                remote_control,
                agent_session_id: None,
                dir: dir.clone(),
            },
        );
        let mut extra = Vec::new();
        if let Some(dir) = &dir {
            let plugin = dir.join("plugin");
            if let Some(termf) = &hooks {
                claude::write_plugin(&plugin, termf)?;
            }
            let prompt = library(state, &cwd, dir, run.as_ref().map(|r| r.project));
            if plugin.is_dir() {
                extra.push(format!(
                    "--plugin-dir {}",
                    shell_quote(&plugin.to_string_lossy())
                ));
            }
            if let Some(prompt) = prompt {
                extra.push(format!(
                    "--append-system-prompt-file {}",
                    shell_quote(&prompt.to_string_lossy())
                ));
            }
        }
        if remote_control {
            extra.push(format!("--remote-control {}", shell_quote(&title)));
        }
        if let Some(run) = &run {
            // shortcut: cmd.exe ends a command at a line break, so on Windows the prompt is one line; spawn
            // claude without cmd.exe to keep its lines.
            let prompt = if cfg!(windows) {
                run.prompt.replace(['\r', '\n'], " ")
            } else {
                run.prompt.to_owned()
            };
            // Before the variadic option, which would take it as a tool name.
            extra.push(shell_quote(&prompt));
            // TermF pushes, once the user delivers the run.
            extra.push(format!(
                "--disallowedTools {}",
                shell_quote("Bash(git push:*)")
            ));
        }
        let command = command.as_deref().map(|c| {
            if extra.is_empty() {
                c.to_owned()
            } else {
                claude::after_program(c, &extra.join(" "))
            }
        });
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

/// Writes the repository's library into the session folder and returns the system prompt file, if any.
/// A library that cannot be read never keeps the agent from starting.
fn library(state: &AppState, cwd: &Path, dir: &Path, project: Option<&str>) -> Option<PathBuf> {
    termf_library::Scope::new(&state.config_dir, Some(cwd), project.map(str::to_owned))
        .and_then(|scope| termf_library::materialize(&state.config_dir, &scope, dir))
        .unwrap_or_else(|e| {
            eprintln!("termf: library not loaded: {e:#}");
            None
        })
}

/// The CLI ships next to the app executable: bundled as a sidecar, or in `target/<profile>/` during development.
pub fn termf_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.with_file_name(format!("termf{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Answers the CLI and the hooks, one thread per connection.
pub fn serve(app: AppHandle, listener: Listener) -> Result<()> {
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
    let endpoint = state.api.endpoint.as_ref().context("local API disabled")?;
    let mut conn = BufReader::new(conn);
    let mut line = String::new();
    while conn.read_line(&mut line)? > 0 {
        let request = serde_json::from_str::<Request>(&line);
        line.clear();
        let request = match request {
            Ok(r) if endpoint.allows(&r) => r,
            Ok(_) => {
                return reply(
                    &mut conn,
                    &Reply::Error("token not valid for this request".into()),
                );
            }
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
            let session = spawn(state, agent.as_deref(), cwd, title, 120, 32, false, None)?;
            state.keep_awake.update(app);
            app.emit("session-opened", &session)?;
            serde_json::to_value(session)?
        }
        Call::Say { session, text } => {
            let id = ipc::find_session(&state.api.list(&state.sessions), &session)?;
            state.sessions.write(id, text.as_bytes())?;
            // Sent apart so a TUI that detects pastes, like Claude Code, reads Enter as a key press.
            std::thread::sleep(Duration::from_millis(100));
            state.sessions.write(id, b"\r")?;
            state.api.input(app, id, b"\r");
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
        Call::Run {
            ticket,
            project,
            repo,
        } => {
            let run = tickets::start(app, &ticket, project.as_deref(), repo.as_deref(), 120, 32)?;
            serde_json::to_value(run)?
        }
        Call::Deliver { run } => serde_json::to_value(tickets::deliver(app, run)?)?,
        Call::Close { run, force } => serde_json::to_value(tickets::close(app, run, force)?)?,
        Call::Watch => unreachable!("handled by the connection loop"),
    })
}
