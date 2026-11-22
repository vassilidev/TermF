//! Local API shared by the app and the `termf` CLI: one JSON object per line over a local socket.
//! Every request carries a token; a watch request turns the connection into an event stream.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ENV_SOCKET: &str = "TERMF_SOCKET";
/// A session's own token, which only reports that session's hooks.
pub const ENV_TOKEN: &str = "TERMF_TOKEN";
pub const ENV_SESSION: &str = "TERMF_SESSION_ID";
/// What `termf mcp` asks with: the agent session it serves, and that session's own token, which only allows
/// [`Call::Mcp`] for it.
pub const ENV_MCP_SESSION: &str = "TERMF_MCP_SESSION";
pub const ENV_MCP_TOKEN: &str = "TERMF_MCP_TOKEN";
/// The tools of `termf mcp` its session may call, comma-separated; TermF sets it from the session (see
/// `termf_agents::mcp_tools`).
pub const ENV_MCP_TOOLS: &str = "TERMF_MCP_TOOLS";

/// The tools of `termf mcp` that only read, and those that act: each action waits for the user's approval
/// (an `ask` rule in the session's settings).
pub const MCP_READ: [&str; 7] = [
    "project",
    "team_runs",
    "team_run",
    "ticket_runs",
    "inbox",
    "waiting",
    "ask_expert",
];
/// The project expert's own tools: searching and reading the project memory.
pub const MCP_EXPERT: [&str; 2] = ["memory_search", "memory_read"];
/// The browser `termf mcp` runs itself for its session (never through TermF's socket): a headless Chrome on a
/// throwaway profile, kept to the worktree's dev server, localhost and the origins the project allows.
pub const MCP_BROWSER: [&str; 8] = [
    "browser_open",
    "browser_snapshot",
    "browser_click",
    "browser_fill",
    "browser_wait",
    "browser_console",
    "browser_network",
    "browser_screenshot",
];
/// TermF's folders, for the browser of `termf mcp`: the project's config, the worktree's ports.
pub const ENV_CONFIG_DIR: &str = "TERMF_CONFIG_DIR";
pub const ENV_DATA_DIR: &str = "TERMF_DATA_DIR";
pub const MCP_ACTIONS: [&str; 6] = [
    "start_team_run",
    "answer",
    "approve",
    "request_changes",
    "stop_team_run",
    "send_message",
];

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Request {
    pub token: String,
    #[serde(flatten)]
    pub call: Call,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Call {
    List,
    /// `agent: None` opens a login shell.
    Open {
        path: String,
        agent: Option<String>,
    },
    /// `session` is an id or a unique part of a title.
    Say {
        session: String,
        text: String,
    },
    Watch,
    Hook {
        session: u32,
        event: String,
        payload: Value,
    },
    /// Starts a run on a ticket (its URL, `owner/repo#42`, `repo#42` or `#42`): a worktree and a Claude Code session.
    Run {
        ticket: String,
        project: Option<String>,
        /// A repository name or path, when the project has several.
        repo: Option<String>,
    },
    /// Commits, pushes and opens the pull request of a run. `confirm`: the fingerprint of the delivery preview
    /// the user approved; refused if the worktree changed since.
    Deliver {
        run: i64,
        confirm: String,
    },
    /// Removes a run's worktree; `force` drops the work that is not pushed.
    Close {
        run: i64,
        force: bool,
    },
    /// Opens a new session that resumes a run's Claude Code conversation, when it has none.
    Resume {
        run: i64,
    },
    /// Runs a scheduled task now; `scheduled`: only if one of its times is due and not run yet.
    Schedule {
        task: String,
        scheduled: bool,
    },
    /// Drives a headless agent session (crate `termf-agents`).
    Agent(AgentCall),
    /// Drives the agent team of a ticket run (crate `termf-team`).
    Team(TeamCall),
    /// The app's vault session (crate `termf-vault`). Never a secret's value: whoever holds the token may be
    /// an agent.
    Vault(VaultCall),
    /// Visual reviews before / after (crate `termf-visual`).
    Review(ReviewCall),
    /// The project memory and its expert (crate `termf-memory`).
    Memory(MemoryCall),
    /// A tool of `termf mcp`, for the agent session it serves, within that session's project.
    Mcp {
        session: i64,
        tool: McpCall,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum MemoryCall {
    /// Brings a project's index up to date, or every project's.
    Reindex { project: Option<String> },
    /// A question to the project's expert, answered by a headless session TermF starts and waits for.
    Ask {
        project: String,
        question: String,
        budget: Option<f64>,
    },
}

/// The tools of `termf mcp` (names in [`MCP_READ`], [`MCP_ACTIONS`] and [`MCP_EXPERT`]). None delivers, closes a run, pushes,
/// or reaches the vault.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "tool", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpCall {
    // Braces, so that a field they do not take is refused as for the others.
    Project {},
    TeamRuns {},
    TeamRun {
        run: i64,
    },
    TicketRuns {},
    Inbox {},
    Waiting {},
    /// The team of a ticket run (`run`), of a ticket that gets a run (`ticket`, `repo`), or of a request
    /// without a ticket (`request`).
    StartTeamRun {
        #[serde(default)]
        run: Option<i64>,
        #[serde(default)]
        ticket: Option<String>,
        #[serde(default)]
        request: Option<String>,
        #[serde(default)]
        repo: Option<String>,
        #[serde(default)]
        workflow: Option<String>,
        #[serde(default)]
        budget: Option<f64>,
    },
    /// `answers`: question id → answer (a text, or a list for a multiple choice) for a step's questionnaire, or
    /// a text for questions that have no id; `note`: anything else.
    Answer {
        run: i64,
        step: String,
        answers: Value,
        #[serde(default)]
        note: Option<String>,
    },
    Approve {
        run: i64,
        step: String,
    },
    RequestChanges {
        run: i64,
        step: String,
        feedback: String,
    },
    StopTeamRun {
        run: i64,
    },
    /// To the session of a run's step, or to a role: its step at work, else a direct conversation.
    SendMessage {
        #[serde(default)]
        run: Option<i64>,
        #[serde(default)]
        step: Option<String>,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        repo: Option<String>,
        text: String,
    },
    /// The project memory's passages that best match `query`, from `sources` (all when empty).
    MemorySearch {
        query: String,
        #[serde(default)]
        sources: Vec<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    /// One document of the project memory, from `offset`.
    MemoryRead {
        id: i64,
        #[serde(default)]
        offset: Option<usize>,
    },
    /// A question to the project's expert, answered with its sources.
    AskExpert {
        question: String,
    },
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum VaultCall {
    Status,
    /// `pin`: the security key's PIN, when security keys are enrolled and TermF asks for it (macOS, Linux).
    Unlock {
        password: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pin: Option<String>,
    },
    Lock,
}

impl std::fmt::Debug for VaultCall {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Self::Status => "Status",
            Self::Unlock { .. } => "Unlock",
            Self::Lock => "Lock",
        })
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReviewCall {
    /// `request`: a `termf_visual::Request`.
    Start { request: Value },
    /// `screen`: a screen id, or `all` for every pending one; `decision`: approve, reject or changes; `pins`:
    /// `termf_visual::Pin`s.
    Decide {
        review: i64,
        screen: String,
        decision: String,
        comment: Option<String>,
        pins: Option<Value>,
    },
    /// Posts the images on the run's pull request; without `confirm` (the preview's fingerprint), shows what
    /// it would post.
    PrComment {
        review: i64,
        confirm: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TeamCall {
    /// Starts the team, or goes on with it; `options`: a `termf_team::Options`. Without `run`, `request` (a
    /// `termf_team::Request`) starts it in a new run of its own.
    Start {
        #[serde(default)]
        run: Option<i64>,
        #[serde(default)]
        request: Option<Value>,
        options: Value,
    },
    /// Answers the questionnaire of a blocked step: question id → answer (one value, or several for a multiple
    /// choice); `note`: anything else.
    Answer {
        run: i64,
        step: String,
        answers: HashMap<String, Vec<String>>,
        #[serde(default)]
        note: String,
    },
    Approve {
        run: i64,
        step: String,
    },
    Revise {
        run: i64,
        step: String,
        feedback: String,
    },
    /// Every team at work without `run`.
    Stop {
        run: Option<i64>,
    },
    /// Rule levels by name, for the ticket or one task of the plan.
    Rules {
        run: i64,
        task: Option<String>,
        rules: HashMap<String, String>,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentCall {
    /// `spec`: a `termf_agents::SessionSpec`.
    Start {
        spec: Value,
        prompt: Option<String>,
    },
    Send {
        session: i64,
        text: String,
    },
    Interrupt {
        session: i64,
    },
    /// Answers a permission request.
    Answer {
        session: i64,
        request: String,
        allow: bool,
        message: Option<String>,
    },
    Stop {
        session: i64,
    },
    Takeover {
        session: i64,
    },
    Handback {
        session: i64,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    Ok(Value),
    Error(String),
    Event(StatusEvent),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Starting,
    Working,
    Waiting,
    Idle,
    Done,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Starting => "starting",
            Status::Working => "working",
            Status::Waiting => "waiting",
            Status::Idle => "idle",
            Status::Done => "done",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatusEvent {
    pub id: u32,
    pub title: String,
    pub status: Status,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: u32,
    pub title: String,
    pub cwd: String,
    /// Agent name the session was opened with, `None` for a plain shell.
    pub agent: Option<String>,
    pub running: bool,
    pub exit_code: Option<u32>,
    /// Only agents that report their state (Claude Code through hooks) have one.
    pub status: Option<Status>,
    pub remote_control: bool,
    /// The agent's own conversation id, e.g. Claude Code's `session_id`.
    pub agent_session_id: Option<String>,
    /// Context gauge and reset, for Claude Code sessions that report through hooks.
    #[serde(default)]
    pub context: Option<crate::context::Gauge>,
}

/// Serializes `msg` as one line; serde_json escapes newlines inside strings.
pub fn encode<T: Serialize>(msg: &T) -> String {
    let mut line = serde_json::to_string(msg).expect("IPC messages always serialize");
    line.push('\n');
    line
}

/// Folder holding the socket, private to the user.
pub fn run_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("run")
}

/// The folder of the user's token, away from the data folder the sessions are given (the socket's); the
/// settings of every Claude Code session TermF starts deny it, and so does `termf guard`.
pub const USER_TOKEN_DIR: &str = "dev.termf.cli";

pub fn user_token_file() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .context("no local data directory on this system")?
        .join(USER_TOKEN_DIR)
        .join("token"))
}

/// A socket file path on Unix, a named pipe path on Windows (derived from the per-user data dir).
pub fn socket_name(data_dir: &Path) -> String {
    #[cfg(windows)]
    {
        let id: String = data_dir
            .to_string_lossy()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        format!(r"\\.\pipe\termf-{id}")
    }
    #[cfg(unix)]
    {
        socket_path(&run_dir(data_dir), uid())
            .to_string_lossy()
            .into_owned()
    }
}

/// Longest socket path: `sun_path` holds 104 bytes on macOS (108 on Linux), the final NUL included.
const MAX_SOCKET_PATH: usize = 103;

/// The socket in the run dir, or in `/tmp/termf-<uid>` when that path is too long for a socket (long user names).
pub fn socket_path(run_dir: &Path, uid: u32) -> PathBuf {
    let path = run_dir.join("termf.sock");
    if path.as_os_str().len() <= MAX_SOCKET_PATH {
        path
    } else {
        PathBuf::from(format!("/tmp/termf-{uid}/termf.sock"))
    }
}

#[cfg(unix)]
pub fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// Creates `dir` readable by the user only, refusing one that another user created first (e.g. in /tmp).
#[cfg(unix)]
pub fn private_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .with_context(|| format!("cannot create {}", dir.display()))?;
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() || meta.uid() != uid() {
        bail!("{} is not a folder owned by you", dir.display());
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub fn read_user_token() -> Result<String> {
    let file = user_token_file()?;
    let token = fs::read_to_string(&file)
        .with_context(|| format!("cannot read {} (is TermF running?)", file.display()))?;
    Ok(token.trim().to_owned())
}

pub fn write_token(file: &Path, token: &str) -> Result<()> {
    let dir = file.parent().context("the token file has no folder")?;
    #[cfg(unix)]
    private_dir(dir)?;
    #[cfg(not(unix))]
    fs::create_dir_all(dir)?;
    let _ = fs::remove_file(file);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(file)
        .and_then(|mut f| std::io::Write::write_all(&mut f, token.as_bytes()))
        .with_context(|| format!("cannot write {}", file.display()))
}

/// Constant-time comparison, so response timing does not leak how much of a guess was right.
pub fn token_matches(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0, |diff, (a, b)| diff | (a ^ b))
            == 0
}

/// What a token allows: the user's token (in the token file) everything but a session's tools, a session's token
/// only its own hooks, an agent session's `termf mcp` token only its tools for that session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Full,
    Session(u32),
    Mcp(i64),
}

impl Access {
    pub fn allows(self, call: &Call) -> bool {
        match (self, call) {
            // A session's actions wait for the user's approval in that session, which a call of its tool skips.
            (Access::Full, Call::Mcp { .. }) => false,
            (Access::Full, _) => true,
            (Access::Session(id), Call::Hook { session, .. }) => *session == id,
            (Access::Mcp(id), Call::Mcp { session, .. }) => *session == id,
            _ => false,
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Reserved {
    pub action: &'static str,
    /// What the user sees before deciding, in full.
    pub args: BTreeMap<&'static str, String>,
}

/// A call the user confirms in the app when it comes through the local API: the CLI cannot tell them from an
/// agent that runs it.
pub fn reserved(call: &Call) -> Option<Reserved> {
    let text = |v: &Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let (action, args): (_, Vec<(_, Option<String>)>) = match call {
        Call::Run {
            ticket,
            project,
            repo,
        } => (
            "run",
            vec![
                ("ticket", Some(ticket.clone())),
                ("project", project.clone()),
                ("repo", repo.clone()),
            ],
        ),
        Call::Resume { run } => ("resume", vec![("run", Some(run.to_string()))]),
        // Typing in a session can answer its permission or trust prompt.
        Call::Say { session, text } => (
            "say",
            vec![
                ("session", Some(session.clone())),
                ("text", Some(text.clone())),
            ],
        ),
        // A shell tile does nothing until something types in it; an agent's runs with the user's settings.
        Call::Open {
            path,
            agent: Some(agent),
        } => (
            "open",
            vec![("agent", Some(agent.clone())), ("cwd", Some(path.clone()))],
        ),
        Call::Deliver { run, .. } => ("deliver", vec![("run", Some(run.to_string()))]),
        Call::Close { run, force } => {
            let action = if *force { "closeForce" } else { "close" };
            (action, vec![("run", Some(run.to_string()))])
        }
        Call::Team(TeamCall::Start { run: Some(run), .. }) => {
            ("team", vec![("run", Some(run.to_string()))])
        }
        Call::Team(TeamCall::Start { request, .. }) => {
            let request = request.clone().unwrap_or_default();
            (
                "teamRequest",
                vec![
                    ("request", text(&request["text"])),
                    ("project", text(&request["project"])),
                    ("repo", text(&request["repo"])),
                ],
            )
        }
        Call::Team(TeamCall::Approve { run, step }) => (
            "approve",
            vec![("run", Some(run.to_string())), ("step", Some(step.clone()))],
        ),
        // Anything but a refusal or changes, as `termf_visual::Decision::parse` reads them, may let the gate pass.
        Call::Review(ReviewCall::Decide {
            review,
            screen,
            decision,
            ..
        }) if !matches!(
            decision.trim().to_lowercase().as_str(),
            "reject" | "rejected" | "changes" | "change"
        ) =>
        {
            (
                "review",
                vec![
                    ("review", Some(review.to_string())),
                    ("screen", Some(screen.clone())),
                ],
            )
        }
        Call::Agent(AgentCall::Answer {
            session,
            allow: true,
            ..
        }) => ("allow", vec![("session", Some(session.to_string()))]),
        Call::Agent(AgentCall::Start { spec, prompt }) => {
            let tools: Vec<&str> = spec["allowedTools"]
                .as_array()
                .map(|t| t.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            (
                "agentStart",
                vec![
                    ("cwd", text(&spec["cwd"])),
                    ("role", text(&spec["role"])),
                    ("mode", text(&spec["permissionMode"])),
                    ("tools", Some(tools.join(", ")).filter(|t| !t.is_empty())),
                    ("model", text(&spec["model"])),
                    ("prompt", prompt.clone().filter(|p| !p.is_empty())),
                ],
            )
        }
        _ => return None,
    };
    Some(Reserved {
        action,
        args: args
            .into_iter()
            .filter_map(|(k, v)| Some((k, v?)))
            .collect(),
    })
}

/// Why the local API never makes `call`, whoever confirms it: a headless session started from outside the app
/// keeps TermF's guard, asks before what its permissions do not allow, and never gets TermF's own tools.
pub fn refused(call: &Call) -> Option<&'static str> {
    let Call::Agent(AgentCall::Start { spec, .. }) = call else {
        return None;
    };
    let mode = spec["permissionMode"].as_str().unwrap_or_default();
    if !matches!(spec["mcp"], Value::Null | Value::Bool(false)) {
        Some("a session with TermF's tools only starts from TermF itself")
    } else if mode.trim().eq_ignore_ascii_case("bypassPermissions") {
        Some("bypassPermissions is refused through the local API")
    } else if spec["guard"] != true {
        Some("a session started through the local API needs TermF's guard (\"guard\": true)")
    } else {
        None
    }
}

/// The access `token` grants, given the user's token, each terminal session's token and each agent session's
/// `termf mcp` token.
pub fn access(
    token: &str,
    user: &str,
    sessions: &HashMap<u32, String>,
    mcp: &HashMap<i64, String>,
) -> Option<Access> {
    if token_matches(token, user) {
        return Some(Access::Full);
    }
    let session = sessions
        .iter()
        .find(|(_, t)| token_matches(token, t))
        .map(|(id, _)| Access::Session(*id));
    session.or_else(|| {
        mcp.iter()
            .find(|(_, t)| token_matches(token, t))
            .map(|(id, _)| Access::Mcp(*id))
    })
}

/// Whether a session needs the computer awake: its agent is working, or it is an agent that reports
/// no status and is still running. Plain shells never do.
pub fn keeps_awake(session: &SessionSummary) -> bool {
    match session.status {
        Some(status) => matches!(status, Status::Starting | Status::Working),
        None => session.running && session.agent.is_some(),
    }
}

/// Whether terminal input can answer a prompt: escape sequences (arrows, focus and mouse reports) cannot.
pub fn answers_prompt(input: &[u8]) -> bool {
    input.first().is_some_and(|&b| b != 0x1b)
}

/// Whether terminal input is someone typing: any key, Esc and Alt sequences included, but not the reports
/// a terminal sends on its own (focus, mouse, replies to queries).
pub fn is_typing(input: &[u8]) -> bool {
    let Some(rest) = input.strip_prefix(b"\x1b") else {
        return !input.is_empty();
    };
    let csi_params = |end: u8| {
        rest.strip_prefix(b"[")
            .and_then(|r| r.strip_suffix(&[end]))
            .is_some_and(|p| !p.is_empty() && p.iter().all(|b| b.is_ascii_digit() || *b == b';'))
    };
    let report = matches!(rest, b"[I" | b"[O") // focus
        || rest.starts_with(b"[M") || rest.starts_with(b"[<") // mouse
        || rest.starts_with(b"[?") || rest.starts_with(b"[>") // device attributes and modes
        || csi_params(b'R') // cursor position
        // OSC (colours…) and DCS replies; Alt+] and Alt+P alone are two bytes.
        || (rest.starts_with(b"]") || rest.starts_with(b"P")) && rest.len() > 1;
    !report
}

/// Status a Claude Code hook event means, or `None` when it says nothing about the state.
pub fn hook_status(event: &str, payload: &Value) -> Option<Status> {
    let field = |key| payload.get(key).and_then(Value::as_str).unwrap_or_default();
    match event {
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => Some(Status::Working),
        "PermissionRequest" => Some(Status::Waiting),
        "Notification" => match field("notification_type") {
            "permission_prompt"
            | "elicitation_dialog"
            | "elicitation_url_dialog"
            | "agent_needs_input"
            | "quota_auto_resume_stale" => Some(Status::Waiting),
            // Also the only sign of a turn interrupted with Esc, which fires no Stop.
            "idle_prompt" => Some(Status::Idle),
            _ => None,
        },
        "Stop" | "StopFailure" => Some(Status::Idle),
        // Compaction can restart a session in the middle of a turn.
        "SessionStart" if field("source") != "compact" => Some(Status::Idle),
        // /clear and /resume end one conversation but the process goes on with another.
        "SessionEnd" if !matches!(field("reason"), "clear" | "resume") => Some(Status::Done),
        _ => None,
    }
}

/// Resolves `query` as a session id, else as a case-insensitive part of exactly one title.
pub fn find_session(sessions: &[SessionSummary], query: &str) -> Result<u32> {
    if let Ok(id) = query.parse::<u32>()
        && sessions.iter().any(|s| s.id == id)
    {
        return Ok(id);
    }
    let query = query.to_lowercase();
    let found: Vec<_> = sessions
        .iter()
        .filter(|s| s.title.to_lowercase().contains(&query))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.id),
        [] => Err(anyhow!("no session matches {query:?}")),
        many => bail!(
            "{query:?} matches several sessions: {}",
            many.iter()
                .map(|s| format!("{} ({})", s.title, s.id))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_working_or_untracked_agents_keep_awake() {
        let with = |status, agent: Option<&str>, running| SessionSummary {
            status,
            agent: agent.map(Into::into),
            running,
            ..session(1, "x")
        };
        assert!(keeps_awake(&with(
            Some(Status::Working),
            Some("claude"),
            true
        )));
        assert!(keeps_awake(&with(
            Some(Status::Starting),
            Some("claude"),
            true
        )));
        for idle in [Status::Waiting, Status::Idle, Status::Done] {
            assert!(!keeps_awake(&with(Some(idle), Some("claude"), true)));
        }
        assert!(
            keeps_awake(&with(None, Some("codex"), true)),
            "no status: running counts"
        );
        assert!(!keeps_awake(&with(None, Some("codex"), false)));
        assert!(
            !keeps_awake(&with(None, None, true)),
            "plain shells never keep awake"
        );
    }

    fn session(id: u32, title: &str) -> SessionSummary {
        SessionSummary {
            id,
            title: title.into(),
            cwd: "/src".into(),
            agent: None,
            running: true,
            exit_code: None,
            status: None,
            remote_control: false,
            agent_session_id: None,
            context: None,
        }
    }

    #[test]
    fn requests_round_trip_on_one_line() {
        let requests = [
            Request {
                token: "t".into(),
                call: Call::List,
            },
            Request {
                token: "t".into(),
                call: Call::Say {
                    session: "api".into(),
                    text: "two\nlines".into(),
                },
            },
            Request {
                token: "t".into(),
                call: Call::Hook {
                    session: 3,
                    event: "Stop".into(),
                    payload: json!({"session_id": "abc"}),
                },
            },
            Request {
                token: "t".into(),
                call: Call::Agent(AgentCall::Answer {
                    session: 4,
                    request: "r1".into(),
                    allow: false,
                    message: Some("not\nnow".into()),
                }),
            },
        ];
        for request in requests {
            let line = encode(&request);
            assert_eq!(line.matches('\n').count(), 1, "{line}");
            assert_eq!(serde_json::from_str::<Request>(&line).unwrap(), request);
        }
        assert_eq!(
            serde_json::from_str::<Request>(r#"{"token":"t","method":"watch"}"#).unwrap(),
            Request {
                token: "t".into(),
                call: Call::Watch
            }
        );
    }

    #[test]
    fn replies_round_trip() {
        let replies = [
            Reply::Ok(json!(null)),
            Reply::Error("nope".into()),
            Reply::Event(StatusEvent {
                id: 1,
                title: "api · claude".into(),
                status: Status::Waiting,
            }),
        ];
        for reply in replies {
            assert_eq!(
                serde_json::from_str::<Reply>(&encode(&reply)).unwrap(),
                reply
            );
        }
        assert_eq!(
            encode(&Reply::Event(StatusEvent {
                id: 1,
                title: "x".into(),
                status: Status::Idle
            })),
            "{\"event\":{\"id\":1,\"title\":\"x\",\"status\":\"idle\"}}\n"
        );
    }

    #[test]
    fn checks_tokens() {
        assert!(token_matches("abc123", "abc123"));
        assert!(!token_matches("abc124", "abc123"));
        assert!(!token_matches("abc", "abc123"));
        assert!(!token_matches("", "abc123"));
    }

    #[test]
    fn maps_hook_events_to_status() {
        let none = json!({});
        assert_eq!(
            hook_status("UserPromptSubmit", &none),
            Some(Status::Working)
        );
        assert_eq!(hook_status("PreToolUse", &none), Some(Status::Working));
        assert_eq!(hook_status("PostToolUse", &none), Some(Status::Working));
        assert_eq!(
            hook_status("PermissionRequest", &none),
            Some(Status::Waiting)
        );
        assert_eq!(
            hook_status(
                "Notification",
                &json!({"notification_type": "permission_prompt"})
            ),
            Some(Status::Waiting)
        );
        assert_eq!(
            hook_status("Notification", &json!({"notification_type": "idle_prompt"})),
            Some(Status::Idle)
        );
        assert_eq!(
            hook_status(
                "Notification",
                &json!({"notification_type": "auth_success"})
            ),
            None
        );
        assert_eq!(hook_status("Notification", &none), None);
        assert_eq!(hook_status("Stop", &none), Some(Status::Idle));
        assert_eq!(
            hook_status("StopFailure", &json!({"error": "rate_limit"})),
            Some(Status::Idle)
        );
        assert_eq!(
            hook_status("SessionStart", &json!({"source": "startup"})),
            Some(Status::Idle)
        );
        assert_eq!(
            hook_status("SessionStart", &json!({"source": "compact"})),
            None
        );
        assert_eq!(
            hook_status("SessionEnd", &json!({"reason": "prompt_input_exit"})),
            Some(Status::Done)
        );
        assert_eq!(hook_status("SessionEnd", &json!({"reason": "clear"})), None);
        assert_eq!(hook_status("SubagentStop", &none), None);
    }

    #[test]
    fn session_tokens_only_report_their_own_hooks() {
        let sessions = HashMap::from([(1, "s1".to_string()), (2, "s2".to_string())]);
        let none = HashMap::new();
        assert_eq!(access("user", "user", &sessions, &none), Some(Access::Full));
        assert_eq!(
            access("s2", "user", &sessions, &none),
            Some(Access::Session(2))
        );
        assert_eq!(access("nope", "user", &sessions, &none), None);

        let hook = |session| Call::Hook {
            session,
            event: "Stop".into(),
            payload: Value::Null,
        };
        let others = [
            Call::List,
            Call::Watch,
            Call::Open {
                path: "/".into(),
                agent: None,
            },
            Call::Say {
                session: "1".into(),
                text: "rm -rf ~".into(),
            },
            Call::Deliver {
                run: 1,
                confirm: String::new(),
            },
            hook(2),
        ];
        let session = Access::Session(1);
        assert!(session.allows(&hook(1)));
        for call in &others {
            assert!(!session.allows(call), "{call:?}");
            assert!(Access::Full.allows(call), "{call:?}");
        }
    }

    #[test]
    fn mcp_tokens_only_call_their_own_tools() {
        let mcp = HashMap::from([(7, "m7".to_string())]);
        let tiles = HashMap::from([(1, "s1".to_string())]);
        assert_eq!(access("m7", "user", &tiles, &mcp), Some(Access::Mcp(7)));
        let tool = |session| Call::Mcp {
            session,
            tool: McpCall::TeamRuns {},
        };
        let access = Access::Mcp(7);
        assert!(access.allows(&tool(7)));
        assert!(!access.allows(&tool(8)), "another session's tools");
        assert!(!Access::Session(1).allows(&tool(7)));
        assert!(
            !Access::Full.allows(&tool(7)),
            "the user's token skips no approval"
        );
        for call in [
            Call::List,
            Call::Deliver {
                run: 1,
                confirm: String::new(),
            },
            Call::Close {
                run: 1,
                force: true,
            },
            Call::Vault(VaultCall::Status),
            Call::Team(TeamCall::Stop { run: None }),
            Call::Agent(AgentCall::Send {
                session: 7,
                text: "hi".into(),
            }),
        ] {
            assert!(!access.allows(&call), "{call:?}");
        }
    }

    #[test]
    fn what_acts_for_the_user_waits_for_the_app() {
        let action = |call: Call| reserved(&call).map(|r| (r.action, r.args));
        let args = |pairs: &[(&'static str, &str)]| -> BTreeMap<&'static str, String> {
            pairs.iter().map(|(k, v)| (*k, v.to_string())).collect()
        };
        assert_eq!(
            action(Call::Deliver {
                run: 12,
                confirm: "f".into()
            }),
            Some(("deliver", args(&[("run", "12")])))
        );
        assert_eq!(
            action(Call::Run {
                ticket: "#42".into(),
                project: Some("acme".into()),
                repo: Some("api".into())
            }),
            Some((
                "run",
                args(&[("ticket", "#42"), ("project", "acme"), ("repo", "api")])
            )),
            "where it runs is part of what you allow"
        );
        assert_eq!(
            action(Call::Resume { run: 7 }),
            Some(("resume", args(&[("run", "7")])))
        );
        assert_eq!(
            action(Call::Say {
                session: "api".into(),
                text: "1".into()
            }),
            Some(("say", args(&[("session", "api"), ("text", "1")]))),
            "typing can answer a permission prompt"
        );
        assert_eq!(
            action(Call::Open {
                path: "/src/api".into(),
                agent: Some("claude".into())
            }),
            Some(("open", args(&[("agent", "claude"), ("cwd", "/src/api")])))
        );
        assert_eq!(
            action(Call::Close {
                run: 3,
                force: true
            })
            .unwrap()
            .0,
            "closeForce"
        );
        assert_eq!(
            action(Call::Team(TeamCall::Approve {
                run: 3,
                step: "plan".into()
            })),
            Some(("approve", args(&[("run", "3"), ("step", "plan")])))
        );
        let request = json!({"text": "x".repeat(5000), "project": "acme"});
        let (kind, said) = action(Call::Team(TeamCall::Start {
            run: None,
            request: Some(request),
            options: json!({}),
        }))
        .unwrap();
        assert_eq!(kind, "teamRequest");
        assert_eq!(said["request"].len(), 5000, "never cut");
        assert_eq!(said["project"], "acme");
        assert_eq!(
            action(Call::Team(TeamCall::Start {
                run: Some(4),
                request: None,
                options: json!({})
            }))
            .unwrap()
            .0,
            "team"
        );
        let decide = |decision: &str| {
            action(Call::Review(ReviewCall::Decide {
                review: 2,
                screen: "home".into(),
                decision: decision.into(),
                comment: None,
                pins: None,
            }))
        };
        for approves in ["approve", "Approved", " APPROVE ", "approved\n"] {
            assert_eq!(decide(approves).unwrap().0, "review", "{approves:?}");
        }
        for not in ["reject", "Rejected", "changes", " CHANGE "] {
            assert_eq!(decide(not), None, "{not:?}");
        }
        let answer = |allow| {
            action(Call::Agent(AgentCall::Answer {
                session: 5,
                request: "r1".into(),
                allow,
                message: None,
            }))
        };
        assert_eq!(answer(true).unwrap().0, "allow");
        assert_eq!(answer(false), None, "refusing needs no confirmation");
        let start = |spec: Value| {
            Call::Agent(AgentCall::Start {
                spec,
                prompt: Some("fix the build".into()),
            })
        };
        let spec = json!({"cwd": "/src/api", "role": "back", "permissionMode": "acceptEdits",
                          "allowedTools": ["Bash(npm *)", "Edit"], "guard": true});
        assert_eq!(
            action(start(spec.clone())),
            Some((
                "agentStart",
                args(&[
                    ("cwd", "/src/api"),
                    ("role", "back"),
                    ("mode", "acceptEdits"),
                    ("tools", "Bash(npm *), Edit"),
                    ("prompt", "fix the build"),
                ])
            ))
        );
        assert_eq!(refused(&start(spec.clone())), None);
        for (key, value) in [
            ("guard", json!(false)),
            ("guard", Value::Null),
            ("mcp", json!(true)),
            ("permissionMode", json!(" BypassPermissions")),
        ] {
            let mut spec = spec.clone();
            spec[key] = value.clone();
            assert!(refused(&start(spec)).is_some(), "{key}: {value}");
        }
        for call in [
            Call::List,
            Call::Watch,
            Call::Open {
                path: "/src/api".into(),
                agent: None,
            },
            Call::Team(TeamCall::Stop { run: None }),
            Call::Vault(VaultCall::Lock),
        ] {
            assert_eq!(reserved(&call), None, "{call:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_user_token_is_private_and_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("termf-token-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file = root.join(USER_TOKEN_DIR).join("token");
        write_token(&file, "old").unwrap();
        write_token(&file, "new").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "new");
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(file.parent().unwrap()), 0o700);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mcp_tools_are_named_as_listed() {
        let names: Vec<&str> = MCP_READ
            .iter()
            .chain(&MCP_ACTIONS)
            .chain(&MCP_EXPERT)
            .copied()
            .collect();
        for name in &names {
            let mut call = serde_json::Map::new();
            call.insert("tool".into(), json!(name));
            for (key, value) in [
                ("run", json!(1)),
                ("step", json!("plan")),
                ("answers", json!("yes")),
                ("feedback", json!("smaller")),
                ("text", json!("hi")),
                ("query", json!("webhooks")),
                ("id", json!(3)),
                ("question", json!("why?")),
            ] {
                call.insert(key.into(), value);
            }
            let read = serde_json::from_value::<McpCall>(Value::Object(call.clone()));
            // Unknown fields are refused: only the ones a tool takes.
            let fields: &[&str] = match *name {
                "team_run" | "stop_team_run" => &["run"],
                "approve" => &["run", "step"],
                "answer" => &["run", "step", "answers"],
                "request_changes" => &["run", "step", "feedback"],
                "send_message" => &["run", "step", "text"],
                "start_team_run" => &["run"],
                "memory_search" => &["query"],
                "memory_read" => &["id"],
                "ask_expert" => &["question"],
                _ => &[],
            };
            call.retain(|k, _| k == "tool" || fields.contains(&k.as_str()));
            assert!(read.is_err(), "{name} takes only its own fields");
            let call = serde_json::from_value::<McpCall>(Value::Object(call)).unwrap();
            assert_eq!(serde_json::to_value(&call).unwrap()["tool"], json!(name));
        }
        assert!(serde_json::from_value::<McpCall>(json!({"tool": "deliver", "run": 1})).is_err());
    }

    #[test]
    fn only_plain_input_answers_a_prompt() {
        assert!(answers_prompt(b"\r"));
        assert!(answers_prompt(b"1"));
        assert!(!answers_prompt(b"\x1b[I"), "focus report");
        assert!(!answers_prompt(b"\x1b[B"), "arrow key");
        assert!(!answers_prompt(b""));
    }

    #[test]
    fn tells_typing_from_terminal_reports() {
        for typed in [
            &b"a"[..],
            b"\r",
            b"\x03",
            b"\x1b",
            b"\x1b[A",
            b"\x1bOB",
            b"\x1b[200~hi\x1b[201~",
            b"\x1b\x7f",   // Alt+Backspace
            b"\x1b[3~",    // Delete
            b"\x1bb",      // Alt+b
            b"\x1b[97;5u", // CSI u
            b"\x1b]",      // Alt+]
            b"\x1bP",      // Alt+Shift+p
            b"\x1b[1;5D",  // Ctrl+Left
        ] {
            assert!(is_typing(typed), "{typed:?}");
        }
        for report in [
            &b""[..],
            b"\x1b[I",
            b"\x1b[O",
            b"\x1b[<0;3;4M",
            b"\x1b[M #!",
            b"\x1b]11;rgb:0/0/0\x07",
            b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\",
            b"\x1bP1$r0m\x1b\\",
            b"\x1b[?1;2c",
            b"\x1b[>0;276;0c",
            b"\x1b[?2004;1$y",
            b"\x1b[12;40R",
        ] {
            assert!(!is_typing(report), "{report:?}");
        }
    }

    #[test]
    fn keeps_socket_paths_short_enough() {
        let short = Path::new("/Users/me/Library/Application Support/dev.termf.app/run");
        assert_eq!(socket_path(short, 501), short.join("termf.sock"));

        let long = format!(
            "/Users/{}/Library/Application Support/dev.termf.app/run",
            "x".repeat(50)
        );
        let path = socket_path(Path::new(&long), 501);
        assert_eq!(path, Path::new("/tmp/termf-501/termf.sock"));
        assert!(Path::new(&long).join("termf.sock").as_os_str().len() > MAX_SOCKET_PATH);
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_dirs() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("termf-private-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        private_dir(&dir).unwrap();
        let mode = fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
        fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn finds_sessions_by_id_or_unique_title() {
        let sessions = [
            session(1, "api · claude"),
            session(2, "web · claude"),
            session(12, "web · shell"),
        ];
        assert_eq!(find_session(&sessions, "2").unwrap(), 2);
        assert_eq!(find_session(&sessions, "API").unwrap(), 1);
        assert_eq!(find_session(&sessions, "web · shell").unwrap(), 12);
        assert!(find_session(&sessions, "web").is_err(), "ambiguous");
        assert!(find_session(&sessions, "mobile").is_err());
        assert!(find_session(&sessions, "7").is_err());
    }
}
