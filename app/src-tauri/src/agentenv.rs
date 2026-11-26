//! What agents get of the environment (`termf_core::agent_env`): the vault's values given to a project's agents
//! (`[agents.secrets]`), the keys an agent asks for (`request_secret`), and the settings the UI shows. The value
//! of a key never goes back to the agent that asked, nor into a log: only whether it was granted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};
use termf_agents::{AgentSession, Given};
use termf_core::agent_env::{self, AgentEnv, Grant};
use termf_vault::{Access, Actor, Scope, Secret, SecretId, Vault};

use crate::{CmdResult, err};

/// How long `request_secret` waits for the user before telling the agent the request stays in TermF.
const WAIT: Duration = Duration::from_secs(300);
/// Requests waiting at once in a project: an agent that asks again and again does not fill the Home.
const MOST: usize = 10;

pub struct AgentEnvState {
    config_dir: PathBuf,
    requests: Requests,
}

pub fn state(config_dir: PathBuf) -> AgentEnvState {
    AgentEnvState {
        config_dir,
        requests: Requests::default(),
    }
}

/// A key an agent asks for, as the Home shows it: never a value.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SecretRequest {
    pub id: u64,
    pub project: String,
    pub variable: String,
    pub purpose: String,
    pub signup_url: Option<String>,
    pub agent: termf_ops::Agent,
    /// Unix seconds.
    pub at: u64,
    /// The project's agents already get the key, but the vault is locked: the user only opens it.
    pub vault: bool,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The vault's values `[agents.secrets]` gives the agents of `project`, read for `actor`, and what to tell when some
/// are missing: a locked vault starts the agent without them.
pub fn given(vault: &Vault, config_dir: &Path, project: Option<&str>, actor: &Actor) -> Given {
    let env = agent_env::load(config_dir, project);
    let names = |grants: &[&Grant]| {
        let names: Vec<&str> = grants.iter().map(|g| g.variable.as_str()).collect();
        names.join(", ")
    };
    let all: Vec<&Grant> = env.secrets.iter().collect();
    if all.is_empty() {
        return Given::default();
    }
    if !vault.initialized() {
        return Given {
            vars: Vec::new(),
            note: Some(format!(
                "TermF: there is no vault yet, so this agent started without {}. Create the vault in TermF and keep the keys there, then start the agent again.",
                names(&all)
            )),
        };
    }
    if !vault.status().is_ok_and(|s| s.unlocked) {
        return Given {
            vars: Vec::new(),
            note: Some(format!(
                "TermF: the vault is locked, so this agent started without {}. Unlock the vault in TermF, then start the agent again.",
                names(&all)
            )),
        };
    }
    let mut vars = Vec::new();
    let mut missing = Vec::new();
    for grant in all {
        match value(vault, project, grant, actor) {
            Ok(secret) => vars.push((grant.variable.clone(), secret.expose().to_owned())),
            Err(_) => missing.push(grant),
        }
    }
    let note = (!missing.is_empty()).then(|| {
        format!(
            "TermF: the vault has no value for {}, so this agent started without it.",
            names(&missing)
        )
    });
    Given { vars, note }
}

/// The project's secret of that name and scope, else the global one. Never prod: `agent_env` refuses it.
fn value(vault: &Vault, project: Option<&str>, grant: &Grant, actor: &Actor) -> Result<Secret> {
    let id = owner(vault, project, grant)?.context("no such secret")?;
    vault.value(actor, &id, Access::Use)
}

/// The secret a grant names for `project`: the project's, else the global one; never another project's.
fn owner(vault: &Vault, project: Option<&str>, grant: &Grant) -> Result<Option<SecretId>> {
    let scope = Scope::parse(&grant.scope)?;
    ensure!(scope != Scope::Prod, "agents never get a prod secret");
    let listed = vault.list(Some(scope), project)?;
    let owner = listed
        .iter()
        .filter(|s| {
            s.name == grant.secret && (s.project.is_none() || s.project.as_deref() == project)
        })
        .map(|s| s.project.clone())
        .max();
    owner
        .map(|p| SecretId::new(&grant.secret, scope, p.as_deref()))
        .transpose()
}

/// A request, and the agents waiting for its answer.
type Pending = (SecretRequest, Vec<mpsc::Sender<bool>>);

/// The requests waiting for the user.
#[derive(Default)]
pub struct Requests {
    next: AtomicU64,
    pending: Mutex<BTreeMap<u64, Pending>>,
}

impl Requests {
    /// Shows `request` (`show` hears `agentenv-request`), or joins the same one already waiting, and waits for the
    /// answer up to `wait`: the request stays in TermF after that, for the agent's next start.
    fn ask(
        &self,
        request: SecretRequest,
        wait: Duration,
        show: &dyn Fn(&str, &SecretRequest),
        gone: &dyn Fn() -> bool,
    ) -> Result<Value> {
        self.ask_until(request, wait, show, gone, &|| false)
    }

    /// `ask`, also granted as soon as `settled` says so (checked every second): the vault opened anywhere.
    fn ask_until(
        &self,
        mut request: SecretRequest,
        wait: Duration,
        show: &dyn Fn(&str, &SecretRequest),
        gone: &dyn Fn() -> bool,
        settled: &dyn Fn() -> bool,
    ) -> Result<Value> {
        let variable = request.variable.clone();
        let (tx, rx) = mpsc::channel();
        let id = {
            let mut pending = self.pending.lock().unwrap();
            let same = pending.iter_mut().find(|(_, (r, _))| {
                r.project == request.project && r.variable == variable && r.vault == request.vault
            });
            match same {
                Some((id, (_, waiting))) => {
                    waiting.push(tx);
                    *id
                }
                None => {
                    let mine = pending
                        .values()
                        .filter(|(r, _)| r.project == request.project);
                    ensure!(
                        mine.count() < MOST,
                        "{MOST} requests already wait for the user in this project: go on without the key"
                    );
                    request.id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
                    show("agentenv-request", &request);
                    let id = request.id;
                    pending.insert(id, (request, vec![tx]));
                    id
                }
            }
        };
        let deadline = Instant::now() + wait;
        let mut checked = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_millis(250).min(wait)) {
                Ok(granted) => return Ok(answer(&variable, Some(granted))),
                Err(_) if gone() => bail!("the caller went away"),
                Err(_) if Instant::now() >= deadline => return Ok(answer(&variable, None)),
                Err(_) if checked.elapsed() >= Duration::from_secs(1) => {
                    checked = Instant::now();
                    if settled() {
                        // Another waiter may have finished it first: its answer is on the way.
                        let _ = self.finish(id, true, show);
                    }
                }
                Err(_) => {}
            }
        }
    }

    fn get(&self, id: u64) -> Result<SecretRequest> {
        let pending = self.pending.lock().unwrap();
        let (request, _) = pending
            .get(&id)
            .context("this request is no longer waiting")?;
        Ok(request.clone())
    }

    /// Tells the agents that wait, and `show` hears `agentenv-request-done`.
    fn finish(&self, id: u64, granted: bool, show: &dyn Fn(&str, &SecretRequest)) -> Result<()> {
        let (request, waiting) = self
            .pending
            .lock()
            .unwrap()
            .remove(&id)
            .context("this request is no longer waiting")?;
        for tx in waiting {
            let _ = tx.send(granted);
        }
        show("agentenv-request-done", &request);
        Ok(())
    }
}

/// A request checked as the agent made it: the variable's name, a purpose on one line, an https:// page.
fn checked(
    project: &str,
    variable: String,
    purpose: String,
    signup_url: Option<String>,
    agent: termf_ops::Agent,
) -> Result<SecretRequest> {
    agent_env::check_variable(&variable)?;
    let purpose = purpose.split_whitespace().collect::<Vec<_>>().join(" ");
    ensure!(
        !purpose.is_empty() && purpose.chars().count() <= 300,
        "say in one short sentence what the key is for (300 characters at most)"
    );
    let signup_url = signup_url
        .map(|u| u.trim().to_owned())
        .filter(|u| !u.is_empty());
    if let Some(url) = &signup_url {
        ensure!(
            url.starts_with("https://")
                && url.len() <= 300
                && !url.chars().any(|c| c.is_whitespace() || c.is_control()),
            "signup_url: an https:// address"
        );
    }
    Ok(SecretRequest {
        id: 0,
        project: project.to_owned(),
        variable,
        purpose,
        signup_url,
        agent,
        at: now(),
        vault: false,
    })
}

/// `request_secret` of `termf mcp`: a card for the user, who pastes the key or refuses. The agent gets the answer,
/// never the value: the variable is there when its step or session starts again.
pub fn request(
    app: &AppHandle,
    session: &AgentSession,
    project: &str,
    variable: String,
    purpose: String,
    signup_url: Option<String>,
    gone: &dyn Fn() -> bool,
) -> Result<Value> {
    let agent = termf_ops::Agent {
        session: session.id,
        claude: session.session_id.clone(),
        run: crate::ops::run_of(app, session.id),
        title: session.spec.title.clone(),
        role: session.spec.role.clone(),
    };
    let request = checked(project, variable, purpose, signup_url, agent)?;
    let state = app.state::<AgentEnvState>();
    let vault = crate::vault::shared(app);
    let show = |event: &str, request: &SecretRequest| {
        let _ = app.emit(event, request);
    };
    let known = known(&vault, &state.config_dir, project, &request.variable);
    let known = match known {
        Some(locked) if locked["status"] == "locked" => {
            let (requests, config) = (&state.requests, state.config_dir.as_path());
            unlock_first(
                requests, &vault, config, &request, locked, WAIT, &show, gone,
            )?
        }
        known => known,
    };
    match known {
        Some(known) => Ok(known),
        None => state.requests.ask(request, WAIT, &show, gone),
    }
}

/// The project's agents already get `variable`, but the vault is locked: TermF's vault card asks the user to open
/// it, then the request goes on (`None`: the vault has no value after all, the user gives one). Refused, the agent
/// is told; nobody opens it in time, the agent gets `locked` and reports blocked.
#[allow(clippy::too_many_arguments)]
fn unlock_first(
    requests: &Requests,
    vault: &Vault,
    config_dir: &Path,
    request: &SecretRequest,
    locked: Value,
    wait: Duration,
    show: &dyn Fn(&str, &SecretRequest),
    gone: &dyn Fn() -> bool,
) -> Result<Option<Value>> {
    let asking = SecretRequest {
        vault: true,
        ..request.clone()
    };
    let open = || vault.status().is_ok_and(|s| s.unlocked);
    let asked = requests.ask_until(asking, wait, show, gone, &open)?;
    Ok(match asked["status"].as_str() {
        Some("granted") => known(vault, config_dir, &request.project, &request.variable),
        Some("refused") => Some(asked),
        _ => Some(locked),
    })
}

/// The answer without asking the user, when the project's agents are already given `variable`: `granted` when the
/// vault has its value, `locked` while the vault is locked; `None` when the user has a key to give (no vault yet, or
/// no value in it).
fn known(vault: &Vault, config_dir: &Path, project: &str, variable: &str) -> Option<Value> {
    let env = agent_env::load(config_dir, Some(project));
    let grant = env.secrets.iter().find(|g| g.variable == variable)?;
    if !vault.initialized() {
        return None;
    }
    if !vault.status().is_ok_and(|s| s.unlocked) {
        return Some(json!({
            "status": "locked",
            "message": format!(
                "{variable} is given to this project's agents, but TermF's vault is locked, so it cannot reach you. \
                 Report blocked and say so: once the user unlocks the vault, your step runs again with it."
            ),
        }));
    }
    match owner(vault, Some(project), grant) {
        Ok(Some(_)) => Some(answer(variable, Some(true))),
        _ => None,
    }
}

/// What the agent reads: `granted`, `refused`, or `waiting` when the user did not answer in time.
fn answer(variable: &str, granted: Option<bool>) -> Value {
    match granted {
        Some(true) => json!({
            "status": "granted",
            "message": format!(
                "The user gave {variable} to this project's agents. It is not in your environment yet: TermF adds it \
                 when this step or session starts again. If you need it now, report blocked and say the key was \
                 granted, so that your step runs again with it; never print, write or log its value."
            ),
        }),
        Some(false) => json!({
            "status": "refused",
            "message": format!(
                "The user refused to give {variable}. Do not ask again unless they tell you to: work with the sources \
                 that need no key, and say what is missing."
            ),
        }),
        None => json!({
            "status": "waiting",
            "message": format!(
                "The user has not answered yet: the request for {variable} waits in TermF. Go on without it; if you \
                 cannot, report blocked and say you asked for it."
            ),
        }),
    }
}

/// Keeps `value` in the project's vault (dev) and gives it to the project's agents. A locked vault opens with
/// `password`.
fn store(
    vault: &Vault,
    config_dir: &Path,
    request: &SecretRequest,
    value: &Secret,
    password: Option<&Secret>,
) -> Result<()> {
    ensure!(!value.is_empty(), "paste the key first");
    ensure!(
        vault.initialized(),
        "create the vault first: the key is kept there"
    );
    let human = Actor::human();
    if !vault.status()?.unlocked {
        let password = password.ok_or(termf_vault::Locked)?;
        vault.unlock(&human, Some(password.expose()), None, None)?;
    }
    let secret = SecretId::new(&request.variable, Scope::Dev, Some(&request.project))?;
    vault.set(&human, &secret, value.clone(), Some(&request.purpose))?;
    agent_env::give(
        config_dir,
        &request.project,
        &request.variable,
        &format!("dev/{}", request.variable),
    )
}

#[tauri::command]
pub fn agentenv_requests(state: State<AgentEnvState>) -> Vec<SecretRequest> {
    let pending = state.requests.pending.lock().unwrap();
    pending.values().map(|(r, _)| r.clone()).collect()
}

/// The user's answer to a request: `value` stores it in the project's vault (dev) and gives it to the project's
/// agents; none refuses. A locked vault opens with `password`.
#[tauri::command]
pub async fn agentenv_answer(
    app: AppHandle,
    id: u64,
    value: Option<String>,
    password: Option<String>,
) -> CmdResult<()> {
    let value = value.map(|v| Secret::new(v.trim().to_owned()));
    let password = password.map(Secret::new);
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let state = app.state::<AgentEnvState>();
        let request = state.requests.get(id)?;
        if let Some(value) = &value {
            let vault = crate::vault::shared(&app);
            let was_open = vault.status().is_ok_and(|s| s.unlocked);
            let stored = store(
                &vault,
                &state.config_dir,
                &request,
                value,
                password.as_ref(),
            );
            // Opened here, even when what follows failed: the rest of the app sees it open.
            if let Ok(status) = vault.status()
                && status.unlocked
                && !was_open
            {
                let _ = app.emit("vault-status", status);
            }
            stored?;
        }
        let show = |event: &str, request: &SecretRequest| {
            let _ = app.emit(event, request);
        };
        state.requests.finish(id, value.is_some(), &show)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    inherit: bool,
    allow: Vec<String>,
    /// What every agent gets, whatever the settings say.
    base: Vec<&'static str>,
    problems: Vec<String>,
}

fn settings(config_dir: &Path) -> Settings {
    let AgentEnv {
        inherit,
        allow,
        problems,
        ..
    } = agent_env::load(config_dir, None);
    Settings {
        inherit,
        allow,
        base: match cfg!(windows) {
            true => [agent_env::BASE, agent_env::WINDOWS].concat(),
            false => agent_env::BASE.to_vec(),
        },
        problems,
    }
}

/// The global allowlist, for Settings.
#[tauri::command]
pub fn agentenv_settings(state: State<AgentEnvState>) -> Settings {
    settings(&state.config_dir)
}

#[tauri::command]
pub fn agentenv_allow_set(state: State<AgentEnvState>, allow: Vec<String>) -> CmdResult<Settings> {
    let allow: Vec<String> = allow
        .iter()
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect();
    agent_env::save_allow(&state.config_dir, &allow).map_err(err)?;
    Ok(settings(&state.config_dir))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Granted {
    #[serde(flatten)]
    grant: Grant,
    /// Whether the vault holds the secret; `None` while it is locked.
    found: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grants {
    grants: Vec<Granted>,
    problems: Vec<String>,
}

fn grants(config_dir: &Path, vault: &Vault, project: &str) -> Grants {
    let env = agent_env::load(config_dir, Some(project));
    let open = vault.initialized() && vault.status().is_ok_and(|s| s.unlocked);
    let grants = env
        .secrets
        .into_iter()
        .map(|grant| {
            let found = open.then(|| {
                Scope::parse(&grant.scope).is_ok_and(|scope| {
                    vault
                        .list(Some(scope), Some(project))
                        .is_ok_and(|l| l.iter().any(|s| s.name == grant.secret))
                })
            });
            Granted { grant, found }
        })
        .collect();
    Grants {
        grants,
        problems: env.problems,
    }
}

/// The keys the project's agents get from the vault, for its Vault view.
#[tauri::command]
pub fn agentenv_grants(app: AppHandle, state: State<AgentEnvState>, project: String) -> Grants {
    grants(&state.config_dir, &crate::vault::shared(&app), &project)
}

/// No longer gives `variable`: in the project's file, or in `termf.toml` when `global`. The vault keeps the secret.
#[tauri::command]
pub fn agentenv_revoke(
    app: AppHandle,
    state: State<AgentEnvState>,
    project: String,
    variable: String,
    global: bool,
) -> CmdResult<Grants> {
    let file = (!global).then_some(project.as_str());
    agent_env::take_back(&state.config_dir, file, &variable).map_err(err)?;
    Ok(grants(
        &state.config_dir,
        &crate::vault::shared(&app),
        &project,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Arc;
    use termf_sync::KeyStore;
    use termf_vault::{Action, ActorKind, SystemClock};

    const PASSWORD: &str = "correct horse battery";
    const KEY: &str = "px-5f3a9c71d2";
    const GRANT: &str = "[agents.secrets]\nPEXELS_API_KEY = \"pexels\"\n";

    #[derive(Default)]
    struct MemoryStore(Mutex<HashMap<String, Vec<u8>>>);

    impl KeyStore for MemoryStore {
        fn get(&self, account: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.0.lock().unwrap().get(account).cloned())
        }
        fn set(&self, account: &str, secret: &[u8]) -> Result<()> {
            self.0.lock().unwrap().insert(account.into(), secret.into());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<()> {
            self.0.lock().unwrap().remove(account);
            Ok(())
        }
    }

    /// A throwaway config with the project `acme`, and a vault created with `PASSWORD`, open.
    fn fixture(name: &str) -> (PathBuf, Vault) {
        let root =
            std::env::temp_dir().join(format!("termf-agentenv-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("config/projects")).unwrap();
        fs::write(
            root.join("config/projects/acme.toml"),
            "# Acme\nname = \"Acme\"\n\n[[repos]]\npath = \"/tmp/acme/api\"\n",
        )
        .unwrap();
        let vault = Vault::new(
            &root.join("data"),
            Box::new(MemoryStore::default()),
            Box::new(SystemClock::default()),
        );
        vault.init(&Actor::human(), PASSWORD, false).unwrap();
        (root, vault)
    }

    fn request(variable: &str) -> SecretRequest {
        let agent = termf_ops::Agent {
            session: 7,
            claude: "c7".into(),
            run: Some(41),
            title: "motion · acme".into(),
            role: Some("motion".into()),
        };
        let purpose = "Stock photos\nfor the launch video".into();
        let url = Some("https://www.pexels.com/api/".into());
        checked("acme", variable.into(), purpose, url, agent).unwrap()
    }

    #[test]
    fn an_agent_asks_and_reads_only_the_answer() {
        let requests = Arc::new(Requests::default());
        let shown: Arc<Mutex<Vec<(String, u64)>>> = Arc::default();
        let show = {
            let shown = shown.clone();
            move |event: &str, r: &SecretRequest| shown.lock().unwrap().push((event.into(), r.id))
        };
        let never = || false;
        let asking = {
            let (requests, show) = (requests.clone(), show.clone());
            std::thread::spawn(move || requests.ask(request("PEXELS_API_KEY"), WAIT, &show, &never))
        };
        let again = {
            let (requests, show) = (requests.clone(), show.clone());
            std::thread::spawn(move || {
                // The same key asked twice waits on the same card.
                std::thread::sleep(Duration::from_millis(100));
                requests.ask(request("PEXELS_API_KEY"), WAIT, &show, &never)
            })
        };
        while requests
            .pending
            .lock()
            .unwrap()
            .values()
            .map(|(_, w)| w.len())
            .sum::<usize>()
            < 2
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        let card = requests.get(1).unwrap();
        assert_eq!(card.purpose, "Stock photos for the launch video");
        assert_eq!(card.agent.run, Some(41));
        requests.finish(1, true, &show).unwrap();
        for reply in [
            asking.join().unwrap().unwrap(),
            again.join().unwrap().unwrap(),
        ] {
            assert_eq!(reply["status"], "granted");
            assert!(
                reply["message"]
                    .as_str()
                    .unwrap()
                    .contains("not in your environment yet")
            );
        }
        assert_eq!(
            *shown.lock().unwrap(),
            [
                ("agentenv-request".into(), 1),
                ("agentenv-request-done".into(), 1)
            ]
        );
        assert!(requests.get(1).is_err(), "answered: gone");

        let refused = {
            let (requests, show) = (requests.clone(), show.clone());
            std::thread::spawn(move || {
                requests.ask(request("PISTE_CLIENT_ID"), WAIT, &show, &never)
            })
        };
        while requests.get(2).is_err() {
            std::thread::sleep(Duration::from_millis(20));
        }
        requests.finish(2, false, &show).unwrap();
        assert_eq!(refused.join().unwrap().unwrap()["status"], "refused");

        // Unanswered in time: the agent goes on, the card stays.
        let waiting = requests
            .ask(
                request("UNSPLASH_ACCESS_KEY"),
                Duration::from_millis(300),
                &show,
                &never,
            )
            .unwrap();
        assert_eq!(waiting["status"], "waiting");
        assert!(requests.get(3).is_ok());
        let gone = requests.ask(request("UNSPLASH_ACCESS_KEY"), WAIT, &show, &|| true);
        assert!(gone.unwrap_err().to_string().contains("went away"));
    }

    #[test]
    fn names_and_pages_are_checked_before_any_card() {
        let agent = || termf_ops::Agent {
            session: 1,
            claude: String::new(),
            run: None,
            title: String::new(),
            role: None,
        };
        let ask = |variable: &str, purpose: &str, url: Option<&str>| {
            checked(
                "acme",
                variable.into(),
                purpose.into(),
                url.map(Into::into),
                agent(),
            )
        };
        for name in [
            "PATH",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "NODE_OPTIONS",
            "TERMF_TOKEN",
            "ANTHROPIC_API_KEY",
            "lower",
        ] {
            assert!(ask(name, "x", None).is_err(), "{name}");
        }
        assert!(ask("PEXELS_API_KEY", "  ", None).is_err());
        assert!(ask("PEXELS_API_KEY", &"x".repeat(301), None).is_err());
        assert!(
            ask(
                "PEXELS_API_KEY",
                "photos",
                Some("http://www.pexels.com/api/")
            )
            .is_err()
        );
        assert!(ask("PEXELS_API_KEY", "photos", Some("javascript:alert(1)")).is_err());
        assert!(ask("PEXELS_API_KEY", "photos", Some("https://x.test/a b")).is_err());
        assert!(
            ask("PEXELS_API_KEY", "photos", Some(""))
                .unwrap()
                .signup_url
                .is_none()
        );
    }

    #[test]
    fn a_locked_vault_is_asked_open_in_a_card_then_the_request_goes_on() {
        let (root, vault) = fixture("unlock-first");
        let config = root.join("config");
        let asking = request("PEXELS_API_KEY");
        store(&vault, &config, &asking, &Secret::new(KEY.into()), None).unwrap();
        vault.lock(&Actor::human()).unwrap();
        let locked = known(&vault, &config, "acme", "PEXELS_API_KEY").unwrap();
        let requests = Requests::default();
        let shown: Mutex<Vec<(String, bool)>> = Mutex::default();
        let show =
            |event: &str, r: &SecretRequest| shown.lock().unwrap().push((event.into(), r.vault));
        let never = || false;
        let ask = |wait| {
            unlock_first(
                &requests,
                &vault,
                &config,
                &asking,
                locked.clone(),
                wait,
                &show,
                &never,
            )
        };
        let card = |id| {
            while requests.get(id).is_err() {
                std::thread::sleep(Duration::from_millis(20));
            }
            requests.get(id).unwrap()
        };

        // Opened in TermF's vault card, or anywhere: the request goes on, granted, never with the value.
        let reply = std::thread::scope(|s| {
            let waiting = s.spawn(|| ask(WAIT));
            assert!(card(1).vault);
            vault
                .unlock(&Actor::human(), Some(PASSWORD), None, None)
                .unwrap();
            waiting.join().unwrap().unwrap().unwrap()
        });
        assert_eq!(reply["status"], "granted");
        assert!(!reply.to_string().contains(KEY));
        assert_eq!(
            *shown.lock().unwrap(),
            [
                ("agentenv-request".into(), true),
                ("agentenv-request-done".into(), true)
            ]
        );

        // Refused in the card: the agent is told.
        vault.lock(&Actor::human()).unwrap();
        let reply = std::thread::scope(|s| {
            let waiting = s.spawn(|| ask(WAIT));
            card(2);
            requests.finish(2, false, &show).unwrap();
            waiting.join().unwrap().unwrap().unwrap()
        });
        assert_eq!(reply["status"], "refused");

        // Nobody opens it in time: blocked and said, as before.
        let reply = ask(Duration::from_millis(300)).unwrap().unwrap();
        assert_eq!(reply["status"], "locked");
        assert!(
            reply["message"]
                .as_str()
                .unwrap()
                .contains("Report blocked")
        );
    }

    #[test]
    fn a_grant_keeps_the_key_in_the_vault_and_names_it_in_the_project() {
        let (root, vault) = fixture("grant");
        let config = root.join("config");
        let request = request("PEXELS_API_KEY");
        vault.lock(&Actor::human()).unwrap();
        let locked = store(&vault, &config, &request, &Secret::new(KEY.into()), None);
        assert!(locked.unwrap_err().to_string().contains("locked"));
        assert_eq!(
            known(&vault, &config, "acme", "PEXELS_API_KEY"),
            None,
            "not given yet: a card"
        );
        let password = Secret::new(PASSWORD.into());
        let key = Secret::new(KEY.into());
        store(&vault, &config, &request, &key, Some(&password)).unwrap();
        let again = known(&vault, &config, "acme", "PEXELS_API_KEY").unwrap();
        assert_eq!(again["status"], "granted");
        let file = fs::read_to_string(config.join("projects/acme.toml")).unwrap();
        assert!(
            file.starts_with("# Acme\nname = \"Acme\""),
            "the rest kept: {file}"
        );
        assert!(
            file.ends_with("[agents.secrets]\nPEXELS_API_KEY = \"dev/PEXELS_API_KEY\"\n"),
            "{file}"
        );
        let id = SecretId::new("PEXELS_API_KEY", Scope::Dev, Some("acme")).unwrap();
        assert_eq!(
            vault
                .value(&Actor::human(), &id, Access::Use)
                .unwrap()
                .expose(),
            KEY
        );
        assert!(!file.contains(KEY));

        // What the project's next agent gets, read for it and audited as its own.
        let agent = Actor::agent(Some(41), Some("7".into()));
        let got = given(&vault, &config, Some("acme"), &agent);
        assert_eq!(got.vars, [("PEXELS_API_KEY".to_owned(), KEY.to_owned())]);
        assert_eq!(got.note, None);
        let audit = vault.audit(1).unwrap();
        assert_eq!(audit[0].action, Action::Read);
        assert_eq!(audit[0].actor.kind, ActorKind::Agent);
        assert_eq!(audit[0].actor.run, Some(41));
        assert!(!serde_json::to_string(&audit).unwrap().contains(KEY));

        // Locked: the agent starts without it, and is told so.
        vault.lock(&Actor::human()).unwrap();
        let got = given(&vault, &config, Some("acme"), &agent);
        assert!(got.vars.is_empty());
        assert!(got.note.unwrap().contains("vault is locked"));
        // Asked again while it is locked: no `granted` the agent could not use.
        let locked = known(&vault, &config, "acme", "PEXELS_API_KEY").unwrap();
        assert_eq!(locked["status"], "locked");
        // Given, but the value is gone from the vault: the user gets a card again.
        vault
            .unlock(&Actor::human(), Some(PASSWORD), None, None)
            .unwrap();
        let id = SecretId::new("PEXELS_API_KEY", Scope::Dev, Some("acme")).unwrap();
        vault.delete(&Actor::human(), &id).unwrap();
        assert_eq!(known(&vault, &config, "acme", "PEXELS_API_KEY"), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_agent_gets_its_project_s_key_or_a_global_one_never_another_project_s() {
        let (root, vault) = fixture("owner");
        let config = root.join("config");
        let human = Actor::human();
        let set = |project: Option<&str>, value: &str| {
            let id = SecretId::new("pexels", Scope::Dev, project).unwrap();
            vault
                .set(&human, &id, Secret::new(value.into()), None)
                .unwrap();
        };
        set(Some("mobile"), "px-of-mobile");
        fs::write(config.join("termf.toml"), GRANT).unwrap();
        let agent = Actor::agent(None, Some("5".into()));
        for project in [None, Some("acme")] {
            let got = given(&vault, &config, project, &agent);
            assert!(got.vars.is_empty(), "{project:?} got another project's key");
            assert!(got.note.unwrap().contains("no value for PEXELS_API_KEY"));
        }
        set(None, "px-global");
        let value = |project| given(&vault, &config, project, &agent).vars[0].1.clone();
        assert_eq!(value(None), "px-global");
        assert_eq!(value(Some("acme")), "px-global");
        assert_eq!(value(Some("mobile")), "px-of-mobile", "the project's first");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn without_a_vault_the_agent_is_told_to_create_one() {
        let root = std::env::temp_dir().join(format!("termf-agentenv-none-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("termf.toml"), GRANT).unwrap();
        let vault = Vault::new(
            &root.join("data"),
            Box::new(MemoryStore::default()),
            Box::new(SystemClock::default()),
        );
        let got = given(&vault, &root, None, &Actor::agent(None, None));
        let note = got.note.unwrap();
        assert!(
            note.contains("no vault yet") && !note.contains("locked"),
            "{note}"
        );
        assert_eq!(
            known(&vault, &root, "acme", "PEXELS_API_KEY"),
            None,
            "the user gets a card"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_prod_secret_or_a_missing_one_never_reaches_an_agent() {
        let (root, vault) = fixture("prod");
        let config = root.join("config");
        let human = Actor::human();
        vault.reauth(&human, PASSWORD).unwrap();
        for scope in [Scope::Prod, Scope::Staging] {
            let id = SecretId::new("stripe", scope, None).unwrap();
            vault
                .set(&human, &id, Secret::new(format!("sk_{scope:?}_x")), None)
                .unwrap();
        }
        fs::write(
            config.join("termf.toml"),
            "[agents.secrets]\nSTRIPE_KEY = \"prod/stripe\"\nSTRIPE_TEST_KEY = \"staging/stripe\"\nGONE_KEY = \"gone\"\n",
        )
        .unwrap();
        let got = given(
            &vault,
            &config,
            Some("acme"),
            &Actor::agent(None, Some("3".into())),
        );
        assert_eq!(
            got.vars,
            [("STRIPE_TEST_KEY".to_owned(), "sk_Staging_x".to_owned())]
        );
        assert!(got.note.unwrap().contains("GONE_KEY"));
        let problems = agent_env::load(&config, None).problems.join("\n");
        assert!(
            problems.contains("STRIPE_KEY: agents never get a prod secret"),
            "{problems}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
