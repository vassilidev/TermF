use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_SETTINGS: &str = r#"# TermF — global settings
# Folders scanned for git repositories. Hidden, system, media and dependency folders are skipped.
# On macOS, Desktop, Documents and Downloads are only scanned when listed here (macOS asks permission for them).
roots = ["~"]
# How many folder levels below each root are searched.
scan_depth = 5
# Command for "Open in external editor" (e.g. "code", "zed"). Unset: the system's default app.
# editor = "code"

# Agents available in the UI: name = command run in the repository.
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
    pub agents: BTreeMap<String, String>,
    pub editor: Option<String>,
}

// Must stay in sync with DEFAULT_SETTINGS (checked by a test); parsing it here would recurse through serde(default).
impl Default for Settings {
    fn default() -> Self {
        Self {
            roots: vec!["~".into()],
            scan_depth: 5,
            agents: BTreeMap::from([("claude".into(), "claude".into())]),
            editor: None,
        }
    }
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
    fn creates_and_reads_default_settings() {
        let dir = temp_dir("settings");
        let settings = load_settings(&dir).unwrap();
        let defaults = Settings::default();
        assert_eq!(settings.roots, defaults.roots);
        assert_eq!(settings.agents, defaults.agents);
        assert!(dir.join("termf.toml").exists());

        fs::write(dir.join("termf.toml"), "roots = [\"/tmp\"]").unwrap();
        let settings = load_settings(&dir).unwrap();
        assert_eq!(settings.roots, ["/tmp"]);
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

    #[test]
    fn expands_tilde_only_at_start() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~/x"), home.join("x"));
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("/a/~b"), PathBuf::from("/a/~b"));
        assert_eq!(expand_tilde("~user/x"), PathBuf::from("~user/x"));
    }
}
