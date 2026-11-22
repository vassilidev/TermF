//! `termf guard` as the hook of a headless session that asks for it (a team's step): what Claude Code is given,
//! run the way Claude Code runs a `PreToolUse` hook.
#![cfg(unix)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use termf_agents::{AgentEvent, Agents, SessionSpec, State};

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("termf-guard-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("repo")).unwrap();
    std::fs::create_dir_all(root.join("config")).unwrap();
    root
}

/// Runs a hook as Claude Code does: the program and its arguments, no shell, the tool call as JSON on stdin; exit
/// code 2 refuses the call and stderr says why.
fn run_hook(hook: &Value, command: &str) -> (i32, String) {
    let args: Vec<&str> = hook["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    let mut child = Command::new(hook["command"].as_str().unwrap())
        .args(args)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let call = json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": command}});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(call.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_guarded_session_gets_the_hook_that_refuses_what_a_ticket_run_refuses() {
    let root = scratch("hook");
    let log = root.join("args");
    let fake = root.join("claude");
    std::fs::write(
        &fake,
        format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", log.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        root.join("config/termf.toml"),
        format!("[agents]\nclaude = \"{}\"\n", fake.display()),
    )
    .unwrap();
    let agents = Agents::build(root.join("config"), root.join("data"), Vec::new(), |_| {});
    agents.serve_guard(env!("CARGO_BIN_EXE_termf").into());
    let spec = SessionSpec {
        cwd: root.join("repo"),
        guard: true,
        ..SessionSpec::default()
    };
    let _ = agents.start(spec, Some("hello"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !log.is_file() {
        assert!(Instant::now() < deadline, "the agent never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(100));
    let args = std::fs::read_to_string(&log).unwrap();
    let settings = args
        .lines()
        .skip_while(|a| *a != "--settings")
        .nth(1)
        .expect("--settings");
    let settings: Value = serde_json::from_str(settings).unwrap();
    let pre = &settings["hooks"]["PreToolUse"][0];
    assert_eq!(pre["matcher"], "Bash");
    let hook = &pre["hooks"][0];
    for refused in [
        "git -C . push origin HEAD",
        "cd /tmp && git push",
        "gh pr create --fill",
        "gh api -X POST repos/acme/api/issues",
        "gh issue close 3",
        "gh workflow run ci.yml",
        "termf deliver 12",
        "sh -c 'git push'",
    ] {
        let (code, why) = run_hook(hook, refused);
        assert_eq!(code, 2, "{refused} let through");
        assert!(why.contains("is refused"), "{refused}: {why}");
    }
    for allowed in [
        "git status",
        "git log --oneline",
        "gh api repos/acme/api/pulls",
        "gh run view 4",
    ] {
        assert_eq!(run_hook(hook, allowed).0, 0, "{allowed}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "alex")
        .env("GIT_AUTHOR_EMAIL", "alex@example.invalid")
        .env("GIT_COMMITTER_NAME", "alex")
        .env("GIT_COMMITTER_EMAIL", "alex@example.invalid")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// With the real Claude Code on Haiku, a few cents: a session that asks for the guard, whose permissions let any
/// git command run (as a user's settings may), told to push to a local remote; the hook refuses, the remote stays
/// empty. A throwaway folder and config, your Claude Code account.
/// `cargo test -p termf-cli --test guard -- --ignored real_guard --nocapture`
#[test]
#[ignore]
fn real_guard() {
    let root = scratch("real");
    let (repo, remote) = (root.join("repo"), root.join("remote.git"));
    git(&root, &["init", "-q", "--bare", "remote.git"]);
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "# Acme\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    git(
        &repo,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    std::fs::write(
        root.join("config/termf.toml"),
        "[agents]\nclaude = \"claude\"\n",
    )
    .unwrap();
    let agents = Agents::build(root.join("config"), root.join("data"), Vec::new(), |_| {});
    agents.serve_guard(env!("CARGO_BIN_EXE_termf").into());
    let spec = SessionSpec {
        cwd: repo.clone(),
        model: Some("haiku".into()),
        effort: Some("low".into()),
        budget: Some(0.20),
        permission_mode: Some("acceptEdits".into()),
        allowed_tools: vec!["Bash(git:*)".into()],
        // What a team's step has: a form such as `git -C . push` gets past it.
        disallowed_tools: vec!["Bash(git push:*)".into()],
        guard: true,
        ..SessionSpec::default()
    };
    let id = agents
        .start(
            spec,
            Some("Run exactly this Bash command, once, and nothing else: git -C . push origin HEAD:refs/heads/main\nThen say in one sentence what happened."),
        )
        .unwrap()
        .id;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let session = agents.session(id).unwrap();
        let events = agents.events(id, 0).unwrap();
        if session.state == State::Waiting {
            // Whatever asks is allowed: only the hook stands in the way.
            let request = events.iter().rev().find_map(|e| match &e.event {
                AgentEvent::Permission { request, .. } => Some(request.clone()),
                _ => None,
            });
            let _ = agents.answer(id, &request.unwrap(), true, None);
        }
        let ended = events
            .iter()
            .any(|e| matches!(e.event, AgentEvent::TurnEnd { .. }));
        if ended && session.state != State::Working {
            break;
        }
        assert!(Instant::now() < deadline, "{events:#?}");
        std::thread::sleep(Duration::from_millis(300));
    }
    let events = agents.events(id, 0).unwrap();
    let results: Vec<String> = events
        .iter()
        .filter_map(|e| match &e.event {
            AgentEvent::ToolResult { output, .. } => Some(output.clone()),
            _ => None,
        })
        .collect();
    println!(
        "real_guard: ${:.4}, tool results: {results:?}",
        agents.session(id).unwrap().cost
    );
    assert!(
        git(&remote, &["for-each-ref"]).is_empty(),
        "nothing reached the remote"
    );
    assert!(
        results.iter().any(|r| r.contains("is refused")),
        "the hook said why: {events:#?}"
    );
    let _ = agents.stop(id);
    let _ = Command::new("claude")
        .args(["purge", &repo.to_string_lossy(), "-y"])
        .output();
    let _ = std::fs::remove_dir_all(&root);
}
