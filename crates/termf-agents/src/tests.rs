use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

use super::*;

/// A stand-in for Claude Code that reads stream-json and answers like 2.1.330, by the words in each message.
/// It logs its arguments, folder, stdin, and when it starts and ends, next to itself.
const FAKE: &str = r#"
[ "$1" = --version ] && { echo '2.1.330 (Claude Code)'; exit 0; }
{ echo "--- $$"; printf '%s\n' "$@"; } >> "$log.args"
pwd > "$log.cwd"
env > "$log.env"
printf '%s' "${TERMF_MCP_TOKEN-}" > "$log.mcptoken"
# Last, so a test that sees a process start also sees its arguments.
echo "start $$" >> "$log.events"
trap 'echo "exit $$" >> "$log.events"' EXIT
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
    *patience*) sleep 1; answer 'Done.'; result success ;;
    *echo-key*) answer "key=${PEXELS_API_KEY-none}"; result success ;;
    *split-key*) delta "key=px-5f"; delta "3a9c, done"; answer "key=px-5f3a9c, done"; result success ;;
    *webhooks*) answer 'Three retries: src/webhooks.rs:12, and conversation 1111 (2026-11-20).'; result success ;;
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
    /// For each end of turn, the session's cost in the store as the event goes out.
    turn_costs: Arc<Mutex<Vec<(i64, f64)>>>,
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
        let turn_costs: Arc<Mutex<Vec<(i64, f64)>>> = Arc::default();
        let (seen, costs, data) = (updates.clone(), turn_costs.clone(), root.join("data"));
        // Never the user's own shell profile.
        let env = vec![("HOME".into(), home.to_string_lossy().into_owned())];
        let agents = Agents::build(config, data.clone(), env, move |u| {
            if let Update::Event(StoredEvent {
                session,
                event: AgentEvent::TurnEnd { .. },
                ..
            }) = &u
            {
                let stored = sessions(&data).unwrap();
                let cost = stored.iter().find(|s| s.id == *session).unwrap().cost;
                costs.lock().unwrap().push((*session, cost));
            }
            seen.lock().unwrap().push(u)
        });
        Env {
            root,
            dir,
            fake,
            agents,
            updates,
            turn_costs,
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
    let seen: Vec<(i64, f64)> = env.turn_costs.lock().unwrap().clone();
    assert_eq!(
        seen.iter()
            .map(|(s, c)| (*s, round(*c)))
            .collect::<Vec<_>>(),
        [(id, 0.01)],
        "the end of a turn goes out once its cost is stored"
    );
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
        args[0][11..17],
        [
            "--model",
            "haiku",
            "--effort",
            "low",
            "--permission-mode",
            "manual"
        ]
    );
    let deny: Vec<&str> = termf_core::claude::SECRET_DENY
        .iter()
        .flat_map(|rule| ["--disallowedTools", rule])
        .collect();
    assert_eq!(args[0][17..17 + deny.len()], deny);
    // Then only what the shipped library brings, such as its skills for every stack.
    let library = &args[0][17 + deny.len()..];
    assert!(
        library.chunks(2).all(|pair| {
            ["--plugin-dir", "--append-system-prompt-file"].contains(&pair[0].as_str())
        }),
        "{library:?}"
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
    let denied = after("--disallowedTools");
    assert_eq!(denied.len(), 4 + termf_core::claude::SECRET_DENY.len());
    assert_eq!(denied[4..], termf_core::claude::SECRET_DENY[..]);
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
fn the_expert_reads_only_and_never_reaches_the_network() {
    let env = Env::new("expert");
    let spec = SessionSpec {
        role: Some("expert".into()),
        ..env.spec()
    };
    let session = env.agents.start(spec, None).unwrap();
    let denied = &session.spec.disallowed_tools;
    for tool in [
        "Edit",
        "Write",
        "NotebookEdit",
        "Bash",
        "PowerShell",
        "WebFetch",
        "WebSearch",
    ] {
        assert!(denied.iter().any(|t| t == tool), "{tool}: {denied:?}");
    }
    assert!(
        !denied.iter().any(|t| t == "Read" || t == "Agent"),
        "it reads and searches wide"
    );
    assert_eq!(
        (
            session.spec.model.as_deref(),
            session.spec.effort.as_deref()
        ),
        (Some("haiku"), Some("low")),
        "the spec's own model and effort win over the role's"
    );
}

#[test]
fn a_message_written_during_a_turn_goes_once_it_ends() {
    let env = Env::new("queue");
    let id = env
        .agents
        .start(env.spec(), Some("patience please"))
        .unwrap()
        .id;
    assert!(
        env.agents.send_or_queue(id, "hello").unwrap(),
        "kept for the end of the turn"
    );
    assert!(env.agents.send_or_queue(id, "and hello again").unwrap());
    env.settle(id, 2, State::Idle);
    let said: Vec<String> = env
        .events(id)
        .into_iter()
        .filter_map(|e| match e {
            AgentEvent::User { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(said, ["patience please", "hello\n\nand hello again"]);
    assert!(
        !env.agents.send_or_queue(id, "hello").unwrap(),
        "between two turns: at once"
    );
    env.settle(id, 3, State::Idle);
}

#[test]
fn consults_an_agent_for_another_and_waits_for_its_answer() {
    let env = Env::new("consult");
    let ask = |prompt: &str| {
        let spec = SessionSpec {
            role: Some("expert".into()),
            budget: Some(0.5),
            ..env.spec()
        };
        env.agents.consult(spec, prompt, Duration::from_secs(20))
    };
    let (session, text) = ask("Why do webhooks retry?").unwrap();
    assert!(text.contains("src/webhooks.rs:12"), "{text}");
    assert_eq!(
        session.state,
        State::Stopped,
        "its process ends with the answer"
    );
    assert_eq!(session.spec.budget, Some(0.5));
    let (session, text) = ask("permission to write a file").unwrap();
    assert_eq!(text, "Not written.", "nobody can approve a tool");
    let refused = env
        .events(session.id)
        .into_iter()
        .any(|e| matches!(e, AgentEvent::PermissionAnswer { allow: false, .. }));
    assert!(refused);
    let error = ask("over budget").unwrap_err().to_string();
    assert!(error.contains("budget ran out"), "{error}");
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
            guard: false,
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
    assert!(refused(|s| s.agent = Some("aider".into())).contains("no agent aider"));
    assert!(env.agents.send(42, "hi").is_err());
    assert!(env.agents.sessions().unwrap().is_empty());
}

fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

#[test]
fn a_session_with_termf_tools_only_reads_and_asks_before_acting() {
    let env = Env::new("mcp");
    let other = env.root.join("api");
    fs::create_dir_all(&other).unwrap();
    let config = env.root.join("config");
    fs::create_dir_all(config.join("projects")).unwrap();
    fs::write(
        config.join("projects/acme.toml"),
        format!(
            "name = \"Acme\"\n[[repos]]\npath = \"{}\"\n[[repos]]\npath = \"{}\"\n",
            env.dir.display(),
            other.display()
        ),
    )
    .unwrap();
    let spec = SessionSpec {
        project: Some("acme".into()),
        role: Some("cto".into()),
        permission_mode: Some("bypassPermissions".into()),
        mcp: true,
        ..env.spec()
    };
    let error = env.agents.start(spec.clone(), Some("hi")).unwrap_err();
    assert!(error.to_string().contains("needs the app"), "{error}");

    // Each process gets a token of its own; `live` holds the ones not voided yet.
    let live: Arc<Mutex<Vec<(i64, u32)>>> = Arc::default();
    let issued = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let (given, gone) = (live.clone(), live.clone());
    env.agents.serve_mcp(
        move |id| {
            let n = issued.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            given.lock().unwrap().retain(|(s, _)| *s != id);
            given.lock().unwrap().push((id, n));
            Ok(json!({"command": "termf", "args": ["mcp"], "env": {"TERMF_MCP_SESSION": id.to_string(), "TERMF_MCP_TOKEN": format!("cap-{n}")}}))
        },
        move |id| gone.lock().unwrap().retain(|(s, _)| *s != id),
    );
    let session = env.agents.create(spec).unwrap();
    assert_eq!(
        session.state,
        State::Stopped,
        "no process until its first message"
    );
    assert!(env.args().is_empty(), "no process yet: {:?}", env.args());
    assert_eq!(session.spec.permission_mode.as_deref(), Some("manual"));
    let id = session.id;
    env.agents.send(id, "say hello").unwrap();
    env.settle(id, 1, State::Idle);
    let args = env.args().pop().unwrap();
    assert_eq!(arg_after(&args, "--permission-mode"), Some("manual"));
    assert!(args.contains(&"--restricted".into()), "{args:?}");
    assert!(args.contains(&"--strict-mcp-config".into()), "{args:?}");
    assert_eq!(arg_after(&args, "--tools"), Some("Read,Grep,Glob"));
    assert_eq!(arg_after(&args, "--add-dir"), Some(other.to_str().unwrap()));
    let settings: Value = serde_json::from_str(arg_after(&args, "--settings").unwrap()).unwrap();
    let names = |key: &str| -> Vec<String> {
        serde_json::from_value(settings["permissions"][key].clone()).unwrap()
    };
    let ask: Vec<String> = termf_core::ipc::MCP_ACTIONS
        .iter()
        .map(|t| format!("mcp__termf__{t}"))
        .collect();
    assert_eq!(names("ask"), ask, "every action waits for the user");
    assert!(names("allow").iter().all(|t| !ask.contains(t)));
    assert!(
        names("allow").contains(&"mcp__termf__do_action".to_string()),
        "a screen's action waits in TermF's own card, never in a permission prompt"
    );
    for tool in ["Bash", "WebFetch", "WebSearch", "Edit", "Write"] {
        assert!(names("deny").contains(&tool.to_string()), "{tool}");
    }
    let file = PathBuf::from(arg_after(&args, "--mcp-config").unwrap());
    let text = fs::read_to_string(&file).unwrap();
    let config: Value = serde_json::from_str(&text).unwrap();
    let server_env = &config["mcpServers"]["termf"]["env"];
    assert_eq!(server_env["TERMF_MCP_SESSION"], id.to_string());
    assert_eq!(
        server_env["TERMF_MCP_TOKEN"], "${TERMF_MCP_TOKEN}",
        "Claude Code passes it on from its own environment"
    );
    assert!(
        !text.contains("cap-1"),
        "no token another session could read: {text}"
    );
    assert_eq!(
        env.log("mcptoken"),
        "cap-1",
        "only in its process's environment"
    );
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(&file).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);

    let error = env.agents.takeover(id).unwrap_err();
    assert!(error.to_string().contains("stays in TermF"), "{error}");
    assert_eq!(env.session(id).state, State::Idle);

    assert_eq!(*live.lock().unwrap(), [(id, 1)]);
    env.agents.stop(id).unwrap();
    env.until("the token voided", || live.lock().unwrap().is_empty());
    assert!(!file.exists(), "its config goes with the process");
    env.agents.send(id, "say hello").unwrap();
    env.settle(id, 2, State::Idle);
    assert_eq!(
        *live.lock().unwrap(),
        [(id, 2)],
        "a new token, never the old one"
    );
    // A process that ends on its own takes its token with it.
    env.agents.send(id, "crash").unwrap();
    env.until("the crash", || env.session(id).state == State::Error);
    env.until("the token voided", || live.lock().unwrap().is_empty());

    let plain = env.agents.start(env.spec(), Some("say hello")).unwrap();
    env.settle(plain.id, 1, State::Idle);
    assert_eq!(env.log("mcptoken"), "", "no tools, no token");
    let args = env.args().pop().unwrap();
    for flag in ["--restricted", "--mcp-config", "--tools", "--settings"] {
        assert!(
            !args.contains(&flag.into()),
            "{flag} only with TermF's tools"
        );
    }
}

#[test]
fn a_session_gets_its_project_s_vault_keys_masked_in_what_it_says() {
    let env = Env::new("secrets");
    let config = env.root.join("config");
    fs::create_dir_all(config.join("projects")).unwrap();
    let project = format!(
        "name = \"Acme\"\n[[repos]]\npath = \"{}\"\n",
        env.dir.display()
    );
    fs::write(config.join("projects/acme.toml"), project).unwrap();
    let asked: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
    let seen = asked.clone();
    env.agents.serve_secrets(move |_, project| {
        seen.lock().unwrap().push(project.map(str::to_owned));
        Given {
            vars: vec![("PEXELS_API_KEY".into(), "px-5f3a9c".into())],
            note: Some("TermF: the vault has no value for PISTE_CLIENT_ID".into()),
        }
    });
    let id = env.agents.start(env.spec(), Some("echo-key")).unwrap().id;
    env.settle(id, 1, State::Idle);
    assert_eq!(
        *asked.lock().unwrap(),
        [Some("acme".to_owned())],
        "the project of its folder"
    );
    assert!(env.log("env").contains("PEXELS_API_KEY=px-5f3a9c"));
    let stored = serde_json::to_string(&env.events(id)).unwrap();
    assert!(stored.contains("key=••••••"), "{stored}");
    assert!(
        !stored.contains("px-5f3a9c"),
        "never in what TermF keeps or shows"
    );
    assert!(env.events(id).iter().any(
        |e| matches!(e, AgentEvent::Error { message } if message.contains("PISTE_CLIENT_ID"))
    ));
    // Streamed in two pieces that each hold part of it: no piece shows any of it.
    env.agents.send(id, "split-key").unwrap();
    env.settle(id, 2, State::Idle);
    let streamed: Vec<String> = env
        .updates
        .lock()
        .unwrap()
        .iter()
        .filter_map(|u| match u {
            Update::Event(StoredEvent {
                session,
                event: AgentEvent::Delta { text },
                ..
            }) if *session == id => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(!streamed.is_empty());
    assert!(
        streamed
            .iter()
            .all(|t| !t.contains("px-") && !t.contains("3a9c")),
        "{streamed:?}"
    );

    // A CTO conversation never gets them.
    env.agents.serve_mcp(
        |id| Ok(json!({"command": "termf", "args": ["mcp"], "env": {"TERMF_MCP_SESSION": id.to_string()}})),
        |_| {},
    );
    let cto = SessionSpec {
        project: Some("acme".into()),
        role: Some("cto".into()),
        mcp: true,
        ..env.spec()
    };
    let cto = env.agents.start(cto, Some("echo-key")).unwrap().id;
    env.settle(cto, 1, State::Idle);
    assert!(!env.log("env").contains("PEXELS_API_KEY"));
    assert_eq!(asked.lock().unwrap().len(), 1);
}

#[test]
fn a_guarded_session_never_runs_without_termf_guard() {
    let env = Env::new("guard");
    let spec = SessionSpec {
        guard: true,
        ..env.spec()
    };
    let refused = env.agents.start(spec.clone(), Some("hello")).unwrap_err();
    assert!(
        refused.to_string().contains("needs TermF's guard"),
        "{refused}"
    );
    assert!(env.agents.sessions().unwrap().is_empty(), "nothing created");

    let termf = env.root.join("bin/termf");
    env.agents.serve_guard(termf.clone());
    let id = env.agents.start(spec, Some("hello")).unwrap().id;
    env.settle(id, 1, State::Idle);
    let args = env.args().pop().unwrap();
    let settings: Value = serde_json::from_str(arg_after(&args, "--settings").unwrap()).unwrap();
    assert_eq!(settings, termf_core::claude::guard_settings(&termf, None));
    assert!(
        env.agents.takeover(id).unwrap().guard,
        "its terminal keeps it"
    );
}

#[test]
fn the_expert_searches_the_memory_and_other_roles_only_ask_it() {
    let env = Env::new("expert-tools");
    let po = SessionSpec {
        role: Some("po".into()),
        allowed_tools: vec!["mcp__termf__ask_expert".into()],
        ..env.spec()
    };
    let alone = env.agents.start(po.clone(), Some("say hello")).unwrap().id;
    env.settle(alone, 1, State::Idle);
    let args = env.args().pop().unwrap();
    assert!(
        !args.contains(&"--mcp-config".into()),
        "no app, no expert: the step still runs"
    );
    env.agents.serve_mcp(
        |id| Ok(json!({"command": "termf", "args": ["mcp"], "env": {"TERMF_MCP_SESSION": id.to_string()}})),
        |_| {},
    );
    let run = |spec: SessionSpec| {
        let id = env.agents.start(spec, Some("say hello")).unwrap().id;
        env.settle(id, 1, State::Idle);
        let args = env.args().pop().unwrap();
        let file = PathBuf::from(arg_after(&args, "--mcp-config").unwrap());
        let config: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        let tools = config["mcpServers"]["termf"]["env"]["TERMF_MCP_TOOLS"].clone();
        (args, tools)
    };

    let expert = SessionSpec {
        role: Some("expert".into()),
        mcp: true,
        ..env.spec()
    };
    let answering = SessionSpec {
        budget: Some(0.5),
        ..expert.clone()
    };
    assert_eq!(
        crate::mcp_tools(&answering),
        termf_core::ipc::MCP_EXPERT,
        "an answer to another agent never acts on the screens"
    );
    let (args, tools) = run(expert);
    assert_eq!(tools, "memory_search,memory_read,actions,do_action");
    assert_eq!(
        arg_after(&args, "--tools"),
        Some("Read,Grep,Glob,Agent"),
        "sub-agents read"
    );
    assert!(args.contains(&"--strict-mcp-config".into()) && args.contains(&"--restricted".into()));
    let settings: Value = serde_json::from_str(arg_after(&args, "--settings").unwrap()).unwrap();
    let permissions = &settings["permissions"];
    assert_eq!(
        permissions["allow"],
        json!([
            "mcp__termf__memory_search",
            "mcp__termf__memory_read",
            "mcp__termf__actions",
            "mcp__termf__do_action"
        ]),
        "the screens' actions: TermF's own card decides each one"
    );
    assert_eq!(permissions["ask"], Value::Null);
    for tool in [
        "Bash",
        "PowerShell",
        "WebFetch",
        "WebSearch",
        "Edit",
        "Write",
        "NotebookEdit",
    ] {
        assert!(
            permissions["deny"]
                .as_array()
                .unwrap()
                .contains(&json!(tool)),
            "{tool}"
        );
    }

    let panel = SessionSpec {
        role: Some("devops".into()),
        actions: true,
        ..env.spec()
    };
    let (args, tools) = run(panel);
    assert_eq!(tools, "actions,do_action");
    let settings: Value = serde_json::from_str(arg_after(&args, "--settings").unwrap()).unwrap();
    assert_eq!(
        settings["permissions"],
        json!({"allow": ["mcp__termf__actions", "mcp__termf__do_action"]}),
        "a panel conversation keeps its own tools; TermF's own card decides each action"
    );
    assert!(!args.contains(&"--restricted".into()));

    let (args, tools) = run(po);
    assert_eq!(tools, "ask_expert");
    assert!(
        args.windows(2)
            .any(|w| w == ["--allowedTools", "mcp__termf__ask_expert"])
    );
    for flag in [
        "--restricted",
        "--strict-mcp-config",
        "--tools",
        "--settings",
    ] {
        assert!(
            !args.contains(&flag.into()),
            "{flag}: a team step keeps its own tools"
        );
    }
}

#[test]
fn a_role_that_allows_the_browser_gets_it_from_termf_mcp() {
    let env = Env::new("browser-tools");
    env.agents.serve_mcp(
        |id| Ok(json!({"command": "termf", "args": ["mcp"], "env": {"TERMF_MCP_SESSION": id.to_string()}})),
        |_| {},
    );
    let start = |allowed: &[&str]| {
        let spec = SessionSpec {
            allowed_tools: allowed.iter().map(|t| t.to_string()).collect(),
            ..env.spec()
        };
        let id = env.agents.start(spec, Some("say hello")).unwrap().id;
        env.settle(id, 1, State::Idle);
        let args = env.args().pop().unwrap();
        let config = arg_after(&args, "--mcp-config").map(|file| {
            let config: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
            config["mcpServers"]["termf"]["env"].clone()
        });
        (args, config)
    };
    let (args, env_of) = start(&[crate::BROWSER, crate::ASK_EXPERT]);
    let env_of = env_of.unwrap();
    assert_eq!(
        env_of["TERMF_MCP_TOOLS"],
        "ask_expert,browser_open,browser_snapshot,browser_click,browser_fill,browser_wait,browser_console,browser_network,browser_screenshot"
    );
    assert_eq!(
        env_of["TERMF_CONFIG_DIR"],
        json!(env.root.join("config").to_string_lossy())
    );
    assert!(env_of["TERMF_DATA_DIR"].as_str().is_some());
    assert!(
        args.windows(2)
            .any(|w| w == ["--allowedTools", "mcp__termf__browser_*"]),
        "allowed without a prompt"
    );
    let (_, env_of) = start(&["mcp__termf__browser_snapshot"]);
    assert_eq!(env_of.unwrap()["TERMF_MCP_TOOLS"], "browser_snapshot");
    let (args, env_of) = start(&["Read"]);
    assert!(
        env_of.is_none() && !args.contains(&"--mcp-config".into()),
        "no browser unless allowed"
    );
}

#[test]
fn a_topic_done_archives_the_conversation_and_a_new_one_takes_its_place() {
    let env = Env::new("topic");
    let id = env.agents.start(env.spec(), Some("hello")).unwrap().id;
    env.settle(id, 1, State::Idle);
    assert!(
        env.agents.delete(id).is_err(),
        "a conversation in the list is archived first"
    );

    let fresh = env.agents.new_topic(id).unwrap();
    let old = env.session(id);
    assert!(old.archived);
    assert_eq!(old.state, State::Stopped, "its process stopped");
    assert!(!fresh.archived && fresh.id > id);
    assert_eq!(fresh.spec, old.spec, "the same role and settings");
    assert_ne!(
        fresh.session_id, old.session_id,
        "a conversation of its own"
    );
    assert!(env.events(fresh.id).is_empty(), "nothing said yet");
    let refused = env.agents.send(id, "hello").unwrap_err().to_string();
    assert!(refused.contains("archived"), "{refused}");

    // Undo: the old one back, the new one gone, as nothing was said in it.
    assert!(!env.agents.archive(id, false).unwrap().archived);
    env.agents.delete(fresh.id).unwrap();
    assert!(env.agents.session(fresh.id).is_err());
    env.agents.send(id, "hello again").unwrap();
    env.settle(id, 2, State::Idle);

    env.agents.archive(id, true).unwrap();
    let folder = env.root.join(format!("data/agents/{id}"));
    assert!(folder.exists());
    env.agents.delete(id).unwrap();
    assert!(env.agents.session(id).is_err());
    assert!(
        env.agents.events(id, 0).unwrap().is_empty(),
        "its events go"
    );
    assert!(!folder.exists(), "and its folder");
}

#[test]
fn an_older_database_reads_with_nothing_archived() {
    let env = Env::new("archived-migration");
    let data = env.root.join("older");
    fs::create_dir_all(&data).unwrap();
    let db = rusqlite::Connection::open(data.join("termf.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE agent_sessions (id INTEGER PRIMARY KEY, runtime TEXT NOT NULL, session_id TEXT NOT NULL,
            spec TEXT NOT NULL, state TEXT NOT NULL, cost REAL NOT NULL, context TEXT, pid INTEGER,
            created INTEGER NOT NULL, updated INTEGER NOT NULL);
         INSERT INTO agent_sessions VALUES (7, 'claude-code', 's7', '{\"cwd\":\"/tmp\",\"title\":\"CTO · Acme\",\"mcp\":true}',
            'idle', 0.5, NULL, NULL, 1, 2);",
    )
    .unwrap();
    drop(db);
    let read = sessions(&data).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(
        (read[0].id, read[0].archived, read[0].cost),
        (7, false, 0.5)
    );
    assert_eq!(read[0].spec.title, "CTO · Acme");
    assert_eq!(sessions(&data).unwrap(), read, "opened again: added once");
}

#[test]
fn images_go_in_the_message_and_nowhere_else() {
    let env = Env::new("images");
    let id = env.agents.create(env.spec()).unwrap().id;
    let image = |media: &str, data: &str| Image {
        media_type: media.into(),
        data: data.into(),
    };
    let png = image("image/png", "iVBORw0KGgoTERMFSHOT");
    let refused = |images: &[Image]| {
        let e = env.agents.send_with(id, "hello", images).unwrap_err();
        e.to_string()
    };
    assert!(refused(&[image("image/svg+xml", "PHN2Zz4=")]).contains("not image/svg+xml"));
    let big = "A".repeat(IMAGE_MAX + 1);
    assert!(refused(&[image("image/jpeg", &big)]).contains("over 10 MB"));
    assert!(
        env.agents.send_with(id, " ", &[]).is_err(),
        "nothing to send"
    );
    assert!(env.args().is_empty(), "a refused message starts nothing");

    env.agents
        .send_with(id, "hello, what is this?", std::slice::from_ref(&png))
        .unwrap();
    env.settle(id, 1, State::Idle);
    let line: serde_json::Value =
        serde_json::from_str(env.log("stdin").lines().next().unwrap()).unwrap();
    assert_eq!(
        line["message"]["content"],
        json!([
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgoTERMFSHOT"}},
            {"type": "text", "text": "hello, what is this?"},
        ])
    );
    assert!(env.events(id).contains(&AgentEvent::User {
        text: "hello, what is this?".into(),
        images: 1
    }));
    for stored in ["termf.db".into(), format!("agents/{id}/events.jsonl")] {
        let bytes = fs::read(env.root.join("data").join(&stored)).unwrap();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("TERMFSHOT"),
            "{stored} keeps no image"
        );
    }

    env.agents.send_with(id, "", &[png]).unwrap();
    env.settle(id, 2, State::Idle);
    let line: serde_json::Value =
        serde_json::from_str(env.log("stdin").lines().nth(1).unwrap()).unwrap();
    assert_eq!(
        line["message"]["content"].as_array().unwrap().len(),
        1,
        "an image alone: no empty text block"
    );
}

#[test]
fn termf_notices_reach_the_agent_apart_from_the_user() {
    let env = Env::new("notices");
    let notice = |about: &str, text: &str| Notice {
        about: about.into(),
        run: Some(3),
        text: text.into(),
        ..Notice::default()
    };
    let session = env.agents.create(env.spec()).unwrap();
    let id = session.id;
    env.agents
        .notify(
            id,
            notice("waiting", "Run 3: the gate approve-plan waits."),
            true,
        )
        .unwrap();
    assert!(
        env.args().is_empty(),
        "a session without a process is never woken"
    );
    env.agents.send(id, "say hello").unwrap();
    env.settle(id, 1, State::Idle);
    let stdin = env.log("stdin");
    assert!(
        stdin.contains("<termf-notices>\\nRun 3: the gate approve-plan waits.\\n</termf-notices>"),
        "{stdin}"
    );
    assert!(
        stdin.contains("The user's message follows.\\n\\nsay hello"),
        "{stdin}"
    );
    assert_eq!(
        env.events(id)
            .iter()
            .filter_map(|e| match e {
                AgentEvent::User { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        ["say hello"],
        "the user's own words only"
    );
    assert_eq!(env.kinds(id)[..3], ["notice", "started", "user"]);

    env.agents
        .notify(id, notice("direct", "The user wrote to back."), false)
        .unwrap();
    env.agents
        .notify(id, notice("blocked", "Run 3: step t1 is blocked."), true)
        .unwrap();
    env.settle(id, 2, State::Idle);
    let stdin = env.log("stdin");
    let last = stdin.lines().last().unwrap();
    assert!(
        last.contains("The user wrote to back.\\nRun 3: step t1 is blocked."),
        "{last}"
    );
    assert!(last.contains("act only on their answer"), "{last}");
    assert_eq!(env.kinds(id).iter().filter(|k| *k == "user").count(), 1);

    env.agents.send(id, "slow").unwrap();
    env.until("the slow turn", || env.log("stdin").contains("slow"));
    env.agents
        .notify(id, notice("waiting", "Run 3: the gate visual waits."), true)
        .unwrap();
    env.agents.interrupt(id).unwrap();
    env.settle(id, 4, State::Idle);
    assert!(
        env.log("stdin")
            .lines()
            .last()
            .unwrap()
            .contains("the gate visual waits"),
        "read once the turn under way ended"
    );
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
    let store = crate::store::Store::open(&root.join("data")).unwrap();
    let pid = store.pid(id).unwrap();
    let dir = store.dir(id).join("session");
    let recognized = pid.is_some_and(|pid| crate::agents::ours(pid, &session.session_id, &dir));
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

const REAL_CRASH: &str = "TERMF_REAL_CRASH_ROOT";
const LONG: &str = "Write the numbers from 1 to 400 in words, one per line, nothing else.";

fn real_agents(root: &std::path::Path) -> Agents {
    Agents::build(root.join("config"), root.join("data"), Vec::new(), |_| {})
}

fn real_turns(agents: &Agents, id: i64, turns: usize) -> Vec<AgentEvent> {
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let events: Vec<AgentEvent> = agents
            .events(id, 0)
            .unwrap()
            .into_iter()
            .map(|e| e.event)
            .collect();
        let ended = events
            .iter()
            .filter(|e| matches!(e, AgentEvent::TurnEnd { .. }))
            .count();
        if ended >= turns && agents.session(id).unwrap().state != State::Working {
            return events;
        }
        assert!(Instant::now() < deadline, "{events:#?}");
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn last_reply(events: &[AgentEvent]) -> String {
    events
        .iter()
        .rev()
        .find_map(|e| match e {
            AgentEvent::TurnEnd { result, .. } => Some(result.clone().unwrap_or_default()),
            _ => None,
        })
        .unwrap_or_default()
        .to_lowercase()
}

/// What `real_claude_restart` runs in a process of its own: a session in the middle of a long turn when
/// the process ends at once, as an app that crashes.
#[test]
#[ignore]
fn real_crash_child() {
    let Some(root) = std::env::var_os(REAL_CRASH).map(PathBuf::from) else {
        return;
    };
    let agents = real_agents(&root);
    let spec = SessionSpec {
        cwd: root.join("repo"),
        model: Some("haiku".into()),
        effort: Some("low".into()),
        permission_mode: Some("manual".into()),
        budget: Some(0.30),
        ..SessionSpec::default()
    };
    let id = agents
        .start(
            spec,
            Some("Remember the word pineapple for later. Reply with exactly: ok"),
        )
        .unwrap()
        .id;
    real_turns(&agents, id, 1);
    agents.send(id, LONG).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(agents.session(id).unwrap().state, State::Working);
    std::process::exit(0);
}

/// With the real Claude Code on Haiku, a few cents: the app ends in the middle of a turn (a process of its
/// own, ended at once); at the next launch its Claude Code, still running, is killed and the session
/// stopped; the next message resumes the conversation. A long turn interrupted, then another message; the
/// session taken over in a real terminal (`claude --resume` in a PTY, as a tile), a message typed there, the
/// terminal closed and the session handed back: the conversation goes on headless with what was said in the
/// terminal, never two processes at once. Run from a Claude Code session, as it often is, it also checks that
/// neither process is that session's child (which would save no transcript). A throwaway folder and config, your
/// Claude Code account.
/// `cargo test -p termf-agents -- --ignored real_claude_restart --nocapture`
#[test]
#[ignore]
#[cfg(unix)]
fn real_claude_restart() {
    let root = std::env::temp_dir().join(format!("termf-agents-restart-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("repo")).unwrap();
    fs::create_dir_all(root.join("config")).unwrap();
    fs::write(
        root.join("config/termf.toml"),
        "[agents]\nclaude = \"claude\"\n",
    )
    .unwrap();
    let crashed = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "tests::real_crash_child"])
        .env(REAL_CRASH, &root)
        .output()
        .unwrap();
    assert!(crashed.status.success(), "{crashed:?}");
    let store = crate::store::Store::open(&root.join("data")).unwrap();
    let id = 1;
    let pid = store.pid(id).unwrap().expect("the pid stays saved");
    let session_id = store.session(id).unwrap().session_id;
    let dir = store.dir(id).join("session");
    assert!(
        crate::agents::ours(pid, &session_id, &dir),
        "Claude Code outlived the app"
    );

    let (tiles, ended) = std::sync::mpsc::channel();
    let agents = real_agents(&root);
    assert!(
        !crate::agents::ours(pid, &session_id, &dir),
        "killed at launch"
    );
    assert_eq!(agents.session(id).unwrap().state, State::Stopped);
    let before = agents.events(id, 0).unwrap().len();
    agents
        .send(
            id,
            "Which word did I ask you to remember? Reply with that word only.",
        )
        .unwrap();
    // The turn under way when the app ended never ended.
    let events = real_turns(&agents, id, 2);
    assert!(last_reply(&events).contains("pineapple"), "{events:#?}");
    assert!(
        agents.events(id, 0).unwrap()[before..]
            .iter()
            .any(|e| matches!(e.event, AgentEvent::Started { resumed: true })),
        "resumed"
    );

    agents.send(id, LONG).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    agents.interrupt(id).unwrap();
    let events = real_turns(&agents, id, 3);
    assert!(
        matches!(
            events
                .iter()
                .rev()
                .find(|e| matches!(e, AgentEvent::TurnEnd { .. })),
            Some(AgentEvent::TurnEnd {
                outcome: Outcome::Interrupted,
                ..
            })
        ),
        "{events:#?}"
    );
    assert_eq!(agents.session(id).unwrap().state, State::Idle);
    agents.send(id, "Reply with exactly: kiwi").unwrap();
    assert!(last_reply(&real_turns(&agents, id, 4)).contains("kiwi"));

    let takeover = agents.takeover(id).unwrap();
    let headless = store.pid(id).unwrap();
    assert!(
        headless.is_none_or(|pid| !crate::agents::ours(pid, &session_id, &dir)),
        "the headless process ended before the terminal opens"
    );
    let sessions = {
        let agents = agents.clone();
        Arc::new(termf_term::Sessions::default().on_exit(move |tile| {
            agents.tile_exited(tile);
            let _ = tiles.send(tile);
        }))
    };
    let install = termf_core::claude::binary("claude").unwrap();
    let mut command = termf_term::shell_quote(&install.path.to_string_lossy());
    for arg in &takeover.args {
        command = format!("{command} {}", termf_term::shell_quote(arg));
    }
    let (program, args) = termf_term::shell_command(Some(&command));
    let tile = sessions
        .spawn(termf_term::SpawnSpec {
            title: takeover.title.clone(),
            program,
            args,
            cwd: takeover.cwd.clone(),
            agent: Some("claude".into()),
            cols: 120,
            rows: 40,
            env: Vec::new(),
            base_env: Some(
                termf_core::agent_env::load(&root.join("config"), None)
                    .keep(Some(termf_core::claude::Tool::Claude), std::env::vars_os()),
            ),
        })
        .unwrap();
    agents.attach_tile(id, Some(tile)).unwrap();
    let screen = || {
        let raw = sessions.attach(tile, 1, Box::new(|_| {})).unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        // Letters only, escape sequences left out.
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            match c {
                '\x1b' => {
                    if chars.next() == Some('[') {
                        while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
                    }
                }
                c if c.is_alphanumeric() => out.push(c),
                _ => {}
            }
        }
        out
    };
    let wait = |what: &str, done: &dyn Fn(&str) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(90);
        while !done(&screen()) {
            assert!(Instant::now() < deadline, "no {what}: {}", screen());
            std::thread::sleep(Duration::from_millis(300));
        }
    };
    wait("prompt", &|s| {
        s.contains("kiwi") || s.contains("Itrustthisfolder")
    });
    if screen().contains("Itrustthisfolder") && !screen().contains("kiwi") {
        // "No, exit" comes selected; an arrow sent before the dialog reads keys would arrive as Esc.
        std::thread::sleep(Duration::from_secs(1));
        sessions.write(tile, b"\x1b[B").unwrap();
        std::thread::sleep(Duration::from_millis(500));
        sessions.write(tile, b"\r").unwrap();
        wait("the resumed conversation", &|s| s.contains("kiwi"));
    }
    std::thread::sleep(Duration::from_secs(2));
    sessions
        .type_line(
            tile,
            "Name the yellow tropical fruit whose name starts with man, in lowercase, one word only.",
        )
        .unwrap();
    // Only the answer holds the word.
    wait("the answer in the terminal", &|s| s.contains("mango"));
    // The terminal's Claude Code saves the exchange in the conversation's transcript.
    let projects = PathBuf::from(std::env::var("HOME").unwrap()).join(".claude/projects");
    let saved = || {
        fs::read_dir(&projects).unwrap().flatten().any(|p| {
            fs::read_to_string(p.path().join(format!("{session_id}.jsonl")))
                .is_ok_and(|t| t.contains("mango"))
        })
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    while !saved() {
        assert!(
            Instant::now() < deadline,
            "the terminal's turn is not in the transcript: {}",
            screen()
        );
        std::thread::sleep(Duration::from_millis(300));
    }
    // Closed first, as it may take longer than the 1 s `handback` waits in tests: Claude Code ends on its own.
    let closing = Instant::now();
    sessions.terminate(tile).unwrap();
    assert_eq!(ended.recv_timeout(Duration::from_secs(10)), Ok(tile));
    println!(
        "real_claude_restart: the terminal ended {:?} after its hang-up",
        closing.elapsed()
    );
    let back = agents
        .handback(id, |_| panic!("the terminal already ended"))
        .unwrap();
    assert_ne!(back.state, State::Interactive);
    agents
        .send(
            id,
            "Which fruit did you name in your last reply? Reply with that word only.",
        )
        .unwrap();
    let events = real_turns(&agents, id, 5);
    let cost = agents.session(id).unwrap().cost;
    let _ = agents.stop(id);
    let _ = std::process::Command::new(&install.path)
        .args(["purge", &root.join("repo").to_string_lossy(), "-y"])
        .output();
    let _ = fs::remove_dir_all(&root);
    println!("real_claude_restart: ${cost:.4} headless (the terminal's turn not counted)");
    assert!(last_reply(&events).contains("mango"), "{events:#?}");
}

/// Codex CLI as `codex exec --json` answers, its lines from developers.openai.com/codex/noninteractive and
/// codex-rs/exec/src/exec_events.rs (2026-11-22; no Codex installed here to record a real run). One process per
/// turn: the prompt on stdin, the thread resumed by its id.
const FAKE_CODEX: &str = r#"
[ "$1" = --version ] && { echo 'codex-cli 0.171.0'; exit 0; }
{ echo "--- $$"; printf '%s\n' "$@"; } >> "$log.args"
prompt=$(cat)
printf '%s\n--- end\n' "$prompt" >> "$log.stdin"
tid=019a6b2e-0000-7000-8000-00000000c0de
prev=
for a in "$@"; do [ "$prev" = resume ] && tid=$a; prev=$a; done
say() { printf '%s\n' "$1"; }
say "{\"type\":\"thread.started\",\"thread_id\":\"$tid\"}"
say '{"type":"turn.started"}'
# The user's words come last, after the instructions TermF puts first.
case "$(printf '%s\n' "$prompt" | tail -n 1)" in
  *slow*)
    trap 'exit 130' INT
    say '{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"bash -lc sleep","status":"in_progress"}}'
    sleep 30; exit 0 ;;
  *fail*) say '{"type":"turn.failed","error":{"message":"model overloaded"}}'; exit 1 ;;
esac
say '{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","status":"in_progress"}}'
say '{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","aggregated_output":"docs\nsdk\n","exit_code":0,"status":"completed"}}'
say '{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"Repo contains docs, sdk, and examples directories."}}'
say '{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122,"reasoning_output_tokens":0}}'
"#;

/// Gemini CLI as `gemini --output-format stream-json` answers, its objects from gemini-cli's
/// packages/core/src/output/types.ts and stream-json-formatter.test.ts (2026-11-22; no Gemini installed here).
const FAKE_GEMINI: &str = r##"
[ "$1" = --version ] && { echo '0.70.0'; exit 0; }
{ echo "--- $$"; printf '%s\n' "$@"; } >> "$log.args"
prompt=$(cat)
printf '%s\n--- end\n' "$prompt" >> "$log.stdin"
prev=
for a in "$@"; do
  if [ "$prev" = --session-id ] || [ "$prev" = --resume ]; then sid=$a; fi
  prev=$a
done
say() { printf '%s\n' "$1"; }
at='"timestamp":"2026-11-22T10:00:00.000Z"'
say "{\"type\":\"init\",$at,\"session_id\":\"$sid\",\"model\":\"gemini-3-pro\"}"
say "{\"type\":\"message\",$at,\"role\":\"user\",\"content\":\"hidden\"}"
case "$prompt" in
  *slow*)
    trap 'echo Cancelling... >&2; exit 130' INT
    say "{\"type\":\"message\",$at,\"role\":\"assistant\",\"content\":\"Counting\",\"delta\":true}"
    sleep 30; exit 0 ;;
esac
say "{\"type\":\"message\",$at,\"role\":\"assistant\",\"content\":\"Reading \",\"delta\":true}"
say "{\"type\":\"message\",$at,\"role\":\"assistant\",\"content\":\"it.\",\"delta\":true}"
say "{\"type\":\"tool_use\",$at,\"tool_name\":\"read_file\",\"tool_id\":\"read-123\",\"parameters\":{\"file_path\":\"README.md\"}}"
say "{\"type\":\"tool_result\",$at,\"tool_id\":\"read-123\",\"status\":\"success\",\"output\":\"# acme\"}"
say "{\"type\":\"message\",$at,\"role\":\"assistant\",\"content\":\"It is acme.\",\"delta\":true}"
say "{\"type\":\"result\",$at,\"status\":\"success\",\"stats\":{\"total_tokens\":100,\"input_tokens\":50,\"output_tokens\":50,\"cached\":20,\"input\":30,\"duration_ms\":1200,\"tool_calls\":1,\"models\":{}}}"
"##;

impl Env {
    /// Adds fake `codex` and `gemini` as agents of those names; `extra`: more of termf.toml.
    fn with_cli(&self, extra: &str) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let bin = self.fake.parent().unwrap();
        let fake = |name: &str, body: &str| {
            let path = bin.join(name);
            fs::write(
                &path,
                format!("#!/bin/sh\nlog='{}'\n{body}", path.display()),
            )
            .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let (codex, gemini) = (fake("codex", FAKE_CODEX), fake("gemini", FAKE_GEMINI));
        fs::write(
            self.root.join("config/termf.toml"),
            format!(
                "[agents]\nclaude = \"{}\"\ncodex = \"{}\"\ngemini = \"{}\"\n{extra}",
                self.fake.display(),
                codex.display(),
                gemini.display()
            ),
        )
        .unwrap();
        (codex, gemini)
    }

    fn cli_args(path: &std::path::Path) -> Vec<Vec<String>> {
        let log = fs::read_to_string(format!("{}.args", path.display())).unwrap_or_default();
        let runs = log.split("--- ").skip(1);
        runs.map(|r| r.lines().skip(1).map(str::to_owned).collect())
            .collect()
    }

    fn exits(&self, id: i64) -> usize {
        self.kinds(id).iter().filter(|k| *k == "exited").count()
    }
}

#[test]
fn codex_runs_a_process_per_turn_and_resumes_its_thread() {
    let env = Env::new("codex");
    let (codex, _) = env.with_cli("[roles.docs]\nagent = \"codex\"\n");
    fs::write(env.dir.join("AGENTS.md"), "mine").unwrap();
    let spec = SessionSpec {
        cwd: env.dir.clone(),
        role: Some("docs".into()),
        permission_mode: Some("acceptEdits".into()),
        ..SessionSpec::default()
    };
    let session = env.agents.start(spec, Some("list the repo")).unwrap();
    let id = session.id;
    assert_eq!(session.runtime, "codex");
    assert_eq!(
        session.spec.agent.as_deref(),
        Some("codex"),
        "the role's agent"
    );
    assert_eq!(
        (
            session.spec.model.as_deref(),
            session.spec.effort.as_deref()
        ),
        (None, None),
        "the role file's sonnet and effort are Claude Code's"
    );
    // A process's end is recorded before the session's new state is saved: each wait below takes both.
    env.until("the first turn and its process's end", || {
        env.turns(id) == 1 && env.exits(id) == 1 && env.session(id).state == State::Idle
    });
    let session = env.session(id);
    assert_eq!(session.state, State::Idle, "no process between turns");
    assert_eq!(session.session_id, "019a6b2e-0000-7000-8000-00000000c0de");
    assert_eq!(
        env.kinds(id),
        [
            "started",
            "user",
            "init",
            "tool_use",
            "tool_result",
            "text",
            "turn_end",
            "exited"
        ]
    );
    let (outcome, cost, ..) = turn_end(&env.events(id));
    assert_eq!((outcome, cost), (Outcome::Done, 0.0));

    let image = Image {
        media_type: "image/png".into(),
        data: "iVBORw0KGgo=".into(),
    };
    let refused = env.agents.send_with(id, "look", &[image]).unwrap_err();
    assert!(refused.to_string().contains("text only"), "{refused}");
    env.agents.send(id, "and again").unwrap();
    env.until("the second turn", || {
        env.turns(id) == 2 && env.exits(id) == 2 && env.session(id).state == State::Idle
    });
    let runs = Env::cli_args(&codex);
    assert_eq!(runs.len(), 2, "one process per turn");
    assert_eq!(
        &runs[0][..5],
        [
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--sandbox",
            "workspace-write"
        ]
    );
    assert!(!runs[0].contains(&"resume".to_owned()));
    assert!(
        !runs[0].iter().any(|a| a.contains("Role")),
        "no instructions on a command line cmd.exe would cut"
    );
    assert_eq!(
        runs[1][runs[1].len() - 3..],
        ["resume", "019a6b2e-0000-7000-8000-00000000c0de", "-"]
    );
    let stdin = fs::read_to_string(format!("{}.stdin", codex.display())).unwrap();
    let turns: Vec<&str> = stdin.split("--- end\n").collect();
    assert!(
        turns[0].starts_with("<termf-instructions>\n## Role: docs"),
        "{stdin}"
    );
    assert!(turns[0].ends_with("list the repo\n"));
    assert_eq!(turns[1], "and again\n", "once per conversation");
    assert_eq!(
        fs::read_to_string(env.dir.join("AGENTS.md")).unwrap(),
        "mine",
        "never written"
    );

    env.agents.send(id, "go slow").unwrap();
    env.until("the command under way", || {
        env.events(id)
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolUse { id, .. } if id == "item_0"))
    });
    env.agents.interrupt(id).unwrap();
    env.until("the interrupted turn, the conversation going on", || {
        env.turns(id) == 3 && env.exits(id) == 3 && env.session(id).state == State::Idle
    });
    assert_eq!(turn_end(&env.events(id)).0, Outcome::Interrupted);

    env.agents.send(id, "this will fail").unwrap();
    env.until("the failed turn", || {
        env.turns(id) == 4 && env.exits(id) == 4 && env.session(id).state == State::Error
    });
    let (outcome, _, _, error, _) = turn_end(&env.events(id));
    assert_eq!(
        (outcome, error.as_deref()),
        (Outcome::Error, Some("model overloaded"))
    );

    let takeover = env.agents.takeover(id).unwrap();
    assert_eq!(takeover.agent, "codex");
    assert_eq!(
        takeover.args[..2],
        ["resume", "019a6b2e-0000-7000-8000-00000000c0de"]
    );
    env.agents.attach_tile(id, None).unwrap();
}

#[test]
fn gemini_reads_the_instructions_once_and_keeps_secrets_out() {
    let env = Env::new("gemini");
    let (_, gemini) = env.with_cli("");
    let spec = SessionSpec {
        cwd: env.dir.clone(),
        agent: Some("gemini".into()),
        role: Some("docs".into()),
        model: Some("gemini-3-pro".into()),
        permission_mode: Some("plan".into()),
        ..SessionSpec::default()
    };
    let id = env.agents.start(spec, Some("what is this")).unwrap().id;
    env.until("the first turn", || {
        env.turns(id) == 1 && env.exits(id) == 1 && env.session(id).state == State::Idle
    });
    let session = env.session(id);
    assert_eq!(
        (session.runtime.as_str(), session.state),
        ("gemini", State::Idle)
    );
    assert_eq!(
        env.kinds(id),
        [
            "started",
            "user",
            "init",
            "text",
            "tool_use",
            "tool_result",
            "text",
            "turn_end",
            "exited"
        ]
    );
    let events = env.events(id);
    assert!(events.contains(&AgentEvent::Text {
        text: "Reading it.".into(),
        parent: None
    }));
    let AgentEvent::TurnEnd { result, usage, .. } = &events[7] else {
        panic!()
    };
    assert_eq!(
        (result.as_deref(), usage.input, usage.cache_read),
        (Some("It is acme."), 50, 20)
    );
    assert!(
        events.contains(&AgentEvent::User {
            text: "what is this".into(),
            images: 0
        }),
        "your words alone"
    );

    env.agents.send(id, "more").unwrap();
    env.until("the second turn", || {
        env.turns(id) == 2 && env.exits(id) == 2 && env.session(id).state == State::Idle
    });
    let runs = Env::cli_args(&gemini);
    assert_eq!(
        arg_after(&runs[0], "--session-id"),
        Some(session.session_id.as_str())
    );
    assert_eq!(
        arg_after(&runs[1], "--resume"),
        Some(session.session_id.as_str())
    );
    assert_eq!(arg_after(&runs[0], "--approval-mode"), Some("plan"));
    let policy = fs::read_to_string(arg_after(&runs[1], "--policy").unwrap()).unwrap();
    assert!(policy.contains("printenv") && policy.contains("decision = \"deny\""));
    let stdin = fs::read_to_string(format!("{}.stdin", gemini.display())).unwrap();
    let turns: Vec<&str> = stdin.split("--- end\n").collect();
    assert!(
        turns[0].starts_with("<termf-instructions>\n## Role: docs"),
        "{stdin}"
    );
    assert!(turns[0].ends_with("what is this\n"));
    assert_eq!(turns[1], "more\n", "once per conversation");
    assert!(!env.dir.join("GEMINI.md").exists());

    env.agents.send(id, "slow please").unwrap();
    env.until("the first words", || {
        env.updates.lock().unwrap().iter().any(|u| {
            matches!(u, Update::Event(StoredEvent { event: AgentEvent::Delta { text }, .. }) if text == "Counting")
        })
    });
    assert!(
        env.agents.send_or_queue(id, "then this").unwrap(),
        "kept for the end of the turn"
    );
    env.agents.interrupt(id).unwrap();
    env.until("the interrupted turn, then the one it kept", || {
        env.turns(id) == 4 && env.exits(id) == 4 && env.session(id).state == State::Idle
    });
    let ends: Vec<Outcome> = env
        .events(id)
        .into_iter()
        .filter_map(|e| match e {
            AgentEvent::TurnEnd { outcome, .. } => Some(outcome),
            _ => None,
        })
        .collect();
    assert_eq!(ends[2..], [Outcome::Interrupted, Outcome::Done]);
    let stdin = fs::read_to_string(format!("{}.stdin", gemini.display())).unwrap();
    assert!(stdin.ends_with("then this\n--- end\n"), "{stdin}");
    assert_eq!(env.session(id).state, State::Idle);
}

#[test]
fn codex_and_gemini_refuse_what_only_claude_code_does() {
    let env = Env::new("cli-refuse");
    let (codex, _) = env.with_cli("[roles.cto]\nagent = \"codex\"\n");
    let refused = |agent: &str, change: fn(&mut SessionSpec)| {
        let mut spec = SessionSpec {
            cwd: env.dir.clone(),
            agent: Some(agent.into()),
            ..SessionSpec::default()
        };
        change(&mut spec);
        env.agents.start(spec, Some("hi")).unwrap_err().to_string()
    };
    for agent in ["codex", "gemini"] {
        assert!(refused(agent, |s| s.json_schema = Some(json!({}))).contains("JSON schema"));
        assert!(refused(agent, |s| s.budget = Some(1.0)).contains("cost"));
        assert!(
            refused(agent, |s| s.permission_mode = Some("manual".into())).contains("cannot ask")
        );
        assert!(refused(agent, |s| s.role = Some("reviewer".into())).contains("single tools"));
    }
    assert!(refused("gemini", |s| s.effort = Some("high".into())).contains("effort"));
    assert!(env.agents.sessions().unwrap().is_empty(), "nothing started");

    let spec = SessionSpec {
        cwd: env.dir.clone(),
        agent: Some("codex".into()),
        ..SessionSpec::default()
    };
    let id = env.agents.start(spec, None).unwrap().id;
    assert_eq!(
        env.session(id).state,
        State::Stopped,
        "no process before its first message"
    );
    assert!(
        env.agents.answer(id, "r1", true, None).is_err(),
        "nothing ever asks"
    );

    // The CTO stays on Claude Code whatever its role says.
    let cto = SessionSpec {
        cwd: env.dir.clone(),
        role: Some("cto".into()),
        ..SessionSpec::default()
    };
    let cto = env.agents.create(SessionSpec { mcp: true, ..cto }).unwrap();
    assert_eq!(
        (cto.runtime.as_str(), cto.spec.agent.as_deref()),
        ("claude-code", Some("claude"))
    );

    fs::write(&codex, "#!/bin/sh\necho 'codex-cli 0.90.0'\n").unwrap();
    let old = refused("codex", |_| {});
    assert!(
        old.contains("Codex CLI 0.90.0 at")
            && old.contains("is too old: TermF needs 0.171.0 or later"),
        "{old}"
    );
}

#[test]
fn the_first_codex_process_left_by_the_last_launch_is_stopped() {
    let env = Env::new("codex-orphan");
    env.with_cli("");
    let spec = SessionSpec {
        cwd: env.dir.clone(),
        agent: Some("codex".into()),
        ..SessionSpec::default()
    };
    let id = env.agents.start(spec, Some("go slow")).unwrap().id;
    env.until("the command under way", || {
        env.events(id)
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolUse { id, .. } if id == "item_0"))
    });
    let store = crate::store::Store::open(&env.root.join("data")).unwrap();
    let session = store.session(id).unwrap();
    let pid = store.pid(id).unwrap().unwrap();
    assert_eq!(
        session.session_id, "019a6b2e-0000-7000-8000-00000000c0de",
        "the id Codex gave, on no command line yet"
    );
    assert!(crate::agents::ours(
        pid,
        &session.session_id,
        &store.dir(id).join("session")
    ));
    assert!(
        !crate::agents::ours(pid, "another", &store.dir(id + 1).join("session")),
        "another session's"
    );
    // The app quit without stopping it: the next launch does.
    let _again = Agents::build(
        env.root.join("config"),
        env.root.join("data"),
        Vec::new(),
        |_| {},
    );
    env.until("the orphan killed", || !running(pid));
}

/// TermF started from a Claude Code session (an agent running `pnpm tauri dev`, a test run in it): the Claude
/// Code it starts must not inherit that session's markers, or it is its child and saves no transcript (Claude
/// Code 2.1.330: "Transcript saving is off — inherited CLAUDE_CODE_CHILD_SESSION marker"), so a hand-back resumes
/// without what was said in the terminal. Runs itself again with the markers in its environment; the fake
/// `claude` writes down the environment it gets, with the allowlist and with `env = "inherit"`.
#[test]
fn a_session_is_no_child_of_the_claude_code_termf_runs_in() {
    const MARKED: &str = "TERMF_TEST_UNDER_CLAUDE_CODE";
    if std::env::var_os(MARKED).is_none() {
        let mut again = std::process::Command::new(std::env::current_exe().unwrap());
        again
            .args([
                "--exact",
                "tests::a_session_is_no_child_of_the_claude_code_termf_runs_in",
            ])
            .env(MARKED, "1");
        for marker in termf_core::claude::SESSION_MARKERS {
            again.env(marker, "parent");
        }
        let out = again.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        return;
    }
    for inherit in [false, true] {
        let env = Env::new(&format!("marked-{inherit}"));
        if inherit {
            let toml = env.root.join("config/termf.toml");
            let text = fs::read_to_string(&toml).unwrap();
            fs::write(
                &toml,
                text.replace("[agents]\n", "[agents]\nenv = \"inherit\"\n"),
            )
            .unwrap();
        }
        let id = env.agents.start(env.spec(), Some("hello")).unwrap().id;
        env.settle(id, 1, State::Idle);
        let seen = env.log("env");
        assert!(seen.contains("PATH="), "{seen}");
        for marker in termf_core::claude::SESSION_MARKERS {
            assert!(
                !seen.lines().any(|l| l.starts_with(&format!("{marker}="))),
                "inherit {inherit}: {marker} reached Claude Code"
            );
        }
    }
}
