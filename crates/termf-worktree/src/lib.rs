//! Stack-aware worktree setup (ARCHITECTURE §9). Each project has a recipe in its library,
//! `recipes/worktree-setup.toml`, with a section per repository: the files git ignores that a worktree needs,
//! the dependency folders (cloned copy-on-write, else installed from the package manager's cache, never copied
//! raw), the local database to copy per worktree, Docker Compose and its ports, and commands. Saving it also
//! writes the `worktree-setup` skill, agent (role) and command of the project's library. Without a saved
//! recipe, what detection finds applies.
//!
//! The database copy a setup made is recorded in `<data>/worktree-databases.json` by worktree id (the run's), and
//! its ports in `<data>/ports.json`, so tearing it down works even once the worktree is gone. The last setup's
//! steps are kept in the worktree's own git folder.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

mod cow;
mod database;
mod detect;
mod files;
mod ports;
mod recipe;
#[cfg(test)]
mod tests;

pub use database::{DbView, Engine};
pub use detect::detect;
pub use ports::reserve as reserve_port;
pub use recipe::{Draft, RecipeView, RepoView, discard, propose, save, view};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const AGENT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const STATE_FILE: &str = "termf-setup.json";
const DATABASES_FILE: &str = "worktree-databases.json";

/// `<project library>/recipes/worktree-setup.toml`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Recipe {
    /// Runs the setup agent once the steps are done, for what needs judgment (migrations, seeds, an install
    /// that failed).
    pub agent: bool,
    /// What preparing a worktree of the project takes beyond the steps; it goes into the skill.
    pub notes: String,
    /// By repository name in the project.
    pub repos: BTreeMap<String, RepoRecipe>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct RepoRecipe {
    /// What the repository is made of, as detection or Claude saw it: `node:pnpm`, `laravel`, `mysql`…
    pub stack: Vec<String>,
    /// `.worktreeinclude` patterns: files git ignores that a worktree needs, copied from the repository.
    pub include: Vec<String>,
    pub deps: Vec<Dep>,
    /// The file that gives the local database's connection (an env file, `config/database.yml`); the worktree
    /// gets its own copy of that database.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Gives each worktree its own `COMPOSE_PROJECT_NAME`.
    pub compose: bool,
    /// Services that get a port of their own, `TERMF_PORT_<NAME>`, on top of `PORT`.
    pub ports: Vec<String>,
    /// Run in the worktree after the rest, through the shell.
    pub commands: Vec<String>,
}

/// A dependency folder: never copied raw.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Dep {
    /// Relative to the repository; empty when the tool keeps its packages in a shared cache.
    pub dir: String,
    /// Clones the repository's folder copy-on-write first, when the file system can.
    pub clone: bool,
    /// Tried in order until one succeeds, the offline one first; run after a clone too, to match the
    /// worktree's lockfile.
    pub install: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    /// Reading the recipe.
    Recipe,
    Include,
    Deps,
    Database,
    /// Ports and the Compose project name.
    Env,
    Command,
    Agent,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Copied,
    Cloned,
    Installed,
    /// Already there from an earlier setup.
    Present,
    Created,
    Ran,
    Set,
    Dropped,
    Released,
    /// Nothing to do, or nothing TermF could do (left to the agent).
    Skipped,
    /// Never done: a remote database, a config TermF cannot read.
    Refused,
    Failed,
}

impl Outcome {
    pub fn failed(self) -> bool {
        matches!(self, Outcome::Refused | Outcome::Failed)
    }
}

/// One step of a setup or a teardown.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub repo: String,
    pub kind: StepKind,
    /// What it worked on: a folder, a database, a command.
    pub target: String,
    pub outcome: Outcome,
    /// How: the command that worked, the files copied, or why it failed.
    pub detail: String,
    /// Milliseconds.
    pub ms: u64,
}

impl Step {
    /// One line of a run's journal.
    pub fn describe(&self) -> String {
        let outcome = serde_json::to_value(self.outcome).unwrap_or_default();
        let outcome = outcome.as_str().unwrap_or_default();
        match self.detail.is_empty() {
            true => format!("{} {outcome}", self.target),
            false => format!("{} {outcome} ({})", self.target, self.detail),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub steps: Vec<Step>,
    /// For the worktree's sessions: `PORT`, `TERMF_PORT_<NAME>`, `COMPOSE_PROJECT_NAME`.
    pub env: BTreeMap<String, String>,
    /// The project's saved recipe applied; otherwise what detection found.
    pub saved: bool,
}

impl Report {
    pub fn failed(&self) -> bool {
        self.steps.iter().any(|s| s.outcome.failed())
    }
}

/// A worktree to prepare.
#[derive(Clone, Debug)]
pub struct Worktree {
    pub project: String,
    /// The repository's name in the project.
    pub repo: String,
    /// The repository it comes from.
    pub source: PathBuf,
    pub path: PathBuf,
    /// Names its database copy and its Compose project: the run's id.
    pub id: String,
}

/// The last setup's steps and the environment it gave, in `<worktree git dir>/termf-setup.json`.
#[derive(Serialize, Deserialize, Default)]
struct State {
    steps: Vec<Step>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

pub struct Setup {
    pub config_dir: PathBuf,
    /// Holds the ports already given.
    pub data_dir: PathBuf,
    /// Added to every command it runs; tests point `PATH` at fakes and keep git away from the user's config.
    pub env: Vec<(String, String)>,
    /// Claude Code, for the setup agent.
    pub claude: Vec<String>,
}

impl Setup {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        let claude = claude_command(&config_dir);
        Self {
            config_dir,
            data_dir,
            env: Vec::new(),
            claude,
        }
    }

    /// The worktree at `path`, from its git metadata and the project that lists its repository. Refused for a
    /// repository's own checkout: its `.env` and database are the user's.
    pub fn locate(&self, path: &Path, id: Option<String>) -> Result<Worktree> {
        let path = git(&self.env, path, &["rev-parse", "--show-toplevel"])
            .with_context(|| format!("{} is not in a git worktree", path.display()))?;
        let common = git(
            &self.env,
            Path::new(&path),
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?;
        let source = Path::new(&common)
            .parent()
            .context("no repository")?
            .to_path_buf();
        let path = PathBuf::from(path);
        ensure!(
            !same(&path, &source),
            "{} is a repository, not one of its worktrees: TermF prepares worktrees only",
            path.display()
        );
        let (project, repo) = termf_core::load_projects(&self.config_dir)?
            .into_iter()
            .find_map(|p| {
                let repo = p.repos.iter().find(|r| same(Path::new(&r.path), &source))?;
                Some((p.id.clone(), repo.name.clone()))
            })
            .with_context(|| format!("{} is in no project", source.display()))?;
        let id =
            id.unwrap_or_else(|| format!("w{:06x}", hash(&path.to_string_lossy()) & 0xff_ffff));
        Ok(Worktree {
            project,
            repo,
            source,
            path,
            id,
        })
    }

    /// Prepares the worktree. A step that fails does not stop the others: everything is in the report.
    pub fn run(&self, wt: &Worktree) -> Report {
        let mut steps = Vec::new();
        let (recipe, repo, saved) = match recipe::effective(&self.config_dir, wt, &self.env) {
            Ok(found) => found,
            Err(e) => {
                let step = step(wt, StepKind::Recipe, "worktree-setup.toml", Instant::now());
                steps.push(step(Outcome::Failed, format!("{e:#}")));
                (Recipe::default(), RepoRecipe::default(), false)
            }
        };
        let mut env = BTreeMap::new();
        let mut agent_access = None;

        if !repo.include.is_empty() {
            let started = step(wt, StepKind::Include, ".worktreeinclude", Instant::now());
            let skip: Vec<&str> = repo.deps.iter().map(|d| d.dir.as_str()).collect();
            steps.push(
                match files::include(&self.env, &wt.source, &wt.path, &repo.include, &skip) {
                    Ok((copied, _)) if !copied.is_empty() => {
                        started(Outcome::Copied, copied.join(", "))
                    }
                    Ok((_, 0)) => started(Outcome::Skipped, "no ignored file matches".into()),
                    Ok(_) => started(Outcome::Present, String::new()),
                    Err(e) => started(Outcome::Failed, format!("{e:#}")),
                },
            );
        }
        for dep in &repo.deps {
            let mut step = self.dep(wt, dep);
            // Node and the tools that look up the folders would quietly take the main checkout's instead.
            let missing = step.outcome == Outcome::Failed || !wt.path.join(&dep.dir).exists();
            let named = !dep.dir.is_empty() && wt.source.join(&dep.dir).is_dir();
            if missing && named && wt.path.starts_with(&wt.source) {
                let note = format!(
                    "the worktree is inside the repository: tools that look up the folders, as Node does, may \
                     use the repository's own {} instead",
                    dep.dir
                );
                step.detail = match step.detail.is_empty() {
                    true => note,
                    false => format!("{}; {note}", step.detail),
                };
            }
            steps.push(step);
        }
        if let Some(from) = &repo.database {
            let started = step(wt, StepKind::Database, from, Instant::now());
            let compose = detect::compose(&wt.source)
                .map(|c| c.services)
                .unwrap_or_default();
            let previous = self.made(&wt.id);
            let outcome = database::copy(&self.env, wt, from, &compose, previous.as_ref());
            agent_access = database::access(&wt.source, from);
            steps.push(match outcome {
                Ok(done) => {
                    env.extend(done.env);
                    // Unrecorded, the copy could never be dropped: the step says so.
                    let saved = done.made.map_or(Ok(()), |m| self.set_made(&wt.id, Some(m)));
                    let mut step = match saved {
                        Ok(()) => started(done.outcome, done.detail),
                        Err(e) => started(Outcome::Failed, format!("{}; {e:#}", done.detail)),
                    };
                    step.target = done.target;
                    step
                }
                Err(e) => started(Outcome::Failed, format!("{e:#}")),
            });
        }

        let started = step(wt, StepKind::Env, "", Instant::now());
        let mut names = vec!["port".to_owned()];
        for name in repo.ports.iter().map(|p| ports::name(p)) {
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        match ports::allocate(&self.data_dir, &wt.path, &names) {
            Ok(given) => {
                for (name, port) in given {
                    let key = match name.as_str() {
                        "port" => "PORT".to_owned(),
                        name => format!("TERMF_PORT_{}", name.to_ascii_uppercase()),
                    };
                    env.insert(key, port.to_string());
                }
                if repo.compose {
                    env.insert("COMPOSE_PROJECT_NAME".into(), compose_name(&wt.id));
                }
                let set: Vec<String> = env.iter().map(|(k, v)| format!("{k}={v}")).collect();
                let mut step = started(Outcome::Set, String::new());
                step.target = set.join(" ");
                steps.push(step);
            }
            Err(e) => steps.push(started(Outcome::Failed, format!("{e:#}"))),
        }

        for line in repo.commands.iter().filter(|c| !c.trim().is_empty()) {
            let started = step(wt, StepKind::Command, line, Instant::now());
            let before = self.changes(&wt.path);
            let mut cmd = shell(line);
            cmd.current_dir(&wt.path).envs(&env);
            steps.push(match self.execute(cmd, COMMAND_TIMEOUT) {
                Ok(_) => match self.left(&wt.path, &before) {
                    Some(left) => started(Outcome::Failed, left),
                    None => started(Outcome::Ran, String::new()),
                },
                Err(e) => started(Outcome::Failed, format!("{e:#}")),
            });
        }
        if recipe.agent {
            steps.push(self.agent(wt, &steps, &env, agent_access.as_ref()));
        }

        let state = State {
            steps: steps.clone(),
            env: env.clone(),
        };
        if let Err(e) = self.save_state(&wt.path, &state) {
            let started = step(wt, StepKind::Recipe, STATE_FILE, Instant::now());
            steps.push(started(Outcome::Failed, format!("{e:#}")));
        }
        Report { steps, env, saved }
    }

    /// Drops the database copy of the worktree `id` names and gives its ports back, even once the worktree is
    /// gone. Its files go with the worktree.
    pub fn teardown(&self, path: &Path, id: &str) -> Report {
        let mut steps = Vec::new();
        let repo = self
            .state(path)
            .and_then(|s| s.steps.first().map(|s| s.repo.clone()))
            .unwrap_or_default();
        if let Some(made) = self.made(id) {
            let started = Instant::now();
            let (outcome, detail) = match database::drop(&self.env, &made) {
                Ok(()) => match self.set_made(id, None) {
                    Ok(()) => (Outcome::Dropped, String::new()),
                    Err(e) => (Outcome::Failed, format!("{e:#}")),
                },
                Err(e) => (Outcome::Failed, format!("{e:#}")),
            };
            steps.push(Step {
                repo: repo.clone(),
                kind: StepKind::Database,
                target: made.name.clone(),
                outcome,
                detail,
                ms: elapsed(started),
            });
        }
        let started = Instant::now();
        if let Ok(Some(given)) = ports::release(&self.data_dir, path) {
            let freed: Vec<String> = given.values().map(u16::to_string).collect();
            steps.push(Step {
                repo,
                kind: StepKind::Env,
                target: freed.join(" "),
                outcome: Outcome::Released,
                detail: String::new(),
                ms: elapsed(started),
            });
        }
        Report {
            steps,
            env: BTreeMap::new(),
            saved: false,
        }
    }

    /// The steps of the last setup of the worktree.
    pub fn last(&self, path: &Path) -> Vec<Step> {
        self.state(path).map(|s| s.steps).unwrap_or_default()
    }

    /// What the last setup of the worktree gave its sessions, for a session that comes back to it.
    pub fn env(&self, path: &Path) -> BTreeMap<String, String> {
        self.state(path).map(|s| s.env).unwrap_or_default()
    }

    fn dep(&self, wt: &Worktree, dep: &Dep) -> Step {
        let target = match dep.dir.as_str() {
            "" => dep
                .install
                .first()
                .and_then(|c| c.split_whitespace().next())
                .unwrap_or("dependencies")
                .to_owned(),
            dir => dir.to_owned(),
        };
        let started = step(wt, StepKind::Deps, &target, Instant::now());
        if !dep.dir.is_empty() && files::relative(&dep.dir).is_none() {
            return started(
                Outcome::Failed,
                format!("{} is not a folder of the repository", dep.dir),
            );
        }
        // Installed files git does not ignore would end up in the run's pull request.
        // The trailing slash lets a `dir/` pattern match a folder that does not exist yet.
        let dir_ignored = |root: &Path| {
            detect::ignored(root, &format!("{}/", dep.dir), &self.env)
                || detect::ignored(root, &dep.dir, &self.env)
        };
        if !dep.dir.is_empty() && !dir_ignored(&wt.path) {
            let why = format!(
                "git does not ignore {}: its files would end up in the run's pull request; ignore it, or remove \
                 this dependency from the recipe",
                dep.dir
            );
            return started(Outcome::Skipped, why);
        }
        let dest = wt.path.join(&dep.dir);
        let mut cloned = false;
        if !dep.dir.is_empty() && dep.clone && !dest.exists() {
            let src = wt.source.join(&dep.dir);
            // A folder git does not ignore would end up in the run's delivery.
            cloned = src.is_dir() && dir_ignored(&wt.source) && cow::clone(&src, &dest).is_ok();
        }
        if dep.install.is_empty() {
            return match (cloned, !dep.dir.is_empty() && dest.exists()) {
                (true, _) => started(Outcome::Cloned, String::new()),
                (false, true) => started(Outcome::Present, String::new()),
                (false, false) => started(
                    Outcome::Skipped,
                    "nothing to clone and no install command: left to the agent".into(),
                ),
            };
        }
        let mut errors = Vec::new();
        let before = self.changes(&wt.path);
        for line in &dep.install {
            let mut cmd = shell(line);
            cmd.current_dir(&wt.path);
            match self.execute(cmd, COMMAND_TIMEOUT) {
                Ok(_) => {
                    if let Some(left) = self.left(&wt.path, &before) {
                        return started(Outcome::Failed, format!("{line}: {left}"));
                    }
                    return match cloned {
                        true => started(Outcome::Cloned, format!("then {line}")),
                        false => started(Outcome::Installed, line.clone()),
                    };
                }
                Err(e) => errors.push(format!("{line}: {e:#}")),
            }
        }
        let detail = errors.join("\n");
        match cloned {
            true => started(Outcome::Failed, format!("cloned, then {detail}")),
            false => started(Outcome::Failed, detail),
        }
    }

    /// The setup agent: Claude Code headless in the worktree, with the project's `worktree-setup` skill and
    /// role, for what the steps could not do.
    fn agent(
        &self,
        wt: &Worktree,
        steps: &[Step],
        env: &BTreeMap<String, String>,
        access: Option<&database::Access>,
    ) -> Step {
        let started = step(wt, StepKind::Agent, "worktree-setup", Instant::now());
        match self.ask_agent(wt, steps, env, access) {
            Ok(detail) => started(Outcome::Ran, detail),
            Err(e) => started(Outcome::Failed, format!("{e:#}")),
        }
    }

    fn ask_agent(
        &self,
        wt: &Worktree,
        steps: &[Step],
        env: &BTreeMap<String, String>,
        access: Option<&database::Access>,
    ) -> Result<String> {
        let library = termf_library::library_dir(&self.config_dir, Some(&wt.project))?;
        let skill = library.join("skills").join(recipe::NAME);
        let role = library.join("roles").join(format!("{}.md", recipe::NAME));
        ensure!(
            skill.join("SKILL.md").is_file() && role.is_file(),
            "the project has no worktree-setup skill and role yet: save its recipe"
        );
        let scratch = std::env::temp_dir().join(format!(
            "termf-setup-agent-{}-{}",
            std::process::id(),
            hash(&wt.path.to_string_lossy())
        ));
        let _ = fs::remove_dir_all(&scratch);
        let plugin = scratch.join("plugin");
        files::copy_tree(&skill, &plugin.join("skills").join(recipe::NAME), None)?;
        fs::create_dir_all(plugin.join(".claude-plugin"))?;
        fs::write(
            plugin.join(".claude-plugin/plugin.json"),
            r#"{ "name": "termf-setup" }"#,
        )?;
        let role_text = fs::read_to_string(&role)?;
        let system = scratch.join("role.md");
        fs::write(&system, role_body(&role_text))?;

        let done: Vec<String> = steps
            .iter()
            .map(|s| format!("- {:?}: {}", s.kind, s.describe()).to_lowercase())
            .collect();
        let prompt = format!(
            "Prepare this git worktree of the repository {} (project {}), following the worktree-setup skill. \
             TermF already ran the recipe's steps:\n{}\n\nDo what is left: what failed, then migrations, seeds or \
             generated files the project needs. Stay in this worktree, never push, and touch no database but this \
             worktree's copy. End with a short summary of what you did.",
            wt.repo,
            wt.project,
            done.join("\n")
        );
        let (program, lead) = self
            .claude
            .split_first()
            .context("no Claude Code command")?;
        let claude = termf_core::claude::binary(program)?;
        let mut cmd = command(&claude.path);
        cmd.args(lead)
            .args(["-p", "--output-format", "json"])
            .arg("--plugin-dir")
            .arg(&plugin)
            .arg("--append-system-prompt-file")
            .arg(&system)
            .args([
                "--restricted",
                "--strict-mcp-config",
                "--no-session-persistence",
            ])
            .args(["--tools", "Bash,Read,Edit,Write,Glob,Grep"])
            .args(["--allowedTools", "Bash,Read,Edit,Write,Glob,Grep"])
            .args(["--disallowedTools", "Bash(git push:*)"])
            .args(["--permission-mode", "dontAsk"])
            .arg("--settings")
            .arg(sandbox(access).to_string())
            .current_dir(&wt.path)
            .envs(env);
        prepare(&mut cmd, &self.env);
        let result = termf_core::process::run(cmd, prompt.as_bytes(), AGENT_TIMEOUT);
        let _ = fs::remove_dir_all(&scratch);
        let (status, out, stderr) =
            result.with_context(|| format!("cannot run Claude Code {claude}"))?;
        let text = String::from_utf8_lossy(&out);
        let answer = text
            .lines()
            .rev()
            .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok());
        let Some(answer) = answer.filter(|a| status.success() || a["is_error"] == true) else {
            bail!("{}", termf_core::claude::failure(&claude, status, &stderr));
        };
        let summary = answer["result"].as_str().unwrap_or_default().trim();
        ensure!(answer["is_error"] != true, "Claude Code: {summary}");
        let mut detail = tail(summary, 600);
        if let Some(cost) = answer["total_cost_usd"].as_f64() {
            detail = format!("{detail} (${cost:.2})");
        }
        Ok(detail)
    }

    /// Runs `cmd` (stdin empty) with the setup's environment and the login shell's PATH; `Err` holds the end
    /// of its output.
    fn execute(&self, cmd: Command, timeout: Duration) -> Result<Vec<u8>> {
        self.execute_with(cmd, b"", timeout)
    }

    fn execute_with(&self, mut cmd: Command, input: &[u8], timeout: Duration) -> Result<Vec<u8>> {
        prepare(&mut cmd, &self.env);
        let (status, stdout, stderr) = termf_core::process::run(cmd, input, timeout)?;
        if status.success() {
            return Ok(stdout);
        }
        let mut text = String::from_utf8_lossy(&stderr).trim().to_owned();
        if text.is_empty() {
            text = String::from_utf8_lossy(&stdout).trim().to_owned();
        }
        bail!("{status}: {}", tail(&text, 800))
    }

    /// What `git status` lists in the worktree: changes git would deliver.
    fn changes(&self, worktree: &Path) -> Vec<String> {
        git(
            &self.env,
            worktree,
            &["status", "--porcelain", "--untracked-files=all"],
        )
        .map(|out| out.lines().map(str::to_owned).collect())
        .unwrap_or_default()
    }

    /// What a command left in the worktree that git does not ignore, which the run's pull request would carry.
    fn left(&self, worktree: &Path, before: &[String]) -> Option<String> {
        let new: Vec<String> = self
            .changes(worktree)
            .into_iter()
            .filter(|c| !before.contains(c))
            .map(|c| c.get(3..).unwrap_or(&c).to_owned())
            .collect();
        (!new.is_empty()).then(|| {
            format!(
                "it left files git does not ignore, which the run's pull request would carry: {}",
                new.join(", ")
            )
        })
    }

    fn made(&self, id: &str) -> Option<database::Made> {
        self.databases().remove(id)
    }

    fn databases(&self) -> BTreeMap<String, database::Made> {
        fs::read_to_string(self.data_dir.join(DATABASES_FILE))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn set_made(&self, id: &str, made: Option<database::Made>) -> Result<()> {
        let mut all = self.databases();
        match made {
            Some(made) => all.insert(id.to_owned(), made),
            None => all.remove(id),
        };
        fs::create_dir_all(&self.data_dir)?;
        termf_core::write_atomic(
            &self.data_dir.join(DATABASES_FILE),
            &serde_json::to_string_pretty(&all)?,
        )
    }

    fn state(&self, worktree: &Path) -> Option<State> {
        let file = self.state_file(worktree).ok()?;
        serde_json::from_str(&fs::read_to_string(file).ok()?).ok()
    }

    fn save_state(&self, worktree: &Path, state: &State) -> Result<()> {
        let file = self.state_file(worktree)?;
        fs::write(&file, serde_json::to_string_pretty(state)?)
            .with_context(|| format!("cannot write {}", file.display()))
    }

    fn state_file(&self, worktree: &Path) -> Result<PathBuf> {
        let dir = git(&self.env, worktree, &["rev-parse", "--absolute-git-dir"])?;
        Ok(Path::new(&dir).join(STATE_FILE))
    }
}

const REGISTRIES: [&str; 15] = [
    "registry.npmjs.org",
    "registry.yarnpkg.com",
    "repo.yarnpkg.com",
    "packagist.org",
    "repo.packagist.org",
    "codeload.github.com",
    "objects.githubusercontent.com",
    "pypi.org",
    "files.pythonhosted.org",
    "rubygems.org",
    "index.rubygems.org",
    "proxy.golang.org",
    "sum.golang.org",
    "index.crates.io",
    "static.crates.io",
];

/// Package managers' caches under the home folder, which installs write to.
const CACHES: [&str; 14] = [
    ".npm",
    ".cache",
    ".composer",
    ".config/composer",
    ".pnpm-store",
    ".local/share/pnpm",
    ".yarn",
    ".bun",
    ".cargo",
    "go/pkg/mod",
    ".gem",
    ".bundle",
    "Library/Caches",
    "Library/pnpm",
];

/// Claude Code's sandbox for the setup agent (code.claude.com/docs/en/sandboxing): Bash writes in the worktree,
/// the temporary folder and the package managers' caches that exist, and reaches the package registries and the
/// worktree's local database.
// shortcut: on Linux a sandboxed command has a localhost of its own, so a database reached over TCP stays out of
// reach there (only `allowLocalBinding` on macOS opens it); excluding the migration command would be next.
fn sandbox(access: Option<&database::Access>) -> serde_json::Value {
    let home = termf_core::expand_tilde("~");
    let caches: Vec<String> = CACHES
        .iter()
        .filter(|c| home.join(c).is_dir())
        .map(|c| format!("~/{c}"))
        .collect();
    let mut network = serde_json::json!({ "allowedDomains": REGISTRIES });
    if let Some(access) = access {
        if access.local_tcp {
            network["allowLocalBinding"] = true.into();
        }
        if !access.sockets.is_empty() {
            network["allowUnixSockets"] = access.sockets.clone().into();
        }
    }
    serde_json::json!({
        "sandbox": {
            "enabled": true,
            "autoAllowBashIfSandboxed": true,
            "filesystem": { "allowWrite": caches },
            "network": network,
        }
    })
}

/// A role file's body, without its frontmatter.
fn role_body(text: &str) -> &str {
    text.strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(text, |(_, body)| body)
}

/// `termf_42`: Compose wants lowercase letters, digits, `-` and `_`.
pub fn compose_name(id: &str) -> String {
    let id: String = id
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect();
    format!("termf_{id}")
}

/// Returns a closure that finishes the step with its outcome, timed from `started`.
fn step(
    wt: &Worktree,
    kind: StepKind,
    target: &str,
    started: Instant,
) -> impl Fn(Outcome, String) -> Step {
    let (repo, target) = (wt.repo.clone(), target.to_owned());
    move |outcome, detail| Step {
        repo: repo.clone(),
        kind,
        target: target.clone(),
        outcome,
        detail,
        ms: elapsed(started),
    }
}

fn elapsed(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn tail(text: &str, max: usize) -> String {
    let start = text.char_indices().rev().nth(max).map_or(0, |(i, _)| i);
    text[start..].to_owned()
}

/// FNV-1a, to name things after a path.
fn hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn same(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The program of the first Claude Code agent in the settings, else `claude`.
fn claude_command(config_dir: &Path) -> Vec<String> {
    let settings = termf_core::load_settings(config_dir).unwrap_or_default();
    let program = settings
        .agents
        .values()
        .find(|c| termf_core::claude::is_claude(c))
        .and_then(|c| c.split_whitespace().next())
        .unwrap_or("claude");
    vec![program.into()]
}

fn git(env: &[(String, String)], dir: &Path, args: &[&str]) -> Result<String> {
    let out = command("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .output()
        .context("cannot run git: is it installed?")?;
    ensure!(
        out.status.success(),
        "git {} failed: {}",
        args[0],
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The login shell's PATH (an app started from the Finder lacks the one that finds `pnpm` or `psql`), then
/// `env`.
fn prepare(cmd: &mut Command, env: &[(String, String)]) {
    if let Some(path) = termf_core::shell::login_path() {
        cmd.env("PATH", path);
    }
    cmd.envs(env.iter().map(|(k, v)| (k, v)));
}

/// A shell command line: `sh -c`, or `cmd /C` on Windows.
fn shell(line: &str) -> Command {
    let mut cmd = command(if cfg!(windows) { "cmd" } else { "sh" });
    cmd.arg(if cfg!(windows) { "/C" } else { "-c" }).arg(line);
    cmd
}

/// A command that opens no console window on Windows.
fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
