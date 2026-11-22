//! Scheduled tasks: a prompt for Claude Code, run on a cron schedule (local time) in a folder, headless
//! (`claude -p`, its answer saved) or as an interactive session in a TermF tile. TermF runs them while it is open;
//! an entry in the system's scheduler runs `termf schedule run <id> --scheduled` while it is closed. Each run is
//! claimed in SQLite first, so a task never runs twice for the same time.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use termf_core::Settings;
use toml_edit::{DocumentMut, Item, Table};

pub mod cron;
pub mod os;
mod run;
mod store;
#[cfg(test)]
mod tests;

pub use cron::Cron;
pub use run::{TIMEOUT, headless_args, headless_command, tile_args};
pub use store::{Execution, Host, Outcome, Status, Trigger};

use store::Store;

/// In the config folder, so it syncs and exports with the rest.
pub const FILE: &str = "schedules.toml";
/// A run this late is still on time; later, it is a missed run, caught up only if the task says so.
pub const GRACE: i64 = 5 * 60;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// `claude -p`, its answer saved with the run.
    #[default]
    Headless,
    /// An interactive session in a tile; needs TermF open.
    Tile,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Task {
    pub name: String,
    pub prompt: String,
    /// The folder it runs in: absolute here, `~/…` in the file.
    pub dir: String,
    /// Five cron fields, in local time.
    pub cron: String,
    #[serde(default)]
    pub mode: Mode,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// Claude Code's `--permission-mode`.
    pub permissions: Option<String>,
    /// The most a headless run may spend, in US dollars (`--max-budget-usd`).
    pub budget: Option<f64>,
    /// The machine it runs on (its sync device name, else its host name): the file syncs, the task does not run
    /// everywhere. `None`: on every machine where TermF is open.
    pub machine: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Runs once, at the first chance, when its time passed while TermF and the computer were off or asleep.
    #[serde(default = "yes")]
    pub catchup: bool,
}

#[derive(Deserialize, Default)]
struct TasksFile {
    #[serde(default)]
    tasks: BTreeMap<String, Task>,
}

/// A task of Claude Desktop (`~/.claude/scheduled-tasks/<name>/SKILL.md`), read only: its schedule lives in
/// the Desktop app.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopTask {
    pub name: String,
    pub description: Option<String>,
    pub prompt: String,
    pub path: String,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The scheduled time to run now, if any: the last one since `since`, while still on time, or later when
/// `catchup` allows. Older missed times are never run: at most one catch-up.
pub fn due(
    cron: &Cron,
    tz: &TimeZone,
    since: i64,
    now: i64,
    catchup: bool,
) -> Option<(i64, Trigger)> {
    let last = cron
        .last_until(Timestamp::from_second(now).ok()?, tz)?
        .as_second();
    if last <= since {
        None
    } else if now - last <= GRACE {
        Some((last, Trigger::Schedule))
    } else {
        catchup.then_some((last, Trigger::Catchup))
    }
}

/// The next `count` runs after `now`, in seconds since the Unix epoch, in the computer's time zone.
pub fn upcoming(expr: &str, now: i64, count: usize) -> Result<Vec<i64>> {
    let cron = Cron::parse(expr)?;
    let runs = cron.upcoming(Timestamp::from_second(now)?, &TimeZone::system(), count);
    ensure!(!runs.is_empty(), "this schedule never comes");
    Ok(runs.iter().map(|t| t.as_second()).collect())
}

/// `2026-11-23 09:00`, in the computer's time zone.
pub fn local(secs: i64) -> String {
    Timestamp::from_second(secs)
        .map(|t| {
            t.to_zoned_datetime(TimeZone::system())
                .strftime("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// Claude Code's own folder, which `$CLAUDE_CONFIG_DIR` moves.
pub fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| termf_core::expand_tilde("~/.claude"))
}

/// The first `[agents]` entry that runs Claude Code, for sessions in a tile.
pub fn claude_agent(settings: &Settings) -> Result<String> {
    settings
        .agents
        .iter()
        .find(|(_, command)| termf_core::claude::is_claude(command))
        .map(|(name, _)| name.clone())
        .context("no agent runs Claude Code in termf.toml [agents]")
}

/// That agent's command, else `claude`, for headless runs (see [`headless_command`]).
pub fn claude_command(settings: &Settings) -> String {
    settings
        .agents
        .values()
        .find(|c| termf_core::claude::is_claude(c))
        .cloned()
        .unwrap_or_else(|| "claude".into())
}

/// `~` expanded, separators as this system writes them: `~/a/b` read back on Windows gets its own.
fn folder(dir: &str) -> PathBuf {
    termf_core::expand_tilde(dir.trim()).components().collect()
}

/// Ids become file and service names.
fn check_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 64
            && !id.starts_with('-')
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "invalid task id {id:?}"
    );
    Ok(())
}

fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = slug.chars().take(48).collect::<String>();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "task".into()
    } else {
        slug.into()
    }
}

/// Values given to Claude Code's options: a word that cannot pass for another option.
fn check_word(what: &str, value: &Option<String>) -> Result<()> {
    if let Some(v) = value {
        ensure!(
            !v.starts_with('-')
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._:-[]".contains(c)),
            "invalid {what} {v:?}"
        );
    }
    Ok(())
}

/// Trims the task and refuses what cannot run. The folder is only checked when the task runs here and is new
/// here (new task, new folder, moved here): switching off a task synced from another machine needs no folder.
fn checked(mut task: Task, previous: Option<&Task>, machine: &str) -> Result<Task> {
    task.name = task.name.trim().replace(['\n', '\r'], " ");
    ensure!(!task.name.is_empty(), "a task needs a name");
    ensure!(!task.prompt.trim().is_empty(), "a task needs a prompt");
    task.cron = task.cron.split_whitespace().collect::<Vec<_>>().join(" ");
    upcoming(&task.cron, now(), 1)?;
    for value in [
        &mut task.model,
        &mut task.effort,
        &mut task.permissions,
        &mut task.machine,
    ] {
        *value = value
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(Into::into);
    }
    check_word("model", &task.model)?;
    check_word("effort", &task.effort)?;
    check_word("permission mode", &task.permissions)?;
    let dir = folder(&task.dir);
    let mine = |t: &Task| t.machine.as_deref().is_none_or(|m| m == machine);
    if mine(&task) && previous.is_none_or(|p| folder(&p.dir) != dir || !mine(p)) {
        ensure!(dir.is_dir(), "the folder {} does not exist", dir.display());
    }
    task.dir = dir.to_string_lossy().into_owned();
    if let Some(budget) = task.budget {
        ensure!(
            budget.is_finite() && budget > 0.0,
            "a budget is more than 0"
        );
    }
    Ok(task)
}

pub struct Scheduler {
    config_dir: PathBuf,
    data_dir: PathBuf,
    /// `None` on a system TermF cannot schedule on.
    pub system: Option<os::System>,
    /// This machine's name, for the tasks that run on one machine only.
    pub machine: String,
}

impl Scheduler {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        let system = os::System::current(&data_dir);
        Self {
            machine: termf_sync::device_name(&config_dir),
            config_dir,
            data_dir,
            system,
        }
    }

    /// Whether the task runs on this machine.
    pub fn runs_here(&self, task: &Task) -> bool {
        task.machine.as_deref().is_none_or(|m| m == self.machine)
    }

    fn here(&self, task: &Task) -> Result<()> {
        match &task.machine {
            Some(machine) if !self.runs_here(task) => bail!("this task runs on {machine}"),
            _ => Ok(()),
        }
    }

    fn store(&self) -> Result<Store> {
        Store::open(&self.data_dir)
    }

    fn file(&self) -> PathBuf {
        self.config_dir.join(FILE)
    }

    /// Every task, by id, folders made absolute.
    pub fn tasks(&self) -> Result<BTreeMap<String, Task>> {
        let file = self.file();
        let Ok(text) = fs::read_to_string(&file) else {
            return Ok(BTreeMap::new());
        };
        let mut tasks = toml::from_str::<TasksFile>(&text)
            .with_context(|| format!("invalid {}", file.display()))?
            .tasks;
        tasks.retain(|id, _| check_id(id).is_ok());
        for task in tasks.values_mut() {
            task.dir = folder(&task.dir).to_string_lossy().into_owned();
        }
        Ok(tasks)
    }

    pub fn task(&self, id: &str) -> Result<Task> {
        self.tasks()?
            .remove(id)
            .with_context(|| format!("no scheduled task {id}"))
    }

    /// Creates a task (`id: None`) or replaces one, and returns its id. Its entry in the system's scheduler, if
    /// any, follows: rewritten for the new schedule, removed when it now runs in a tile.
    pub fn save(&self, id: Option<&str>, task: Task, termf: Option<&Path>) -> Result<String> {
        let previous = id.and_then(|id| self.task(id).ok());
        let mut task = checked(task, previous.as_ref(), &self.machine)?;
        let here = self.runs_here(&task);
        let file = self.file();
        let mut doc: DocumentMut = fs::read_to_string(&file)
            .unwrap_or_default()
            .parse()
            .with_context(|| format!("invalid {}", file.display()))?;
        let tasks = doc
            .entry("tasks")
            .or_insert_with(|| {
                let mut t = Table::new();
                t.set_implicit(true);
                Item::Table(t)
            })
            .as_table_mut()
            .with_context(|| format!("`tasks` in {} must be a table", file.display()))?;
        let id = match id {
            Some(id) => {
                check_id(id)?;
                ensure!(tasks.contains_key(id), "no scheduled task {id}");
                id.to_owned()
            }
            None => {
                let base = slugify(&task.name);
                (1..)
                    .map(|n| match n {
                        1 => base.clone(),
                        n => format!("{base}-{n}"),
                    })
                    .find(|id| !tasks.contains_key(id))
                    .unwrap()
            }
        };
        let absolute = std::mem::take(&mut task.dir);
        task.dir = termf_core::portable(&absolute);
        fs::create_dir_all(&self.config_dir)?;
        let table: DocumentMut = toml::to_string(&task)?.parse()?;
        tasks[id.as_str()] = Item::Table(table.as_table().clone());
        termf_core::write_atomic(&file, &doc.to_string())?;
        task.dir = absolute;
        self.store()?.track(&id, &task, now())?;
        if self.installed(&id) {
            match (task.mode, termf) {
                (Mode::Headless, Some(termf)) if here => self.install(&id, termf)?,
                (Mode::Headless, None) if here => {}
                _ => self.uninstall(&id)?,
            }
        }
        Ok(id)
    }

    /// Also removes its entry in the system's scheduler; its history stays.
    pub fn delete(&self, id: &str) -> Result<()> {
        check_id(id)?;
        self.uninstall(id)?;
        let file = self.file();
        let mut doc: DocumentMut = fs::read_to_string(&file)
            .with_context(|| format!("no scheduled task {id}"))?
            .parse()
            .with_context(|| format!("invalid {}", file.display()))?;
        let removed = doc
            .get_mut("tasks")
            .and_then(Item::as_table_mut)
            .and_then(|t| t.remove(id));
        ensure!(removed.is_some(), "no scheduled task {id}");
        termf_core::write_atomic(&file, &doc.to_string())?;
        self.store()?.forget(id)
    }

    /// The time the task is due for now, if any (see [`due`]).
    pub fn due(&self, id: &str, task: &Task, now: i64) -> Result<Option<(i64, Trigger)>> {
        if !self.runs_here(task) {
            return Ok(None);
        }
        let Some(since) = self.store()?.track(id, task, now)? else {
            return Ok(None);
        };
        Ok(due(
            &Cron::parse(&task.cron)?,
            &TimeZone::system(),
            since,
            now,
            task.catchup,
        ))
    }

    /// Records the run before it starts; `None` when that scheduled time already has one. Refused for a task
    /// of another machine.
    pub fn claim(
        &self,
        id: &str,
        task: &Task,
        trigger: Trigger,
        slot: Option<i64>,
        host: Host,
    ) -> Result<Option<Execution>> {
        self.here(task)?;
        self.store()?.claim(id, task, trigger, slot, host, now())
    }

    pub fn finish(&self, run: i64, outcome: &Outcome) -> Result<Execution> {
        self.store()?.finish(run, outcome, now())
    }

    /// Runs a claimed headless run to its end with `agent`, the command of a Claude Code agent.
    pub fn run_headless(&self, run: &Execution, task: &Task, agent: &str) -> Result<Execution> {
        let store = self.store()?;
        let outcome = match termf_core::claude::in_command(agent, termf_term::shell_quote) {
            Ok((agent, claude)) => {
                let mut outcome =
                    run::headless(headless_command(&agent, task), task, TIMEOUT, |pid| {
                        if let Err(e) = store.set_pid(run.id, pid) {
                            eprintln!("termf: scheduled run {}: {e:#}", run.id);
                        }
                    });
                if let Some(error) = &mut outcome.error {
                    *error = format!("Claude Code {claude}: {error}");
                }
                outcome
            }
            Err(e) => run::failed(format!("{e:#}")),
        };
        self.finish(run.id, &outcome)
    }

    /// Fails the runs left running whose process is gone, and those running for longer than a run may.
    pub fn interrupted(&self) -> Result<()> {
        let now = now();
        let longest = TIMEOUT.as_secs() as i64 + 600;
        self.store()?.interrupted(now - longest, now)
    }

    /// Newest first; every task's when `task` is `None`.
    pub fn history(&self, task: Option<&str>, limit: usize) -> Result<Vec<Execution>> {
        self.store()?.history(task, limit)
    }

    pub fn run(&self, id: i64) -> Result<Execution> {
        self.store()?.get(id)
    }

    fn system(&self) -> Result<&os::System> {
        self.system
            .as_ref()
            .context("TermF cannot use this system's scheduler")
    }

    /// Whether the system's scheduler runs the task while TermF is closed.
    pub fn installed(&self, id: &str) -> bool {
        self.system.as_ref().is_some_and(|s| s.installed(id))
    }

    /// Has the system's scheduler run `termf` for the task while TermF is closed.
    pub fn install(&self, id: &str, termf: &Path) -> Result<()> {
        let task = self.task(id)?;
        if task.mode == Mode::Tile {
            bail!(
                "a task that opens a tile needs TermF open: only headless tasks run while it is closed"
            );
        }
        self.here(&task)?;
        let system = self.system()?;
        let files = system.files(
            id,
            &task.name,
            termf,
            &Cron::parse(&task.cron)?,
            task.catchup,
        );
        system.install(id, &files)
    }

    pub fn uninstall(&self, id: &str) -> Result<()> {
        match &self.system {
            Some(system) => system.uninstall(id),
            None => Ok(()),
        }
    }

    /// Rewrites the installed entries that no longer match their task (changed on another machine, TermF
    /// moved), and removes those of tasks deleted, moved to another machine or to a tile. Never fails: a stale
    /// entry still runs its task, or names one that no longer runs here.
    pub fn refresh_entries(&self, termf: &Path) {
        let (Ok(tasks), Some(system)) = (self.tasks(), &self.system) else {
            return;
        };
        for id in system.installed_ids() {
            let task = tasks
                .get(&id)
                .filter(|t| t.mode == Mode::Headless && self.runs_here(t));
            let result = match task.map(|t| (t, Cron::parse(&t.cron))) {
                Some((task, Ok(cron))) => {
                    let files = system.files(&id, &task.name, termf, &cron, task.catchup);
                    let same = files
                        .iter()
                        .all(|(p, b)| fs::read(p).ok().as_ref() == Some(b));
                    if same {
                        Ok(())
                    } else {
                        system.install(&id, &files)
                    }
                }
                _ => system.uninstall(&id),
            };
            if let Err(e) = result {
                eprintln!("termf: scheduled task {id}: {e:#}");
            }
        }
    }
}

/// Claude Desktop's scheduled tasks, by name.
pub fn desktop_tasks(claude_dir: &Path) -> Vec<DesktopTask> {
    let Ok(entries) = fs::read_dir(claude_dir.join("scheduled-tasks")) else {
        return Vec::new();
    };
    let mut tasks: Vec<DesktopTask> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path().join("SKILL.md");
            let text = fs::read_to_string(&path).ok()?;
            let (meta, body) = termf_library::parse_markdown(&text)
                .unwrap_or_else(|_| (Default::default(), text.as_str()));
            let field = |key: &str| {
                meta.get(key)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
            };
            Some(DesktopTask {
                name: field("name")
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned()),
                description: field("description"),
                prompt: body.trim().to_owned(),
                path: path.to_string_lossy().into_owned(),
            })
        })
        .collect();
    tasks.sort_by_key(|t| t.name.to_lowercase());
    tasks
}
