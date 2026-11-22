use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;

// shortcut: trimming by bytes may cut an escape sequence on replay, upgrade to line-aware trimming if it shows.
const SCROLLBACK_BYTES: usize = 1 << 20;

pub enum Output {
    Data(Vec<u8>),
    Exit(Option<u32>),
}

pub type Sink = Box<dyn FnMut(Output) + Send>;

type EnvFn = Box<dyn Fn(u32) -> Vec<(String, String)> + Send + Sync>;
type ExitFn = Arc<dyn Fn(u32) + Send + Sync>;

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
        self.scrollback.drain(..overflow);
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
    killer: Box<dyn ChildKiller + Send + Sync>,
    shared: Arc<Mutex<Shared>>,
}

struct Process {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
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
                killer: process.killer,
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
        let mut old = self.with(id, |s| s.killer.clone_killer())?;
        {
            let mut shared = shared.lock().unwrap();
            shared.process += 1;
            shared.exit = None;
        }
        let _ = old.kill();
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
        session.killer = process.killer;
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
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("cannot start {}", spec.program))?;
        // The master only sees EOF once every slave handle is closed.
        drop(pair.slave);

        let killer = child.clone_killer();
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
                let code = child.wait().ok().map(|s| s.exit_code());
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
            killer,
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

    /// Replaces the output sink without a snapshot: for a view gone off screen, which only wants to hear of the exit
    /// while the output fills the scrollback for its next `attach`.
    pub fn detach(&self, id: u32, sink: Sink) -> Result<()> {
        let shared = self.with(id, |s| s.shared.clone())?;
        shared.lock().unwrap().sink = Some(sink);
        Ok(())
    }

    pub fn write(&self, id: u32, data: &[u8]) -> Result<()> {
        let writer = self.with(id, |s| s.writer.clone())?;
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
        let mut session = self
            .map
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(|| anyhow!("unknown session {id}"))?;
        // Already exited is fine: dropping the session closes the PTY anyway.
        let _ = session.killer.kill();
        Ok(())
    }

    /// Ends the session's process but keeps the session, as when it exits by itself.
    pub fn terminate(&self, id: u32) -> Result<()> {
        let mut killer = self.with(id, |s| s.killer.clone_killer())?;
        // Already exited is fine.
        let _ = killer.kill();
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
    fn detached_output_waits_in_the_scrollback() {
        let sessions = Sessions::default();
        let id = sessions
            .spawn(spec(
                "/bin/sh",
                &["-c", "read x; printf 'after %s' \"$x\"; exit 4"],
            ))
            .unwrap();
        let (seen, data) = mpsc::channel();
        sessions
            .attach(id, Box::new(move |out| drop(seen.send(out))))
            .unwrap();
        let (tx, rx) = mpsc::channel();
        sessions
            .detach(
                id,
                Box::new(move |out| {
                    if let Output::Exit(code) = out {
                        let _ = tx.send(code);
                    }
                }),
            )
            .unwrap();
        sessions.write(id, b"hidden\n").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)), Ok(Some(4)));
        assert_eq!(data.try_iter().count(), 0, "the attached view heard on");
        let (text, code) = collect(&sessions, id);
        assert!(text.contains("after hidden"), "got {text:?}");
        assert_eq!(code, Some(4));
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
}
