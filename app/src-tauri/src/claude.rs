//! The Claude view (ARCHITECTURE §11.3): the machine's whole Claude Code config, simple settings edits, and the
//! account's claude.ai artifacts, listed by a hidden Claude session.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};
use termf_claude::artifacts::{self, Artifact, Deletion, Failure, Policy, Refresh, Step};
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
    deletion: Mutex<Deletion>,
}

impl ClaudeState {
    pub fn new(data_dir: &Path) -> Self {
        let index = data_dir.join("artifacts.json");
        let refresh = Refresh::restore(&artifacts::load(&index));
        Self {
            index,
            agent_dir: data_dir.join("claude-agent"),
            index_lock: Mutex::default(),
            refresh: Mutex::new(refresh),
            deletion: Mutex::default(),
        }
    }
}

/// Tools a task's session never needs, denied in its own settings: unlike its hook, which TermF must answer
/// within two seconds, a deny rule holds whatever happens. The session reads none of your `allow` rules either
/// (`--restricted`), so without the hook's answer Claude Code asks, and the session shows its tile.
const DENIED: &[&str] = &[
    "Bash",
    "PowerShell",
    "Edit",
    "Write",
    "NotebookEdit",
    "WebFetch",
    "WebSearch",
    "Agent",
];

fn task_settings() -> Value {
    json!({ "permissions": { "deny": DENIED } })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The `[agents]` entry that runs Claude Code (the one named `claude` first) and its command.
pub(crate) fn claude_agent(config_dir: &Path) -> Result<(String, String)> {
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
pub(crate) fn program(command: &str) -> String {
    command.split_whitespace().next().unwrap_or("claude").into()
}

pub(crate) fn env(config_dir: &Path, repo: Option<String>) -> Env {
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
    deletion: Deletion,
    delete_without_confirmation: bool,
}

fn artifacts_state(app: &AppHandle) -> Artifacts {
    let claude = app.state::<ClaudeState>();
    let index = {
        let _lock = claude.index_lock.lock().unwrap();
        artifacts::load(&claude.index)
    };
    let config_dir = &app.state::<AppState>().config_dir;
    let settings = termf_core::load_settings(config_dir).ok();
    Artifacts {
        artifacts: index.artifacts,
        listed: index.listed,
        refresh: claude.refresh.lock().unwrap().clone(),
        refresh_hours: settings.as_ref().map_or(0, |s| s.artifacts.refresh_hours),
        deletion: claude.deletion.lock().unwrap().clone(),
        delete_without_confirmation: settings
            .is_some_and(|s| s.artifacts.delete_without_confirmation),
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
    let current = termf_core::load_settings(config_dir)
        .map_err(err)?
        .artifacts
        .refresh_hours;
    let hours = match (on, current) {
        (false, _) => 0,
        (true, 0) => 6,
        (true, hours) => hours,
    };
    termf_core::save_artifacts_refresh(config_dir, hours).map_err(err)?;
    Ok(artifacts_state(&app))
}

fn check_url(url: &str) -> Result<String> {
    artifacts::artifact_url(url)
        .map(|(_, url)| url)
        .ok_or_else(|| anyhow!("{url} is not a claude.ai artifact"))
}

#[tauri::command]
pub fn artifacts_confirm(app: AppHandle, skip: bool) -> CmdResult<Artifacts> {
    let config_dir = &app.state::<AppState>().config_dir;
    termf_core::save_artifacts_delete_without_confirmation(config_dir, skip).map_err(err)?;
    Ok(artifacts_state(&app))
}

/// The deletion goes ahead only once you confirmed it in TermF, or chose yourself to skip that confirmation.
fn may_delete(confirmed: bool, without_confirmation: bool) -> Result<()> {
    anyhow::ensure!(
        confirmed || without_confirmation,
        "confirm the deletion in TermF first"
    );
    Ok(())
}

/// Deletes the artifacts you confirmed in TermF through a hidden session: its hook allows `Artifact` delete on
/// these links only and refuses anything else, its settings deny the tools it does not need, and it runs in
/// the mode that asks, so whatever the hook does not answer shows its tile instead of going ahead.
#[tauri::command]
pub fn artifacts_delete(
    app: AppHandle,
    urls: Vec<String>,
    confirmed: bool,
) -> CmdResult<Artifacts> {
    let state = app.state::<AppState>();
    let settings = termf_core::load_settings(&state.config_dir).map_err(err)?;
    may_delete(confirmed, settings.artifacts.delete_without_confirmation).map_err(err)?;
    let urls = urls
        .iter()
        .map(|u| check_url(u))
        .collect::<Result<Vec<_>>>()
        .map_err(err)?;
    let claude = app.state::<ClaudeState>();
    let (ids, urls) = {
        let mut deletion = claude.deletion.lock().unwrap();
        deletion.begin(&urls, now()).map_err(err)?;
        let items = &deletion.items;
        (
            items.iter().map(|d| d.id.clone()).collect::<Vec<_>>(),
            items.iter().map(|d| d.url.clone()).collect::<Vec<_>>(),
        )
    };
    let spawned = (|| {
        let (agent, command) = claude_agent(&state.config_dir)?;
        std::fs::create_dir_all(&claude.agent_dir)?;
        let task = Task {
            agent,
            command: program(&command),
            args: delete_args(),
            prompt: artifacts::delete_prompt(&urls),
            hidden: true,
            policy: Some(Policy::Delete(ids)),
            settings: Some(task_settings()),
        };
        let title = "claude.ai artifacts".to_owned();
        api::spawn_task(&state, claude.agent_dir.clone(), title, task)
    })();
    {
        let mut deletion = claude.deletion.lock().unwrap();
        match &spawned {
            Ok(session) => deletion.session = Some(session.id),
            Err(e) => deletion.end(Some((Failure::Spawn, Some(format!("{e:#}"))))),
        }
    }
    changed(&app);
    Ok(artifacts_state(&app))
}

fn delete_args() -> Vec<String> {
    let mut args = list_args();
    args.extend(["--permission-mode".into(), "default".into()]);
    args
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
        policy: None,
        settings: None,
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
            policy: Some(Policy::List),
            settings: Some(task_settings()),
        };
        let title = "claude.ai artifacts".to_owned();
        api::spawn_task(&state, claude.agent_dir.clone(), title, task)
    })();
    {
        let mut refresh = claude.refresh.lock().unwrap();
        match &spawned {
            Ok(session) => refresh.session = Some(session.id),
            Err(e) => {
                refresh.end(now(), Err((Failure::Spawn, Some(format!("{e:#}")))));
                save_run(&claude, &refresh);
            }
        }
    }
    changed(app);
    spawned.map(|_| ())
}

fn save_run(claude: &ClaudeState, refresh: &Refresh) {
    let _lock = claude.index_lock.lock().unwrap();
    if let Err(e) = artifacts::save_run(&claude.index, refresh) {
        eprintln!("termf: artifacts refresh not saved: {e:#}");
    }
}

/// A small model, none of your MCP servers, and none of your settings files: `--restricted` skips them, while
/// `--settings` and `--plugin-dir`, which carry TermF's hooks, still apply.
fn list_args() -> Vec<String> {
    ["--model", "haiku", "--strict-mcp-config", "--restricted"]
        .map(String::from)
        .to_vec()
}

/// What TermF does with a hook of any of its sessions; `Some`: the hook's answer.
pub fn hook(app: &AppHandle, id: u32, event: &str, payload: &Value) -> Option<Value> {
    let claude = app.state::<ClaudeState>();
    if event == "PostToolUse" && payload["tool_name"] == "Artifact" {
        let recorded = {
            let _lock = claude.index_lock.lock().unwrap();
            artifacts::record(&claude.index, payload, now(), &claude.agent_dir)
        };
        match recorded {
            Ok(true) => changed(app),
            Ok(false) => {}
            Err(e) => eprintln!("termf: artifact not indexed: {e:#}"),
        }
    }
    let state = app.state::<AppState>();
    if event == "PreToolUse"
        && let Some(policy) = state.api.policy(id)
    {
        return Some(artifacts::decide(&policy, payload));
    }
    let step = claude.refresh.lock().unwrap().hook(id, event, payload);
    apply(app, id, step);
    let step = claude.deletion.lock().unwrap().hook(id, event, payload);
    apply_deletion(app, id, step);
    None
}

fn apply_deletion(app: &AppHandle, id: u32, step: Step) {
    let failure = match step {
        Step::Nothing | Step::Done(_) => return,
        Step::Reveal => {
            let state = app.state::<AppState>();
            state.api.reveal(app, &state.sessions, id);
            changed(app);
            return;
        }
        Step::Answered => None,
        Step::Fail(failure, detail) => Some((failure, detail)),
    };
    let claude = app.state::<ClaudeState>();
    if claude.deletion.lock().unwrap().finish(id, failure) {
        dispose(app, id);
    }
}

/// Carries out a step of the listing run of session `id`; a step of a run already over does nothing.
fn apply(app: &AppHandle, id: u32, step: Step) {
    let claude = app.state::<ClaudeState>();
    let state = app.state::<AppState>();
    let result = match step {
        Step::Nothing | Step::Answered => return,
        Step::Reveal => {
            state.api.reveal(app, &state.sessions, id);
            changed(app);
            return;
        }
        Step::Done(listed) => {
            let started = {
                let refresh = claude.refresh.lock().unwrap();
                if !refresh.owns(id) {
                    return;
                }
                refresh.started
            };
            let count = listed.len();
            let _lock = claude.index_lock.lock().unwrap();
            artifacts::apply_listing(&claude.index, listed, started, now())
                .map(|_| count)
                .map_err(|e| (Failure::Invalid, Some(format!("{e:#}"))))
        }
        Step::Fail(failure, detail) => Err((failure, detail)),
    };
    {
        let mut refresh = claude.refresh.lock().unwrap();
        if !refresh.finish(id, now(), result) {
            return;
        }
        save_run(&claude, &refresh);
    }
    dispose(app, id);
}

/// After the hook has its answer, the session goes, with its hook process.
fn dispose(app: &AppHandle, id: u32) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        let state = app.state::<AppState>();
        state.api.forget(id);
        let _ = state.sessions.kill(id);
        changed(&app);
    });
}

pub fn exited(app: &AppHandle, id: u32) {
    let claude = app.state::<ClaudeState>();
    let ours = {
        let mut refresh = claude.refresh.lock().unwrap();
        let ours = refresh.finish(id, now(), Err((Failure::Closed, None)));
        if ours {
            save_run(&claude, &refresh);
        }
        ours
    };
    let deleting = claude
        .deletion
        .lock()
        .unwrap()
        .finish(id, Some((Failure::Closed, None)));
    if ours || deleting {
        dispose(app, id);
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
            let (session, step) = {
                let mut deletion = claude.deletion.lock().unwrap();
                (deletion.session, deletion.tick(now()))
            };
            if let Some(id) = session {
                apply_deletion(&app, id, step);
            }
            let hours = termf_core::load_settings(&app.state::<AppState>().config_dir)
                .map_or(0, |s| s.artifacts.refresh_hours);
            let listed = {
                let _lock = claude.index_lock.lock().unwrap();
                artifacts::load(&claude.index).listed
            };
            let due = claude.refresh.lock().unwrap().due(now(), hours, listed);
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
        assert!(args.contains("--model haiku") && args.contains("--restricted"));
        assert!(
            !args.contains("allowedTools") && !args.contains("bypass") && !args.contains("skip")
        );
        assert!(
            delete_args()
                .join(" ")
                .ends_with("--permission-mode default")
        );
    }

    /// Without TermF's hook (too slow, or gone), no rule allows a tool (yours are not read, see `list_args`):
    /// Claude Code asks, and the tile shows.
    #[test]
    fn hidden_sessions_allow_nothing_by_their_settings() {
        let settings = task_settings();
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        assert!(deny.contains(&json!("Bash")) && !deny.contains(&json!("Artifact")));
        assert!(settings["permissions"]["allow"].is_null());
    }

    #[test]
    fn deletes_only_once_you_confirmed_or_chose_not_to_be_asked() {
        assert!(may_delete(false, false).is_err());
        assert!(may_delete(true, false).is_ok());
        assert!(may_delete(false, true).is_ok());
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
