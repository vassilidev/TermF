//! Claude Code, headless: `claude -p --input-format stream-json --output-format stream-json`, checked on
//! Claude Code 2.1.330. One JSON object per line each way; permission prompts and interrupts go through its
//! control protocol (`control_request` / `control_response`), as the Agent SDK does.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use termf_core::context::Gauge;
use termf_core::ipc::{MCP_ACTIONS, MCP_BROWSER, MCP_EXPERT, MCP_OPS, MCP_READ};

use crate::{AgentEvent, AgentRuntime, Launch, Outcome, Parser, SessionSpec, Usage};

/// How many characters of a tool's output an event keeps.
pub const TOOL_OUTPUT: usize = 4000;

pub struct ClaudeCode;

/// The role whose conversation searches the project memory, and the tool another role calls it with.
pub const EXPERT: &str = "expert";
pub const ASK_EXPERT: &str = "mcp__termf__ask_expert";
/// What a role allows to get the browser of `termf mcp`: every `browser_*` tool, without a permission prompt.
pub const BROWSER: &str = "mcp__termf__browser_*";
/// What a role allows to get Ops: `ops_recipes` and `ops_run`, each run approved in TermF's own card.
pub const OPS: &str = "mcp__termf__ops_*";

fn expert(spec: &SessionSpec) -> bool {
    spec.mcp && spec.role.as_deref() == Some(EXPERT)
}

/// The tools of `termf mcp` a session gets: the expert's conversation searches the project memory; a CTO
/// conversation reads and acts; any other session the ones its allowed tools name: `mcp__termf__ask_expert`
/// asks the expert, `mcp__termf__browser_*` (or one `mcp__termf__browser_…`) drives its own browser.
pub fn mcp_tools(spec: &SessionSpec) -> Vec<&'static str> {
    if expert(spec) {
        return MCP_EXPERT.to_vec();
    }
    if spec.mcp {
        return MCP_READ
            .iter()
            .chain(&MCP_ACTIONS)
            .chain(&MCP_OPS)
            .copied()
            .collect();
    }
    let allowed = |all: &str, tool: &str| {
        spec.allowed_tools
            .iter()
            .any(|t| t == all || t.strip_prefix("mcp__termf__") == Some(tool))
    };
    let mut tools: Vec<&str> = Vec::new();
    if spec.allowed_tools.iter().any(|t| t == ASK_EXPERT) {
        tools.push("ask_expert");
    }
    tools.extend(MCP_BROWSER.iter().filter(|t| allowed(BROWSER, t)));
    // Only where `termf guard` sees each command: once it read prod, the session's ways out need the user.
    if spec.guard {
        tools.extend(MCP_OPS.iter().filter(|t| allowed(OPS, t)));
    }
    tools
}

/// What a session with TermF's tools may do: call its read tools, and each of its actions only once the user
/// approves it (`ask` wins over any `allow`, the managed settings' included); no command, no edit, no web. The
/// expert, read-only, has no action, and its read-only sub-agents search wide. Ops are allowed here: TermF asks
/// the user in its own card, with the exact command.
pub fn mcp_settings(spec: &SessionSpec) -> Value {
    let tools = |names: &[&str]| -> Vec<String> {
        names.iter().map(|n| format!("mcp__termf__{n}")).collect()
    };
    let mut deny = vec![
        "Bash",
        "PowerShell",
        "Edit",
        "Write",
        "NotebookEdit",
        "WebFetch",
        "WebSearch",
    ];
    if expert(spec) {
        return json!({ "permissions": { "allow": tools(&MCP_EXPERT), "deny": deny } });
    }
    deny.push("Agent");
    json!({ "permissions": {
        "allow": tools(&MCP_READ).into_iter().chain(tools(&MCP_OPS)).collect::<Vec<_>>(),
        "ask": tools(&MCP_ACTIONS),
        "deny": deny,
    } })
}

/// `--model`, `--effort`, `--permission-mode` and the tools, as the spec sets them.
fn options(spec: &SessionSpec) -> Vec<String> {
    let mut args = Vec::new();
    for (flag, value) in [
        ("--model", &spec.model),
        ("--effort", &spec.effort),
        ("--permission-mode", &spec.permission_mode),
    ] {
        if let Some(value) = value {
            args.extend([flag.to_owned(), value.clone()]);
        }
    }
    // One value each: Claude Code reads a comma inside a rule such as `Bash(a, b)` as a separator.
    for (flag, tools) in [
        ("--allowedTools", &spec.allowed_tools),
        ("--disallowedTools", &spec.disallowed_tools),
    ] {
        for tool in tools {
            args.extend([flag.to_owned(), tool.clone()]);
        }
    }
    args
}

impl AgentRuntime for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn label(&self) -> &'static str {
        "Claude Code"
    }

    fn headless_args(&self, spec: &SessionSpec, launch: &Launch) -> Vec<String> {
        let mut args: Vec<String> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            // Not in `--help`: permission prompts come to TermF as control requests, as with the Agent SDK.
            "--permission-prompt-tool",
            "stdio",
        ]
        .map(Into::into)
        .to_vec();
        let id = if launch.resume {
            "--resume"
        } else {
            "--session-id"
        };
        args.extend([id.into(), launch.session_id.into()]);
        args.extend(options(spec));
        // A tile gets them from its `--settings` file, which a headless session has none of.
        for rule in termf_core::claude::SECRET_DENY {
            args.extend(["--disallowedTools".into(), rule.to_string()]);
        }
        if let Some(budget) = launch.budget {
            args.extend(["--max-budget-usd".into(), format!("{budget:.4}")]);
        }
        if let Some(turns) = spec.max_turns {
            args.extend(["--max-turns".into(), turns.to_string()]);
        }
        if let Some(plugin) = launch.plugin {
            args.extend(["--plugin-dir".into(), plugin.to_string_lossy().into_owned()]);
        }
        if let Some(schema) = &spec.json_schema {
            args.extend(["--json-schema".into(), schema.to_string()]);
        }
        if let Some(prompt) = launch.system_prompt {
            args.extend([
                "--append-system-prompt-file".into(),
                prompt.to_string_lossy().into_owned(),
            ]);
        }
        // Both in one `--settings`: several are not known to add up.
        let mut settings = json!({});
        if let Some(termf) = launch.guard {
            settings = termf_core::claude::guard_settings(termf);
        }
        if spec.mcp {
            // Trifecta (ARCHITECTURE §12.4): no command, no web, no MCP server but TermF's, none of the
            // user's or the repository's settings files, the file tools kept to the repositories.
            let built_in = if expert(spec) {
                "Read,Grep,Glob,Agent"
            } else {
                "Read,Grep,Glob"
            };
            args.extend(
                ["--restricted", "--tools", built_in, "--strict-mcp-config"].map(Into::into),
            );
            settings["permissions"] = mcp_settings(spec)["permissions"].take();
        }
        for dir in launch.dirs {
            args.extend(["--add-dir".into(), dir.to_string_lossy().into_owned()]);
        }
        if let Some(config) = launch.mcp {
            args.extend(["--mcp-config".into(), config.to_string_lossy().into_owned()]);
        }
        if settings != json!({}) {
            args.extend(["--settings".into(), settings.to_string()]);
        }
        args
    }

    fn interactive_args(&self, spec: &SessionSpec, session_id: &str) -> Vec<String> {
        let mut args = vec!["--resume".into(), session_id.into()];
        args.extend(options(spec));
        args
    }

    fn parser(&self) -> Box<dyn Parser> {
        Box::<Stream>::default()
    }

    fn message(&self, text: &str) -> String {
        json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string()
    }

    fn interrupt(&self) -> Option<String> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = format!("termf-interrupt-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let line = json!({ "type": "control_request", "request_id": id, "request": { "subtype": "interrupt" } });
        Some(line.to_string())
    }

    fn answer(&self, request: &str, allow: bool, input: &Value, message: &str) -> String {
        let decision = if allow {
            json!({ "behavior": "allow", "updatedInput": input })
        } else {
            json!({ "behavior": "deny", "message": message })
        };
        json!({
            "type": "control_response",
            "response": { "subtype": "success", "request_id": request, "response": decision },
        })
        .to_string()
    }
}

#[derive(Default)]
struct Stream {
    /// Claude Code says `system/init` at every turn: one is enough.
    init: bool,
    /// Tokens in context after the last answer of the main conversation, and the model that gave it.
    tokens: Option<u64>,
    model: Option<String>,
    /// `total_cost_usd` adds up the whole process: a turn costs the difference.
    spent: f64,
}

impl Parser for Stream {
    fn parse(&mut self, line: &[u8]) -> Vec<AgentEvent> {
        let Ok(msg) = serde_json::from_slice::<Value>(line) else {
            return Vec::new();
        };
        let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        // Subagents' messages carry the tool call that started them.
        let parent = msg["parent_tool_use_id"].as_str().map(str::to_owned);
        let main = parent.is_none();
        match msg["type"].as_str().unwrap_or_default() {
            "system" => match msg["subtype"].as_str() {
                Some("init") if !self.init => {
                    self.init = true;
                    vec![AgentEvent::Init {
                        session_id: text(&msg["session_id"]),
                        model: text(&msg["model"]),
                        permission_mode: text(&msg["permissionMode"]),
                    }]
                }
                Some("permission_denied") => vec![AgentEvent::Denied {
                    tool: text(&msg["tool_name"]),
                    message: text(&msg["message"]),
                }],
                _ => Vec::new(),
            },
            "stream_event" if main => {
                let event = &msg["event"];
                let delta = &event["delta"];
                if event["type"] == "content_block_delta" && delta["type"] == "text_delta" {
                    vec![AgentEvent::Delta {
                        text: text(&delta["text"]),
                    }]
                } else {
                    Vec::new()
                }
            }
            "assistant" => {
                let message = &msg["message"];
                if main && let Some(usage) = message["usage"].as_object() {
                    let in_context = [
                        "input_tokens",
                        "cache_creation_input_tokens",
                        "cache_read_input_tokens",
                    ];
                    self.tokens = Some(
                        in_context
                            .iter()
                            .filter_map(|k| usage.get(*k)?.as_u64())
                            .sum(),
                    );
                    self.model = message["model"].as_str().map(str::to_owned);
                }
                blocks(message)
                    .filter_map(|block| match block["type"].as_str()? {
                        "text" if block["text"].as_str().is_some_and(|t| !t.is_empty()) => {
                            Some(AgentEvent::Text {
                                text: text(&block["text"]),
                                parent: parent.clone(),
                            })
                        }
                        "tool_use" => Some(AgentEvent::ToolUse {
                            id: text(&block["id"]),
                            name: text(&block["name"]),
                            input: block["input"].clone(),
                            parent: parent.clone(),
                        }),
                        _ => None,
                    })
                    .collect()
            }
            "user" => blocks(&msg["message"])
                .filter(|block| block["type"] == "tool_result")
                .map(|block| {
                    let (output, truncated) = cut(&tool_output(&block["content"]));
                    AgentEvent::ToolResult {
                        id: text(&block["tool_use_id"]),
                        output,
                        is_error: block["is_error"] == true,
                        truncated,
                        parent: parent.clone(),
                    }
                })
                .collect(),
            "control_request" if msg["request"]["subtype"] == "can_use_tool" => {
                let request = &msg["request"];
                vec![AgentEvent::Permission {
                    request: text(&msg["request_id"]),
                    tool: text(&request["tool_name"]),
                    input: request["input"].clone(),
                    description: request["description"].as_str().map(str::to_owned),
                }]
            }
            "result" => vec![self.turn_end(&msg)],
            _ => Vec::new(),
        }
    }
}

impl Stream {
    fn turn_end(&mut self, msg: &Value) -> AgentEvent {
        let spent = msg["total_cost_usd"].as_f64().unwrap_or(self.spent);
        let cost = (spent - self.spent).max(0.0);
        self.spent = spent;
        let subtype = msg["subtype"].as_str().unwrap_or_default();
        let aborted = msg["terminal_reason"]
            .as_str()
            .is_some_and(|r| r.starts_with("aborted"));
        let outcome = match subtype {
            "success" if msg["is_error"] != true => Outcome::Done,
            "error_max_budget_usd" => Outcome::Budget,
            "error_during_execution" if aborted => Outcome::Interrupted,
            _ => Outcome::Error,
        };
        let result = msg["result"].as_str().map(str::to_owned);
        let structured = Some(msg["structured_output"].clone()).filter(|v| !v.is_null());
        let error = matches!(outcome, Outcome::Error | Outcome::Budget).then(|| {
            let errors: Vec<&str> = msg["errors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            match (errors.is_empty(), &result) {
                (false, _) => errors.join("; "),
                (true, Some(r)) if !r.trim().is_empty() => r.clone(),
                _ => subtype.to_owned(),
            }
        });
        let usage = &msg["usage"];
        let count = |key: &str| usage[key].as_u64().unwrap_or(0);
        AgentEvent::TurnEnd {
            outcome,
            result,
            structured,
            error,
            usage: Usage {
                input: count("input_tokens"),
                output: count("output_tokens"),
                cache_read: count("cache_read_input_tokens"),
                cache_creation: count("cache_creation_input_tokens"),
            },
            cost,
            total: 0.0,
            context: self.gauge(&msg["modelUsage"]),
        }
    }

    /// The window is the one `modelUsage` gives for the model of the last answer.
    fn gauge(&self, models: &Value) -> Option<Gauge> {
        let tokens = self.tokens?;
        let window = self
            .model
            .as_ref()
            .and_then(|m| models[m]["contextWindow"].as_u64())
            .or_else(|| {
                let all = models.as_object()?.values();
                all.filter_map(|m| m["contextWindow"].as_u64()).max()
            })
            .filter(|w| *w > 0);
        let percent = window.map(|w| (tokens as f64 * 100.0 / w as f64).round().min(100.0) as u8);
        Some(Gauge {
            percent,
            tokens: Some(tokens),
            window,
            ..Gauge::default()
        })
    }
}

fn blocks(message: &Value) -> impl Iterator<Item = &Value> {
    message["content"].as_array().into_iter().flatten()
}

/// A tool result is text, or blocks of text and images.
fn tool_output(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .map(|b| match b["type"].as_str() {
                Some("text") => b["text"].as_str().unwrap_or_default().to_owned(),
                Some(other) => format!("[{other}]"),
                None => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub(crate) fn cut(text: &str) -> (String, bool) {
    match text.char_indices().nth(TOOL_OUTPUT) {
        Some((end, _)) => (text[..end].to_owned(), true),
        None => (text.to_owned(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(lines: &[&str]) -> Vec<AgentEvent> {
        let mut parser = ClaudeCode.parser();
        lines
            .iter()
            .flat_map(|l| parser.parse(l.as_bytes()))
            .collect()
    }

    /// Lines as Claude Code 2.1.330 wrote them, shortened.
    #[test]
    fn reads_a_turn() {
        let events = parse_all(&[
            r#"{"type":"system","subtype":"hook_started","hook_id":"h1","hook_name":"SessionStart:startup","session_id":"s1"}"#,
            r#"{"type":"system","subtype":"init","cwd":"/tmp/x","session_id":"s1","tools":["Bash"],"model":"claude-haiku-5-5","permissionMode":"default"}"#,
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"model":"claude-haiku-5-5"}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}},"parent_tool_use_id":null}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"h"}},"parent_tool_use_id":null}"#,
            r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","content":[{"type":"thinking","thinking":"","signature":"x"}],"usage":{"input_tokens":2,"cache_creation_input_tokens":18226,"cache_read_input_tokens":11844,"output_tokens":5}},"parent_tool_use_id":null,"session_id":"s1"}"#,
            r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"echo hi > f.txt"}}],"usage":{"input_tokens":2,"cache_creation_input_tokens":18226,"cache_read_input_tokens":11844,"output_tokens":5}},"parent_tool_use_id":null}"#,
            r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"echo hi > f.txt"},"description":"Write hi","tool_use_id":"toolu_1"}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_1","type":"tool_result","content":"(Bash completed with no output)","is_error":false}]},"parent_tool_use_id":null}"#,
            r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5","permissionMode":"default"}"#,
            r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","content":[{"type":"text","text":"Done."}],"usage":{"input_tokens":2,"cache_creation_input_tokens":7546,"cache_read_input_tokens":31765,"output_tokens":4}},"parent_tool_use_id":null}"#,
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"Done.","total_cost_usd":0.0058,"usage":{"input_tokens":4,"cache_creation_input_tokens":25919,"cache_read_input_tokens":45157,"output_tokens":440},"modelUsage":{"claude-haiku-5-5":{"costUSD":0.0058,"contextWindow":1000000}},"terminal_reason":"completed","session_id":"s1"}"#,
        ]);
        let kinds: Vec<String> = events
            .iter()
            .map(|e| {
                serde_json::to_value(e).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            kinds,
            [
                "init",
                "delta",
                "tool_use",
                "permission",
                "tool_result",
                "text",
                "turn_end"
            ],
            "{events:#?}"
        );
        assert_eq!(
            events[3],
            AgentEvent::Permission {
                request: "r1".into(),
                tool: "Bash".into(),
                input: json!({"command": "echo hi > f.txt"}),
                description: Some("Write hi".into()),
            }
        );
        let AgentEvent::TurnEnd {
            outcome,
            cost,
            usage,
            context,
            ..
        } = &events[6]
        else {
            panic!()
        };
        assert_eq!(*outcome, Outcome::Done);
        assert_eq!(*cost, 0.0058);
        assert_eq!(usage.cache_read, 45157);
        let context = context.as_ref().unwrap();
        assert_eq!(context.tokens, Some(2 + 7546 + 31765), "the last answer's");
        assert_eq!(
            (context.window, context.percent),
            (Some(1_000_000), Some(4))
        );
    }

    #[test]
    fn tags_what_subagents_say() {
        let mut parser = ClaudeCode.parser();
        let mut parse = |line: Value| parser.parse(line.to_string().as_bytes());
        let main = json!({"type": "assistant", "parent_tool_use_id": null, "message": {"model": "m", "content": [{"type": "tool_use", "id": "task_1", "name": "Agent", "input": {}}], "usage": {"input_tokens": 100}}});
        assert_eq!(
            parse(main),
            [AgentEvent::ToolUse {
                id: "task_1".into(),
                name: "Agent".into(),
                input: json!({}),
                parent: None,
            }]
        );
        let sub = json!({"type": "assistant", "parent_tool_use_id": "task_1", "message": {"model": "m", "content": [{"type": "text", "text": "looking"}], "usage": {"input_tokens": 9}}});
        assert_eq!(
            parse(sub),
            [AgentEvent::Text {
                text: "looking".into(),
                parent: Some("task_1".into()),
            }]
        );
        let result = json!({"type": "user", "parent_tool_use_id": "task_1", "message": {"content": [{"type": "tool_result", "tool_use_id": "t", "content": "ok"}]}});
        let [AgentEvent::ToolResult { parent, .. }] = &parse(result)[..] else {
            panic!()
        };
        assert_eq!(parent.as_deref(), Some("task_1"));
        let end = parse(
            json!({"type": "result", "subtype": "success", "modelUsage": {"m": {"contextWindow": 1000}}}),
        );
        let [AgentEvent::TurnEnd { context, .. }] = &end[..] else {
            panic!()
        };
        assert_eq!(
            context.as_ref().unwrap().tokens,
            Some(100),
            "the main agent's context"
        );
        let text = serde_json::to_value(AgentEvent::Text {
            text: "hi".into(),
            parent: None,
        })
        .unwrap();
        assert_eq!(text, json!({"kind": "text", "text": "hi"}));
    }

    #[test]
    fn ignores_what_it_does_not_know() {
        let events = parse_all(&[
            "",
            "Welcome to zsh",
            "{\"type\":\"assistant\",\"mess",
            "[1, 2]",
            r#"{"type":"brand_new_kind","payload":{}}"#,
            r#"{"type":"system","subtype":"something_later"}"#,
            r#"{"type":"assistant","message":"not an object"}"#,
            r#"{"type":"user","message":{"content":"[Request interrupted by user]"}}"#,
            r#"{"type":"control_request","request_id":"r","request":{"subtype":"hook_callback"}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"sub"}},"parent_tool_use_id":"toolu_9"}"#,
        ]);
        assert_eq!(events, []);
    }

    #[test]
    fn costs_a_turn_by_difference_and_reads_its_outcome() {
        let mut parser = ClaudeCode.parser();
        let mut end = |line: &str| match parser.parse(line.as_bytes()).pop() {
            Some(AgentEvent::TurnEnd {
                outcome,
                cost,
                error,
                context,
                ..
            }) => (outcome, (cost * 1000.0).round() / 1000.0, error, context),
            other => panic!("{other:?}"),
        };
        let first = end(r#"{"type":"result","subtype":"success","total_cost_usd":0.006}"#);
        assert_eq!((first.0, first.1, first.2), (Outcome::Done, 0.006, None));
        assert_eq!(first.3, None, "no answer, no gauge");
        let interrupted = end(
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"terminal_reason":"aborted_streaming","total_cost_usd":0.007,"errors":["[ede_diagnostic] result_type=user"]}"#,
        );
        assert_eq!(
            (interrupted.0, interrupted.1, interrupted.2),
            (Outcome::Interrupted, 0.001, None)
        );
        let budget = end(
            r#"{"type":"result","subtype":"error_max_budget_usd","is_error":true,"terminal_reason":"budget_exhausted","total_cost_usd":0.012,"errors":["Reached maximum budget ($0.01)"]}"#,
        );
        assert_eq!(budget.0, Outcome::Budget);
        assert_eq!(budget.1, 0.005);
        assert_eq!(budget.2.as_deref(), Some("Reached maximum budget ($0.01)"));
        let api = end(
            r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: 529 overloaded","total_cost_usd":0.012}"#,
        );
        assert_eq!(
            (api.0, api.1, api.2.as_deref()),
            (Outcome::Error, 0.0, Some("API Error: 529 overloaded"))
        );
    }

    #[test]
    fn cuts_long_tool_output() {
        let long = "é".repeat(TOOL_OUTPUT + 10);
        let line = json!({"type": "user", "message": {"content": [
            {"type": "tool_result", "tool_use_id": "t", "content": [{"type": "text", "text": long}, {"type": "image"}], "is_error": true},
        ]}});
        let events = ClaudeCode.parser().parse(line.to_string().as_bytes());
        let [
            AgentEvent::ToolResult {
                output,
                is_error: true,
                truncated: true,
                ..
            },
        ] = &events[..]
        else {
            panic!("{events:?}")
        };
        assert_eq!(output.chars().count(), TOOL_OUTPUT);
    }

    #[test]
    fn writes_stdin_lines() {
        let line: Value = serde_json::from_str(&ClaudeCode.message("hi \"there\"\n")).unwrap();
        assert_eq!(line["message"]["content"], "hi \"there\"\n");
        let interrupt: Value = serde_json::from_str(&ClaudeCode.interrupt().unwrap()).unwrap();
        assert_eq!(interrupt["request"]["subtype"], "interrupt");
        assert_ne!(
            interrupt["request_id"],
            serde_json::from_str::<Value>(&ClaudeCode.interrupt().unwrap()).unwrap()["request_id"]
        );
        let allow: Value =
            serde_json::from_str(&ClaudeCode.answer("r1", true, &json!({"a": 1}), "")).unwrap();
        assert_eq!(
            allow["response"],
            json!({"subtype": "success", "request_id": "r1", "response": {"behavior": "allow", "updatedInput": {"a": 1}}})
        );
        let deny: Value =
            serde_json::from_str(&ClaudeCode.answer("r1", false, &json!({}), "not now")).unwrap();
        assert_eq!(
            deny["response"]["response"],
            json!({"behavior": "deny", "message": "not now"})
        );
    }
}
