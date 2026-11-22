use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

use super::*;

/// A stand-in for Claude Code that reads stream-json and answers like 2.1.330, by the words in each message.
/// It logs its arguments, folder, stdin, and when it starts and ends, next to itself.
const FAKE: &str = r#"
echo "start $$" >> "$log.events"
trap 'echo "exit $$" >> "$log.events"' EXIT
{ echo "--- $$"; printf '%s\n' "$@"; } >> "$log.args"
pwd > "$log.cwd"
prev=
for a in "$@"; do
  if [ "$prev" = --session-id ] || [ "$prev" = --resume ]; then sid=$a; fi
  prev=$a
done
spent=0
say() { printf '%s\n' "$1"; }
result() {
  spent=$((spent + 1))
  say "{\"type\":\"result\",\"subtype\":\"$1\",\"total_cost_usd\":0.0$spent,\"usage\":{\"input_tokens\":5,\"output_tokens\":7,\"cache_read_input_tokens\":9000,\"cache_creation_input_tokens\":1000},\"modelUsage\":{\"claude-haiku-5-5\":{\"contextWindow\":200000}}$2,\"session_id\":\"$sid\"}"
}
answer() {
  say "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-haiku-5-5\",\"content\":[{\"type\":\"text\",\"text\":\"$1\"}],\"usage\":{\"input_tokens\":5,\"cache_creation_input_tokens\":1000,\"cache_read_input_tokens\":9000,\"output_tokens\":7}},\"parent_tool_use_id\":null}"
}
delta() {
  say "{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"$1\"}},\"parent_tool_use_id\":null}"
}
tool_result() {
  say "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"$1\",\"content\":\"$2\",\"is_error\":$3}]},\"parent_tool_use_id\":null}"
}
say 'Welcome: a shell profile says hello'
say '{"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup"}'
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log.stdin"
  case "$line" in
    *'"type":"user"'*) ;;
    *) continue ;;
  esac
  say "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$sid\",\"model\":\"claude-haiku-5-5\",\"permissionMode\":\"default\"}"
  say '{"type":"brand_new_kind","later":true}'
  case "$line" in
    *hello*) delta Hel; delta lo; answer 'Hello, alex.'; result success ;;
    *tool*)
      say '{"type":"assistant","message":{"model":"claude-haiku-5-5","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]},"parent_tool_use_id":null}'
      tool_result t1 "$(printf '%5000s' '' | tr ' ' x)" false
      answer 'Listed.'; result success ;;
    *permission*)
      say '{"type":"assistant","message":{"model":"claude-haiku-5-5","content":[{"type":"tool_use","id":"t2","name":"Write","input":{"file_path":"a.txt","content":"hi"}}]},"parent_tool_use_id":null}'
      say '{"type":"control_request","request_id":"req-1","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"a.txt","content":"hi"},"description":"Write a.txt","tool_use_id":"t2"}}'
      IFS= read -r reply; printf '%s\n' "$reply" >> "$log.stdin"
      case "$reply" in
        *'"behavior":"allow"'*) tool_result t2 'File created' false; answer 'Written.' ;;
        *) tool_result t2 'Refused' true; answer 'Not written.' ;;
      esac
      result success ;;
    *slow*)
      delta 'Counting: 1, 2, '
      IFS= read -r next; printf '%s\n' "$next" >> "$log.stdin"
      say '{"type":"control_response","response":{"subtype":"success","request_id":"x","response":{}}}'
      say '{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"parent_tool_use_id":null}'
      result error_during_execution ',"is_error":true,"terminal_reason":"aborted_streaming","errors":["[ede_diagnostic] result_type=user"]' ;;
    *structured*) answer '{"ok":true}'; result success ',"structured_output":{"ok":true}' ;;
    *budget*) result error_max_budget_usd ',"is_error":true,"terminal_reason":"budget_exhausted","errors":["Reached maximum budget ($0.05)"]' ;;
    *crash*) printf '{"type":"assistant","mess'; echo 'boom: connection lost' >&2; exit 3 ;;
    *) answer 'OK.'; result success ;;
  esac
done
"#;

/// A throwaway config, data folder, repository and home, with the fake as the `claude` agent.
struct Env {
    root: PathBuf,
    dir: PathBuf,
    fake: PathBuf,
    agents: Agents,
    updates: Arc<Mutex<Vec<Update>>>,
}

impl Env {
    fn new(name: &str) -> Env {
        let root = std::env::temp_dir().join(format!("termf-agents-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (dir, config, home) = (root.join("repo"), root.join("config"), root.join("home"));
        for d in [&dir, &config, &home] {
            fs::create_dir_all(d).unwrap();
        }
        let fake = root.join("bin/claude");
        fs::create_dir_all(fake.parent().unwrap()).unwrap();
        fs::write(
            &fake,
            format!("#!/bin/sh\nlog='{}'\n{FAKE}", fake.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            config.join("termf.toml"),
            format!(
                "[agents]\nshell = \"zsh\"\nclaude = \"{}\"\n",
                fake.display()
            ),
        )
        .unwrap();
        let updates: Arc<Mutex<Vec<Update>>> = Arc::default();
        let seen = updates.clone();
        // Never the user's own shell profile.
        let env = vec![("HOME".into(), home.to_string_lossy().into_owned())];
        let agents = Agents::build(config, root.join("data"), env, move |u| {
            seen.lock().unwrap().push(u)
        });
        Env {
            root,
            dir,
            fake,
            agents,
            updates,
        }
    }

    fn spec(&self) -> SessionSpec {
        SessionSpec {
            cwd: self.dir.clone(),
            model: Some("haiku".into()),
            effort: Some(" low ".into()),
            permission_mode: Some("manual".into()),
            ..SessionSpec::default()
        }
    }

    fn session(&self, id: i64) -> AgentSession {
        let all = self.agents.sessions().unwrap();
        all.into_iter().find(|s| s.id == id).unwrap()
    }

    fn events(&self, id: i64) -> Vec<AgentEvent> {
        let events = self.agents.events(id, 0).unwrap();
        events.into_iter().map(|e| e.event).collect()
    }

    fn kinds(&self, id: i64) -> Vec<String> {
        self.events(id)
            .iter()
            .map(|e| {
                serde_json::to_value(e).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }

    fn turns(&self, id: i64) -> usize {
        self.kinds(id).iter().filter(|k| *k == "turn_end").count()
    }

    /// Waits until `turns` turns ended and the session is in `state`.
    fn settle(&self, id: i64, turns: usize, state: State) -> AgentSession {
        self.until(&format!("{turns} turns, {state:?}"), || {
            self.turns(id) == turns && self.session(id).state == state
        });
        self.session(id)
    }

    fn until(&self, what: &str, done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {:#?}",
                self.agents.sessions().unwrap()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn log(&self, ext: &str) -> String {
        fs::read_to_string(format!("{}.{ext}", self.fake.display())).unwrap_or_default()
    }

    /// Each process's arguments.
    fn args(&self) -> Vec<Vec<String>> {
        let log = self.log("args");
        let runs = log.split("--- ").skip(1);
        runs.map(|r| r.lines().skip(1).map(str::to_owned).collect())
            .collect()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for session in self.agents.sessions().unwrap_or_default() {
            let _ = self.agents.stop(session.id);
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn turn_end(events: &[AgentEvent]) -> (Outcome, f64, f64, Option<String>, Option<Gauge>) {
    match events
        .iter()
        .rev()
        .find(|e| matches!(e, AgentEvent::TurnEnd { .. }))
    {
        Some(AgentEvent::TurnEnd {
            outcome,
            cost,
            total,
            error,
            context,
            ..
        }) => (
            *outcome,
            round(*cost),
            round(*total),
            error.clone(),
            context.clone(),
        ),
        _ => panic!("no turn ended: {events:#?}"),
    }
}

fn round(dollars: f64) -> f64 {
    (dollars * 10_000.0).round() / 10_000.0
}

#[test]
fn runs_several_turns_on_one_process() {
    let env = Env::new("turns");
    let session = env.agents.start(env.spec(), Some("say hello")).unwrap();
    let id = session.id;
    assert_eq!(session.spec.title, "repo");
    assert_eq!(
        session.spec.agent.as_deref(),
        Some("claude"),
        "the first agent running Claude Code"
    );
    assert_eq!(session.spec.effort.as_deref(), Some("low"), "trimmed");
    let session = env.settle(id, 1, State::Idle);
    assert_eq!(
        env.kinds(id),
        ["started", "user", "init", "text", "turn_end"],
        "deltas, unknown lines and noise are not stored"
    );
    let (outcome, cost, total, _, context) = turn_end(&env.events(id));
    assert_eq!((outcome, cost, total), (Outcome::Done, 0.01, 0.01));
    let context = context.unwrap();
    assert_eq!(
        (context.tokens, context.window, context.percent),
        (Some(10_005), Some(200_000), Some(5))
    );
    assert_eq!(session.context, Some(context));
    assert_eq!(round(session.cost), 0.01);
    let deltas: Vec<(i64, String)> = env
        .updates
        .lock()
        .unwrap()
        .iter()
        .filter_map(|u| match u {
            Update::Event(StoredEvent {
                id,
                event: AgentEvent::Delta { text },
                ..
            }) => Some((*id, text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, [(0, "Hel".into()), (0, "lo".into())]);

    env.agents.send(id, "use a tool").unwrap();
    let session = env.settle(id, 2, State::Idle);
    let events = env.events(id);
    assert!(events.contains(&AgentEvent::ToolUse {
        id: "t1".into(),
        name: "Bash".into(),
        input: json!({"command": "ls"}),
        parent: None,
    }));
    let output = events.iter().find_map(|e| match e {
        AgentEvent::ToolResult {
            output,
            truncated: true,
            is_error: false,
            ..
        } => Some(output.clone()),
        _ => None,
    });
    assert_eq!(output.unwrap().len(), TOOL_OUTPUT);
    let (_, cost, total, _, _) = turn_end(&events);
    assert_eq!(
        (cost, total),
        (0.01, 0.02),
        "a turn costs what the process adds"
    );
    assert_eq!(round(session.cost), 0.02);

    let args = env.args();
    assert_eq!(args.len(), 1, "one process for both turns");
    assert_eq!(
        args[0][..10],
        [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-prompt-tool",
            "stdio",
            "--session-id"
        ]
    );
    assert_eq!(args[0][10], session.session_id);
    assert_eq!(
        args[0][11..],
        [
            "--model",
            "haiku",
            "--effort",
            "low",
            "--permission-mode",
            "manual"
        ]
    );
    assert_eq!(
        fs::canonicalize(env.log("cwd").trim()).unwrap(),
        fs::canonicalize(&env.dir).unwrap()
    );
    let stdin = env.log("stdin");
    assert!(
        stdin
            .lines()
            .all(|l| serde_json::from_str::<serde_json::Value>(l).is_ok())
    );
    assert_eq!(stdin.lines().count(), 2);

    let jsonl = env.root.join(format!("data/agents/{id}/events.jsonl"));
    let lines = fs::read_to_string(jsonl).unwrap();
    assert_eq!(lines.lines().count(), env.events(id).len());
    let first: StoredEvent = serde_json::from_str(lines.lines().next().unwrap()).unwrap();
    assert_eq!(first.event, AgentEvent::Started { resumed: false });
    assert_eq!(
        env.agents.events(id, first.id + 1).unwrap().len(),
        env.events(id).len() - 2
    );
}

#[test]
fn plays_a_role_and_answers_with_its_schema() {
    let env = Env::new("role");
    let spec = SessionSpec {
        cwd: env.dir.clone(),
        role: Some("orchestrator".into()),
        model: Some("haiku".into()),
        disallowed_tools: vec!["Bash(git push:*)".into()],
        rules: [("scope-strict".to_owned(), "warn".to_owned())].into(),
        json_schema: Some(json!({"type": "object"})),
        ..SessionSpec::default()
    };
    let session = env.agents.start(spec, Some("structured please")).unwrap();
    let spec = &session.spec;
    assert_eq!(spec.role.as_deref(), Some("orchestrator"));
    assert_eq!(
        (spec.model.as_deref(), spec.effort.as_deref()),
        (Some("haiku"), Some("high")),
        "the spec's model, the role's effort"
    );
    assert_eq!(
        spec.disallowed_tools,
        ["Bash(git push:*)", "Edit", "Write", "NotebookEdit"]
    );
    env.settle(session.id, 1, State::Idle);
    let structured = env.events(session.id).into_iter().find_map(|e| match e {
        AgentEvent::TurnEnd { structured, .. } => structured,
        _ => None,
    });
    assert_eq!(structured, Some(json!({"ok": true})));
    let args = env.args().remove(0);
    let after = |flag: &str| -> Vec<String> {
        args.windows(2)
            .filter(|w| w[0] == flag)
            .map(|w| w[1].clone())
            .collect()
    };
    assert_eq!(after("--disallowedTools").len(), 4);
    assert_eq!(after("--json-schema"), [r#"{"type":"object"}"#]);
    let prompt = fs::read_to_string(&after("--append-system-prompt-file")[0]).unwrap();
    assert!(
        prompt.starts_with("## Role: orchestrator\n\nYou are the tech lead"),
        "the former name of lead: {prompt}"
    );
    assert!(prompt.contains("## scope-strict"), "{prompt}");

    let unknown = SessionSpec {
        role: Some("nobody".into()),
        ..env.spec()
    };
    let error = env.agents.start(unknown, None).unwrap_err().to_string();
    assert!(error.contains("no role named nobody"), "{error}");
}

#[test]
fn asks_permission_and_interrupts() {
    let env = Env::new("permission");
    let id = env
        .agents
        .start(env.spec(), Some("please ask permission"))
        .unwrap()
        .id;
    env.until("a permission request", || {
        env.session(id).state == State::Waiting
    });
    assert!(env.events(id).contains(&AgentEvent::Permission {
        request: "req-1".into(),
        tool: "Write".into(),
        input: json!({"file_path": "a.txt", "content": "hi"}),
        description: Some("Write a.txt".into()),
    }));
    assert!(env.agents.answer(id, "req-9", true, None).is_err());
    assert!(env.agents.send(id, "more").is_err(), "not while it waits");
    env.agents.answer(id, "req-1", true, None).unwrap();
    env.settle(id, 1, State::Idle);
    let stdin = env.log("stdin");
    let reply: serde_json::Value = serde_json::from_str(stdin.lines().nth(1).unwrap()).unwrap();
    assert_eq!(
        reply["response"]["response"],
        json!({"behavior": "allow", "updatedInput": {"file_path": "a.txt", "content": "hi"}})
    );
    assert!(env.events(id).iter().any(|e| matches!(
        e,
        AgentEvent::ToolResult { output, is_error: false, .. } if output == "File created"
    )));

    env.agents.send(id, "ask permission again").unwrap();
    env.until("the second request", || {
        env.session(id).state == State::Waiting
    });
    env.agents
        .answer(id, "req-1", false, Some(" not now "))
        .unwrap();
    env.settle(id, 2, State::Idle);
    let reply: serde_json::Value =
        serde_json::from_str(env.log("stdin").lines().last().unwrap()).unwrap();
    assert_eq!(
        reply["response"]["response"],
        json!({"behavior": "deny", "message": "not now"})
    );
    assert!(env.events(id).contains(&AgentEvent::PermissionAnswer {
        request: "req-1".into(),
        allow: false,
        message: Some("not now".into()),
    }));

    assert!(env.agents.interrupt(id).is_err(), "nothing to interrupt");
    env.agents.send(id, "count slowly").unwrap();
    env.until("the first words", || {
        env.updates.lock().unwrap().iter().any(|u| {
            matches!(u, Update::Event(StoredEvent { event: AgentEvent::Delta { text }, .. }) if text.starts_with("Counting"))
        })
    });
    assert!(env.agents.send(id, "hurry").is_err(), "one turn at a time");
    assert!(env.agents.takeover(id).is_err(), "not during a turn");
    env.agents.interrupt(id).unwrap();
    env.settle(id, 3, State::Idle);
    let (outcome, _, total, error, _) = turn_end(&env.events(id));
    assert_eq!((outcome, total, error), (Outcome::Interrupted, 0.03, None));
    assert!(
        env.log("stdin")
            .lines()
            .last()
            .unwrap()
            .contains("\"interrupt\"")
    );

    env.agents.send(id, "count slowly once more").unwrap();
    env.until("words again", || {
        env.updates.lock().unwrap().iter().filter(|u| {
            matches!(u, Update::Event(StoredEvent { event: AgentEvent::Delta { text }, .. }) if text.starts_with("Counting"))
        }).count() == 2
    });
    let stopped = env.agents.stop(id).unwrap();
    assert_eq!(
        stopped.state,
        State::Stopped,
        "stopping interrupts the turn first"
    );
    assert_eq!(
        env.log("events")
            .lines()
            .filter(|l| l.starts_with("exit"))
            .count(),
        1
    );
}

#[test]
fn recovers_from_a_broken_stream() {
    let env = Env::new("crash");
    let id = env.agents.start(env.spec(), None).unwrap().id;
    env.until("the process", || env.log("events").contains("start"));
    assert_eq!(env.session(id).state, State::Idle);
    env.agents.stop(id).unwrap();
    assert_eq!(env.session(id).state, State::Stopped);
    assert!(
        env.agents.takeover(id).is_err(),
        "no conversation to take over yet"
    );

    env.agents.send(id, "crash now").unwrap();
    env.until("the error", || env.session(id).state == State::Error);
    let events = env.events(id);
    assert!(
        events.contains(&AgentEvent::Exited { code: Some(3) }),
        "{events:#?}"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::Error { message } if message.contains("boom: connection lost")
    )));

    env.agents.send(id, "hello again").unwrap();
    let session = env.settle(id, 1, State::Idle);
    let args = env.args();
    assert_eq!(args.len(), 3);
    let flag = |run: &Vec<String>| run[9].clone();
    assert_eq!(flag(&args[0]), "--session-id");
    assert_eq!(
        flag(&args[1]),
        "--session-id",
        "never resumed: it took no message"
    );
    assert_eq!(
        flag(&args[2]),
        "--resume",
        "the crashed turn's conversation exists"
    );
    assert_eq!(args[2][10], session.session_id);

    // A new launch of the app: no process survived the last one.
    let again = Agents::build(
        env.root.join("config"),
        env.root.join("data"),
        Vec::new(),
        |_| {},
    );
    let listed = again.sessions().unwrap();
    assert_eq!(listed[0].state, State::Stopped);
}

#[test]
fn keeps_to_the_budget() {
    let env = Env::new("budget");
    let spec = SessionSpec {
        budget: Some(0.025),
        ..env.spec()
    };
    let id = env.agents.start(spec.clone(), Some("hello")).unwrap().id;
    env.settle(id, 1, State::Idle);
    env.agents.stop(id).unwrap();
    env.agents.send(id, "hello").unwrap();
    env.settle(id, 2, State::Idle);
    let args = env.args();
    let budget = |run: &Vec<String>| {
        let at = run.iter().position(|a| a == "--max-budget-usd").unwrap();
        run[at + 1].clone()
    };
    assert_eq!(budget(&args[0]), "0.0250");
    assert_eq!(budget(&args[1]), "0.0150", "what the first process left");
    env.agents.send(id, "hello").unwrap();
    let spent = env.settle(id, 3, State::Budget);
    assert_eq!(round(spent.cost), 0.03);
    let refused = env.agents.send(id, "hello").unwrap_err().to_string();
    assert!(refused.contains("budget"), "{refused}");

    let id = env
        .agents
        .start(
            SessionSpec {
                budget: Some(1.0),
                ..spec
            },
            Some("over budget"),
        )
        .unwrap()
        .id;
    env.settle(id, 1, State::Budget);
    let (outcome, _, _, error, _) = turn_end(&env.events(id));
    assert_eq!(outcome, Outcome::Budget);
    assert_eq!(error.as_deref(), Some("Reached maximum budget ($0.05)"));
}

#[test]
fn takes_over_and_hands_back_without_two_processes() {
    let env = Env::new("takeover");
    let id = env.agents.start(env.spec(), Some("hello")).unwrap().id;
    let session = env.settle(id, 1, State::Idle);
    let takeover = env.agents.takeover(id).unwrap();
    assert_eq!(
        takeover,
        Takeover {
            agent: "claude".into(),
            cwd: env.dir.clone(),
            title: "repo".into(),
            args: [
                "--resume",
                &session.session_id,
                "--model",
                "haiku",
                "--effort",
                "low",
                "--permission-mode",
                "manual"
            ]
            .map(String::from)
            .to_vec(),
        }
    );
    assert!(
        env.log("events").contains("exit"),
        "the headless process ended first"
    );
    assert_eq!(env.session(id).state, State::Interactive);
    // Until the terminal is attached, nothing may start a process next to the one it is about to run.
    let busy = |e: anyhow::Error| e.to_string().contains("being stopped, taken over");
    assert!(busy(
        env.agents
            .handback(id, |_| panic!("no terminal"))
            .unwrap_err()
    ));
    assert!(busy(env.agents.send(id, "hi").unwrap_err()));
    assert!(busy(env.agents.stop(id).unwrap_err()));
    assert!(busy(env.agents.takeover(id).unwrap_err()));

    let note = |text: &str| {
        let file = format!("{}.events", env.fake.display());
        let mut log = fs::read_to_string(&file).unwrap();
        log.push_str(text);
        log.push('\n');
        fs::write(&file, log).unwrap();
    };
    note("tile 7 open");
    env.agents.attach_tile(id, Some(7)).unwrap();
    assert_eq!(env.session(id).tile, Some(7));
    let refused = env.agents.send(id, "hi").unwrap_err().to_string();
    assert!(refused.contains("hand it back"), "{refused}");
    assert!(env.agents.stop(id).is_err());
    assert!(env.agents.takeover(id).is_err());
    let (agents, note) = (&env.agents, &note);
    let session = std::thread::scope(|s| {
        agents
            .handback(id, |tile| {
                note(&format!("tile {tile} asked to close"));
                s.spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    note(&format!("tile {tile} closed"));
                    agents.tile_exited(tile);
                });
            })
            .unwrap()
    });
    assert_eq!((session.state, session.tile), (State::Idle, None));
    let starts = || {
        env.log("events")
            .lines()
            .filter(|l| l.starts_with("start"))
            .count()
    };
    env.until("the new process", || starts() == 2);
    let log: Vec<String> = env
        .log("events")
        .lines()
        .map(|l| match l.split_once(' ') {
            Some((word @ ("start" | "exit"), _)) => word.to_owned(),
            _ => l.to_owned(),
        })
        .collect();
    assert_eq!(
        log,
        [
            "start",
            "exit",
            "tile 7 open",
            "tile 7 asked to close",
            "tile 7 closed",
            "start"
        ],
    );
    let args = env.args();
    assert_eq!(
        args[1][9..11],
        ["--resume".to_owned(), session.session_id.clone()]
    );
    let kinds = env.kinds(id);
    assert!(
        kinds.ends_with(&["exited", "taken_over", "handed_back", "started"].map(String::from)),
        "{kinds:?}"
    );

    // A terminal that does not close keeps the conversation there.
    env.agents.takeover(id).unwrap();
    env.agents.attach_tile(id, Some(8)).unwrap();
    let refused = env.agents.handback(id, |_| {}).unwrap_err().to_string();
    assert!(refused.contains("did not close"), "{refused}");
    assert_eq!((env.session(id).state, starts()), (State::Interactive, 2));
    env.agents.tile_exited(8);
    assert_eq!(env.session(id).tile, None);
    env.agents
        .handback(id, |_| panic!("already closed"))
        .unwrap();
    env.until("a third process", || starts() == 3);

    // A terminal that could not open.
    env.agents.takeover(id).unwrap();
    env.agents.attach_tile(id, None).unwrap();
    assert_eq!(env.session(id).state, State::Stopped);
    env.agents.send(id, "hello").unwrap();
    env.settle(id, 2, State::Idle);
}

fn running(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A process holding `args` on its command line, in its own group, as an agent's.
fn process(args: &[&str]) -> std::process::Child {
    use std::os::unix::process::CommandExt;
    std::process::Command::new("sh")
        // Two commands, so the shell does not exec `sleep` and keeps its own command line.
        .args(["-c", "sleep 60; exit 0", "sh"])
        .args(args)
        .process_group(0)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
}

fn end(mut process: std::process::Child) {
    crate::agents::kill_group(process.id());
    let _ = process.wait();
}

#[test]
fn never_leaves_a_process_behind() {
    let env = Env::new("orphans");
    let id = env.agents.start(env.spec(), Some("hello")).unwrap().id;
    let session = env.settle(id, 1, State::Idle);
    let pid: u32 = env
        .log("events")
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    assert!(running(pid));
    // The app quits.
    env.agents.shutdown();
    env.until("the process killed", || !running(pid));
    env.until("its end seen", || env.session(id).state != State::Idle);

    // A process the last launch left behind, and a pid the system gave since to another program.
    let mut orphan = process(&[&session.session_id]);
    let mut stranger = process(&["unrelated"]);
    let other = env.agents.start(env.spec(), None).unwrap().id;
    env.agents.stop(other).unwrap();
    // And the live process of an app that crashed, as TermF started it.
    let crashed = env.agents.start(env.spec(), Some("hello")).unwrap().id;
    env.settle(crashed, 1, State::Idle);
    let last = env.log("events").lines().last().unwrap().to_owned();
    let crashed_pid: u32 = last.strip_prefix("start ").unwrap().parse().unwrap();
    let store = crate::store::Store::open(&env.root.join("data")).unwrap();
    store.set_pid(id, Some(orphan.id())).unwrap();
    store.set_pid(other, Some(stranger.id())).unwrap();
    // The app quit in the middle of a turn.
    let mut mid_turn = store.session(id).unwrap();
    mid_turn.state = State::Working;
    store.update(&mut mid_turn).unwrap();
    let again = Agents::build(
        env.root.join("config"),
        env.root.join("data"),
        Vec::new(),
        |_| {},
    );
    env.until("the crashed app's process killed", || !running(crashed_pid));
    let killed = orphan.wait().unwrap();
    assert!(!killed.success(), "the orphan is killed");
    assert!(
        stranger.try_wait().unwrap().is_none(),
        "the stranger is left alone"
    );
    assert_eq!(
        (store.pid(id).unwrap(), store.pid(other).unwrap()),
        (None, None)
    );
    let listed = again.sessions().unwrap();
    let state = listed.iter().find(|s| s.id == id).unwrap().state;
    assert_eq!(
        state,
        State::Stopped,
        "no process left: the next message resumes"
    );

    // Never a second process while one of the conversation still runs.
    let late = process(&[&session.session_id]);
    store.set_pid(id, Some(late.id())).unwrap();
    let refused = again.send(id, "hello").unwrap_err().to_string();
    assert!(refused.contains("still runs"), "{refused}");
    end(late);
    end(stranger);
    let starts = || {
        env.log("events")
            .lines()
            .filter(|l| l.starts_with("start"))
            .count()
    };
    let before = starts();
    again.send(id, "hello").unwrap();
    env.until("a new process", || starts() == before + 1);
    assert_eq!(env.args().last().unwrap()[9], "--resume");
    again.shutdown();
}

#[test]
fn keeps_the_state_when_an_event_cannot_be_saved() {
    let env = Env::new("unsaved");
    let id = env.agents.start(env.spec(), None).unwrap().id;
    let db = rusqlite::Connection::open(env.root.join("data/termf.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER full BEFORE INSERT ON agent_events WHEN NEW.kind = 'turn_end'
         BEGIN SELECT RAISE(FAIL, 'database or disk is full'); END;",
    )
    .unwrap();
    env.agents.send(id, "hello").unwrap();
    env.until("the end of the turn", || {
        let s = env.session(id);
        s.state == State::Idle && s.cost > 0.0
    });
    let session = env.session(id);
    assert_eq!(round(session.cost), 0.01);
    assert!(session.context.is_some());
    assert!(!env.kinds(id).contains(&"turn_end".to_owned()));
}

#[test]
fn a_tool_call_keeps_the_event_id_on_the_wire() {
    let stored = StoredEvent {
        id: 7,
        session: 1,
        at: 0,
        event: AgentEvent::ToolUse {
            id: "toolu_1".into(),
            name: "Bash".into(),
            input: json!({"command": "ls"}),
            parent: None,
        },
    };
    let line = serde_json::to_string(&stored).unwrap();
    assert_eq!(line.matches("\"id\"").count(), 1, "{line}");
    let value: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        (value["id"].clone(), value["call"].clone()),
        (json!(7), json!("toolu_1"))
    );
    assert_eq!(serde_json::from_str::<StoredEvent>(&line).unwrap(), stored);
    let before: AgentEvent = serde_json::from_value(
        json!({"kind": "tool_result", "id": "toolu_1", "output": "ok", "isError": false, "truncated": false}),
    )
    .unwrap();
    assert!(matches!(before, AgentEvent::ToolResult { id, .. } if id == "toolu_1"));
}

#[test]
fn refuses_what_cannot_start() {
    let env = Env::new("refuse");
    let refused = |change: fn(&mut SessionSpec)| {
        let mut spec = env.spec();
        change(&mut spec);
        env.agents.start(spec, None).unwrap_err().to_string()
    };
    assert!(refused(|s| s.cwd.push("missing")).contains("not a folder"));
    assert!(refused(|s| s.cwd = "repo".into()).contains("not a folder"));
    assert!(
        refused(|s| s.model = Some("--dangerously-skip-permissions".into()))
            .contains("not a valid")
    );
    assert!(refused(|s| s.budget = Some(0.0)).contains("budget"));
    assert!(refused(|s| s.budget = Some(f64::NAN)).contains("budget"));
    assert!(refused(|s| s.agent = Some("shell".into())).contains("does not run Claude Code"));
    assert!(refused(|s| s.agent = Some("codex".into())).contains("no agent codex"));
    assert!(env.agents.send(42, "hi").is_err());
    assert!(env.agents.sessions().unwrap().is_empty());
}

/// Checks the stream-json format against the real Claude Code, with Haiku, for a few cents. Writes a
/// transcript for a throwaway folder, removed with `claude purge`; never touches the config.
/// `cargo test -p termf-agents -- --ignored real_claude`
#[test]
#[ignore]
fn real_claude() {
    let root = std::env::temp_dir().join(format!("termf-agents-real-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (dir, config) = (root.join("repo"), root.join("config"));
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("termf.toml"), "[agents]\nclaude = \"claude\"\n").unwrap();
    let agents = Agents::build(config, root.join("data"), Vec::new(), |_| {});
    let spec = SessionSpec {
        cwd: dir.clone(),
        model: Some("haiku".into()),
        effort: Some("low".into()),
        permission_mode: Some("manual".into()),
        budget: Some(0.20),
        ..SessionSpec::default()
    };
    let session = agents
        .start(spec, Some("Reply with exactly: pineapple"))
        .unwrap();
    let id = session.id;
    let wait = |turns: usize| {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let s = agents.sessions().unwrap().remove(0);
            let ended = agents
                .events(id, 0)
                .unwrap()
                .iter()
                .filter(|e| matches!(e.event, AgentEvent::TurnEnd { .. }))
                .count();
            if ended == turns && s.state != State::Working {
                return s;
            }
            if matches!(s.state, State::Waiting) {
                let request =
                    agents
                        .events(id, 0)
                        .unwrap()
                        .iter()
                        .rev()
                        .find_map(|e| match &e.event {
                            AgentEvent::Permission { request, .. } => Some(request.clone()),
                            _ => None,
                        });
                let _ = agents.answer(id, &request.unwrap(), true, None);
            }
            assert!(
                Instant::now() < deadline,
                "{:#?}",
                agents.events(id, 0).unwrap()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let first = wait(1);
    agents
        .send(
            id,
            "Create the file f.txt containing hi, with the Bash tool: echo hi > f.txt",
        )
        .unwrap();
    let second = wait(2);
    let pid = crate::store::Store::open(&root.join("data"))
        .unwrap()
        .pid(id)
        .unwrap();
    let recognized = pid.is_some_and(|pid| crate::agents::ours(pid, &session.session_id));
    let events: Vec<AgentEvent> = agents
        .events(id, 0)
        .unwrap()
        .into_iter()
        .map(|e| e.event)
        .collect();
    let _ = agents.stop(id);
    let _ = std::process::Command::new("claude")
        .args(["purge", &dir.to_string_lossy(), "-y"])
        .output();
    let _ = fs::remove_dir_all(&root);

    assert_eq!(first.state, State::Idle, "{events:#?}");
    assert!(recognized, "its process is known by its command line");
    assert!(events.iter().any(|e| matches!(e, AgentEvent::Init { session_id, model, .. } if *session_id == session.session_id && model.contains("haiku"))), "{events:#?}");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Text { text, .. } if text.contains("pineapple"))),
        "{events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::Permission { tool, .. } if tool == "Bash")),
        "{events:#?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolResult {
                is_error: false,
                ..
            }
        )),
        "{events:#?}"
    );
    assert!(second.cost > first.cost && first.cost > 0.0);
    let context = second.context.unwrap();
    assert!(
        context.window.is_some_and(|w| w >= 200_000) && context.tokens.is_some_and(|t| t > 1000),
        "{context:?}"
    );
    assert!(
        events
            .iter()
            .filter(|e| matches!(e, AgentEvent::Started { .. }))
            .count()
            == 1
    );
}
