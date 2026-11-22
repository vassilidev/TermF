//! Agent runtimes (ARCHITECTURE §6). A headless session is Claude Code driven through stream-json: TermF writes
//! the user's messages on its stdin and reads typed events on its stdout, stored in `termf.db` and in
//! `agents/<id>/events.jsonl`. Taking a session over stops that process between two turns and reopens the same
//! conversation in a terminal; handing it back closes the terminal and resumes headless. A conversation never
//! has two processes at once: Claude Code would lose messages.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use termf_core::context::Gauge;

mod agents;
mod claude;
mod store;
// The fake Claude Code is a shell script.
#[cfg(all(test, unix))]
mod tests;

pub use agents::{Agents, Takeover};
pub use claude::{ClaudeCode, TOOL_OUTPUT, mcp_settings};
pub use store::{events, sessions};

/// An agent CLI TermF drives (ARCHITECTURE §6.1); only Claude Code so far.
pub trait AgentRuntime: Send + Sync {
    /// `claude-code`.
    fn id(&self) -> &'static str;
    /// The options of a headless process, added after the agent's command.
    fn headless_args(&self, spec: &SessionSpec, launch: &Launch) -> Vec<String>;
    /// The options that reopen the conversation in a terminal.
    fn interactive_args(&self, spec: &SessionSpec, session_id: &str) -> Vec<String>;
    /// Reads one process's output, line by line.
    fn parser(&self) -> Box<dyn Parser>;
    /// The stdin line of a user message.
    fn message(&self, text: &str) -> String;
    /// The stdin line that stops the turn under way.
    fn interrupt(&self) -> String;
    /// The stdin line that answers a permission request; `input` is the tool input it asked about.
    fn answer(&self, request: &str, allow: bool, input: &Value, message: &str) -> String;
}

/// One per process: the end of a turn adds up what several lines said.
pub trait Parser: Send {
    /// A line it does not understand gives nothing: the format changes between versions.
    fn parse(&mut self, line: &[u8]) -> Vec<AgentEvent>;
}

/// What a process starts with, besides its session's spec.
pub struct Launch<'a> {
    /// The conversation's own id.
    pub session_id: &'a str,
    /// Continue the conversation rather than start it.
    pub resume: bool,
    /// What is left of the session's budget, in US dollars.
    pub budget: Option<f64>,
    /// The library's plugin and system prompt, when it has them.
    pub plugin: Option<&'a Path>,
    pub system_prompt: Option<&'a Path>,
    /// The config of `termf mcp` for a session with TermF's tools, and the other folders it may read.
    pub mcp: Option<&'a Path>,
    pub dirs: &'a [PathBuf],
    /// The `termf` CLI whose `guard` the session's Bash commands go through.
    pub guard: Option<&'a Path>,
}

/// What a session runs with. Empty values mean the agent's default.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionSpec {
    /// The folder the agent works in.
    pub cwd: PathBuf,
    /// The folder's name unless set.
    pub title: String,
    /// The project whose library applies; by default the one that lists the folder.
    pub project: Option<String>,
    /// The `[agents]` entry to run, which must run Claude Code; by default the first that does.
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// `--permission-mode`.
    pub permission_mode: Option<String>,
    /// In US dollars, for the whole session, every process included.
    pub budget: Option<f64>,
    /// The library role it plays (ARCHITECTURE §7.3): its prompt, and its model, effort, permission mode and
    /// tools unless set here; then only the preferences and rules that apply to it.
    pub role: Option<String>,
    /// The squad of the project it works in: its settings for the role, and the preferences and rules for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub squad: Option<String>,
    /// `--allowedTools` and `--disallowedTools`, the role's included.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disallowed_tools: Vec<String>,
    /// Rule levels (`off`, `advise`, `warn`, `block`) over the library's.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub rules: BTreeMap<String, String>,
    /// `--json-schema`: the agent's answer at the end of each turn follows it (`turn_end.structured`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<Value>,
    /// TermF's tools (`termf mcp`) and only the read tools of the project's repositories, each action asked to
    /// the user first: a project's CTO conversation. No terminal takes it over: its tools would not follow.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mcp: bool,
    /// Each Bash command goes through `termf guard` first, as in a ticket run's tile: no push, no pull request,
    /// no write to the tracker or the CI. Such a session never starts without it, nor in a terminal.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub guard: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// A turn is under way.
    Working,
    /// A tool waits for an approval.
    Waiting,
    /// Waiting for the next message.
    Idle,
    /// Taken over in a terminal.
    Interactive,
    /// No process: the next message resumes the conversation.
    #[default]
    Stopped,
    /// The last turn or process ended on an error; the next message resumes.
    Error,
    /// Its budget is spent.
    Budget,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Working => "working",
            State::Waiting => "waiting",
            State::Idle => "idle",
            State::Interactive => "interactive",
            State::Stopped => "stopped",
            State::Error => "error",
            State::Budget => "budget",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        serde_json::from_value(Value::String(text.into())).ok()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: i64,
    pub runtime: String,
    /// The conversation's own id, the one `claude --resume` takes.
    pub session_id: String,
    #[serde(flatten)]
    pub spec: SessionSpec,
    pub state: State,
    /// Spent so far, in US dollars, every headless process included (a terminal's spending is not seen).
    pub cost: f64,
    /// After the last turn.
    pub context: Option<Gauge>,
    /// The terminal of a session taken over, while it runs, for this launch of the app.
    pub tile: Option<u32>,
    /// Seconds since the Unix epoch.
    pub created: i64,
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Interrupted,
    Error,
    /// `--max-budget-usd` stopped it.
    Budget,
}

/// What a turn used, all its calls to the model added up.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AgentEvent {
    /// A process started; `resumed`: it continues the conversation.
    Started {
        resumed: bool,
    },
    /// What the agent says once it starts working.
    Init {
        session_id: String,
        model: String,
        permission_mode: String,
    },
    /// A message sent to the agent.
    User {
        text: String,
    },
    /// Text as it streams: sent to the UI, never stored; the `text` event that follows holds all of it.
    Delta {
        text: String,
    },
    /// `parent`, here and in tool events: the tool call of the subagent that said it, none for the agent itself.
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
    },
    /// The tool call's id goes out as `call`: as `id`, it would clash with the stored event's own once flattened
    /// (`alias`: events stored before).
    ToolUse {
        #[serde(rename = "call", alias = "id")]
        id: String,
        name: String,
        input: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
    },
    /// `output` keeps the first [`TOOL_OUTPUT`] characters when `truncated`.
    ToolResult {
        #[serde(rename = "call", alias = "id")]
        id: String,
        output: String,
        is_error: bool,
        truncated: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
    },
    /// A tool waits for an approval: answer with `request`.
    Permission {
        request: String,
        tool: String,
        input: Value,
        description: Option<String>,
    },
    PermissionAnswer {
        request: String,
        allow: bool,
        message: Option<String>,
    },
    /// A tool the permission mode refused without asking.
    Denied {
        tool: String,
        message: String,
    },
    TurnEnd {
        outcome: Outcome,
        /// The agent's last answer.
        result: Option<String>,
        /// That answer, when the session has a JSON schema.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structured: Option<Value>,
        error: Option<String>,
        usage: Usage,
        /// This turn's cost, and the session's so far, in US dollars.
        cost: f64,
        total: f64,
        context: Option<Gauge>,
    },
    Error {
        message: String,
    },
    /// The process ended.
    Exited {
        code: Option<i32>,
    },
    /// The conversation went on in this terminal session.
    TakenOver {
        tile: u32,
    },
    HandedBack,
    /// From TermF, not from the user.
    Notice(Notice),
}

/// A message from TermF in a conversation: something waits for the user, or what the user wrote to another
/// agent. The agent reads `text`; the UI says it from the other fields.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    /// `waiting` (a gate), `blocked` (a step failed or asks something), `review` (a round of the code review ended),
    /// `step` or `direct` (the user wrote to a run's step or to a direct conversation).
    pub about: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The agent session it is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<i64>,
    /// Why a step waits, or what the user wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub text: String,
}

impl AgentEvent {
    /// One line for the CLI.
    pub fn describe(&self) -> String {
        match self {
            AgentEvent::Started { resumed: true } => "process started, conversation resumed".into(),
            AgentEvent::Started { resumed: false } => "process started".into(),
            AgentEvent::Init {
                model,
                permission_mode,
                ..
            } => format!("{model}, permission mode {permission_mode}"),
            AgentEvent::User { text } => format!("you: {text}"),
            AgentEvent::Delta { text } | AgentEvent::Text { text, .. } => text.clone(),
            AgentEvent::ToolUse { name, input, .. } => format!("{name} {input}"),
            AgentEvent::ToolResult {
                output, is_error, ..
            } => {
                let first = output.lines().next().unwrap_or_default();
                format!("{} {first}", if *is_error { "failed:" } else { "→" })
            }
            AgentEvent::Permission {
                request,
                tool,
                input,
                ..
            } => format!("asks to use {tool} {input} (request {request})"),
            AgentEvent::PermissionAnswer { allow, .. } => {
                if *allow { "allowed" } else { "denied" }.into()
            }
            AgentEvent::Denied { tool, message } => format!("{tool} refused: {message}"),
            AgentEvent::TurnEnd {
                outcome,
                error,
                cost,
                total,
                context,
                ..
            } => {
                let percent = context.as_ref().and_then(|c| c.percent);
                let context = percent.map(|p| format!(", context {p}%"));
                let error = error.as_deref().map(|e| format!(": {e}"));
                format!(
                    "turn {}{} (${cost:.4}, session ${total:.4}{})",
                    format!("{outcome:?}").to_lowercase(),
                    error.unwrap_or_default(),
                    context.unwrap_or_default()
                )
            }
            AgentEvent::Error { message } => format!("error: {message}"),
            AgentEvent::Exited { code } => match code {
                Some(code) => format!("process exited ({code})"),
                None => "process stopped".into(),
            },
            AgentEvent::TakenOver { tile } => format!("taken over in terminal {tile}"),
            AgentEvent::HandedBack => "handed back".into(),
            AgentEvent::Notice(notice) => format!("termf: {}", notice.text),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StoredEvent {
    /// 0 for a `delta`, which is never stored.
    pub id: i64,
    pub session: i64,
    /// Seconds since the Unix epoch.
    pub at: i64,
    #[serde(flatten)]
    pub event: AgentEvent,
}

/// What the UI hears about.
// A few a second, each sent on at once: boxing would only add an allocation.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Update {
    Event(StoredEvent),
    Session(AgentSession),
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
