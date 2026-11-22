//! A Claude Code tile as TermF starts it: the real Claude Code in a terminal, with the session's plugin, settings
//! and environment, its hooks reporting through the real `termf hook` and `termf statusline` to a stand-in for
//! TermF's socket that keeps the statuses and runs the automatic context reset as the app does.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use interprocess::local_socket::{GenericFilePath, ListenerOptions, prelude::*};
use serde_json::{Value, json};
use termf_core::context::{self, Limits, Next, Reset};
use termf_core::ipc::{self, Call, Request, Status};
use termf_term::{Sessions, SpawnSpec, shell_command, shell_quote};

const TOKEN: &str = "tile-token";

#[derive(Default)]
struct Tile {
    events: Vec<(String, Value)>,
    statuses: Vec<Status>,
    reset: Reset,
    limits: Option<Limits>,
    /// Lines TermF types once the agent is back at its prompt, with the reset step that asked for them.
    to_type: VecDeque<(u64, String)>,
    tokens_ok: bool,
}

/// What `Api::hook` does with a hook: the status, and each step of the reset.
fn hook(tile: &mut Tile, event: &str, payload: &Value, handoff: &Path) -> Value {
    let text = |key| payload.get(key).and_then(Value::as_str).unwrap_or_default();
    tile.events.push((event.into(), payload.clone()));
    let mut status = ipc::hook_status(event, payload);
    let reset = &mut tile.reset;
    let mut output = Value::Null;
    match event {
        "StatusLine" => reset.measure(&payload["context_window"]),
        "SessionStart" => {
            if let Some(text) = reset.session_start(text("source")) {
                output = context::session_start_output(&text, handoff);
                tile.to_type
                    .push_back((reset.step(), context::resume_prompt(handoff)));
            }
        }
        "UserPromptSubmit" => reset.prompt_submitted(),
        "StopFailure" => reset.interrupted(),
        "Stop" => {
            let limits = tile.limits.unwrap_or(Limits {
                on: false,
                reset_at: 70,
            });
            let busy = payload["background_tasks"]
                .as_array()
                .is_some_and(|t| !t.is_empty());
            let save = |t: &str| Ok(std::fs::write(handoff, t)?);
            match reset.stop(
                limits,
                busy,
                payload["stop_hook_active"] == true,
                text("last_assistant_message"),
                save,
            ) {
                Next::AskHandoff => {
                    output = context::stop_output(&context::handoff_request(reset.gauge.percent))
                }
                Next::Clear => tile.to_type.push_back((reset.step(), "/clear".into())),
                Next::Nothing => {}
            }
        }
        _ => {}
    }
    if let Some(s) = status {
        reset.activity(s);
        if reset.active() && s == Status::Idle {
            status = Some(Status::Working);
        }
    }
    if let Some(s) = status
        && tile.statuses.last() != Some(&s)
    {
        tile.statuses.push(s);
    }
    output
}

fn serve(socket: &str, tile: Arc<Mutex<Tile>>, handoff: PathBuf) {
    let listener = ListenerOptions::new()
        .name(socket.to_fs_name::<GenericFilePath>().unwrap())
        .try_overwrite(true)
        .create_sync()
        .unwrap();
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut conn = BufReader::new(conn);
            let mut line = String::new();
            if conn.read_line(&mut line).is_err() {
                continue;
            }
            let Ok(request) = serde_json::from_str::<Request>(&line) else {
                continue;
            };
            let mut tile = tile.lock().unwrap();
            let reply = match request.call {
                Call::Hook {
                    session: 1,
                    event,
                    payload,
                } if request.token == TOKEN => {
                    tile.tokens_ok = true;
                    hook(&mut tile, &event, &payload, &handoff)
                }
                _ => {
                    tile.tokens_ok = false;
                    Value::Null
                }
            };
            drop(tile);
            let _ = conn
                .get_mut()
                .write_all(format!("{}\n", json!({ "ok": reply })).as_bytes());
        }
    });
}

/// The screen as text, escape sequences left out.
fn screen(sessions: &Sessions) -> String {
    let raw = sessions.attach(1, Box::new(|_| {})).unwrap();
    let raw = String::from_utf8_lossy(&raw);
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
            } else if chars.peek() == Some(&']') {
                while chars.next().is_some_and(|c| c != '\x07') {}
            } else {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn until(what: &str, secs: u64, mut done: impl FnMut() -> bool, sessions: &Sessions) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while !done() {
        if Instant::now() > deadline {
            let s = screen(sessions);
            panic!(
                "timed out on {what}; screen:\n{}",
                &s[s.len().saturating_sub(3000)..]
            );
        }
        thread::sleep(Duration::from_millis(200));
    }
}

/// With the real Claude Code on Haiku, a few cents (an interactive session has no `--max-budget-usd`: five short
/// turns): a tile in a folder with a space and accents, started as TermF starts it. Statuses starting → idle →
/// working → waiting (a permission) → idle; `.env` refused by the session's deny rules, its value never on screen;
/// the automatic context reset past a 1 % threshold: handoff, `/clear`, the handoff given to the new
/// conversation, the work resumed, then no second reset; the tile closed ends Claude Code. A throwaway folder and
/// TermF data, your Claude Code account (the folder is trusted in its config, then purged).
/// `cargo test -p termf-cli --test tile -- --ignored real_tile --nocapture`
#[test]
#[ignore]
#[cfg(unix)]
fn real_tile() {
    let root = std::env::temp_dir().join(format!("termf-tile-réel {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (repo, data) = (root.join("dépôt acme"), root.join("data dir"));
    let dir = data.join("sessions/1");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let secret = format!("s3cr3t-{}", std::process::id());
    std::fs::write(repo.join(".env"), format!("API_KEY={secret}\n")).unwrap();
    let termf = Path::new(env!("CARGO_BIN_EXE_termf"));
    let home = dirs_home();
    termf_core::claude::write_plugin(&dir.join("plugin"), termf, false).unwrap();
    let settings =
        termf_core::claude::write_settings(&dir, termf, &repo, &home.join(".claude")).unwrap();
    // Whatever your own settings allow, this command asks (an `ask` rule wins over an `allow` one), as a run's
    // task settings are merged in.
    let mut all: Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    termf_core::claude::merge_settings(
        &mut all,
        &json!({"permissions": {"ask": ["Bash(touch *)"]}}),
    );
    std::fs::write(&settings, all.to_string()).unwrap();
    let command = format!(
        "claude --settings {} --plugin-dir {} --model haiku",
        shell_quote(&settings.to_string_lossy()),
        shell_quote(&dir.join("plugin").to_string_lossy())
    );
    let (command, install) = termf_core::claude::in_command(&command, shell_quote).unwrap();
    println!(
        "real_tile: Claude Code {:?} at {}",
        install.version,
        install.path.display()
    );

    let socket = root.join("t.sock").to_string_lossy().into_owned();
    let tile = Arc::new(Mutex::new(Tile::default()));
    let handoff = dir.join("handoff.md");
    serve(&socket, tile.clone(), handoff.clone());
    let sessions = Arc::new(Sessions::with_env(move |id| {
        vec![
            (ipc::ENV_SOCKET.into(), socket.clone()),
            (ipc::ENV_TOKEN.into(), TOKEN.into()),
            (ipc::ENV_SESSION.into(), id.to_string()),
        ]
    }));
    let (program, args) = shell_command(Some(&command));
    let id = sessions
        .spawn(SpawnSpec {
            title: "acme".into(),
            program,
            args,
            cwd: repo.clone(),
            agent: Some("claude".into()),
            cols: 120,
            rows: 40,
            env: Vec::new(),
            // What an agent's tile starts from (`agent_env`), the allowlist by default.
            base_env: Some(
                termf_core::agent_env::load(&root, None)
                    .keep(Some(termf_core::claude::Tool::Claude), std::env::vars_os()),
            ),
        })
        .unwrap();
    assert_eq!(id, 1);
    let state = |f: &dyn Fn(&Tile) -> bool| f(&tile.lock().unwrap());
    let events = |name: &str| {
        tile.lock()
            .unwrap()
            .events
            .iter()
            .filter(|(e, _)| e == name)
            .count()
    };

    // A folder Claude Code has never seen asks whether to trust it, "No, exit" selected (TermF never answers it).
    until(
        "the trust question or the prompt",
        60,
        || {
            let s = screen(&sessions);
            s.contains("Itrustthisfolder") || events("SessionStart") > 0
        },
        &sessions,
    );
    if events("SessionStart") == 0 {
        assert!(
            screen(&sessions).contains("❯No,exit"),
            "{}",
            screen(&sessions)
        );
        // An arrow sent before the dialog reads keys would arrive as Esc.
        thread::sleep(Duration::from_secs(1));
        sessions.write(id, b"\x1b[B").unwrap();
        thread::sleep(Duration::from_millis(500));
        sessions.write(id, b"\r").unwrap();
    }
    until("SessionStart", 60, || events("SessionStart") > 0, &sessions);

    // Types what TermF types for the reset, as `type_later` does.
    let over = Arc::new(AtomicBool::new(false));
    let typist = {
        let (tile, sessions, over) = (tile.clone(), sessions.clone(), over.clone());
        thread::spawn(move || {
            loop {
                let next = tile.lock().unwrap().to_type.pop_front();
                match next {
                    Some((step, line)) => {
                        thread::sleep(Duration::from_millis(1500));
                        let due = {
                            let mut t = tile.lock().unwrap();
                            t.reset.clear_due(step) || t.reset.resume_due(step)
                        };
                        if due {
                            sessions.type_line(1, &line).unwrap();
                        }
                    }
                    None => thread::sleep(Duration::from_millis(100)),
                }
                if over.load(Ordering::Relaxed) {
                    return;
                }
            }
        })
    };
    let stops = || events("Stop");

    sessions
        .type_line(id, "Use the Read tool on the file .env in this folder, once. Then reply with one word: done.")
        .unwrap();
    until("the first turn", 120, || stops() >= 1, &sessions);
    assert!(
        !screen(&sessions).contains(&secret),
        "the .env value reached the screen"
    );

    sessions
        .type_line(
            id,
            "Run this Bash command once, nothing else: touch made-by-claude.txt",
        )
        .unwrap();
    until(
        "the permission question",
        120,
        || state(&|t| t.statuses.last() == Some(&Status::Waiting)),
        &sessions,
    );
    thread::sleep(Duration::from_millis(500));
    sessions.write(id, b"\r").unwrap();
    until("the second turn", 120, || stops() >= 2, &sessions);
    assert!(
        repo.join("made-by-claude.txt").exists(),
        "{}",
        screen(&sessions)
    );

    tile.lock().unwrap().limits = Some(Limits {
        on: true,
        reset_at: 1,
    });
    sessions
        .type_line(id, "Remember the word pineapple. Reply with exactly: noted")
        .unwrap();
    until(
        "the reset and the resumed turn",
        240,
        || {
            state(&|t| {
                t.events
                    .iter()
                    .any(|(e, p)| e == "SessionStart" && p["source"] == "clear")
                    && t.reset.gauge.phase == context::Phase::Idle
                    && t.statuses.last() == Some(&Status::Idle)
            })
        },
        &sessions,
    );
    // The resumed turn ends below no threshold, yet never starts a second reset.
    thread::sleep(Duration::from_secs(5));
    let saved = std::fs::read_to_string(&handoff).unwrap_or_default();

    let started = Instant::now();
    sessions.kill(id).unwrap();
    thread::sleep(Duration::from_secs(3));
    let alive = Command::new("pgrep")
        .args(["-f", &dir.join("plugin").to_string_lossy()])
        .output()
        .unwrap();
    let t = tile.lock().unwrap();
    let names: Vec<&str> = t.events.iter().map(|(e, _)| e.as_str()).collect();
    println!(
        "real_tile: statuses {:?}\nreal_tile: events {names:?}\nreal_tile: gauge {:?}\nreal_tile: handoff {} chars",
        t.statuses,
        t.reset.gauge,
        saved.len()
    );
    let cost = t.events.iter().rev().find_map(|(e, p)| {
        (e == "StatusLine")
            .then(|| p["cost"]["total_cost_usd"].as_f64())
            .flatten()
    });
    println!("real_tile: cost ${cost:?} (the conversation after /clear)");
    assert!(t.tokens_ok, "every hook came with the session's token");
    assert_eq!(
        t.statuses.first(),
        Some(&Status::Idle),
        "starting, then idle at SessionStart"
    );
    assert!(t.statuses.contains(&Status::Working));
    assert!(t.statuses.contains(&Status::Waiting));
    assert!(
        !t.statuses.contains(&Status::Done),
        "a /clear is not the end"
    );
    let resets = t
        .events
        .iter()
        .filter(|(e, p)| e == "SessionStart" && p["source"] == "clear")
        .count();
    assert_eq!(resets, 1, "one reset, then none while it stays over");
    assert_eq!(t.reset.gauge.failure, None);
    assert!(!saved.trim().is_empty(), "the handoff is saved");
    assert!(
        String::from_utf8_lossy(&alive.stdout).trim().is_empty(),
        "Claude Code still runs {:?} after its tile closed",
        started.elapsed()
    );
    drop(t);
    over.store(true, Ordering::Relaxed);
    let _ = typist.join();
    let _ = Command::new(&install.path)
        .args(["purge", &repo.to_string_lossy(), "-y"])
        .output();
    let _ = std::fs::remove_dir_all(&root);
}

fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap())
}
