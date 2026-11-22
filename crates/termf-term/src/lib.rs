use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, ensure};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;

const SCROLLBACK_BYTES: usize = 1 << 20;
/// How far past the cut the scrollback looks for a line start.
const LINE_SEARCH: usize = 64 << 10;

pub enum Output {
    Data(Vec<u8>),
    Exit(Option<u32>),
}

pub type Sink = Box<dyn FnMut(Output) + Send>;

type EnvFn = Box<dyn Fn(u32) -> Vec<(String, String)> + Send + Sync>;
type ExitFn = Arc<dyn Fn(u32) + Send + Sync>;
type ChildRef = Arc<Mutex<Box<dyn Child + Send + Sync>>>;

pub struct SpawnSpec {
    pub title: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Agent name the UI launched, `None` for a plain shell.
    pub agent: Option<String>,
    pub cols: u16,
    pub rows: u16,
    /// Added to the session's environment, after the registry's own.
    pub env: Vec<(String, String)>,
    /// What it starts from instead of TermF's own environment, before the registry's and `env`: an agent's
    /// (`termf_core::agent_env`).
    pub base_env: Option<Vec<(OsString, OsString)>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: u32,
    pub title: String,
    pub cwd: String,
    pub agent: Option<String>,
    pub running: bool,
    pub exit_code: Option<u32>,
}

#[derive(Default)]
struct Shared {
    scrollback: VecDeque<u8>,
    sink: Option<Sink>,
    exit: Option<Option<u32>>,
    /// Which process of the session is current: one replaced by `respawn_with` ends unheard.
    process: u64,
    /// When the program last wrote.
    written: Option<std::time::Instant>,
}

impl Shared {
    fn push(&mut self, chunk: &[u8]) {
        self.written = Some(std::time::Instant::now());
        self.scrollback.extend(chunk);
        let overflow = self.scrollback.len().saturating_sub(SCROLLBACK_BYTES);
        if overflow > 0 {
            // A replay starts at a line, never inside a character or an escape sequence.
            let rest = self.scrollback.range(overflow..).take(LINE_SEARCH);
            // shortcut: with no line and no escape sequence starting near the cut, a replay may start inside a
            // sequence left unfinished; trim by parsing the sequences if that ever shows.
            let cut = match rest.clone().position(|&b| b == b'\n') {
                Some(i) => overflow + i + 1,
                None => match rest.clone().position(|&b| b == 0x1b) {
                    Some(i) => overflow + i,
                    None => overflow + rest.take_while(|&&b| b & 0xC0 == 0x80).count(),
                },
            };
            self.scrollback.drain(..cut);
        }
        if let Some(sink) = self.sink.as_mut() {
            sink(Output::Data(chunk.to_vec()));
        }
    }

    fn finish(&mut self, code: Option<u32>) {
        self.exit = Some(code);
        if let Some(sink) = self.sink.as_mut() {
            sink(Output::Exit(code));
        }
    }
}

struct Session {
    title: String,
    cwd: PathBuf,
    agent: Option<String>,
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: ChildRef,
    shared: Arc<Mutex<Shared>>,
}

struct Process {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: ChildRef,
}

/// How long a hung-up process has to end by itself (Claude Code saves its conversation and runs its SessionEnd
/// hooks) before it is killed.
const GRACE: Duration = Duration::from_secs(5);

/// Hangs up on the process, waits up to [`GRACE`] for it to end, then kills it, as one that ignores SIGHUP.
fn hang_up(child: &ChildRef) {
    let running = |child: &mut Box<dyn Child + Send + Sync>| matches!(child.try_wait(), Ok(None));
    {
        let mut child = child.lock().unwrap();
        // Already reaped: its pid may belong to another process by now.
        if !running(&mut child) {
            return;
        }
        // SIGHUP under Unix (TerminateProcess under Windows).
        let _ = child.clone_killer().kill();
    }
    let deadline = Instant::now() + GRACE;
    while Instant::now() < deadline {
        if !running(&mut child.lock().unwrap()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut child = child.lock().unwrap();
    if running(&mut child) {
        let _ = child.kill();
    }
}

/// [`hang_up`] in the background: nothing waits for it.
fn end(child: &ChildRef) {
    let child = child.clone();
    std::thread::spawn(move || hang_up(&child));
}

#[derive(Default)]
pub struct Sessions {
    map: Mutex<HashMap<u32, Session>>,
    next_id: AtomicU32,
    env: Option<EnvFn>,
    on_exit: Option<ExitFn>,
}

impl Sessions {
    /// `env` gives the extra environment variables of each session from its id.
    pub fn with_env(env: impl Fn(u32) -> Vec<(String, String)> + Send + Sync + 'static) -> Self {
        Self {
            env: Some(Box::new(env)),
            ..Self::default()
        }
    }

    /// `on_exit` runs with the session id once its process has ended, whether or not a view is attached.
    pub fn on_exit(self, on_exit: impl Fn(u32) + Send + Sync + 'static) -> Self {
        Self {
            on_exit: Some(Arc::new(on_exit)),
            ..self
        }
    }

    pub fn spawn(&self, spec: SpawnSpec) -> Result<u32> {
        self.spawn_with(|_| Ok(spec))
    }

    /// `build` receives the new session id, so the command can refer to it.
    pub fn spawn_with(&self, build: impl FnOnce(u32) -> Result<SpawnSpec>) -> Result<u32> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let spec = build(id)?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let process = self.start(id, &spec, &shared)?;
        self.map.lock().unwrap().insert(
            id,
            Session {
                title: spec.title,
                cwd: spec.cwd,
                agent: spec.agent,
                master: process.master,
                writer: Arc::new(Mutex::new(process.writer)),
                child: process.child,
                shared,
            },
        );
        Ok(id)
    }

    /// Ends a session's process and starts another in its place, at the same size: same id, same view and scrollback,
    /// and no exit reported for the one replaced.
    pub fn respawn_with(
        &self,
        id: u32,
        build: impl FnOnce(u32) -> Result<SpawnSpec>,
    ) -> Result<()> {
        let (shared, size) = self.with(id, |s| (s.shared.clone(), s.master.get_size()))?;
        let mut spec = build(id)?;
        if let Ok(size) = size {
            (spec.cols, spec.rows) = (size.cols, size.rows);
        }
        let old = self.with(id, |s| s.child.clone())?;
        {
            let mut shared = shared.lock().unwrap();
            shared.process += 1;
            shared.exit = None;
        }
        // Never two processes at once: the new one may resume the same conversation.
        hang_up(&old);
        let process = match self.start(id, &spec, &shared) {
            Ok(process) => process,
            Err(e) => {
                // Nothing runs in the session any more: it ends as if its process had.
                shared.lock().unwrap().finish(None);
                if let Some(on_exit) = &self.on_exit {
                    on_exit(id);
                }
                return Err(e);
            }
        };
        let mut map = self.map.lock().unwrap();
        let session = map
            .get_mut(&id)
            .ok_or_else(|| anyhow!("unknown session {id}"))?;
        session.master = process.master;
        *session.writer.lock().unwrap() = process.writer;
        session.child = process.child;
        Ok(())
    }

    fn start(&self, id: u32, spec: &SpawnSpec, shared: &Arc<Mutex<Shared>>) -> Result<Process> {
        let pair = native_pty_system().openpty(size(spec.cols, spec.rows))?;

        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.cwd(&spec.cwd);
        if let Some(base) = &spec.base_env {
            cmd.env_clear();
            for (key, value) in base {
                cmd.env(key, value);
            }
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (key, value) in self.env.as_ref().map(|env| env(id)).unwrap_or_default() {
            cmd.env(key, value);
        }
        for (key, value) in &spec.env {
            cmd.env(key, value);
        }
        let child: ChildRef = Arc::new(Mutex::new(
            pair.slave
                .spawn_command(cmd)
                .with_context(|| format!("cannot start {}", spec.program))?,
        ));
        // The master only sees EOF once every slave handle is closed.
        drop(pair.slave);

        let thread_child = child.clone();
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let process = shared.lock().unwrap().process;

        let thread_shared = shared.clone();
        let on_exit = self.on_exit.clone();
        std::thread::Builder::new()
            .name(format!("pty-{id}"))
            .spawn(move || {
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => thread_shared.lock().unwrap().push(&buf[..n]),
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                // Polled, so `end` never waits behind a process that closed its terminal but lives on.
                let mut pause = Duration::from_millis(10);
                let code = loop {
                    match thread_child.lock().unwrap().try_wait() {
                        Ok(Some(status)) => break Some(status.exit_code()),
                        Ok(None) => std::thread::sleep(pause),
                        Err(_) => break None,
                    }
                    pause = (pause * 2).min(Duration::from_secs(1));
                };
                {
                    let mut shared = thread_shared.lock().unwrap();
                    if shared.process != process {
                        return;
                    }
                    shared.finish(code);
                }
                if let Some(on_exit) = on_exit {
                    on_exit(id);
                }
            })?;
        Ok(Process {
            master: pair.master,
            writer,
            child,
        })
    }

    /// Replaces the output sink and returns the scrollback, so a new view replays history then streams.
    pub fn attach(&self, id: u32, mut sink: Sink) -> Result<Vec<u8>> {
        let shared = self.with(id, |s| s.shared.clone())?;
        let mut shared = shared.lock().unwrap();
        let snapshot = shared.scrollback.iter().copied().collect();
        if let Some(code) = shared.exit {
            sink(Output::Exit(code));
        }
        shared.sink = Some(sink);
        Ok(snapshot)
    }

    pub fn write(&self, id: u32, data: &[u8]) -> Result<()> {
        let (writer, shared) = self.with(id, |s| (s.writer.clone(), s.shared.clone()))?;
        // Rather than the terminal's "Input/output error".
        ensure!(
            shared.lock().unwrap().exit.is_none(),
            "session {id} has ended"
        );
        let mut writer = writer.lock().unwrap();
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    /// Types `text` then Enter, sent apart so a TUI that detects pastes, like Claude Code, reads Enter as a key press.
    pub fn type_line(&self, id: u32, text: &str) -> Result<()> {
        self.write(id, text.as_bytes())?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.write(id, b"\r")
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<()> {
        self.with(id, |s| s.master.resize(size(cols, rows)))?
    }

    pub fn kill(&self, id: u32) -> Result<()> {
        let session = self
            .map
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(|| anyhow!("unknown session {id}"))?;
        end(&session.child);
        Ok(())
    }

    /// Ends the session's process but keeps the session, as when it exits by itself.
    pub fn terminate(&self, id: u32) -> Result<()> {
        end(&self.with(id, |s| s.child.clone())?);
        Ok(())
    }

    /// How long the session's program has written nothing; `None` when it never wrote or is unknown.
    pub fn quiet_for(&self, id: u32) -> Option<std::time::Duration> {
        let shared = self.with(id, |s| s.shared.clone()).ok()?;
        let written = shared.lock().unwrap().written?;
        Some(written.elapsed())
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        let mut list: Vec<_> = self
            .map
            .lock()
            .unwrap()
            .iter()
            .map(|(id, s)| {
                let exit = s.shared.lock().unwrap().exit;
                SessionInfo {
                    id: *id,
                    title: s.title.clone(),
                    cwd: s.cwd.to_string_lossy().into_owned(),
                    agent: s.agent.clone(),
                    running: exit.is_none(),
                    exit_code: exit.flatten(),
                }
            })
            .collect();
        list.sort_by_key(|s| s.id);
        list
    }

    fn with<T>(&self, id: u32, f: impl FnOnce(&Session) -> T) -> Result<T> {
        let map = self.map.lock().unwrap();
        let session = map
            .get(&id)
            .ok_or_else(|| anyhow!("unknown session {id}"))?;
        Ok(f(session))
    }
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols,
        rows,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Program and args that run `command` (or an interactive shell when `None`) through the user's login shell,
/// so GUI launches still get the PATH from the shell profile.
pub fn shell_command(command: Option<&str>) -> (String, Vec<String>) {
    if cfg!(windows) {
        match command {
            Some(c) => ("cmd.exe".into(), vec!["/d".into(), "/c".into(), c.into()]),
            None => ("powershell.exe".into(), vec!["-NoLogo".into()]),
        }
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        match command {
            Some(c) => (shell, vec!["-l".into(), "-i".into(), "-c".into(), c.into()]),
            None => (shell, vec!["-l".into()]),
        }
    }
}

/// Quotes `arg` as one word for the shell `shell_command` runs.
pub fn shell_quote(arg: &str) -> String {
    if cfg!(windows) {
        // shortcut: cmd.exe has no escape for a double quote inside quotes, so they are dropped.
        format!("\"{}\"", arg.replace('"', ""))
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn collect(sessions: &Sessions, id: u32) -> (String, Option<u32>) {
        let (tx, rx) = mpsc::channel();
        let snapshot = sessions
            .attach(
                id,
                Box::new(move |out| {
                    let _ = tx.send(out);
                }),
            )
            .unwrap();
        let mut text = String::from_utf8_lossy(&snapshot).into_owned();
        loop {
            match rx
                .recv_timeout(Duration::from_secs(10))
                .expect("pty timed out")
            {
                Output::Data(d) => text.push_str(&String::from_utf8_lossy(&d)),
                Output::Exit(code) => return (text, code),
            }
        }
    }

    fn spec(program: &str, args: &[&str]) -> SpawnSpec {
        SpawnSpec {
            title: "test".into(),
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: std::env::temp_dir(),
            agent: None,
            cols: 80,
            rows: 24,
            env: Vec::new(),
            base_env: None,
        }
    }

    #[test]
    fn a_shell_keeps_termf_s_environment_and_an_agent_only_its_own() {
        let sessions = Sessions::with_env(|_| vec![("TERMF_TOKEN".into(), "t".into())]);
        let home = std::env::var("HOME").unwrap();
        let printed = |base_env: Option<Vec<(OsString, OsString)>>| {
            let script = r#"printf '[%s|%s|%s]' "$HOME" "$TERMF_TOKEN" "$PEXELS_API_KEY""#;
            let mut spec = spec("/bin/sh", &["-c", script]);
            spec.base_env = base_env;
            spec.env.push(("PEXELS_API_KEY".into(), "p".into()));
            collect(&sessions, sessions.spawn(spec).unwrap()).0
        };
        assert!(printed(None).contains(&format!("[{home}|t|p]")));
        assert!(printed(Some(Vec::new())).contains("[|t|p]"));
        let other = vec![("HOME".into(), "/home/agent".into())];
        assert!(printed(Some(other)).contains("[/home/agent|t|p]"));
    }

    #[test]
    fn replays_output_and_reports_exit() {
        let sessions = Sessions::default();
        let id = sessions
            .spawn(spec("/bin/sh", &["-c", "printf hello; exit 3"]))
            .unwrap();
        let (text, code) = collect(&sessions, id);
        assert!(text.contains("hello"), "got {text:?}");
        assert_eq!(code, Some(3));
        assert!(!sessions.list()[0].running);
        assert_eq!(
            sessions.list()[0].cwd,
            std::env::temp_dir().to_string_lossy()
        );
    }

    #[test]
    fn reports_exit_without_a_view() {
        let (tx, rx) = mpsc::channel();
        let sessions = Sessions::default().on_exit(move |id| {
            let _ = tx.send(id);
        });
        let id = sessions.spawn(spec("/bin/sh", &["-c", "exit 0"])).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)), Ok(id));
    }

    #[test]
    fn passes_env_and_quotes_args() {
        let sessions = Sessions::with_env(|id| vec![("TERMF_TEST_ID".into(), id.to_string())]);
        let tricky = "it's a \"test\" $HOME `x` · ok";
        let mut with_env = spec(
            "/bin/sh",
            &[
                "-c",
                &format!(
                    "printf '[%s|%s|%s]' \"$TERMF_TEST_ID\" \"$TERMF_TEST_RUN\" {}",
                    shell_quote(tricky)
                ),
            ],
        );
        with_env.env.push(("TERMF_TEST_RUN".into(), "41000".into()));
        let id = sessions.spawn(with_env).unwrap();
        let (text, _) = collect(&sessions, id);
        assert!(
            text.contains(&format!("[{id}|41000|{tricky}]")),
            "got {text:?}"
        );
    }

    #[test]
    fn types_a_line_then_enter() {
        let sessions = Sessions::default();
        // Raw mode: the bytes as typed, Enter included, dumped in hex.
        let id = sessions
            .spawn(spec(
                "/bin/sh",
                &[
                    "-c",
                    "stty raw -echo; printf ready; head -c 7 | od -An -tx1",
                ],
            ))
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let mut seen = String::from_utf8_lossy(
            &sessions
                .attach(
                    id,
                    Box::new(move |out| {
                        if let Output::Data(d) = out {
                            let _ = tx.send(d);
                        }
                    }),
                )
                .unwrap(),
        )
        .into_owned();
        let mut wait_for = |text: &str| {
            while !seen.contains(text) {
                let d = rx.recv_timeout(Duration::from_secs(10)).expect("no output");
                seen.push_str(&String::from_utf8_lossy(&d));
            }
        };
        wait_for("ready");
        sessions.type_line(id, "/clear").unwrap();
        wait_for("0d");
        let hex = seen.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(hex.contains("2f 63 6c 65 61 72 0d"), "got {seen:?}");
    }

    #[test]
    fn writes_input_and_kills() {
        let sessions = Sessions::default();
        let id = sessions.spawn(spec("/bin/cat", &[])).unwrap();
        sessions.write(id, b"ping\n").unwrap();
        sessions.resize(id, 100, 30).unwrap();
        let (tx, rx) = mpsc::channel();
        let snapshot = sessions
            .attach(
                id,
                Box::new(move |out| {
                    if let Output::Data(d) = out {
                        let _ = tx.send(d);
                    }
                }),
            )
            .unwrap();
        let mut seen = String::from_utf8_lossy(&snapshot).into_owned();
        while !seen.contains("ping") {
            let d = rx.recv_timeout(Duration::from_secs(10)).expect("no echo");
            seen.push_str(&String::from_utf8_lossy(&d));
        }
        sessions.kill(id).unwrap();
        assert!(sessions.list().is_empty());
    }

    #[test]
    fn terminates_and_keeps_the_session() {
        let sessions = Sessions::default();
        let id = sessions.spawn(spec("/bin/cat", &[])).unwrap();
        sessions.terminate(id).unwrap();
        collect(&sessions, id);
        assert!(!sessions.list()[0].running);
        sessions.terminate(id).unwrap();
        assert!(sessions.terminate(id + 1).is_err());
    }

    #[test]
    fn respawns_in_place_at_the_same_size() {
        let (tx, exits) = mpsc::channel();
        let sessions = Sessions::with_env(|id| vec![("TERMF_TEST_ID".into(), id.to_string())])
            .on_exit(move |id| {
                let _ = tx.send(id);
            });
        let id = sessions
            .spawn(spec("/bin/sh", &["-c", "printf first; exec cat"]))
            .unwrap();
        sessions.resize(id, 101, 31).unwrap();
        while !String::from_utf8_lossy(&sessions.attach(id, Box::new(|_| {})).unwrap())
            .contains("first")
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        sessions
            .respawn_with(id, |same| {
                assert_eq!(same, id);
                Ok(spec(
                    "/bin/sh",
                    &[
                        "-c",
                        "printf \"second $TERMF_TEST_ID $(stty size)\"; exit 4",
                    ],
                ))
            })
            .unwrap();
        let (text, code) = collect(&sessions, id);
        assert!(text.contains("first"), "the scrollback stays: {text:?}");
        assert!(text.contains(&format!("second {id} 31 101")), "{text:?}");
        assert_eq!(code, Some(4), "the new process's exit");
        assert_eq!(exits.recv_timeout(Duration::from_secs(10)), Ok(id));
        assert!(
            exits.recv_timeout(Duration::from_millis(300)).is_err(),
            "the process replaced ends unheard"
        );
        assert_eq!(sessions.list().len(), 1);
        assert!(
            sessions
                .respawn_with(id + 1, |_| Ok(spec("/bin/true", &[])))
                .is_err()
        );
    }

    fn alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .status()
            .unwrap()
            .success()
    }

    fn wait_for(sessions: &Sessions, id: u32, text: &str) -> String {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let seen = String::from_utf8_lossy(&sessions.attach(id, Box::new(|_| {})).unwrap())
                .into_owned();
            if seen.contains(text) {
                return seen;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no {text:?} in {seen:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn ends_a_process_that_ignores_the_hang_up() {
        let sessions = Sessions::default();
        let ignores = "trap '' HUP; echo pid=$$.; while :; do sleep 1; done";
        let pid = |id| {
            let seen = wait_for(&sessions, id, ".");
            seen.split("pid=")
                .nth(1)
                .unwrap()
                .split('.')
                .next()
                .unwrap()
                .to_owned()
        };
        let closed = sessions.spawn(spec("/bin/sh", &["-c", ignores])).unwrap();
        let closed_pid = pid(closed);
        sessions.kill(closed).unwrap();
        let stopped = sessions.spawn(spec("/bin/sh", &["-c", ignores])).unwrap();
        let stopped_pid = pid(stopped);
        sessions.terminate(stopped).unwrap();
        collect(&sessions, stopped);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while alive(&closed_pid) || alive(&stopped_pid) {
            assert!(std::time::Instant::now() < deadline, "still running");
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!sessions.list()[0].running);
    }

    /// A simple generator with a fixed seed, so a failure reproduces.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    /// Random terminal output, cut anywhere into chunks (inside characters and escape sequences too):
    /// the scrollback stays within its size, is the end of what was written, and starts at a line, or
    /// at a character when no line starts near the cut.
    #[test]
    fn the_scrollback_keeps_the_end_and_starts_at_a_line() {
        const PIECES: [&str; 8] = [
            "a",
            "é",
            "日本",
            "😀",
            "\x1b[38;5;196m",
            "\x1b[0m",
            "\r\n",
            "x y",
        ];
        for (seed, newlines) in [(7, true), (11, true), (13, false)] {
            let mut rng = Rng(seed);
            let mut shared = Shared::default();
            let mut written = Vec::new();
            let mut checks = 0;
            while written.len() < 3 * SCROLLBACK_BYTES {
                let mut chunk = Vec::new();
                for _ in 0..rng.below(4000) {
                    let piece = PIECES[rng.below(PIECES.len() - usize::from(!newlines) * 2)];
                    chunk.extend_from_slice(piece.as_bytes());
                }
                // Lines of a few hundred bytes at most.
                if newlines && rng.below(2) == 0 {
                    chunk.push(b'\n');
                }
                let mut rest = &chunk[..];
                while !rest.is_empty() {
                    let n = 1 + rng.below(rest.len().min(20_000));
                    shared.push(&rest[..n]);
                    written.extend_from_slice(&rest[..n]);
                    rest = &rest[n..];
                }
                if rng.below(20) != 0 {
                    continue;
                }
                checks += 1;
                let kept: Vec<u8> = shared.scrollback.iter().copied().collect();
                assert!(kept.len() <= SCROLLBACK_BYTES);
                assert!(
                    written.ends_with(&kept),
                    "seed {seed}: not the end of the output"
                );
                if kept.len() == written.len() {
                    continue;
                }
                assert!(
                    kept.len() > SCROLLBACK_BYTES / 2,
                    "seed {seed}: cut too far"
                );
                let before = written[written.len() - kept.len() - 1];
                if newlines {
                    assert_eq!(before, b'\n', "seed {seed}: replay starts inside a line");
                } else {
                    assert!(
                        kept[0] & 0xC0 != 0x80,
                        "seed {seed}: replay starts inside a character"
                    );
                }
            }
            assert!(checks > 5);
        }
    }

    /// Megabytes of multibyte output through a real terminal: nothing lost on the way, the session ends,
    /// and its replay is whole lines of valid UTF-8 ending with the last one.
    #[test]
    fn streams_a_large_output_and_replays_its_end() {
        let sessions = Sessions::default();
        let id = sessions
            .spawn(spec(
                "/bin/sh",
                &[
                    "-c",
                    "i=0; while [ $i -lt 40 ]; do yes 'é日本😀 ligne' | head -n 5000; i=$((i+1)); done; echo fin-$((6*7))",
                ],
            ))
            .unwrap();
        let (tx, rx) = mpsc::channel();
        sessions
            .attach(
                id,
                Box::new(move |out| {
                    let _ = tx.send(out);
                }),
            )
            .unwrap();
        let mut streamed = 0;
        let code = loop {
            match rx
                .recv_timeout(Duration::from_secs(30))
                .expect("pty timed out")
            {
                Output::Data(d) => streamed += d.len(),
                Output::Exit(code) => break code,
            }
        };
        assert_eq!(code, Some(0));
        let line = "é日本😀 ligne\r\n".len();
        assert!(streamed >= 200_000 * line, "{streamed} bytes");
        let replay = sessions.attach(id, Box::new(|_| {})).unwrap();
        assert!(replay.len() <= SCROLLBACK_BYTES);
        let text = std::str::from_utf8(&replay).expect("valid UTF-8");
        assert!(text.starts_with("é日本😀 ligne\r\n"), "{:?}", &text[..40]);
        assert!(
            text.ends_with("fin-42\r\n"),
            "{:?}",
            &text[text.len() - 40..]
        );
    }

    #[test]
    fn unicode_typed_comes_back_whole() {
        let sessions = Sessions::default();
        let id = sessions.spawn(spec("/bin/cat", &[])).unwrap();
        let typed = "héllo 日本語 😀 e\u{301} \u{202e}rtl\u{202c} Ω\n";
        sessions.write(id, typed.as_bytes()).unwrap();
        let seen = wait_for(&sessions, id, "Ω\r\n");
        assert!(
            seen.contains("héllo 日本語 😀 e\u{301} \u{202e}rtl\u{202c} Ω"),
            "{seen:?}"
        );
        sessions.kill(id).unwrap();
    }

    #[test]
    fn a_running_program_sees_the_new_size() {
        let sessions = Sessions::default();
        let id = sessions
            .spawn(spec(
                "/bin/sh",
                &["-c", "trap 'echo size=$(stty size).' WINCH; echo ready.; while :; do sleep 0.05; done"],
            ))
            .unwrap();
        wait_for(&sessions, id, "ready.");
        sessions.resize(id, 132, 50).unwrap();
        wait_for(&sessions, id, "size=50 132.");
        sessions.resize(id, 1, 1).unwrap();
        wait_for(&sessions, id, "size=1 1.");
        sessions.kill(id).unwrap();
    }

    #[test]
    fn writing_to_an_ended_session_never_hangs() {
        let sessions = Sessions::default();
        let id = sessions.spawn(spec("/bin/sh", &["-c", "exit 0"])).unwrap();
        collect(&sessions, id);
        let (tx, rx) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..100 {
                    let _ = sessions.write(id, &[b'x'; 4096]);
                }
                let _ = tx.send(());
            });
            rx.recv_timeout(Duration::from_secs(10))
                .expect("write blocked");
        });
        sessions.resize(id, 10, 10).ok();
        let ended = sessions.write(id, b"x").unwrap_err().to_string();
        assert!(ended.contains("has ended"), "{ended}");
        assert!(sessions.write(id + 1, b"x").is_err());
    }
}
