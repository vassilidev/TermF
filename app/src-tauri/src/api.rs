//! Local API socket for the `termf` CLI, and the status agents report through their hooks.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail, ensure};
use interprocess::local_socket::{GenericFilePath, Listener, ListenerOptions, Stream, prelude::*};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use termf_claude::artifacts::Policy;
use termf_core::claude;
use termf_core::context::{self, Gauge, Limits, Next, Phase, Reset};
use termf_core::ipc::{self, Call, Reply, Request, SessionSummary, Status, StatusEvent};
use termf_term::{SessionInfo, Sessions, SpawnSpec, shell_command, shell_quote};

use crate::{AppState, schedule, tickets};

struct Meta {
    title: String,
    status: Option<Status>,
    remote_control: bool,
    agent_session_id: Option<String>,
    /// Removed with the session.
    dir: Option<PathBuf>,
    /// Claude Code sessions that report through hooks only.
    reset: Option<Reset>,
    /// A session TermF runs for itself, with no tile until it needs the user.
    hidden: bool,
    policy: Option<Policy>,
}

/// Lets the agent get back to its prompt before TermF types in it.
const TYPE_AFTER: Duration = Duration::from_millis(1500);
/// How long a step of the reset may take: writing the handoff, or reacting to what TermF typed.
const HANDOFF_TIME: Duration = Duration::from_secs(600);
const STEP_TIME: Duration = Duration::from_secs(30);

#[derive(Serialize, Clone)]
struct ContextEvent {
    id: u32,
    context: Gauge,
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
    /// The context reset settings last read from a valid `termf.toml`.
    limits: Mutex<Option<Limits>>,
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
            limits: Mutex::default(),
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
                    let mut env = vec![
                        (ipc::ENV_SOCKET.into(), socket.clone()),
                        (ipc::ENV_TOKEN.into(), token),
                        (ipc::ENV_SESSION.into(), id.to_string()),
                    ];
                    // The CLI is not always on the PATH: `/worktree-setup` finds it here.
                    if let Some(termf) = termf_binary() {
                        env.push(("TERMF_BIN".into(), termf.to_string_lossy().into_owned()));
                    }
                    env
                })
            }
            None => Sessions::default(),
        };
        sessions.on_exit(move |id| app.state::<AppState>().api.exited(&app, id))
    }

    pub fn list(&self, sessions: &Sessions) -> Vec<SessionSummary> {
        let hidden: Vec<u32> = self
            .meta
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, m)| m.hidden)
            .map(|(id, _)| *id)
            .collect();
        sessions
            .list()
            .into_iter()
            .filter(|s| !hidden.contains(&s.id))
            .map(|s| self.summary(s))
            .collect()
    }

    /// What a task's session may do, set before it started.
    pub fn policy(&self, id: u32) -> Option<Policy> {
        self.meta.lock().unwrap().get(&id)?.policy.clone()
    }

    /// Gives a hidden session its tile.
    pub fn reveal(&self, app: &AppHandle, sessions: &Sessions, id: u32) {
        if let Some(m) = self.meta.lock().unwrap().get_mut(&id) {
            m.hidden = false;
        }
        if let Some(info) = sessions.list().into_iter().find(|s| s.id == id) {
            let _ = app.emit("session-opened", self.summary(info));
        }
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
            context: meta.and_then(|m| m.reset.as_ref()).map(|r| r.gauge.clone()),
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

    /// Handles a hook, or the status line `termf statusline` reports, and returns what `termf hook`
    /// prints for Claude Code to read (null: nothing).
    fn hook(&self, app: &AppHandle, id: u32, event: &str, payload: &Value) -> Value {
        let task_reply = crate::claude::hook(app, id, event, payload);
        let text = |key| payload.get(key).and_then(Value::as_str).unwrap_or_default();
        let (agent_session, handoff) = (text("session_id"), self.handoff_file(id));
        let mut current = true;
        if !agent_session.is_empty()
            && let Some(m) = self.meta.lock().unwrap().get_mut(&id)
        {
            if event == "StatusLine" {
                // A status line still running for the conversation before /clear.
                current = m
                    .agent_session_id
                    .as_deref()
                    .is_none_or(|s| s == agent_session);
            } else {
                m.agent_session_id = Some(agent_session.into());
            }
        }
        if event == "SessionStart" && !agent_session.is_empty() {
            tickets::conversation(app, id, agent_session);
        }
        let mut status = ipc::hook_status(event, payload);
        let limits = (event == "Stop").then(|| self.limits(&app.state::<AppState>().config_dir));
        let mut then = None;
        let output = self.update(app, id, true, |reset| {
            let mut output = Value::Null;
            match event {
                "StatusLine" if current => reset.measure(&payload["context_window"]),
                "SessionStart" => {
                    if let Some(handoff_text) = reset.session_start(text("source")) {
                        output = context::session_start_output(&handoff_text, &handoff);
                        then = Some((reset.step(), context::resume_prompt(&handoff)));
                    }
                }
                "UserPromptSubmit" => reset.prompt_submitted(),
                "StopFailure" => reset.interrupted(),
                // A minute at the prompt with a step still waiting for the agent: it will not come.
                "Notification" if status == Some(Status::Idle) => {
                    reset.timeout(reset.step());
                }
                "Stop" => {
                    let busy = payload["background_tasks"]
                        .as_array()
                        .is_some_and(|tasks| !tasks.is_empty());
                    let continuing = payload["stop_hook_active"] == true;
                    let save = |text: &str| Ok(fs::write(&handoff, text)?);
                    let limits = limits.expect("read for Stop");
                    let reply = text("last_assistant_message");
                    match reset.stop(limits, busy, continuing, reply, save) {
                        Next::AskHandoff => {
                            let request = context::handoff_request(reset.gauge.percent);
                            output = context::stop_output(&request);
                        }
                        Next::Clear => then = Some((reset.step(), "/clear".into())),
                        Next::Nothing => {}
                    }
                }
                _ => {}
            }
            if let Some(status) = status {
                reset.activity(status);
            }
            // The agent is not done while it goes through the reset: no "finished" notification.
            if reset.active() && status == Some(Status::Idle) {
                status = Some(Status::Working);
            }
            output
        });
        if let Some(status) = status {
            self.set_status(app, id, status);
        }
        if let Some((step, line)) = then {
            type_later(app.clone(), id, step, line);
        }
        task_reply.unwrap_or_else(|| output.unwrap_or_default())
    }

    fn handoff_file(&self, id: u32) -> PathBuf {
        let meta = self.meta.lock().unwrap();
        let dir = meta
            .get(&id)
            .and_then(|m| m.dir.clone())
            .unwrap_or_default();
        dir.join("handoff.md")
    }

    /// The context reset settings, or the last valid ones while `termf.toml` cannot be read; none read
    /// yet, no automatic reset.
    fn limits(&self, config_dir: &Path) -> Limits {
        let mut last = self.limits.lock().unwrap();
        if let Ok(settings) = termf_core::load_settings(config_dir) {
            *last = Some(Limits {
                on: settings.context_reset,
                reset_at: settings.context_reset_at.clamp(1, 100),
            });
        }
        last.unwrap_or(Limits {
            on: false,
            reset_at: 100,
        })
    }

    /// `update` for what does not come from a hook.
    fn reset<T>(&self, app: &AppHandle, id: u32, f: impl FnOnce(&mut Reset) -> T) -> Option<T> {
        self.update(app, id, false, f)
    }

    /// Runs `f` on a Claude Code session's reset, then shows the change: the gauge in the UI, a timer
    /// for a new step, and the agent idle again when a sequence stops while it sat at its prompt (hooks
    /// bring their own status).
    fn update<T>(
        &self,
        app: &AppHandle,
        id: u32,
        from_hook: bool,
        f: impl FnOnce(&mut Reset) -> T,
    ) -> Option<T> {
        let (value, gauge, timer, idle) = {
            let mut meta = self.meta.lock().unwrap();
            let reset = meta.get_mut(&id)?.reset.as_mut()?;
            let (before, step) = (reset.gauge.clone(), reset.step());
            let value = f(reset);
            let time = match reset.gauge.phase {
                Phase::Handoff => HANDOFF_TIME,
                _ => STEP_TIME,
            };
            let timer = (reset.step() != step && reset.active()).then(|| (reset.step(), time));
            let idle = !from_hook
                && matches!(before.phase, Phase::Clearing | Phase::Resuming)
                && !reset.active();
            let gauge = (reset.gauge != before).then(|| reset.gauge.clone());
            (value, gauge, timer, idle)
        };
        if let Some(context) = gauge {
            let _ = app.emit("session-context", ContextEvent { id, context });
        }
        if let Some((step, after)) = timer {
            time_out(app.clone(), id, step, after);
        }
        if idle {
            self.set_status(app, id, Status::Idle);
        }
        Some(value)
    }

    /// The reset button: at once when the agent waits at its prompt, else at the end of its turn.
    pub fn request_reset(&self, app: &AppHandle, id: u32) -> Result<()> {
        let idle = self
            .meta
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|m| m.status == Some(Status::Idle));
        let request = self
            .reset(app, id, |r| {
                r.request(idle)
                    .then(|| (r.step(), context::handoff_request(r.gauge.percent)))
            })
            .context("only Claude Code sessions that report to TermF can be reset")?;
        if let Some((step, line)) = request
            && let Err(e) = app.state::<AppState>().sessions.type_line(id, &line)
        {
            self.reset(app, id, |r| r.write_failed(step));
            return Err(e);
        }
        Ok(())
    }

    /// Typing into a waiting session answers it (e.g. approves a tool), which no hook reports right away.
    /// Typing otherwise stops a reset under way, and keeps one from typing over a draft.
    pub fn input(&self, app: &AppHandle, id: u32, input: &[u8]) {
        let waiting = self
            .meta
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|m| m.status == Some(Status::Waiting));
        if waiting && ipc::answers_prompt(input) {
            self.set_status(app, id, Status::Working);
        } else if !waiting && ipc::is_typing(input) {
            self.reset(app, id, Reset::typed);
        }
    }

    /// The process ended: an agent that reports its status is done.
    fn exited(&self, app: &AppHandle, id: u32) {
        crate::claude::exited(app, id);
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

/// What a ticket run adds to its Claude Code session: the project whose library applies, the file that
/// holds its task, what its worktree's setup gives it (ports, Compose project name), and its conversation
/// (a new one with its first message, or one resumed).
pub struct RunSession<'a> {
    pub project: &'a str,
    pub brief: &'a Path,
    pub env: &'a BTreeMap<String, String>,
    pub args: &'a [String],
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
    // Added at the end of the agent's command.
    args: &[String],
) -> Result<SessionSummary> {
    let command = match agent {
        Some(name) => {
            let settings = termf_core::load_settings(&state.config_dir)?;
            let command = settings.agents.get(name).cloned();
            Some(command.ok_or_else(|| anyhow!("unknown agent {name}"))?)
        }
        None => None,
    };
    launch(
        state,
        agent,
        command,
        cwd,
        title,
        (cols, rows),
        remote_control,
        run,
        None,
        args,
    )
}

/// A Claude Code session TermF starts for a task of its own: its first message, and the options it needs.
pub struct Task {
    /// The `[agents]` name shown on its tile.
    pub agent: String,
    pub command: String,
    pub args: Vec<String>,
    pub prompt: String,
    /// No tile until `Api::reveal`.
    pub hidden: bool,
    /// What its `PreToolUse` hook lets it do, known before it starts. Such a session gets no library.
    pub policy: Option<Policy>,
    /// Added to the session's `--settings` file, e.g. permission rules that hold even without TermF's hooks.
    pub settings: Option<Value>,
}

/// Starts a task's session. It needs the hooks, which tell TermF what it does.
pub fn spawn_task(
    state: &AppState,
    cwd: PathBuf,
    title: String,
    task: Task,
) -> Result<SessionSummary> {
    launch(
        state,
        Some(&task.agent),
        Some(task.command.clone()),
        cwd,
        title,
        (120, 32),
        false,
        None,
        Some(&task),
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn launch(
    state: &AppState,
    agent: Option<&str>,
    command: Option<String>,
    cwd: PathBuf,
    title: String,
    (cols, rows): (u16, u16),
    remote_control: bool,
    run: Option<RunSession>,
    task: Option<&Task>,
    args: &[String],
) -> Result<SessionSummary> {
    let is_claude = command.as_deref().is_some_and(claude::is_claude);
    let remote_control = remote_control && is_claude;
    // Session folders live with the local API, which clears them at startup.
    let sessions_dir = match &state.api.endpoint {
        Some(endpoint) if is_claude => Some(endpoint.data_dir.join("sessions")),
        _ => None,
    };
    // Status needs the local API and the CLI the hooks run.
    let hooks = sessions_dir.as_ref().and_then(|_| termf_binary());
    ensure!(
        run.is_none() || hooks.is_some(),
        "a ticket run needs its guard hook, which this session cannot get"
    );
    ensure!(
        task.is_none() || hooks.is_some(),
        "this needs TermF's local API and the termf CLI next to TermF, whose hooks tell TermF what Claude does"
    );

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
                reset: hooks.is_some().then(Reset::default),
                hidden: task.is_some_and(|t| t.hidden),
                policy: task.and_then(|t| t.policy.clone()),
            },
        );
        let mut extra = Vec::new();
        if let Some(dir) = &dir {
            let plugin = dir.join("plugin");
            if let Some(termf) = &hooks {
                claude::write_plugin(&plugin, termf, run.is_some())?;
                // The status line is what measures the context.
                let settings = claude::write_settings(dir, termf, &cwd, &claude_dir())?;
                if let Some(Value::Object(more)) = task.and_then(|t| t.settings.as_ref()) {
                    let mut all: Value = serde_json::from_str(&fs::read_to_string(&settings)?)?;
                    if let Value::Object(map) = &mut all {
                        map.extend(more.clone());
                    }
                    fs::write(&settings, serde_json::to_string_pretty(&all)?)?;
                }
                extra.push(format!(
                    "--settings {}",
                    shell_quote(&settings.to_string_lossy())
                ));
            }
            let prompt = task
                .is_none_or(|t| t.policy.is_none())
                .then(|| library(state, &cwd, dir, run.as_ref().map(|r| r.project)))
                .flatten();
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
            // Before the variadic options, which would take the first message as one of their values.
            extra.extend(run.args.iter().map(|a| shell_quote(a)));
            if let Some(folder) = run.brief.parent() {
                extra.push(format!(
                    "--add-dir {}",
                    shell_quote(&folder.to_string_lossy())
                ));
            }
            // Defense in depth with the `termf guard` hook, not a guarantee: TermF pushes once the user
            // delivers the run.
            extra.push(format!(
                "--disallowedTools {}",
                shell_quote("Bash(git push:*)")
            ));
        }
        if let Some(task) = task {
            extra.extend(task.args.iter().map(|a| shell_quote(a)));
            extra.push(shell_quote(&task.prompt));
        }
        let command = command.as_deref().map(|c| {
            let mut c = if extra.is_empty() {
                c.to_owned()
            } else {
                claude::after_program(c, &extra.join(" "))
            };
            // Last, so they win over the agent's own options and may end with `-- <prompt>`.
            for arg in args {
                c = format!("{c} {}", shell_quote(arg));
            }
            c
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
            env: run
                .as_ref()
                .map(|r| r.env.clone().into_iter().collect())
                .unwrap_or_default(),
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

/// Checked before a ticket run creates anything: its ticket is untrusted, so its session never starts without
/// the `termf guard` hook, which needs the local API and the CLI.
pub fn run_guard(state: &AppState) -> Result<()> {
    let api = &state.api;
    guard_ready(api.endpoint.is_some(), api.error.as_deref(), termf_binary())
}

fn guard_ready(api_on: bool, api_error: Option<&str>, termf: Option<PathBuf>) -> Result<()> {
    ensure!(
        api_on,
        "a ticket run needs TermF's local API, whose hooks keep its agent from pushing: {}",
        api_error.unwrap_or("it is off")
    );
    ensure!(
        termf.is_some(),
        "a ticket run needs the termf CLI next to TermF (`pnpm bundle`, or `cargo build -p termf-cli` in development): its hook keeps the agent from pushing"
    );
    Ok(())
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
            let session = spawn(
                state,
                agent.as_deref(),
                cwd,
                title,
                120,
                32,
                false,
                None,
                &[],
            )?;
            state.keep_awake.update(app);
            app.emit("session-opened", &session)?;
            serde_json::to_value(session)?
        }
        Call::Say { session, text } => {
            let id = ipc::find_session(&state.api.list(&state.sessions), &session)?;
            state.sessions.type_line(id, &text)?;
            state.api.input(app, id, b"\r");
            Value::Null
        }
        Call::Hook {
            session,
            event,
            payload,
        } => state.api.hook(app, session, &event, &payload),
        Call::Run {
            ticket,
            project,
            repo,
        } => {
            let run = tickets::start(app, &ticket, project.as_deref(), repo.as_deref(), 120, 32)?;
            serde_json::to_value(run)?
        }
        Call::Deliver { run, confirm } => {
            serde_json::to_value(tickets::deliver(app, run, &confirm)?)?
        }
        Call::Close { run, force } => serde_json::to_value(tickets::close(app, run, force)?)?,
        Call::Resume { run } => serde_json::to_value(tickets::resume(app, run, 120, 32)?)?,
        Call::Schedule { task, scheduled } => {
            serde_json::to_value(schedule::run(app, &task, scheduled)?)?
        }
        Call::Watch => unreachable!("handled by the connection loop"),
    })
}

fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| termf_core::expand_tilde("~/.claude"))
}

/// Ends a reset whose step is still `step` after `after`. Not generic, unlike `Api::update` which calls it.
fn time_out(app: AppHandle, id: u32, step: u64, after: Duration) {
    thread::spawn(move || {
        thread::sleep(after);
        app.state::<AppState>()
            .api
            .reset(&app, id, |r| r.timeout(step));
    });
}

/// Types `line` once the agent is back at its prompt, if the reset step that asked for it is still on.
fn type_later(app: AppHandle, id: u32, step: u64, line: String) {
    thread::spawn(move || {
        thread::sleep(TYPE_AFTER);
        let state = app.state::<AppState>();
        let due = state
            .api
            .reset(&app, id, |r| r.clear_due(step) || r.resume_due(step));
        if due == Some(true) && state.sessions.type_line(id, &line).is_err() {
            state.api.reset(&app, id, |r| r.write_failed(step));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_needs_its_guard() {
        let termf = || Some(PathBuf::from("/opt/termf"));
        assert!(guard_ready(true, None, termf()).is_ok());
        let no_cli = guard_ready(true, None, None).unwrap_err().to_string();
        assert!(no_cli.contains("needs the termf CLI"), "{no_cli}");
        let no_api = guard_ready(false, Some("cannot listen"), termf())
            .unwrap_err()
            .to_string();
        assert!(
            no_api.contains("local API") && no_api.contains("cannot listen"),
            "{no_api}"
        );
    }
}
