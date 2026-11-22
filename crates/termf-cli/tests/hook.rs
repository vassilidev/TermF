//! `termf hook` and `termf statusline` against a stand-in for TermF's socket.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};

use interprocess::local_socket::{GenericFilePath, ListenerOptions, prelude::*};
use serde_json::{Value, json};

/// Listens like TermF, answers one request with `reply`, and returns the request.
fn fake_termf(name: &str, reply: Value) -> (String, JoinHandle<Value>) {
    let socket = if cfg!(windows) {
        format!(r"\\.\pipe\termf-test-{name}-{}", std::process::id())
    } else {
        std::env::temp_dir()
            .join(format!("termf-test-{name}-{}.sock", std::process::id()))
            .to_string_lossy()
            .into_owned()
    };
    let listener = ListenerOptions::new()
        .name(socket.as_str().to_fs_name::<GenericFilePath>().unwrap())
        .try_overwrite(true)
        .create_sync()
        .unwrap();
    let request = thread::spawn(move || {
        let mut conn = BufReader::new(listener.incoming().next().unwrap().unwrap());
        let mut line = String::new();
        conn.read_line(&mut line).unwrap();
        let answer = format!("{}\n", json!({ "ok": reply }));
        conn.get_mut().write_all(answer.as_bytes()).unwrap();
        serde_json::from_str(&line).unwrap()
    });
    (socket, request)
}

fn termf(args: &[&str], socket: &str, stdin: &str) -> String {
    termf_with(args, socket, stdin, None)
}

/// `bridge`: the Remote Control session Claude Code gives the hook's environment while connected.
fn termf_with(args: &[&str], socket: &str, stdin: &str, bridge: Option<&str>) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_termf"));
    command.env_remove("CLAUDE_CODE_BRIDGE_SESSION_ID");
    if let Some(bridge) = bridge {
        command.env("CLAUDE_CODE_BRIDGE_SESSION_ID", bridge);
    }
    let mut child = command
        .args(args)
        .env("TERMF_SOCKET", socket)
        .env("TERMF_TOKEN", "session-token")
        .env("TERMF_SESSION_ID", "7")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn prints_what_termf_answers_a_hook() {
    let output = json!({"hookSpecificOutput": {"hookEventName": "Stop", "additionalContext": "write the handoff"}});
    let (socket, request) = fake_termf("stop", output.clone());
    let stop =
        json!({"session_id": "abc", "hook_event_name": "Stop", "last_assistant_message": "done"});
    let printed = termf(&["hook", "Stop"], &socket, &stop.to_string());
    assert_eq!(
        serde_json::from_str::<Value>(&printed).unwrap(),
        output,
        "{printed}"
    );
    assert_eq!(
        request.join().unwrap(),
        json!({"token": "session-token", "method": "hook", "params": {"session": 7, "event": "Stop", "payload": stop}})
    );

    let (socket, request) = fake_termf("start", Value::Null);
    let start =
        json!({"session_id": "abc", "hook_event_name": "SessionStart", "source": "startup"});
    assert_eq!(
        termf(&["hook", "SessionStart"], &socket, &start.to_string()),
        ""
    );
    assert_eq!(request.join().unwrap()["params"]["event"], "SessionStart");
}

#[test]
fn tells_termf_the_remote_control_session() {
    let (socket, request) = fake_termf("bridge", Value::Null);
    let stop = json!({"session_id": "abc", "hook_event_name": "Stop"});
    termf_with(
        &["hook", "Stop"],
        &socket,
        &stop.to_string(),
        Some("session_01Ab"),
    );
    let payload = request.join().unwrap()["params"]["payload"].clone();
    assert_eq!(payload["termf_bridge"], "session_01Ab");
    assert_eq!(payload["session_id"], "abc");
}

#[test]
fn reports_the_status_line_and_prints_nothing_of_its_own() {
    let (socket, request) = fake_termf("status", Value::Null);
    let input = json!({"session_id": "abc", "context_window": {"used_percentage": 42}});
    assert_eq!(termf(&["statusline"], &socket, &input.to_string()), "");
    let request = request.join().unwrap();
    assert_eq!(request["params"]["event"], "StatusLine");
    assert_eq!(request["params"]["payload"], input);
}

#[cfg(unix)]
#[test]
fn runs_your_status_line_with_the_same_input() {
    let (socket, request) = fake_termf("chained", Value::Null);
    let chained = std::env::temp_dir().join(format!("termf-test-chained-{}", std::process::id()));
    std::fs::write(&chained, "printf 'mine:'; cat").unwrap();
    let input = r#"{"session_id":"abc"}"#;
    let printed = termf(&["statusline", chained.to_str().unwrap()], &socket, input);
    assert_eq!(printed, format!("mine:{input}"));
    request.join().unwrap();
    std::fs::remove_file(chained).unwrap();
}

#[cfg(unix)]
#[test]
fn codex_notify_reports_the_end_of_a_turn_then_runs_yours() {
    let (socket, request) = fake_termf("codex", Value::Null);
    let dir = std::env::temp_dir().join(format!("termf-test-notify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let yours = dir.join("notify.sh");
    let got = dir.join("got");
    std::fs::write(
        &yours,
        format!("#!/bin/sh\nprintf '%s|' \"$@\" > '{}'\n", got.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&yours, std::fs::Permissions::from_mode(0o755)).unwrap();
    let payload = r#"{"type":"agent-turn-complete","thread-id":"t1","turn-id":"u1","cwd":"/src","last-assistant-message":"Done."}"#;
    let printed = termf(
        &[
            "hook",
            "agent-turn-complete",
            yours.to_str().unwrap(),
            "--quiet",
            payload,
        ],
        &socket,
        "",
    );
    assert_eq!(printed, "", "Codex reads nothing back");
    let request = request.join().unwrap();
    assert_eq!(request["params"]["event"], "agent-turn-complete");
    assert_eq!(request["params"]["payload"]["thread-id"], "t1");
    assert_eq!(
        std::fs::read_to_string(&got).unwrap(),
        format!("--quiet|{payload}|"),
        "your notify gets the same JSON"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn gemini_session_start_gets_the_librarys_instructions() {
    let (socket, request) = fake_termf("gemini", Value::Null);
    let prompt = std::env::temp_dir().join(format!("termf-test-prompt-{}", std::process::id()));
    std::fs::write(&prompt, "## Role: docs").unwrap();
    let start = json!({"session_id": "g1", "hook_event_name": "SessionStart", "source": "startup"});
    let printed = termf(
        &["hook", "SessionStart", prompt.to_str().unwrap()],
        &socket,
        &start.to_string(),
    );
    assert_eq!(
        serde_json::from_str::<Value>(&printed).unwrap(),
        json!({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": "## Role: docs"}})
    );
    assert_eq!(request.join().unwrap()["params"]["payload"], start);
    std::fs::remove_file(prompt).unwrap();
}
