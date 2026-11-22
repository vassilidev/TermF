//! Local API shared by the app and the `termf` CLI: one JSON object per line over a local socket.
//! Every request carries a token; a watch request turns the connection into an event stream.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ENV_SOCKET: &str = "TERMF_SOCKET";
/// A session's own token, which only reports that session's hooks.
pub const ENV_TOKEN: &str = "TERMF_TOKEN";
pub const ENV_SESSION: &str = "TERMF_SESSION_ID";

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

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum VaultCall {
    Status,
    Unlock { password: String },
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
    /// Starts the team, or goes on with it; `options`: a `termf_team::Options`.
    Start {
        run: i64,
        options: Value,
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

/// Folder holding the socket and the token, private to the user.
pub fn run_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("run")
}

pub fn token_file(data_dir: &Path) -> PathBuf {
    run_dir(data_dir).join("token")
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

pub fn read_token(data_dir: &Path) -> Result<String> {
    let file = token_file(data_dir);
    let token = fs::read_to_string(&file)
        .with_context(|| format!("cannot read {} (is TermF running?)", file.display()))?;
    Ok(token.trim().to_owned())
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

/// What a token allows: the user's token (in the token file) everything, a session's token only its own hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Full,
    Session(u32),
}

impl Access {
    pub fn allows(self, call: &Call) -> bool {
        match (self, call) {
            (Access::Full, _) => true,
            (Access::Session(id), Call::Hook { session, .. }) => *session == id,
            (Access::Session(_), _) => false,
        }
    }
}

/// The access `token` grants, given the user's token and each session's token.
pub fn access(token: &str, user: &str, sessions: &HashMap<u32, String>) -> Option<Access> {
    if token_matches(token, user) {
        return Some(Access::Full);
    }
    sessions
        .iter()
        .find(|(_, t)| token_matches(token, t))
        .map(|(id, _)| Access::Session(*id))
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
        assert_eq!(access("user", "user", &sessions), Some(Access::Full));
        assert_eq!(access("s2", "user", &sessions), Some(Access::Session(2)));
        assert_eq!(access("nope", "user", &sessions), None);

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
