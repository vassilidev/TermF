//! The headless sessions of this launch of the app: at most one process per conversation, the events it
//! reports, and handing the conversation to a terminal and back.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use termf_core::claude::Tool;
use termf_core::{Settings, agent_env, claude, cli_agents, ipc};
use termf_term::{shell_command, shell_quote};

use crate::store::{self, Store};
use crate::{
    AgentEvent, AgentRuntime, AgentSession, ClaudeCode, Codex, Gemini, Launch, Notice, Outcome,
    SessionSpec, State, StoredEvent, Update, Usage, now,
};

/// The `termf mcp` server of a session with TermF's tools, one per process: what `--mcp-config` takes, and what
/// voids its token once the process ends.
struct McpServer {
    server: Box<dyn Fn(i64) -> Result<Value> + Send + Sync>,
    revoke: Box<dyn Fn(i64) + Send + Sync>,
}

/// How long a process may take to end once its stdin is closed, then once killed.
const STOP_WAIT: Duration = Duration::from_secs(10);
const KILL_WAIT: Duration = Duration::from_secs(5);
/// How long a terminal may take to close when its session is handed back.
const TILE_WAIT: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 10 });
const CONSULT_POLL: Duration = Duration::from_millis(if cfg!(test) { 50 } else { 500 });
/// As the vault masks its values everywhere else.
const MASK: &str = "••••••";
const BUSY: &str = "the session is being stopped, taken over or handed back";
const NO_GUARD: &str = "this session needs TermF's guard, the hook that keeps it from pushing: the termf CLI next to TermF (`pnpm bundle`, or `cargo build -p termf-cli` in development)";

/// Clones share the same sessions.
#[derive(Clone)]
pub struct Agents {
    inner: Arc<Inner>,
}

struct Inner {
    config_dir: PathBuf,
    data_dir: PathBuf,
    live: Mutex<HashMap<i64, Live>>,
    /// A process or a terminal ended.
    ended: Condvar,
    sink: Box<dyn Fn(Update) + Send + Sync>,
    /// Added to every process's environment.
    env: Vec<(String, String)>,
    mcp: OnceLock<McpServer>,
    /// The `termf` CLI, for the sessions whose Bash commands go through its `guard`, and for `agent-env`.
    guard: OnceLock<PathBuf>,
    secrets: OnceLock<Secrets>,
    /// By session, the vault's values its process got: masked in what it reports.
    hidden: Mutex<HashMap<i64, Vec<String>>>,
}

/// What the vault gives a process of a session (its id, its project): `[agents.secrets]`.
type Secrets = Box<dyn Fn(i64, Option<&str>) -> Given + Send + Sync>;

/// The variables a session's process gets from the vault, and what to tell it when some are missing.
#[derive(Default)]
pub struct Given {
    pub vars: Vec<(String, String)>,
    pub note: Option<String>,
}

#[derive(Default)]
struct Live {
    process: Option<Process>,
    tile: Option<Tile>,
    /// TermF's notices the agent has not read yet, and whether one should start a turn of its own.
    notices: Vec<String>,
    wake: bool,
    /// The user's messages written during a turn: they go, as one, once it ends.
    queued: Vec<String>,
    /// The library's instructions, for the first message of a runtime that reads them there.
    instructions: Option<String>,
    /// A process that runs one turn: the state its end of turn sets, once it has exited (a message sent sooner
    /// would find it still there).
    after_exit: Option<State>,
    /// Permission requests waiting for an answer, with the tool input each asks about.
    pending: HashMap<String, Value>,
    interrupting: bool,
    /// A stop, a takeover until its terminal is attached, or a hand-back: nothing may start a process meanwhile.
    busy: bool,
    generation: u64,
}

struct Process {
    /// Closing it ends Claude Code once its turn is over.
    stdin: Option<ChildStdin>,
    pid: u32,
    generation: u64,
}

struct Tile {
    id: u32,
    running: bool,
}

/// What reopens a conversation in a terminal: the `[agents]` entry, and the options after its command.
#[derive(Clone, Debug, PartialEq)]
pub struct Takeover {
    pub agent: String,
    pub cwd: PathBuf,
    pub title: String,
    pub args: Vec<String>,
    /// Its terminal must have TermF's guard too.
    pub guard: bool,
}

impl Agents {
    /// `sink` hears every event and every change of a session. No process outlived the last launch: the
    /// sessions it left running are stopped.
    pub fn new(
        config_dir: PathBuf,
        data_dir: PathBuf,
        sink: impl Fn(Update) + Send + Sync + 'static,
    ) -> Self {
        Self::build(config_dir, data_dir, Vec::new(), sink)
    }

    /// `env` is added to every process's environment.
    pub fn build(
        config_dir: PathBuf,
        data_dir: PathBuf,
        env: Vec<(String, String)>,
        sink: impl Fn(Update) + Send + Sync + 'static,
    ) -> Self {
        let agents = Self {
            inner: Arc::new(Inner {
                config_dir,
                data_dir,
                live: Mutex::default(),
                ended: Condvar::new(),
                sink: Box::new(sink),
                env,
                mcp: OnceLock::new(),
                guard: OnceLock::new(),
                secrets: OnceLock::new(),
                hidden: Mutex::default(),
            }),
        };
        if let Err(e) = agents.inner.store().and_then(|s| agents.inner.reap(&s)) {
            eprintln!("termf: agent sessions left as they were: {e:#}");
        }
        agents
    }

    /// Kills every headless process: none may outlive the app, the only one that can talk to it.
    pub fn shutdown(&self) {
        let live = self.inner.live.lock().unwrap();
        for process in live.values().filter_map(|e| e.process.as_ref()) {
            kill_group(process.pid);
        }
    }

    /// What gives each process of a session with TermF's tools its `termf mcp` server (`command`, `args`, `env`,
    /// a token of its own), and what voids that token once the process ends or cannot start. Set once.
    pub fn serve_mcp(
        &self,
        server: impl Fn(i64) -> Result<Value> + Send + Sync + 'static,
        revoke: impl Fn(i64) + Send + Sync + 'static,
    ) {
        let _ = self.inner.mcp.set(McpServer {
            server: Box::new(server),
            revoke: Box::new(revoke),
        });
    }

    /// The `termf` CLI whose `guard` the sessions that ask for it go through. Set once; without it, they never
    /// start.
    pub fn serve_guard(&self, termf: PathBuf) {
        let _ = self.inner.guard.set(termf);
    }

    /// What gives each process of a session the vault's values for its project (`[agents.secrets]`). Set once;
    /// without it, none. A CTO or expert conversation never gets them.
    pub fn serve_secrets(
        &self,
        given: impl Fn(i64, Option<&str>) -> Given + Send + Sync + 'static,
    ) {
        let _ = self.inner.secrets.set(Box::new(given));
    }

    /// A new session with no process yet: its first message starts one.
    pub fn create(&self, spec: SessionSpec) -> Result<AgentSession> {
        let inner = &self.inner;
        let mut spec = checked(spec)?;
        ensure!(!spec.guard || inner.guard.get().is_some(), NO_GUARD);
        let settings = termf_core::load_settings(&inner.config_dir)?;
        let role = match spec.role.clone() {
            Some(name) => Some((role_of(&inner.config_dir, &spec, &name)?, name)),
            None => None,
        };
        // TermF's own conversations (CTO, expert) stay on Claude Code, whatever their role says.
        let wanted = spec.agent.clone().or_else(|| {
            let (settings, _) = role.as_ref().filter(|_| !spec.mcp)?;
            let agent = settings.1.iter().find(|s| s.key == "agent")?;
            agent.value.as_str().map(str::to_owned)
        });
        let (agent, tool) = agent_named(&settings, wanted.as_deref())?;
        spec.agent = Some(agent);
        if let Some((settings, name)) = &role {
            play(&mut spec, settings, name, tool == Tool::Claude)?;
        }
        let runtime = runtime_of(tool);
        runtime.refuse(&spec)?;
        let session = inner.store()?.create(runtime.id(), &uuid()?, &spec)?;
        (inner.sink)(Update::Session(session.clone()));
        Ok(session)
    }

    /// A new session, its process started, with `prompt` as its first message when there is one.
    pub fn start(&self, spec: SessionSpec, prompt: Option<&str>) -> Result<AgentSession> {
        let inner = &self.inner;
        let _ = inner.command(&spec);
        let id = self.create(spec)?.id;
        let store = inner.store()?;
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        match prompt.filter(|p| !p.trim().is_empty()) {
            Some(text) => inner.deliver(&store, id, entry, Some(text))?,
            // A runtime that runs a process per turn starts with its first message.
            None if inner.runtime(&store, id)?.per_turn() => {}
            None => inner.launch(&store, id, entry)?,
        }
        inner.view(&store, id, entry)
    }

    /// Sends a message between two turns. A session without a process resumes its conversation first.
    pub fn send(&self, id: i64, text: &str) -> Result<()> {
        ensure!(!text.trim().is_empty(), "the message is empty");
        let inner = &self.inner;
        let store = inner.store()?;
        let _ = inner.command(&store.session(id)?.spec);
        let mut live = inner.live.lock().unwrap();
        inner.deliver(&store, id, live.entry(id).or_default(), Some(text))
    }

    /// Sends a message now between two turns; during one, keeps it for its end (`true`), when it goes as the
    /// user's message, so that nothing written to a busy agent is lost.
    pub fn send_or_queue(&self, id: i64, text: &str) -> Result<bool> {
        ensure!(!text.trim().is_empty(), "the message is empty");
        let inner = &self.inner;
        let store = inner.store()?;
        let _ = inner.command(&store.session(id)?.spec);
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        if matches!(store.session(id)?.state, State::Working | State::Waiting) {
            entry.queued.push(text.to_owned());
            return Ok(true);
        }
        inner.deliver(&store, id, entry, Some(text))?;
        Ok(false)
    }

    /// Shows a notice from TermF in the conversation; the agent reads it with the next message. `wake`: in a
    /// turn of its own, at once when the agent's process waits between two turns, else as soon as its turn
    /// ends; a session without a process (stopped by the user, or since the app started) is never woken.
    pub fn notify(&self, id: i64, notice: Notice, wake: bool) -> Result<()> {
        let inner = &self.inner;
        let store = inner.store()?;
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        entry.notices.push(notice.text.clone());
        entry.wake |= wake;
        inner.record(&store, id, AgentEvent::Notice(notice));
        if entry.wake && !entry.busy && store.session(id)?.state == State::Idle {
            inner.deliver(&store, id, entry, None)?;
        }
        Ok(())
    }

    /// Stops the turn under way; the conversation goes on with the next message.
    pub fn interrupt(&self, id: i64) -> Result<()> {
        let inner = &self.inner;
        let store = inner.store()?;
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        let state = store.session(id)?.state;
        ensure!(
            matches!(state, State::Working | State::Waiting),
            "the agent is not working"
        );
        inner.interrupt(&store, id, entry)?;
        entry.interrupting = true;
        Ok(())
    }

    /// Answers the permission request `request`; `message` tells the agent why it is refused.
    pub fn answer(&self, id: i64, request: &str, allow: bool, message: Option<&str>) -> Result<()> {
        let inner = &self.inner;
        let store = inner.store()?;
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        let input =
            entry.pending.get(request).cloned().with_context(|| {
                format!("no permission request {request} waits in this session")
            })?;
        let message = message.map(str::trim).filter(|m| !m.is_empty());
        let why = message.unwrap_or("The user refused.");
        let answer = inner
            .runtime(&store, id)?
            .answer(request, allow, &input, why);
        inner.write(entry, &answer)?;
        entry.pending.remove(request);
        inner.record(
            &store,
            id,
            AgentEvent::PermissionAnswer {
                request: request.into(),
                allow,
                message: message.map(str::to_owned),
            },
        );
        if entry.pending.is_empty() && store.session(id)?.state == State::Waiting {
            inner.set_state(&store, id, entry, State::Working)?;
        }
        Ok(())
    }

    /// Ends the session's process, after interrupting its turn if one is under way.
    pub fn stop(&self, id: i64) -> Result<AgentSession> {
        let inner = &self.inner;
        let store = inner.store()?;
        let pid = {
            let mut live = inner.live.lock().unwrap();
            let entry = live.entry(id).or_default();
            let session = store.session(id)?;
            ensure!(!entry.busy, BUSY);
            ensure!(
                session.state != State::Interactive,
                "the session is taken over in a terminal: hand it back first"
            );
            if matches!(session.state, State::Working | State::Waiting) {
                let _ = inner.interrupt(&store, id, entry);
            }
            if matches!(
                session.state,
                State::Working | State::Waiting | State::Idle | State::Error
            ) {
                inner.set_state(&store, id, entry, State::Stopped)?;
            }
            let Some(process) = entry.process.as_mut() else {
                return inner.view(&store, id, entry);
            };
            process.stdin = None;
            entry.busy = true;
            process.pid
        };
        let ended = inner.wait_end(id, pid);
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        entry.busy = false;
        if ended.is_err() {
            // Its pid stays saved: no new process starts while it runs.
            entry.process = None;
            inner.set_state(&store, id, entry, State::Error)?;
        }
        ended?;
        inner.view(&store, id, entry)
    }

    /// Ends the headless process between two turns, so the conversation can go on in a terminal opened with
    /// what this returns; then say which one with [`Agents::attach_tile`].
    pub fn takeover(&self, id: i64) -> Result<Takeover> {
        let inner = &self.inner;
        let store = inner.store()?;
        let (session, pid) = {
            let mut live = inner.live.lock().unwrap();
            let entry = live.entry(id).or_default();
            let mut session = store.session(id)?;
            ensure!(!entry.busy, BUSY);
            ensure!(
                !session.spec.mcp,
                "this conversation stays in TermF: its tools would not follow it into a terminal"
            );
            match session.state {
                State::Working | State::Waiting => {
                    bail!("the agent is working: wait for the end of its turn, or interrupt it")
                }
                State::Interactive => bail!("the session is already taken over"),
                // A terminal has no budget to keep to.
                State::Budget => bail!("the session's budget is spent"),
                _ => {}
            }
            ensure!(
                store.started(id)?,
                "the agent has no conversation yet: send it a message first"
            );
            let pid = entry.process.as_mut().map(|p| {
                p.stdin = None;
                p.pid
            });
            session.state = State::Interactive;
            inner.save(&store, &mut session, entry)?;
            // Until `attach_tile`: a hand-back now would start a process next to the terminal's.
            entry.busy = true;
            (session, pid)
        };
        if let Some(pid) = pid
            && let Err(e) = inner.wait_end(id, pid)
        {
            let mut live = inner.live.lock().unwrap();
            let entry = live.entry(id).or_default();
            entry.busy = false;
            entry.process = None;
            inner.set_state(&store, id, entry, State::Error)?;
            return Err(e);
        }
        let spec = &session.spec;
        Ok(Takeover {
            agent: spec.agent.clone().unwrap_or_default(),
            cwd: spec.cwd.clone(),
            title: spec.title.clone(),
            args: runtime(&session.runtime).interactive_args(spec, &session.session_id),
            guard: spec.guard,
        })
    }

    /// The terminal the conversation went on in, or `None` when it could not open: the session is stopped.
    pub fn attach_tile(&self, id: i64, tile: Option<u32>) -> Result<()> {
        let inner = &self.inner;
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        entry.busy = false;
        entry.tile = tile.map(|id| Tile { id, running: true });
        let store = inner.store()?;
        match tile {
            Some(tile) => {
                inner.record(&store, id, AgentEvent::TakenOver { tile });
                inner
                    .view(&store, id, entry)
                    .map(|s| (inner.sink)(Update::Session(s)))
            }
            None => inner.set_state(&store, id, entry, State::Stopped).map(drop),
        }
    }

    /// A terminal session ended.
    pub fn tile_exited(&self, tile: u32) {
        let inner = &self.inner;
        let mut live = inner.live.lock().unwrap();
        for (id, entry) in live.iter_mut() {
            if let Some(t) = entry.tile.as_mut().filter(|t| t.id == tile && t.running) {
                t.running = false;
                if let Ok(session) = inner.store().and_then(|s| inner.view(&s, *id, entry)) {
                    (inner.sink)(Update::Session(session));
                }
            }
        }
        inner.ended.notify_all();
    }

    /// Closes the session's terminal with `close`, waits for it to end, then resumes the conversation headless.
    pub fn handback(&self, id: i64, close: impl FnOnce(u32)) -> Result<AgentSession> {
        let inner = &self.inner;
        let store = inner.store()?;
        let tile = {
            let mut live = inner.live.lock().unwrap();
            let entry = live.entry(id).or_default();
            ensure!(!entry.busy, BUSY);
            ensure!(
                store.session(id)?.state == State::Interactive,
                "the session is not taken over"
            );
            entry.busy = true;
            entry.tile.as_ref().filter(|t| t.running).map(|t| t.id)
        };
        if let Some(tile) = tile {
            close(tile);
            let live = inner.live.lock().unwrap();
            let open = |live: &mut HashMap<i64, Live>| {
                live.get(&id)
                    .and_then(|e| e.tile.as_ref())
                    .is_some_and(|t| t.running)
            };
            let (mut live, wait) = inner
                .ended
                .wait_timeout_while(live, TILE_WAIT, open)
                .unwrap();
            if wait.timed_out() {
                live.entry(id).or_default().busy = false;
                bail!("terminal {tile} did not close: close it, then hand the session back");
            }
        }
        let _ = inner.command(&store.session(id)?.spec);
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        entry.busy = false;
        entry.tile = None;
        inner.record(&store, id, AgentEvent::HandedBack);
        inner.set_state(&store, id, entry, State::Stopped)?;
        if !inner.runtime(&store, id)?.per_turn() {
            inner.launch(&store, id, entry)?;
        }
        inner.view(&store, id, entry)
    }

    pub fn session(&self, id: i64) -> Result<AgentSession> {
        let inner = &self.inner;
        let live = inner.live.lock().unwrap();
        let session = inner.store()?.session(id)?;
        Ok(AgentSession {
            tile: live.get(&id).and_then(tile),
            ..session
        })
    }

    /// Newest first.
    pub fn sessions(&self) -> Result<Vec<AgentSession>> {
        let mut all = store::sessions(&self.inner.data_dir)?;
        let live = self.inner.live.lock().unwrap();
        for session in &mut all {
            session.tile = live.get(&session.id).and_then(tile);
        }
        Ok(all)
    }

    pub fn events(&self, id: i64, after: i64) -> Result<Vec<StoredEvent>> {
        store::events(&self.inner.data_dir, id, after)
    }

    /// Starts a session on `prompt` and waits for its answer, what TermF asks an agent for another (`ask_expert`).
    /// Nobody is there to approve a tool, so each permission it asks for is refused; past `timeout` it is stopped.
    /// Its process ends with the answer; the conversation stays, and a message resumes it.
    pub fn consult(
        &self,
        spec: SessionSpec,
        prompt: &str,
        timeout: Duration,
    ) -> Result<(AgentSession, String)> {
        let id = self.start(spec, Some(prompt))?.id;
        let deadline = std::time::Instant::now() + timeout;
        let session = loop {
            let session = self.session(id)?;
            match session.state {
                State::Working => {}
                State::Waiting => {
                    let pending: Vec<String> = {
                        let live = self.inner.live.lock().unwrap();
                        live.get(&id)
                            .map(|e| e.pending.keys().cloned().collect())
                            .unwrap_or_default()
                    };
                    for request in pending {
                        let why = "Nobody can approve this here: answer with what you can read.";
                        let _ = self.answer(id, &request, false, Some(why));
                    }
                }
                _ => break session,
            }
            if std::time::Instant::now() > deadline {
                let _ = self.stop(id);
                bail!(
                    "no answer within {} minutes (conversation {id})",
                    timeout.as_secs() / 60
                );
            }
            std::thread::sleep(CONSULT_POLL);
        };
        if session.state == State::Idle {
            self.stop(id)?;
        }
        let events = self.events(id, 0)?;
        // The result line holds the answer; its last text says the same when that line has none.
        let said = events.iter().rev().find_map(|e| match &e.event {
            AgentEvent::Text { text, parent: None } => Some(text.clone()),
            _ => None,
        });
        let end = events.into_iter().rev().find_map(|e| match e.event {
            AgentEvent::TurnEnd {
                outcome,
                result,
                error,
                ..
            } => Some((outcome, result.or(said.clone()), error)),
            _ => None,
        });
        match end {
            Some((Outcome::Done, Some(text), _)) if !text.trim().is_empty() => {
                Ok((self.session(id)?, text))
            }
            Some((Outcome::Budget, ..)) => bail!(
                "its budget ran out (${:.2}) before it answered (conversation {id})",
                session.cost
            ),
            Some((_, _, Some(error))) => bail!("no answer: {error} (conversation {id})"),
            _ => bail!("no answer (conversation {id})"),
        }
    }
}

fn tile(entry: &Live) -> Option<u32> {
    entry.tile.as_ref().filter(|t| t.running).map(|t| t.id)
}

impl Inner {
    fn store(&self) -> Result<Store> {
        Store::open(&self.data_dir)
    }

    fn view(&self, store: &Store, id: i64, entry: &Live) -> Result<AgentSession> {
        let mut session = store.session(id)?;
        session.tile = tile(entry);
        Ok(session)
    }

    /// Saves the session and tells the UI.
    fn save(&self, store: &Store, session: &mut AgentSession, entry: &Live) -> Result<()> {
        store.update(session)?;
        session.tile = tile(entry);
        (self.sink)(Update::Session(session.clone()));
        Ok(())
    }

    fn set_state(&self, store: &Store, id: i64, entry: &Live, state: State) -> Result<()> {
        let mut session = store.session(id)?;
        session.state = state;
        self.save(store, &mut session, entry)
    }

    /// An event that cannot be saved is lost, never the change of state that comes with it.
    fn hide(&self, id: i64, event: AgentEvent) -> AgentEvent {
        match self.hidden.lock().unwrap().get(&id) {
            Some(values) if !values.is_empty() => hide(event, values),
            _ => event,
        }
    }

    fn record(&self, store: &Store, id: i64, event: AgentEvent) {
        let event = self.hide(id, event);
        match store.record(id, &event) {
            Ok(stored) => (self.sink)(Update::Event(stored)),
            Err(e) => eprintln!("termf: agent session {id}: event not saved: {e:#}"),
        }
    }

    /// Kills what the last launch of the app left running, and frees the sessions whose process is gone.
    fn reap(&self, store: &Store) -> Result<()> {
        store.stop_all()?;
        for (id, pid, session_id) in store.pids()? {
            let dir = store.dir(id).join("session");
            if ours(pid, &session_id, &dir) {
                kill_group(pid);
                let deadline = std::time::Instant::now() + KILL_WAIT;
                while ours(pid, &session_id, &dir) && std::time::Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            if ours(pid, &session_id, &dir) {
                let message = format!("its process {pid} from the last launch still runs");
                self.record(store, id, AgentEvent::Error { message });
                self.set_state(store, id, &Live::default(), State::Error)?;
            } else {
                store.clear_pid(id, pid)?;
            }
        }
        Ok(())
    }

    fn runtime(&self, store: &Store, id: i64) -> Result<&'static dyn AgentRuntime> {
        Ok(runtime(&store.session(id)?.runtime))
    }

    /// Stops the turn under way: a line on stdin, or an interrupt signal to a process that runs one turn.
    fn interrupt(&self, store: &Store, id: i64, entry: &mut Live) -> Result<()> {
        if let Some(line) = self.runtime(store, id)?.interrupt() {
            return self.write(entry, &line);
        }
        let process = entry
            .process
            .as_ref()
            .context("the agent's process is not running")?;
        interrupt_group(process.pid);
        Ok(())
    }

    fn write(&self, entry: &mut Live, line: &str) -> Result<()> {
        let stdin = entry
            .process
            .as_mut()
            .and_then(|p| p.stdin.as_mut())
            .context("the agent's process is not running")?;
        writeln!(stdin, "{line}")
            .and_then(|()| stdin.flush())
            .context("cannot write to the agent")
    }

    /// Starts a turn with the user's message, or with TermF's notices alone (`text: None`).
    fn deliver(
        self: &Arc<Self>,
        store: &Store,
        id: i64,
        entry: &mut Live,
        text: Option<&str>,
    ) -> Result<()> {
        ensure!(!entry.busy, BUSY);
        match store.session(id)?.state {
            State::Working | State::Waiting => {
                bail!("the agent is working: wait for the end of its turn, or interrupt it")
            }
            State::Interactive => {
                bail!("the session is taken over in a terminal: hand it back first")
            }
            State::Budget => bail!("the session's budget is spent"),
            _ => {}
        }
        if entry.process.is_none() {
            self.launch(store, id, entry)?;
        }
        let runtime = self.runtime(store, id)?;
        let mut said = entry.queued.clone();
        said.extend(text.map(str::to_owned));
        let text = (!said.is_empty()).then(|| said.join("\n\n"));
        let mut line = message(&entry.notices, text.as_deref());
        if let Some(instructions) = entry.instructions.take() {
            line = format!(
                "<termf-instructions>\n{}\n</termf-instructions>\nTermF's instructions for this whole conversation, from the project's library.\n\n{line}",
                instructions.trim()
            );
        }
        self.write(entry, &runtime.message(&line))?;
        if runtime.per_turn()
            && let Some(process) = entry.process.as_mut()
        {
            // Its message is all it reads: closing stdin lets it start.
            process.stdin = None;
        }
        entry.notices.clear();
        entry.queued.clear();
        entry.wake = false;
        entry.interrupting = false;
        if let Some(text) = text {
            self.record(store, id, AgentEvent::User { text });
        }
        self.set_state(store, id, entry, State::Working)
    }

    /// The agent's command, a bare `claude`, `codex` or `gemini` replaced by the install it runs. Asking an install
    /// its version can take seconds: callers that may start a process ask once before taking `live`, then it is
    /// cached.
    fn command(&self, spec: &SessionSpec) -> Result<(String, claude::Install)> {
        let settings = termf_core::load_settings(&self.config_dir)?;
        let agent = spec.agent.as_deref().unwrap_or_default();
        let command = settings
            .agent(agent)
            .with_context(|| format!("no agent {agent} in termf.toml [agents]"))?;
        claude::in_command(&command, shell_quote)
    }

    /// Starts the session's process, which waits for a message. The library is written again, so that what
    /// changed since applies.
    fn launch(self: &Arc<Self>, store: &Store, id: i64, entry: &mut Live) -> Result<()> {
        let session = store.session(id)?;
        let budget = session.spec.budget.map(|b| b - session.cost);
        if budget.is_some_and(|left| left < 0.0001) {
            self.set_state(store, id, entry, State::Budget)?;
            bail!("the session's budget is spent");
        }
        if let Some(pid) = store.pid(id)?
            && ours(pid, &session.session_id, &store.dir(id).join("session"))
        {
            bail!("a process of this session still runs (pid {pid}): it must end first");
        }
        let (command, claude) = self.command(&session.spec)?;
        let runtime = runtime(&session.runtime);
        let dir = store.dir(id).join("session");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        let system_prompt = library(&self.config_dir, &session.spec, &dir);
        let plugin = dir.join("plugin");
        let resume = store.started(id)?;
        let gemini = runtime.id() == Gemini.id();
        let policy = match gemini {
            true => Some(cli_agents::write_gemini_policy(&dir)?),
            false => None,
        };
        let guard = match session.spec.guard {
            true => Some(self.guard.get().context(NO_GUARD)?.as_path()),
            false => None,
        };
        let dirs = match session.spec.mcp {
            true => other_repos(&self.config_dir, &session.spec)?,
            false => Vec::new(),
        };
        let tools = crate::mcp_tools(&session.spec);
        let mcp = if tools.is_empty() {
            None
        } else if session.spec.mcp {
            Some(self.mcp_config(id, &dir, &tools)?)
        } else {
            // A role that may ask the expert still works without it, e.g. with no app to serve the tool.
            self.mcp_config(id, &dir, &tools).ok()
        };
        let args = runtime.headless_args(
            &session.spec,
            &Launch {
                session_id: &session.session_id,
                resume,
                budget,
                plugin: plugin.is_dir().then_some(plugin.as_path()),
                system_prompt: system_prompt.as_deref(),
                mcp: mcp.as_ref().map(|(file, ..)| file.as_path()),
                mcp_server: mcp.as_ref().map(|(_, server, _)| server),
                dirs: &dirs,
                guard,
                policy: policy.as_deref(),
                dir: &dir,
            },
        );
        let args: Vec<String> = args.iter().map(|a| shell_quote(a)).collect();
        let mut line = format!("{command} {}", args.join(" "));
        let tool = Tool::of(&command);
        let project = session.spec.project.clone().or_else(|| {
            termf_core::project_of(&self.config_dir, &session.spec.cwd)
                .ok()
                .flatten()
        });
        let policy = agent_env::load(&self.config_dir, project.as_deref());
        let given = match (self.secrets.get(), session.spec.mcp) {
            (Some(secrets), false) => secrets(id, project.as_deref()),
            _ => Given::default(),
        };
        let mut set: Vec<(String, String)> = self.env.clone();
        set.extend(
            mcp.as_ref()
                .and_then(|(.., token)| token.clone())
                .map(|t| (ipc::ENV_MCP_TOKEN.into(), t)),
        );
        if gemini && let Some((_, server, _)) = &mcp {
            let settings = cli_agents::gemini_settings(&dir, None, None, Some(server))?;
            set.push((
                cli_agents::GEMINI_SETTINGS_ENV.into(),
                settings.to_string_lossy().into_owned(),
            ));
        }
        set.extend(given.vars.iter().cloned());
        let names: Vec<String> = set.iter().map(|(k, _)| k.clone()).collect();
        let wrapped = policy.wrap(
            &line,
            tool,
            self.guard.get().map(PathBuf::as_path),
            &names,
            shell_quote,
        );
        if let Some((wrapped, _)) = &wrapped {
            line = wrapped.clone();
        }
        // Through the login shell, as a tile: `~`, quotes and the shell's PATH work the same.
        let (program, shell_args) = shell_command(Some(&line));
        let mut cmd = Command::new(program);
        cmd.args(shell_args)
            .current_dir(&session.spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Hooks would report to the terminal TermF itself may run in.
        for var in [ipc::ENV_SOCKET, ipc::ENV_TOKEN, ipc::ENV_SESSION] {
            cmd.env_remove(var);
        }
        cmd.envs(set);
        if let Some((_, spec)) = wrapped {
            cmd.env(agent_env::SPEC, spec);
        }
        policy.apply(&mut cmd, tool);
        if let Some(note) = given.note {
            self.record(store, id, AgentEvent::Error { message: note });
        }
        let hidden = given.vars.into_iter().map(|(_, v)| v);
        let hidden = hidden.filter(|v| v.chars().count() >= 4).collect();
        self.hidden.lock().unwrap().insert(id, hidden);
        // Its own process group, so stopping it also stops what it started.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
        let started = cmd
            .spawn()
            .context("cannot start the agent")
            .and_then(|mut child| {
                match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
                    (Some(stdin), Some(stdout), Some(stderr)) => Ok((child, stdin, stdout, stderr)),
                    _ => bail!("the agent's process has no pipes"),
                }
            });
        let (child, stdin, stdout, stderr) = started.inspect_err(|_| self.end_mcp(id, &dir))?;
        let pid = child.id();
        entry.generation += 1;
        let generation = entry.generation;
        entry.process = Some(Process {
            stdin: Some(stdin),
            pid,
            generation,
        });
        entry.pending.clear();
        entry.interrupting = false;
        entry.instructions = system_prompt
            .filter(|_| runtime.instructions_in_message() && !resume)
            .and_then(|file| fs::read_to_string(file).ok())
            .filter(|text| !text.trim().is_empty());
        let label = runtime.label();
        let said = std::thread::spawn(move || match said(stderr) {
            said if said.is_empty() => said,
            said => format!("{label} {claude}: {said}"),
        });
        let inner = self.clone();
        std::thread::spawn(move || inner.read(id, generation, child, stdout, said, runtime));
        store.set_pid(id, Some(pid))?;
        self.record(store, id, AgentEvent::Started { resumed: resume });
        self.set_state(store, id, entry, State::Idle)
    }

    fn read(
        self: Arc<Self>,
        id: i64,
        generation: u64,
        mut child: Child,
        stdout: ChildStdout,
        said: JoinHandle<String>,
        runtime: &'static dyn AgentRuntime,
    ) {
        let pid = child.id();
        let mut parser = runtime.parser();
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        // Opened again only after a failure.
        let mut store = None;
        while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            for event in parser.parse(&line) {
                let handled = self
                    .opened(&mut store)
                    .and_then(|s| self.handle(s, id, generation, event, runtime));
                if let Err(e) = handled {
                    eprintln!("termf: agent session {id}: {e:#}");
                }
            }
            line.clear();
        }
        let code = child.wait().ok().and_then(|s| s.code());
        let said = said.join().unwrap_or_default();
        let exited = self.exited(&mut store, id, generation, pid, code, &said, runtime);
        if let Err(e) = exited {
            eprintln!("termf: agent session {id}: {e:#}");
        }
    }

    fn opened<'a>(&self, store: &'a mut Option<Store>) -> Result<&'a Store> {
        if store.is_none() {
            *store = Some(self.store()?);
        }
        Ok(store.as_ref().expect("just opened"))
    }

    fn handle(
        self: &Arc<Self>,
        store: &Store,
        id: i64,
        generation: u64,
        mut event: AgentEvent,
        runtime: &dyn AgentRuntime,
    ) -> Result<()> {
        let mut live = self.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        // A process being stopped still reports, e.g. the end of its interrupted turn: the state is already set.
        let current = entry
            .process
            .as_ref()
            .is_some_and(|p| p.generation == generation);
        if let AgentEvent::Delta { .. } = event {
            if current {
                let event = self.hide(id, event);
                let (session, at) = (id, now());
                (self.sink)(Update::Event(StoredEvent {
                    id: 0,
                    session,
                    at,
                    event,
                }));
            }
            return Ok(());
        }
        let mut session = store.session(id)?;
        let before = session.clone();
        let moves = current
            && matches!(
                session.state,
                State::Working | State::Waiting | State::Idle | State::Error
            );
        let mut state = session.state;
        match &mut event {
            AgentEvent::Init { session_id, .. }
                if !session_id.is_empty() && *session_id != session.session_id =>
            {
                session.session_id = session_id.clone();
            }
            AgentEvent::Permission { request, input, .. } => {
                entry.pending.insert(request.clone(), input.clone());
                state = State::Waiting;
            }
            AgentEvent::TurnEnd {
                outcome,
                cost,
                total,
                context,
                ..
            } => {
                if entry.interrupting && *outcome == Outcome::Error {
                    *outcome = Outcome::Interrupted;
                }
                entry.interrupting = false;
                entry.pending.clear();
                session.cost += *cost;
                *total = session.cost;
                if context.is_some() {
                    session.context = context.clone();
                }
                let spent = session.spec.budget.is_some_and(|b| session.cost >= b);
                state = match outcome {
                    Outcome::Budget => State::Budget,
                    _ if spent => State::Budget,
                    Outcome::Error => State::Error,
                    _ => State::Idle,
                };
                if runtime.per_turn() && current {
                    entry.after_exit = Some(state);
                    state = session.state;
                }
            }
            _ => {}
        }
        if moves || (state == State::Budget && session.state != State::Interactive) {
            session.state = state;
        }
        let changed = session != before;
        // The session first: whoever sees the end of a turn also sees what it cost.
        let updated = match changed {
            true => store.update(&mut session),
            false => Ok(()),
        };
        self.record(store, id, event);
        updated?;
        if changed {
            session.tile = tile(entry);
            (self.sink)(Update::Session(session.clone()));
        }
        let due = entry.wake || !entry.queued.is_empty();
        if current && due && !entry.busy && session.state == State::Idle {
            self.deliver(store, id, entry, None)?;
        }
        Ok(())
    }

    /// Writes the `--mcp-config` of the session's `termf mcp`, and returns it with its server and the token its
    /// process gets in its environment: never in the file, which another session of the user could read. Claude
    /// Code gives a stdio server its own environment and expands `${…}` in its config (2.1.330, `--mcp-config`
    /// included), as Gemini CLI does in its settings; Codex CLI is given the variable's name (`env_vars`).
    fn mcp_config(
        &self,
        id: i64,
        dir: &Path,
        tools: &[&str],
    ) -> Result<(PathBuf, Value, Option<String>)> {
        let mcp = self
            .mcp
            .get()
            .context("TermF's tools cannot be served here: the conversation needs the app")?;
        let mut server = (mcp.server)(id)?;
        server["env"][ipc::ENV_MCP_TOOLS] = tools.join(",").into();
        if tools.iter().any(|t| t.starts_with("browser_")) {
            // Its browser reads the project's `[browser]` and reserves the worktree's port where TermF does.
            server["env"][ipc::ENV_CONFIG_DIR] = self.config_dir.to_string_lossy().into();
            server["env"][ipc::ENV_DATA_DIR] = self.data_dir.to_string_lossy().into();
        }
        let env = server["env"].as_object_mut().expect("set just above");
        let token = env
            .remove(ipc::ENV_MCP_TOKEN)
            .and_then(|t| t.as_str().map(str::to_owned));
        if token.is_some() {
            env.insert(
                ipc::ENV_MCP_TOKEN.into(),
                format!("${{{}}}", ipc::ENV_MCP_TOKEN).into(),
            );
        }
        let file = dir.join("mcp.json");
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let config = json!({ "mcpServers": { "termf": server } });
        options
            .open(&file)
            .and_then(|mut f| f.write_all(config.to_string().as_bytes()))
            .with_context(|| format!("cannot write {}", file.display()))
            .inspect_err(|_| self.end_mcp(id, dir))?;
        Ok((file, server, token))
    }

    /// Voids the token of the session's `termf mcp` and removes its config: no process of it runs any more.
    fn end_mcp(&self, id: i64, dir: &Path) {
        if let Some(mcp) = self.mcp.get() {
            (mcp.revoke)(id);
        }
        let _ = fs::remove_file(dir.join("mcp.json"));
    }

    #[allow(clippy::too_many_arguments)]
    fn exited(
        self: &Arc<Self>,
        store: &mut Option<Store>,
        id: i64,
        generation: u64,
        pid: u32,
        code: Option<i32>,
        said: &str,
        runtime: &dyn AgentRuntime,
    ) -> Result<()> {
        let mut live = self.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        let current = entry
            .process
            .as_ref()
            .is_some_and(|p| p.generation == generation);
        let interrupted = current && entry.interrupting;
        let ended = current.then(|| entry.after_exit.take()).flatten();
        if current {
            entry.process = None;
            entry.pending.clear();
            entry.interrupting = false;
        }
        self.ended.notify_all();
        let store = self.opened(store)?;
        // A newer process has a token of its own; none outlives the last one.
        if entry.process.is_none() {
            self.end_mcp(id, &store.dir(id).join("session"));
        }
        store.clear_pid(id, pid)?;
        self.record(store, id, AgentEvent::Exited { code });
        if !current {
            return Ok(());
        }
        let mut session = store.session(id)?;
        let working = matches!(session.state, State::Working | State::Waiting);
        let failed = working || (session.state == State::Idle && code != Some(0));
        if let Some(state) = ended.filter(|_| working) {
            session.state = state;
        } else if working && interrupted && runtime.per_turn() {
            // An interrupt signal ends the process without an end of turn: the conversation goes on.
            let end = AgentEvent::TurnEnd {
                outcome: Outcome::Interrupted,
                result: None,
                structured: None,
                error: None,
                usage: Usage::default(),
                cost: 0.0,
                total: session.cost,
                context: None,
            };
            self.record(store, id, end);
            session.state = State::Idle;
        } else if failed {
            let message = match said {
                "" => "the agent's process ended during its turn".into(),
                said => said.to_owned(),
            };
            self.record(store, id, AgentEvent::Error { message });
            session.state = State::Error;
        } else if session.state == State::Idle && !runtime.per_turn() {
            session.state = State::Stopped;
        }
        self.save(store, &mut session, entry)?;
        // What waited for the end of the turn: a message written meanwhile, a notice that wakes the agent.
        let due = entry.wake || !entry.queued.is_empty();
        if runtime.per_turn() && due && !entry.busy && session.state == State::Idle {
            self.deliver(store, id, entry, None)?;
        }
        Ok(())
    }

    /// Waits for the session's process to end, and kills it when it takes too long.
    fn wait_end(&self, id: i64, pid: u32) -> Result<()> {
        let running =
            |live: &mut HashMap<i64, Live>| live.get(&id).is_some_and(|e| e.process.is_some());
        let live = self.live.lock().unwrap();
        let (live, wait) = self
            .ended
            .wait_timeout_while(live, STOP_WAIT, running)
            .unwrap();
        if !wait.timed_out() {
            return Ok(());
        }
        drop(live);
        kill_group(pid);
        let live = self.live.lock().unwrap();
        let (live, wait) = self
            .ended
            .wait_timeout_while(live, KILL_WAIT, running)
            .unwrap();
        drop(live);
        ensure!(!wait.timed_out(), "the agent's process {pid} did not stop");
        Ok(())
    }
}

/// Empty values are the agent's defaults; none may start with `-`, which the agent would read as an option.
fn checked(mut spec: SessionSpec) -> Result<SessionSpec> {
    ensure!(
        spec.cwd.is_absolute() && spec.cwd.is_dir(),
        "not a folder: {}",
        spec.cwd.display()
    );
    for value in [
        &mut spec.project,
        &mut spec.agent,
        &mut spec.model,
        &mut spec.effort,
        &mut spec.permission_mode,
        &mut spec.role,
        &mut spec.squad,
    ] {
        *value = value
            .take()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        if let Some(v) = value {
            ensure!(!v.starts_with('-'), "{v} is not a valid value");
        }
    }
    for tools in [&mut spec.allowed_tools, &mut spec.disallowed_tools] {
        tools.retain(|t| !t.trim().is_empty());
        for tool in tools.iter_mut() {
            *tool = tool.trim().to_owned();
            ensure!(!tool.starts_with('-'), "{tool} is not a valid tool");
        }
    }
    ensure!(
        spec.budget.is_none_or(|b| b.is_finite() && b > 0.0),
        "the budget must be more than 0"
    );
    if spec.mcp {
        // Every action waits for the user: no mode that lets one through, whatever the role says.
        spec.permission_mode = Some("manual".into());
    }
    if spec.title.trim().is_empty() {
        let name = spec.cwd.file_name().unwrap_or(spec.cwd.as_os_str());
        spec.title = name.to_string_lossy().into_owned();
    }
    Ok(spec)
}

fn runtime_of(tool: Tool) -> &'static dyn AgentRuntime {
    match tool {
        Tool::Claude => &ClaudeCode,
        Tool::Codex => &Codex,
        Tool::Gemini => &Gemini,
    }
}

/// A session's runtime, by the id it was created with.
fn runtime(id: &str) -> &'static dyn AgentRuntime {
    [&Codex as &dyn AgentRuntime, &Gemini]
        .into_iter()
        .find(|r| r.id() == id)
        .unwrap_or(&ClaudeCode)
}

/// The `[agents]` entry named `name` (Codex CLI and Gemini CLI also when installed without one), else the first
/// that runs Claude Code; and what it runs.
fn agent_named(settings: &Settings, name: Option<&str>) -> Result<(String, Tool)> {
    let (name, command) = match name {
        Some(name) => {
            let command = settings.agent(name);
            let command =
                command.with_context(|| format!("no agent {name} in termf.toml [agents]"))?;
            (name.to_owned(), command)
        }
        None => settings
            .agents
            .iter()
            .find(|(_, c)| claude::is_claude(c))
            .map(|(n, c)| (n.clone(), c.clone()))
            .context("no agent runs Claude Code in termf.toml [agents]")?,
    };
    let tool = Tool::of(&command).with_context(|| {
        format!("the agent {name} does not run Claude Code, Codex CLI or Gemini CLI")
    })?;
    Ok((name, tool))
}

type RoleSettings = (termf_library::Item, Vec<termf_library::Setting>);

fn role_of(config_dir: &Path, spec: &SessionSpec, role: &str) -> Result<RoleSettings> {
    let scope = termf_library::Scope {
        squad: spec.squad.clone(),
        ..termf_library::Scope::new(config_dir, Some(&spec.cwd), spec.project.clone())?
    };
    termf_library::role_settings(config_dir, &scope, role)
}

/// What playing `role` sets that the spec does not: model, effort, permission mode, and the tools it allows or
/// denies (`allowedTools`, `disallowedTools` and `deny`, as lists or comma-separated). Its prompt comes with the
/// library, at each launch. On another runtime than Claude Code, the model, effort and permission mode the role's
/// own file names are Claude Code's: only those set in a `[roles.<name>]` table apply.
fn play(
    spec: &mut SessionSpec,
    (item, settings): &RoleSettings,
    role: &str,
    claude: bool,
) -> Result<()> {
    let text = |keys: &[&str]| {
        let key = keys.iter().find(|k| item.meta.contains_key(**k))?;
        let from = settings.iter().find(|s| s.key == *key).map(|s| &s.from);
        if !claude && from.is_none_or(|f| f.iter().all(|layer| layer == "role")) {
            return None;
        }
        item.meta.get(*key)?.as_str().map(str::to_owned)
    };
    let list = |key: &str| -> Vec<String> {
        match item.meta.get(key) {
            Some(Value::String(s)) => s.split(',').map(|t| t.trim().to_owned()).collect(),
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        }
    };
    spec.model = spec.model.take().or_else(|| text(&["model"]));
    spec.effort = spec.effort.take().or_else(|| text(&["effort"]));
    spec.permission_mode = spec
        .permission_mode
        .take()
        .or_else(|| text(&["permission_mode", "permissionMode"]));
    spec.allowed_tools.extend(list("allowedTools"));
    for key in ["disallowedTools", "deny"] {
        spec.disallowed_tools.extend(list(key));
    }
    for tools in [&mut spec.allowed_tools, &mut spec.disallowed_tools] {
        tools.retain(|t| !t.is_empty());
        let mut seen = std::collections::HashSet::new();
        tools.retain(|t| seen.insert(t.clone()));
    }
    let values = [&spec.model, &spec.effort, &spec.permission_mode];
    let tools = spec.allowed_tools.iter().chain(&spec.disallowed_tools);
    for value in values.into_iter().flatten().chain(tools) {
        ensure!(
            !value.starts_with('-'),
            "the role {role} sets {value}, which is not a valid value"
        );
    }
    Ok(())
}

/// Writes the library's plugin and system prompt into `dir`. A library that cannot be read never keeps the
/// agent from starting.
fn library(config_dir: &Path, spec: &SessionSpec, dir: &Path) -> Option<PathBuf> {
    termf_library::Scope::new(config_dir, Some(&spec.cwd), spec.project.clone())
        .and_then(|scope| {
            let scope = termf_library::Scope {
                role: spec.role.clone(),
                rules: spec.rules.clone(),
                squad: spec.squad.clone(),
                ..scope
            };
            termf_library::materialize(config_dir, &scope, dir)
        })
        .unwrap_or_else(|e| {
            eprintln!("termf: library not loaded: {e:#}");
            None
        })
}

/// What the agent reads: TermF's notices since its last message, set apart from the user's own words.
fn message(notices: &[String], text: Option<&str>) -> String {
    if notices.is_empty() {
        return text.unwrap_or_default().to_owned();
    }
    let mut out = String::from("<termf-notices>\n");
    for notice in notices {
        out.push_str(notice);
        out.push('\n');
    }
    out.push_str("</termf-notices>\nThese notices come from TermF, not from the user.");
    match text {
        Some(text) => {
            out.push_str(" The user's message follows.\n\n");
            out.push_str(text);
        }
        None => out.push_str(
            " Tell the user in a sentence or two what waits for them and ask what to do; act only on their answer.",
        ),
    }
    out
}

/// The project's repositories other than the session's folder, which a session with TermF's tools may read.
fn other_repos(config_dir: &Path, spec: &SessionSpec) -> Result<Vec<PathBuf>> {
    let projects = termf_core::load_projects(config_dir)?;
    let repos = projects
        .into_iter()
        .find(|p| Some(&p.id) == spec.project.as_ref())
        .map(|p| p.repos)
        .unwrap_or_default();
    Ok(repos
        .into_iter()
        .map(|r| PathBuf::from(r.path))
        .filter(|p| *p != spec.cwd && p.is_dir())
        .collect())
}

/// The vault's values a session's process got, masked in what it reports.
fn hide(event: AgentEvent, values: &[String]) -> AgentEvent {
    fn walk(value: &mut Value, values: &[String]) {
        match value {
            Value::String(text) => {
                for secret in values {
                    if text.contains(secret.as_str()) {
                        *text = text.replace(secret.as_str(), MASK);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|v| walk(v, values)),
            Value::Object(fields) => fields.values_mut().for_each(|v| walk(v, values)),
            _ => {}
        }
    }
    let Ok(mut value) = serde_json::to_value(&event) else {
        return event;
    };
    walk(&mut value, values);
    serde_json::from_value(value).unwrap_or(event)
}

/// What the process wrote on stderr, for an error message: its start, where the error is, and its end.
fn said(mut stderr: impl Read) -> String {
    let (mut head, mut kept, mut chunk) = (Vec::new(), Vec::new(), [0u8; 4096]);
    while let Ok(n) = stderr.read(&mut chunk) {
        if n == 0 {
            break;
        }
        if head.len() < 8192 {
            head.extend_from_slice(&chunk[..n]);
            continue;
        }
        kept.extend_from_slice(&chunk[..n]);
        let extra = kept.len().saturating_sub(8192);
        kept.drain(..extra);
    }
    head.push(b'\n');
    head.extend(kept);
    termf_core::process::excerpt(&String::from_utf8_lossy(&head))
}

fn interrupt_group(pid: u32) {
    #[cfg(unix)]
    termf_core::process::interrupt_group(pid);
    // shortcut: Windows has no interrupt signal for a process group; the process is ended instead.
    #[cfg(windows)]
    kill_group(pid);
}

/// Whether `pid` still runs this conversation's agent: its command line names the conversation, or the session's
/// folder (the first `codex exec` of a conversation knows no id yet), so a pid the system has given to another
/// program since is never taken for it.
pub(crate) fn ours(pid: u32, session_id: &str, dir: &Path) -> bool {
    #[cfg(unix)]
    let out = Command::new("ps")
        .args(["-ww", "-o", "args=", "-p", &pid.to_string()])
        .output();
    // shortcut: PowerShell takes a second; it is only asked when a process may have been left behind.
    #[cfg(windows)]
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-CimInstance Win32_Process -Filter 'ProcessId={pid}').CommandLine"),
        ])
        .output();
    let dir = dir.to_string_lossy();
    out.is_ok_and(|o| {
        let line = String::from_utf8_lossy(&o.stdout);
        line.contains(session_id) || line.contains(dir.as_ref())
    })
}

pub(crate) fn kill_group(pid: u32) {
    #[cfg(unix)]
    termf_core::process::kill_group(pid);
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

/// A random (version 4) UUID, which `--session-id` requires.
fn uuid() -> Result<String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("no randomness for a session id: {e}"))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}
