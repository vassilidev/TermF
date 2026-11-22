//! Ops for the UI and for the agents (`termf-ops`, ARCHITECTURE §12.3–§12.5): a project's connections and
//! recipes, runs by the user, and the agents' `ops_run`, which waits for the user's answer to its card
//! (`ops-approval`, then `ops-approval-done`). A masked value of the vault reaches the webview only through
//! `ops_reveal`, once the master password was typed within 5 minutes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_agents::AgentSession;
use termf_core::ipc::McpCall;
use termf_ops::{
    APPROVAL_WAIT, Agent, Approvals, Check, Connection, Ops, Output, Plan, Recipe, Request, Runner,
};
use termf_vault::{Action, Actor, Entry, Event, NeedsPassword, Secret, Vault};

use crate::{CmdResult, err};

/// How long a result's masked values of the vault stay revealable.
const REVEALABLE: Duration = Duration::from_secs(600);

pub struct OpsState {
    config_dir: PathBuf,
    pub(crate) data_dir: PathBuf,
    approvals: Arc<Approvals>,
    runner: Runner,
    next: AtomicU64,
    /// A result's id → the values of the vault masked in it, for `ops_reveal`.
    hidden: Mutex<BTreeMap<u64, (Instant, Vec<Secret>)>>,
}

pub fn state(config_dir: PathBuf, data_dir: PathBuf) -> OpsState {
    OpsState {
        config_dir,
        data_dir,
        approvals: Arc::default(),
        runner: Runner::default(),
        next: AtomicU64::new(1),
        hidden: Mutex::default(),
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
    .map_err(err)
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
        let mut hidden = state.hidden.lock().unwrap();
        hidden.retain(|_, (at, _)| at.elapsed() < REVEALABLE);
        hidden.insert(id, (Instant::now(), std::mem::take(&mut output.hidden)));
        Ok(Ran { id, plan, output })
    })
    .await
}

/// A value of the vault masked in a result: only with the master password typed within 5 minutes, audited.
#[tauri::command]
pub async fn ops_reveal(app: AppHandle, result: u64, index: usize) -> CmdResult<String> {
    blocking(app, move |_, state, vault| {
        if vault.status()?.prod_until.is_none() {
            bail!(NeedsPassword);
        }
        let hidden = state.hidden.lock().unwrap();
        let value = hidden
            .get(&result)
            .filter(|(at, _)| at.elapsed() < REVEALABLE)
            .and_then(|(_, values)| values.get(index))
            .context("this result is too old to reveal its values: run it again")?;
        vault.record(Event {
            size: Some(value.len() as u64),
            detail: Some("a value of the vault masked in a recipe's output".into()),
            ..Event::new(&Actor::human(), Action::Reveal)
        })?;
        Ok(value.expose().to_owned())
    })
    .await
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
        if allow && request.kind == "approve" && prod && vault.initialized() {
            if let Some(password) = &password {
                vault.reauth(&Actor::human(), password.expose())?;
            }
            if vault.status()?.prod_until.is_none() {
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
