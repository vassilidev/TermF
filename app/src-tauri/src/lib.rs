mod files;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody, Response};
use tauri::{AppHandle, Emitter, State};
use termf_core::{Project, Repo};
use termf_files::Files;
use termf_term::{Output, SessionInfo, Sessions, SpawnSpec, shell_command};

struct AppState {
    config_dir: PathBuf,
    sessions: Sessions,
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
    })
}

/// `agent: None` opens a plain login shell.
#[tauri::command]
fn pty_spawn(
    state: State<AppState>,
    agent: Option<String>,
    cwd: String,
    title: String,
    cols: u16,
    rows: u16,
) -> CmdResult<SessionInfo> {
    let command = match &agent {
        Some(name) => {
            let settings = termf_core::load_settings(&state.config_dir).map_err(err)?;
            Some(
                settings
                    .agents
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("unknown agent {name}"))?,
            )
        }
        None => None,
    };
    let (program, args) = shell_command(command.as_deref());
    let id = state
        .sessions
        .spawn(SpawnSpec {
            title: title.clone(),
            program,
            args,
            cwd: cwd.clone().into(),
            cols,
            rows,
        })
        .map_err(err)?;
    Ok(SessionInfo {
        id,
        title,
        cwd,
        running: true,
        exit_code: None,
    })
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
fn pty_write(state: State<AppState>, id: u32, data: String) -> CmdResult<()> {
    state.sessions.write(id, data.as_bytes()).map_err(err)
}

#[tauri::command]
fn pty_resize(state: State<AppState>, id: u32, cols: u16, rows: u16) -> CmdResult<()> {
    state.sessions.resize(id, cols, rows).map_err(err)
}

#[tauri::command]
fn pty_kill(state: State<AppState>, id: u32) -> CmdResult<()> {
    state.sessions.kill(id).map_err(err)
}

#[tauri::command]
fn pty_list(state: State<AppState>) -> Vec<SessionInfo> {
    state.sessions.list()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config_dir = termf_core::default_config_dir().expect("cannot locate the config directory");
    tauri::Builder::default()
        .manage(AppState {
            config_dir,
            sessions: Sessions::default(),
        })
        .manage(Arc::new(Files::default()))
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
            files::files_write,
            files::files_head,
            files::files_open_external,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
