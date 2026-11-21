//! Claude Code on this machine, read-only: every setting by scope, MCP servers, plugins, skills and the like,
//! hooks, memory, account, version and the tools around it (ARCHITECTURE §11.3, CLAUDE-CODE.md). What is
//! shown, copied or exported is masked first: no secret, token or credential file ever leaves this crate.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

pub mod artifacts;
pub mod cli;
pub mod mask;
pub mod settings;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Absent,
    Invalid,
    Ok,
}

/// Where Claude Code keeps things, and how to run it.
pub struct Env {
    /// `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
    pub claude_dir: PathBuf,
    /// Where managed settings and the managed `CLAUDE.md` live on this OS.
    pub managed_dir: PathBuf,
    /// The repository whose project and local scopes are read.
    pub repo: Option<PathBuf>,
    /// `PATH` for the commands; `None`: the current one.
    pub path: Option<OsString>,
    /// The program that runs Claude Code.
    pub claude: String,
}

impl Env {
    pub fn current(repo: Option<PathBuf>, claude: String) -> Env {
        Env {
            claude_dir: claude_dir(),
            managed_dir: managed_dir(),
            repo,
            path: termf_core::shell::login_path().map(OsString::from),
            claude,
        }
    }

    /// Commands run in the repository, where its project and local MCP servers apply.
    fn cwd(&self) -> PathBuf {
        self.repo
            .clone()
            .filter(|r| r.is_dir())
            .unwrap_or_else(|| termf_core::expand_tilde("~"))
    }
}

pub fn claude_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| termf_core::expand_tilde("~/.claude"))
}

fn managed_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        "/Library/Application Support/ClaudeCode".into()
    } else if cfg!(windows) {
        r"C:\Program Files\ClaudeCode".into()
    } else {
        "/etc/claude-code".into()
    }
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name: String,
    pub description: Option<String>,
    pub path: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    /// `skills`, `agents`, `commands`, `output-styles` or `rules`.
    pub kind: &'static str,
    pub scope: settings::Scope,
    pub path: String,
    pub items: Vec<Item>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Hook {
    pub scope: settings::Scope,
    pub event: String,
    pub matcher: Option<String>,
    /// `command`, `http`, `prompt`…
    pub kind: String,
    /// The command, URL or prompt, masked.
    pub run: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub scope: Option<settings::Scope>,
    pub path: String,
    pub state: State,
    pub size: Option<u64>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFolder {
    /// The project folder name under `projects/`, which encodes its path.
    pub project: String,
    pub path: String,
    pub files: Vec<(String, u64)>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    /// `CLAUDE.md` and `CLAUDE.local.md` by scope.
    pub files: Vec<FileInfo>,
    /// Automatic memory, one folder per project.
    pub auto: Vec<MemoryFolder>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub keybindings: FileInfo,
    /// Custom themes in `themes/`, for `"theme": "custom:<name>"`.
    pub themes: Vec<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub claude_dir: String,
    pub repo: Option<String>,
    pub settings: settings::Settings,
    pub mcp: cli::Mcp,
    pub plugins: cli::Plugins,
    pub extensions: Vec<Folder>,
    pub hooks: Vec<Hook>,
    pub memory: Memory,
    pub interface: Interface,
    pub account: cli::Account,
    pub tools: Vec<cli::Tool>,
}

/// Everything at once: the files are read while Claude Code's commands run.
pub fn overview(env: &Env) -> Overview {
    let (mcp, plugins, account, tools) = std::thread::scope(|s| {
        let mcp = s.spawn(|| cli::mcp(env));
        let plugins = s.spawn(|| cli::plugins(env));
        let account = s.spawn(|| cli::account(env));
        let tools = s.spawn(|| cli::tools(env));
        (
            mcp.join().unwrap_or_default(),
            plugins.join().unwrap_or_default(),
            account.join().unwrap_or_default(),
            tools.join().unwrap_or_default(),
        )
    });
    let settings = settings::read(env);
    let hooks = hooks(&settings);
    let auto_dir = settings
        .effective
        .iter()
        .find(|e| e.key == "autoMemoryDirectory")
        .and_then(|e| e.value.as_str())
        .map(termf_core::expand_tilde);
    Overview {
        claude_dir: env.claude_dir.to_string_lossy().into_owned(),
        repo: env.repo.as_ref().map(|r| r.to_string_lossy().into_owned()),
        settings: settings::masked(settings),
        mcp,
        plugins,
        extensions: extensions(env),
        hooks,
        memory: memory(env, auto_dir),
        interface: interface(env),
        account,
        tools,
    }
}

fn hooks(settings: &settings::Settings) -> Vec<Hook> {
    let mut list = Vec::new();
    for f in &settings.files {
        let Some(Value::Object(events)) = f.value.as_ref().and_then(|v| v.get("hooks")) else {
            continue;
        };
        for (event, groups) in events {
            for group in groups.as_array().into_iter().flatten() {
                let matcher = group["matcher"].as_str().filter(|m| !m.is_empty());
                for hook in group["hooks"].as_array().into_iter().flatten() {
                    let kind = hook["type"].as_str().unwrap_or("command");
                    let run = ["command", "url", "prompt", "tool"]
                        .iter()
                        .find_map(|k| hook[*k].as_str())
                        .unwrap_or_default();
                    let args: Vec<&str> = hook["args"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect();
                    let run = [run].into_iter().chain(args).collect::<Vec<_>>().join(" ");
                    list.push(Hook {
                        scope: f.scope,
                        event: event.clone(),
                        matcher: matcher.map(str::to_owned),
                        kind: kind.to_owned(),
                        run: mask::mask_text(&run),
                    });
                }
            }
        }
    }
    list
}

/// `name` and `description` from a Markdown file's frontmatter.
fn frontmatter(path: &Path) -> (Option<String>, Option<String>) {
    let Ok(text) = fs::read_to_string(path) else {
        return (None, None);
    };
    let text = text.replace("\r\n", "\n");
    let Some(rest) = text.strip_prefix("---\n") else {
        return (None, None);
    };
    let Some(end) = rest.find("\n---") else {
        return (None, None);
    };
    let Ok(docs) = yaml_rust2::YamlLoader::load_from_str(&rest[..end]) else {
        return (None, None);
    };
    let field = |key: &str| {
        docs.first()
            .and_then(|d| d[key].as_str())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    (field("name"), field("description"))
}

fn folder(kind: &'static str, scope: settings::Scope, dir: PathBuf) -> Option<Folder> {
    let mut items = Vec::new();
    collect(kind, &dir, &dir, &mut items);
    if items.is_empty() && !dir.is_dir() {
        return None;
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Some(Folder {
        kind,
        scope,
        path: dir.to_string_lossy().into_owned(),
        items,
    })
}

/// Skills are folders with a `SKILL.md`; the rest are Markdown files, in subfolders too (`commands/git/pr.md`
/// is `/git:pr`).
fn collect(kind: &str, root: &Path, dir: &Path, items: &mut Vec<Item>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let skill = path.join("SKILL.md");
        if kind == "skills" {
            if skill.is_file() {
                let (_, description) = frontmatter(&skill);
                items.push(Item {
                    name,
                    description,
                    path: skill.to_string_lossy().into_owned(),
                });
            }
        } else if path.is_dir() {
            collect(kind, root, &path, items);
        } else if let Some(stem) = name.strip_suffix(".md") {
            let (named, description) = frontmatter(&path);
            let relative = path.strip_prefix(root).unwrap_or(&path).with_extension("");
            let relative = relative.to_string_lossy().replace(['/', '\\'], ":");
            items.push(Item {
                name: if kind == "agents" {
                    named.unwrap_or_else(|| stem.to_owned())
                } else {
                    relative
                },
                description,
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
}

const KINDS: &[&str] = &["skills", "agents", "commands", "output-styles", "rules"];

fn extensions(env: &Env) -> Vec<Folder> {
    use settings::Scope;
    let mut folders = Vec::new();
    for kind in KINDS {
        folders.extend(folder(kind, Scope::User, env.claude_dir.join(kind)));
        if let Some(repo) = &env.repo {
            folders.extend(folder(
                kind,
                Scope::Project,
                repo.join(".claude").join(kind),
            ));
        }
    }
    folders
}

fn file_info(scope: Option<settings::Scope>, path: PathBuf) -> FileInfo {
    let meta = fs::metadata(&path).ok().filter(|m| m.is_file());
    FileInfo {
        scope,
        state: if meta.is_some() {
            State::Ok
        } else {
            State::Absent
        },
        size: meta.map(|m| m.len()),
        path: path.to_string_lossy().into_owned(),
    }
}

fn memory(env: &Env, auto_dir: Option<PathBuf>) -> Memory {
    use settings::Scope;
    let mut files = vec![
        file_info(Some(Scope::Managed), env.managed_dir.join("CLAUDE.md")),
        file_info(Some(Scope::User), env.claude_dir.join("CLAUDE.md")),
    ];
    if let Some(repo) = &env.repo {
        let shared = repo.join("CLAUDE.md");
        let in_folder = repo.join(".claude/CLAUDE.md");
        files.push(file_info(
            Some(Scope::Project),
            if !shared.is_file() && in_folder.is_file() {
                in_folder
            } else {
                shared
            },
        ));
        files.push(file_info(Some(Scope::Local), repo.join("CLAUDE.local.md")));
    }
    let folders: Vec<(String, PathBuf)> = match auto_dir {
        Some(dir) => vec![(String::new(), dir)],
        None => fs::read_dir(env.claude_dir.join("projects"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    e.path().join("memory"),
                )
            })
            .filter(|(_, dir)| dir.is_dir())
            .collect(),
    };
    let mut auto: Vec<MemoryFolder> = folders
        .into_iter()
        .map(|(project, dir)| {
            let mut files: Vec<(String, u64)> = fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    let meta = e.metadata().ok().filter(|m| m.is_file())?;
                    Some((e.file_name().to_string_lossy().into_owned(), meta.len()))
                })
                .collect();
            files.sort();
            MemoryFolder {
                project,
                path: dir.to_string_lossy().into_owned(),
                files,
            }
        })
        .collect();
    auto.sort_by(|a, b| a.project.cmp(&b.project));
    Memory { files, auto }
}

fn interface(env: &Env) -> Interface {
    let mut themes: Vec<String> = fs::read_dir(env.claude_dir.join("themes"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(".json")
                .map(str::to_owned)
        })
        .collect();
    themes.sort();
    Interface {
        keybindings: file_info(None, env.claude_dir.join("keybindings.json")),
        themes,
    }
}

/// The text of an export, masked once more: whatever the page put in it, no secret reaches the file.
pub fn export_text(text: &str) -> String {
    text.lines()
        .map(mask::mask_text)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[cfg(test)]
mod tests;
