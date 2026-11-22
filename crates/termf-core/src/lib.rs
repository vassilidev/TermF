use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

pub mod claude;
pub mod context;
pub mod ipc;
pub mod process;
pub mod shell;
pub mod squads;

const DEFAULT_SETTINGS: &str = r#"# TermF — global settings
# Folders scanned for git repositories. Hidden, system, media and dependency folders are skipped.
# On macOS, Desktop, Documents and Downloads are only scanned when listed here (macOS asks permission for them).
roots = ["~"]
# How many folder levels below each root are searched.
scan_depth = 5
# Inside TermF, Ctrl+` shows or hides the quick terminal. To also reach it when TermF is in the
# background, set a system-wide shortcut (read at startup), for example:
# quick_terminal_shortcut = "Alt+Shift+Space"
# Command for "Open in external editor" (e.g. "code", "zed"). Unset: the system's default app.
# editor = "code"
# Keep the computer from sleeping, which suspends agents (a locked screen does not):
# "auto" while an agent works (plus 2 minutes), "always" or "off".
keep_awake = "auto"
# Also keep the screen on, which prevents the automatic lock. Off by default: it costs energy,
# and an unattended unlocked computer is a risk.
keep_display_on = false
# Claude Code sessions: once the context passes `context_reset_at` % of the model's window, at the end
# of a turn the agent writes a handoff, TermF clears the context (/clear) and the agent resumes from
# the handoff, with no action from you. The gauge on each session warns from `context_warning_at` %.
context_reset = true
context_reset_at = 70
context_warning_at = 50

# Agents available in the UI: name = command run in the repository.
# A command that starts with `claude` also reports its status (working, waiting…) to TermF.
[agents]
claude = "claude"
"#;

// Never entered while scanning; listing one of them as a root still scans it.
const SKIPPED_DIRS: &[&str] = &[
    "Library",
    "Applications",
    "AppData",
    "Movies",
    "Music",
    "Pictures",
    "Videos",
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    "snap",
];

// macOS shows a permission prompt the first time an app reads these, so a background scan must not.
const MACOS_PROTECTED_DIRS: &[&str] = &["Desktop", "Documents", "Downloads"];

// Caps the directories read so a huge home folder stays fast; the UI tells the user when it was hit.
const SCAN_BUDGET: usize = 50_000;

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct Settings {
    pub roots: Vec<String>,
    pub scan_depth: usize,
    /// System-wide quick terminal shortcut; empty means none.
    pub quick_terminal_shortcut: String,
    pub agents: BTreeMap<String, String>,
    pub editor: Option<String>,
    pub keep_awake: KeepAwake,
    pub keep_display_on: bool,
    pub context_reset: bool,
    pub context_reset_at: u8,
    pub context_warning_at: u8,
    pub tickets: TicketSettings,
    pub ci: CiSettings,
    pub artifacts: ArtifactSettings,
    /// `[keys.<system>]` (`macos`, `linux`, `windows`): shortcut id → combination, only those changed in
    /// the UI; "" is none. One table per system, since termf.toml is synced and ⌘ only exists on macOS.
    /// Anything else than tables of strings is ignored rather than making the whole file invalid.
    #[serde(deserialize_with = "key_tables")]
    pub keys: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(deserialize_with = "vault_settings")]
    pub vault: VaultSettings,
    pub memory: MemorySettings,
    pub api: ApiSettings,
}

/// `[api]` in `termf.toml`: the local API the `termf` CLI talks to.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct ApiSettings {
    /// Starting, delivering or closing a run, starting a team, letting a gate pass or allowing what an agent
    /// asks waits for the user's confirmation in the app when it comes through the local API: the CLI cannot tell
    /// the user from an agent that runs it. Read at startup, so that an agent that writes it changes nothing
    /// before the user starts TermF again.
    pub confirm_cli: bool,
}

impl Default for ApiSettings {
    fn default() -> Self {
        Self { confirm_cli: true }
    }
}

/// `[memory]` in `termf.toml`: the project memory the expert searches (crate `termf-memory`).
#[derive(Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct MemorySettings {
    pub enabled: bool,
    /// Sources left out of the index, by id (`claude`, `git`…).
    pub exclude: Vec<String>,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            exclude: Vec::new(),
        }
    }
}

/// `[vault]` in `termf.toml`.
#[derive(Debug, PartialEq)]
pub struct VaultSettings {
    /// The vault locks after this many minutes without use, 1 to 1440.
    pub auto_lock_minutes: u32,
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: 10,
        }
    }
}

/// A number out of range is brought into it; anything else is the default, rather than making the whole file
/// invalid (it may come from another machine through the sync).
fn vault_settings<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<VaultSettings, D::Error> {
    let value = toml::Value::deserialize(deserializer)?;
    let minutes = value
        .get("auto_lock_minutes")
        .and_then(toml::Value::as_integer)
        .map(|m| m.clamp(1, 24 * 60) as u32);
    Ok(VaultSettings {
        auto_lock_minutes: minutes.unwrap_or(VaultSettings::default().auto_lock_minutes),
    })
}

/// The id of the system-wide quick terminal shortcut in `[keys]`.
pub const QUICK_TERMINAL_GLOBAL: &str = "quick_terminal_global";
const SYSTEMS: [&str; 3] = ["macos", "linux", "windows"];

fn key_tables<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, BTreeMap<String, String>>, D::Error> {
    let toml::Value::Table(systems) = toml::Value::deserialize(deserializer)? else {
        return Ok(BTreeMap::new());
    };
    Ok(systems
        .into_iter()
        .filter_map(|(system, keys)| {
            let toml::Value::Table(keys) = keys else {
                return None;
            };
            let keys = keys
                .into_iter()
                .filter_map(|(id, combo)| Some((id, combo.as_str()?.to_owned())))
                .collect();
            Some((system, keys))
        })
        .collect())
}

/// `[tickets]` in `termf.toml`.
#[derive(Deserialize, Default, Debug)]
pub struct TicketSettings {
    /// The `[agents]` entry a ticket's session runs; without it, the one named `claude`.
    pub agent: Option<String>,
    #[serde(default)]
    pub cleanup: TicketCleanup,
    #[serde(default)]
    pub worktrees: TicketWorktrees,
}

/// Where a new run's worktree goes. A run keeps the path it was created with.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TicketWorktrees {
    /// `<repo>/.claude/worktrees/`, as Claude Code's own: a session there inherits the trust given to the
    /// repository instead of asking for a folder nobody approved.
    #[default]
    Repo,
    /// TermF's data folder: nothing in the repository, for tools that look up the folders or skip git's
    /// ignores and would see the worktrees, or the main checkout from them.
    Data,
}

/// What TermF does once the pull request of a delivered run is merged or closed.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum TicketCleanup {
    /// Closes the run once its pull request is merged, unless its worktree holds work that is not pushed or its
    /// session is at work; a pull request closed without a merge is only told, as `Ask` does.
    #[default]
    Auto,
    /// Tells the user, who closes it.
    Ask,
    /// Does not look.
    Off,
}

/// `[artifacts]` in `termf.toml`.
#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct ArtifactSettings {
    /// A hidden Claude session lists the account's claude.ai artifacts at startup and every so many hours;
    /// 0: only when asked.
    pub refresh_hours: u32,
    pub delete_without_confirmation: bool,
}

impl Default for ArtifactSettings {
    fn default() -> Self {
        Self {
            refresh_hours: 6,
            delete_without_confirmation: false,
        }
    }
}

impl Settings {
    /// The agent a ticket's session runs: `[tickets] agent`, else the `[agents]` entry named `claude`, and
    /// never a guess. It must run Claude Code, which takes the ticket's task as its first message.
    pub fn ticket_agent(&self) -> Result<String> {
        let name = self.tickets.agent.as_deref().unwrap_or("claude");
        let command = self.agents.get(name).with_context(|| {
            format!(
                "no agent {name} in termf.toml [agents]: add it, or name the agent of tickets with [tickets] agent = \"<name>\""
            )
        })?;
        ensure!(
            claude::is_claude(command),
            "the agent {name} does not run Claude Code, which ticket runs need"
        );
        Ok(name.to_owned())
    }

    /// The program of the first Claude Code agent, else `claude`, for headless runs (`claude -p`).
    pub fn claude_program(&self) -> String {
        let program = self
            .agents
            .values()
            .find(|c| claude::is_claude(c))
            .and_then(|c| c.split_whitespace().next());
        program.unwrap_or("claude").to_owned()
    }

    /// `[keys.<this system>]`, with the older `quick_terminal_shortcut` as the system-wide shortcut until
    /// `[keys]` holds it.
    pub fn key_overrides(&self) -> BTreeMap<String, String> {
        let mut keys = self
            .keys
            .get(std::env::consts::OS)
            .cloned()
            .unwrap_or_default();
        let legacy = self.quick_terminal_shortcut.trim();
        if !legacy.is_empty() {
            keys.entry(QUICK_TERMINAL_GLOBAL.into())
                .or_insert_with(|| legacy.into());
        }
        keys
    }

    /// The system-wide quick terminal shortcut; empty when none.
    pub fn global_shortcut(&self) -> String {
        self.key_overrides()
            .remove(QUICK_TERMINAL_GLOBAL)
            .map(|s| s.trim().to_owned())
            .unwrap_or_default()
    }
}

// Must stay in sync with DEFAULT_SETTINGS (checked by a test); parsing it here would recurse through serde(default).
impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: vec!["~".into()],
            scan_depth: 5,
            quick_terminal_shortcut: String::new(),
            agents: BTreeMap::from([("claude".into(), "claude".into())]),
            editor: None,
            keep_awake: KeepAwake::Auto,
            keep_display_on: false,
            context_reset: true,
            context_reset_at: 70,
            context_warning_at: 50,
            tickets: TicketSettings::default(),
            ci: CiSettings::default(),
            artifacts: ArtifactSettings::default(),
            keys: BTreeMap::new(),
            vault: VaultSettings::default(),
            memory: MemorySettings::default(),
            api: ApiSettings::default(),
        }
    }
}

/// `[ci]` in `termf.toml`.
#[derive(Deserialize, Default, Debug)]
pub struct CiSettings {
    #[serde(default)]
    pub notify: CiNotify,
}

/// Which finished CI runs TermF notifies.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CiNotify {
    /// Runs the user triggered, or on the branch of one of their ticket runs.
    #[default]
    Mine,
    All,
    Off,
}

/// When TermF keeps the computer from sleeping.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum KeepAwake {
    /// While an agent works.
    #[default]
    Auto,
    Always,
    Off,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Repo {
    pub name: String,
    pub path: String,
    /// Last git activity, in seconds since the Unix epoch.
    pub modified: Option<u64>,
}

impl Repo {
    pub fn at(path: PathBuf, name: Option<String>) -> Self {
        Self {
            name: name.unwrap_or_else(|| file_name(&path)),
            modified: last_activity(&path),
            path: path.to_string_lossy().into_owned(),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub repos: Vec<Repo>,
}

#[derive(Deserialize)]
struct ProjectFile {
    name: String,
    #[serde(default)]
    repos: Vec<RepoEntry>,
}

#[derive(Deserialize)]
struct RepoEntry {
    path: String,
    name: Option<String>,
}

pub fn default_config_dir() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("no config directory on this system")?
        .join("termf"))
}

/// Runtime data (socket, token, session plugins), kept out of the config folder.
/// Same folder as Tauri's `app_data_dir` for the `dev.termf.app` identifier, so the CLI finds it without Tauri.
pub fn default_data_dir() -> Result<PathBuf> {
    Ok(dirs::data_dir()
        .context("no data directory on this system")?
        .join("dev.termf.app"))
}

/// Reads `termf.toml`, creating it with defaults on first launch so the user can find and edit it.
pub fn load_settings(config_dir: &Path) -> Result<Settings> {
    let file = config_dir.join("termf.toml");
    if !file.exists() {
        fs::create_dir_all(config_dir)?;
        fs::write(&file, DEFAULT_SETTINGS)?;
    }
    let text = fs::read_to_string(&file)?;
    toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))
}

/// Saves the keep-awake choice made in the UI into `termf.toml`, keeping the rest of the file and its comments.
pub fn save_keep_awake(config_dir: &Path, mode: KeepAwake, display_on: bool) -> Result<()> {
    let mode = match mode {
        KeepAwake::Auto => "auto",
        KeepAwake::Always => "always",
        KeepAwake::Off => "off",
    };
    edit_settings(config_dir, |doc| {
        doc["keep_awake"] = value(mode);
        doc["keep_display_on"] = value(display_on);
    })
}

/// Saves the context reset settings made in the UI into `termf.toml`, like `save_keep_awake`.
pub fn save_context_reset(config_dir: &Path, on: bool, reset_at: u8, warning_at: u8) -> Result<()> {
    ensure!(
        (1..=100).contains(&reset_at) && (1..=100).contains(&warning_at),
        "thresholds go from 1 to 100 %"
    );
    edit_settings(config_dir, |doc| {
        doc["context_reset"] = value(on);
        doc["context_reset_at"] = value(i64::from(reset_at));
        doc["context_warning_at"] = value(i64::from(warning_at));
    })
}

/// Saves the CI notification choice made in the UI into `termf.toml`, like `save_keep_awake`.
pub fn save_ci_notify(config_dir: &Path, notify: CiNotify) -> Result<()> {
    let notify = match notify {
        CiNotify::Mine => "mine",
        CiNotify::All => "all",
        CiNotify::Off => "off",
    };
    edit_settings(config_dir, |doc| {
        // A `[ci]` section at the end, not an inline table above the user's comments.
        doc.entry("ci").or_insert(toml_edit::table())["notify"] = value(notify);
    })
}

/// Saves how often the account's artifacts are listed (0: only when asked), like `save_keep_awake`.
pub fn save_artifacts_refresh(config_dir: &Path, hours: u32) -> Result<()> {
    edit_settings(config_dir, |doc| {
        doc.entry("artifacts").or_insert(toml_edit::table())["refresh_hours"] =
            value(i64::from(hours));
    })
}

pub fn save_artifacts_delete_without_confirmation(config_dir: &Path, on: bool) -> Result<()> {
    edit_settings(config_dir, |doc| {
        doc.entry("artifacts").or_insert(toml_edit::table())["delete_without_confirmation"] =
            value(on);
    })
}

pub fn save_vault_auto_lock(config_dir: &Path, minutes: u32) -> Result<()> {
    edit_settings(config_dir, |doc| {
        table_in(doc.as_table_mut(), "vault")
            .insert("auto_lock_minutes", value(i64::from(minutes)));
    })
}

pub fn save_memory(config_dir: &Path, settings: &MemorySettings) -> Result<()> {
    edit_settings(config_dir, |doc| {
        let table = table_in(doc.as_table_mut(), "memory");
        table.insert("enabled", value(settings.enabled));
        table.insert(
            "exclude",
            value(settings.exclude.iter().collect::<toml_edit::Array>()),
        );
    })
}

/// Saves this system's shortcuts changed in the UI as `[keys.<system>]`, like `save_keep_awake`: `keys` holds
/// only those that differ from the defaults; the other systems' tables are kept. `quick_terminal_shortcut`
/// moves into `[keys]`: the UI sends it for this system, and the other systems, which it held for too, get it.
pub fn save_keys(config_dir: &Path, keys: &BTreeMap<String, String>) -> Result<()> {
    let this = std::env::consts::OS;
    edit_settings(config_dir, |doc| {
        let legacy = doc
            .remove("quick_terminal_shortcut")
            .and_then(|item| Some(item.as_str()?.trim().to_owned()))
            .filter(|s| !s.is_empty());
        let root = table_in(doc.as_table_mut(), "keys");
        root.set_implicit(true);
        for system in SYSTEMS.iter().filter(|s| **s != this) {
            if let Some(legacy) = &legacy {
                table_in(root, system)
                    .entry(QUICK_TERMINAL_GLOBAL)
                    .or_insert(value(legacy));
            }
        }
        let own = table_in(root, this);
        own.retain(|id, _| keys.contains_key(id));
        for (id, combo) in keys {
            if own.get(id).and_then(Item::as_str) != Some(combo) {
                own.insert(id, value(combo));
            }
        }
        if own.is_empty() {
            root.remove(this);
        }
        if root.is_empty() {
            doc.remove("keys");
        }
    })
}

/// `parent[name]` as a table: an inline table becomes one with its keys, anything else is replaced.
pub fn table_in<'a>(parent: &'a mut Table, name: &str) -> &'a mut Table {
    let item = parent.entry(name).or_insert(toml_edit::table());
    if !item.is_table() {
        *item = match std::mem::take(item).into_table() {
            Ok(table) => Item::Table(table),
            Err(_) => toml_edit::table(),
        };
    }
    item.as_table_mut().expect("just made a table")
}

fn edit_settings(config_dir: &Path, edit: impl FnOnce(&mut DocumentMut)) -> Result<()> {
    load_settings(config_dir)?;
    let file = config_dir.join("termf.toml");
    let mut doc: DocumentMut = fs::read_to_string(&file)?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    edit(&mut doc);
    write_atomic(&file, &doc.to_string())
}

/// Reads every `projects/*.toml`; the file stem is the project id.
pub fn load_projects(config_dir: &Path) -> Result<Vec<Project>> {
    let dir = config_dir.join("projects");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut projects = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "toml") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let file: ProjectFile =
            toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
        let repos = file
            .repos
            .into_iter()
            .map(|r| Repo::at(expand_tilde(&r.path), r.name))
            .collect();
        projects.push(Project {
            id: path.file_stem().unwrap().to_string_lossy().into_owned(),
            name: file.name,
            repos,
        });
    }
    projects.sort_by_key(|p| p.name.to_lowercase());
    Ok(projects)
}

/// Creates a project (`id: None`) or rewrites one, and returns its id. Comments, unknown keys and the
/// entries of repositories that stay in the project are kept as they are.
pub fn save_project(
    config_dir: &Path,
    id: Option<&str>,
    name: &str,
    repos: &[String],
) -> Result<String> {
    let name = name.trim();
    ensure!(!name.is_empty(), "a project needs a name");
    let (id, file, mut doc) = match id {
        Some(id) => {
            let file = project_file(config_dir, id)?;
            let doc: DocumentMut = fs::read_to_string(&file)?
                .parse()
                .with_context(|| format!("invalid {}", file.display()))?;
            (id.to_owned(), file, doc)
        }
        None => {
            let dir = config_dir.join("projects");
            fs::create_dir_all(&dir)?;
            let base = slugify(name);
            let id = (1..)
                .map(|n| match n {
                    1 => base.clone(),
                    n => format!("{base}-{n}"),
                })
                .find(|id| !dir.join(format!("{id}.toml")).exists())
                .unwrap();
            (
                id.clone(),
                dir.join(format!("{id}.toml")),
                DocumentMut::new(),
            )
        }
    };
    fill(&mut doc, name, repos).with_context(|| format!("cannot write {}", file.display()))?;
    write_atomic(&file, &doc.to_string())?;
    Ok(id)
}

/// The file a new project starts with, as `save_project` would write it.
pub fn project_text(name: &str, repos: &[String]) -> Result<String> {
    let name = name.trim();
    ensure!(!name.is_empty(), "a project needs a name");
    let mut doc = DocumentMut::new();
    fill(&mut doc, name, repos)?;
    Ok(doc.to_string())
}

fn fill(doc: &mut DocumentMut, name: &str, repos: &[String]) -> Result<()> {
    doc["name"] = value(name);
    let list = doc
        .entry("repos")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .context("`repos` must be [[repos]] tables")?;
    let path_of = |t: &Table| t.get("path").and_then(Item::as_str).map(expand_tilde);
    list.retain(|t| path_of(t).is_some_and(|p| repos.iter().any(|r| p == Path::new(r))));
    let mut listed: Vec<PathBuf> = list.iter().filter_map(path_of).collect();
    for repo in repos {
        if !listed.iter().any(|p| p == Path::new(repo)) {
            // Only new entries are checked: hand-written relative paths already in the file stay as they are.
            ensure!(
                Path::new(repo).is_absolute(),
                "repository paths must be absolute"
            );
            let mut table = Table::new();
            table["path"] = value(portable(repo));
            list.push(table);
            listed.push(repo.into());
        }
    }
    Ok(())
}

/// Also removes `projects/<id>/` (its library), which a new project with the same id would otherwise inherit.
pub fn delete_project(config_dir: &Path, id: &str) -> Result<()> {
    fs::remove_file(project_file(config_dir, id)?)?;
    match fs::remove_dir_all(config_dir.join("projects").join(id)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

pub fn project_file(config_dir: &Path, id: &str) -> Result<PathBuf> {
    // The id comes from the UI and becomes a file name.
    ensure!(
        !id.is_empty() && !id.starts_with('.') && !id.contains(['/', '\\', ':']),
        "invalid project id {id:?}"
    );
    let file = config_dir.join("projects").join(format!("{id}.toml"));
    ensure!(file.is_file(), "unknown project {id}");
    Ok(file)
}

fn slugify(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    match slug.as_str() {
        "" => "project".into(),
        s if reserved_on_windows(s) => format!("{s}-project"),
        _ => slug,
    }
}

/// Device names Windows refuses as file names, whatever the extension.
fn reserved_on_windows(name: &str) -> bool {
    matches!(name, "con" | "prn" | "aux" | "nul")
        || ["com", "lpt"].iter().any(|device| {
            name.strip_prefix(device)
                .is_some_and(|n| n.chars().count() == 1 && "0123456789¹²³".contains(n))
        })
}

/// A saved set of sessions, reopened in one click.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Layout {
    pub name: String,
    pub sessions: Vec<LayoutSession>,
    /// The Terminals tabs over `sessions`; none in a layout saved before tabs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<LayoutTab>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutTab {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The tab's splits, written and checked by the app (`split-logic.ts`), as indexes into `sessions`.
    pub splits: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LayoutSession {
    pub path: String,
    /// `None` opens a shell.
    pub agent: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct LayoutsFile {
    #[serde(default)]
    layouts: Vec<Layout>,
}

/// Reads `layouts.toml`, in the order the layouts were first saved.
pub fn load_layouts(config_dir: &Path) -> Result<Vec<Layout>> {
    let file = config_dir.join("layouts.toml");
    if !file.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&file)?;
    let mut parsed: LayoutsFile =
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
    for session in parsed.layouts.iter_mut().flat_map(|l| &mut l.sessions) {
        session.path = expand_tilde(&session.path).to_string_lossy().into_owned();
    }
    Ok(parsed.layouts)
}

/// Adds `layout`, or replaces the layout with the same name.
pub fn save_layout(config_dir: &Path, mut layout: Layout) -> Result<()> {
    layout.name = layout.name.trim().into();
    ensure!(!layout.name.is_empty(), "a layout needs a name");
    ensure!(
        !layout.sessions.is_empty(),
        "a layout needs at least one session"
    );
    let mut layouts = load_layouts(config_dir)?;
    match layouts.iter_mut().find(|l| l.name == layout.name) {
        Some(existing) => *existing = layout,
        None => layouts.push(layout),
    }
    write_layouts(config_dir, layouts)
}

pub fn delete_layout(config_dir: &Path, name: &str) -> Result<()> {
    let mut layouts = load_layouts(config_dir)?;
    layouts.retain(|l| l.name != name);
    write_layouts(config_dir, layouts)
}

fn write_layouts(config_dir: &Path, mut layouts: Vec<Layout>) -> Result<()> {
    for session in layouts.iter_mut().flat_map(|l| &mut l.sessions) {
        session.path = portable(&session.path);
    }
    fs::create_dir_all(config_dir)?;
    let text = toml::to_string(&LayoutsFile { layouts })?;
    write_atomic(&config_dir.join("layouts.toml"), &text)
}

/// A line diff (`-` removed, `+` added, ` ` context) of the changed part; empty when nothing changes.
pub fn diff(before: &str, after: &str) -> String {
    const CONTEXT: usize = 3;
    let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (am, bm) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    if am.is_empty() && bm.is_empty() {
        return String::new();
    }
    // Longest common subsequence of the changed middle, which is small: one block.
    let mut lcs = vec![vec![0u32; bm.len() + 1]; am.len() + 1];
    for i in (0..am.len()).rev() {
        for j in (0..bm.len()).rev() {
            lcs[i][j] = if am[i] == bm[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut out: Vec<String> = a[prefix.saturating_sub(CONTEXT)..prefix]
        .iter()
        .map(|l| format!(" {l}"))
        .collect();
    let (mut i, mut j) = (0, 0);
    while i < am.len() || j < bm.len() {
        if i < am.len() && j < bm.len() && am[i] == bm[j] {
            out.push(format!(" {}", am[i]));
            (i, j) = (i + 1, j + 1);
        } else if i < am.len() && (j == bm.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(format!("-{}", am[i]));
            i += 1;
        } else {
            out.push(format!("+{}", bm[j]));
            j += 1;
        }
    }
    let tail = a.len() - suffix;
    out.extend(
        a[tail..(tail + CONTEXT).min(a.len())]
            .iter()
            .map(|l| format!(" {l}")),
    );
    out.join("\n") + "\n"
}

/// Writes through a temporary file so a crash never leaves a half-written config.
pub fn write_atomic(file: &Path, text: &str) -> Result<()> {
    let tmp = file.with_extension("toml.tmp");
    fs::write(&tmp, text)
        .and_then(|()| fs::rename(&tmp, file))
        .with_context(|| format!("cannot write {}", file.display()))
}

pub struct Discovery {
    pub repos: Vec<Repo>,
    /// The scan hit `SCAN_BUDGET` before finishing.
    pub truncated: bool,
}

/// Git repositories up to `max_depth` levels below each root. A repository is not searched further,
/// so nested worktrees and sub-repositories don't show up as separate entries.
/// Breadth-first, so when the budget runs out it is the deepest levels that are left out.
pub fn discover_repos(roots: &[String], max_depth: usize) -> Discovery {
    discover_with_budget(roots, max_depth, SCAN_BUDGET)
}

fn discover_with_budget(roots: &[String], max_depth: usize, budget: usize) -> Discovery {
    let mut found = Vec::new();
    let mut queue = VecDeque::new();
    for root in roots {
        let root = expand_tilde(root);
        if is_repo(&root) {
            found.push(root);
        } else {
            queue.push_back((root, max_depth));
        }
    }
    let mut read = 0;
    let mut truncated = false;
    while let Some((dir, depth_left)) = queue.pop_front() {
        if depth_left == 0 {
            continue;
        }
        if read == budget {
            truncated = true;
            break;
        }
        read += 1;
        for child in subdirs(&dir) {
            if is_repo(&child) {
                found.push(child);
            } else {
                queue.push_back((child, depth_left - 1));
            }
        }
    }
    found.sort();
    found.dedup();
    Discovery {
        repos: found.into_iter().map(|p| Repo::at(p, None)).collect(),
        truncated,
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        // file_type() does not follow symlinks, which also keeps the walk free of loops.
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.')
                && !SKIPPED_DIRS.contains(&name.as_ref())
                && !(cfg!(target_os = "macos") && MACOS_PROTECTED_DIRS.contains(&name.as_ref()))
        })
        .map(|e| e.path())
        .collect()
}

fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// `.git/index` is rewritten by commits, checkouts and the `git status` refreshes editors run,
/// so it tracks when a repository was last worked on. Worktrees (`.git` file) fall back to that file.
fn last_activity(repo: &Path) -> Option<u64> {
    let git = repo.join(".git");
    [git.join("index"), git.join("logs").join("HEAD"), git]
        .iter()
        .filter_map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
        .max()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

pub fn expand_tilde(path: &str) -> PathBuf {
    match (path.strip_prefix("~"), dirs::home_dir()) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            home.join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(path),
    }
}

/// Inverse of `expand_tilde`, so config files stay valid on another machine.
pub fn portable(path: &str) -> String {
    let home = dirs::home_dir();
    match home
        .as_deref()
        .and_then(|h| Path::new(path).strip_prefix(h).ok())
    {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        // Always `/`: the path is read back on other systems, and expand_tilde accepts it everywhere.
        Some(rest) => {
            let parts: Vec<_> = rest.iter().map(|p| p.to_string_lossy()).collect();
            format!("~/{}", parts.join("/"))
        }
        None => path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("termf-core-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_table_written_inline_keeps_its_keys() {
        let mut doc: DocumentMut = "keys = { macos = { find = \"Cmd+F\" } }\nname = 1\n"
            .parse()
            .unwrap();
        let keys = table_in(doc.as_table_mut(), "keys");
        let macos = table_in(keys, "macos");
        macos.insert("open", value("Cmd+O"));
        let text = doc.to_string();
        let back: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(
            back["keys"]["macos"]["find"].as_str(),
            Some("Cmd+F"),
            "{text}"
        );
        assert_eq!(back["keys"]["macos"]["open"].as_str(), Some("Cmd+O"));
        assert_eq!(back["name"].as_integer(), Some(1));
        let mut doc: DocumentMut = "keys = 3\n".parse().unwrap();
        assert!(table_in(doc.as_table_mut(), "keys").is_empty());
    }

    #[test]
    fn saves_keep_awake_and_keeps_comments() {
        let dir = temp_dir("keep-awake");
        fs::write(
            dir.join("termf.toml"),
            "# mine\nroots = [\"/src\"]\n\n[agents]\nclaude = \"claude\"\n",
        )
        .unwrap();
        save_keep_awake(&dir, KeepAwake::Always, true).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        assert!(text.starts_with("# mine\n"), "comments kept: {text}");
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.keep_awake, KeepAwake::Always);
        assert!(settings.keep_display_on);
        assert_eq!(settings.roots, ["/src"]);
        assert_eq!(settings.agents["claude"], "claude");

        save_keep_awake(&dir, KeepAwake::Off, false).unwrap();
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.keep_awake, KeepAwake::Off);
        assert!(!settings.keep_display_on);
    }

    #[test]
    fn picks_the_agent_of_tickets() {
        let settings = |text: &str| toml::from_str::<Settings>(text).unwrap();
        assert_eq!(settings("").ticket_agent().unwrap(), "claude");
        let chosen = "[agents]\nclaude = \"claude\"\nopus = \"claude --model opus\"\n[tickets]\nagent = \"opus\"\n";
        assert_eq!(settings(chosen).ticket_agent().unwrap(), "opus");
        assert_eq!(settings("").tickets.cleanup, TicketCleanup::Auto);
        let ask = settings("[tickets]\ncleanup = \"ask\"\n");
        assert_eq!(ask.tickets.cleanup, TicketCleanup::Ask);
        assert_eq!(settings("").tickets.worktrees, TicketWorktrees::Repo);
        let data = settings("[tickets]\nworktrees = \"data\"\n");
        assert_eq!(data.tickets.worktrees, TicketWorktrees::Data);
        for (text, error) in [
            (
                "[agents]\nsonnet = \"claude --model sonnet\"\n",
                "no agent claude",
            ),
            ("[agents]\nclaude = \"codex\"\n", "does not run Claude Code"),
            ("[tickets]\nagent = \"nope\"\n", "no agent nope"),
        ] {
            let found = settings(text).ticket_agent().unwrap_err().to_string();
            assert!(found.contains(error), "{text}: {found}");
        }
    }

    #[test]
    fn saves_memory_settings() {
        let dir = temp_dir("memory");
        fs::write(dir.join("termf.toml"), "# mine\nkeep_awake = \"off\"\n").unwrap();
        assert_eq!(
            load_settings(&dir).unwrap().memory,
            MemorySettings::default()
        );
        let off = MemorySettings {
            enabled: false,
            exclude: vec!["claude".into(), "git".into()],
        };
        save_memory(&dir, &off).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        assert!(text.starts_with("# mine\n"), "comments kept: {text}");
        assert_eq!(load_settings(&dir).unwrap().memory, off);
    }

    #[test]
    fn saves_context_reset_settings() {
        let dir = temp_dir("context-reset");
        fs::write(dir.join("termf.toml"), "# mine\nkeep_awake = \"off\"\n").unwrap();
        save_context_reset(&dir, false, 80, 60).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        assert!(text.starts_with("# mine\n"), "comments kept: {text}");
        let settings = load_settings(&dir).unwrap();
        assert!(!settings.context_reset);
        assert_eq!(
            (settings.context_reset_at, settings.context_warning_at),
            (80, 60)
        );
        assert_eq!(settings.keep_awake, KeepAwake::Off);
        assert!(save_context_reset(&dir, true, 0, 50).is_err());
        assert!(save_context_reset(&dir, true, 70, 101).is_err());
    }

    #[test]
    fn saves_ci_notifications() {
        let dir = temp_dir("ci-notify");
        fs::write(
            dir.join("termf.toml"),
            "# mine\n\n[agents]\nclaude = \"claude\"\n",
        )
        .unwrap();
        assert_eq!(load_settings(&dir).unwrap().ci.notify, CiNotify::Mine);
        save_ci_notify(&dir, CiNotify::Off).unwrap();
        save_ci_notify(&dir, CiNotify::All).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        assert!(text.starts_with("# mine\n"), "comments kept: {text}");
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.ci.notify, CiNotify::All);
        assert_eq!(settings.agents["claude"], "claude");
    }

    #[test]
    fn saves_the_artifacts_refresh() {
        let dir = temp_dir("artifacts-refresh");
        fs::write(
            dir.join("termf.toml"),
            "# mine\n[agents]\nclaude = \"claude\"\n",
        )
        .unwrap();
        assert_eq!(load_settings(&dir).unwrap().artifacts.refresh_hours, 6);
        save_artifacts_refresh(&dir, 0).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        assert!(text.starts_with("# mine\n"), "comments kept: {text}");
        assert_eq!(load_settings(&dir).unwrap().artifacts.refresh_hours, 0);
        assert!(
            !load_settings(&dir)
                .unwrap()
                .artifacts
                .delete_without_confirmation
        );
        save_artifacts_delete_without_confirmation(&dir, true).unwrap();
        let artifacts = load_settings(&dir).unwrap().artifacts;
        assert!(artifacts.delete_without_confirmation && artifacts.refresh_hours == 0);
    }

    #[test]
    fn saves_keys_and_keeps_the_rest() {
        let this = std::env::consts::OS;
        let other = if this == "macos" { "linux" } else { "macos" };
        let dir = temp_dir("keys");
        fs::write(
            dir.join("termf.toml"),
            format!(
                "# mine\nroots = [\"/src\"]\n# global\nquick_terminal_shortcut = \"Alt+Space\"\n\n[agents]\nclaude = \"claude\"\n\n[keys.{other}]\nmaximize = \"Ctrl+M\"\n"
            ),
        )
        .unwrap();
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.global_shortcut(), "Alt+Space");
        assert_eq!(
            settings.key_overrides(),
            BTreeMap::from([(QUICK_TERMINAL_GLOBAL.into(), "Alt+Space".into())])
        );

        let keys = BTreeMap::from([
            (QUICK_TERMINAL_GLOBAL.to_owned(), "Alt+Space".to_owned()),
            ("maximize".to_owned(), "Cmd+Shift+M".to_owned()),
            ("tile".to_owned(), String::new()),
        ]);
        save_keys(&dir, &keys).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        assert!(text.starts_with("# mine\nroots"), "comments kept: {text}");
        assert!(
            !text.contains("quick_terminal_shortcut"),
            "migrated: {text}"
        );
        assert!(!text.contains("[keys]\n"), "no empty [keys]: {text}");
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.keys[this], keys);
        assert_eq!(settings.key_overrides(), keys);
        assert_eq!(settings.global_shortcut(), "Alt+Space");
        assert_eq!(settings.roots, ["/src"]);
        assert_eq!(settings.agents["claude"], "claude");
        // Another system keeps its own shortcuts, and gets the older system-wide one it read too.
        assert_eq!(
            settings.keys[other],
            BTreeMap::from([
                ("maximize".into(), "Ctrl+M".into()),
                (QUICK_TERMINAL_GLOBAL.into(), "Alt+Space".into())
            ])
        );

        // Back to a default: only the overrides stay, and the user's own lines keep their place.
        let header = format!("[keys.{this}]\n");
        let text = text.replace(&header, &format!("{header}# by hand\n"));
        fs::write(dir.join("termf.toml"), text).unwrap();
        let keys = BTreeMap::from([("maximize".to_owned(), "Cmd+Shift+M".to_owned())]);
        save_keys(&dir, &keys).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        assert!(text.contains(&format!("{header}# by hand\n")), "{text}");
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.keys[this], keys);
        assert_eq!(settings.global_shortcut(), "");
        save_keys(&dir, &BTreeMap::new()).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        assert!(!text.contains(&header), "{text}");
        let settings = load_settings(&dir).unwrap();
        assert!(!settings.keys.contains_key(this));
        assert!(settings.keys.contains_key(other));
    }

    #[test]
    fn ignores_keys_that_are_not_combinations() {
        let this = std::env::consts::OS;
        let settings = |text: &str| toml::from_str::<Settings>(text).unwrap();
        let read = settings(&format!(
            "[keys]\nflat = \"Cmd+P\"\n[keys.{this}]\npalette = \"Cmd+P\"\nbroken = 3\nnested = {{ a = \"b\" }}\n"
        ));
        assert_eq!(
            read.key_overrides(),
            BTreeMap::from([("palette".into(), "Cmd+P".into())])
        );
        assert!(settings("keys = 3\n").keys.is_empty());
        // [keys] wins over the older setting, even to say "none".
        let read = settings(&format!(
            "quick_terminal_shortcut = \"Alt+Space\"\n[keys.{this}]\nquick_terminal_global = \"\"\n"
        ));
        assert_eq!(read.global_shortcut(), "");

        let dir = temp_dir("keys-replaced");
        fs::write(dir.join("termf.toml"), "keys = \"odd\"\n").unwrap();
        let keys = BTreeMap::from([("palette".to_owned(), "Cmd+P".to_owned())]);
        save_keys(&dir, &keys).unwrap();
        assert_eq!(load_settings(&dir).unwrap().key_overrides(), keys);
    }

    #[test]
    fn bounds_the_vault_auto_lock_and_never_breaks_the_file_for_it() {
        let minutes = |text: &str| {
            toml::from_str::<Settings>(text)
                .unwrap()
                .vault
                .auto_lock_minutes
        };
        assert_eq!(minutes(""), 10);
        assert_eq!(minutes("[vault]\nauto_lock_minutes = 30\n"), 30);
        assert_eq!(minutes("[vault]\nauto_lock_minutes = 0\n"), 1);
        assert_eq!(minutes("[vault]\nauto_lock_minutes = 99999\n"), 1440);
        assert_eq!(minutes("[vault]\nauto_lock_minutes = \"soon\"\n"), 10);
        assert_eq!(minutes("vault = 3\nscan_depth = 2\n"), 10);

        let dir = temp_dir("vault-auto-lock");
        fs::write(dir.join("termf.toml"), "vault = \"odd\"\n").unwrap();
        save_vault_auto_lock(&dir, 25).unwrap();
        assert_eq!(load_settings(&dir).unwrap().vault.auto_lock_minutes, 25);
    }

    #[test]
    fn creates_and_reads_default_settings() {
        let dir = temp_dir("settings");
        let settings = load_settings(&dir).unwrap();
        let defaults = Settings::default();
        assert_eq!(settings.roots, defaults.roots);
        assert_eq!(settings.agents, defaults.agents);
        assert_eq!(
            settings.quick_terminal_shortcut,
            defaults.quick_terminal_shortcut
        );
        assert!(
            settings.quick_terminal_shortcut.is_empty(),
            "no system-wide shortcut by default"
        );
        assert_eq!(settings.keep_awake, defaults.keep_awake);
        assert_eq!(settings.keep_display_on, defaults.keep_display_on);
        assert_eq!(settings.context_reset, defaults.context_reset);
        assert_eq!(settings.context_reset_at, defaults.context_reset_at);
        assert_eq!(settings.context_warning_at, defaults.context_warning_at);
        assert!(settings.context_reset, "on by default");
        assert!(dir.join("termf.toml").exists());

        fs::write(
            dir.join("termf.toml"),
            "roots = [\"/tmp\"]\nquick_terminal_shortcut = \"Alt+Space\"",
        )
        .unwrap();
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.roots, ["/tmp"]);
        assert_eq!(settings.quick_terminal_shortcut, "Alt+Space");
        assert_eq!(
            settings.agents["claude"], "claude",
            "missing keys keep defaults"
        );
    }

    #[test]
    fn discovers_nested_repos_and_skips_noise() {
        let root = temp_dir("discover");
        for dir in [
            "api/.git",
            "api/packages/inner/.git",
            "group/x/y/deep/.git",
            "group/x/y/z/too-deep/.git",
            "group/not-a-repo/src",
            ".hidden/h/.git",
            "node_modules/n/.git",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        let scan = |roots: &[PathBuf], budget| {
            let roots: Vec<_> = roots
                .iter()
                .map(|r| r.to_string_lossy().into_owned())
                .collect();
            let discovery = discover_with_budget(&roots, 4, budget);
            assert!(discovery.repos.iter().all(|r| r.modified.is_some()));
            let mut names: Vec<_> = discovery.repos.into_iter().map(|r| r.name).collect();
            names.sort();
            (names, discovery.truncated)
        };
        assert_eq!(
            scan(std::slice::from_ref(&root), 100),
            (vec!["api".into(), "deep".into()], false)
        );
        assert_eq!(
            scan(
                &[
                    root.join("node_modules"),
                    root.join("api"),
                    root.join("api")
                ],
                100
            ),
            (vec!["api".into(), "n".into()], false),
            "explicit roots are scanned even if skipped by name, and deduplicated"
        );
        assert_eq!(
            scan(std::slice::from_ref(&root), 2),
            (vec!["api".into()], true),
            "breadth-first: the budget cuts the deep levels first"
        );
    }

    #[test]
    fn reads_projects() {
        let dir = temp_dir("projects");
        fs::create_dir_all(dir.join("projects")).unwrap();
        fs::write(
            dir.join("projects/acme.toml"),
            "name = \"Acme\"\n[[repos]]\npath = \"/src/acme-api\"\n[[repos]]\npath = \"/src/web\"\nname = \"front\"\n",
        )
        .unwrap();
        let projects = load_projects(&dir).unwrap();
        assert_eq!(projects[0].id, "acme");
        let names: Vec<_> = projects[0].repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["acme-api", "front"]);
    }

    fn home_path(rel: &str) -> String {
        dirs::home_dir()
            .unwrap()
            .join(rel)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn slugifies_and_validates_projects() {
        assert_eq!(slugify("Acme Corp!"), "acme-corp");
        assert_eq!(slugify("  Été 2026 "), "été-2026");
        assert_eq!(slugify("!!!"), "project");
        assert_eq!(slugify("CON"), "con-project");
        assert_eq!(slugify("Nul"), "nul-project");
        assert_eq!(slugify("lpt9"), "lpt9-project");
        assert_eq!(slugify("COM¹"), "com¹-project");
        assert_eq!(slugify("console"), "console");
        assert_eq!(slugify("com10"), "com10");

        let dir = temp_dir("validation");
        assert!(save_project(&dir, None, "  ", &[]).is_err());
        assert!(save_project(&dir, None, "X", &["relative/path".into()]).is_err());
        assert!(save_project(&dir, Some("missing"), "X", &[]).is_err());
        for bad in ["", "../termf", ".hidden", "a/b", "a\\b", "c:x", "missing"] {
            assert!(delete_project(&dir, bad).is_err(), "{bad:?} accepted");
        }
    }

    #[test]
    fn saves_projects_and_keeps_hand_edits() {
        let dir = temp_dir("save-projects");
        let (api, web, docs) = (
            home_path("src/api"),
            home_path("src/web"),
            home_path("src/docs"),
        );

        let id = save_project(&dir, None, "Acme Corp", &[api.clone(), web.clone()]).unwrap();
        assert_eq!(id, "acme-corp");
        assert_eq!(
            save_project(&dir, None, "acme corp", &[]).unwrap(),
            "acme-corp-2"
        );
        let text = fs::read_to_string(dir.join("projects/acme-corp.toml")).unwrap();
        assert!(!text.contains(&home_path("")), "paths are stored as ~/…");

        // Debug formatting gives valid TOML strings, Windows backslashes included.
        fs::write(
            dir.join("projects/acme-corp.toml"),
            format!(
                "# Acme notes\nname = \"Acme\"\n\n[[repos]]\npath = {api:?}\nname = \"backend\" # renamed\nroles = [\"back\"]\n\n[[repos]]\npath = {web:?}\n\n[[repos]]\npath = \"code/legacy\"\n"
            ),
        )
        .unwrap();
        let legacy = "code/legacy".to_string();
        save_project(&dir, Some("acme-corp"), " Acme Inc ", &[api, legacy, docs]).unwrap();
        assert!(
            save_project(&dir, Some("acme-corp"), "Acme Inc", &["code/new".into()]).is_err(),
            "a relative path added from the UI is still rejected"
        );
        let text = fs::read_to_string(dir.join("projects/acme-corp.toml")).unwrap();
        for kept in [
            "# Acme notes",
            "path = \"code/legacy\"",
            "name = \"backend\" # renamed",
            "roles = [\"back\"]",
        ] {
            assert!(text.contains(kept), "lost {kept:?} in:\n{text}");
        }
        assert!(!dir.join("projects/acme-corp.toml.tmp").exists());

        let projects = load_projects(&dir).unwrap();
        let acme = projects.iter().find(|p| p.id == "acme-corp").unwrap();
        assert_eq!(acme.name, "Acme Inc");
        let names: Vec<_> = acme.repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["backend", "legacy", "docs"]);

        fs::create_dir_all(dir.join("projects/acme-corp/library/skills")).unwrap();
        delete_project(&dir, "acme-corp").unwrap();
        assert!(!dir.join("projects/acme-corp").exists());
        let ids: Vec<_> = load_projects(&dir)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, ["acme-corp-2"]);
    }

    #[test]
    fn layouts_round_trip() {
        let dir = temp_dir("layouts");
        assert!(load_layouts(&dir).unwrap().is_empty());
        let session = |rel: &str, agent: Option<&str>| LayoutSession {
            path: home_path(rel),
            agent: agent.map(Into::into),
        };
        let morning = Layout {
            name: "morning".into(),
            sessions: vec![session("src/api", Some("claude")), session("src/web", None)],
            tabs: vec![LayoutTab {
                name: "review".into(),
                splits: r#"{"dir":"row","sizes":[0.5,0.5],"children":[0,1]}"#.into(),
            }],
        };
        save_layout(
            &dir,
            Layout {
                name: " morning ".into(),
                ..morning.clone()
            },
        )
        .unwrap();
        save_layout(
            &dir,
            Layout {
                name: "evening".into(),
                sessions: vec![session("src/docs", None)],
                tabs: vec![],
            },
        )
        .unwrap();
        let text = fs::read_to_string(dir.join("layouts.toml")).unwrap();
        assert!(!text.contains(&home_path("")), "paths are stored as ~/…");
        assert_eq!(text.matches("[[layouts.tabs]]").count(), 1, "{text}");
        assert_eq!(load_layouts(&dir).unwrap()[0], morning);

        save_layout(
            &dir,
            Layout {
                name: "morning".into(),
                sessions: vec![session("src/api", None)],
                tabs: vec![],
            },
        )
        .unwrap();
        let layouts = load_layouts(&dir).unwrap();
        assert_eq!(layouts.len(), 2, "same name replaces");
        assert_eq!(layouts[0].sessions, [session("src/api", None)]);

        delete_layout(&dir, "morning").unwrap();
        let names: Vec<_> = load_layouts(&dir)
            .unwrap()
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["evening"]);

        let empty_name = Layout {
            name: " ".into(),
            ..morning.clone()
        };
        assert!(save_layout(&dir, empty_name).is_err());
        let no_sessions = Layout {
            sessions: vec![],
            ..morning
        };
        assert!(save_layout(&dir, no_sessions).is_err());

        // Saved before tabs: no `tabs` key.
        let old = temp_dir("layouts-old");
        fs::write(
            old.join("layouts.toml"),
            "[[layouts]]\nname = \"x\"\n\n[[layouts.sessions]]\npath = \"/a\"\n",
        )
        .unwrap();
        assert!(load_layouts(&old).unwrap()[0].tabs.is_empty());
    }

    #[test]
    fn expands_tilde_only_at_start() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~/x"), home.join("x"));
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("/a/~b"), PathBuf::from("/a/~b"));
        assert_eq!(expand_tilde("~user/x"), PathBuf::from("~user/x"));
    }
}
