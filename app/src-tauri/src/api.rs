//! Local API socket for the `termf` CLI, and the status agents report through their hooks.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use interprocess::local_socket::{GenericFilePath, Listener, ListenerOptions, Stream, prelude::*};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_claude::artifacts::Policy;
use termf_core::claude::{self, Tool};
use termf_core::context::{self, Gauge, Limits, Next, Phase, Reset};
use termf_core::ipc::{
    self, AgentCall, Call, Reply, Request, Reserved, SessionSummary, Status, StatusEvent,
};
use termf_core::remote;
use termf_core::{agent_env, cli_agents};
use termf_term::{SessionInfo, Sessions, SpawnSpec, shell_command, shell_quote};

use crate::remote::{Relaunch, Restart, Step, Tile};
use crate::{AppState, agents, schedule, team, tickets, visual};

struct Meta {
    title: String,
    status: Option<Status>,
    /// When `status` last changed, or the session started.
    since: Instant,
    remote_control: bool,
    agent_session_id: Option<String>,
    /// Removed with the session.
    dir: Option<PathBuf>,
    /// Claude Code sessions that report through hooks only.
    reset: Option<Reset>,
    /// A session TermF runs for itself, with no tile until it needs the user.
    hidden: bool,
    policy: Option<Policy>,
    /// The switch of a Claude Code tile that reports to TermF; `remote_control` says how the others started.
    remote: Option<Tile>,
    /// A Codex CLI terminal's prompt: Codex says when a turn ends, not when one starts.
    typed: Option<ipc::Typed>,
}

/// How often a Codex turn TermF guessed is checked, and how long a quiet terminal takes to end it.
const QUIET_CHECK: Duration = Duration::from_secs(5);
const QUIET_AFTER: Duration = Duration::from_secs(20);
/// Lets the agent get back to its prompt before TermF types in it.
const TYPE_AFTER: Duration = Duration::from_millis(1500);
/// How long a step of the reset may take: writing the handoff, or reacting to what TermF typed.
const HANDOFF_TIME: Duration = Duration::from_secs(600);
const STEP_TIME: Duration = Duration::from_secs(30);
const CONFIRM_TIME: Duration = Duration::from_secs(300);
/// How often a call that waits for the user checks that its caller is still there.
const CALLER_CHECK: Duration = Duration::from_millis(250);

#[derive(Serialize, Clone)]
struct ContextEvent {
    id: u32,
    context: Gauge,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteEvent {
    id: u32,
    remote_control: bool,
    remote: Option<remote::State>,
}

struct Endpoint {
    data_dir: PathBuf,
    socket: String,
    /// The user's token, also in the token file.
    token: String,
    /// Session id → the token its hooks report with, until its process ends.
    session_tokens: Arc<Mutex<HashMap<u32, String>>>,
    /// Agent session id → the token of its `termf mcp`, which only calls TermF's tools for it.
    mcp_tokens: Mutex<HashMap<i64, String>>,
}

impl Endpoint {
    fn allows(&self, request: &Request) -> bool {
        let sessions = self.session_tokens.lock().unwrap();
        let mcp = self.mcp_tokens.lock().unwrap();
        ipc::access(&request.token, &self.token, &sessions, &mcp)
            .is_some_and(|a| a.allows(&request.call))
    }

    /// A fresh token for one process of the session, which voids the one before.
    fn issue_mcp(&self, session: i64) -> Result<String> {
        let token = random_token()?;
        self.mcp_tokens
            .lock()
            .unwrap()
            .insert(session, token.clone());
        Ok(token)
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Confirm {
    id: u64,
    #[serde(flatten)]
    reserved: Reserved,
}

#[derive(Default)]
struct Confirms {
    next: AtomicU64,
    pending: Mutex<BTreeMap<u64, (Confirm, mpsc::Sender<bool>)>>,
}

impl Confirms {
    /// `show` hears `api-confirm`, then `api-confirm-done`. A caller that went away (Ctrl-C, a command that timed
    /// out) gets nothing done: allowing it later would act for nobody.
    fn ask(
        &self,
        reserved: Reserved,
        wait: Duration,
        show: impl Fn(&str, &Confirm),
        gone: impl Fn() -> bool,
    ) -> Result<()> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let confirm = Confirm { id, reserved };
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap()
            .insert(id, (confirm.clone(), tx));
        show("api-confirm", &confirm);
        let deadline = Instant::now() + wait;
        let answer = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left.min(CALLER_CHECK)) {
                Ok(allow) => break Ok(allow),
                Err(_) if gone() => break Err(anyhow!("the caller went away")),
                Err(_) if left.is_zero() => {
                    break Err(anyhow!(
                        "not confirmed in TermF within {} minutes",
                        wait.as_secs() / 60
                    ));
                }
                Err(_) => {}
            }
        };
        self.pending.lock().unwrap().remove(&id);
        show("api-confirm-done", &confirm);
        match answer? {
            true => Ok(()),
            false => bail!("refused in TermF"),
        }
    }

    fn answer(&self, id: u64, allow: bool) {
        if let Some((_, tx)) = self.pending.lock().unwrap().get(&id) {
            let _ = tx.send(allow);
        }
    }

    fn list(&self) -> Vec<Confirm> {
        let pending = self.pending.lock().unwrap();
        pending.values().map(|(c, _)| c.clone()).collect()
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
    /// Read once: an agent that edits `termf.toml` changes nothing before the user starts TermF again.
    confirm_cli: bool,
    confirms: Confirms,
}

impl Api {
    /// Sets up the local API, or one that is off (and says why) when that fails.
    pub fn new(confirm_cli: bool) -> (Self, Option<Listener>) {
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
            confirm_cli,
            confirms: Confirms::default(),
        };
        (api, listener)
    }

    /// `[api] confirm_cli` as read at startup, which is what applies until TermF starts again.
    pub fn confirm_cli(&self) -> bool {
        self.confirm_cli
    }

    fn confirm(&self, app: &AppHandle, call: &Call, caller: &Stream) -> Result<()> {
        let Some(mut reserved) = ipc::reserved(call).filter(|_| self.confirm_cli) else {
            return Ok(());
        };
        match call {
            Call::Agent(AgentCall::Answer {
                session, request, ..
            }) => reserved
                .args
                .extend(agents::permission(app, *session, request)),
            Call::Say { session, .. } => {
                let sessions = self.list(&app.state::<AppState>().sessions);
                if let Ok(id) = ipc::find_session(&sessions, session) {
                    let title = sessions
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| s.title.clone());
                    reserved.args.extend(title.map(|t| ("title", t)));
                }
            }
            _ => {}
        }
        let show = |event: &str, confirm: &Confirm| {
            let _ = app.emit(event, confirm);
        };
        self.confirms
            .ask(reserved, CONFIRM_TIME, show, || hung_up(caller))
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

    /// Whether a session's agent is at work, or stopped less than `recent` ago. One that reports no status may be
    /// at work too.
    pub fn busy(&self, id: u32, recent: Duration) -> bool {
        self.meta
            .lock()
            .unwrap()
            .get(&id)
            .is_none_or(|m| match m.status {
                Some(Status::Idle | Status::Done) => m.since.elapsed() < recent,
                _ => true,
            })
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
            remote_control: meta
                .is_some_and(|m| m.remote.as_ref().map_or(m.remote_control, Tile::on)),
            agent_session_id: meta.and_then(|m| m.agent_session_id.clone()),
            context: meta.and_then(|m| m.reset.as_ref()).map(|r| r.gauge.clone()),
            remote: meta.and_then(|m| m.remote.as_ref()).map(Tile::state),
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
            m.since = Instant::now();
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
        if status == Status::Idle {
            self.remote_later(app, id);
        }
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
        self.remote_seen(app, id, event, payload);
        if event == "StatusLine" && current {
            crate::claude_native::status_line(app, id, payload);
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

    /// A tile's switch. `/remote-control <name>` turns it on in the running process; off, or on for a process that
    /// cannot connect, takes the same conversation to a new process (`--resume`), kept from connecting when off.
    /// Either waits for the agent to be at its prompt, unless `now`.
    pub fn remote_set(&self, app: &AppHandle, id: u32, on: bool, now: bool) -> Result<()> {
        let data_dir = self.endpoint.as_ref().map(|e| e.data_dir.clone());
        let (at_prompt, newly_locked, done) = {
            let mut meta = self.meta.lock().unwrap();
            let m = meta.get_mut(&id).context("unknown session")?;
            let at_prompt = now || m.status == Some(Status::Idle);
            let conversation = m.agent_session_id.clone();
            let tile = m.remote.as_mut().context(
                "only the Claude Code sessions that report to TermF have the Remote Control switch",
            )?;
            // Marked by Ops since its last hook, an idle session would otherwise connect.
            let conversation = conversation.or_else(|| tile.relaunch.conversation.clone());
            let prod = on
                && data_dir.as_deref().is_some_and(|d| {
                    crate::remote::prod_read(d, conversation.as_deref(), tile.run)
                });
            let newly_locked = prod && !tile.locked;
            (at_prompt, newly_locked, tile.set(on, prod))
        };
        self.remote_changed(app, id);
        if newly_locked {
            self.remote_now(app, id, true)?;
        }
        done?;
        if at_prompt {
            self.remote_now(app, id, now)?;
        }
        Ok(())
    }

    /// Every session off at once, mid-turn or not: the tiles in a new process, those started with Remote Control
    /// but without TermF's hooks by ending their process. Returns how many were on.
    pub fn remote_cut_all(&self, app: &AppHandle) -> usize {
        let (tiles, bare, changed) = cut_all(&mut self.meta.lock().unwrap());
        for &id in &changed {
            self.remote_changed(app, id);
        }
        for &id in &tiles {
            if let Err(e) = self.remote_now(app, id, true) {
                eprintln!("termf: session {id}: Remote Control not cut: {e:#}");
            }
        }
        let sessions = &app.state::<AppState>().sessions;
        let running: Vec<u32> = sessions
            .list()
            .iter()
            .filter(|s| s.running)
            .map(|s| s.id)
            .collect();
        let bare: Vec<u32> = bare.into_iter().filter(|id| running.contains(id)).collect();
        for &id in &bare {
            if let Err(e) = sessions.terminate(id) {
                eprintln!("termf: session {id}: not ended: {e:#}");
            }
            let event = RemoteEvent {
                id,
                remote_control: false,
                remote: None,
            };
            let _ = app.emit("session-remote", event);
        }
        tiles.len() + bare.len()
    }

    /// Does what the switch waits for. Without `force`, a draft in the prompt or a context reset under way keeps it
    /// waiting: typing would join the draft, a new process would lose it.
    fn remote_now(&self, app: &AppHandle, id: u32, force: bool) -> Result<()> {
        let (step, name) = {
            let mut meta = self.meta.lock().unwrap();
            let Some(m) = meta.get_mut(&id) else {
                return Ok(());
            };
            let busy = !force && m.reset.as_ref().is_some_and(|r| r.draft() || r.active());
            let Some(tile) = m.remote.as_mut() else {
                return Ok(());
            };
            let step = tile.next();
            tile.pending = busy && step != Step::Nothing;
            if tile.pending {
                return Ok(());
            }
            if step == Step::Type {
                tile.asked = true;
            }
            (step, tile.name.clone())
        };
        let done = match step {
            Step::Nothing => Ok(()),
            Step::Type => app
                .state::<AppState>()
                .sessions
                .type_line(id, &format!("/remote-control {name}")),
            Step::Relaunch => self.relaunch(app, id),
        };
        self.remote_changed(app, id);
        done
    }

    /// The agent is back at its prompt: what the switch waits for, once it had time to draw it.
    fn remote_later(&self, app: &AppHandle, id: u32) {
        let pending = self
            .meta
            .lock()
            .unwrap()
            .get(&id)
            .and_then(|m| m.remote.as_ref())
            .is_some_and(|t| t.pending);
        if !pending {
            return;
        }
        let app = app.clone();
        thread::spawn(move || {
            thread::sleep(TYPE_AFTER);
            let api = &app.state::<AppState>().api;
            let idle = api
                .meta
                .lock()
                .unwrap()
                .get(&id)
                .is_some_and(|m| m.status == Some(Status::Idle));
            if idle && let Err(e) = api.remote_now(&app, id, false) {
                eprintln!("termf: session {id}: Remote Control: {e:#}");
            }
        });
    }

    /// The tile's conversation in a new process, connected or kept from connecting as the switch and the lock want.
    fn relaunch(&self, app: &AppHandle, id: u32) -> Result<()> {
        let state = app.state::<AppState>();
        let live = state
            .sessions
            .list()
            .into_iter()
            .find(|s| s.id == id && s.running);
        let (relaunch, conversation, connect) = {
            let mut meta = self.meta.lock().unwrap();
            let m = meta.get_mut(&id).context("unknown session")?;
            let known = m.agent_session_id.clone();
            let tile = m.remote.as_mut().context("no Remote Control here")?;
            let restart = tile.restart(known.as_deref())?;
            match (live.is_some(), restart) {
                (
                    true,
                    Restart::In {
                        conversation,
                        connect,
                    },
                ) => (tile.relaunch.clone(), conversation, connect),
                // Its process ended, and its connection with it; or it has no conversation yet, nothing to lose.
                (live, _) => {
                    (tile.bridge, tile.asked) = (None, false);
                    drop(meta);
                    if live {
                        state.sessions.terminate(id)?;
                    }
                    return Ok(());
                }
            }
        };
        if let Some(settings) = &relaunch.settings {
            crate::remote::block(settings, connect.is_none())?;
        }
        let mut line = relaunch.line(Some(&conversation), connect.as_deref())?;
        let mut env = relaunch.env.clone();
        if let Some(allowlist) = &relaunch.allowlist {
            let names: Vec<String> = env.iter().map(|(k, _)| k.clone()).collect();
            let cli = termf_binary();
            let tool = Some(Tool::Claude);
            if let Some((wrapped, spec)) =
                allowlist.wrap(&line, tool, cli.as_deref(), &names, shell_quote)
            {
                line = wrapped;
                env.push((agent_env::SPEC.into(), spec));
            }
        }
        let (program, args) = shell_command(Some(&line));
        let info = live.expect("checked above");
        // The process replaced may still fire hooks (its SessionEnd, with its old connection): none counts.
        self.revoke_session(id);
        state.sessions.respawn_with(id, |_| {
            Ok(SpawnSpec {
                title: info.title,
                program,
                args,
                cwd: relaunch.cwd.clone(),
                agent: info.agent,
                cols: 120,
                rows: 32,
                env,
                base_env: relaunch.base_env.clone(),
            })
        })?;
        if let Some(tile) = self
            .meta
            .lock()
            .unwrap()
            .get_mut(&id)
            .and_then(|m| m.remote.as_mut())
        {
            tile.asked = connect.is_some();
            tile.blocked = connect.is_none();
            tile.bridge = None;
        }
        self.set_status(app, id, Status::Starting);
        Ok(())
    }

    /// What a hook says of Remote Control: connected or not, and whether the session read production data, which
    /// cuts it at once.
    fn remote_seen(&self, app: &AppHandle, id: u32, event: &str, payload: &Value) {
        let bridge = payload.get(remote::PAYLOAD_BRIDGE).and_then(Value::as_str);
        // The status line may run without the hooks' environment: only a hook tells it is off.
        if event == "StatusLine" && bridge.is_none() {
            return;
        }
        let data_dir = self.endpoint.as_ref().map(|e| e.data_dir.clone());
        let (changed, cut) = {
            let mut meta = self.meta.lock().unwrap();
            let Some(m) = meta.get_mut(&id) else {
                return;
            };
            let conversation = m.agent_session_id.clone();
            let Some(tile) = m.remote.as_mut() else {
                return;
            };
            let locked = data_dir
                .as_deref()
                .is_some_and(|d| crate::remote::prod_read(d, conversation.as_deref(), tile.run));
            let before = tile.state();
            let cut = tile.seen(bridge, locked);
            (tile.state() != before, cut)
        };
        if changed {
            self.remote_changed(app, id);
        }
        if cut {
            let app = app.clone();
            // After this hook's answer: the process it came from is the one cut.
            thread::spawn(move || {
                thread::sleep(TYPE_AFTER);
                if let Err(e) = app.state::<AppState>().api.remote_now(&app, id, true) {
                    eprintln!("termf: session {id}: Remote Control not cut: {e:#}");
                }
            });
        }
    }

    fn remote_changed(&self, app: &AppHandle, id: u32) {
        let event = self.meta.lock().unwrap().get(&id).and_then(|m| {
            let tile = m.remote.as_ref()?;
            Some(RemoteEvent {
                id,
                remote_control: tile.on(),
                remote: Some(tile.state()),
            })
        });
        if let Some(event) = event {
            let _ = app.emit("session-remote", event);
        }
    }

    /// Typing into a waiting session answers it (e.g. approves a tool), which no hook reports right away.
    /// Typing otherwise stops a reset under way, and keeps one from typing over a draft.
    /// At Codex CLI's prompt, a message sent starts a turn, and Esc during one stops it.
    pub fn input(&self, app: &AppHandle, id: u32, input: &[u8]) {
        let (waiting, sent, stopped) = {
            let mut meta = self.meta.lock().unwrap();
            let Some(m) = meta.get_mut(&id) else {
                return;
            };
            let status = m.status;
            let sent = m.typed.as_mut().is_some_and(|t| t.send(input));
            let stopped = m.typed.is_some() && status == Some(Status::Working) && input == b"\x1b";
            (
                status == Some(Status::Waiting),
                sent && status == Some(Status::Idle),
                stopped,
            )
        };
        if (waiting && ipc::answers_prompt(input)) || sent {
            self.set_status(app, id, Status::Working);
            if sent {
                self.watch_quiet(app, id);
            }
        } else if stopped {
            self.set_status(app, id, Status::Idle);
        } else if !waiting && ipc::is_typing(input) {
            self.reset(app, id, Reset::typed);
        }
    }

    /// A Codex turn TermF saw start ends on its own once the terminal stays quiet: Codex redraws its timer every
    /// second while it works, and an interrupted turn sends no end.
    fn watch_quiet(&self, app: &AppHandle, id: u32) {
        let since = self.meta.lock().unwrap().get(&id).map(|m| m.since);
        let app = app.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(QUIET_CHECK);
                let state = app.state::<AppState>();
                let same =
                    state.api.meta.lock().unwrap().get(&id).is_some_and(|m| {
                        m.status == Some(Status::Working) && Some(m.since) == since
                    });
                if !same {
                    return;
                }
                if state
                    .sessions
                    .quiet_for(id)
                    .is_some_and(|q| q >= QUIET_AFTER)
                {
                    state.api.set_status(&app, id, Status::Idle);
                    return;
                }
            }
        });
    }

    /// The process ended: an agent that reports its status is done.
    fn exited(&self, app: &AppHandle, id: u32) {
        self.revoke_session(id);
        crate::claude::exited(app, id);
        agents::tile_exited(app, id);
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

    /// What a session's process left running (a server, a script) no longer reports through its token.
    fn revoke_session(&self, id: u32) {
        if let Some(endpoint) = &self.endpoint {
            endpoint.session_tokens.lock().unwrap().remove(&id);
        }
    }

    pub fn forget(&self, id: u32) {
        self.revoke_session(id);
        if let Some(Meta { dir: Some(dir), .. }) = self.meta.lock().unwrap().remove(&id) {
            let _ = fs::remove_dir_all(dir);
        }
    }

    /// The `termf mcp` server of agent session `session`: the CLI, given the socket and a fresh token of its own
    /// (never the user's), which only calls TermF's tools for that session.
    pub fn mcp_server(&self, session: i64) -> Result<Value> {
        let endpoint = self
            .endpoint
            .as_ref()
            .context("TermF's tools need its local API, which is off")?;
        let termf = termf_binary().context(
            "TermF's tools need the termf CLI next to TermF (`pnpm bundle`, or `cargo build -p termf-cli` in development)",
        )?;
        let token = endpoint.issue_mcp(session)?;
        let env = serde_json::Map::from_iter([
            (
                ipc::ENV_SOCKET.to_owned(),
                Value::from(endpoint.socket.clone()),
            ),
            (ipc::ENV_MCP_TOKEN.to_owned(), token.into()),
            (ipc::ENV_MCP_SESSION.to_owned(), session.to_string().into()),
        ]);
        Ok(serde_json::json!({
            "type": "stdio",
            "command": termf,
            "args": ["mcp"],
            "env": env,
        }))
    }

    /// The process of agent session `session` ended: its `termf mcp` token no longer opens anything.
    pub fn revoke_mcp(&self, session: i64) {
        if let Some(endpoint) = &self.endpoint {
            endpoint.mcp_tokens.lock().unwrap().remove(&session);
        }
    }
}

/// Writes a fresh user token to a file only the user can read, in a folder no session is given, and listens on
/// the socket. No session outlives the app, so old session folders go, and the token the sessions could find
/// next to the socket before.
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
    let _ = fs::remove_file(run.join("token"));
    let token = random_token()?;

    // Replaces the socket of a TermF that crashed; a second instance never gets here.
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
    ipc::write_token(&ipc::user_token_file()?, &token)?;

    let endpoint = Endpoint {
        data_dir,
        socket,
        token,
        session_tokens: Arc::default(),
        mcp_tokens: Mutex::default(),
    };
    Ok((endpoint, listener))
}

fn random_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow!("no randomness for an API token: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// What "cut all" reaches, each switched off: the tiles that were on or on their way, the sessions started with
/// Remote Control but without TermF's hooks (only their end cuts them), and every tile whose state changed (a
/// question gone too).
fn cut_all(meta: &mut HashMap<u32, Meta>) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let (mut tiles, mut bare, mut changed) = (Vec::new(), Vec::new(), Vec::new());
    for (&id, m) in meta.iter_mut() {
        match m.remote.as_mut() {
            Some(tile) => {
                let before = tile.state();
                if tile.on() || tile.asked {
                    tiles.push(id);
                }
                tile.want = false;
                tile.ask = false;
                if tile.state() != before {
                    changed.push(id);
                }
            }
            None if m.remote_control => {
                m.remote_control = false;
                bare.push(id);
            }
            None => {}
        }
    }
    (tiles, bare, changed)
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

/// What a Claude Code tile's Remote Control starts with (ARCHITECTURE §6.6): the user's choice at launch, else the
/// project's and the role's mode; and what its name on the phone, `<project>/<ticket>/<role>`, is made of. The project
/// is the one that lists the folder unless given; the folder's name stands for a ticket.
#[derive(Default)]
pub struct Remote<'a> {
    pub on: Option<bool>,
    pub project: Option<&'a str>,
    pub ticket: Option<&'a str>,
    pub role: Option<&'a str>,
    /// The ticket run the session works for.
    pub run: Option<i64>,
}

/// Starts a session. A Claude Code agent gets an ephemeral plugin whose hooks report its status,
/// and Remote Control as `remote` says; other agents and shells get neither.
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    state: &AppState,
    agent: Option<&str>,
    cwd: PathBuf,
    title: String,
    cols: u16,
    rows: u16,
    remote: Remote,
    run: Option<RunSession>,
    // Added at the end of the agent's command.
    args: &[String],
) -> Result<SessionSummary> {
    let guard = run.is_some();
    launch(
        state,
        agent,
        agent_command(state, agent)?,
        cwd,
        title,
        (cols, rows),
        remote,
        run,
        None,
        guard,
        args,
    )
}

/// Starts a session whose Bash commands go through the `termf guard` hook, as a ticket run's: a headless
/// session of a team's step, taken over in a terminal.
pub fn spawn_guarded(
    state: &AppState,
    agent: &str,
    cwd: PathBuf,
    title: String,
    (cols, rows): (u16, u16),
    remote: Remote,
    args: &[String],
) -> Result<SessionSummary> {
    launch(
        state,
        Some(agent),
        agent_command(state, Some(agent))?,
        cwd,
        title,
        (cols, rows),
        remote,
        None,
        None,
        true,
        args,
    )
}

fn agent_command(state: &AppState, agent: Option<&str>) -> Result<Option<String>> {
    let Some(name) = agent else {
        return Ok(None);
    };
    let settings = termf_core::load_settings(&state.config_dir)?;
    let command = settings.agent(name);
    Ok(Some(
        command.ok_or_else(|| anyhow!("unknown agent {name}"))?,
    ))
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
        Remote {
            on: Some(false),
            ..Remote::default()
        },
        None,
        Some(&task),
        false,
        &[],
    )
}

/// A command of Claude Code's own in a tile, as typed (`claude auth login`, `claude attach <id>`): no hooks, no
/// library, nothing added to it. No `SECRET_DENY` either: neither takes `--settings`, login runs no tool, and
/// attach opens a session whose settings were set when it started. Conversations resumed go through `spawn`.
pub fn spawn_command(
    state: &AppState,
    cwd: PathBuf,
    title: String,
    command: &str,
) -> Result<SessionSummary> {
    let cwd_text = cwd.to_string_lossy().into_owned();
    let (program, args) = shell_command(Some(command));
    let mut new_id = None;
    let spawned = state.sessions.spawn_with(|id| {
        new_id = Some(id);
        state.api.meta.lock().unwrap().insert(
            id,
            Meta {
                title: title.clone(),
                status: None,
                since: Instant::now(),
                remote_control: false,
                agent_session_id: None,
                dir: None,
                reset: None,
                hidden: false,
                policy: None,
                remote: None,
                typed: None,
            },
        );
        Ok(SpawnSpec {
            title: title.clone(),
            program,
            args,
            cwd,
            agent: None,
            cols: 120,
            rows: 32,
            env: Vec::new(),
            base_env: None,
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
        agent: None,
        running: true,
        exit_code: None,
    }))
}

#[allow(clippy::too_many_arguments)]
fn launch(
    state: &AppState,
    agent: Option<&str>,
    command: Option<String>,
    cwd: PathBuf,
    title: String,
    (cols, rows): (u16, u16),
    remote: Remote,
    run: Option<RunSession>,
    task: Option<&Task>,
    guard: bool,
    args: &[String],
) -> Result<SessionSummary> {
    let tool = command.as_deref().and_then(Tool::of);
    let is_claude = tool == Some(Tool::Claude);
    // TermF's own tasks never get Remote Control.
    let remote_tile = is_claude && task.is_none();
    let (project, policy) = match remote_tile {
        true => crate::remote::project_policy(&state.config_dir, remote.project, &cwd),
        false => Default::default(),
    };
    let conversation = remote::conversation(run.as_ref().map_or(&[], |r| r.args))
        .or_else(|| remote::conversation(args));
    let locked = remote_tile
        && state
            .api
            .endpoint
            .as_ref()
            .is_some_and(|e| crate::remote::prod_read(&e.data_dir, conversation, remote.run));
    let (remote_control, ask) = remote::at_start(remote.on, policy.mode(remote.role), locked);
    let remote_control = remote_control && remote_tile;
    let folder = cwd.file_name().map(|f| f.to_string_lossy().into_owned());
    let mut remote_name = remote::name(
        project.as_deref(),
        remote.ticket.or(folder.as_deref()),
        remote.role,
    );
    if remote_name.is_empty() {
        remote_name = title.clone();
    }
    // Session folders live with the local API, which clears them at startup.
    let sessions_dir = match &state.api.endpoint {
        Some(endpoint) if tool.is_some() => Some(endpoint.data_dir.join("sessions")),
        _ => None,
    };
    // A Codex CLI or Gemini CLI older than TermF needs runs as it is, saying so in its terminal.
    let (codex_or_gemini, old) = match tool {
        Some(tool @ (Tool::Codex | Tool::Gemini)) => {
            let program = command.as_deref().unwrap_or_default();
            let program = program.split_whitespace().next().unwrap_or_default();
            (
                Some(tool),
                claude::binary(program).err().map(|e| format!("{e:#}")),
            )
        }
        _ => (None, None),
    };
    // Status needs the local API and the CLI the hooks run.
    let hooks = sessions_dir
        .as_ref()
        .filter(|_| old.is_none())
        .and_then(|_| termf_binary());
    ensure!(
        !guard || hooks.is_some(),
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
                // Codex CLI and Gemini CLI start at their prompt.
                status: hooks.is_some().then_some(if is_claude {
                    Status::Starting
                } else {
                    Status::Idle
                }),
                since: Instant::now(),
                remote_control,
                agent_session_id: None,
                dir: dir.clone(),
                reset: (hooks.is_some() && is_claude).then(Reset::default),
                hidden: task.is_some_and(|t| t.hidden),
                policy: task.and_then(|t| t.policy.clone()),
                remote: None,
                typed: (hooks.is_some() && tool == Some(Tool::Codex)).then(ipc::Typed::default),
            },
        );
        let mut extra = Vec::new();
        let mut settings_file = None;
        let mut env: Vec<(String, String)> = run
            .as_ref()
            .map(|r| r.env.clone().into_iter().collect())
            .unwrap_or_default();
        let mut warnings: Vec<String> = old.iter().cloned().collect();
        // An agent starts with the allowlist and what TermF gives it: a shell keeps all of TermF's environment.
        let allowlist = command.as_ref().map(|_| {
            let run_project = run.as_ref().map(|r| r.project);
            let project = project_for(&state.config_dir, remote.project, run_project, &cwd);
            let actor = termf_vault::Actor::agent(None, Some(format!("terminal {id}")));
            let project = project.as_deref();
            let given = crate::agentenv::given(&state.vault, &state.config_dir, project, &actor);
            env.extend(given.vars);
            warnings.extend(given.note);
            agent_env::load(&state.config_dir, project)
        });
        let base_env = allowlist
            .as_ref()
            .filter(|p| !p.inherit)
            .map(|p| p.keep(tool, std::env::vars_os()));
        if let (Some(tool), None) = (codex_or_gemini, &old) {
            let prompt = dir.as_ref().and_then(|dir| library(state, &cwd, dir, None));
            let tile = cli_agents::tile(
                tool,
                command.as_deref().unwrap_or_default(),
                dir.as_deref(),
                hooks.as_deref(),
                prompt.as_deref(),
                &termf_core::default_data_dir()?,
            )?;
            extra.extend(tile.args.iter().map(|a| shell_quote(a)));
            env.extend(tile.env);
            warnings.extend(tile.warnings);
        }
        if let Some(dir) = dir.as_ref().filter(|_| is_claude) {
            let plugin = dir.join("plugin");
            if let Some(termf) = &hooks {
                claude::write_plugin(&plugin, termf, guard)?;
                if let Some(endpoint) = &state.api.endpoint
                    && termf_visual::agent::for_tiles(&state.config_dir, &cwd)
                {
                    // The project gives its terminals the agents' browser: a `termf mcp` with its tools only.
                    let server = serde_json::json!({ "mcpServers": { "browser": {
                        "command": termf,
                        "args": ["mcp"],
                        "env": {
                            ipc::ENV_MCP_TOOLS: ipc::MCP_BROWSER.join(","),
                            ipc::ENV_CONFIG_DIR: state.config_dir,
                            ipc::ENV_DATA_DIR: endpoint.data_dir,
                        },
                    } } });
                    fs::write(plugin.join(".mcp.json"), server.to_string())?;
                }
                // The status line is what measures the context.
                let settings = claude::write_settings(dir, termf, &cwd, &claude_dir())?;
                if let Some(more @ Value::Object(_)) = task.and_then(|t| t.settings.as_ref()) {
                    let mut all: Value = serde_json::from_str(&fs::read_to_string(&settings)?)?;
                    claude::merge_settings(&mut all, more);
                    fs::write(&settings, serde_json::to_string_pretty(&all)?)?;
                }
                if remote_tile && locked {
                    crate::remote::block(&settings, true)?;
                }
                extra.push(format!(
                    "--settings {}",
                    shell_quote(&settings.to_string_lossy())
                ));
                settings_file = Some(settings);
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
        // Without TermF's hooks (no local API, no termf CLI), the secret files stay denied all the same.
        if is_claude && hooks.is_none() {
            let settings = claude::write_deny_settings(&termf_core::default_data_dir()?)?;
            extra.push(format!(
                "--settings {}",
                shell_quote(&settings.to_string_lossy())
            ));
        }
        let before = extra.clone();
        if remote_control {
            extra.push(format!("--remote-control {}", shell_quote(&remote_name)));
        }
        let mut after = Vec::new();
        if let Some(run) = &run {
            // Before the variadic options, which would take the first message as one of their values.
            extra.extend(run.args.iter().map(|a| shell_quote(a)));
            if let Some(folder) = run.brief.parent() {
                after.push(format!(
                    "--add-dir {}",
                    shell_quote(&folder.to_string_lossy())
                ));
            }
            // Defense in depth with the `termf guard` hook, not a guarantee: TermF pushes once the user
            // delivers the run.
            after.push(format!(
                "--disallowedTools {}",
                shell_quote("Bash(git push:*)")
            ));
            extra.extend(after.iter().cloned());
        }
        if remote_tile
            && hooks.is_some()
            && let Some(m) = state.api.meta.lock().unwrap().get_mut(&id)
        {
            m.remote = Some(Tile {
                name: remote_name.clone(),
                want: remote_control,
                asked: remote_control,
                blocked: locked,
                ask,
                locked,
                run: remote.run,
                relaunch: Relaunch {
                    command: command.clone().unwrap_or_default(),
                    before,
                    after,
                    trailing: remote::strip_conversation(args),
                    cwd: cwd.clone(),
                    env: env.clone(),
                    settings: settings_file,
                    conversation: conversation.map(str::to_owned),
                    base_env: base_env.clone(),
                    allowlist: allowlist.clone(),
                },
                ..Tile::default()
            });
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
        // Last: a path with a space would be cut by `after_program`.
        let command = match command {
            Some(c) if tool.is_some() && old.is_none() => {
                Some(claude::in_command(&c, shell_quote)?.0)
            }
            other => other,
        };
        // Before the lines it prints first: the program the wrapper resolves is the agent's.
        let mut command = command;
        if let (Some(allowlist), Some(c)) = (&allowlist, &command) {
            let names: Vec<String> = env.iter().map(|(k, _)| k.clone()).collect();
            let cli = termf_binary();
            if let Some((wrapped, spec)) =
                allowlist.wrap(c, tool, cli.as_deref(), &names, shell_quote)
            {
                command = Some(wrapped);
                env.push((agent_env::SPEC.into(), spec));
            }
        }
        let command = command.map(|c| cli_agents::after_lines(&c, &warnings, shell_quote));
        let (program, args) = shell_command(command.as_deref());
        Ok(SpawnSpec {
            title: title.clone(),
            program,
            args,
            cwd,
            agent: agent.map(str::to_owned),
            cols,
            rows,
            env,
            base_env,
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

/// An agent's project: the caller's (a step's worktree does not say it), else its run's, else its folder's.
fn project_for(
    config_dir: &Path,
    given: Option<&str>,
    run: Option<&str>,
    cwd: &Path,
) -> Option<String> {
    let known = given.or(run).map(str::to_owned);
    known.or_else(|| termf_core::project_of(config_dir, cwd).ok().flatten())
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
        // Only the user's token makes such a call, and whoever holds it may be an agent that runs the CLI: the
        // user decides in the app.
        let allowed = match ipc::refused(&request.call) {
            Some(why) => Err(anyhow!(why)),
            None => state.api.confirm(app, &request.call, conn.get_ref()),
        };
        let result = match allowed.and_then(|()| call(app, &state, request.call, conn.get_ref())) {
            Ok(value) => Reply::Ok(value),
            Err(e) => Reply::Error(format!("{e:#}")),
        };
        reply(&mut conn, &result)?;
    }
    Ok(())
}

/// Whether the client closed the connection; nothing it sends is expected while it waits for a reply.
#[cfg(unix)]
fn hung_up(caller: &Stream) -> bool {
    use std::io::Read;
    if caller.set_nonblocking(true).is_err() {
        return false;
    }
    let mut reader = caller;
    let gone = match reader.read(&mut [0; 1]) {
        Ok(read) => read == 0,
        Err(e) => e.kind() != std::io::ErrorKind::WouldBlock,
    };
    let _ = caller.set_nonblocking(false);
    gone
}

/// A named pipe read with nothing to read looks like its end (`ERROR_NO_DATA` becomes EOF), so the pipe is asked.
#[cfg(windows)]
fn hung_up(caller: &Stream) -> bool {
    use std::os::windows::io::{AsHandle, AsRawHandle};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn PeekNamedPipe(
            pipe: *mut std::ffi::c_void,
            buffer: *mut std::ffi::c_void,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> i32;
    }
    let Stream::NamedPipe(pipe) = caller;
    let null = std::ptr::null_mut();
    // SAFETY: the handle lives as long as `caller`, and with no buffer the call only says whether the pipe is
    // still connected.
    unsafe {
        PeekNamedPipe(
            pipe.as_handle().as_raw_handle(),
            null,
            0,
            null.cast(),
            null.cast(),
            null.cast(),
        ) == 0
    }
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

fn call(app: &AppHandle, state: &AppState, call: Call, caller: &Stream) -> Result<Value> {
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
                Remote::default(),
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
        Call::Agent(call) => agents::call(app, call)?,
        Call::Team(call) => team::call(app, call)?,
        Call::Vault(call) => crate::vault::call(app, call)?,
        Call::Review(call) => visual::call(app, call)?,
        Call::Memory(call) => crate::memory::call(app, call)?,
        Call::Mcp { session, tool } => crate::cto::call(app, session, tool, &|| hung_up(caller))?,
        Call::Watch => unreachable!("handled by the connection loop"),
    })
}

#[tauri::command]
pub fn api_confirms(state: State<AppState>) -> Vec<Confirm> {
    state.api.confirms.list()
}

#[tauri::command]
pub fn api_confirm(state: State<AppState>, id: u64, allow: bool) {
    state.api.confirms.answer(id, allow);
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
    fn a_step_taken_over_keeps_its_project_whatever_its_folder() {
        let root = std::env::temp_dir().join(format!("termf-api-project-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = root.join("acme-api");
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::create_dir_all(root.join("config/projects")).unwrap();
        let file = format!(
            "name = \"Acme\"\n[[repos]]\npath = \"{}\"\n",
            repo.display()
        );
        fs::write(root.join("config/projects/acme.toml"), file).unwrap();
        let config = root.join("config");
        // A team step's worktree lives in TermF's data folder, which no project lists.
        let worktree = root.join("data/team/12/back");
        fs::create_dir_all(&worktree).unwrap();
        let project = |given, run, cwd: &Path| project_for(&config, given, run, cwd);
        assert_eq!(
            project(Some("acme"), None, &worktree).as_deref(),
            Some("acme")
        );
        assert_eq!(
            project(None, Some("acme"), &worktree).as_deref(),
            Some("acme")
        );
        assert_eq!(
            project(None, None, &repo.join("src")).as_deref(),
            Some("acme")
        );
        assert_eq!(project(None, None, &worktree), None);
        fs::remove_dir_all(&root).unwrap();
    }

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

    fn api(session_tokens: HashMap<u32, String>) -> Api {
        Api {
            endpoint: Some(Endpoint {
                data_dir: PathBuf::new(),
                socket: String::new(),
                token: "user".into(),
                session_tokens: Arc::new(Mutex::new(session_tokens)),
                mcp_tokens: Mutex::default(),
            }),
            error: None,
            meta: Mutex::default(),
            watchers: Mutex::default(),
            limits: Mutex::default(),
            confirm_cli: true,
            confirms: Confirms::default(),
        }
    }

    #[test]
    fn a_termf_mcp_token_lives_as_long_as_its_process() {
        let api = api(HashMap::new());
        let endpoint = api.endpoint.as_ref().unwrap();
        let tool = |token: &str| Request {
            token: token.into(),
            call: Call::Mcp {
                session: 7,
                tool: ipc::McpCall::TeamRuns {},
            },
        };
        let first = endpoint.issue_mcp(7).unwrap();
        assert!(endpoint.allows(&tool(&first)));
        let second = endpoint.issue_mcp(7).unwrap();
        assert_ne!(first, second);
        assert!(!endpoint.allows(&tool(&first)), "never used again");
        assert!(endpoint.allows(&tool(&second)));
        api.revoke_mcp(7);
        assert!(!endpoint.allows(&tool(&second)), "its process ended");
    }

    #[test]
    fn a_reserved_call_waits_for_the_user() {
        let confirms = Arc::new(Confirms::default());
        let deliver = || {
            ipc::reserved(&Call::Deliver {
                run: 12,
                confirm: String::new(),
            })
            .unwrap()
        };
        let shown: Arc<Mutex<Vec<(String, u64)>>> = Arc::default();
        let caller_gone = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ask = |wait| {
            let (confirms, shown, gone) = (confirms.clone(), shown.clone(), caller_gone.clone());
            thread::spawn(move || {
                confirms.ask(
                    deliver(),
                    wait,
                    |event, c| shown.lock().unwrap().push((event.into(), c.id)),
                    || gone.load(Ordering::SeqCst),
                )
            })
        };
        let waiting = |n| {
            while confirms.list().len() != n {
                thread::sleep(Duration::from_millis(10));
            }
            confirms.list()
        };

        let allowed = ask(Duration::from_secs(10));
        let pending = waiting(1);
        assert_eq!(pending[0].reserved.action, "deliver");
        assert_eq!(pending[0].reserved.args["run"], "12");
        confirms.answer(pending[0].id, true);
        assert!(allowed.join().unwrap().is_ok());

        let refused = ask(Duration::from_secs(10));
        let id = waiting(1)[0].id;
        confirms.answer(id, false);
        let error = refused.join().unwrap().unwrap_err().to_string();
        assert!(error.contains("refused in TermF"), "{error}");

        let ignored = ask(Duration::from_millis(50)).join().unwrap();
        assert!(ignored.unwrap_err().to_string().contains("not confirmed"));

        // Ctrl-C in the terminal: the banner goes, and allowing it afterwards does nothing.
        let abandoned = ask(Duration::from_secs(10));
        let id = waiting(1)[0].id;
        caller_gone.store(true, Ordering::SeqCst);
        let error = abandoned.join().unwrap().unwrap_err().to_string();
        assert!(error.contains("went away"), "{error}");
        confirms.answer(id, true);
        assert!(confirms.list().is_empty(), "none left waiting");
        let events: Vec<String> = shown
            .lock()
            .unwrap()
            .iter()
            .map(|(e, _)| e.clone())
            .collect();
        assert_eq!(events.len(), 8, "each shown, then gone: {events:?}");
        assert_eq!(events[6..], ["api-confirm", "api-confirm-done"]);
    }

    #[test]
    fn notices_a_caller_that_hung_up() {
        let name = if cfg!(windows) {
            format!(r"\\.\pipe\termf-hangup-{}", std::process::id())
        } else {
            std::env::temp_dir()
                .join(format!("termf-hangup-{}.sock", std::process::id()))
                .to_string_lossy()
                .into_owned()
        };
        let listener = ListenerOptions::new()
            .name(name.as_str().to_fs_name::<GenericFilePath>().unwrap())
            .try_overwrite(true)
            .create_sync()
            .unwrap();
        let client =
            Stream::connect(name.as_str().to_fs_name::<GenericFilePath>().unwrap()).unwrap();
        let server = listener.accept().unwrap();
        assert!(!hung_up(&server), "still waiting for its reply");
        drop(client);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !hung_up(&server) {
            assert!(Instant::now() < deadline, "the hang-up was never seen");
            thread::sleep(Duration::from_millis(20));
        }
        if !cfg!(windows) {
            let _ = fs::remove_file(&name);
        }
    }

    #[test]
    fn a_session_token_ends_with_its_process() {
        let api = api(HashMap::from([(3, "s3".to_string())]));
        let hook = Request {
            token: "s3".into(),
            call: Call::Hook {
                session: 3,
                event: "Stop".into(),
                payload: Value::Null,
            },
        };
        let endpoint = api.endpoint.as_ref().unwrap();
        assert!(endpoint.allows(&hook));
        api.revoke_session(3);
        assert!(
            !endpoint.allows(&hook),
            "what it left running reports nothing"
        );
        // Started again in place (`relaunch`): the new process gets a token of its own, the old one stays refused.
        endpoint
            .session_tokens
            .lock()
            .unwrap()
            .insert(3, "s3-new".into());
        assert!(!endpoint.allows(&hook), "the replaced process's SessionEnd");
        let new = Request {
            token: "s3-new".into(),
            ..hook
        };
        assert!(endpoint.allows(&new));
    }

    fn meta(remote_control: bool, remote: Option<Tile>) -> Meta {
        Meta {
            title: String::new(),
            status: None,
            since: Instant::now(),
            remote_control,
            agent_session_id: None,
            dir: None,
            reset: None,
            hidden: false,
            policy: None,
            remote,
            typed: None,
        }
    }

    #[test]
    fn cut_all_reaches_every_session_and_every_question() {
        let tile = |want: bool, asked: bool, ask: bool| Tile {
            want,
            asked,
            ask,
            ..Tile::default()
        };
        let mut all = HashMap::from([
            (1, meta(true, Some(tile(true, false, false)))),
            (2, meta(false, Some(tile(false, true, false)))),
            (3, meta(false, Some(tile(false, false, true)))),
            (4, meta(false, Some(tile(false, false, false)))),
            (5, meta(true, None)),
            (6, meta(false, None)),
        ]);
        let (mut tiles, bare, mut changed) = cut_all(&mut all);
        tiles.sort();
        changed.sort();
        assert_eq!(tiles, [1, 2], "on, or on its way");
        assert_eq!(bare, [5], "started with Remote Control, without hooks");
        assert_eq!(changed, [1, 3], "the switch off, the question gone");
        assert!(!all[&5].remote_control);
        assert!(
            all.values()
                .filter_map(|m| m.remote.as_ref())
                .all(|t| !t.want && !t.ask)
        );
    }
}
