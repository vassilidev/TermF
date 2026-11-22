//! Claude Code's saved conversations by project, its background sessions, and purging a project's data.
//! A transcript's format is internal to Claude Code (CLAUDE-CODE.md): only a few metadata lines are read, from
//! the start and the end of the file, and anything not found is simply absent.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;

use crate::cli::{self, Output};
use crate::{Env, mask};

/// Read from each end of a transcript, enough for its first prompt and its latest title.
const EDGE: u64 = 64 * 1024;
pub(crate) const QUICK: Duration = Duration::from_secs(20);

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    /// The name set with `--name` or `/rename`, else the generated title.
    pub title: Option<String>,
    /// The first prompt, else the latest one, cut short.
    pub prompt: Option<String>,
    /// Seconds since the epoch.
    pub modified: u64,
    pub size: u64,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    /// The folder under `projects/`, which encodes the working directory.
    pub folder: String,
    /// The working directory the transcripts give.
    pub cwd: Option<String>,
    /// Most recent first.
    pub sessions: Vec<Session>,
}

/// The folder Claude Code keeps a directory's sessions in: every character but ASCII letters and digits
/// becomes `-` (code.claude.com/docs/en/sessions).
pub fn folder_name(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The repository's own folder and those of its worktrees (`<repo>/.claude/worktrees/…`). A name longer than
/// 200 characters is cut and hashed by Claude Code: only its first 200 are compared.
fn belongs(folder: &str, repo: &str) -> bool {
    let (folder, repo) = (
        &folder[..folder.len().min(200)],
        &repo[..repo.len().min(200)],
    );
    folder == repo || folder.starts_with(&format!("{repo}--claude-worktrees-"))
}

/// `repo` by its real path too: Claude Code names the folder after it (`/tmp` is `/private/tmp` on macOS).
fn repo_names(repo: &Path) -> Vec<String> {
    let mut names = vec![folder_name(repo)];
    if let Ok(real) = fs::canonicalize(repo) {
        names.push(folder_name(&real));
    }
    names.dedup();
    names
}

fn cut(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text,
    }
}

/// The text of a user message: a plain string, or the text blocks of a list.
fn message_text(entry: &Value) -> Option<String> {
    let content = &entry["message"]["content"];
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    // Commands and the notes Claude Code adds for itself are tags, not prompts.
    let text = text.trim();
    (!text.is_empty() && !text.starts_with('<')).then(|| text.to_owned())
}

#[derive(Default, Debug, PartialEq)]
pub struct Meta {
    pub title: Option<String>,
    pub prompt: Option<String>,
    pub cwd: Option<String>,
}

/// What the edges of a transcript tell. `head` and `tail` may start or end mid-line: such lines do not parse
/// and are skipped.
pub fn meta(head: &str, tail: &str) -> Meta {
    let parse = |text: &str| -> Vec<Value> {
        text.lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .collect()
    };
    let (head, tail) = (parse(head), parse(tail));
    let (mut custom, mut generated, mut last, mut first, mut cwd) = (None, None, None, None, None);
    for entry in head.iter().chain(&tail) {
        let text = |key: &str| entry[key].as_str().filter(|s| !s.trim().is_empty());
        match entry["type"].as_str() {
            Some("custom-title") => custom = text("customTitle").or(custom),
            Some("ai-title") => generated = text("aiTitle").or(generated),
            Some("summary") => generated = generated.or(text("summary")),
            Some("last-prompt") => last = text("lastPrompt").or(last),
            Some("user") if first.is_none() && entry["isMeta"] != true => {
                first = message_text(entry);
            }
            _ => {}
        }
        if cwd.is_none() {
            cwd = text("cwd").map(str::to_owned);
        }
    }
    let clean = |s: &str| mask::mask_text(&cut(s, 120));
    Meta {
        title: custom.or(generated).map(clean),
        prompt: first.map(|f| clean(&f)).or(last.map(clean)),
        cwd,
    }
}

fn edges(file: &Path, size: u64) -> (String, String) {
    let Ok(mut f) = File::open(file) else {
        return Default::default();
    };
    let mut head = Vec::new();
    let _ = (&mut f).take(EDGE).read_to_end(&mut head);
    let mut tail = Vec::new();
    if size > EDGE
        && f.seek(SeekFrom::Start(size.saturating_sub(EDGE).max(EDGE)))
            .is_ok()
    {
        let _ = f.read_to_end(&mut tail);
    }
    (
        String::from_utf8_lossy(&head).into_owned(),
        String::from_utf8_lossy(&tail).into_owned(),
    )
}

fn project(dir: &Path) -> Option<Project> {
    let mut cwd = None;
    let mut sessions: Vec<Session> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let id = path
                .file_name()?
                .to_str()?
                .strip_suffix(".jsonl")?
                .to_owned();
            let m = e.metadata().ok().filter(|m| m.is_file())?;
            let (head, tail) = edges(&path, m.len());
            let meta = meta(&head, &tail);
            cwd = cwd.take().or(meta.cwd);
            Some(Session {
                id,
                title: meta.title,
                prompt: meta.prompt,
                modified: m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
                size: m.len(),
            })
        })
        .collect();
    if sessions.is_empty() {
        return None;
    }
    sessions.sort_by_key(|s| std::cmp::Reverse(s.modified));
    Some(Project {
        folder: dir.file_name()?.to_string_lossy().into_owned(),
        cwd,
        sessions,
    })
}

/// The saved conversations of `repo` (and its worktrees), or of every project when `None`, most recent first.
// shortcut: every transcript's edges are read on each call; cache by modification time if a machine with
// thousands of sessions finds it slow.
pub fn list(env: &Env, repo: Option<&Path>) -> Vec<Project> {
    let names = repo.map(repo_names);
    let mut projects: Vec<Project> = fs::read_dir(env.claude_dir.join("projects"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            names
                .as_ref()
                .is_none_or(|names| names.iter().any(|n| belongs(&folder, n)))
        })
        .filter_map(|e| project(&e.path()))
        .collect();
    projects.sort_by_key(|p| std::cmp::Reverse(p.sessions[0].modified));
    projects
}

/// A session id Claude Code takes for `--resume`: a UUID, never something that reads as an option.
pub fn check_id(id: &str) -> Result<()> {
    ensure!(
        id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
        "not a session id: {id}"
    );
    Ok(())
}

/// A background session's short id or name, as `claude agents --json` gives it.
pub fn check_background(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && !id.starts_with('-')
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c)),
        "not a background session id: {id}"
    );
    Ok(())
}

/// Through its resolved path: on Windows `claude` is often a `claude.cmd` that `Command` would not find.
pub(crate) fn claude(env: &Env, args: &[&str], timeout: Duration) -> Result<Output> {
    let program = cli::which(env, &env.claude)
        .map_or_else(|| env.claude.clone(), |p| p.to_string_lossy().into_owned());
    cli::run(env, &program, args, timeout)
}

/// What a command printed, masked; its error output when it failed.
pub(crate) fn printed(out: Output) -> Result<String> {
    let text = format!("{}{}", out.stdout, out.stderr);
    let text = crate::native::mask_lines(text.trim_end());
    if !out.ok {
        bail!(
            "{}",
            if text.is_empty() {
                "failed".into()
            } else {
                text
            }
        );
    }
    Ok(text)
}

fn purge_target(path: &Path) -> Result<String> {
    ensure!(path.is_absolute(), "purge takes a project's folder");
    Ok(path.to_string_lossy().into_owned())
}

/// What `claude purge` would delete for this project: its transcripts, automatic memory, tasks, file history
/// and its entry in `~/.claude.json`.
pub fn purge_preview(env: &Env, path: &Path) -> Result<String> {
    let target = purge_target(path)?;
    printed(claude(env, &["purge", &target, "--dry-run"], QUICK)?)
}

/// Purges once the user confirmed in TermF: Claude Code's own question has nobody to answer it.
pub fn purge(env: &Env, path: &Path) -> Result<String> {
    let target = purge_target(path)?;
    printed(claude(env, &["purge", &target, "--yes"], QUICK)?)
}

#[derive(Serialize, Debug, Default)]
pub struct Background {
    /// `claude agents --json --all`, masked.
    pub sessions: Vec<Value>,
    pub error: Option<String>,
}

pub fn background(env: &Env) -> Background {
    let listed = claude(env, &["agents", "--json", "--all"], QUICK).and_then(|out| {
        if !out.ok {
            bail!(
                "{}",
                printed(out)
                    .err()
                    .map(|e| e.to_string())
                    .unwrap_or_default()
            );
        }
        let mut list: Vec<Value> = serde_json::from_str(out.stdout.trim())?;
        list.iter_mut().for_each(mask::mask);
        Ok(list)
    });
    match listed {
        Ok(sessions) => Background {
            sessions,
            error: None,
        },
        Err(e) => Background {
            sessions: Vec::new(),
            error: Some(format!("{e:#}")),
        },
    }
}

pub fn stop(env: &Env, id: &str) -> Result<String> {
    check_background(id)?;
    printed(claude(env, &["stop", id], QUICK)?)
}
