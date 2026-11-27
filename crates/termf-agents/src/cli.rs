//! Codex CLI (`codex exec --json`) and Gemini CLI (`gemini --output-format stream-json`), headless: one process
//! per turn, the message on its stdin, its events on stdout (docs/AGENTS-CLI.md). Neither asks for a permission,
//! says what a turn costs, nor answers in a JSON schema: a session that needs it is refused, never faked.

use std::collections::HashSet;

use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use termf_core::claude::Tool;
use termf_core::cli_agents;

use crate::claude::cut;
use crate::{AgentEvent, AgentRuntime, Image, Launch, Outcome, Parser, SessionSpec, Usage};

pub struct Codex;
pub struct Gemini;

/// What a runtime other than Claude Code refuses, whatever its own options.
fn refuse(tool: Tool, spec: &SessionSpec) -> Result<()> {
    let label = tool.label();
    ensure!(
        !spec.mcp,
        "{label} cannot hold a conversation with TermF's tools: the CTO and the expert run on Claude Code"
    );
    ensure!(
        !spec.guard,
        "{label} gets no hook TermF trusts to refuse a push (`termf guard`): a ticket's or a team's session runs on \
         Claude Code"
    );
    ensure!(
        spec.json_schema.is_none(),
        "{label} cannot be held to a JSON schema (`--json-schema`)"
    );
    ensure!(
        spec.budget.is_none(),
        "{label} does not say what a turn costs: no budget can be kept"
    );
    ensure!(
        spec.max_turns.is_none(),
        "{label} has no limit of model calls per message (`--max-turns`)"
    );
    let single: Vec<&str> = spec
        .allowed_tools
        .iter()
        .filter(|t| !t.starts_with("mcp__termf__"))
        .chain(&spec.disallowed_tools)
        .map(String::as_str)
        .collect();
    ensure!(
        single.is_empty(),
        "{label} cannot allow or deny single tools ({}): choose a permission mode instead",
        single.join(", ")
    );
    Ok(())
}

/// The permission modes a runtime maps to its own, the first being the default's.
fn mode<'a>(tool: Tool, spec: &SessionSpec, modes: &[(&str, &'a str)]) -> Result<Option<&'a str>> {
    let Some(asked) = spec.permission_mode.as_deref() else {
        return Ok(None);
    };
    match modes.iter().find(|(m, _)| *m == asked) {
        Some((_, theirs)) => Ok(Some(theirs)),
        None => bail!(
            "{} cannot ask for a permission in a headless run: choose {} rather than {asked}",
            tool.label(),
            modes.iter().map(|(m, _)| *m).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Claude Code's modes as Codex's sandbox: nothing asks in `exec`, so `default` only reads, as `plan`.
const CODEX_MODES: &[(&str, &str)] = &[
    ("default", "read-only"),
    ("plan", "read-only"),
    ("acceptEdits", "workspace-write"),
    ("bypassPermissions", "danger-full-access"),
];

/// Claude Code's modes as Gemini's approval modes; a tool that would ask is refused in a headless run.
const GEMINI_MODES: &[(&str, &str)] = &[
    ("default", "default"),
    ("plan", "plan"),
    ("acceptEdits", "auto_edit"),
    ("bypassPermissions", "yolo"),
];

impl AgentRuntime for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn label(&self) -> &'static str {
        Tool::Codex.label()
    }

    fn per_turn(&self) -> bool {
        true
    }

    // On stdin, as Gemini's: `developer_instructions` on the command line would not fit cmd.exe's 8191 characters.
    fn instructions_in_message(&self) -> bool {
        true
    }

    fn refuse(&self, spec: &SessionSpec) -> Result<()> {
        refuse(Tool::Codex, spec)?;
        mode(Tool::Codex, spec, CODEX_MODES).map(drop)
    }

    fn headless_args(&self, spec: &SessionSpec, launch: &Launch) -> Vec<String> {
        let mut args: Vec<String> = ["exec", "--json", "--skip-git-repo-check"]
            .map(Into::into)
            .to_vec();
        // Before `resume`, which does not take it.
        if let Ok(Some(sandbox)) = mode(Tool::Codex, spec, CODEX_MODES) {
            args.extend(["--sandbox".into(), sandbox.into()]);
        }
        args.extend(codex_options(spec));
        // Its path also marks the process: the first one's command line has no conversation id.
        let last = launch.dir.join("last-message.txt");
        args.extend(["-o".into(), last.to_string_lossy().into_owned()]);
        if let Some(server) = launch.mcp_server {
            for arg in cli_agents::codex_mcp(server, termf_core::ipc::ENV_MCP_TOKEN) {
                args.extend(["-c".into(), arg]);
            }
        }
        if launch.resume {
            args.extend(["resume".into(), launch.session_id.into()]);
        }
        // The message comes on stdin.
        args.push("-".into());
        args
    }

    fn interactive_args(&self, spec: &SessionSpec, session_id: &str) -> Vec<String> {
        let mut args = vec!["resume".into(), session_id.into()];
        args.extend(codex_options(spec));
        args
    }

    fn parser(&self) -> Box<dyn Parser> {
        Box::<CodexStream>::default()
    }

    fn message(&self, text: &str, _: &[Image]) -> String {
        text.to_owned()
    }

    fn interrupt(&self) -> Option<String> {
        None
    }

    fn answer(&self, _: &str, _: bool, _: &Value, _: &str) -> String {
        String::new()
    }
}

fn codex_options(spec: &SessionSpec) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(model) = &spec.model {
        args.extend(["-m".into(), model.clone()]);
    }
    if let Some(effort) = &spec.effort {
        args.extend(["-c".into(), format!("model_reasoning_effort={effort}")]);
    }
    args
}

impl AgentRuntime for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn label(&self) -> &'static str {
        Tool::Gemini.label()
    }

    fn per_turn(&self) -> bool {
        true
    }

    fn instructions_in_message(&self) -> bool {
        true
    }

    fn refuse(&self, spec: &SessionSpec) -> Result<()> {
        refuse(Tool::Gemini, spec)?;
        ensure!(
            spec.effort.is_none(),
            "Gemini CLI has no reasoning effort to set"
        );
        mode(Tool::Gemini, spec, GEMINI_MODES).map(drop)
    }

    fn headless_args(&self, spec: &SessionSpec, launch: &Launch) -> Vec<String> {
        let mut args: Vec<String> = ["--output-format", "stream-json", "--prompt", ""]
            .map(Into::into)
            .to_vec();
        args.extend(gemini_options(spec));
        if let Some(policy) = launch.policy {
            args.extend(["--policy".into(), policy.to_string_lossy().into_owned()]);
        }
        // Gemini takes the id TermF chooses, then resumes it.
        let id = if launch.resume {
            "--resume"
        } else {
            "--session-id"
        };
        args.extend([id.into(), launch.session_id.into()]);
        args
    }

    fn interactive_args(&self, spec: &SessionSpec, session_id: &str) -> Vec<String> {
        let mut args = vec!["--resume".into(), session_id.into()];
        args.extend(gemini_options(spec));
        args
    }

    fn parser(&self) -> Box<dyn Parser> {
        Box::<GeminiStream>::default()
    }

    fn message(&self, text: &str, _: &[Image]) -> String {
        text.to_owned()
    }

    fn interrupt(&self) -> Option<String> {
        None
    }

    fn answer(&self, _: &str, _: bool, _: &Value, _: &str) -> String {
        String::new()
    }
}

fn gemini_options(spec: &SessionSpec) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(model) = &spec.model {
        args.extend(["--model".into(), model.clone()]);
    }
    if let Ok(Some(mode)) = mode(Tool::Gemini, spec, GEMINI_MODES) {
        args.extend(["--approval-mode".into(), mode.into()]);
    }
    args
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn turn_end(
    outcome: Outcome,
    result: Option<String>,
    error: Option<String>,
    usage: Usage,
) -> AgentEvent {
    AgentEvent::TurnEnd {
        outcome,
        result,
        structured: None,
        error,
        usage,
        cost: 0.0,
        total: 0.0,
        context: None,
    }
}

/// `codex exec --json`: `thread.started`, `turn.*` and `item.*` events (codex-rs/exec/src/exec_events.rs).
#[derive(Default)]
struct CodexStream {
    /// Tool items already shown as started.
    started: HashSet<String>,
    said: Option<String>,
    error: Option<String>,
}

impl CodexStream {
    /// A tool call from an item, or `None` for what is no tool call.
    fn call(item: &Value) -> Option<(String, Value)> {
        Some(match item["type"].as_str()? {
            "command_execution" => ("shell".into(), json!({ "command": item["command"] })),
            "file_change" => ("file_change".into(), json!({ "changes": item["changes"] })),
            "mcp_tool_call" => (
                format!("mcp__{}__{}", text(&item["server"]), text(&item["tool"])),
                item["arguments"].clone(),
            ),
            "web_search" => ("web_search".into(), json!({ "query": item["query"] })),
            _ => return None,
        })
    }

    fn result(item: &Value) -> (String, bool) {
        let failed = matches!(item["status"].as_str(), Some("failed" | "declined"));
        match item["type"].as_str().unwrap_or_default() {
            "command_execution" => (
                text(&item["aggregated_output"]),
                failed || item["exit_code"].as_i64().is_some_and(|c| c != 0),
            ),
            "file_change" => {
                let changes = item["changes"].as_array().into_iter().flatten();
                let lines: Vec<String> = changes
                    .map(|c| format!("{} {}", text(&c["kind"]), text(&c["path"])))
                    .collect();
                (lines.join("\n"), failed)
            }
            "mcp_tool_call" if !item["error"].is_null() => (text(&item["error"]["message"]), true),
            "mcp_tool_call" => {
                let content = item["result"]["content"].as_array().into_iter().flatten();
                let texts: Vec<String> = content.map(|c| text(&c["text"])).collect();
                (texts.join("\n"), failed)
            }
            _ => (String::new(), failed),
        }
    }
}

impl Parser for CodexStream {
    fn parse(&mut self, line: &[u8]) -> Vec<AgentEvent> {
        let Ok(msg) = serde_json::from_slice::<Value>(line) else {
            return Vec::new();
        };
        let item = &msg["item"];
        let id = text(&item["id"]);
        match msg["type"].as_str().unwrap_or_default() {
            "thread.started" => vec![AgentEvent::Init {
                session_id: text(&msg["thread_id"]),
                model: String::new(),
                permission_mode: String::new(),
            }],
            "item.started" => match Self::call(item) {
                Some((name, input)) if self.started.insert(id.clone()) => {
                    vec![AgentEvent::ToolUse {
                        id,
                        name,
                        input,
                        parent: None,
                    }]
                }
                _ => Vec::new(),
            },
            "item.completed" => match item["type"].as_str().unwrap_or_default() {
                "agent_message" if !text(&item["text"]).is_empty() => {
                    self.said = Some(text(&item["text"]));
                    vec![AgentEvent::Text {
                        text: text(&item["text"]),
                        parent: None,
                    }]
                }
                _ => {
                    let Some((name, input)) = Self::call(item) else {
                        return Vec::new();
                    };
                    let mut events = Vec::new();
                    if self.started.insert(id.clone()) {
                        events.push(AgentEvent::ToolUse {
                            id: id.clone(),
                            name,
                            input,
                            parent: None,
                        });
                    }
                    let (output, is_error) = Self::result(item);
                    let (output, truncated) = cut(&output);
                    events.push(AgentEvent::ToolResult {
                        id,
                        output,
                        is_error,
                        truncated,
                        parent: None,
                    });
                    events
                }
            },
            "error" => {
                self.error = Some(text(&msg["message"]));
                Vec::new()
            }
            "turn.completed" => {
                let usage = &msg["usage"];
                let count = |key: &str| usage[key].as_u64().unwrap_or(0);
                vec![turn_end(
                    Outcome::Done,
                    self.said.take(),
                    None,
                    Usage {
                        input: count("input_tokens"),
                        output: count("output_tokens"),
                        cache_read: count("cached_input_tokens"),
                        cache_creation: count("cache_write_input_tokens"),
                    },
                )]
            }
            "turn.failed" => {
                let error = Some(text(&msg["error"]["message"]))
                    .filter(|e| !e.is_empty())
                    .or(self.error.take());
                vec![turn_end(
                    Outcome::Error,
                    self.said.take(),
                    error,
                    Usage::default(),
                )]
            }
            _ => Vec::new(),
        }
    }
}

/// `gemini --output-format stream-json`: `init`, `message`, `tool_use`, `tool_result`, `error`, `result`
/// (packages/core/src/output/types.ts). The answer comes in chunks: TermF makes one text of them.
#[derive(Default)]
struct GeminiStream {
    text: String,
    said: Option<String>,
    error: Option<String>,
}

impl GeminiStream {
    fn flush(&mut self, events: &mut Vec<AgentEvent>) {
        if !self.text.trim().is_empty() {
            let text = std::mem::take(&mut self.text);
            self.said = Some(text.clone());
            events.push(AgentEvent::Text { text, parent: None });
        }
        self.text.clear();
    }
}

impl Parser for GeminiStream {
    fn parse(&mut self, line: &[u8]) -> Vec<AgentEvent> {
        let Ok(msg) = serde_json::from_slice::<Value>(line) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        match msg["type"].as_str().unwrap_or_default() {
            "init" => events.push(AgentEvent::Init {
                session_id: text(&msg["session_id"]),
                model: text(&msg["model"]),
                permission_mode: String::new(),
            }),
            "message" if msg["role"] == "assistant" => {
                let chunk = text(&msg["content"]);
                self.text.push_str(&chunk);
                events.push(AgentEvent::Delta { text: chunk });
            }
            "tool_use" => {
                self.flush(&mut events);
                events.push(AgentEvent::ToolUse {
                    id: text(&msg["tool_id"]),
                    name: text(&msg["tool_name"]),
                    input: msg["parameters"].clone(),
                    parent: None,
                });
            }
            "tool_result" => {
                let is_error = msg["status"] == "error";
                let output = match msg["output"].as_str() {
                    Some(out) => out.to_owned(),
                    None => text(&msg["error"]["message"]),
                };
                let (output, truncated) = cut(&output);
                events.push(AgentEvent::ToolResult {
                    id: text(&msg["tool_id"]),
                    output,
                    is_error,
                    truncated,
                    parent: None,
                });
            }
            "error" if msg["severity"] != "warning" => self.error = Some(text(&msg["message"])),
            "result" => {
                self.flush(&mut events);
                let stats = &msg["stats"];
                let count = |key: &str| stats[key].as_u64().unwrap_or(0);
                let failed = msg["status"] != "success";
                let error = failed.then(|| {
                    let said = msg["error"]["message"].as_str().or(msg["error"].as_str());
                    said.map(str::to_owned)
                        .or(self.error.take())
                        .unwrap_or_else(|| "the turn failed".into())
                });
                let outcome = if failed {
                    Outcome::Error
                } else {
                    Outcome::Done
                };
                events.push(turn_end(
                    outcome,
                    self.said.take(),
                    error,
                    Usage {
                        input: count("input_tokens"),
                        output: count("output_tokens"),
                        cache_read: count("cached"),
                        cache_creation: 0,
                    },
                ));
            }
            _ => {}
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(events: &[AgentEvent]) -> Vec<String> {
        events
            .iter()
            .map(|e| {
                serde_json::to_value(e).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }

    fn parse(runtime: &dyn AgentRuntime, lines: &[&str]) -> Vec<AgentEvent> {
        let mut parser = runtime.parser();
        lines
            .iter()
            .flat_map(|l| parser.parse(l.as_bytes()))
            .collect()
    }

    /// Lines from the Codex documentation (developers.openai.com/codex/noninteractive, 2026-11-22), and the item
    /// fields of codex-rs/exec/src/exec_events.rs.
    #[test]
    fn reads_a_codex_turn() {
        let events = parse(
            &Codex,
            &[
                r#"{"type":"thread.started","thread_id":"0199a213-81c0-7800-8aa1-bbab2a035a53"}"#,
                r#"{"type":"turn.started"}"#,
                r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","status":"in_progress"}}"#,
                r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","aggregated_output":"docs\nsdk\n","exit_code":0,"status":"completed"}}"#,
                r#"{"type":"item.completed","item":{"id":"item_2","type":"reasoning","text":"**Listing**"}}"#,
                r#"{"type":"item.completed","item":{"id":"item_4","type":"file_change","changes":[{"path":"src/a.rs","kind":"update"}],"status":"completed"}}"#,
                r#"{"type":"item.completed","item":{"id":"item_3","type":"agent_message","text":"Repo contains docs, sdk, and examples directories."}}"#,
                "not json",
                r#"{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122,"reasoning_output_tokens":0}}"#,
            ],
        );
        assert_eq!(
            kinds(&events),
            [
                "init",
                "tool_use",
                "tool_result",
                "tool_use",
                "tool_result",
                "text",
                "turn_end"
            ]
        );
        let AgentEvent::Init { session_id, .. } = &events[0] else {
            panic!()
        };
        assert_eq!(session_id, "0199a213-81c0-7800-8aa1-bbab2a035a53");
        assert_eq!(
            events[1],
            AgentEvent::ToolUse {
                id: "item_1".into(),
                name: "shell".into(),
                input: json!({"command": "bash -lc ls"}),
                parent: None
            }
        );
        let AgentEvent::ToolResult {
            output,
            is_error: false,
            ..
        } = &events[4]
        else {
            panic!()
        };
        assert_eq!(output, "update src/a.rs");
        let AgentEvent::TurnEnd {
            outcome,
            result,
            usage,
            cost,
            context,
            ..
        } = &events[6]
        else {
            panic!()
        };
        assert_eq!(*outcome, Outcome::Done);
        assert_eq!(
            result.as_deref(),
            Some("Repo contains docs, sdk, and examples directories.")
        );
        assert_eq!(
            (usage.input, usage.output, usage.cache_read),
            (24763, 122, 24448)
        );
        assert_eq!(
            (*cost, context),
            (0.0, &None),
            "Codex says no cost and no window"
        );

        let failed = parse(
            &Codex,
            &[
                r#"{"type":"error","message":"stream disconnected"}"#,
                r#"{"type":"item.completed","item":{"id":"c","type":"command_execution","command":"false","aggregated_output":"","exit_code":1,"status":"failed"}}"#,
                r#"{"type":"turn.failed","error":{"message":""}}"#,
            ],
        );
        assert_eq!(kinds(&failed), ["tool_use", "tool_result", "turn_end"]);
        let AgentEvent::ToolResult { is_error: true, .. } = &failed[1] else {
            panic!()
        };
        let AgentEvent::TurnEnd {
            outcome: Outcome::Error,
            error,
            ..
        } = &failed[2]
        else {
            panic!()
        };
        assert_eq!(error.as_deref(), Some("stream disconnected"));
    }

    /// The stream-json objects of gemini-cli's packages/core/src/output/stream-json-formatter.test.ts.
    #[test]
    fn reads_a_gemini_turn() {
        let events = parse(
            &Gemini,
            &[
                r#"{"type":"init","timestamp":"2025-10-10T12:00:00.000Z","session_id":"test-session-123","model":"gemini-2.0-flash-exp"}"#,
                r#"{"type":"message","timestamp":"2025-10-10T12:00:00.000Z","role":"user","content":"Read the file"}"#,
                r#"{"type":"message","timestamp":"2025-10-10T12:00:00.000Z","role":"assistant","content":"Reading ","delta":true}"#,
                r#"{"type":"message","timestamp":"2025-10-10T12:00:00.000Z","role":"assistant","content":"it.","delta":true}"#,
                r#"{"type":"tool_use","timestamp":"2025-10-10T12:00:00.000Z","tool_name":"Read","tool_id":"read-123","parameters":{"file_path":"/path/to/file.txt"}}"#,
                r#"{"type":"tool_result","timestamp":"2025-10-10T12:00:00.000Z","tool_id":"read-123","status":"success","output":"File contents here"}"#,
                r#"{"type":"message","timestamp":"2025-10-10T12:00:00.000Z","role":"assistant","content":"4","delta":true}"#,
                r#"{"type":"error","timestamp":"2025-10-10T12:00:00.000Z","severity":"warning","message":"Loop detected"}"#,
                r#"{"type":"result","timestamp":"2025-10-10T12:00:00.000Z","status":"success","stats":{"total_tokens":100,"input_tokens":50,"output_tokens":50,"cached":0,"input":50,"duration_ms":1200,"tool_calls":2,"models":{}}}"#,
            ],
        );
        assert_eq!(
            kinds(&events),
            [
                "init",
                "delta",
                "delta",
                "text",
                "tool_use",
                "tool_result",
                "delta",
                "text",
                "turn_end"
            ]
        );
        assert_eq!(
            events[3],
            AgentEvent::Text {
                text: "Reading it.".into(),
                parent: None
            }
        );
        let AgentEvent::TurnEnd {
            outcome,
            result,
            usage,
            ..
        } = &events[8]
        else {
            panic!()
        };
        assert_eq!(*outcome, Outcome::Done);
        assert_eq!(result.as_deref(), Some("4"));
        assert_eq!((usage.input, usage.output), (50, 50));

        let failed = parse(
            &Gemini,
            &[
                r#"{"type":"tool_result","tool_id":"t","status":"error","error":{"type":"x","message":"denied by policy"}}"#,
                r#"{"type":"error","severity":"error","message":"quota exceeded"}"#,
                r#"{"type":"result","status":"error","stats":{}}"#,
            ],
        );
        let AgentEvent::ToolResult {
            output,
            is_error: true,
            ..
        } = &failed[0]
        else {
            panic!()
        };
        assert_eq!(output, "denied by policy");
        let AgentEvent::TurnEnd {
            outcome: Outcome::Error,
            error,
            ..
        } = &failed[1]
        else {
            panic!()
        };
        assert_eq!(error.as_deref(), Some("quota exceeded"));
    }

    #[test]
    fn refuses_what_they_cannot_do() {
        let spec = SessionSpec::default;
        let refused = |runtime: &dyn AgentRuntime, change: fn(&mut SessionSpec)| {
            let mut s = spec();
            change(&mut s);
            runtime.refuse(&s).unwrap_err().to_string()
        };
        for runtime in [&Codex as &dyn AgentRuntime, &Gemini] {
            assert!(runtime.refuse(&spec()).is_ok());
            assert!(refused(runtime, |s| s.json_schema = Some(json!({}))).contains("JSON schema"));
            assert!(refused(runtime, |s| s.guard = true).contains("termf guard"));
            assert!(refused(runtime, |s| s.mcp = true).contains("Claude Code"));
            assert!(refused(runtime, |s| s.budget = Some(1.0)).contains("cost"));
            assert!(
                refused(runtime, |s| s.disallowed_tools = vec!["Edit".into()]).contains("Edit")
            );
            assert!(
                refused(runtime, |s| s.permission_mode = Some("manual".into()))
                    .contains("cannot ask")
            );
        }
        let mut browser = spec();
        browser.allowed_tools = vec!["mcp__termf__browser_*".into()];
        assert!(
            Codex.refuse(&browser).is_ok(),
            "TermF's tools come through MCP"
        );
        assert!(refused(&Gemini, |s| s.effort = Some("high".into())).contains("effort"));
        let mut codex = spec();
        codex.effort = Some("high".into());
        assert!(Codex.refuse(&codex).is_ok());
    }

    #[test]
    fn starts_and_resumes_a_turn() {
        let prompt = std::env::temp_dir().join(format!("termf-cli-prompt-{}", std::process::id()));
        std::fs::write(&prompt, "## Role: front").unwrap();
        let spec = SessionSpec {
            model: Some("gpt-5.5-codex".into()),
            effort: Some("high".into()),
            permission_mode: Some("acceptEdits".into()),
            ..SessionSpec::default()
        };
        let tmp = std::env::temp_dir();
        let launch = |resume| Launch {
            session_id: "t-1",
            resume,
            budget: None,
            plugin: None,
            system_prompt: Some(&prompt),
            mcp: None,
            mcp_server: None,
            dirs: &[],
            guard: None,
            policy: None,
            dir: &tmp,
            connectors: &Value::Null,
        };
        let first = Codex.headless_args(&spec, &launch(false));
        assert_eq!(
            &first[..5],
            [
                "exec",
                "--json",
                "--skip-git-repo-check",
                "--sandbox",
                "workspace-write"
            ]
        );
        assert!(first.contains(&"model_reasoning_effort=high".to_owned()));
        assert!(
            !first.iter().any(|a| a.contains("Role")),
            "Codex reads them in the message"
        );
        assert_eq!(
            first.iter().position(|a| a == "-o").map(|i| &first[i + 1]),
            Some(
                &std::env::temp_dir()
                    .join("last-message.txt")
                    .to_string_lossy()
                    .into_owned()
            )
        );
        assert_eq!(first.last().unwrap(), "-");
        assert!(!first.contains(&"resume".to_owned()));
        let next = Codex.headless_args(&spec, &launch(true));
        assert_eq!(&next[next.len() - 3..], ["resume", "t-1", "-"]);
        assert_eq!(Codex.interactive_args(&spec, "t-1")[..2], ["resume", "t-1"]);

        let spec = SessionSpec {
            model: Some("gemini-3-pro".into()),
            permission_mode: Some("plan".into()),
            ..SessionSpec::default()
        };
        let first = Gemini.headless_args(&spec, &launch(false));
        assert!(first.windows(2).any(|w| w == ["--session-id", "t-1"]));
        assert!(first.windows(2).any(|w| w == ["--approval-mode", "plan"]));
        assert!(
            !first.iter().any(|a| a.contains("Role")),
            "Gemini reads them in the message"
        );
        let next = Gemini.headless_args(&spec, &launch(true));
        assert!(next.windows(2).any(|w| w == ["--resume", "t-1"]));
        std::fs::remove_file(&prompt).unwrap();
    }
}
