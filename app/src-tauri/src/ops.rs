//! Ops for the UI and for the agents (`termf-ops`, ARCHITECTURE §12.3–§12.5): a project's connections and
//! recipes, runs by the user, and the agents' `ops_run`, which waits for the user's answer to its card
//! (`ops-approval`, then `ops-approval-done`). A masked value of the vault reaches the webview only through
//! `ops_reveal`, once the master password was typed within 5 minutes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_agents::AgentSession;
use termf_core::ipc::McpCall;
use termf_ops::export::Format;
use termf_ops::{
    APPROVAL_WAIT, Agent, Approvals, Check, Connection, Following, Ops, Output, Plan, Recipe,
    Request, Runner, Streamed, Use,
};
use termf_tickets::RunStatus;
use termf_vault::{Action, Actor, Entry, Event, NeedsPassword, Secret, Vault};

use crate::{CmdResult, err};

/// How long a result's masked values of the vault stay revealable.
const REVEALABLE: Duration = Duration::from_secs(600);
/// The query results kept for their CSV export: masked, they hold no secret, so no deadline, only a count.
const TABLES: usize = 20;

/// A result as the user may still use it: the values masked in it (a `.env`'s, by key).
struct Kept {
    at: Instant,
    project: String,
    plan: Plan,
    hidden: Vec<Secret>,
    keys: Vec<String>,
}

pub struct OpsState {
    pub(crate) config_dir: PathBuf,
    pub(crate) data_dir: PathBuf,
    approvals: Arc<Approvals>,
    pub(crate) runner: Runner,
    next: AtomicU64,
    /// A result's id → what `ops_reveal` and the `.env` commands take from it.
    kept: Mutex<BTreeMap<u64, Kept>>,
    /// A query result's id → its masked table, for `ops_sql_export`: the last [`TABLES`].
    tables: Mutex<BTreeMap<u64, KeptTable>>,
    /// The streams followed, by the id the front end chose: dropping one stops its client.
    follows: Mutex<BTreeMap<u64, Following>>,
    /// The passwords of the databases the last `ops_detect` found, by their rank: they go to the vault, never to
    /// the webview.
    found: Mutex<Vec<Option<Secret>>>,
}

pub fn state(config_dir: PathBuf, data_dir: PathBuf) -> OpsState {
    OpsState {
        config_dir,
        data_dir,
        approvals: Arc::default(),
        runner: Runner::default(),
        next: AtomicU64::new(1),
        kept: Mutex::default(),
        tables: Mutex::default(),
        follows: Mutex::default(),
        found: Mutex::default(),
    }
}

/// A query's masked table and the database it came from: its export writes that database's SQL.
type KeptTable = (termf_ops::sql::Table, termf_ops::Kind);

/// Keeps a table for its export, the oldest dropped past [`TABLES`].
fn keep_table(tables: &mut BTreeMap<u64, KeptTable>, id: u64, table: KeptTable) {
    tables.insert(id, table);
    while tables.len() > TABLES {
        tables.pop_first();
    }
}

/// An error for the front end: a query the SQL console refuses is `{"refused": {code, subject}, "message"}`, which
/// it words in its language; anything else, the message.
fn failed(e: anyhow::Error) -> String {
    match e.downcast_ref::<termf_ops::sql::Refused>() {
        Some(r) => serde_json::json!({ "refused": r, "message": r.to_string() }).to_string(),
        None => err(e),
    }
}

/// Stops every followed stream as the app quits.
pub fn shutdown(app: &AppHandle) {
    if let Some(state) = app.try_state::<OpsState>() {
        state.follows.lock().unwrap().clear();
    }
}

impl OpsState {
    fn ops<'a>(&'a self, vault: &'a Vault) -> Ops<'a> {
        Ops {
            config_dir: &self.config_dir,
            data_dir: &self.data_dir,
            vault,
            runner: &self.runner,
        }
    }
}

pub(crate) async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppHandle, &OpsState, &Vault) -> Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(move || {
        let vault = crate::vault::shared(&app);
        f(&app, &app.state::<OpsState>(), &vault)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(failed)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    connections: Vec<Connection>,
    recipes: Vec<Recipe>,
    /// Recipe files that do not read.
    errors: Vec<String>,
    queries: Vec<termf_ops::Query>,
    /// Saved queries that do not read, left out.
    query_errors: Vec<String>,
}

#[tauri::command]
pub fn ops_overview(state: State<OpsState>, project: String) -> CmdResult<Overview> {
    let connections = termf_ops::connections(&state.config_dir, &project).map_err(err)?;
    let (recipes, errors) = termf_ops::recipes(&state.config_dir, Some(&project));
    let (queries, query_errors) = termf_ops::queries(&state.config_dir, &project).map_err(err)?;
    Ok(Overview {
        connections,
        recipes,
        errors,
        queries,
        query_errors,
    })
}

/// The password waiting to go to the vault with a connection: typed or pasted in a URL, or found by `ops_detect`.
fn pending(state: &OpsState, password: Option<String>, found: Option<usize>) -> Option<Secret> {
    let value = match (password, found) {
        (Some(p), _) => Some(Secret::new(p)),
        (None, Some(i)) => state.found.lock().unwrap().get(i).cloned().flatten(),
        (None, None) => None,
    };
    value.filter(|v| !v.expose().is_empty())
}

/// The password a connection's test uses for `name` before it is in the vault: the one waiting, for its own
/// `password` secret.
fn waiting_for(connection: &Connection, name: &str, waiting: Option<&Secret>) -> Option<Secret> {
    waiting
        .filter(|_| connection.secrets.get("password").map(String::as_str) == Some(name))
        .cloned()
}

/// Why a password saved with `connection` must not go to the vault unless the user says to replace it: its secret
/// is another connection's, or the vault already holds it (`exists`) and it was not this connection's.
fn clash(
    all: &[Connection],
    connection: &Connection,
    previous: Option<&str>,
    exists: bool,
) -> Option<String> {
    let name = connection.secrets.get("password")?;
    if let Some(owner) = termf_ops::secret_owner(all, connection, previous) {
        return Some(format!(
            "the secret {name} already holds the password of {}: name another secret, or replace it",
            owner.name
        ));
    }
    let mine = previous
        .and_then(|p| all.iter().find(|c| c.name == p))
        .is_some_and(|c| c.env == connection.env && c.secrets.get("password") == Some(name));
    (exists && !mine).then(|| {
        format!(
            "the vault already has the secret {name} ({}): name another secret, or replace it",
            connection.env.as_str()
        )
    })
}

/// Saves a connection; `password` (typed or pasted in a URL) or `found` (a database `ops_detect` found) goes to
/// the vault under the name its `password` secret gives, in its environment and the project, once the connection
/// is known to save; a secret that is another connection's, or already in the vault, is replaced only with
/// `replace`.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ops_connection_save(
    app: AppHandle,
    project: String,
    connection: Connection,
    previous: Option<String>,
    password: Option<String>,
    found: Option<usize>,
    replace: Option<bool>,
) -> CmdResult<Vec<Connection>> {
    blocking(app, move |_, state, vault| {
        let all = termf_ops::check_save(
            &state.config_dir,
            &project,
            &connection,
            previous.as_deref(),
        )?;
        if let Some(value) = pending(state, password, found) {
            let name = connection
                .secrets
                .get("password")
                .context("name the vault's secret that holds the password")?;
            ensure!(
                vault.initialized(),
                "there is no vault yet: create it in the Vault view, the password goes there"
            );
            let exists = vault
                .list(Some(connection.env), Some(&project))?
                .iter()
                .any(|s| &s.name == name && s.project.as_deref() == Some(project.as_str()));
            if replace != Some(true)
                && let Some(why) = clash(&all, &connection, previous.as_deref(), exists)
            {
                bail!(why);
            }
            let id = termf_vault::SecretId::new(name, connection.env, Some(&project))?;
            vault.set(
                &Actor::human(),
                &id,
                value,
                Some(&format!("Password of the database {}", connection.name)),
            )?;
        }
        termf_ops::save_connection(
            &state.config_dir,
            &project,
            &connection,
            previous.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub fn ops_connection_delete(
    state: State<OpsState>,
    project: String,
    name: String,
) -> CmdResult<Vec<Connection>> {
    termf_ops::delete_connection(&state.config_dir, &project, &name).map_err(err)
}

/// Tests a connection as the form holds it, saved or not; `password` or `found`: its password, not yet in the vault.
#[tauri::command]
pub async fn ops_test(
    app: AppHandle,
    project: String,
    connection: Connection,
    previous: Option<String>,
    password: Option<String>,
    found: Option<usize>,
) -> CmdResult<Vec<Check>> {
    blocking(app, move |_, state, vault| {
        let mut all = termf_ops::connections(&state.config_dir, &project)?;
        all.retain(|c| Some(&c.name) != previous.as_ref() && c.name != connection.name);
        all.push(connection.clone());
        let human = Actor::human();
        let waiting = pending(state, password, found);
        let secret = |name: &str| -> Result<Secret> {
            if let Some(value) = waiting_for(&connection, name, waiting.as_ref()) {
                return Ok(value);
            }
            if !vault.initialized() {
                bail!("there is no vault yet: create it in the Vault view, then add {name} there");
            }
            let id = termf_vault::SecretId::new(name, connection.env, Some(&project))?;
            match vault.value(&human, &id, termf_vault::Access::Use) {
                Err(e) if format!("{e:#}").starts_with("no secret") => {
                    let global = termf_vault::SecretId::new(name, connection.env, None)?;
                    vault
                        .value(&human, &global, termf_vault::Access::Use)
                        .with_context(|| {
                            format!(
                                "no secret {name} in the vault ({})",
                                connection.env.as_str()
                            )
                        })
                }
                other => other,
            }
        };
        let mut checks = state.runner.test(&connection, &all, &secret)?;
        if let Ok(masker) = vault.masker() {
            for c in &mut checks {
                c.detail = masker.mask(&c.detail).text;
            }
        }
        Ok(checks)
    })
    .await
}

#[tauri::command]
pub async fn ops_plan(
    app: AppHandle,
    project: String,
    recipe: String,
    connection: String,
    params: BTreeMap<String, String>,
) -> CmdResult<Plan> {
    blocking(app, move |_, state, vault| {
        state
            .ops(vault)
            .plan(&project, &recipe, &connection, &params)
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ran {
    /// What `ops_reveal` takes.
    id: u64,
    plan: Plan,
    output: Output,
}

/// The user's run; `proposal`: the agent's proposal it answers, then dropped.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ops_run(
    app: AppHandle,
    project: String,
    recipe: String,
    connection: String,
    params: BTreeMap<String, String>,
    confirm: Option<String>,
    proposal: Option<u64>,
) -> CmdResult<Ran> {
    blocking(app, move |app, state, vault| {
        let (plan, mut output) = state.ops(vault).run(
            &project,
            &Actor::human(),
            &recipe,
            &connection,
            &params,
            confirm.as_deref(),
        )?;
        if let Some(done) = proposal.and_then(|p| state.approvals.done(p)) {
            let _ = app.emit("ops-approval-done", &done);
        }
        let id = state.next.fetch_add(1, Ordering::Relaxed);
        if let Some(table) = &output.table {
            keep_table(
                &mut state.tables.lock().unwrap(),
                id,
                (table.clone(), plan.kind),
            );
        }
        let mut kept = state.kept.lock().unwrap();
        kept.retain(|_, k| k.at.elapsed() < REVEALABLE);
        kept.insert(
            id,
            Kept {
                at: Instant::now(),
                project,
                plan: plan.clone(),
                hidden: std::mem::take(&mut output.hidden),
                keys: output.keys.clone(),
            },
        );
        Ok(Ran { id, plan, output })
    })
    .await
}

/// A value of a result still kept, once the master password was typed within 5 minutes: its key (a `.env`'s),
/// the value, and where it came from, for the audit.
struct Taken {
    key: Option<String>,
    value: Secret,
    project: String,
    plan: Plan,
}

fn take(state: &OpsState, vault: &Vault, result: u64, index: usize) -> Result<Taken> {
    let taken = {
        let kept = state.kept.lock().unwrap();
        let found = kept
            .get(&result)
            .filter(|k| k.at.elapsed() < REVEALABLE)
            .context("this result is too old to use its values: run it again")?;
        Taken {
            value: found
                .hidden
                .get(index)
                .context("no such value in this result")?
                .clone(),
            key: found.keys.get(index).cloned(),
            project: found.project.clone(),
            plan: found.plan.clone(),
        }
    };
    if taken.key.is_some() {
        ensure!(
            vault.initialized(),
            "a value of a .env shows after the vault's master password: create the vault first"
        );
    }
    if vault.status()?.prod_until.is_none() {
        bail!(NeedsPassword);
    }
    Ok(taken)
}

/// The value at `index` to show or copy, audited before it leaves: a `.env`'s by its key and what is done with it
/// (in the Ops history), a value of the vault masked in an output as a reveal of the vault.
fn value(
    state: &OpsState,
    vault: &Vault,
    result: u64,
    index: usize,
    what: Use,
) -> Result<(String, Secret)> {
    let Taken {
        key,
        value,
        project,
        plan,
    } = take(state, vault, result, index)?;
    match &key {
        Some(key) => {
            state
                .ops(vault)
                .record_use(&project, &plan, what, key, value.len(), None)?;
        }
        None => {
            ensure!(
                what == Use::Reveal,
                "only a .env's value is copied or written"
            );
            vault.record(Event {
                size: Some(value.len() as u64),
                detail: Some("a value of the vault masked in a recipe's output".into()),
                ..Event::new(&Actor::human(), Action::Reveal)
            })?;
        }
    }
    Ok((key.unwrap_or_default(), value))
}

/// A value masked in a result: only with the master password typed within 5 minutes, audited.
#[tauri::command]
pub async fn ops_reveal(app: AppHandle, result: u64, index: usize) -> CmdResult<String> {
    blocking(app, move |_, state, vault| {
        let (_, value) = value(state, vault, result, index, Use::Reveal)?;
        Ok(value.expose().to_owned())
    })
    .await
}

/// A `.env`'s value to copy: the front end clears the clipboard after 30 s, as the vault's copies.
#[tauri::command]
pub async fn ops_env_copy(app: AppHandle, result: u64, index: usize) -> CmdResult<String> {
    blocking(app, move |_, state, vault| {
        let (_, value) = value(state, vault, result, index, Use::Copy)?;
        Ok(value.expose().to_owned())
    })
    .await
}

/// Writes a `.env`'s value into the `.env` of a run's worktree of the same project: 0600, hidden from git, gone
/// with the worktree when the run ends.
#[tauri::command]
pub async fn ops_env_write(
    app: AppHandle,
    result: u64,
    index: usize,
    run: i64,
) -> CmdResult<String> {
    blocking(app, move |app, state, vault| {
        let found = crate::tickets::tickets(app).run(run)?;
        let project = state
            .kept
            .lock()
            .unwrap()
            .get(&result)
            .map(|k| k.project.clone());
        ensure!(
            project.as_deref() == Some(found.project.as_str()),
            "run {run} is not one of this project's"
        );
        let open = matches!(found.status, RunStatus::Running | RunStatus::Delivered);
        let worktree = found
            .worktree
            .filter(|_| open)
            .with_context(|| format!("run {run} has no worktree left"))?;
        let taken = take(state, vault, result, index)?;
        let key = taken.key.context("only a .env's value is written")?;
        // Audited once done, with why when it was not: the value stays in TermF either way.
        let written = termf_ops::env::write(std::path::Path::new(&worktree), &key, &taken.value);
        state.ops(vault).record_use(
            &taken.project,
            &taken.plan,
            Use::Write(run),
            &key,
            taken.value.len(),
            written.as_ref().err().map(|e| format!("{e:#}")),
        )?;
        written?;
        Ok(worktree)
    })
    .await
}

/// A `.env` result against a local file of one of the project's repositories, by the hashes of the values.
#[tauri::command]
pub async fn ops_env_compare(
    app: AppHandle,
    result: u64,
    repo: String,
    file: String,
) -> CmdResult<Vec<termf_ops::env::Diff>> {
    blocking(app, move |_, state, _| {
        let kept = state.kept.lock().unwrap();
        let found = kept
            .get(&result)
            .filter(|k| k.at.elapsed() < REVEALABLE && !k.keys.is_empty())
            .context("this result is too old to compare: run it again")?;
        let project = termf_core::load_projects(&state.config_dir)?
            .into_iter()
            .find(|p| p.id == found.project)
            .context("no such project")?;
        ensure!(
            project.repos.iter().any(|r| r.path == repo),
            "{repo} is not a repository of {}",
            project.name
        );
        let path = termf_ops::env::inside(std::path::Path::new(&repo), &file)?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => termf_vault::Zeroizing::new(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                termf_vault::Zeroizing::new(String::new())
            }
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        };
        let prod: Vec<(String, Secret)> = found
            .keys
            .iter()
            .cloned()
            .zip(found.hidden.iter().cloned())
            .collect();
        Ok(termf_ops::env::compare(
            &prod,
            &termf_ops::env::parse(&text),
        ))
    })
    .await
}

fn kept_table(state: &OpsState, result: u64) -> Result<KeptTable> {
    state
        .tables
        .lock()
        .unwrap()
        .get(&result)
        .cloned()
        .context("this result is no longer kept: run the query again")
}

fn write_file(
    path: &str,
    table: &termf_ops::sql::Table,
    format: Format,
    kind: termf_ops::Kind,
    name: &str,
) -> Result<()> {
    let file = std::fs::File::create(path).with_context(|| format!("cannot write {path}"))?;
    let mut out = std::io::BufWriter::new(file);
    termf_ops::export::write(table, format, kind, name, &mut out)?;
    std::io::Write::flush(&mut out)?;
    Ok(())
}

/// A query's table, masked as shown, written where the user chose: CSV unless `format` says otherwise; `name`: the
/// table an SQL export's `INSERT`s fill.
#[tauri::command]
pub async fn ops_sql_export(
    app: AppHandle,
    result: u64,
    path: String,
    format: Option<Format>,
    name: Option<String>,
) -> CmdResult<()> {
    blocking(app, move |_, state, _| {
        let (table, kind) = kept_table(state, result)?;
        let name = name.unwrap_or_else(|| "export".into());
        write_file(&path, &table, format.unwrap_or(Format::Csv), kind, &name)
    })
    .await
}

/// A query's table as text for the clipboard: TSV, CSV, JSON or Markdown.
#[tauri::command]
pub async fn ops_sql_copy(app: AppHandle, result: u64, format: Format) -> CmdResult<String> {
    blocking(app, move |_, state, _| {
        let (table, kind) = kept_table(state, result)?;
        termf_ops::export::text(&table, format, kind, "export")
    })
    .await
}

/// A chart of a result, drawn by the interface, written where the user chose: PNG or SVG only.
#[tauri::command]
pub async fn ops_chart_save(app: AppHandle, path: String, bytes: Vec<u8>) -> CmdResult<()> {
    blocking(app, move |_, _, _| {
        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);
        if !matches!(ext.as_deref(), Some("png" | "svg")) {
            bail!("a chart is saved as .png or .svg");
        }
        std::fs::write(&path, bytes).with_context(|| format!("cannot write {path}"))
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    rows: usize,
    /// It stopped at the export's limit: more rows were left.
    truncated: bool,
}

/// The query run again past the console's rows, up to `termf_ops::EXPORT_ROWS`, masked as the console masks it,
/// then written row by row.
#[tauri::command]
pub async fn ops_sql_export_all(
    app: AppHandle,
    project: String,
    connection: String,
    query: String,
    path: String,
    format: Format,
    name: Option<String>,
) -> CmdResult<Exported> {
    blocking(app, move |_, state, vault| {
        let (plan, output) = state.ops(vault).export(&project, &connection, &query)?;
        let table = match output.table {
            Some(table) if output.exit == Some(0) => table,
            _ => bail!("{}", output.stopped.unwrap_or(output.errors)),
        };
        let name = name.unwrap_or_else(|| "export".into());
        write_file(&path, &table, format, plan.kind, &name)?;
        Ok(Exported {
            rows: table.rows.len(),
            truncated: output.truncated,
        })
    })
    .await
}

/// A database URL as a connection's fields; its password comes back apart, for `ops_connection_save`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FromUrl {
    connection: Connection,
    password: Option<String>,
    ignored: Vec<String>,
}

#[tauri::command]
pub fn ops_parse_url(url: String) -> CmdResult<FromUrl> {
    let parsed = termf_ops::parse_url(&url).map_err(err)?;
    Ok(FromUrl {
        connection: parsed.connection,
        password: parsed.password,
        ignored: parsed.ignored,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    /// What `ops_connection_save` takes as `found`.
    id: usize,
    #[serde(flatten)]
    candidate: termf_ops::Candidate,
}

/// The databases the project's repositories declare, read from their files only; their passwords stay here.
#[tauri::command]
pub async fn ops_detect(app: AppHandle, project: String) -> CmdResult<Vec<Detected>> {
    blocking(app, move |_, state, _| {
        let project = termf_core::load_projects(&state.config_dir)?
            .into_iter()
            .find(|p| p.id == project)
            .context("no such project")?;
        let repos: Vec<(String, PathBuf)> = project
            .repos
            .iter()
            .map(|r| (r.name.clone(), PathBuf::from(&r.path)))
            .collect();
        Ok(hold_declared(state, &repos))
    })
    .await
}

/// [`ops_detect`] for repositories that are not a project yet (the new project's setup), by path.
#[tauri::command]
pub async fn ops_declared(app: AppHandle, repos: Vec<String>) -> CmdResult<Vec<Detected>> {
    blocking(app, move |_, state, _| {
        let repos: Vec<(String, PathBuf)> = repos
            .iter()
            .map(|p| {
                let path = PathBuf::from(p);
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                (name, path)
            })
            .collect();
        Ok(hold_declared(state, &repos))
    })
    .await
}

/// The databases the repositories declare; their passwords are kept here, by rank, for `found`.
fn hold_declared(state: &OpsState, repos: &[(String, PathBuf)]) -> Vec<Detected> {
    let mut candidates = termf_ops::declared(repos);
    let mut found = state.found.lock().unwrap();
    *found = candidates
        .iter_mut()
        .map(|c| c.password.take().map(Secret::new))
        .collect();
    candidates
        .into_iter()
        .enumerate()
        .map(|(id, candidate)| Detected { id, candidate })
        .collect()
}

/// The database's tables, columns, keys and indexes.
#[tauri::command]
pub async fn ops_schema(
    app: AppHandle,
    project: String,
    connection: String,
) -> CmdResult<Vec<termf_ops::sql::SchemaTable>> {
    blocking(app, move |_, state, vault| {
        state.ops(vault).schema(&project, &connection)
    })
    .await
}

#[tauri::command]
pub fn ops_query_save(
    state: State<OpsState>,
    project: String,
    query: termf_ops::Query,
    previous: Option<String>,
) -> CmdResult<Vec<termf_ops::Query>> {
    termf_ops::save_query(&state.config_dir, &project, &query, previous.as_deref()).map_err(err)
}

#[tauri::command]
pub fn ops_query_delete(
    state: State<OpsState>,
    project: String,
    name: String,
) -> CmdResult<Vec<termf_ops::Query>> {
    termf_ops::delete_query(&state.config_dir, &project, &name).map_err(err)
}

/// The project's recipe that runs this saved query only, for the agents: its name.
#[tauri::command]
pub fn ops_query_recipe(
    state: State<OpsState>,
    project: String,
    name: String,
) -> CmdResult<String> {
    termf_ops::query_recipe(&state.config_dir, &project, &name).map_err(err)
}

/// Asks the console's DBA (the `dba` role, read-only): a query from words, an explanation, or a faster query from
/// the database's plan. The first message of a conversation (no `session`) carries the schema; the session sees
/// rows only through `ops_run` and the user's approval, when the connection's `ai_access` allows it.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn ops_ask(
    app: AppHandle,
    project: String,
    connection: String,
    session: Option<i64>,
    kind: String,
    text: String,
    lang: String,
    images: Option<Vec<termf_agents::Image>>,
) -> CmdResult<AgentSession> {
    blocking(app, move |app, state, vault| {
        let ops = state.ops(vault);
        let conn = termf_ops::connections(&state.config_dir, &project)?
            .into_iter()
            .find(|c| c.name == connection)
            .with_context(|| format!("no connection {connection} in project {project}"))?;
        let plan;
        let ask = match kind.as_str() {
            "write" => termf_ops::dba::Ask::Write(&text),
            "explain" => termf_ops::dba::Ask::Explain(&text),
            "optimize" => {
                let explained = ops.explain(&project, &connection, &text)?;
                plan = match explained.table {
                    Some(t) if explained.exit == Some(0) => t
                        .rows
                        .iter()
                        .map(|r| r.join(" | "))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => bail!("the database gave no plan: {}", explained.errors),
                };
                termf_ops::dba::Ask::Optimize {
                    query: &text,
                    plan: &plan,
                }
            }
            other => bail!("unknown request {other}"),
        };
        let agents = crate::agents::agents(app);
        if let Some(id) = session {
            let images = images.unwrap_or_default();
            agents.send_with(id, &termf_ops::dba::next(&ask), &images)?;
            return agents
                .sessions()?
                .into_iter()
                .find(|s| s.id == id)
                .context("no such session");
        }
        let schema = ops.schema(&project, &connection)?;
        let repo = termf_core::load_projects(&state.config_dir)?
            .into_iter()
            .find(|p| p.id == project)
            .and_then(|p| p.repos.first().map(|r| PathBuf::from(&r.path)))
            .context("the project has no repository")?;
        let spec = |data: bool| {
            let mut allowed: Vec<String> = ["Read", "Grep", "Glob"].map(String::from).to_vec();
            if data {
                allowed.push(termf_agents::OPS.into());
            }
            termf_agents::SessionSpec {
                cwd: repo.clone(),
                title: format!("DBA · {connection}"),
                project: Some(project.clone()),
                role: Some("dba".into()),
                effort: Some("medium".into()),
                allowed_tools: allowed,
                disallowed_tools: [
                    "Bash",
                    "PowerShell",
                    "Edit",
                    "Write",
                    "NotebookEdit",
                    "WebFetch",
                    "WebSearch",
                ]
                .map(String::from)
                .to_vec(),
                guard: data,
                ..termf_agents::SessionSpec::default()
            }
        };
        // Rows only through ops_run, which a guarded session gets; without the termf CLI, the schema only.
        let data = conn.ai_access != termf_ops::AiAccess::None;
        let (created, data) = match agents.create(spec(data)) {
            Ok(s) => (s, data),
            Err(_) if data => (agents.create(spec(false))?, false),
            Err(e) => return Err(e),
        };
        agents.send(
            created.id,
            &termf_ops::dba::first(&conn, &schema, data, &ask, &lang),
        )?;
        agents
            .sessions()?
            .into_iter()
            .find(|s| s.id == created.id)
            .context("no such session")
    })
    .await
}

#[derive(Serialize, Clone)]
struct FollowEvent {
    id: u64,
    #[serde(flatten)]
    event: Streamed,
}

#[derive(Serialize)]
pub struct Followed {
    /// What `ops_follow_stop` takes, and the `id` of its `ops-follow` events.
    id: u64,
    plan: Plan,
}

/// Follows a recipe that runs until stopped: `ops-follow` events carry its masked lines, then its end. `id` is the
/// front end's, known before the first event can come; a stream that ends on its own is forgotten then.
#[tauri::command]
pub async fn ops_follow_start(
    app: AppHandle,
    id: u64,
    project: String,
    recipe: String,
    connection: String,
    params: BTreeMap<String, String>,
) -> CmdResult<Followed> {
    blocking(app, move |app, state, vault| {
        // Held until the stream is in: its end, however soon, then finds it to remove.
        let mut follows = state.follows.lock().unwrap();
        ensure!(
            !follows.contains_key(&id),
            "stream {id} is already followed"
        );
        let emitter = app.clone();
        let (plan, following) =
            state
                .ops(vault)
                .follow(&project, &recipe, &connection, &params, move |event| {
                    let end = matches!(event, Streamed::End { .. });
                    let _ = emitter.emit("ops-follow", FollowEvent { id, event });
                    if end {
                        ended(&emitter.state::<OpsState>(), id);
                    }
                })?;
        follows.insert(id, following);
        Ok(Followed { id, plan })
    })
    .await
}

/// A stream that ended on its own leaves: its client was waited for, and is never signalled after.
fn ended(state: &OpsState, id: u64) {
    let gone = state.follows.lock().unwrap().remove(&id);
    drop(gone);
}

#[tauri::command]
pub fn ops_follow_stop(state: State<OpsState>, id: u64) {
    state.follows.lock().unwrap().remove(&id);
}

#[tauri::command]
pub async fn ops_history(app: AppHandle, project: String) -> CmdResult<Vec<Entry>> {
    blocking(app, move |_, state, vault| {
        state.ops(vault).history(&project, 200)
    })
    .await
}

#[tauri::command]
pub fn ops_requests(state: State<OpsState>) -> Vec<Request> {
    state.approvals.list()
}

/// The user's answer to an agent's card. Approving a prod run asks for the master password within 5 minutes,
/// when there is a vault: `password` gives it here.
#[tauri::command]
pub async fn ops_answer(
    app: AppHandle,
    id: u64,
    allow: bool,
    password: Option<String>,
) -> CmdResult<()> {
    let password = password.map(Secret::new);
    blocking(app, move |app, state, vault| {
        let request = state
            .approvals
            .get(id)
            .context("this request is no longer waiting")?;
        let prod = request.plan.env == termf_ops::Env::Prod;
        // The vault masks the output with its values, so it must be open; prod needs the password within 5 min.
        if allow && request.kind == "approve" && vault.initialized() {
            let human = Actor::human();
            let status = vault.status()?;
            match &password {
                Some(p) if !status.unlocked => {
                    vault.unlock(&human, Some(p.expose()), None, None)?;
                }
                Some(p) if prod && status.prod_until.is_none() => {
                    vault.reauth(&human, p.expose())?;
                }
                _ => {}
            }
            let status = vault.status()?;
            if !status.unlocked {
                bail!(termf_vault::Locked);
            }
            if prod && status.prod_until.is_none() {
                bail!(NeedsPassword);
            }
        }
        state.approvals.answer(id, allow);
        if request.kind == "propose" && !allow {
            let _ = app.emit("ops-approval-done", &request);
        }
        Ok(())
    })
    .await
}

/// The ticket run an agent session works on, as a step of a team run.
pub(crate) fn run_of(app: &AppHandle, session: i64) -> Option<i64> {
    let runs = crate::team::team(app).runs().ok()?;
    runs.into_iter()
        .find(|r| r.steps.iter().any(|s| s.sessions.contains(&session)))
        .map(|r| r.target.run)
}

/// A screen's action of an agent that runs a recipe (`ops.run_query`): the agent's own path, as `ops_run` of
/// `termf mcp` (refused where agents may not look, Ops' card in prod or when the recipe asks, masking, the prod
/// mark, the audit in the agent's name), never the user's. A recipe that writes is only proposed.
#[tauri::command]
pub async fn ops_agent_run(
    app: AppHandle,
    session: i64,
    project: String,
    recipe: String,
    connection: String,
    params: BTreeMap<String, String>,
) -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let session = crate::agents::agents(&app).session(session)?;
        let params = params
            .into_iter()
            .map(|(k, v)| (k, Value::String(v)))
            .collect();
        let call = McpCall::OpsRun {
            recipe,
            connection: Some(connection),
            params,
        };
        mcp(&app, &session, &project, call, &|| false)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}

/// `ops_recipes` and `ops_run` of `termf mcp`, for `session` in `project`; `gone`: its caller went away.
pub fn mcp(
    app: &AppHandle,
    session: &AgentSession,
    project: &str,
    tool: McpCall,
    gone: &dyn Fn() -> bool,
) -> Result<Value> {
    let state = app.state::<OpsState>();
    let vault = crate::vault::shared(app);
    let ops = state.ops(&vault);
    match tool {
        McpCall::OpsRecipes {} => Ok(serde_json::to_value(ops.seen(project)?)?),
        McpCall::OpsRun {
            recipe,
            connection,
            params,
        } => {
            let params = params
                .into_iter()
                .map(|(k, v)| match v {
                    Value::String(s) => Ok((k, s)),
                    Value::Number(n) => Ok((k, n.to_string())),
                    _ => bail!("{k}: a text or a number"),
                })
                .collect::<Result<_>>()?;
            let agent = Agent {
                session: session.id,
                claude: session.session_id.clone(),
                run: run_of(app, session.id),
                title: session.spec.title.clone(),
                role: session.spec.role.clone(),
            };
            let show = |event: &str, request: &Request| {
                let _ = app.emit(event, request);
            };
            ops.agent_run(
                project,
                &agent,
                &recipe,
                connection.as_deref(),
                &params,
                &state.approvals,
                APPROVAL_WAIT,
                &show,
                gone,
            )
        }
        _ => bail!("not an Ops tool"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use termf_ops::sql::Table;

    #[test]
    fn a_refused_query_reaches_the_front_end_as_its_code() {
        let refused = termf_ops::sql::analyze("DELETE FROM users", termf_ops::Kind::Postgres, 500)
            .unwrap_err();
        let sent: Value = serde_json::from_str(&failed(anyhow::Error::new(refused))).unwrap();
        assert_eq!(sent["refused"]["code"], "notRead");
        assert_eq!(sent["refused"]["subject"], "DELETE");
        assert!(sent["message"].as_str().unwrap().contains("is not a read"));
        let other = failed(anyhow::anyhow!("the vault is locked"));
        assert_eq!(
            other, "the vault is locked",
            "other errors stay as they were"
        );
    }

    #[test]
    fn query_tables_stay_for_their_export_the_last_ones_only() {
        let mut tables = BTreeMap::new();
        for id in 1..=(TABLES as u64 + 5) {
            let table = Table {
                columns: vec!["id".into(), "note".into()],
                rows: vec![vec![id.to_string(), "a, \"b\"\nc".into()]],
            };
            keep_table(&mut tables, id, (table, termf_ops::Kind::Mysql));
        }
        assert_eq!(tables.len(), TABLES);
        assert!(!tables.contains_key(&5) && tables.contains_key(&6));
        let (table, kind) = &tables[&6];
        assert_eq!(
            termf_ops::export::text(table, Format::Csv, *kind, "t").unwrap(),
            "id,note\n6,\"a, \"\"b\"\"\nc\"\n",
            "quoted as a spreadsheet reads it"
        );
        // Its database stays with it, whatever became of the result's revealable values.
        let sql = termf_ops::export::text(table, Format::Sql, *kind, "t").unwrap();
        assert!(sql.starts_with("INSERT INTO `t` (`id`, `note`)"), "{sql}");
    }

    fn db(name: &str, secret: &str, env: &str) -> Connection {
        serde_json::from_value(serde_json::json!({
            "name": name, "kind": "postgres", "env": env, "host": "h", "user": "u", "database": "d",
            "secrets": { "password": secret },
        }))
        .unwrap()
    }

    #[test]
    fn a_password_never_replaces_another_connections_secret_unasked() {
        let all = vec![db("shop-db", "SHOP_DB_PASSWORD", "prod")];
        // shop_db names the secret shop-db's password lives in.
        let new = db("shop_db", "SHOP_DB_PASSWORD", "prod");
        let why = clash(&all, &new, None, true).unwrap();
        assert!(
            why.contains("already holds the password of shop-db"),
            "{why}"
        );
        // In another environment, it is another secret.
        assert_eq!(
            clash(&all, &db("shop_db", "SHOP_DB_PASSWORD", "dev"), None, false),
            None
        );
        // A secret already in the vault, but no connection's: asked too.
        let why = clash(&all, &db("other", "OTHER_PASSWORD", "prod"), None, true).unwrap();
        assert!(
            why.contains("the vault already has the secret OTHER_PASSWORD"),
            "{why}"
        );
        assert_eq!(
            clash(&all, &db("other", "OTHER_PASSWORD", "prod"), None, false),
            None
        );
        // Its own secret, changed or renamed: no question.
        let same = db("shop-db", "SHOP_DB_PASSWORD", "prod");
        assert_eq!(clash(&all, &same, Some("shop-db"), true), None);
        let renamed = db("shop", "SHOP_DB_PASSWORD", "prod");
        assert_eq!(clash(&all, &renamed, Some("shop-db"), true), None);
    }

    #[test]
    fn a_new_databases_test_uses_its_password_before_the_vault_has_it() {
        let c = db("shop", "SHOP_PASSWORD", "prod");
        let waiting = Secret::new("s3cret".into());
        assert_eq!(
            waiting_for(&c, "SHOP_PASSWORD", Some(&waiting)).map(|s| s.expose().to_owned()),
            Some("s3cret".into())
        );
        assert!(waiting_for(&c, "OTHER", Some(&waiting)).is_none());
        assert!(waiting_for(&c, "SHOP_PASSWORD", None).is_none());
    }
}
