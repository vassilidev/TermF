//! Remote Control of the Claude Code tiles (ARCHITECTURE §6.6): the switch on each tile, the project's and the role's
//! default, "cut all", and the lock of a session that read production data.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use termf_core::remote::{self, Policy};
use termf_core::{Project, claude};
use termf_term::shell_quote;

use crate::{AppState, CmdResult, err};

/// What starts a tile's conversation again in a new process: its command and options, without those that picked
/// its conversation.
#[derive(Clone, Debug, Default)]
pub struct Relaunch {
    /// The `[agents]` command.
    pub command: String,
    /// Options before the conversation's, quoted (settings, plugin, system prompt).
    pub before: Vec<String>,
    /// Options after it, quoted (a run's folders and refused tools).
    pub after: Vec<String>,
    /// The last options, as given.
    pub trailing: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    /// The session's own `--settings` file, where Remote Control is disabled.
    pub settings: Option<PathBuf>,
}

impl Relaunch {
    /// The command line that resumes `conversation` (a fresh one when unknown), connected to Remote Control as `connect`.
    pub fn line(&self, conversation: Option<&str>, connect: Option<&str>) -> Result<String> {
        let mut extra = self.before.clone();
        if let Some(name) = connect {
            extra.push(format!("--remote-control {}", shell_quote(name)));
        }
        if let Some(id) = conversation {
            extra.push(format!("--resume {}", shell_quote(id)));
        }
        extra.extend(self.after.iter().cloned());
        let mut line = claude::after_program(&self.command, &extra.join(" "));
        for arg in &self.trailing {
            line = format!("{line} {}", shell_quote(arg));
        }
        Ok(claude::in_command(&line, shell_quote)?.0)
    }
}

/// A Claude Code tile's Remote Control. TermF follows what the session's hooks say, and the user's switch.
#[derive(Clone, Debug, Default)]
pub struct Tile {
    pub name: String,
    /// What the user asked for last.
    pub want: bool,
    /// The remote session's id, while the hooks say it is connected.
    pub bridge: Option<String>,
    /// The process was told to connect and has not said it is.
    pub asked: bool,
    /// The process cannot connect (`disableRemoteControl`): only another one turns it on.
    pub blocked: bool,
    /// The tile asks whether to turn it on.
    pub ask: bool,
    /// A change waits for the agent to be at its prompt.
    pub pending: bool,
    /// It read production data: off for good.
    pub locked: bool,
    /// The ticket run it works for.
    pub run: Option<i64>,
    pub relaunch: Relaunch,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// `/remote-control <name>` in the running process.
    Type,
    /// The conversation in a new process.
    Relaunch,
}

impl Tile {
    pub fn on(&self) -> bool {
        self.want || self.bridge.is_some()
    }

    pub fn state(&self) -> remote::State {
        remote::State {
            name: self.name.clone(),
            on: self.on(),
            url: self.bridge.as_deref().and_then(remote::link),
            pending: self.pending,
            ask: self.ask,
            locked: self.locked,
        }
    }

    /// What gets the session where the switch and the lock want it.
    pub fn next(&self) -> Step {
        let live = self.bridge.is_some() || self.asked;
        match (self.want && !self.locked, live) {
            (true, true) | (false, false) => Step::Nothing,
            (true, false) if self.blocked => Step::Relaunch,
            (true, false) => Step::Type,
            (false, true) => Step::Relaunch,
        }
    }

    /// What a hook says: the remote session while connected (`bridge`), and whether the session read production
    /// data. Returns whether the change cuts it.
    pub fn seen(&mut self, bridge: Option<&str>, locked: bool) -> bool {
        self.locked |= locked;
        match bridge {
            Some(id) => {
                self.bridge = Some(id.to_owned());
                self.asked = false;
                // Turned on by hand in the terminal: the switch follows, unless it is on its way off.
                self.want |= !self.pending;
            }
            // Ended by itself or by hand; one asked for may still come.
            None if !self.asked && !self.pending => {
                self.bridge = None;
                self.want = false;
            }
            None => self.bridge = None,
        }
        if self.locked {
            self.want = false;
            self.ask = false;
        }
        self.locked && self.next() == Step::Relaunch
    }
}

/// Disables Remote Control in a session's settings, or lifts that, keeping the rest of the file.
pub fn block(settings: &Path, blocked: bool) -> Result<()> {
    let mut all: Value = serde_json::from_str(&fs::read_to_string(settings)?)?;
    if let Some(map) = all.as_object_mut() {
        // Removed rather than false: `false` would override the user's own `true`.
        match blocked {
            true => map.insert("disableRemoteControl".into(), true.into()),
            false => map.remove("disableRemoteControl"),
        };
    }
    let tmp = settings.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(&all)?)?;
    fs::rename(&tmp, settings)?;
    Ok(())
}

/// The project a session works in (`given`, else the one that lists its folder) and that project's Remote Control
/// policy.
pub fn project_policy(
    config_dir: &Path,
    given: Option<&str>,
    cwd: &Path,
) -> (Option<String>, Policy) {
    let project = given
        .map(str::to_owned)
        .or_else(|| termf_memory::project_of(config_dir, cwd).ok().flatten());
    let policy = project
        .as_deref()
        .and_then(|id| {
            let projects = termf_core::load_projects(config_dir).ok()?;
            projects.into_iter().find(|p| p.id == id).map(|p| p.remote)
        })
        .unwrap_or_default();
    (project, policy)
}

/// The conversation `--resume` picks in a session's options.
pub fn resumed(args: &[String]) -> Option<&str> {
    let at = args.iter().position(|a| a == "--resume" || a == "-r")?;
    args.get(at + 1).map(String::as_str)
}

/// The "trifecta" rule (§12.4): the conversation, or the run it works for, read production data.
pub fn prod_read(data_dir: &Path, conversation: Option<&str>, run: Option<i64>) -> bool {
    conversation.is_some_and(|c| termf_core::taint::session(data_dir, c))
        || run.is_some_and(|r| termf_core::taint::run(data_dir, r))
}

/// The tile's switch: on at once when the agent is at its prompt, else at the end of its turn.
#[tauri::command]
pub async fn remote_set(app: AppHandle, id: u32, on: bool) -> CmdResult<()> {
    blocking(app, move |app| {
        app.state::<AppState>().api.remote_set(app, id, on, false)
    })
    .await
}

/// "Cut all": every tile off now, mid-turn or not. Returns how many were on.
#[tauri::command]
pub async fn remote_cut_all(app: AppHandle) -> CmdResult<usize> {
    blocking(app, |app| {
        Ok(app.state::<AppState>().api.remote_cut_all(app))
    })
    .await
}

#[tauri::command]
pub fn remote_policy_save(
    state: State<AppState>,
    project: String,
    policy: Policy,
) -> CmdResult<Vec<Project>> {
    remote::save(&state.config_dir, &project, &policy).map_err(err)?;
    termf_core::load_projects(&state.config_dir).map_err(err)
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle) -> Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile() -> Tile {
        Tile {
            name: "acme/api".into(),
            ..Tile::default()
        }
    }

    #[test]
    fn the_switch_types_the_command_or_starts_again() {
        let mut t = tile();
        assert_eq!(t.next(), Step::Nothing);
        t.want = true;
        assert_eq!(
            t.next(),
            Step::Type,
            "a running process connects by command"
        );
        t.asked = true;
        assert_eq!(t.next(), Step::Nothing, "asked: it connects");
        t.want = false;
        assert_eq!(t.next(), Step::Relaunch, "off takes a new process");
        let mut blocked = Tile {
            want: true,
            blocked: true,
            ..tile()
        };
        assert_eq!(
            blocked.next(),
            Step::Relaunch,
            "it cannot connect by command"
        );
        blocked.want = false;
        assert_eq!(blocked.next(), Step::Nothing);
    }

    #[test]
    fn follows_the_hooks() {
        let mut t = tile();
        assert!(!t.seen(Some("session_01"), false));
        assert!(t.want && t.state().url.as_deref() == Some("https://claude.ai/code/session_01"));
        assert!(!t.seen(None, false));
        assert!(!t.on(), "ended in the terminal: off");
        t.want = true;
        t.asked = true;
        assert!(!t.seen(None, false));
        assert!(t.want, "still connecting");
        let mut bad = tile();
        bad.seen(Some("../x"), false);
        assert_eq!(
            bad.state().url,
            None,
            "an id that is not plain gives no link"
        );
    }

    #[test]
    fn production_data_cuts_and_locks() {
        let mut t = tile();
        t.seen(Some("session_01"), false);
        assert!(t.seen(Some("session_01"), true), "connected: cut");
        assert!(!t.want && t.state().locked);
        t.want = true;
        assert_eq!(t.next(), Step::Relaunch, "never back on");
        let mut idle = Tile {
            ask: true,
            ..tile()
        };
        assert!(!idle.seen(None, true), "nothing to cut");
        assert!(!idle.ask && idle.locked);
        assert!(!idle.seen(None, false), "a lock stays");
        assert!(idle.locked);
    }

    #[test]
    fn the_conversation_or_the_run_that_read_prod_locks_it() {
        let data = std::env::temp_dir().join(format!("termf-rc-taint-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        assert!(!prod_read(&data, Some("c0ffee"), Some(7)));
        termf_core::taint::mark_session(&data, "c0ffee").unwrap();
        assert!(prod_read(&data, Some("c0ffee"), None));
        assert!(!prod_read(&data, Some("other"), Some(7)));
        termf_core::taint::mark_run(&data, 7).unwrap();
        assert!(prod_read(&data, Some("other"), Some(7)));
        assert!(!prod_read(&data, None, None));
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn disables_remote_control_in_the_session_settings_only() {
        let dir = std::env::temp_dir().join(format!("termf-rc-block-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        fs::write(&file, r#"{"permissions":{"deny":["Read(.env)"]}}"#).unwrap();
        block(&file, true).unwrap();
        let read = || serde_json::from_str::<Value>(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(read()["disableRemoteControl"], true);
        assert_eq!(read()["permissions"]["deny"][0], "Read(.env)");
        block(&file, false).unwrap();
        assert!(read().get("disableRemoteControl").is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    /// A fake `claude` prints its arguments: the tile starts again in the same conversation, connected or not.
    #[cfg(unix)]
    #[test]
    fn starts_again_in_the_same_conversation() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::mpsc;
        use std::time::Duration;
        use termf_term::{Output, Sessions, SpawnSpec};

        let dir = std::env::temp_dir().join(format!("termf-rc-relaunch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let claude = dir.join("claude");
        fs::write(
            &claude,
            "#!/bin/sh\n[ \"$1\" = --version ] && { echo '2.1.330 (Claude Code)'; exit 0; }\nprintf '<%s>' \"$@\"\n",
        )
        .unwrap();
        fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
        let relaunch = Relaunch {
            command: claude.to_string_lossy().into_owned(),
            before: vec!["--settings '/s/settings.json'".into()],
            after: vec!["--disallowedTools 'Bash(git push:*)'".into()],
            trailing: remote::strip_conversation(&[
                "--resume".into(),
                "old".into(),
                "--model".into(),
                "opus".into(),
            ]),
            cwd: dir.clone(),
            ..Relaunch::default()
        };
        let sessions = Sessions::default();
        let spec = |line: String| SpawnSpec {
            title: "acme".into(),
            program: "/bin/sh".into(),
            args: vec!["-c".into(), line],
            cwd: dir.clone(),
            agent: None,
            cols: 200,
            rows: 24,
            env: Vec::new(),
        };
        let id = sessions.spawn(spec("exec cat".into())).unwrap();
        let (tx, rx) = mpsc::channel();
        sessions
            .attach(
                id,
                Box::new(move |out| {
                    if let Output::Data(d) = out {
                        let _ = tx.send(d);
                    }
                }),
            )
            .unwrap();
        let run = |line: String| {
            sessions.respawn_with(id, |_| Ok(spec(line))).unwrap();
            let mut seen = String::new();
            while !seen.contains("<opus>") {
                let d = rx.recv_timeout(Duration::from_secs(10)).expect("no output");
                seen.push_str(&String::from_utf8_lossy(&d));
            }
            seen
        };
        let on = relaunch
            .line(Some("c0ffee"), Some("acme/ACME-1/back"))
            .unwrap();
        let seen = run(on);
        assert!(
            seen.contains("<--settings></s/settings.json><--remote-control><acme/ACME-1/back><--resume><c0ffee><--disallowedTools><Bash(git push:*)><--model><opus>"),
            "{seen}"
        );
        let off = relaunch.line(Some("c0ffee"), None).unwrap();
        let seen = run(off);
        assert!(
            seen.contains("<--settings></s/settings.json><--resume><c0ffee><--disallowedTools>"),
            "{seen}"
        );
        assert!(!seen.contains("remote-control"), "{seen}");
        fs::remove_dir_all(dir).unwrap();
    }
}
