//! A Claude Code tile started the way `api::launch` starts one (ARCHITECTURE §12.2): the environment allowlist, a
//! login shell, the real `termf agent-env`, then the hooks plugin and settings. Its hooks must still reach TermF,
//! or the tile has no status.
#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use interprocess::local_socket::{GenericFilePath, ListenerOptions, prelude::*};
use serde_json::{Value, json};
use termf_core::agent_env::{self, AgentEnv};
use termf_core::claude::{self, Tool};
use termf_core::loops::{Kind, Loops, now_ms};
use termf_term::{Sessions, SpawnSpec, shell_command, shell_quote};

const TOKEN: &str = "tile-token";

fn termf() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_termf"))
}

/// Stands in for TermF's local API: keeps every request, answers each with nothing.
fn fake_termf(socket: &Path) -> Arc<Mutex<Vec<Value>>> {
    let listener = ListenerOptions::new()
        .name(socket.to_fs_name::<GenericFilePath>().unwrap())
        .try_overwrite(true)
        .create_sync()
        .unwrap();
    let got = Arc::new(Mutex::new(Vec::new()));
    let kept = got.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut conn = BufReader::new(conn);
            let mut line = String::new();
            if conn.read_line(&mut line).is_ok()
                && let Ok(request) = serde_json::from_str(&line)
            {
                kept.lock().unwrap().push(request);
                let _ = conn.get_mut().write_all(b"{\"ok\":null}\n");
            }
        }
    });
    got
}

/// The registry's own variables, as `Api::sessions` gives them.
fn sessions(socket: &Path) -> Sessions {
    let socket = socket.to_string_lossy().into_owned();
    Sessions::with_env(move |id| {
        vec![
            ("TERMF_SOCKET".into(), socket.clone()),
            ("TERMF_TOKEN".into(), TOKEN.into()),
            ("TERMF_SESSION_ID".into(), id.to_string()),
            ("TERMF_BIN".into(), termf().to_string_lossy().into_owned()),
        ]
    })
}

/// Starts `program` (and `more` options) in a tile, with `app_env` as TermF's own environment.
fn tile(
    sessions: &Sessions,
    program: &str,
    more: &str,
    cwd: &Path,
    dir: &Path,
    app_env: Vec<(OsString, OsString)>,
) -> u32 {
    let plugin = dir.join("plugin");
    claude::write_plugin(&plugin, termf(), false).unwrap();
    let settings = claude::write_settings(dir, termf(), cwd, &dir.join("no-claude-dir")).unwrap();
    let extra = format!(
        "--settings {} --plugin-dir {}",
        shell_quote(&settings.to_string_lossy()),
        shell_quote(&plugin.to_string_lossy())
    );
    let command = claude::after_program(&format!("{program} {more}"), &extra);
    let command = claude::in_command(&command, shell_quote).unwrap().0;
    let allowlist = AgentEnv::default();
    let (wrapped, spec) = allowlist
        .wrap(
            &command,
            Some(Tool::Claude),
            Some(termf()),
            &[],
            shell_quote,
        )
        .expect("the command goes through termf agent-env");
    let (program, args) = shell_command(Some(&wrapped));
    sessions
        .spawn(SpawnSpec {
            title: "claude".into(),
            program,
            args,
            cwd: cwd.to_path_buf(),
            agent: None,
            cols: 120,
            rows: 40,
            env: vec![(agent_env::SPEC.into(), spec)],
            base_env: Some(allowlist.keep(Some(Tool::Claude), app_env)),
        })
        .unwrap()
}

/// TermF's environment: an app opened from the Dock has none of the variables of a Claude Code session running
/// these tests.
fn app_env() -> Vec<(OsString, OsString)> {
    std::env::vars_os()
        .filter(|(k, _)| !k.to_string_lossy().starts_with("CLAUDE"))
        .collect()
}

/// `what` says what was there when it times out.
fn wait_for(what: impl Fn() -> String, timeout: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while !done() {
        assert!(Instant::now() < deadline, "timed out: {}", what());
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn events(got: &Mutex<Vec<Value>>, session: u32) -> Vec<String> {
    got.lock()
        .unwrap()
        .iter()
        .filter(|r| {
            r["method"] == "hook" && r["token"] == TOKEN && r["params"]["session"] == session
        })
        .filter_map(|r| r["params"]["event"].as_str().map(str::to_owned))
        .collect()
}

/// Shows the tile's screen to the test, says yes to trusting the throwaway folder, and waits for the session.
fn started(sessions: &Sessions, id: u32, got: &Mutex<Vec<Value>>) -> Arc<Mutex<String>> {
    let screen = Arc::new(Mutex::new(String::new()));
    let seen = screen.clone();
    sessions
        .attach(
            id,
            1,
            Box::new(move |out| {
                if let termf_term::Output::Data(d) = out {
                    seen.lock().unwrap().push_str(&String::from_utf8_lossy(&d));
                }
            }),
        )
        .unwrap();
    let mut trusted = false;
    wait_for(
        || report(got, id, &screen),
        Duration::from_secs(60),
        || {
            // A new folder: Claude Code asks whether to trust it ("No, exit" first) before it loads any hook.
            if !trusted && screen.lock().unwrap().contains("trust") {
                trusted = true;
                std::thread::sleep(Duration::from_secs(2));
                let _ = sessions.write(id, b"\x1b[B");
                std::thread::sleep(Duration::from_millis(300));
                let _ = sessions.write(id, b"\r");
            }
            events(got, id).iter().any(|e| e == "SessionStart")
        },
    );
    std::thread::sleep(Duration::from_secs(2));
    screen
}

fn report(got: &Mutex<Vec<Value>>, id: u32, screen: &Mutex<String>) -> String {
    let text = screen.lock().unwrap().clone();
    let tail = text.chars().rev().take(4000).collect::<String>();
    format!(
        "hooks {:?}, screen:\n{}",
        events(got, id),
        tail.chars().rev().collect::<String>()
    )
}

/// What TermF reads from each hook of `session`.
fn payloads(got: &Mutex<Vec<Value>>, session: u32) -> Vec<(String, Value)> {
    got.lock()
        .unwrap()
        .iter()
        .filter(|r| {
            r["method"] == "hook" && r["token"] == TOKEN && r["params"]["session"] == session
        })
        .filter_map(|r| {
            Some((
                r["params"]["event"].as_str()?.to_owned(),
                r["params"]["payload"].clone(),
            ))
        })
        .collect()
}

#[test]
fn a_claude_tile_keeps_its_hooks_plugin_and_reaches_termf() {
    let root = std::env::temp_dir().join(format!("termf-tile-hooks-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (home, repo, dir) = (root.join("home"), root.join("repo"), root.join("session"));
    for d in [&home, &repo, &dir] {
        fs::create_dir_all(d).unwrap();
    }
    // What a tile's agent got: its words and its environment, for its hooks.
    let fake = root.join("claude");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\n[ \"$1\" = --version ] && {{ echo '2.1.330 (Claude Code)'; exit 0; }}\n\
             env > '{0}/env'\nprintf '%s\\n' \"$@\" > '{0}/args.tmp' && mv '{0}/args.tmp' '{0}/args'\n",
            root.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let got = fake_termf(&root.join("termf.sock"));
    let sessions = sessions(&root.join("termf.sock"));
    let mut app_env: Vec<_> = app_env().into_iter().filter(|(k, _)| k != "HOME").collect();
    app_env.push(("HOME".into(), home.clone().into()));
    app_env.push(("STRIPE_SECRET_KEY".into(), "sk_live_from_the_app".into()));
    let id = tile(&sessions, &fake.to_string_lossy(), "", &repo, &dir, app_env);

    let started = || "the agent never started".to_owned();
    wait_for(started, Duration::from_secs(20), || {
        root.join("args").exists()
    });
    let args: Vec<String> = fs::read_to_string(root.join("args"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let after = |flag: &str| {
        let i = args.iter().position(|a| a == flag);
        PathBuf::from(&args[i.unwrap_or_else(|| panic!("no {flag}: {args:?}")) + 1])
    };
    let (plugin, settings) = (after("--plugin-dir"), after("--settings"));
    assert!(settings.is_file(), "{args:?}");
    let env: Vec<(String, String)> = fs::read_to_string(root.join("env"))
        .unwrap()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    let named = |name: &str| env.iter().any(|(k, _)| k == name);
    assert!(
        !named("STRIPE_SECRET_KEY") && !named(agent_env::SPEC),
        "{env:?}"
    );
    for name in [
        "TERMF_SOCKET",
        "TERMF_TOKEN",
        "TERMF_SESSION_ID",
        "TERMF_BIN",
    ] {
        assert!(named(name), "{name} lost: {env:?}");
    }

    // Claude Code runs each hook of the plugin with its own environment.
    let hooks: Value =
        serde_json::from_str(&fs::read_to_string(plugin.join("hooks/hooks.json")).unwrap())
            .unwrap();
    for event in ["SessionStart", "UserPromptSubmit", "Stop"] {
        let hook = &hooks["hooks"][event][0]["hooks"][0];
        let args: Vec<&str> = hook["args"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let mut child = Command::new(hook["command"].as_str().unwrap())
            .args(args)
            .env_clear()
            .envs(env.iter().cloned())
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let payload = json!({"session_id": "abc", "hook_event_name": event});
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success(), "{event}");
    }
    assert_eq!(
        events(&got, id),
        ["SessionStart", "UserPromptSubmit", "Stop"]
    );
    let _ = fs::remove_dir_all(&root);
}

/// The real Claude Code (haiku, one short answer), in a throwaway folder, with TermF's data throwaway:
/// `TERMF_TEST_REAL_CLAUDE=1 cargo test -p termf-cli --test tile_hooks -- --ignored --nocapture`.
#[test]
#[ignore]
fn the_real_claude_code_reports_through_its_hooks() {
    assert!(std::env::var_os("TERMF_TEST_REAL_CLAUDE").is_some());
    let root = std::env::temp_dir().join(format!("termf-tile-real-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (repo, dir) = (root.join("repo"), root.join("session"));
    for d in [&repo, &dir] {
        fs::create_dir_all(d).unwrap();
    }
    let got = fake_termf(&root.join("termf.sock"));
    let sessions = sessions(&root.join("termf.sock"));
    let id = tile(
        &sessions,
        "claude",
        "--model haiku --max-budget-usd 0.05",
        &repo,
        &dir,
        app_env(),
    );
    let _cleanup = Cleanup {
        sessions: &sessions,
        id,
        // The path Claude Code keeps it under (`/private/var/…` on macOS).
        repo: fs::canonicalize(&repo).unwrap(),
        root: root.clone(),
    };
    let screen = started(&sessions, id, &got);
    let what = || report(&got, id, &screen);
    let has = |e: &str| events(&got, id).iter().any(|x| x == e);
    sessions
        .type_line(id, "Reply with the single word ok")
        .unwrap();
    wait_for(what, Duration::from_secs(120), || has("Stop"));
    sessions.type_line(id, "/exit").unwrap();
    wait_for(what, Duration::from_secs(30), || has("SessionEnd"));
    let events = events(&got, id);
    eprintln!("hooks received: {events:?}");
    let at = |e: &str| {
        events
            .iter()
            .position(|x| x == e)
            .unwrap_or_else(|| panic!("no {e}: {events:?}"))
    };
    assert!(at("SessionStart") < at("UserPromptSubmit") && at("UserPromptSubmit") < at("Stop"));
}

/// The real Claude Code (haiku) runs `/loop 1m …` in a tile: what its hooks tell TermF shows the loop, until the
/// session ends. `TERMF_TEST_REAL_CLAUDE=1 cargo test -p termf-cli --test tile_hooks -- --ignored --nocapture`.
#[test]
#[ignore]
fn the_real_claude_code_shows_its_loop_to_termf() {
    assert!(std::env::var_os("TERMF_TEST_REAL_CLAUDE").is_some());
    let root = std::env::temp_dir().join(format!("termf-tile-loop-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (repo, dir) = (root.join("repo"), root.join("session"));
    for d in [&repo, &dir] {
        fs::create_dir_all(d).unwrap();
    }
    let got = fake_termf(&root.join("termf.sock"));
    let sessions = sessions(&root.join("termf.sock"));
    let id = tile(
        &sessions,
        "claude",
        "--model haiku --max-budget-usd 0.10",
        &repo,
        &dir,
        app_env(),
    );
    let _cleanup = Cleanup {
        sessions: &sessions,
        id,
        repo: fs::canonicalize(&repo).unwrap(),
        root: root.clone(),
    };
    let screen = started(&sessions, id, &got);
    let what = || report(&got, id, &screen);
    let has = |e: &str| events(&got, id).iter().any(|x| x == e);
    sessions
        .type_line(id, "/loop 1m reply with the single word hi")
        .unwrap();
    wait_for(what, Duration::from_secs(120), || has("Stop"));
    let mut loops = Loops::default();
    for (event, payload) in payloads(&got, id) {
        loops.hook(&event, &payload, now_ms());
    }
    let active = loops.active(now_ms());
    eprintln!("loops: {active:?}");
    assert!(
        active.iter().any(|l| l.kind == Kind::Cron
            && l.from_loop
            && l.recurring
            && matches!(l.cron.as_deref(), Some("* * * * *" | "*/1 * * * *"))),
        "{active:?}\n{}",
        what()
    );
    sessions.type_line(id, "/exit").unwrap();
    let mut answered = false;
    wait_for(what, Duration::from_secs(30), || {
        // With a task scheduled, Claude Code asks first; "Exit and stop tasks" comes first.
        if !answered && screen.lock().unwrap().contains("Exit and stop tasks") {
            answered = true;
            std::thread::sleep(Duration::from_millis(500));
            let _ = sessions.write(id, b"\r");
        }
        has("SessionEnd")
    });
    for (event, payload) in payloads(&got, id) {
        loops.hook(&event, &payload, now_ms());
    }
    assert!(loops.active(now_ms()).is_empty());
}

/// Leaves nothing in your Claude Code data, even when the test fails: `claude purge` takes the throwaway folder's
/// trust in `~/.claude.json`, its transcripts and its prompts.
struct Cleanup<'a> {
    sessions: &'a Sessions,
    id: u32,
    repo: PathBuf,
    root: PathBuf,
}

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = self.sessions.terminate(self.id);
        // Claude Code writes `~/.claude.json` again as it exits: purge once it is gone.
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline
            && self
                .sessions
                .list()
                .iter()
                .any(|s| s.id == self.id && s.running)
        {
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_secs(1));
        if let Ok(claude) = claude::binary("claude") {
            let _ = Command::new(claude.path)
                .args(["purge", "-y"])
                .arg(&self.repo)
                .stdout(Stdio::null())
                .status();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
