//! The project memory for the UI and the local API, kept up to date in the background, and the expert's answers
//! to the CLI and the other agents (`ask_expert`): a headless session of the `expert` role that TermF starts and
//! waits for. What it stores and what it hands out is masked with the vault's values too while it is open.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager};
use termf_agents::{AgentSession, SessionSpec, State};
use termf_core::MemorySettings;
use termf_core::ipc::MemoryCall;
use termf_memory::{Hit, Memory, Page, Paths, Source, Status};

use crate::tickets::blocking;
use crate::{CmdResult, agents, vault};

/// The first indexing waits for the app to settle; the following ones catch up with what changed.
const FIRST: Duration = Duration::from_secs(60);
const EVERY: Duration = Duration::from_secs(15 * 60);
/// What one question may cost and take, unless the caller gives a smaller budget.
const ASK_BUDGET: f64 = 1.0;
const ASK_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub struct MemoryState {
    paths: Paths,
    /// One indexing at a time: two would read and write the same documents.
    indexing: Mutex<()>,
    /// Expert conversations with a message kept for the end of their turn: brought up to date when it goes.
    queued: Mutex<HashSet<i64>>,
}

pub fn start(app: &AppHandle, config_dir: PathBuf, data_dir: PathBuf) -> MemoryState {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST);
        loop {
            if let Err(e) = index(&app, None) {
                eprintln!("termf: project memory not indexed: {e:#}");
            }
            std::thread::sleep(EVERY);
        }
    });
    MemoryState {
        paths: Paths::new(config_dir, data_dir),
        indexing: Mutex::new(()),
        queued: Mutex::default(),
    }
}

/// The vault's values, while it is open; `termf_memory::clean` adds the patterns.
fn vault_values(app: &AppHandle) -> impl Fn(&str) -> String {
    let masker = vault::masker(app);
    move |text: &str| {
        masker
            .as_ref()
            .map_or_else(|| text.to_owned(), |m| m.mask(text).text)
    }
}

/// Again on the way out: a value stored in the vault after the document was indexed.
fn masking(app: &AppHandle) -> impl Fn(&str) -> String {
    let values = vault_values(app);
    move |text: &str| termf_memory::clean(text, &values)
}

fn state(app: &AppHandle) -> &MemoryState {
    app.state::<MemoryState>().inner()
}

/// `project`, or every project when `None`; then their status.
fn index(app: &AppHandle, project: Option<&str>) -> Result<Vec<Status>> {
    let _one = state(app).indexing.lock().unwrap();
    index_held(app, project)
}

/// [`index`], the indexing lock already held.
fn index_held(app: &AppHandle, project: Option<&str>) -> Result<Vec<Status>> {
    let state = state(app);
    let memory = Memory::open(&state.paths.data_dir)?;
    let mask = vault_values(app);
    let projects = match project {
        Some(id) => {
            memory.index(&state.paths, id, &mask)?;
            vec![id.to_owned()]
        }
        None => memory
            .index_all(&state.paths, &mask)?
            .into_iter()
            .map(|(id, result)| {
                if let Err(e) = result {
                    eprintln!("termf: memory of {id} not indexed: {e:#}");
                }
                id
            })
            .collect(),
    };
    projects
        .iter()
        .map(|id| memory.status(&state.paths.config_dir, id))
        .collect()
}

pub fn search(
    app: &AppHandle,
    project: &str,
    query: &str,
    sources: &[Source],
    limit: usize,
) -> Result<Vec<Hit>> {
    let state = state(app);
    let settings = termf_core::load_settings(&state.paths.config_dir)?.memory;
    let memory = Memory::open(&state.paths.data_dir)?;
    let mask = masking(app);
    let mut hits = memory.search(&settings, project, query, sources, limit)?;
    for hit in &mut hits {
        hit.snippet = mask(&hit.snippet);
        hit.title = mask(&hit.title);
    }
    Ok(hits)
}

pub fn read(app: &AppHandle, project: &str, id: i64, offset: usize) -> Result<Page> {
    let state = state(app);
    let settings = termf_core::load_settings(&state.paths.config_dir)?.memory;
    let memory = Memory::open(&state.paths.data_dir)?;
    let mut page = memory.read(&settings, project, id, offset)?;
    page.text = masking(app)(&page.text);
    Ok(page)
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    /// The expert's headless session, in the Conversations.
    pub session: i64,
    pub text: String,
    pub cost: f64,
}

/// Asks the project's expert and waits for its answer, with the memory brought up to date first.
pub fn ask(app: &AppHandle, project: &str, question: &str, budget: Option<f64>) -> Result<Answer> {
    let question = question.trim();
    ensure!(!question.is_empty(), "the question is empty");
    let config_dir = &state(app).paths.config_dir;
    let repo = termf_core::load_projects(config_dir)?
        .into_iter()
        .find(|p| p.id == project)
        .with_context(|| format!("unknown project {project}"))?
        .repos
        .into_iter()
        .next()
        .with_context(|| format!("the project {project} has no repository"))?;
    if let Err(e) = index(app, Some(project)) {
        eprintln!("termf: memory of {project} not brought up to date: {e:#}");
    }
    let title: String = question.chars().take(60).collect();
    let spec = SessionSpec {
        cwd: repo.path.into(),
        title: format!("expert · {title}"),
        project: Some(project.to_owned()),
        role: Some(termf_agents::EXPERT.into()),
        mcp: true,
        budget: Some(budget.unwrap_or(ASK_BUDGET).clamp(0.01, ASK_BUDGET)),
        ..SessionSpec::default()
    };
    let (session, text) = agents::agents(app)
        .consult(spec, question, ASK_TIMEOUT)
        .context("the expert did not answer")?;
    Ok(Answer {
        session: session.id,
        text,
        cost: session.cost,
    })
}

/// Before a message reaches a project's expert conversation, as before `ask_expert`: the memory brought up to date.
/// Only for a message the session takes now: one kept for the end of a turn is handled by [`delivering`]. It
/// never waits for an indexing already running (all projects, every 15 min): the message goes without it.
pub fn before_expert(app: &AppHandle, session: &AgentSession, text: &str) {
    let takes = matches!(session.state, State::Idle | State::Stopped | State::Error);
    if expert(session) && takes && !text.trim().is_empty() {
        refresh(app, session);
    }
}

/// A message to the expert waits for the end of its turn: the memory is brought up to date when it goes.
pub fn expert_queued(app: &AppHandle, session: &AgentSession) {
    if expert(session) {
        state(app).queued.lock().unwrap().insert(session.id);
    }
}

/// A session's change, right before TermF hands it what waited for the end of its turn.
pub fn delivering(app: &AppHandle, session: &AgentSession) {
    let Some(memory) = app.try_state::<MemoryState>() else {
        return;
    };
    if session.state == State::Idle && memory.queued.lock().unwrap().remove(&session.id) {
        refresh(app, session);
    }
}

fn expert(session: &AgentSession) -> bool {
    session.spec.mcp && session.spec.role.as_deref() == Some(termf_agents::EXPERT)
}

fn refresh(app: &AppHandle, session: &AgentSession) {
    let Some(project) = session.spec.project.as_deref() else {
        return;
    };
    let Ok(_one) = state(app).indexing.try_lock() else {
        return;
    };
    if let Err(e) = index_held(app, Some(project)) {
        eprintln!("termf: memory of {project} not brought up to date: {e:#}");
    }
}

/// What the CLI asks through the local API.
pub fn call(app: &AppHandle, call: MemoryCall) -> Result<Value> {
    Ok(match call {
        MemoryCall::Reindex { project } => serde_json::to_value(index(app, project.as_deref())?)?,
        MemoryCall::Ask {
            project,
            question,
            budget,
        } => serde_json::to_value(ask(app, &project, &question, budget)?)?,
    })
}

/// The project's expert conversation, created the first time.
#[tauri::command]
pub async fn expert_open(app: AppHandle, project: String) -> CmdResult<AgentSession> {
    blocking(app, move |app| {
        crate::cto::standing(app, &project, termf_agents::EXPERT)
    })
    .await
}

#[tauri::command]
pub async fn memory_status(app: AppHandle, project: String) -> CmdResult<Status> {
    blocking(app, move |app| {
        let state = state(app);
        Memory::open(&state.paths.data_dir)?.status(&state.paths.config_dir, &project)
    })
    .await
}

/// Saves the settings, drops what they leave out from every project at once, then indexes the project given.
#[tauri::command]
pub async fn memory_settings_save(
    app: AppHandle,
    project: Option<String>,
    settings: MemorySettings,
) -> CmdResult<Option<Status>> {
    blocking(app, move |app| {
        for source in &settings.exclude {
            Source::parse(source)?;
        }
        let state = state(app);
        termf_core::save_memory(&state.paths.config_dir, &settings)?;
        {
            // After an indexing under way, which read the old settings.
            let _one = state.indexing.lock().unwrap();
            Memory::open(&state.paths.data_dir)?.forget(&settings)?;
        }
        match project {
            Some(id) => Ok(index(app, Some(&id))?.pop()),
            None => Ok(None),
        }
    })
    .await
}

#[tauri::command]
pub async fn memory_reindex(app: AppHandle, project: String) -> CmdResult<Status> {
    blocking(app, move |app| {
        index(app, Some(&project))?.pop().context("no status")
    })
    .await
}
