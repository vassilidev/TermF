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
    config_dir: PathBuf,
    pub(crate) data_dir: PathBuf,
    approvals: Arc<Approvals>,
    runner: Runner,
    next: AtomicU64,
    /// A result's id → what `ops_reveal` and the `.env` commands take from it.
    kept: Mutex<BTreeMap<u64, Kept>>,
    /// A query result's id → its masked table, for `ops_sql_export`: the last [`TABLES`].
    tables: Mutex<BTreeMap<u64, termf_ops::sql::Table>>,
    /// The streams followed, by the id the front end chose: dropping one stops its client.
    follows: Mutex<BTreeMap<u64, Following>>,
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
    }
}

/// Keeps a table for its export, the oldest dropped past [`TABLES`].
fn keep_table(
    tables: &mut BTreeMap<u64, termf_ops::sql::Table>,
    id: u64,
    table: termf_ops::sql::Table,
) {
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

async fn blocking<T: Send + 'static>(
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
}

#[tauri::command]
pub fn ops_overview(state: State<OpsState>, project: String) -> CmdResult<Overview> {
    let connections = termf_ops::connections(&state.config_dir, &project).map_err(err)?;
    let (recipes, errors) = termf_ops::recipes(&state.config_dir, Some(&project));
    Ok(Overview {
        connections,
        recipes,
        errors,
    })
}

#[tauri::command]
pub fn ops_connection_save(
    state: State<OpsState>,
    project: String,
    connection: Connection,
    previous: Option<String>,
) -> CmdResult<Vec<Connection>> {
    termf_ops::save_connection(
        &state.config_dir,
        &project,
        &connection,
        previous.as_deref(),
    )
    .map_err(err)
}

#[tauri::command]
pub fn ops_connection_delete(
    state: State<OpsState>,
    project: String,
    name: String,
) -> CmdResult<Vec<Connection>> {
    termf_ops::delete_connection(&state.config_dir, &project, &name).map_err(err)
}

/// Tests a connection as the form holds it, saved or not.
#[tauri::command]
pub async fn ops_test(
    app: AppHandle,
    project: String,
    connection: Connection,
    previous: Option<String>,
) -> CmdResult<Vec<Check>> {
    blocking(app, move |_, state, vault| {
        let mut all = termf_ops::connections(&state.config_dir, &project)?;
        all.retain(|c| Some(&c.name) != previous.as_ref() && c.name != connection.name);
        all.push(connection.clone());
        let human = Actor::human();
        let secret = |name: &str| -> Result<Secret> {
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
            keep_table(&mut state.tables.lock().unwrap(), id, table.clone());
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

/// A query's table, masked as shown, written as CSV where the user chose.
#[tauri::command]
pub async fn ops_sql_export(app: AppHandle, result: u64, path: String) -> CmdResult<()> {
    blocking(app, move |_, state, _| {
        let table = state
            .tables
            .lock()
            .unwrap()
            .get(&result)
            .cloned()
            .context("this result is no longer kept: run the query again")?;
        std::fs::write(&path, csv(&table)).with_context(|| format!("cannot write {path}"))
    })
    .await
}

/// A table as CSV: a cell in double quotes when it holds a comma, a quote or a line break.
fn csv(table: &termf_ops::sql::Table) -> String {
    let cell = |c: &String| {
        if c.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", c.replace('"', "\"\""))
        } else {
            c.clone()
        }
    };
    std::iter::once(&table.columns)
        .chain(&table.rows)
        .map(|r| r.iter().map(cell).collect::<Vec<_>>().join(",") + "\n")
        .collect()
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
fn run_of(app: &AppHandle, session: i64) -> Option<i64> {
    let runs = crate::team::team(app).runs().ok()?;
    runs.into_iter()
        .find(|r| r.steps.iter().any(|s| s.sessions.contains(&session)))
        .map(|r| r.target.run)
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
            keep_table(&mut tables, id, table);
        }
        assert_eq!(tables.len(), TABLES);
        assert!(!tables.contains_key(&5) && tables.contains_key(&6));
        assert_eq!(
            csv(&tables[&6]),
            "id,note\n6,\"a, \"\"b\"\"\nc\"\n",
            "quoted as a spreadsheet reads it"
        );
    }
}
