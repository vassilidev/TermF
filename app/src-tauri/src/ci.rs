//! CI/CD for the UI (ARCHITECTURE §13.8). Every run read goes through `observe`, which follows the runs that
//! concern the user while they run and emits `ci-finished` with each one that passes or fails; `ci_poll`, which
//! the UI calls every minute, asks GitHub only about those.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager, State};
use termf_ci::{
    AgentMessage, Ci, Filter, Input, JobLog, RunDetail, RunPage, Watch, Workflow, WorkflowRun,
};
use termf_core::CiNotify;
use termf_tickets::Tickets;

use crate::tickets::blocking;
use crate::{AppState, CmdResult, err};

pub struct CiState {
    ci: Arc<Ci>,
    watch: Mutex<Watch>,
}

impl CiState {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            ci: Arc::new(Ci::new(Tickets::new(config_dir, data_dir))),
            watch: Mutex::default(),
        }
    }
}

fn ci(app: &AppHandle) -> Arc<Ci> {
    app.state::<CiState>().ci.clone()
}

fn notify(app: &AppHandle) -> CiNotify {
    let settings = termf_core::load_settings(&app.state::<AppState>().config_dir);
    settings.map(|s| s.ci.notify).unwrap_or_default()
}

fn observe(app: &AppHandle, runs: &[WorkflowRun]) {
    let notify = notify(app);
    if notify == CiNotify::Off {
        return;
    }
    let ci = ci(app);
    // Who the user is matters only for runs still running: no question to GitHub otherwise.
    let (me, branches) = if notify == CiNotify::Mine && runs.iter().any(|r| r.status != "completed")
    {
        (ci.me(), ci.ticket_branches())
    } else {
        (None, Vec::new())
    };
    let finished = app
        .state::<CiState>()
        .watch
        .lock()
        .unwrap()
        .observe(runs, |run| {
            notify == CiNotify::All || termf_ci::concerns(run, me.as_deref(), &branches)
        });
    for run in finished {
        let _ = app.emit("ci-finished", &run);
    }
}

#[tauri::command]
pub async fn ci_runs(
    app: AppHandle,
    repo: String,
    branch: Option<String>,
    status: Option<String>,
    pr: Option<u64>,
    page: Option<u32>,
) -> CmdResult<RunPage> {
    blocking(app, move |app| {
        let filter = Filter {
            branch,
            status,
            pr,
            page: page.unwrap_or(1),
        };
        let page = ci(app).runs(&repo, &filter)?;
        observe(app, &page.runs);
        Ok(page)
    })
    .await
}

#[tauri::command]
pub async fn ci_run(app: AppHandle, repo: String, id: u64) -> CmdResult<RunDetail> {
    blocking(app, move |app| {
        let detail = ci(app).run(&repo, id)?;
        observe(app, std::slice::from_ref(&detail.run));
        Ok(detail)
    })
    .await
}

#[tauri::command]
pub async fn ci_log(app: AppHandle, repo: String, job: u64) -> CmdResult<JobLog> {
    blocking(app, move |app| ci(app).log(&repo, job)).await
}

#[tauri::command]
pub async fn ci_rerun(app: AppHandle, repo: String, id: u64, failed: bool) -> CmdResult<()> {
    blocking(app, move |app| ci(app).rerun(&repo, id, failed)).await
}

#[tauri::command]
pub async fn ci_cancel(app: AppHandle, repo: String, id: u64) -> CmdResult<()> {
    blocking(app, move |app| ci(app).cancel(&repo, id)).await
}

#[tauri::command]
pub async fn ci_workflows(app: AppHandle, repo: String) -> CmdResult<Vec<Workflow>> {
    blocking(app, move |app| ci(app).workflows(&repo)).await
}

#[tauri::command]
pub async fn ci_inputs(
    app: AppHandle,
    repo: String,
    path: String,
    git_ref: Option<String>,
) -> CmdResult<Vec<Input>> {
    blocking(app, move |app| {
        ci(app).inputs(&repo, &path, git_ref.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn ci_dispatch(
    app: AppHandle,
    repo: String,
    workflow: String,
    git_ref: Option<String>,
    inputs: BTreeMap<String, String>,
) -> CmdResult<()> {
    blocking(app, move |app| {
        ci(app).dispatch(&repo, &workflow, git_ref.as_deref(), &inputs)
    })
    .await
}

/// The checks of a ticket run's pull request.
#[tauri::command]
pub async fn ci_checks(app: AppHandle, run: i64) -> CmdResult<Vec<WorkflowRun>> {
    blocking(app, move |app| {
        let runs = ci(app).checks(run)?;
        observe(app, &runs);
        Ok(runs)
    })
    .await
}

#[tauri::command]
pub async fn ci_failure(app: AppHandle, run: i64, job: u64) -> CmdResult<AgentMessage> {
    blocking(app, move |app| ci(app).failure_message(run, job)).await
}

/// Types `text` then Enter in a session, as `termf say` does.
#[tauri::command]
pub fn ci_send(
    app: AppHandle,
    state: State<AppState>,
    session: u32,
    text: String,
) -> CmdResult<()> {
    state.sessions.type_line(session, &text).map_err(err)?;
    state.api.input(&app, session, b"\r");
    Ok(())
}

#[tauri::command]
pub async fn ci_poll(app: AppHandle) -> CmdResult<()> {
    blocking(app, |app| {
        let tracked = app.state::<CiState>().watch.lock().unwrap().tracked();
        let ci = ci(app);
        for (repo, id) in tracked {
            // shortcut: a run that can no longer be read (deleted, access lost) is asked about until TermF
            // restarts; drop it on a 404 if that ever shows.
            if let Ok(run) = ci.workflow_run(&repo, id) {
                observe(app, &[run]);
            }
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn ci_notify(app: AppHandle) -> CiNotify {
    notify(&app)
}

#[tauri::command]
pub fn ci_notify_set(app: AppHandle, notify: CiNotify) -> CmdResult<CiNotify> {
    let state = app.state::<AppState>();
    termf_core::save_ci_notify(&state.config_dir, notify).map_err(err)?;
    if notify == CiNotify::Off {
        app.state::<CiState>().watch.lock().unwrap().clear();
    }
    Ok(notify)
}
