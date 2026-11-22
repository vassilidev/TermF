//! The project's CTO conversation (ARCHITECTURE §7.1–§7.2): one headless session per project playing `cto`, with
//! TermF's tools (`termf mcp`) and the read tools of the project's repositories. What the project's team runs
//! wait for reaches it as TermF's notices; `@role` hands a message to another agent instead.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Manager};
use termf_agents::{AgentSession, Agents, Notice, SessionSpec, State};
use termf_claude::mask::{mask, mask_text};
use termf_core::Project;
use termf_core::ipc::{McpCall, Status};
use termf_library::{Kind, Scope};
use termf_team::{Options, RunState, StepState, TeamEvent, TeamRun};

use crate::agents::AgentsState;
use crate::tickets::{blocking, tickets};
use crate::{AppState, CmdResult, team};

/// Held while a project's conversation is looked for and created, so that it has only one.
static OPENING: Mutex<()> = Mutex::new(());

fn agents(app: &AppHandle) -> Agents {
    app.state::<AgentsState>().agents()
}

fn project(app: &AppHandle, id: &str) -> Result<Project> {
    let config_dir = &app.state::<AppState>().config_dir;
    termf_core::load_projects(config_dir)?
        .into_iter()
        .find(|p| p.id == id)
        .with_context(|| format!("no project {id}"))
}

/// The project's conversation; the newest, should there be several.
fn conversation(agents: &Agents, project: &str) -> Result<Option<AgentSession>> {
    let sessions = agents.sessions()?.into_iter();
    Ok(sessions
        .into_iter()
        .find(|s| s.spec.mcp && s.spec.project.as_deref() == Some(project)))
}

/// The project's CTO conversation, created the first time with no process: the first message starts one, and
/// each later one resumes it.
pub fn open(app: &AppHandle, id: &str) -> Result<AgentSession> {
    let _one = OPENING.lock().unwrap();
    let agents = agents(app);
    if let Some(session) = conversation(&agents, id)? {
        return Ok(session);
    }
    let project = project(app, id)?;
    let repo = project
        .repos
        .first()
        .with_context(|| format!("the project {} has no repository", project.name))?;
    agents.create(SessionSpec {
        cwd: repo.path.clone().into(),
        title: format!("CTO · {}", project.name),
        project: Some(project.id),
        role: Some("cto".into()),
        mcp: true,
        ..SessionSpec::default()
    })
}

/// Where a message to a role went.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Addressed {
    pub session: AgentSession,
    /// `step`: the session of the role's step at work in a team run; `direct`: a conversation with the role.
    pub via: &'static str,
    pub run: Option<i64>,
    pub step: Option<String>,
}

/// The repository named `repo` (its name, else its path), by default the project's first.
fn repo_path(project: &Project, repo: Option<&str>) -> Result<PathBuf> {
    let found = match repo.map(str::trim).filter(|r| !r.is_empty()) {
        Some(name) => project
            .repos
            .iter()
            .find(|r| r.name == name || r.path == name),
        None => project.repos.first(),
    };
    found.map(|r| PathBuf::from(&r.path)).with_context(|| {
        format!(
            "no repository {} in {}",
            repo.unwrap_or_default(),
            project.name
        )
    })
}

/// Sends `text` to the session of `role`'s step at work in one of the project's team runs, else to the direct
/// conversation with `role` in `repo`, which starts the first time.
pub fn address(
    app: &AppHandle,
    id: &str,
    role: &str,
    text: &str,
    repo: Option<&str>,
) -> Result<Addressed> {
    ensure!(!text.trim().is_empty(), "the message is empty");
    let agents = agents(app);
    let project = project(app, id)?;
    if let Some((run, step, session)) = step_at_work(&team::team(app).runs()?, id, role) {
        agents.send(session, text)?;
        return Ok(Addressed {
            session: agents.session(session)?,
            via: "step",
            run: Some(run),
            step: Some(step),
        });
    }
    let cwd = repo_path(&project, repo)?;
    let session = match direct_conversation(agents.sessions()?, id, role, &cwd) {
        Some(session) => {
            agents.send(session.id, text)?;
            agents.session(session.id)?
        }
        None => {
            let name = cwd.file_name().unwrap_or(cwd.as_os_str());
            let title = format!("{role} · {}", name.to_string_lossy());
            let spec = SessionSpec {
                cwd,
                title,
                project: Some(project.id.clone()),
                role: Some(role.into()),
                ..SessionSpec::default()
            };
            agents.start(spec, Some(text))?
        }
    };
    Ok(Addressed {
        session,
        via: "direct",
        run: None,
        step: None,
    })
}

/// The step of `role` at work in one of the project's team runs that go on: its run, key and agent session.
fn step_at_work(runs: &[TeamRun], project: &str, role: &str) -> Option<(i64, String, i64)> {
    runs.iter()
        .filter(|r| r.target.project == project)
        .filter(|r| matches!(r.state, RunState::Running | RunState::Waiting))
        .find_map(|r| {
            let step = r
                .steps
                .iter()
                .find(|s| s.state == StepState::Running && s.role.as_deref() == Some(role))?;
            Some((r.run, step.key.clone(), step.session?))
        })
}

/// The newest direct conversation with `role` in `cwd`: neither a CTO conversation nor a team step's session,
/// which answers in a schema.
fn direct_conversation(
    sessions: Vec<AgentSession>,
    project: &str,
    role: &str,
    cwd: &Path,
) -> Option<AgentSession> {
    sessions.into_iter().find(|s| {
        s.spec.project.as_deref() == Some(project)
            && s.spec.role.as_deref() == Some(role)
            && !s.spec.mcp
            && s.spec.json_schema.is_none()
            && s.spec.cwd == cwd
    })
}

/// Tells the project's CTO conversation, with the user's next message, what the user wrote to another agent.
fn tell(app: &AppHandle, project: &str, role: &str, text: &str, to: &Addressed) -> Result<()> {
    let agents = agents(app);
    let Some(cto) = conversation(&agents, project)? else {
        return Ok(());
    };
    let said = match (to.run, to.step.as_deref()) {
        (Some(run), Some(step)) => format!(
            "The user wrote to {role}, at its step {step} of team run {run} (conversation {}): {text}",
            to.session.id
        ),
        _ => format!(
            "The user wrote to {role} directly, in conversation {} ({}): {text}",
            to.session.id,
            to.session.spec.cwd.display()
        ),
    };
    let notice = Notice {
        about: to.via.into(),
        run: to.run,
        step: to.step.clone(),
        role: Some(role.into()),
        conversation: Some(to.session.id),
        detail: Some(text.into()),
        text: said,
    };
    agents.notify(cto.id, notice, false)
}

/// Tells the project's CTO conversation what a team run waits for (a gate, or a step blocked by a failure or a
/// question), so that it relays it at once. Off the team's thread, which may be in the middle of a change.
pub fn team_event(app: &AppHandle, event: &TeamEvent) {
    let about = match event.kind.as_str() {
        "step_waiting" => "waiting",
        "step_blocked" => "blocked",
        _ => return,
    };
    let (app, event) = (app.clone(), event.clone());
    std::thread::spawn(move || {
        if let Err(e) = relay(&app, &event, about) {
            eprintln!(
                "termf: the CTO conversation was not told of run {}: {e:#}",
                event.run
            );
        }
    });
}

fn relay(app: &AppHandle, event: &TeamEvent, about: &str) -> Result<()> {
    let run = team::team(app).run(event.run)?;
    let agents = agents(app);
    let Some(cto) = conversation(&agents, &run.target.project)? else {
        return Ok(());
    };
    let step = run
        .steps
        .iter()
        .find(|s| Some(&s.key) == event.step.as_ref());
    // A command's output may hold a secret: none goes to an agent.
    let detail = step
        .and_then(|s| s.note.clone())
        .unwrap_or_else(|| event.message.clone());
    let detail = mask_text(&detail);
    let key = event.step.clone().unwrap_or_default();
    let what = match about {
        "waiting" => format!("the gate {key} waits for the user's approval"),
        _ => format!("the step {key} is blocked"),
    };
    let text = format!(
        "Team run {} ({} · {}): {what}. {detail}",
        run.run, run.target.ticket, run.target.title
    );
    let notice = Notice {
        about: about.into(),
        run: Some(run.run),
        step: event.step.clone(),
        role: step.and_then(|s| s.role.clone()),
        conversation: step.and_then(|s| s.session),
        detail: Some(detail),
        text: mask_text(&text),
    };
    agents.notify(cto.id, notice, true)
}

/// A tool of `termf mcp`, for agent session `session` and within its project only. Nothing that looks like a
/// secret goes back to the agent.
pub fn call(app: &AppHandle, session: i64, tool: McpCall) -> Result<Value> {
    let agents = agents(app);
    let conversation = agents.session(session)?;
    ensure!(
        conversation.spec.mcp,
        "agent session {session} has no TermF tools"
    );
    let project = conversation
        .spec
        .project
        .context("this conversation has no project")?;
    let mut value = run_tool(app, &agents, &project, tool)?;
    mask(&mut value);
    Ok(value)
}

fn run_tool(app: &AppHandle, agents: &Agents, id: &str, tool: McpCall) -> Result<Value> {
    let team = team::team(app);
    let owned = |run: i64| -> Result<TeamRun> {
        let found = team.run(run)?;
        ensure!(
            found.target.project == id,
            "team run {run} is not in this project"
        );
        Ok(found)
    };
    let ticket_run = |run: i64| -> Result<()> {
        let found = tickets(app).run(run)?;
        ensure!(found.project == id, "run {run} is not in this project");
        Ok(())
    };
    Ok(match tool {
        McpCall::Project {} => described(app, id)?,
        McpCall::TeamRuns {} => {
            let runs = team.runs()?;
            json!(
                runs.iter()
                    .filter(|r| r.target.project == id)
                    .map(summary)
                    .collect::<Vec<_>>()
            )
        }
        McpCall::TeamRun { run } => {
            let found = owned(run)?;
            let mut value = serde_json::to_value(&found)?;
            value["waits"] = json!(waits(&found));
            value
        }
        McpCall::TicketRuns {} => {
            let runs = tickets(app).runs()?;
            serde_json::to_value(
                runs.into_iter()
                    .filter(|r| r.project == id)
                    .collect::<Vec<_>>(),
            )?
        }
        McpCall::Inbox {} => serde_json::to_value(tickets(app).inbox(Some(id), false)?)?,
        McpCall::Waiting {} => waiting(app, agents, id)?,
        McpCall::StartTeamRun {
            run,
            ticket,
            request,
            repo,
            workflow,
            budget,
        } => {
            let options = Options {
                workflow,
                budget,
                ..Options::default()
            };
            let started = match (run, ticket, request) {
                (Some(run), None, None) => {
                    ticket_run(run)?;
                    team::start(app, run, options)?
                }
                (None, Some(ticket), None) => {
                    from_ticket(app, id, &ticket, repo.as_deref(), options)?
                }
                (None, None, Some(_)) => bail!(
                    "a request without a ticket cannot start a team run yet: give a ticket, or a ticket run"
                ),
                _ => bail!("give one of run, ticket or request"),
            };
            summary(&started)
        }
        McpCall::Answer { run, step, answers } => {
            owned(run)?;
            summary(&team.revise(run, &step, &answers)?)
        }
        McpCall::Approve { run, step } => {
            owned(run)?;
            summary(&team.approve(run, &step)?)
        }
        McpCall::RequestChanges {
            run,
            step,
            feedback,
        } => {
            owned(run)?;
            summary(&team.revise(run, &step, &feedback)?)
        }
        McpCall::StopTeamRun { run } => {
            owned(run)?;
            json!(
                team.stop(Some(run))?
                    .iter()
                    .map(summary)
                    .collect::<Vec<_>>()
            )
        }
        McpCall::SendMessage {
            run,
            step,
            role,
            repo,
            text,
        } => match (run, step, role) {
            (Some(run), Some(key), None) => {
                let found = owned(run)?;
                let step = found
                    .steps
                    .iter()
                    .find(|s| s.key == key)
                    .with_context(|| format!("team run {run} has no step {key}"))?;
                let session = step.session.context("this step has no agent session")?;
                agents.send(session, &text)?;
                json!({ "via": "step", "run": run, "step": key, "conversation": session })
            }
            (None, None, Some(role)) => {
                let to = address(app, id, &role, &text, repo.as_deref())?;
                json!({ "via": to.via, "run": to.run, "step": to.step, "conversation": to.session.id })
            }
            _ => bail!("give run and step, or role"),
        },
    })
}

/// A ticket run made for the team, with no Claude Code session of its own; closed again when its team does not
/// start.
fn from_ticket(
    app: &AppHandle,
    id: &str,
    ticket: &str,
    repo: Option<&str>,
    options: Options,
) -> Result<TeamRun> {
    crate::api::run_guard(&app.state::<AppState>())?;
    let run = tickets(app).start_without_session(ticket, Some(id), repo)?;
    crate::tickets::updated(app, &run);
    team::start(app, run.id, options).inspect_err(|_| {
        if let Ok(closed) = crate::tickets::close(app, run.id, false) {
            crate::tickets::updated(app, &closed);
        }
    })
}

fn described(app: &AppHandle, id: &str) -> Result<Value> {
    let project = project(app, id)?;
    let config_dir = &app.state::<AppState>().config_dir;
    let scope = Scope::new(config_dir, None, Some(id.to_owned()))?;
    let roles: Vec<Value> = termf_library::resolve(config_dir, &scope)?
        .into_iter()
        .filter(|i| i.kind == Kind::Role && i.active)
        .map(|i| json!({ "role": i.name, "description": i.description }))
        .collect();
    let repos: Vec<Value> = project
        .repos
        .iter()
        .map(|r| json!({ "name": r.name, "path": r.path }))
        .collect();
    Ok(json!({ "id": project.id, "name": project.name, "repos": repos, "roles": roles }))
}

fn summary(run: &TeamRun) -> Value {
    let at_work: Vec<&str> = run
        .steps
        .iter()
        .filter(|s| s.state == StepState::Running)
        .map(|s| s.key.as_str())
        .collect();
    json!({
        "run": run.run,
        "ticket": run.target.ticket,
        "title": run.target.title,
        "repo": run.target.repo,
        "workflow": run.workflow,
        "state": run.state,
        "note": run.note,
        "cost": run.cost,
        "budget": run.budget,
        "atWork": at_work,
        "waits": waits(run),
    })
}

/// The gates and blocked steps that wait for the user, with their questions.
fn waits(run: &TeamRun) -> Vec<Value> {
    let plan = run
        .plan
        .as_ref()
        .map(|p| p.questions.clone())
        .unwrap_or_default();
    run.steps
        .iter()
        .filter(|s| matches!(s.state, StepState::Waiting | StepState::Blocked))
        .map(|s| {
            let mut questions = s.report.as_ref().map(|r| r.questions.clone()).unwrap_or_default();
            if s.plan && questions.is_empty() {
                questions = plan.clone();
            }
            json!({ "step": s.key, "state": s.state, "role": s.role, "task": s.task, "note": s.note, "questions": questions })
        })
        .collect()
}

fn waiting(app: &AppHandle, agents: &Agents, id: &str) -> Result<Value> {
    let project = project(app, id)?;
    let inside = |cwd: &Path| project.repos.iter().any(|r| cwd.starts_with(&r.path));
    let state = app.state::<AppState>();
    let terminals: Vec<Value> = state
        .api
        .list(&state.sessions)
        .into_iter()
        .filter(|s| s.status == Some(Status::Waiting) && inside(Path::new(&s.cwd)))
        .map(|s| json!({ "terminal": s.id, "title": s.title }))
        .collect();
    let conversations: Vec<Value> = agents
        .sessions()?
        .into_iter()
        .filter(|s| s.state == State::Waiting)
        .filter(|s| s.spec.project.as_deref() == Some(id) || inside(&s.spec.cwd))
        .map(|s| json!({ "conversation": s.id, "title": s.spec.title, "role": s.spec.role }))
        .collect();
    let runs = team::team(app).runs()?;
    let team_runs: Vec<Value> = runs
        .iter()
        .filter(|r| r.target.project == id)
        .map(|r| (r, waits(r)))
        .filter(|(_, w)| !w.is_empty())
        .map(|(r, w)| json!({ "run": r.run, "ticket": r.target.ticket, "title": r.target.title, "waits": w }))
        .collect();
    Ok(json!({ "terminals": terminals, "conversations": conversations, "teamRuns": team_runs }))
}

#[tauri::command]
pub async fn cto_open(app: AppHandle, project: String) -> CmdResult<AgentSession> {
    blocking(app, move |app| open(app, &project)).await
}

/// `@role` in the CTO conversation: the message goes to the role, and the CTO hears of it.
#[tauri::command]
pub async fn cto_address(
    app: AppHandle,
    project: String,
    role: String,
    text: String,
    repo: Option<String>,
) -> CmdResult<Addressed> {
    blocking(app, move |app| {
        let to = address(app, &project, &role, &text, repo.as_deref())?;
        if let Err(e) = tell(app, &project, &role, &text, &to) {
            eprintln!("termf: the CTO conversation was not told: {e:#}");
        }
        Ok(to)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: i64, project: &str, state: &str, steps: Value) -> TeamRun {
        serde_json::from_value(json!({
            "run": id,
            "target": {"run": id, "project": project, "repo": "api", "repoPath": "/src/api", "worktree": "/src/api/wt",
                       "ticket": "#4", "title": "Login", "brief": ""},
            "workflow": "default", "state": state, "planOnly": false, "cost": 0.0, "maxParallel": 4,
            "workflowRules": {}, "rules": {}, "taskRules": {}, "uiChanged": false, "steps": steps,
            "created": 0, "updated": 0,
        }))
        .unwrap()
    }

    fn step(key: &str, role: &str, state: &str, session: Option<i64>) -> Value {
        json!({"key": key, "kind": "task", "role": role, "needs": [], "state": state, "plan": false,
               "session": session, "sessions": [], "after": 0, "attempt": 1, "integrated": false,
               "files": [], "violations": [], "cost": 0.0})
    }

    fn session(id: i64, role: &str, cwd: &str, extra: Value) -> AgentSession {
        let mut v = json!({"id": id, "runtime": "claude-code", "sessionId": "s", "cwd": cwd, "title": "t",
                           "project": "acme", "role": role, "state": "idle", "cost": 0.0,
                           "context": null, "tile": null, "created": 0, "updated": 0});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn a_role_at_work_in_a_team_run_gets_the_message() {
        let runs = [
            run(
                1,
                "acme",
                "done",
                json!([step("implement:t1", "back", "running", Some(5))]),
            ),
            run(
                2,
                "other",
                "running",
                json!([step("implement:t1", "back", "running", Some(6))]),
            ),
            run(
                3,
                "acme",
                "waiting",
                json!([
                    step("implement:t1", "back", "done", Some(7)),
                    step("implement:t2", "back", "running", Some(8)),
                    step("implement:t3", "front", "running", Some(9)),
                ]),
            ),
        ];
        assert_eq!(
            step_at_work(&runs, "acme", "back"),
            Some((3, "implement:t2".into(), 8)),
            "only a step at work, in a run of the project that goes on"
        );
        assert_eq!(step_at_work(&runs, "acme", "qa"), None);
        let without = [run(
            4,
            "acme",
            "running",
            json!([step("implement:t1", "qa", "running", None)]),
        )];
        assert_eq!(step_at_work(&without, "acme", "qa"), None, "no session yet");
    }

    #[test]
    fn a_direct_conversation_is_neither_the_cto_s_nor_a_team_step() {
        let cwd = Path::new("/src/api");
        let sessions = vec![
            session(
                9,
                "back",
                "/src/api",
                json!({"jsonSchema": {"type": "object"}}),
            ),
            session(8, "back", "/src/web", json!({})),
            session(7, "cto", "/src/api", json!({"mcp": true})),
            session(6, "back", "/src/api", json!({"project": "other"})),
            session(5, "back", "/src/api", json!({})),
            session(4, "back", "/src/api", json!({})),
        ];
        let found = direct_conversation(sessions.clone(), "acme", "back", cwd);
        assert_eq!(
            found.map(|s| s.id),
            Some(5),
            "the newest in that repository"
        );
        assert!(direct_conversation(sessions, "acme", "cto", cwd).is_none());
    }
}
