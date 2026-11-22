//! Tickets and their runs, for the UI and, through the local API, for the CLI. A run's session is an ordinary
//! terminal session, opened in the run's worktree. Every change to a run emits `run-updated` with the run.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use termf_core::TicketCleanup;
use termf_core::ipc::SessionSummary;
use termf_tickets::{
    Configured, Delivery, DeliveryPreview, Inbox, McpServer, PrCheck, PrMerge, Resolution, Run,
    RunEvent, Sources, TicketSummary, Tickets, Trackers, Unpushed,
};

use crate::{AppState, CmdResult, api, err};

/// How often the pull requests of delivered runs are asked about.
const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);
/// The first time, once the window listens.
const FIRST_CHECK: Duration = Duration::from_secs(30);
/// A run's session idle for less than this may be the user's at work: `auto` leaves its run alone.
const QUIET: Duration = Duration::from_secs(30 * 60);

pub struct TicketsState {
    tickets: Arc<Tickets>,
    /// Run → its session, for this launch only: session ids start over with the app.
    sessions: Mutex<HashMap<i64, u32>>,
    /// The runs whose session is being resumed right now.
    resuming: Mutex<HashSet<i64>>,
    /// Held while pull requests are checked or a run cleaned up, so each happens once.
    checking: Mutex<()>,
}

impl TicketsState {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            tickets: Arc::new(Tickets::new(config_dir, data_dir)),
            sessions: Mutex::default(),
            resuming: Mutex::default(),
            checking: Mutex::default(),
        }
    }
}

/// A run being resumed, given back when dropped, whatever stopped the resume.
struct Resuming<'a>(&'a Mutex<HashSet<i64>>, i64);

impl Drop for Resuming<'_> {
    fn drop(&mut self) {
        self.0.lock().unwrap().remove(&self.1);
    }
}

/// A resumed run, and a doubt about its conversation to show the user.
#[derive(Serialize)]
pub struct Resumed {
    run: Run,
    warning: Option<String>,
}

pub(crate) fn tickets(app: &AppHandle) -> Arc<Tickets> {
    app.state::<TicketsState>().tickets.clone()
}

/// git and gh reach the network: never on the UI thread.
pub(crate) async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle) -> Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

/// The run with its session, while that session still exists.
fn with_session(app: &AppHandle, mut run: Run) -> Run {
    let session = app
        .state::<TicketsState>()
        .sessions
        .lock()
        .unwrap()
        .get(&run.id)
        .copied();
    let live = app.state::<AppState>().sessions.list();
    run.session_id = session.filter(|id| live.iter().any(|s| s.id == *id));
    run
}

pub(crate) fn updated(app: &AppHandle, run: &Run) {
    let _ = app.emit("run-updated", with_session(app, run.clone()));
}

/// Resolves and prepares the run, then opens its Claude Code session, pointed at the file with its task.
pub fn start(
    app: &AppHandle,
    input: &str,
    project: Option<&str>,
    repo: Option<&str>,
    cols: u16,
    rows: u16,
) -> Result<Run> {
    let state = app.state::<AppState>();
    let agent = termf_core::load_settings(&state.config_dir)?.ticket_agent()?;
    api::run_guard(&state)?;
    let tickets = tickets(app);
    let launch = tickets.start(input, project, repo)?;
    let run = &launch.run;
    let worktree = run.worktree.clone().context("the run has no worktree")?;
    let session = api::spawn(
        &state,
        Some(&agent),
        worktree.into(),
        format!("{} · {}", run.ticket.key, run.title),
        cols,
        rows,
        remote_of(run),
        Some(api::RunSession {
            project: &run.project,
            brief: &launch.brief,
            env: &launch.env,
            args: &launch.args,
        }),
        &[],
    );
    let session = match session {
        Ok(session) => session,
        Err(e) => {
            updated(app, &tickets.fail(run.id, &format!("{e:#}"))?);
            return Err(e);
        }
    };
    let sessions = &app.state::<TicketsState>().sessions;
    sessions.lock().unwrap().insert(run.id, session.id);
    state.keep_awake.update(app);
    let _ = app.emit("session-opened", &session);
    let run = with_session(app, tickets.session_opened(run.id, session.id)?);
    updated(app, &run);
    Ok(run)
}

pub fn deliver(app: &AppHandle, id: i64, confirm: &str) -> Result<Delivery> {
    let team = crate::team::team(app);
    let unmet = team.check_delivery(id)?;
    let tickets = tickets(app);
    let delivered = tickets.deliver(id, confirm);
    if let (Ok(_), Some(done)) = (&delivered, &unmet)
        && let Err(e) = team.delivered_without(id, done)
    {
        eprintln!("termf: run {id}: delivered, the decision not kept: {e:#}");
    }
    // Even a failed delivery may have pushed or opened the pull request.
    if let Ok(run) = tickets.run(id) {
        updated(app, &run);
    }
    let mut delivered = delivered?;
    delivered.run = with_session(app, delivered.run);
    Ok(delivered)
}

/// Opens a new session in the run's worktree that resumes its Claude Code conversation, with what its first
/// session had (setup, guard, library). Refused while a session of TermF is already in that conversation:
/// two processes on one conversation may lose messages.
pub fn resume(app: &AppHandle, id: i64, cols: u16, rows: u16) -> Result<Resumed> {
    let state = app.state::<AppState>();
    let agent = termf_core::load_settings(&state.config_dir)?.ticket_agent()?;
    api::run_guard(&state)?;
    let tickets = tickets(app);
    let ticket_state = app.state::<TicketsState>();
    // Kept until the new session is known, so two resumes of a run cannot both start one.
    ensure!(
        ticket_state.resuming.lock().unwrap().insert(id),
        "run {id} is being resumed already"
    );
    let _resuming = Resuming(&ticket_state.resuming, id);
    let launch = tickets.resume(id)?;
    let run = &launch.run;
    let conversation = run.claude_session.clone().unwrap_or_default();
    let live = state.api.list(&state.sessions);
    let mine = ticket_state.sessions.lock().unwrap().get(&id).copied();
    if let Some(open) = in_conversation(&live, mine, &conversation) {
        bail!(
            "run {id} already has a session in its conversation: {}",
            open.title
        );
    }
    let worktree = run.worktree.clone().context("the run has no worktree")?;
    let session = api::spawn(
        &state,
        Some(&agent),
        worktree.into(),
        format!("{} · {}", run.ticket.key, run.title),
        cols,
        rows,
        remote_of(run),
        Some(api::RunSession {
            project: &run.project,
            brief: &launch.brief,
            env: &launch.env,
            args: &launch.args,
        }),
        &[],
    )?;
    ticket_state.sessions.lock().unwrap().insert(id, session.id);
    state.keep_awake.update(app);
    let _ = app.emit("session-opened", &session);
    let run = with_session(app, tickets.session_resumed(id, session.id, &conversation)?);
    updated(app, &run);
    Ok(Resumed {
        run,
        warning: launch.warning,
    })
}

/// A run's tile follows its project's Remote Control, named after its ticket.
fn remote_of(run: &Run) -> api::Remote<'_> {
    api::Remote {
        project: Some(&run.project),
        ticket: Some(&run.ticket.key),
        run: Some(run.id),
        ..api::Remote::default()
    }
}

/// A session still running in the run's conversation: the run's own (`mine`), or any whose hooks say they are
/// in it, such as a tile where the user resumed it by hand.
fn in_conversation<'a>(
    live: &'a [SessionSummary],
    mine: Option<u32>,
    conversation: &str,
) -> Option<&'a SessionSummary> {
    live.iter().find(|s| {
        s.running && (Some(s.id) == mine || s.agent_session_id.as_deref() == Some(conversation))
    })
}

/// Keeps the conversation of a run's session, which `/clear` changes, for `resume`.
pub(crate) fn conversation(app: &AppHandle, session: u32, conversation: &str) {
    let run = {
        let state = app.state::<TicketsState>();
        let sessions = state.sessions.lock().unwrap();
        sessions
            .iter()
            .find(|(_, s)| **s == session)
            .map(|(r, _)| *r)
    };
    if let Some(run) = run
        && let Err(e) = tickets(app).conversation(run, conversation)
    {
        eprintln!("termf: conversation of run {run} not kept: {e:#}");
    }
}

/// Ends the run's session, so no agent is left working in a deleted folder.
fn stop_session(app: &AppHandle, id: i64) {
    let sessions = &app.state::<TicketsState>().sessions;
    let session = sessions.lock().unwrap().remove(&id);
    if let Some(session) = session {
        let state = app.state::<AppState>();
        state.api.forget(session);
        let _ = state.sessions.kill(session);
        state.keep_awake.update(app);
    }
}

/// The run's tile at work or used lately, or its team running: never closed behind the user's back.
fn busy(app: &AppHandle, id: i64) -> bool {
    let session = app
        .state::<TicketsState>()
        .sessions
        .lock()
        .unwrap()
        .get(&id)
        .copied();
    let state = app.state::<AppState>();
    let tile = session.is_some_and(|session| {
        let live = state
            .sessions
            .list()
            .iter()
            .any(|s| s.id == session && s.running);
        live && state.api.busy(session, QUIET)
    });
    tile || crate::team::team(app)
        .run(id)
        .is_ok_and(|run| run.state == termf_team::RunState::Running)
}

/// Closes the run; its session ends first.
pub fn close(app: &AppHandle, id: i64, force: bool) -> Result<Run> {
    let run = tickets(app).close(id, force, || stop_session(app, id))?;
    updated(app, &run);
    Ok(with_session(app, run))
}

/// Closes a run whose pull request was merged or closed, if nothing would be lost.
pub fn cleanup(app: &AppHandle, id: i64) -> Result<Run> {
    let tickets = tickets(app);
    let ticket_state = app.state::<TicketsState>();
    let checking = ticket_state.checking.lock().unwrap();
    let cleaned = tickets.cleanup(id, || stop_session(app, id));
    drop(checking);
    // Even a refused cleanup may have learnt that the pull request was merged.
    if let Ok(run) = tickets.run(id) {
        updated(app, &run);
    }
    Ok(with_session(app, cleaned?))
}

/// Asks GitHub about the pull requests of delivered runs, as `[tickets] cleanup` says (`off`: not at all),
/// and emits `run-cleanup` for each one merged or closed since the last check.
pub fn check(app: &AppHandle) -> Result<PrCheck> {
    let mode = termf_core::load_settings(&app.state::<AppState>().config_dir)?
        .tickets
        .cleanup;
    if mode == TicketCleanup::Off {
        return Ok(PrCheck::default());
    }
    let ticket_state = app.state::<TicketsState>();
    let checking = ticket_state.checking.lock().unwrap();
    let auto = mode == TicketCleanup::Auto;
    let mut check =
        tickets(app).after_merge(auto, |id| busy(app, id), |id| stop_session(app, id))?;
    drop(checking);
    for cleanup in &mut check.cleanups {
        cleanup.run = with_session(app, cleanup.run.clone());
        updated(app, &cleanup.run);
        let _ = app.emit("run-cleanup", &*cleanup);
    }
    Ok(check)
}

/// Checks the pull requests of delivered runs soon after startup, then every 15 minutes. Without a delivered
/// run whose pull request is open, a check reads the run database only.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    let _ = thread::Builder::new()
        .name("tickets-merged".into())
        .spawn(move || {
            thread::sleep(FIRST_CHECK);
            loop {
                match check(&app) {
                    Ok(check) => {
                        for e in check.errors {
                            eprintln!(
                                "termf: pull request of run {} not checked: {}",
                                e.run, e.message
                            );
                        }
                    }
                    Err(e) => eprintln!("termf: pull requests not checked: {e:#}"),
                }
                thread::sleep(CHECK_EVERY);
            }
        });
}

/// `refresh` asks the project's MCP sources again instead of reading their last answer, and GitHub Issues
/// without the ETag of its last answer.
#[tauri::command]
pub async fn tickets_inbox(
    app: AppHandle,
    project: Option<String>,
    refresh: Option<bool>,
) -> CmdResult<Inbox> {
    blocking(app, move |app| {
        tickets(app).inbox(project.as_deref(), refresh.unwrap_or(false))
    })
    .await
}

/// Whether the keychain holds an API token for this Jira site and e-mail.
#[tauri::command]
pub async fn tickets_jira_token(app: AppHandle, site: String, email: String) -> CmdResult<bool> {
    blocking(app, move |app| tickets(app).has_jira_token(&site, &email)).await
}

#[tauri::command]
pub async fn tickets_sources(app: AppHandle, project: String) -> CmdResult<Sources> {
    blocking(app, move |app| tickets(app).sources(&project)).await
}

#[tauri::command]
pub async fn tickets_sources_save(
    app: AppHandle,
    project: String,
    trackers: Trackers,
) -> CmdResult<Sources> {
    blocking(app, move |app| {
        tickets(app).save_sources(&project, &trackers)
    })
    .await
}

/// A few tickets of one source (`github`, `githubProject`, `jira` or `mcp:<name>`), with the sources as the
/// editor holds them, saved or not.
#[tauri::command]
pub async fn tickets_source_test(
    app: AppHandle,
    project: String,
    trackers: Trackers,
    source: String,
) -> CmdResult<Vec<TicketSummary>> {
    blocking(app, move |app| {
        tickets(app).test_source(&project, trackers, &source)
    })
    .await
}

#[tauri::command]
pub async fn tickets_jira_token_save(
    app: AppHandle,
    site: String,
    email: String,
    token: String,
) -> CmdResult<()> {
    blocking(app, move |app| {
        tickets(app).save_jira_token(&site, &email, &token)
    })
    .await
}

#[tauri::command]
pub async fn tickets_jira_token_forget(
    app: AppHandle,
    site: String,
    email: String,
) -> CmdResult<()> {
    blocking(app, move |app| {
        tickets(app).forget_jira_token(&site, &email)
    })
    .await
}

/// Whether the keychain holds a Linear (`kind` `linear`, `target` its workspace), GitLab (`gitlab`, its host)
/// or Azure DevOps (`azure`, its organization) token.
#[tauri::command]
pub async fn tickets_token(app: AppHandle, kind: String, target: String) -> CmdResult<bool> {
    blocking(app, move |app| tickets(app).has_token(&kind, &target)).await
}

#[tauri::command]
pub async fn tickets_token_save(
    app: AppHandle,
    kind: String,
    target: String,
    token: String,
) -> CmdResult<()> {
    blocking(app, move |app| {
        tickets(app).save_token(&kind, &target, &token)
    })
    .await
}

#[tauri::command]
pub async fn tickets_token_forget(app: AppHandle, kind: String, target: String) -> CmdResult<()> {
    blocking(app, move |app| tickets(app).forget_token(&kind, &target)).await
}

/// The MCP servers of the user's Claude Code (`claude mcp list`, which checks each one: slow).
#[tauri::command]
pub async fn tickets_mcp_servers(
    app: AppHandle,
    project: Option<String>,
) -> CmdResult<Vec<McpServer>> {
    blocking(app, move |app| tickets(app).mcp_servers(project.as_deref())).await
}

#[tauri::command]
pub async fn tickets_resolve(
    app: AppHandle,
    input: String,
    project: Option<String>,
) -> CmdResult<Resolution> {
    blocking(app, move |app| {
        tickets(app).resolve(&input, project.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn tickets_run(
    app: AppHandle,
    input: String,
    project: Option<String>,
    repo: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    session: Option<bool>,
) -> CmdResult<Run> {
    blocking(app, move |app| {
        // Its agent team works in the worktree instead: no Claude Code session at work beside it.
        if session == Some(false) {
            api::run_guard(&app.state::<AppState>())?;
            let run =
                tickets(app).start_without_session(&input, project.as_deref(), repo.as_deref())?;
            updated(app, &run);
            return Ok(run);
        }
        let (cols, rows) = (cols.unwrap_or(120), rows.unwrap_or(32));
        start(app, &input, project.as_deref(), repo.as_deref(), cols, rows)
    })
    .await
}

#[tauri::command]
pub async fn tickets_runs(app: AppHandle) -> CmdResult<Vec<Run>> {
    blocking(app, |app| {
        let runs = tickets(app).runs()?;
        Ok(runs.into_iter().map(|r| with_session(app, r)).collect())
    })
    .await
}

#[tauri::command]
pub async fn tickets_events(app: AppHandle, id: i64) -> CmdResult<Vec<RunEvent>> {
    blocking(app, move |app| tickets(app).events(id)).await
}

/// What a delivery would send; `done`: what the definition of done misses, when the run's team stopped short of its
/// delivery gate and the delivery is the user's.
#[derive(Serialize)]
pub struct Preview {
    #[serde(flatten)]
    preview: DeliveryPreview,
    done: Option<termf_team::done::Done>,
}

#[tauri::command]
pub async fn tickets_deliver_preview(app: AppHandle, id: i64) -> CmdResult<Preview> {
    blocking(app, move |app| {
        let done = crate::team::team(app).check_delivery(id)?;
        let preview = tickets(app).deliver_preview(id)?;
        Ok(Preview { preview, done })
    })
    .await
}

#[tauri::command]
pub async fn tickets_deliver(app: AppHandle, id: i64, confirm: String) -> CmdResult<Delivery> {
    blocking(app, move |app| deliver(app, id, &confirm)).await
}

/// Why a run was not closed. `uncommitted` or `unpushed`: closing would lose work (`uncommitted`: changes not
/// committed; `commits`: commits not pushed), which `force` drops. `error`: anything else.
#[derive(Serialize)]
pub struct CloseError {
    kind: &'static str,
    message: String,
    uncommitted: bool,
    commits: u32,
}

fn close_error(e: anyhow::Error) -> CloseError {
    match e.downcast_ref::<Unpushed>() {
        Some(left) => CloseError {
            kind: if left.uncommitted {
                "uncommitted"
            } else {
                "unpushed"
            },
            message: left.to_string(),
            uncommitted: left.uncommitted,
            commits: left.commits,
        },
        None => CloseError {
            kind: "error",
            message: err(e),
            uncommitted: false,
            commits: 0,
        },
    }
}

#[tauri::command]
pub async fn tickets_close(
    app: AppHandle,
    id: i64,
    force: Option<bool>,
) -> Result<Run, CloseError> {
    let closed =
        tauri::async_runtime::spawn_blocking(move || close(&app, id, force.unwrap_or(false)))
            .await
            .map_err(anyhow::Error::from)
            .and_then(|closed| closed);
    closed.map_err(close_error)
}

/// Closes a run whose pull request was merged or closed; refused, as `tickets_close` without force, while
/// work is not pushed, and while the pull request is open.
#[tauri::command]
pub async fn tickets_cleanup(app: AppHandle, id: i64) -> Result<Run, CloseError> {
    let cleaned = tauri::async_runtime::spawn_blocking(move || cleanup(&app, id))
        .await
        .map_err(anyhow::Error::from)
        .and_then(|cleaned| cleaned);
    cleaned.map_err(close_error)
}

/// What GitHub says of a delivered run's open pull request against its base; `null` when it has none open.
#[tauri::command]
pub async fn tickets_pr_merge(app: AppHandle, id: i64) -> CmdResult<Option<PrMerge>> {
    blocking(app, move |app| tickets(app).pr_merge(id)).await
}

/// The runs whose pull request was merged or closed since the last check (also `run-cleanup` events).
#[tauri::command]
pub async fn tickets_check(app: AppHandle) -> CmdResult<PrCheck> {
    blocking(app, check).await
}

#[tauri::command]
pub async fn tickets_resume(
    app: AppHandle,
    id: i64,
    cols: Option<u16>,
    rows: Option<u16>,
) -> CmdResult<Resumed> {
    blocking(app, move |app| {
        resume(app, id, cols.unwrap_or(120), rows.unwrap_or(32))
    })
    .await
}

/// For each project (or only `project`), whether its inbox has a source, and which.
#[tauri::command]
pub async fn tickets_configured(
    app: AppHandle,
    project: Option<String>,
) -> CmdResult<Vec<Configured>> {
    blocking(app, move |app| tickets(app).configured(project.as_deref())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_session_per_conversation() {
        let session = |id, running, conversation: Option<&str>| SessionSummary {
            id,
            title: format!("tile {id}"),
            cwd: "/w".into(),
            agent: Some("claude".into()),
            running,
            exit_code: None,
            status: None,
            remote_control: false,
            agent_session_id: conversation.map(Into::into),
            context: None,
            remote: None,
        };
        let c = "0b9c3c1e-5f0a-4d6b-9a57-3f1d2c4e8a10";
        let live = [
            session(1, false, Some(c)),
            session(2, true, None),
            session(3, true, Some("other")),
        ];
        assert!(
            in_conversation(&live, Some(1), c).is_none(),
            "an ended session leaves room"
        );
        assert_eq!(in_conversation(&live, Some(2), c).unwrap().id, 2);
        let by_hand = [session(4, true, Some(c))];
        assert_eq!(in_conversation(&by_hand, None, c).unwrap().id, 4);
        assert!(in_conversation(&live, None, c).is_none());
    }
}
