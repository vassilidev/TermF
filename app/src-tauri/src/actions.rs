//! The actions of TermF's screens an agent asks for (`actions` and `do_action` of `termf mcp`). The screens hold
//! them, so each call goes to the front end (`agent-action`), which answers with `actions_reply`: the list, or, once
//! the user approved the action in TermF's card and it ran, its result. What the user types in that card (a password,
//! a token) goes to the vault or the keychain, never back here.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use termf_agents::AgentSession;
use termf_core::ipc::McpCall;

use crate::{CmdResult, err};

/// The list comes at once from a window that runs.
const LIST_WAIT: Duration = Duration::from_secs(20);
/// The user's approval (the card waits 5 minutes) and the action's own run.
const DO_WAIT: Duration = Duration::from_secs(15 * 60);

/// A call the front end answers; no `action`: the list.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionCall {
    pub id: u64,
    pub session: i64,
    pub project: String,
    pub role: Option<String>,
    pub title: String,
    pub action: Option<String>,
    pub args: serde_json::Map<String, Value>,
    pub hints: BTreeMap<String, String>,
}

type Answer = Result<Value, String>;

/// The calls waiting for the front end's answer.
#[derive(Default)]
pub struct ActionsState {
    next: AtomicU64,
    pending: Mutex<BTreeMap<u64, mpsc::Sender<Answer>>>,
}

impl ActionsState {
    /// `show` hears `agent-action`, and `agent-action-done` when nobody answered: the front end drops its card.
    fn ask(
        &self,
        mut call: ActionCall,
        wait: Duration,
        show: &dyn Fn(&str, &ActionCall),
        gone: &dyn Fn() -> bool,
    ) -> Result<Value> {
        call.id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(call.id, tx);
        show("agent-action", &call);
        let deadline = Instant::now() + wait;
        let answer = loop {
            match rx.recv_timeout(Duration::from_millis(250).min(wait)) {
                Ok(answer) => break Some(answer),
                Err(_) if gone() => break None,
                Err(_) if Instant::now() >= deadline => break None,
                Err(_) => {}
            }
        };
        self.pending.lock().unwrap().remove(&call.id);
        match answer {
            Some(Ok(value)) => Ok(value),
            Some(Err(why)) => Err(anyhow!(why)),
            None => {
                show("agent-action-done", &call);
                match gone() {
                    true => bail!("the caller went away"),
                    false => bail!(
                        "TermF did not answer in time: nothing was done. Tell the user what you wanted to do."
                    ),
                }
            }
        }
    }

    fn reply(&self, id: u64, answer: Answer) -> Result<()> {
        let tx = self.pending.lock().unwrap().remove(&id);
        let tx = tx.ok_or_else(|| anyhow!("this action is no longer waiting"))?;
        let _ = tx.send(answer);
        Ok(())
    }
}

/// `actions` and `do_action` of `termf mcp`, for `session` in `project`; `gone`: its caller went away.
pub fn call(
    app: &AppHandle,
    session: &AgentSession,
    project: &str,
    tool: McpCall,
    gone: &dyn Fn() -> bool,
) -> Result<Value> {
    let (action, args, hints, wait) = match tool {
        McpCall::Actions {} => (None, Default::default(), Default::default(), LIST_WAIT),
        McpCall::DoAction {
            action,
            args,
            hints,
        } => (Some(action), args, hints, DO_WAIT),
        _ => bail!("not a screen's action"),
    };
    let call = ActionCall {
        id: 0,
        session: session.id,
        project: project.to_owned(),
        role: session.spec.role.clone(),
        title: session.spec.title.clone(),
        action,
        args,
        hints,
    };
    let show = |event: &str, call: &ActionCall| {
        let _ = app.emit(event, call);
    };
    app.state::<ActionsState>().ask(call, wait, &show, gone)
}

/// The front end's answer to a call: `result`, or `error` in plain words.
#[tauri::command]
pub fn actions_reply(
    state: State<ActionsState>,
    id: u64,
    result: Option<Value>,
    error: Option<String>,
) -> CmdResult<()> {
    let answer = match error {
        Some(why) => Err(why),
        None => Ok(result.unwrap_or(Value::Null)),
    };
    state.reply(id, answer).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn call(action: Option<&str>) -> ActionCall {
        ActionCall {
            id: 0,
            session: 4,
            project: "acme".into(),
            role: Some("devops".into()),
            title: "devops · api".into(),
            action: action.map(Into::into),
            args: json!({"name": "acme-prod"}).as_object().unwrap().clone(),
            hints: BTreeMap::new(),
        }
    }

    /// Answers each call shown with `answer`, from another thread, as the front end does.
    fn answered(state: &Arc<ActionsState>, answer: Answer) -> impl Fn(&str, &ActionCall) {
        let state = state.clone();
        move |event: &str, call: &ActionCall| {
            if event == "agent-action" {
                let (state, id, answer) = (state.clone(), call.id, answer.clone());
                std::thread::spawn(move || state.reply(id, answer).unwrap());
            }
        }
    }

    #[test]
    fn the_front_end_s_answer_goes_back_to_the_agent_as_given() {
        let state = Arc::new(ActionsState::default());
        let never = || false;
        let listed = json!({"screen": "ops", "actions": [{"id": "ops.add_connection"}]});
        let show = answered(&state, Ok(listed.clone()));
        assert_eq!(
            state
                .ask(call(None), Duration::from_secs(5), &show, &never)
                .unwrap(),
            listed
        );
        let show = answered(
            &state,
            Err("The user refused this in TermF: it did not run.".into()),
        );
        let refused = state
            .ask(
                call(Some("ops.add_connection")),
                Duration::from_secs(5),
                &show,
                &never,
            )
            .unwrap_err();
        assert!(format!("{refused:#}").starts_with("The user refused"));
        assert!(state.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn nobody_answering_or_the_caller_leaving_drops_the_card() {
        let state = ActionsState::default();
        let shown = Mutex::new(Vec::new());
        let show = |event: &str, call: &ActionCall| {
            shown.lock().unwrap().push((event.to_owned(), call.id));
        };
        let late = state
            .ask(
                call(Some("ci.rerun")),
                Duration::from_millis(300),
                &show,
                &|| false,
            )
            .unwrap_err();
        assert!(format!("{late:#}").contains("did not answer in time"));
        let left = state
            .ask(
                call(Some("ci.rerun")),
                Duration::from_secs(5),
                &show,
                &|| true,
            )
            .unwrap_err();
        assert_eq!(format!("{left:#}"), "the caller went away");
        assert_eq!(
            *shown.lock().unwrap(),
            [
                ("agent-action".to_owned(), 1),
                ("agent-action-done".to_owned(), 1),
                ("agent-action".to_owned(), 2),
                ("agent-action-done".to_owned(), 2),
            ]
        );
        assert!(
            state.reply(1, Ok(json!(true))).is_err(),
            "a late answer finds nothing"
        );
    }
}
