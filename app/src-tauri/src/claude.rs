//! The Claude view (ARCHITECTURE §11.3): the machine's whole Claude Code config, simple settings edits, and the
//! account's claude.ai artifacts, listed by a hidden Claude session.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_claude::artifacts::{self, Artifact, Failure, Policy, Refresh, Step};
use termf_claude::settings::{self, Preview, Scope, Settings};
use termf_claude::{Env, Overview};
use termf_core::ipc::SessionSummary;

use crate::api::{self, Task};
use crate::{AppState, CmdResult, err};

pub struct ClaudeState {
    index: PathBuf,
    /// Where hidden sessions run: an empty folder of TermF's, trusted once by the user.
    agent_dir: PathBuf,
    index_lock: Mutex<()>,
    refresh: Mutex<Refresh>,
    /// Deletion sessions and the artifact each may delete.
    policies: Mutex<HashMap<u32, Policy>>,
}

impl ClaudeState {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            index: data_dir.join("artifacts.json"),
            agent_dir: data_dir.join("claude-agent"),
            index_lock: Mutex::default(),
            refresh: Mutex::default(),
            policies: Mutex::default(),
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The `[agents]` entry that runs Claude Code (the one named `claude` first) and its command.
fn claude_agent(config_dir: &Path) -> Result<(String, String)> {
    let settings = termf_core::load_settings(config_dir)?;
    let named = settings
        .agents
        .get_key_value("claude")
        .filter(|(_, c)| termf_core::claude::is_claude(c));
    let (name, command) = named
        .or_else(|| {
            settings
                .agents
                .iter()
                .find(|(_, c)| termf_core::claude::is_claude(c))
        })
        .context("no agent runs Claude Code in termf.toml [agents]")?;
    Ok((name.clone(), command.clone()))
}

/// Only the program: the agent's own options (say `--dangerously-skip-permissions`) never reach a hidden session.
fn program(command: &str) -> String {
    command.split_whitespace().next().unwrap_or("claude").into()
}

fn env(config_dir: &Path, repo: Option<String>) -> Env {
    let claude = claude_agent(config_dir).map_or_else(|_| "claude".into(), |(_, c)| program(&c));
    Env::current(repo.map(PathBuf::from), claude)
}

#[tauri::command]
pub async fn claude_overview(
    state: State<'_, AppState>,
    repo: Option<String>,
) -> CmdResult<Overview> {
    let env = env(&state.config_dir, repo);
    tauri::async_runtime::spawn_blocking(move || termf_claude::overview(&env))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn claude_setting_preview(
    state: State<AppState>,
    scope: Scope,
    key: String,
    value: Option<Value>,
    repo: Option<String>,
) -> CmdResult<Preview> {
    let env = env(&state.config_dir, repo);
    settings::preview(&env, scope, &key, value.as_ref()).map_err(err)
}

/// Returns the settings read again, masked.
#[tauri::command]
pub fn claude_setting_save(
    state: State<AppState>,
    scope: Scope,
    key: String,
    value: Option<Value>,
    repo: Option<String>,
) -> CmdResult<Settings> {
    let env = env(&state.config_dir, repo);
    settings::save(&env, scope, &key, value.as_ref()).map_err(err)?;
    Ok(settings::masked(settings::read(&env)))
}

/// Writes an export the page built, to the path the save dialog gave, masked once more.
#[tauri::command]
pub fn claude_export(path: String, text: String) -> CmdResult<()> {
    let path = PathBuf::from(path);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !["md", "json"].contains(&ext) {
        return Err("an export is a .md or .json file".into());
    }
    settings::write_atomic(&path, &termf_claude::export_text(&text)).map_err(err)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Artifacts {
    artifacts: Vec<Artifact>,
    listed: Option<u64>,
    refresh: Refresh,
    refresh_hours: u32,
}

fn artifacts_state(app: &AppHandle) -> Artifacts {
    let claude = app.state::<ClaudeState>();
    let index = {
        let _lock = claude.index_lock.lock().unwrap();
        artifacts::load(&claude.index)
    };
    let config_dir = &app.state::<AppState>().config_dir;
    Artifacts {
        artifacts: index.artifacts,
        listed: index.listed,
        refresh: claude.refresh.lock().unwrap().clone(),
        refresh_hours: termf_core::load_settings(config_dir)
            .map_or(0, |s| s.artifacts.refresh_hours),
    }
}

fn changed(app: &AppHandle) {
    let _ = app.emit("artifacts", artifacts_state(app));
}

#[tauri::command]
pub fn artifacts_list(app: AppHandle) -> Artifacts {
    artifacts_state(&app)
}

#[tauri::command]
pub fn artifacts_refresh(app: AppHandle) -> CmdResult<Artifacts> {
    start(&app, true).map_err(err)?;
    Ok(artifacts_state(&app))
}

#[tauri::command]
pub fn artifacts_cancel(app: AppHandle) -> Artifacts {
    let session = app.state::<ClaudeState>().refresh.lock().unwrap().session;
    if let Some(id) = session {
        apply(&app, id, Step::Fail(Failure::Cancelled, None));
    }
    artifacts_state(&app)
}

#[tauri::command]
pub fn artifacts_auto(app: AppHandle, on: bool) -> CmdResult<Artifacts> {
    let config_dir = &app.state::<AppState>().config_dir;
    termf_core::save_artifacts_refresh(config_dir, if on { 6 } else { 0 }).map_err(err)?;
    Ok(artifacts_state(&app))
}

fn check_url(url: &str) -> Result<String> {
    artifacts::artifact_url(url)
        .map(|(_, url)| url)
        .ok_or_else(|| anyhow!("{url} is not a claude.ai artifact"))
}

/// Opens a session where Claude reads the artifact, asks you, and deletes it once you confirm in its tile:
/// TermF never answers for you (its `PreToolUse` hook asks you even when your settings allow the tool).
#[tauri::command]
pub fn artifacts_delete(app: AppHandle, url: String) -> CmdResult<SessionSummary> {
    let url = check_url(&url).map_err(err)?;
    let state = app.state::<AppState>();
    let claude = app.state::<ClaudeState>();
    let (agent, command) = claude_agent(&state.config_dir).map_err(err)?;
    std::fs::create_dir_all(&claude.agent_dir).map_err(|e| e.to_string())?;
    let task = Task {
        agent,
        command: program(&command),
        args: [
            "--model",
            "haiku",
            "--strict-mcp-config",
            "--permission-mode",
            "default",
        ]
        .map(String::from)
        .to_vec(),
        prompt: artifacts::delete_prompt(&url),
        hidden: false,
    };
    let title = "claude.ai artifact".to_owned();
    let session = api::spawn_task(&state, claude.agent_dir.clone(), title, task).map_err(err)?;
    claude
        .policies
        .lock()
        .unwrap()
        .insert(session.id, Policy::Delete(url));
    Ok(session)
}

/// Opens your Claude Code agent on the artifact, in the folder it was published from when TermF knows it.
#[tauri::command]
pub fn artifacts_edit(app: AppHandle, url: String) -> CmdResult<SessionSummary> {
    let url = check_url(&url).map_err(err)?;
    let state = app.state::<AppState>();
    let claude = app.state::<ClaudeState>();
    let (agent, command) = claude_agent(&state.config_dir).map_err(err)?;
    let cwd = artifacts::load(&claude.index)
        .artifacts
        .into_iter()
        .find(|a| a.url == url)
        .and_then(|a| a.cwd)
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| termf_core::expand_tilde("~"));
    let title = format!(
        "{} · {agent}",
        cwd.file_name().unwrap_or_default().to_string_lossy()
    );
    let task = Task {
        agent,
        command,
        args: Vec::new(),
        prompt: artifacts::edit_prompt(&url),
        hidden: false,
    };
    api::spawn_task(&state, cwd, title, task).map_err(err)
}

/// Starts the hidden listing session, unless one runs.
fn start(app: &AppHandle, manual: bool) -> Result<()> {
    let claude = app.state::<ClaudeState>();
    claude.refresh.lock().unwrap().begin(manual, now())?;
    let spawned = (|| {
        let state = app.state::<AppState>();
        let (agent, command) = claude_agent(&state.config_dir)?;
        std::fs::create_dir_all(&claude.agent_dir)?;
        let task = Task {
            agent,
            command: program(&command),
            args: list_args(),
            prompt: artifacts::list_prompt(),
            hidden: true,
        };
        let title = "claude.ai artifacts".to_owned();
        api::spawn_task(&state, claude.agent_dir.clone(), title, task)
    })();
    {
        let mut refresh = claude.refresh.lock().unwrap();
        match &spawned {
            Ok(session) => refresh.session = Some(session.id),
            Err(e) => refresh.finish(now(), Err((Failure::Spawn, Some(format!("{e:#}"))))),
        }
    }
    changed(app);
    spawned.map(|_| ())
}

/// A small model, and none of your MCP servers: it only calls the `Artifact` tool.
fn list_args() -> Vec<String> {
    ["--model", "haiku", "--strict-mcp-config"]
        .map(String::from)
        .to_vec()
}

/// What TermF does with a hook of any of its sessions; `Some`: the hook's answer.
pub fn hook(app: &AppHandle, id: u32, event: &str, payload: &Value) -> Option<Value> {
    let claude = app.state::<ClaudeState>();
    if event == "PostToolUse" && payload["tool_name"] == "Artifact" {
        let recorded = {
            let _lock = claude.index_lock.lock().unwrap();
            artifacts::record(&claude.index, payload, now())
        };
        match recorded {
            Ok(true) => changed(app),
            Ok(false) => {}
            Err(e) => eprintln!("termf: artifact not indexed: {e:#}"),
        }
    }
    if event == "PreToolUse"
        && let Some(policy) = claude.policies.lock().unwrap().get(&id)
    {
        return Some(artifacts::decide(policy, payload));
    }
    let step = claude.refresh.lock().unwrap().hook(id, event, payload);
    apply(app, id, step)
}

fn apply(app: &AppHandle, id: u32, step: Step) -> Option<Value> {
    let claude = app.state::<ClaudeState>();
    let state = app.state::<AppState>();
    let result = match step {
        Step::Nothing => return None,
        Step::Reply(reply) => return Some(reply),
        Step::Reveal => {
            state.api.reveal(app, &state.sessions, id);
            changed(app);
            return None;
        }
        Step::Done(listed) => {
            let started = claude.refresh.lock().unwrap().started;
            let _lock = claude.index_lock.lock().unwrap();
            let count = listed.len();
            artifacts::apply_listing(&claude.index, listed, started, now())
                .map(|_| count)
                .map_err(|e| (Failure::Invalid, Some(format!("{e:#}"))))
        }
        Step::Fail(failure, detail) => Err((failure, detail)),
    };
    claude.refresh.lock().unwrap().finish(now(), result);
    // After the hook has its answer: the session goes with its hook process.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        let state = app.state::<AppState>();
        state.api.forget(id);
        let _ = state.sessions.kill(id);
        changed(&app);
    });
    None
}

pub fn exited(app: &AppHandle, id: u32) {
    let claude = app.state::<ClaudeState>();
    claude.policies.lock().unwrap().remove(&id);
    let closed = {
        let mut refresh = claude.refresh.lock().unwrap();
        let ours = refresh.running && refresh.session == Some(id);
        if ours {
            refresh.finish(now(), Err((Failure::Closed, None)));
        }
        ours
    };
    if closed {
        changed(app);
    }
}

/// Times the listing out, and lists the account at startup then every `[artifacts] refresh_hours`.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        // Leaves the app time to start before a first automatic listing.
        std::thread::sleep(Duration::from_secs(30));
        loop {
            let claude = app.state::<ClaudeState>();
            let (session, step) = {
                let mut refresh = claude.refresh.lock().unwrap();
                let session = refresh.session;
                (session, refresh.tick(now()))
            };
            if let Some(id) = session {
                apply(&app, id, step);
            }
            let hours = termf_core::load_settings(&app.state::<AppState>().config_dir)
                .map_or(0, |s| s.artifacts.refresh_hours);
            let due = claude.refresh.lock().unwrap().due(now(), hours);
            if due && let Err(e) = start(&app, false) {
                eprintln!("termf: artifacts not listed: {e:#}");
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_sessions_get_the_program_only() {
        assert_eq!(program("claude --dangerously-skip-permissions"), "claude");
        assert_eq!(program("/opt/bin/claude --model opus"), "/opt/bin/claude");
        let args = list_args().join(" ");
        assert!(args.contains("--model haiku"));
        assert!(
            !args.contains("allowedTools") && !args.contains("bypass") && !args.contains("skip")
        );
    }

    #[test]
    fn only_artifact_links_open_a_session() {
        assert!(check_url("https://claude.ai/code/artifact/abc").is_ok());
        assert!(check_url("https://claude.ai/code/artifact/abc'; rm -rf ~").is_err());
        assert!(check_url("https://evil.test/artifact/abc").is_err());
    }

    #[test]
    fn exports_only_markdown_or_json() {
        let dir = std::env::temp_dir().join(format!("termf-claude-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("claude.md");
        claude_export(
            md.to_string_lossy().into(),
            "GITHUB_TOKEN= ghp_0123456789abcdefghij0123456789abcd".into(),
        )
        .unwrap();
        let text = std::fs::read_to_string(&md).unwrap();
        assert!(!text.contains("ghp_"), "{text}");
        assert!(claude_export(dir.join("x.sh").to_string_lossy().into(), "x".into()).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
