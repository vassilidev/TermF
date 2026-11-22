//! `termf mcp` against a stand-in for TermF's socket: the MCP protocol on stdio, and what reaches TermF.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use interprocess::local_socket::{GenericFilePath, ListenerOptions, prelude::*};
use serde_json::{Value, json};

const TOKEN: &str = "cap-0123456789abcdef";

/// Listens like TermF: answers each request with what `reply` makes of it, and keeps the requests.
fn fake_termf(name: &str, reply: fn(&Value) -> Value) -> (String, Arc<Mutex<Vec<Value>>>) {
    let socket = if cfg!(windows) {
        format!(r"\\.\pipe\termf-mcp-{name}-{}", std::process::id())
    } else {
        std::env::temp_dir()
            .join(format!("termf-mcp-{name}-{}.sock", std::process::id()))
            .to_string_lossy()
            .into_owned()
    };
    let listener = ListenerOptions::new()
        .name(socket.as_str().to_fs_name::<GenericFilePath>().unwrap())
        .try_overwrite(true)
        .create_sync()
        .unwrap();
    let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
    let requests = seen.clone();
    thread::spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            let mut conn = BufReader::new(conn);
            let mut line = String::new();
            if conn.read_line(&mut line).unwrap_or(0) == 0 {
                continue;
            }
            let request: Value = serde_json::from_str(&line).unwrap();
            let answer = format!("{}\n", reply(&request));
            requests.lock().unwrap().push(request);
            let _ = conn.get_mut().write_all(answer.as_bytes());
        }
    });
    (socket, seen)
}

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// Every line it printed.
    said: Vec<String>,
}

impl Server {
    fn start(socket: &str, capability: bool) -> Server {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_termf"));
        cmd.arg("mcp")
            .env("TERMF_SOCKET", socket)
            .env_remove("TERMF_MCP_TOKEN")
            .env_remove("TERMF_MCP_SESSION")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        if capability {
            cmd.env("TERMF_MCP_TOKEN", TOKEN)
                .env("TERMF_MCP_SESSION", "7");
        }
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server {
            child,
            stdin,
            stdout,
            said: Vec::new(),
        }
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn ask(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        self.said.push(line.clone());
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["id"], json!(id), "{reply}");
        reply
    }

    fn call(&mut self, id: i64, name: &str, arguments: Value) -> Value {
        self.ask(
            id,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        )
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"].as_str().unwrap()
}

#[test]
fn speaks_mcp_and_calls_termf_with_its_own_token_only() {
    let (socket, requests) = fake_termf("protocol", |request| {
        match request["params"]["tool"]["tool"].as_str() {
            Some("team_runs") => {
                json!({"ok": [{"run": 12, "state": "waiting", "waits": [{"step": "approve-plan"}]}]})
            }
            _ => json!({"error": "run 99 is not in this project"}),
        }
    });
    let mut server = Server::start(&socket, true);
    let init = server.ask(
        1,
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "claude-code", "version": "2.1.330"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "termf");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(
        init["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("<termf-notices>")
    );
    server.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    assert_eq!(
        server.ask(2, "ping", json!({}))["result"],
        json!({}),
        "no reply to a notification"
    );

    let list = server.ask(3, "tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 12, "{names:?}");
    for never in ["deliver", "close", "push", "vault", "secret"] {
        assert!(!names.iter().any(|n| n.contains(never)), "{never}");
    }

    let runs = server.call(4, "team_runs", json!({}));
    assert_eq!(runs["result"]["isError"], false, "{runs}");
    let got: Value = serde_json::from_str(text(&runs)).unwrap();
    assert_eq!(got[0]["run"], 12);
    assert_eq!(
        requests.lock().unwrap()[0],
        json!({"token": TOKEN, "method": "mcp", "params": {"session": 7, "tool": {"tool": "team_runs"}}})
    );

    let refused = server.call(5, "approve", json!({"run": 99, "step": "approve-plan"}));
    assert_eq!(refused["result"]["isError"], true);
    assert_eq!(text(&refused), "run 99 is not in this project");

    for (id, name) in [(6, "deliver"), (7, "vault_reveal"), (8, "close")] {
        let reply = server.call(id, name, json!({"run": 12}));
        assert_eq!(reply["error"]["code"], -32602, "{reply}");
    }
    let bad = server.call(9, "approve", json!({"run": 12, "step": "x", "force": true}));
    assert_eq!(bad["result"]["isError"], true);
    assert!(text(&bad).contains("invalid arguments"), "{bad}");
    assert_eq!(
        server.ask(10, "resources/list", json!({}))["error"]["code"],
        -32601
    );
    assert_eq!(
        requests.lock().unwrap().len(),
        2,
        "refused calls never reach TermF"
    );
    assert!(
        server.said.iter().all(|l| !l.contains(TOKEN)),
        "the token is never in a reply"
    );
}

#[test]
fn serves_nothing_without_termf_s_capability() {
    let (socket, requests) = fake_termf("none", |_| json!({"ok": []}));
    let mut server = Server::start(&socket, false);
    let reply = server.call(1, "team_runs", json!({}));
    assert_eq!(reply["result"]["isError"], true);
    assert!(
        text(&reply).contains("TermF did not start this server"),
        "{reply}"
    );
    assert!(requests.lock().unwrap().is_empty());
}

/// The CTO conversation with the real Claude Code, Haiku and this `termf mcp`, against a stand-in for TermF: it
/// reads the team with a tool, and an action waits for the user. A few cents; a throwaway folder and config,
/// your Claude Code account. `cargo test -p termf-cli --test mcp -- --ignored real_cto --nocapture`
#[cfg(unix)]
#[test]
#[ignore]
fn real_cto() {
    use std::time::{Duration, Instant};
    use termf_agents::{AgentEvent, Agents, SessionSpec, State};

    let (socket, requests) = fake_termf("real", |request| {
        let summary = json!({"run": 12, "ticket": "ACME-42", "title": "Login page", "state": "waiting",
            "atWork": [], "waits": [{"step": "approve-plan", "state": "waiting", "note": "the plan waits for your approval"}]});
        match request["params"]["tool"]["tool"].as_str() {
            Some("team_runs") => json!({"ok": [summary]}),
            Some("waiting") => {
                json!({"ok": {"terminals": [], "conversations": [], "teamRuns": [summary]}})
            }
            Some("team_run") => json!({"ok": summary}),
            Some("project") => {
                json!({"ok": {"id": "acme", "name": "Acme", "repos": [], "roles": []}})
            }
            _ => json!({"ok": []}),
        }
    });
    let root = std::env::temp_dir().join(format!("termf-cto-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (dir, config) = (root.join("repo"), root.join("config"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("termf.toml"), "[agents]\nclaude = \"claude\"\n").unwrap();
    let agents = Agents::build(config, root.join("data"), Vec::new(), |_| {});
    agents.serve_mcp(
        move |id| {
            Ok(json!({"type": "stdio", "command": env!("CARGO_BIN_EXE_termf"), "args": ["mcp"],
                      "env": {"TERMF_SOCKET": socket, "TERMF_MCP_TOKEN": TOKEN, "TERMF_MCP_SESSION": id.to_string()}}))
        },
        |_| {},
    );
    let spec = SessionSpec {
        cwd: dir,
        role: Some("cto".into()),
        model: Some("haiku".into()),
        effort: Some("low".into()),
        budget: Some(0.40),
        mcp: true,
        ..SessionSpec::default()
    };
    let id = agents.create(spec).unwrap().id;
    let events = || agents.events(id, 0).unwrap();
    let wait = |done: &dyn Fn() -> bool| {
        let deadline = Instant::now() + Duration::from_secs(180);
        while !done() {
            assert!(Instant::now() < deadline, "{:#?}", events());
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let turns = || {
        events()
            .iter()
            .filter(|e| matches!(e.event, AgentEvent::TurnEnd { .. }))
            .count()
    };
    let state = || agents.session(id).unwrap().state;

    agents.send(id, "Où en est l'équipe ?").unwrap();
    wait(&|| turns() == 1 && state() != State::Working);
    let used: Vec<String> = events()
        .iter()
        .filter_map(|e| match &e.event {
            AgentEvent::ToolUse { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    println!("tools: {used:?}");
    assert!(
        used.iter().any(|n| n.starts_with("mcp__termf__")),
        "{:#?}",
        events()
    );
    assert!(!requests.lock().unwrap().is_empty());

    agents
        .send(
            id,
            "Approuve l'étape approve-plan du run 12, avec l'outil approve.",
        )
        .unwrap();
    wait(&|| state() == State::Waiting || turns() == 2);
    let asked = events().iter().rev().find_map(|e| match &e.event {
        AgentEvent::Permission { request, tool, .. } => Some((request.clone(), tool.clone())),
        _ => None,
    });
    let (request, tool) = asked.expect("the action waits for the user");
    assert_eq!(tool, "mcp__termf__approve");
    agents
        .answer(id, &request, false, Some("Not now."))
        .unwrap();
    wait(&|| turns() == 2 && state() != State::Working);
    let approvals = requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r["params"]["tool"]["tool"] == "approve")
        .count();
    assert_eq!(approvals, 0, "refused: never reached TermF");
    println!("cost: ${:.4}", agents.session(id).unwrap().cost);
    let _ = agents.stop(id);
    let _ = std::fs::remove_dir_all(&root);
}
