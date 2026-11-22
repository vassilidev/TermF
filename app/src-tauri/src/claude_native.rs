//! The rest of the Claude view (ARCHITECTURE §11.3 d–g): Claude Code's own skills, agents, commands, styles,
//! rules and memory, with the export of library items into its folders; its saved and background sessions;
//! usage read from the status line of the sessions TermF starts; the account and the installation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_claude::native::{self, Deleted, Text};
use termf_claude::sessions::{self, Background, Project};
use termf_claude::settings::{Preview, Scope};
use termf_claude::usage::{self, Billing, Limits, Window};
use termf_claude::{Env, Files, account};
use termf_core::claude::start;
use termf_core::ipc::SessionSummary;
use termf_library::Kind;
use termf_term::shell_quote;

use crate::claude::{claude_agent, env};
use crate::{AppState, CmdResult, api, err};

pub struct NativeState {
    trash: PathBuf,
    usage: Mutex<Usage>,
    billing: Mutex<Option<Billing>>,
}

impl NativeState {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            trash: data_dir.join("trash/claude"),
            usage: Mutex::default(),
            billing: Mutex::default(),
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

// Skills, agents, commands, output styles, rules and memory.

#[tauri::command]
pub fn claude_files(state: State<AppState>, repo: Option<String>) -> Files {
    termf_claude::files(&env(&state.config_dir, repo))
}

#[tauri::command]
pub fn claude_file_read(
    state: State<AppState>,
    path: String,
    reveal: bool,
    repo: Option<String>,
) -> CmdResult<Text> {
    native::read(&env(&state.config_dir, repo), Path::new(&path), reveal).map_err(err)
}

/// `hash`: the file as the editor read it (`claude_file_read`); `null` for a file to create.
#[tauri::command]
pub fn claude_file_preview(
    state: State<AppState>,
    path: String,
    text: String,
    hash: Option<String>,
    repo: Option<String>,
) -> CmdResult<Preview> {
    let env = env(&state.config_dir, repo);
    native::preview(&env, Path::new(&path), &text, hash.as_deref()).map_err(err)
}

#[tauri::command]
pub fn claude_file_save(
    state: State<AppState>,
    path: String,
    text: String,
    hash: Option<String>,
    repo: Option<String>,
) -> CmdResult<Preview> {
    let env = env(&state.config_dir, repo);
    native::save(&env, Path::new(&path), &text, hash.as_deref()).map_err(err)
}

/// Returns the new file.
#[tauri::command]
pub fn claude_item_create(
    state: State<AppState>,
    kind: String,
    scope: Scope,
    name: String,
    description: String,
    repo: Option<String>,
) -> CmdResult<String> {
    let env = env(&state.config_dir, repo);
    native::create(&env, &kind, scope, &name, &description)
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(err)
}

/// Into TermF's trash, `<data>/trash/claude/`.
#[tauri::command]
pub fn claude_file_delete(
    state: State<AppState>,
    native_state: State<NativeState>,
    path: String,
    repo: Option<String>,
) -> CmdResult<Deleted> {
    let env = env(&state.config_dir, repo);
    native::delete(&env, Path::new(&path), &native_state.trash).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreview {
    /// The folder of a skill, else the file.
    dest: String,
    exists: bool,
    /// Of the main file, masked.
    diff: String,
}

fn export_dir(env: &Env, to: Scope) -> Result<PathBuf> {
    Ok(match to {
        Scope::User => env.claude_dir.clone(),
        Scope::Project => env
            .repo
            .as_ref()
            .context("choose a repository to export to its project scope")?
            .join(".claude"),
        _ => anyhow::bail!("an item goes to the user or the project scope"),
    })
}

fn library_item(
    state: &AppState,
    kind: Kind,
    name: &str,
    repo: Option<&str>,
    project: Option<String>,
) -> Result<termf_library::Item> {
    let scope = termf_library::Scope::new(&state.config_dir, repo.map(Path::new), project)?;
    termf_library::explain(&state.config_dir, &scope, kind, name)
}

/// Async, like the other commands that read the library: its stack may walk a large repository.
#[tauri::command]
pub async fn claude_export_preview(
    state: State<'_, AppState>,
    kind: Kind,
    name: String,
    to: Scope,
    repo: Option<String>,
    project: Option<String>,
) -> CmdResult<ExportPreview> {
    (|| {
        let item = library_item(&state, kind, &name, repo.as_deref(), project)?;
        let dir = export_dir(&env(&state.config_dir, repo), to)?;
        let (dest, main) = termf_library::export_target(&item, &dir)?;
        let before = std::fs::read_to_string(&main).unwrap_or_default();
        let after = termf_library::export_text(&item)?;
        Ok(ExportPreview {
            dest: dest.to_string_lossy().into_owned(),
            exists: dest.exists(),
            diff: native::mask_lines(&native::line_diff(&before, &after)),
        })
    })()
    .map_err(err)
}

/// Writes the item into Claude Code's folder. Something already there is replaced only with `overwrite`, once
/// you saw the diff, and goes to TermF's trash first.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn claude_export_item(
    state: State<'_, AppState>,
    native_state: State<'_, NativeState>,
    kind: Kind,
    name: String,
    to: Scope,
    overwrite: bool,
    repo: Option<String>,
    project: Option<String>,
) -> CmdResult<String> {
    (|| {
        let item = library_item(&state, kind, &name, repo.as_deref(), project)?;
        let dir = export_dir(&env(&state.config_dir, repo), to)?;
        let (dest, _) = termf_library::export_target(&item, &dir)?;
        if dest.exists() {
            ensure!(overwrite, "{} exists already", dest.display());
            // Written first next to it, so a failed export leaves the old one in place.
            let staging = dir.join(".termf-export");
            let _ = std::fs::remove_dir_all(&staging);
            let swapped = termf_library::export_claude(&item, &staging)
                .and_then(|fresh| native::replace(&dest, &fresh, &native_state.trash));
            let _ = std::fs::remove_dir_all(&staging);
            swapped?;
            return Ok(dest.to_string_lossy().into_owned());
        }
        let dest = termf_library::export_claude(&item, &dir)?;
        Ok(dest.to_string_lossy().into_owned())
    })()
    .map_err(err)
}

// Sessions.

#[tauri::command]
pub async fn claude_sessions(
    state: State<'_, AppState>,
    repo: Option<String>,
) -> CmdResult<Vec<Project>> {
    let env = env(&state.config_dir, repo.clone());
    blocking(move || Ok(sessions::list(&env, repo.as_deref().map(Path::new)))).await
}

/// Resumes a saved conversation in a tile, or a copy of it (`fork`), with the Claude Code agent's command.
#[tauri::command]
pub async fn claude_session_open(
    state: State<'_, AppState>,
    id: String,
    cwd: Option<String>,
    fork: bool,
) -> CmdResult<SessionSummary> {
    sessions::check_id(&id).map_err(err)?;
    let (agent, _) = claude_agent(&state.config_dir).map_err(err)?;
    // Elsewhere, Claude Code would resume it in the wrong folder.
    let cwd = cwd
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .ok_or("the folder this conversation ran in is gone: Claude Code resumes a session in its own folder")?;
    let title = format!(
        "{} · {agent}",
        cwd.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut args = vec!["--resume".to_owned(), id];
    if fork {
        args.push("--fork-session".into());
    }
    api::spawn(
        &state,
        Some(&agent),
        cwd,
        title,
        120,
        32,
        api::Remote::default(),
        None,
        &args,
    )
    .map_err(err)
}

#[tauri::command]
pub async fn claude_purge_preview(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let env = env(&state.config_dir, None);
    blocking(move || sessions::purge_preview(&env, Path::new(&path))).await
}

/// After the confirmation in TermF, which showed what `claude_purge_preview` said.
#[tauri::command]
pub async fn claude_purge(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let env = env(&state.config_dir, None);
    blocking(move || sessions::purge(&env, Path::new(&path))).await
}

#[tauri::command]
pub async fn claude_background(state: State<'_, AppState>) -> CmdResult<Background> {
    let env = env(&state.config_dir, None);
    blocking(move || Ok(sessions::background(&env))).await
}

/// How a tile's command starts Claude Code. Asking its version can take seconds.
async fn claude_program(state: &AppState) -> CmdResult<String> {
    let env = env(&state.config_dir, None);
    blocking(move || Ok(start(&termf_claude::cli::program(&env), shell_quote))).await
}

/// Opens a background session in a tile, with `claude attach`.
#[tauri::command]
pub async fn claude_background_attach(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<SessionSummary> {
    sessions::check_background(&id).map_err(err)?;
    let command = format!(
        "{} attach {}",
        claude_program(&state).await?,
        shell_quote(&id)
    );
    let home = termf_core::expand_tilde("~");
    api::spawn_command(&state, home, format!("claude attach {id}"), &command).map_err(err)
}

#[tauri::command]
pub async fn claude_background_stop(state: State<'_, AppState>, id: String) -> CmdResult<String> {
    let env = env(&state.config_dir, None);
    blocking(move || sessions::stop(&env, &id)).await
}

// Usage.

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SeenLimits {
    limits: Limits,
    /// When Claude Code said so, and where: the status line of a tile (`session`), or the `rate_limit_event` of
    /// a headless conversation (`conversation`).
    at: u64,
    session: Option<u32>,
    conversation: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsage {
    cost: Option<f64>,
    model: Option<String>,
    at: u64,
}

/// Only what the status lines of this run of TermF said: nothing is kept across restarts.
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    limits: Option<SeenLimits>,
    /// By tile.
    sessions: BTreeMap<u32, SessionUsage>,
}

/// The status line of a session TermF started: its cost and the account's limits.
pub fn status_line(app: &AppHandle, id: u32, payload: &Value) {
    let reading = usage::read(payload);
    let state = app.state::<NativeState>();
    let snapshot = {
        let mut usage = state.usage.lock().unwrap();
        let at = now();
        let limits = !reading.limits.is_empty();
        let mut changed = limits
            && usage
                .limits
                .as_ref()
                .is_none_or(|l| l.limits != reading.limits);
        if limits {
            usage.limits = Some(SeenLimits {
                limits: reading.limits,
                at,
                session: Some(id),
                conversation: None,
            });
        }
        changed |= usage
            .sessions
            .get(&id)
            .is_none_or(|s| s.cost != reading.cost || s.model != reading.model);
        usage.sessions.insert(
            id,
            SessionUsage {
                cost: reading.cost,
                model: reading.model,
                at,
            },
        );
        changed.then(|| usage.clone())
    };
    if let Some(usage) = snapshot {
        let _ = app.emit("claude-usage", usage);
    }
}

/// A headless session neared or hit a limit (`rate_limit_event`): that window is updated, the others kept.
pub fn rate_limit(
    app: &AppHandle,
    conversation: i64,
    status: &str,
    window: Option<&str>,
    resets_at: Option<u64>,
    used: Option<f64>,
) {
    let Some(used) = used.or((status == "rejected").then_some(100.0)) else {
        return;
    };
    let seen = Window {
        used_percentage: used,
        resets_at,
        used_usd: None,
        limit_usd: None,
    };
    let Some(state) = app.try_state::<NativeState>() else {
        return;
    };
    let snapshot = {
        let mut usage = state.usage.lock().unwrap();
        let mut limits = usage
            .limits
            .as_ref()
            .map(|l| l.limits.clone())
            .unwrap_or_default();
        match window {
            Some("five_hour") => limits.five_hour = Some(seen),
            Some("seven_day") => limits.seven_day = Some(seen),
            _ => return,
        }
        usage.limits = Some(SeenLimits {
            limits,
            at: now(),
            session: None,
            conversation: Some(conversation),
        });
        usage.clone()
    };
    let _ = app.emit("claude-usage", snapshot);
}

#[tauri::command]
pub fn claude_usage(native_state: State<NativeState>) -> Usage {
    native_state.usage.lock().unwrap().clone()
}

/// How the account pays, read once and kept; `refresh` reads it again (after logging in or out).
#[tauri::command]
pub async fn claude_billing(
    state: State<'_, AppState>,
    native_state: State<'_, NativeState>,
    refresh: Option<bool>,
) -> CmdResult<Billing> {
    if !refresh.unwrap_or(false)
        && let Some(billing) = native_state.billing.lock().unwrap().clone()
    {
        return Ok(billing);
    }
    let env = env(&state.config_dir, None);
    let billing = blocking(move || Ok(account::billing(&env))).await?;
    *native_state.billing.lock().unwrap() = Some(billing.clone());
    Ok(billing)
}

/// What the CTO reads before it starts a team (`usage` of `termf mcp`): how the account pays, the limits last
/// seen, and the window nearly used up, if any.
pub fn usage_for_agents(app: &AppHandle) -> Value {
    let native = app.state::<NativeState>();
    let billing = native.billing.lock().unwrap().clone().unwrap_or_else(|| {
        let state = app.state::<AppState>();
        let billing = account::billing(&env(&state.config_dir, None));
        *native.billing.lock().unwrap() = Some(billing.clone());
        billing
    });
    let seen = native.usage.lock().unwrap().limits.clone();
    let nearly = seen.as_ref().and_then(|s| {
        let (window, w) = s.limits.pressure(usage::NEARLY_USED)?;
        Some(serde_json::json!({"window": window, "usedPercentage": w.used_percentage, "resetsAt": w.resets_at}))
    });
    let advice = match (&nearly, billing.mode) {
        (Some(_), _) => {
            "A usage window is nearly used up: do not start a team run; answer directly, or advise a direct conversation with one role on a lighter model (sonnet or haiku, low effort), and tell the user why."
        }
        (None, "subscription") => {
            "The subscription's limits are fine. Dollar amounts are API-equivalent list prices: the subscription is not billed per use, it draws on these limits."
        }
        _ => "Dollar amounts are what the account is billed.",
    };
    serde_json::json!({
        "billing": billing,
        "limits": seen.as_ref().map(|s| &s.limits),
        "seenAt": seen.as_ref().map(|s| s.at),
        "nearlyUsedUp": nearly,
        "advice": advice,
    })
}

// Account and installation.

/// `claude auth login` in a tile: it opens a browser and may ask for a code, which only you can give.
#[tauri::command]
pub async fn claude_auth_login(
    state: State<'_, AppState>,
    method: Option<String>,
) -> CmdResult<SessionSummary> {
    let flag = match method.as_deref() {
        None | Some("claudeai") => "",
        Some("console") => " --console",
        Some("sso") => " --sso",
        Some(other) => return Err(format!("unknown login method {other}")),
    };
    let command = format!("{} auth login{flag}", claude_program(&state).await?);
    let home = termf_core::expand_tilde("~");
    api::spawn_command(&state, home, "claude auth login".into(), &command).map_err(err)
}

#[tauri::command]
pub async fn claude_auth_logout(state: State<'_, AppState>) -> CmdResult<String> {
    let env = env(&state.config_dir, None);
    blocking(move || account::logout(&env)).await
}

#[tauri::command]
pub async fn claude_update(state: State<'_, AppState>) -> CmdResult<String> {
    let env = env(&state.config_dir, None);
    blocking(move || account::update(&env)).await
}

#[tauri::command]
pub async fn claude_doctor(state: State<'_, AppState>, repo: Option<String>) -> CmdResult<String> {
    let env = env(&state.config_dir, repo);
    blocking(move || account::doctor(&env)).await
}
