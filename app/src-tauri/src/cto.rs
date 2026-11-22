//! The project's CTO conversation (ARCHITECTURE §7.1–§7.2): one headless session per project playing `cto`, with
//! TermF's tools (`termf mcp`) and the read tools of the project's repositories. What the project's team runs
//! wait for reaches it as TermF's notices; `@role` hands a message to another agent instead.

use std::collections::BTreeMap;
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
use termf_library::{Kind, Layer, Scope};
use termf_team::{Options, Request, RunState, StepState, TeamEvent, TeamRun};

use crate::agents::AgentsState;
use crate::tickets::{blocking, tickets};
use crate::{AppState, CmdResult, memory, team};

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

/// The project's standing conversation with `role` (the CTO's, the expert's); the newest, should there be several.
/// It has no budget: the user talks with it as long as they like, while each answer of the expert to another agent
/// (`ask_expert`) gets its own session and budget.
fn conversation(agents: &Agents, project: &str, role: &str) -> Result<Option<AgentSession>> {
    let sessions = agents.sessions()?.into_iter();
    Ok(sessions.into_iter().find(|s| {
        s.spec.mcp
            && s.spec.budget.is_none()
            && s.spec.role.as_deref() == Some(role)
            && s.spec.project.as_deref() == Some(project)
    }))
}

/// The project's standing conversation with `role`, created the first time with no process: the first message
/// starts one, and each later one resumes it.
pub fn standing(app: &AppHandle, id: &str, role: &str) -> Result<AgentSession> {
    let _one = OPENING.lock().unwrap();
    let agents = agents(app);
    if let Some(session) = conversation(&agents, id, role)? {
        return Ok(session);
    }
    let project = project(app, id)?;
    let repo = project
        .repos
        .first()
        .with_context(|| format!("the project {} has no repository", project.name))?;
    let title = match role {
        termf_agents::EXPERT => "Expert",
        _ => "CTO",
    };
    agents.create(SessionSpec {
        cwd: repo.path.clone().into(),
        title: format!("{title} · {}", project.name),
        project: Some(project.id),
        role: Some(role.into()),
        mcp: true,
        ..SessionSpec::default()
    })
}

/// Where a message to a role went.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Addressed {
    pub session: AgentSession,
    /// `step`: the session of the role's step at work in a team run; `direct`: a conversation with the role;
    /// `queued`: the expert's conversation, which takes it at the end of its turn under way.
    pub via: &'static str,
    /// The step: the one that got the message, or, with `direct`, the one in the middle of a turn that could not.
    pub run: Option<i64>,
    pub step: Option<String>,
}

/// Where `@role` goes: the session of the role's step at work when it takes the message now, between two turns;
/// else the direct conversation, the busy step kept to say so. A message is never lost.
#[derive(Debug, PartialEq)]
enum Route {
    Step {
        run: i64,
        step: String,
        session: i64,
    },
    Direct {
        busy: Option<(i64, String)>,
    },
}

fn route(at_work: Option<(i64, String, i64)>, send: impl FnOnce(i64) -> bool) -> Route {
    match at_work {
        Some((run, step, session)) if send(session) => Route::Step { run, step, session },
        Some((run, step, _)) => Route::Direct {
            busy: Some((run, step)),
        },
        None => Route::Direct { busy: None },
    }
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

/// Sends `text` to the session of `role`'s step at work in one of the project's team runs when it is between two
/// turns, else to the direct conversation with `role` in `repo`, which starts the first time.
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
    // The expert answers in its own conversation, with the project memory: it never works at a step. In the
    // middle of a turn, it gets the message at its end.
    if role == termf_agents::EXPERT {
        let session = standing(app, id, role)?;
        let queued = agents.send_or_queue(session.id, text)?;
        return Ok(Addressed {
            session: agents.session(session.id)?,
            via: if queued { "queued" } else { "direct" },
            run: None,
            step: None,
        });
    }
    let at_work = step_at_work(&team::team(app).runs()?, id, role);
    let busy = match route(at_work, |session| agents.send(session, text).is_ok()) {
        Route::Step { run, step, session } => {
            return Ok(Addressed {
                session: agents.session(session)?,
                via: "step",
                run: Some(run),
                step: Some(step),
            });
        }
        Route::Direct { busy } => busy,
    };
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
    let (run, step) = busy.unzip();
    Ok(Addressed {
        session,
        via: "direct",
        run,
        step,
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
    let Some(cto) = conversation(&agents, project, "cto")? else {
        return Ok(());
    };
    let (id, cwd) = (to.session.id, to.session.spec.cwd.display());
    let said = match (to.via, to.run, to.step.as_deref()) {
        ("step", Some(run), Some(step)) => format!(
            "The user wrote to {role}, at its step {step} of team run {run} (conversation {id}): {text}"
        ),
        (_, Some(run), Some(step)) => format!(
            "The user wrote to {role} directly, in conversation {id} ({cwd}), its step {step} of team run {run} \
             being in the middle of a turn: {text}"
        ),
        ("queued", ..) => format!(
            "The user wrote to {role}, in conversation {id}, which reads it once its turn under way ends: {text}"
        ),
        _ => format!("The user wrote to {role} directly, in conversation {id} ({cwd}): {text}"),
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
    let Some(cto) = conversation(&agents, &run.target.project, "cto")? else {
        return Ok(());
    };
    agents.notify(cto.id, notice_of(event, &run, about), true)
}

/// The notice of a gate or a blocked step. The event says why; the run, read without the team's lock and maybe
/// before the change is saved, only gives the ticket, the step's role and its session.
fn notice_of(event: &TeamEvent, run: &TeamRun, about: &str) -> Notice {
    let step = run
        .steps
        .iter()
        .find(|s| Some(&s.key) == event.step.as_ref());
    // A command's output may hold a secret: none goes to an agent.
    let detail = mask_text(&event.message);
    let key = event.step.clone().unwrap_or_default();
    let what = match about {
        "waiting" => format!("the gate {key} waits for the user's approval"),
        _ => format!("the step {key} is blocked"),
    };
    let text = format!(
        "Team run {} ({} · {}): {what}. {detail}",
        event.run, run.target.ticket, run.target.title
    );
    Notice {
        about: about.into(),
        run: Some(event.run),
        step: event.step.clone(),
        role: step.and_then(|s| s.role.clone()),
        conversation: step.and_then(|s| s.session),
        // A gate says nothing more than that it waits; a blocked step says why.
        detail: (about == "blocked").then_some(detail),
        text: mask_text(&text),
    }
}

/// A tool of `termf mcp`, for agent session `session` and within its project only. Nothing that looks like a
/// secret goes back to the agent.
pub fn call(app: &AppHandle, session: i64, tool: McpCall) -> Result<Value> {
    let agents = agents(app);
    let conversation = agents.session(session)?;
    let name = serde_json::to_value(&tool)?["tool"].clone();
    ensure!(
        termf_agents::mcp_tools(&conversation.spec)
            .iter()
            .any(|t| name == *t),
        "agent session {session} has no TermF tool {name}"
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
                    team::start(app, Some(run), None, options)?
                }
                (None, Some(ticket), None) => {
                    from_ticket(app, id, &ticket, repo.as_deref(), options)?
                }
                // A run of its own, closed again when its team does not start.
                (None, None, Some(text)) => {
                    let request = Request {
                        text,
                        project: id.to_owned(),
                        repo,
                    };
                    team::start(app, None, Some(request), options)?
                }
                _ => bail!("give one of run, ticket or request"),
            };
            summary(&started)
        }
        McpCall::Answer {
            run,
            step,
            answers,
            note,
        } => {
            owned(run)?;
            let note = note.unwrap_or_default();
            let answered = match answers {
                Value::String(text) => {
                    let text = format!("{text}\n{note}");
                    team.revise(run, &step, text.trim())?
                }
                Value::Object(answers) => {
                    team.answer(run, &step, questionnaire(answers)?, &note)?
                }
                _ => bail!("answers: each question's id with its answer, or a text"),
            };
            summary(&answered)
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
        McpCall::MemorySearch {
            query,
            sources,
            limit,
        } => {
            let sources = sources
                .iter()
                .map(|s| termf_memory::Source::parse(s))
                .collect::<Result<Vec<_>>>()?;
            let hits = memory::search(app, id, &query, &sources, limit.unwrap_or(10))?;
            serde_json::to_value(hits)?
        }
        McpCall::MemoryRead { id: doc, offset } => {
            serde_json::to_value(memory::read(app, id, doc, offset.unwrap_or(0))?)?
        }
        McpCall::AskExpert { question } => {
            let answer = memory::ask(app, id, &question, None)?;
            json!({ "answer": answer.text, "conversation": answer.session, "cost": answer.cost })
        }
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
    team::start(app, Some(run.id), None, options).inspect_err(|_| {
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
        // A shipped role plays in the team even when it is off for terminals.
        .filter(|i| i.kind == Kind::Role && (i.active || i.layer == Layer::Shipped))
        .map(|i| json!({ "role": i.name, "description": i.description }))
        .collect();
    let repos: Vec<Value> = project
        .repos
        .iter()
        .map(|r| json!({ "name": r.name, "path": r.path }))
        .collect();
    Ok(json!({ "id": project.id, "name": project.name, "repos": repos, "roles": roles }))
}

/// The answers of a questionnaire as `team_answer` takes them: a text, yes or no, or a list for a multiple choice.
fn questionnaire(answers: serde_json::Map<String, Value>) -> Result<BTreeMap<String, Vec<String>>> {
    answers
        .into_iter()
        .map(|(id, answer)| {
            let values = match answer {
                Value::String(text) => vec![text],
                Value::Bool(yes) => vec![if yes { "yes" } else { "no" }.to_owned()],
                Value::Array(items) => items
                    .into_iter()
                    .map(|v| v.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .with_context(|| format!("question {id}: a list of texts"))?,
                _ => bail!("question {id}: a text, yes or no, or a list of texts"),
            };
            Ok((id, values))
        })
        .collect()
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
        "routing": run.routing,
        "atWork": at_work,
        "waits": waits(run),
    })
}

/// The gates and blocked steps that wait for the user, with their questions: the triage's, the plan's or a
/// report's.
fn waits(run: &TeamRun) -> Vec<Value> {
    run.steps
        .iter()
        .enumerate()
        .filter(|(_, s)| matches!(s.state, StepState::Waiting | StepState::Blocked))
        .map(|(i, s)| {
            json!({ "step": s.key, "state": s.state, "role": s.role, "task": s.task, "note": s.note, "questions": run.questions(i) })
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
    blocking(app, move |app| standing(app, &project, "cto")).await
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
    fn the_cto_s_questions_wait_with_their_ids_and_answers_come_back_structured() {
        let mut triage = step("triage", "cto", "blocked", Some(3));
        triage["kind"] = json!("agent");
        let mut asking = run(9, "acme", "waiting", json!([triage]));
        asking.routing = Some(
            serde_json::from_value(json!({"kind": "feature", "size": "M", "reason": "r", "workflow": "default",
                "skip": [], "add": [], "efforts": [], "squads": [], "budget_usd": 0.0,
                "questions": [{"id": "period", "theme": "product", "question": "Which period?", "why": "volume",
                    "kind": "choice", "options": ["shown", "all"], "proposed": ["shown"], "implications": []}]}))
            .unwrap(),
        );
        let waits = waits(&asking);
        assert_eq!(waits[0]["questions"][0]["id"], "period");
        assert_eq!(summary(&asking)["routing"]["workflow"], "default");

        let answers =
            json!({"period": "all of it, monthly", "public": false, "columns": ["date", "vat"]});
        let Value::Object(answers) = answers else {
            unreachable!()
        };
        let read = questionnaire(answers).unwrap();
        assert_eq!(read["period"], ["all of it, monthly"]);
        assert_eq!(read["public"], ["no"]);
        assert_eq!(read["columns"], ["date", "vat"]);
        let Value::Object(bad) = json!({"columns": [1]}) else {
            unreachable!()
        };
        assert!(questionnaire(bad).is_err());
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

    #[test]
    fn a_message_to_a_step_in_the_middle_of_a_turn_goes_to_the_direct_conversation() {
        let at_work = || Some((3, "implement:t2".to_string(), 8));
        assert_eq!(
            route(at_work(), |s| s == 8),
            Route::Step {
                run: 3,
                step: "implement:t2".into(),
                session: 8
            }
        );
        assert_eq!(
            route(at_work(), |_| false),
            Route::Direct {
                busy: Some((3, "implement:t2".into()))
            },
            "refused by the step's session: never lost, and the busy step is said"
        );
        let mut asked = false;
        assert_eq!(
            route(None, |_| {
                asked = true;
                true
            }),
            Route::Direct { busy: None }
        );
        assert!(!asked);
    }

    #[test]
    fn a_relayed_step_says_why_from_the_event_not_from_the_stored_run() {
        let mut stale = step("implement:t2", "back", "running", Some(8));
        stale["note"] = json!("an older reason");
        let run = run(3, "acme", "running", json!([stale]));
        let event = TeamEvent {
            id: 1,
            run: 3,
            at: 0,
            step: Some("implement:t2".into()),
            kind: "step_blocked".into(),
            message:
                "the report is blocked: which table? token ghp_0123456789abcdefghijklmnopqrstuvwxyz"
                    .into(),
        };
        let notice = notice_of(&event, &run, "blocked");
        let detail = notice.detail.unwrap();
        assert!(
            detail.starts_with("the report is blocked: which table?"),
            "{detail}"
        );
        assert!(
            !detail.contains("ghp_") && !notice.text.contains("ghp_"),
            "masked"
        );
        assert!(!notice.text.contains("older"));
        assert_eq!(
            (notice.role.as_deref(), notice.conversation),
            (Some("back"), Some(8))
        );
        let gate = notice_of(&event, &run, "waiting");
        assert_eq!(gate.detail, None);
    }
}
