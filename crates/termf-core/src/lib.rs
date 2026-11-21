use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

pub mod claude;
pub mod ipc;

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
        }
    }
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
    fn at(path: PathBuf, name: Option<String>) -> Self {
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
    load_settings(config_dir)?;
    let file = config_dir.join("termf.toml");
    let mut doc: DocumentMut = fs::read_to_string(&file)?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    let mode = match mode {
        KeepAwake::Auto => "auto",
        KeepAwake::Always => "always",
        KeepAwake::Off => "off",
    };
    doc["keep_awake"] = value(mode);
    doc["keep_display_on"] = value(display_on);
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
    doc["name"] = value(name);
    let list = doc
        .entry("repos")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .with_context(|| format!("`repos` in {} must be [[repos]] tables", file.display()))?;
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
    write_atomic(&file, &doc.to_string())?;
    Ok(id)
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
        Some(rest) => Path::new("~").join(rest).to_string_lossy().into_owned(),
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
            },
        )
        .unwrap();
        let text = fs::read_to_string(dir.join("layouts.toml")).unwrap();
        assert!(!text.contains(&home_path("")), "paths are stored as ~/…");
        assert_eq!(load_layouts(&dir).unwrap()[0], morning);

        save_layout(
            &dir,
            Layout {
                name: "morning".into(),
                sessions: vec![session("src/api", None)],
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
