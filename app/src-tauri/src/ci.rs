//! CI/CD for the UI (ARCHITECTURE §13.8). Every run read goes through `observe`, which follows the runs that
//! concern the user while they run and emits `ci-finished` with each one that passes or fails; `ci_poll`, which
//! the UI calls every minute, asks GitHub only about those.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use tauri::{AppHandle, Emitter, Manager, State};
use termf_ci::deploy::{Deploy, Deployment, Target, ToolInfo};
use termf_ci::{
    AgentMessage, Ci, Delivery, Filter, Hook, Input, JobLog, Node, RunDetail, RunPage, Watch,
    Workflow, WorkflowRun,
};
use termf_core::CiNotify;
use termf_tickets::Tickets;

use crate::tickets::blocking;
use crate::{AppState, CmdResult, err};

pub struct CiState {
    ci: Arc<Ci>,
    watch: Mutex<Watch>,
    deploy: Deploy,
}

impl CiState {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            ci: Arc::new(Ci::new(Tickets::new(config_dir, data_dir))),
            watch: Mutex::default(),
            deploy: Deploy::default(),
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

#[tauri::command]
pub async fn ci_graph(
    app: AppHandle,
    repo: String,
    path: String,
    sha: String,
) -> CmdResult<Vec<Node>> {
    blocking(app, move |app| ci(app).graph(&repo, &path, &sha)).await
}

#[tauri::command]
pub async fn ci_hooks(app: AppHandle, repo: String) -> CmdResult<Vec<Hook>> {
    blocking(app, move |app| ci(app).hooks(&repo)).await
}

#[tauri::command]
pub async fn ci_deliveries(app: AppHandle, repo: String, hook: u64) -> CmdResult<Vec<Delivery>> {
    blocking(app, move |app| ci(app).deliveries(&repo, hook)).await
}

#[tauri::command]
pub async fn ci_redeliver(app: AppHandle, repo: String, hook: u64, delivery: u64) -> CmdResult<()> {
    blocking(app, move |app| ci(app).redeliver(&repo, hook, delivery)).await
}

/// Asked to the user first, in the UI.
#[tauri::command]
pub async fn ci_hook_create(
    app: AppHandle,
    repo: String,
    url: String,
    events: Vec<String>,
) -> CmdResult<Hook> {
    blocking(app, move |app| ci(app).create_hook(&repo, &url, &events)).await
}

/// Asked to the user first, in the UI.
#[tauri::command]
pub async fn ci_hook_update(
    app: AppHandle,
    repo: String,
    hook: u64,
    url: String,
    events: Vec<String>,
) -> CmdResult<()> {
    blocking(app, move |app| {
        ci(app).update_hook(&repo, hook, &url, &events)
    })
    .await
}

/// Asked to the user first, in the UI.
#[tauri::command]
pub async fn ci_hook_delete(app: AppHandle, repo: String, hook: u64) -> CmdResult<()> {
    blocking(app, move |app| ci(app).delete_hook(&repo, hook)).await
}

#[tauri::command]
pub async fn deploy_tools(app: AppHandle) -> CmdResult<Vec<ToolInfo>> {
    blocking(app, |app| Ok(app.state::<CiState>().deploy.tools())).await
}

#[tauri::command]
pub fn deploy_targets(state: State<AppState>, project: String) -> CmdResult<Vec<Target>> {
    termf_ci::deploy::targets(&state.config_dir, &project).map_err(err)
}

#[tauri::command]
pub fn deploy_target_save(
    state: State<AppState>,
    project: String,
    target: Target,
    previous: Option<String>,
) -> CmdResult<Vec<Target>> {
    termf_ci::deploy::save_target(&state.config_dir, &project, &target, previous.as_deref())
        .map_err(err)
}

#[tauri::command]
pub fn deploy_target_delete(
    state: State<AppState>,
    project: String,
    name: String,
) -> CmdResult<Vec<Target>> {
    termf_ci::deploy::delete_target(&state.config_dir, &project, &name).map_err(err)
}

/// The target, and its token from the vault when it names one: the project's secret, else the global one.
fn target_and_token(
    app: &AppHandle,
    project: &str,
    name: &str,
) -> Result<(Target, Option<termf_vault::Secret>)> {
    let config = &app.state::<AppState>().config_dir;
    let target = termf_ci::deploy::targets(config, project)?
        .into_iter()
        .find(|t| t.name == name)
        .with_context(|| format!("no deploy target named {name}"))?;
    let Some(secret) = target.token.clone() else {
        // A Forge target that names no token deploys with the project's Forge account, when it has one.
        if target.tool == termf_ci::deploy::Tool::Forge {
            let vault = crate::vault::shared(app);
            return Ok((
                target,
                crate::forge::account_token(config, &vault, project)?,
            ));
        }
        return Ok((target, None));
    };
    let vault = crate::vault::shared(app);
    if !vault.initialized() {
        bail!("there is no vault yet: create it in the Vault view, then add {secret} there");
    }
    let scope = termf_vault::Scope::parse(&target.env)?;
    let human = termf_vault::Actor::human();
    let id = termf_vault::SecretId::new(&secret, scope, Some(project))?;
    let token = match vault.value(&human, &id, termf_vault::Access::Use) {
        Err(e) if format!("{e:#}").starts_with("no secret") => {
            let global = termf_vault::SecretId::new(&secret, scope, None)?;
            vault
                .value(&human, &global, termf_vault::Access::Use)
                .with_context(|| format!("no secret {secret} in the vault ({})", target.env))?
        }
        other => other?,
    };
    Ok((target, Some(token)))
}

/// Runs a deploy tool for a target with its token; what it printed, and why it failed, come back through `hide`,
/// which takes out the vault's values and the token.
fn deploying<T>(
    app: &AppHandle,
    project: &str,
    name: &str,
    f: impl FnOnce(&Deploy, &Target, Option<&str>, &dyn Fn(String) -> String) -> Result<T>,
) -> Result<T> {
    let (target, token) = target_and_token(app, project, name)?;
    let mut masker = crate::vault::masker(app).unwrap_or_else(|| termf_vault::Masker::new([]));
    if let Some(token) = &token {
        masker.add(token);
    }
    let hide = |text: String| masker.mask(&text).text;
    let expose = token.as_ref().map(termf_vault::Secret::expose);
    f(&app.state::<CiState>().deploy, &target, expose, &hide)
        .map_err(|e| anyhow::anyhow!(hide(format!("{e:#}"))))
}

#[tauri::command]
pub async fn deploy_list(
    app: AppHandle,
    project: String,
    name: String,
) -> CmdResult<Vec<Deployment>> {
    blocking(app, move |app| {
        deploying(app, &project, &name, |deploy, target, token, hide| {
            let found = deploy.list(target, token)?.into_iter();
            Ok(found
                .map(|mut d| {
                    d.url = d.url.map(hide);
                    d.description = d.description.map(hide);
                    d.by = d.by.map(hide);
                    d
                })
                .collect())
        })
    })
    .await
}

#[tauri::command]
pub async fn deploy_logs(
    app: AppHandle,
    project: String,
    name: String,
    id: String,
) -> CmdResult<Vec<String>> {
    blocking(app, move |app| {
        deploying(app, &project, &name, |deploy, target, token, hide| {
            let text = hide(deploy.logs(target, &id, token)?.join("\n"));
            Ok(text.lines().map(str::to_owned).collect())
        })
    })
    .await
}

/// Approved by the user first, in the UI: a deployment does not come undone.
#[tauri::command]
pub async fn deploy_redeploy(
    app: AppHandle,
    project: String,
    name: String,
    id: String,
) -> CmdResult<String> {
    blocking(app, move |app| {
        deploying(app, &project, &name, |deploy, target, token, hide| {
            Ok(hide(deploy.redeploy(target, &id, token)?))
        })
    })
    .await
}
