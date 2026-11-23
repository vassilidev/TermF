//! Scheduled tasks while TermF is open: a thread looks every minute for the ones due and runs them, headless in
//! the background or as a session in a tile. Every change to a run emits `schedule-run` with the run.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use termf_schedule::{
    DesktopTask, Execution, Host, Mode, Outcome, Scheduler, Status, Task, Trigger,
};

use crate::{AppState, CmdResult, api, err};

pub struct ScheduleState(Arc<Scheduler>);

/// Manages the scheduler, before anything can call it, then checks the tasks every minute.
pub fn start(app: &AppHandle, config_dir: PathBuf, data_dir: PathBuf) {
    let scheduler = Arc::new(Scheduler::new(config_dir, data_dir));
    app.manage(ScheduleState(scheduler.clone()));
    let app = app.clone();
    let _ = thread::Builder::new()
        .name("schedule".into())
        .spawn(move || {
            loop {
                tick(&app, &scheduler);
                // Just past the next minute; it is also how soon a time missed while asleep is caught up.
                let wait = 61 - termf_schedule::now().rem_euclid(60);
                thread::sleep(Duration::from_secs(wait as u64));
            }
        });
}

fn scheduler(app: &AppHandle) -> Arc<Scheduler> {
    app.state::<ScheduleState>().0.clone()
}

/// Also fails the runs whose process is gone and brings the system's entries up to date (an update waits while
/// its job runs).
fn tick(app: &AppHandle, s: &Arc<Scheduler>) {
    if let Err(e) = s.interrupted() {
        eprintln!("termf: scheduled tasks: {e:#}");
    }
    if let Some(termf) = api::termf_binary() {
        s.refresh_entries(&termf);
    }
    let tasks = match s.tasks() {
        Ok(tasks) => tasks,
        Err(e) => return eprintln!("termf: scheduled tasks: {e:#}"),
    };
    let now = termf_schedule::now();
    for (id, task) in tasks {
        let started = s.due(&id, &task, now).and_then(|due| match due {
            Some((slot, trigger)) => launch(app, s, &id, &task, trigger, Some(slot)),
            None => Ok(None),
        });
        if let Err(e) = started {
            eprintln!("termf: scheduled task {id}: {e:#}");
        }
    }
}

fn updated(app: &AppHandle, run: &Execution) {
    let _ = app.emit("schedule-run", run);
}

fn failed(e: anyhow::Error) -> Outcome {
    Outcome {
        status: Status::Failed,
        cost: None,
        output: None,
        error: Some(format!("{e:#}")),
    }
}

/// Claims the run, then starts it; `None` when that time already ran (the system's scheduler was first).
fn launch(
    app: &AppHandle,
    s: &Arc<Scheduler>,
    id: &str,
    task: &Task,
    trigger: Trigger,
    slot: Option<i64>,
) -> Result<Option<Execution>> {
    // Read first: a run claimed and never started would show as running until the next launch.
    let settings = termf_core::load_settings(&app.state::<AppState>().config_dir)?;
    let Some(run) = s.claim(id, task, trigger, slot, Host::App)? else {
        return Ok(None);
    };
    updated(app, &run);
    if task.mode == Mode::Tile {
        let outcome = match open_tile(app, task) {
            Ok(()) => Outcome {
                status: Status::Opened,
                cost: None,
                output: None,
                error: None,
            },
            Err(e) => failed(e),
        };
        let run = s.finish(run.id, &outcome)?;
        updated(app, &run);
        return Ok(Some(run));
    }
    let agent = termf_schedule::claude_command(&settings);
    let (app, s, task, started) = (app.clone(), s.clone(), task.clone(), run.clone());
    thread::spawn(move || {
        let done = s
            .run_headless(&started, &task, &agent)
            .or_else(|e| s.finish(started.id, &failed(e)));
        match done {
            Ok(done) => updated(&app, &done),
            Err(e) => eprintln!("termf: scheduled run {}: {e:#}", started.id),
        }
    });
    Ok(Some(run))
}

fn open_tile(app: &AppHandle, task: &Task) -> Result<()> {
    let state = app.state::<AppState>();
    let agent = termf_schedule::claude_agent(&termf_core::load_settings(&state.config_dir)?)?;
    let session = api::spawn(
        &state,
        Some(&agent),
        task.dir.clone().into(),
        task.name.clone(),
        120,
        32,
        api::Remote::default(),
        None,
        &termf_schedule::tile_args(task),
    )?;
    state.keep_awake.update(app);
    app.emit("session-opened", &session)?;
    Ok(())
}

/// Runs a task now, for the UI and the CLI. `scheduled`: only when one of its times is due and has not run.
pub fn run(app: &AppHandle, id: &str, scheduled: bool) -> Result<Option<Execution>> {
    let s = scheduler(app);
    let task = s.task(id)?;
    if !scheduled {
        return launch(app, &s, id, &task, Trigger::Manual, None);
    }
    match s.due(id, &task, termf_schedule::now())? {
        Some((slot, trigger)) => launch(app, &s, id, &task, trigger, Some(slot)),
        None => Ok(None),
    }
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &Scheduler) -> Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app, &scheduler(&app)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(err)
}

/// A new task that runs on this machine only, as one made in the Scheduler; its id. The view learns of it by
/// `schedule-saved`.
pub(crate) fn save_here(app: &AppHandle, mut task: Task) -> Result<String> {
    let s = scheduler(app);
    task.machine = Some(s.machine.clone());
    let id = s.save(None, task, api::termf_binary().as_deref())?;
    let _ = app.emit("schedule-saved", &id);
    Ok(id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEntry {
    id: String,
    #[serde(flatten)]
    task: Task,
    /// The system's scheduler also runs it while TermF is closed.
    installed: bool,
    /// Its next run, when it is on.
    next: Option<i64>,
    last: Option<Execution>,
}

#[tauri::command]
pub async fn schedule_tasks(app: AppHandle) -> CmdResult<Vec<TaskEntry>> {
    blocking(app, |_, s| {
        let now = termf_schedule::now();
        s.tasks()?
            .into_iter()
            .map(|(id, task)| {
                let next = termf_schedule::upcoming(&task.cron, now, 1).ok();
                Ok(TaskEntry {
                    installed: s.installed(&id),
                    next: next
                        .filter(|_| task.enabled && s.runs_here(&task))
                        .and_then(|n| n.first().copied()),
                    last: s.history(Some(&id), 1)?.pop(),
                    id,
                    task,
                })
            })
            .collect()
    })
    .await
}

#[tauri::command]
pub async fn schedule_save(app: AppHandle, id: Option<String>, task: Task) -> CmdResult<String> {
    blocking(app, move |_, s| {
        s.save(id.as_deref(), task, api::termf_binary().as_deref())
    })
    .await
}

#[tauri::command]
pub async fn schedule_delete(app: AppHandle, id: String) -> CmdResult<()> {
    blocking(app, move |_, s| s.delete(&id)).await
}

/// The next `count` runs of a cron expression, to show while it is typed.
#[tauri::command]
pub fn schedule_preview(cron: String, count: Option<usize>) -> CmdResult<Vec<i64>> {
    termf_schedule::upcoming(&cron, termf_schedule::now(), count.unwrap_or(5)).map_err(err)
}

#[tauri::command]
pub async fn schedule_run(app: AppHandle, id: String) -> CmdResult<Execution> {
    blocking(app, move |app, _| {
        run(app, &id, false)?.context("the run did not start")
    })
    .await
}

#[tauri::command]
pub async fn schedule_history(
    app: AppHandle,
    task: Option<String>,
    limit: Option<usize>,
) -> CmdResult<Vec<Execution>> {
    blocking(app, move |_, s| {
        s.history(task.as_deref(), limit.unwrap_or(50))
    })
    .await
}

/// Has the system's scheduler run the task while TermF is closed (`on`), or stop; returns whether it does.
#[tauri::command]
pub async fn schedule_install(app: AppHandle, id: String, on: bool) -> CmdResult<bool> {
    blocking(app, move |_, s| {
        if on {
            let termf = api::termf_binary().context(
                "running while TermF is closed needs the termf CLI next to TermF (`pnpm bundle`, or `cargo build -p termf-cli` in development)",
            )?;
            s.install(&id, &termf)?;
        } else {
            s.uninstall(&id)?;
        }
        Ok(s.installed(&id))
    })
    .await
}

/// This machine's name, which tasks that run on one machine name.
#[tauri::command]
pub async fn schedule_machine(app: AppHandle) -> CmdResult<String> {
    blocking(app, |_, s| Ok(s.machine.clone())).await
}

/// Claude Desktop's scheduled tasks, read only.
#[tauri::command]
pub async fn schedule_desktop(app: AppHandle) -> CmdResult<Vec<DesktopTask>> {
    blocking(app, |_, _| {
        Ok(termf_schedule::desktop_tasks(&termf_schedule::claude_dir()))
    })
    .await
}
