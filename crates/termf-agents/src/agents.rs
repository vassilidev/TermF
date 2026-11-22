//! The headless sessions of this launch of the app: at most one process per conversation, the events it
//! reports, and handing the conversation to a terminal and back.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use termf_core::{Settings, claude, ipc};
use termf_term::{shell_command, shell_quote};

use crate::store::{self, Store};
use crate::{
    AgentEvent, AgentRuntime, AgentSession, ClaudeCode, Launch, Outcome, SessionSpec, State,
    StoredEvent, Update, now,
};

/// How long a process may take to end once its stdin is closed, then once killed.
const STOP_WAIT: Duration = Duration::from_secs(10);
const KILL_WAIT: Duration = Duration::from_secs(5);
/// How long a terminal may take to close when its session is handed back.
const TILE_WAIT: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 10 });
const BUSY: &str = "the session is being stopped, taken over or handed back";

/// Clones share the same sessions.
#[derive(Clone)]
pub struct Agents {
    inner: Arc<Inner>,
}

struct Inner {
    config_dir: PathBuf,
    data_dir: PathBuf,
    runtime: Box<dyn AgentRuntime>,
    live: Mutex<HashMap<i64, Live>>,
    /// A process or a terminal ended.
    ended: Condvar,
    sink: Box<dyn Fn(Update) + Send + Sync>,
    /// Added to every process's environment.
    env: Vec<(String, String)>,
}

#[derive(Default)]
struct Live {
    process: Option<Process>,
    tile: Option<Tile>,
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
                runtime: Box::new(ClaudeCode),
                live: Mutex::default(),
                ended: Condvar::new(),
                sink: Box::new(sink),
                env,
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

    /// A new session, its process started, with `prompt` as its first message when there is one.
    pub fn start(&self, spec: SessionSpec, prompt: Option<&str>) -> Result<AgentSession> {
        let inner = &self.inner;
        let mut spec = checked(spec)?;
        let settings = termf_core::load_settings(&inner.config_dir)?;
        spec.agent = Some(claude_agent(&settings, spec.agent.as_deref())?);
        if let Some(role) = spec.role.clone() {
            play(&inner.config_dir, &mut spec, &role)?;
        }
        let store = inner.store()?;
        let id = store.create(inner.runtime.id(), &uuid()?, &spec)?.id;
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        match prompt.filter(|p| !p.trim().is_empty()) {
            Some(text) => inner.deliver(&store, id, entry, text)?,
            None => inner.launch(&store, id, entry)?,
        }
        inner.view(&store, id, entry)
    }

    /// Sends a message between two turns. A session without a process resumes its conversation first.
    pub fn send(&self, id: i64, text: &str) -> Result<()> {
        ensure!(!text.trim().is_empty(), "the message is empty");
        let inner = &self.inner;
        let store = inner.store()?;
        let mut live = inner.live.lock().unwrap();
        inner.deliver(&store, id, live.entry(id).or_default(), text)
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
        inner.write(entry, &inner.runtime.interrupt())?;
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
        inner.write(entry, &inner.runtime.answer(request, allow, &input, why))?;
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
                let _ = inner.write(entry, &inner.runtime.interrupt());
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
            args: inner.runtime.interactive_args(spec, &session.session_id),
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
        let mut live = inner.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        entry.busy = false;
        entry.tile = None;
        inner.record(&store, id, AgentEvent::HandedBack);
        inner.set_state(&store, id, entry, State::Stopped)?;
        inner.launch(&store, id, entry)?;
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
    fn record(&self, store: &Store, id: i64, event: AgentEvent) {
        match store.record(id, &event) {
            Ok(stored) => (self.sink)(Update::Event(stored)),
            Err(e) => eprintln!("termf: agent session {id}: event not saved: {e:#}"),
        }
    }

    /// Kills what the last launch of the app left running, and frees the sessions whose process is gone.
    fn reap(&self, store: &Store) -> Result<()> {
        store.stop_all()?;
        for (id, pid, session_id) in store.pids()? {
            if ours(pid, &session_id) {
                kill_group(pid);
                let deadline = std::time::Instant::now() + KILL_WAIT;
                while ours(pid, &session_id) && std::time::Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            if ours(pid, &session_id) {
                let message = format!("its process {pid} from the last launch still runs");
                self.record(store, id, AgentEvent::Error { message });
                self.set_state(store, id, &Live::default(), State::Error)?;
            } else {
                store.clear_pid(id, pid)?;
            }
        }
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

    fn deliver(
        self: &Arc<Self>,
        store: &Store,
        id: i64,
        entry: &mut Live,
        text: &str,
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
        self.write(entry, &self.runtime.message(text))?;
        entry.interrupting = false;
        self.record(store, id, AgentEvent::User { text: text.into() });
        self.set_state(store, id, entry, State::Working)
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
            && ours(pid, &session.session_id)
        {
            bail!("a process of this session still runs (pid {pid}): it must end first");
        }
        let settings = termf_core::load_settings(&self.config_dir)?;
        let agent = session.spec.agent.as_deref().unwrap_or_default();
        let command = settings
            .agents
            .get(agent)
            .with_context(|| format!("no agent {agent} in termf.toml [agents]"))?;
        let dir = store.dir(id).join("session");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        let system_prompt = library(&self.config_dir, &session.spec, &dir);
        let plugin = dir.join("plugin");
        let resume = store.started(id)?;
        let args = self.runtime.headless_args(
            &session.spec,
            &Launch {
                session_id: &session.session_id,
                resume,
                budget,
                plugin: plugin.is_dir().then_some(plugin.as_path()),
                system_prompt: system_prompt.as_deref(),
            },
        );
        let args: Vec<String> = args.iter().map(|a| shell_quote(a)).collect();
        // Through the login shell, as a tile: `~`, quotes and the shell's PATH work the same.
        let (program, shell_args) = shell_command(Some(&format!("{command} {}", args.join(" "))));
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
        cmd.envs(self.env.iter().map(|(k, v)| (k, v)));
        // Its own process group, so stopping it also stops what it started.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
        let mut child = cmd.spawn().context("cannot start the agent")?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            bail!("the agent's process has no pipes");
        };
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
        let said = std::thread::spawn(move || tail(stderr));
        let inner = self.clone();
        std::thread::spawn(move || inner.read(id, generation, child, stdout, said));
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
    ) {
        let pid = child.id();
        let mut parser = self.runtime.parser();
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        // Opened again only after a failure.
        let mut store = None;
        while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            for event in parser.parse(&line) {
                let handled = self
                    .opened(&mut store)
                    .and_then(|s| self.handle(s, id, generation, event));
                if let Err(e) = handled {
                    eprintln!("termf: agent session {id}: {e:#}");
                }
            }
            line.clear();
        }
        let code = child.wait().ok().and_then(|s| s.code());
        let said = said.join().unwrap_or_default();
        if let Err(e) = self.exited(&mut store, id, generation, pid, code, &said) {
            eprintln!("termf: agent session {id}: {e:#}");
        }
    }

    fn opened<'a>(&self, store: &'a mut Option<Store>) -> Result<&'a Store> {
        if store.is_none() {
            *store = Some(self.store()?);
        }
        Ok(store.as_ref().expect("just opened"))
    }

    fn handle(&self, store: &Store, id: i64, generation: u64, mut event: AgentEvent) -> Result<()> {
        let mut live = self.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        // A process being stopped still reports, e.g. the end of its interrupted turn: the state is already set.
        let current = entry
            .process
            .as_ref()
            .is_some_and(|p| p.generation == generation);
        if let AgentEvent::Delta { .. } = event {
            if current {
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
        Ok(())
    }

    fn exited(
        &self,
        store: &mut Option<Store>,
        id: i64,
        generation: u64,
        pid: u32,
        code: Option<i32>,
        said: &str,
    ) -> Result<()> {
        let mut live = self.live.lock().unwrap();
        let entry = live.entry(id).or_default();
        let current = entry
            .process
            .as_ref()
            .is_some_and(|p| p.generation == generation);
        if current {
            entry.process = None;
            entry.pending.clear();
            entry.interrupting = false;
        }
        self.ended.notify_all();
        let store = self.opened(store)?;
        store.clear_pid(id, pid)?;
        self.record(store, id, AgentEvent::Exited { code });
        if !current {
            return Ok(());
        }
        let mut session = store.session(id)?;
        let failed = matches!(session.state, State::Working | State::Waiting)
            || (session.state == State::Idle && code != Some(0));
        if failed {
            let message = match said {
                "" => "the agent's process ended during its turn".into(),
                said => said.to_owned(),
            };
            self.record(store, id, AgentEvent::Error { message });
            session.state = State::Error;
        } else if session.state == State::Idle {
            session.state = State::Stopped;
        }
        self.save(store, &mut session, entry)
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
    if spec.title.trim().is_empty() {
        let name = spec.cwd.file_name().unwrap_or(spec.cwd.as_os_str());
        spec.title = name.to_string_lossy().into_owned();
    }
    Ok(spec)
}

/// The `[agents]` entry named `name`, else the first that runs Claude Code.
fn claude_agent(settings: &Settings, name: Option<&str>) -> Result<String> {
    let (name, command) = match name {
        Some(name) => settings
            .agents
            .get_key_value(name)
            .with_context(|| format!("no agent {name} in termf.toml [agents]"))?,
        None => settings
            .agents
            .iter()
            .find(|(_, c)| claude::is_claude(c))
            .context("no agent runs Claude Code in termf.toml [agents]")?,
    };
    ensure!(
        claude::is_claude(command),
        "the agent {name} does not run Claude Code"
    );
    Ok(name.clone())
}

/// What playing `role` sets that the spec does not: model, effort, permission mode, and the tools it allows or
/// denies (`allowedTools`, `disallowedTools` and `deny`, as lists or comma-separated). Its prompt comes with the
/// library, at each launch.
fn play(config_dir: &Path, spec: &mut SessionSpec, role: &str) -> Result<()> {
    let scope = termf_library::Scope {
        squad: spec.squad.clone(),
        ..termf_library::Scope::new(config_dir, Some(&spec.cwd), spec.project.clone())?
    };
    let item = termf_library::role(config_dir, &scope, role)?;
    let text = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| item.meta.get(*k)?.as_str().map(str::to_owned))
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

/// The end of what the process wrote on stderr, for an error message.
fn tail(mut stderr: impl Read) -> String {
    let (mut kept, mut chunk) = (Vec::new(), [0u8; 4096]);
    while let Ok(n) = stderr.read(&mut chunk) {
        if n == 0 {
            break;
        }
        kept.extend_from_slice(&chunk[..n]);
        let extra = kept.len().saturating_sub(8192);
        kept.drain(..extra);
    }
    termf_core::process::tail(&String::from_utf8_lossy(&kept))
}

/// Whether `pid` still runs this conversation's agent: its command line names the conversation, so a pid the
/// system has given to another program since is never taken for it.
pub(crate) fn ours(pid: u32, session_id: &str) -> bool {
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
    out.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(session_id))
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
