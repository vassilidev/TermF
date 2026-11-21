mod api;
mod config;
mod files;
mod keep_awake;
mod library;
mod quick_terminal;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody, Response};
use tauri::{AppHandle, Emitter, Manager, State};
use termf_core::ipc::SessionSummary;
use termf_core::{Project, Repo};
use termf_files::Files;
use termf_term::{Output, Sessions};

struct AppState {
    config_dir: PathBuf,
    sessions: Sessions,
    quick_terminal: quick_terminal::QuickTerminal,
    api: api::Api,
    keep_awake: keep_awake::KeepAwake,
}

type CmdResult<T> = Result<T, String>;

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Workspace {
    config_dir: String,
    home: String,
    agents: Vec<String>,
    projects: Vec<Project>,
    repos: Vec<Repo>,
    scan_truncated: bool,
    quick_terminal_error: Option<String>,
    /// Why the local API (CLI, agent status) is off.
    api_error: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ExitEvent {
    id: u32,
    exit_code: Option<u32>,
}

// async so the disk scan runs off the main thread and never freezes the window.
#[tauri::command]
async fn workspace(
    state: State<'_, AppState>,
    files: State<'_, Arc<Files>>,
) -> CmdResult<Workspace> {
    let settings = termf_core::load_settings(&state.config_dir).map_err(err)?;
    let discovery = termf_core::discover_repos(&settings.roots, settings.scan_depth);
    let projects = termf_core::load_projects(&state.config_dir).map_err(err)?;
    let repos = discovery
        .repos
        .iter()
        .chain(projects.iter().flat_map(|p| &p.repos));
    files.set_roots(repos.map(|r| &r.path));
    Ok(Workspace {
        config_dir: state.config_dir.to_string_lossy().into_owned(),
        home: termf_core::expand_tilde("~").to_string_lossy().into_owned(),
        agents: settings.agents.keys().cloned().collect(),
        projects,
        repos: discovery.repos,
        scan_truncated: discovery.truncated,
        quick_terminal_error: state.quick_terminal.error.lock().unwrap().clone(),
        api_error: state.api.error.clone(),
    })
}

/// `agent: None` opens a plain login shell.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn pty_spawn(
    app: AppHandle,
    state: State<AppState>,
    agent: Option<String>,
    cwd: String,
    title: String,
    cols: u16,
    rows: u16,
    remote_control: Option<bool>,
) -> CmdResult<SessionSummary> {
    let remote_control = remote_control.unwrap_or(false);
    let session = api::spawn(
        &state,
        agent.as_deref(),
        cwd.into(),
        title,
        cols,
        rows,
        remote_control,
    )
    .map_err(err)?;
    state.keep_awake.update(&app);
    Ok(session)
}

#[tauri::command]
fn pty_attach(
    app: AppHandle,
    state: State<AppState>,
    id: u32,
    on_data: Channel<InvokeResponseBody>,
) -> CmdResult<Response> {
    let snapshot = state
        .sessions
        .attach(
            id,
            Box::new(move |out| match out {
                // A closed channel only means the view went away; the session keeps running.
                Output::Data(bytes) => {
                    let _ = on_data.send(InvokeResponseBody::Raw(bytes));
                }
                Output::Exit(exit_code) => {
                    let _ = app.emit("pty-exit", ExitEvent { id, exit_code });
                }
            }),
        )
        .map_err(err)?;
    Ok(Response::new(snapshot))
}

#[tauri::command]
fn pty_write(app: AppHandle, state: State<AppState>, id: u32, data: String) -> CmdResult<()> {
    state.sessions.write(id, data.as_bytes()).map_err(err)?;
    state.api.input(&app, id, data.as_bytes());
    Ok(())
}

#[tauri::command]
fn pty_resize(state: State<AppState>, id: u32, cols: u16, rows: u16) -> CmdResult<()> {
    state.sessions.resize(id, cols, rows).map_err(err)
}

#[tauri::command]
fn pty_kill(app: AppHandle, state: State<AppState>, id: u32) -> CmdResult<()> {
    state.api.forget(id);
    state.sessions.kill(id).map_err(err)?;
    state.keep_awake.update(&app);
    Ok(())
}

#[tauri::command]
fn pty_list(state: State<AppState>) -> Vec<SessionSummary> {
    // The quick terminal has its own panel, never a tile.
    let quick = *state.quick_terminal.session.lock().unwrap();
    state
        .api
        .list(&state.sessions)
        .into_iter()
        .filter(|s| Some(s.id) != quick)
        .collect()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = termf_core::default_config_dir().expect("cannot locate the config directory");
    tauri::Builder::default()
        // First, so a second launch only focuses this window, before setup touches the socket or the sessions.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .manage(Arc::new(Files::default()))
        .plugin(quick_terminal::plugin())
        // One setup: a second call to `setup` would replace this one.
        .setup(move |app| {
            let settings = termf_core::load_settings(&config_dir).unwrap_or_default();
            // The UI and the terminals work without the local API.
            let (api, listener) = api::Api::new();
            app.manage(AppState {
                config_dir,
                sessions: api.sessions(app.handle().clone()),
                quick_terminal: Default::default(),
                api,
                keep_awake: Default::default(),
            });
            app.state::<AppState>().keep_awake.configure(
                app.handle(),
                settings.keep_awake,
                settings.keep_display_on,
            );
            if let Some(listener) = listener
                && let Err(e) = api::serve(app.handle().clone(), listener)
            {
                eprintln!("termf: local API disabled: {e:#}");
            }
            quick_terminal::register(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace,
            pty_spawn,
            pty_attach,
            pty_write,
            pty_resize,
            pty_kill,
            pty_list,
            files::files_list,
            files::files_status,
            files::files_find,
            files::files_search,
            files::files_read,
            files::files_stat,
            files::files_write,
            files::files_head,
            files::files_open_external,
            config::project_save,
            config::project_delete,
            config::layouts,
            config::layout_save,
            config::layout_delete,
            quick_terminal::quick_terminal,
            keep_awake::keep_awake_state,
            keep_awake::keep_awake_set,
            library::library_list,
            library::library_toggle,
            library::library_explain,
            library::library_import_claude,
            library::config_export,
            library::config_import_preview,
            library::config_import_apply,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
