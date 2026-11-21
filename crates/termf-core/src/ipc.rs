//! Local API shared by the app and the `termf` CLI: one JSON object per line over a local socket.
//! Every request carries the token; a watch request turns the connection into an event stream.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ENV_SOCKET: &str = "TERMF_SOCKET";
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
    if cfg!(windows) {
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
    } else {
        run_dir(data_dir)
            .join("termf.sock")
            .to_string_lossy()
            .into_owned()
    }
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
            _ => None,
        },
        "Stop" => Some(Status::Idle),
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
            None,
            "already idle since Stop"
        );
        assert_eq!(hook_status("Notification", &none), None);
        assert_eq!(hook_status("Stop", &none), Some(Status::Idle));
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
