//! Stack-aware worktree setup (ARCHITECTURE §9). Each project has a recipe in its library,
//! `recipes/worktree-setup.toml`, with a section per repository: the files git ignores that a worktree needs,
//! the dependency folders (cloned copy-on-write, else installed from the package manager's cache, never copied
//! raw), the local database to copy per worktree, Docker Compose and its ports, and commands. Saving it also
//! writes the `worktree-setup` skill, agent (role) and command of the project's library. Without a saved
//! recipe, what detection finds applies.
//!
//! What a setup made (its database copy, its ports) is recorded in the worktree's own git folder, so tearing it
//! down needs nothing else and goes away with the worktree.

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
pub use recipe::{Draft, RecipeView, RepoView, discard, propose, save, view};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const AGENT_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const STATE_FILE: &str = "termf-setup.json";

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

/// What a setup made, in `<worktree git dir>/termf-setup.json`.
#[derive(Serialize, Deserialize, Default)]
struct State {
    database: Option<database::Made>,
    steps: Vec<Step>,
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
        let mut state = self.state(&wt.path).unwrap_or_default();
        let mut env = BTreeMap::new();

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
            steps.push(self.dep(wt, dep));
        }
        if let Some(from) = &repo.database {
            let started = step(wt, StepKind::Database, from, Instant::now());
            let compose = detect::compose(&wt.source)
                .map(|c| c.services)
                .unwrap_or_default();
            let outcome = database::copy(&self.env, wt, from, &compose, state.database.as_ref());
            steps.push(match outcome {
                Ok(done) => {
                    env.extend(done.env);
                    if done.made.is_some() {
                        state.database = done.made;
                    }
                    let mut step = started(done.outcome, done.detail);
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
            let mut cmd = shell(line);
            cmd.current_dir(&wt.path).envs(&env);
            steps.push(match self.execute(cmd, COMMAND_TIMEOUT) {
                Ok(_) => started(Outcome::Ran, String::new()),
                Err(e) => started(Outcome::Failed, format!("{e:#}")),
            });
        }
        if recipe.agent {
            steps.push(self.agent(wt, &steps, &env));
        }

        state.steps = steps.clone();
        if let Err(e) = self.save_state(&wt.path, &state) {
            let started = step(wt, StepKind::Recipe, STATE_FILE, Instant::now());
            steps.push(started(Outcome::Failed, format!("{e:#}")));
        }
        Report { steps, env, saved }
    }

    /// Drops the worktree's database copy and gives its ports back. Its files go with the worktree.
    pub fn teardown(&self, path: &Path) -> Report {
        let mut steps = Vec::new();
        let mut state = self.state(path).unwrap_or_default();
        let repo = state
            .steps
            .first()
            .map(|s| s.repo.clone())
            .unwrap_or_default();
        if let Some(made) = state.database.take() {
            let started = Instant::now();
            let (outcome, detail) = match database::drop(&self.env, &made) {
                Ok(()) => (Outcome::Dropped, String::new()),
                Err(e) => {
                    state.database = Some(made.clone());
                    (Outcome::Failed, format!("{e:#}"))
                }
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
        let _ = self.save_state(path, &state);
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
        let dest = wt.path.join(&dep.dir);
        let mut cloned = false;
        if !dep.dir.is_empty() && dep.clone && !dest.exists() {
            let src = wt.source.join(&dep.dir);
            // A folder git does not ignore would end up in the run's delivery.
            cloned = src.is_dir()
                && detect::ignored(&wt.source, &dep.dir, &self.env)
                && cow::clone(&src, &dest).is_ok();
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
        for line in &dep.install {
            let mut cmd = shell(line);
            cmd.current_dir(&wt.path);
            match self.execute(cmd, COMMAND_TIMEOUT) {
                Ok(_) if cloned => return started(Outcome::Cloned, format!("then {line}")),
                Ok(_) => return started(Outcome::Installed, line.clone()),
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
    fn agent(&self, wt: &Worktree, steps: &[Step], env: &BTreeMap<String, String>) -> Step {
        let started = step(wt, StepKind::Agent, "worktree-setup", Instant::now());
        match self.ask_agent(wt, steps, env) {
            Ok(detail) => started(Outcome::Ran, detail),
            Err(e) => started(Outcome::Failed, format!("{e:#}")),
        }
    }

    fn ask_agent(
        &self,
        wt: &Worktree,
        steps: &[Step],
        env: &BTreeMap<String, String>,
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
        files::copy_tree(&skill, &plugin.join("skills").join(recipe::NAME))?;
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
        let mut cmd = command(program);
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
            .arg(SANDBOX.trim())
            .current_dir(&wt.path)
            .envs(env);
        let result = self.execute_with(cmd, prompt.as_bytes(), AGENT_TIMEOUT);
        let _ = fs::remove_dir_all(&scratch);
        let out = result?;
        let text = String::from_utf8_lossy(&out);
        let answer = text
            .lines()
            .rev()
            .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
            .context("Claude Code gave no answer")?;
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

/// Lets Bash write in the worktree only, and reach the package registries.
const SANDBOX: &str = r#"
{"sandbox":{"enabled":true,"autoAllowBashIfSandboxed":true,"network":{"allowedDomains":[
"registry.npmjs.org","registry.yarnpkg.com","repo.yarnpkg.com","packagist.org","repo.packagist.org",
"codeload.github.com","objects.githubusercontent.com","pypi.org","files.pythonhosted.org","rubygems.org",
"index.rubygems.org","proxy.golang.org","sum.golang.org","index.crates.io","static.crates.io"]}}}
"#;

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
