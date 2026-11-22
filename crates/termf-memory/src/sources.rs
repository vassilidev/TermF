//! What each source holds for a project. A source lists every key it has with its stamp, and reads a document
//! only when its stamp is new. Claude Code's transcripts are an internal format (CLAUDE-CODE.md): only user
//! prompts and the text of answers are taken, tolerantly; any other line is skipped.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use termf_claude::sessions::{belongs, folder_name};
use termf_core::Project;

use crate::{Doc, Found, MAX_BODY, Paths, Source, cut};

/// The end of a transcript read: its latest part, the one indexed anyway.
const TRANSCRIPT_READ: u64 = 16 << 20;
const MAX_COMMITS: usize = 2000;
const MAX_DOCS: usize = 1000;
const DOC_EXTENSIONS: [&str; 6] = ["md", "mdx", "markdown", "rst", "adoc", "txt"];
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn collect(
    source: Source,
    paths: &Paths,
    project: &Project,
    known: &HashMap<String, String>,
) -> Result<Found> {
    let mut found = Found::default();
    let repos: Vec<PathBuf> = project
        .repos
        .iter()
        .map(|r| PathBuf::from(&r.path))
        .collect();
    match source {
        Source::Claude => {
            for dir in claude_folders(&paths.claude_dir, &repos) {
                for file in files_in(&dir, &["jsonl"]) {
                    found.add(known, key(&file), stamp(&file), || transcript(&file));
                }
            }
        }
        Source::Memory => {
            let mut files = Vec::new();
            for repo in &repos {
                for name in ["CLAUDE.md", ".claude/CLAUDE.md", "CLAUDE.local.md"] {
                    files.push(repo.join(name));
                }
                walk(&repo.join(".claude/rules"), &["md"], &mut files);
            }
            for dir in claude_folders(&paths.claude_dir, &repos) {
                walk(&dir.join("memory"), &["md"], &mut files);
            }
            add_files(&mut found, known, files.into_iter().filter(|f| f.is_file()));
        }
        Source::Docs => {
            for repo in &repos {
                let files = git(repo, &["ls-files", "-z"], b"")?;
                let docs = files
                    .split('\0')
                    .filter(|f| {
                        let name = f.rsplit('/').next().unwrap_or(f);
                        !["CLAUDE.md", "CLAUDE.local.md"].contains(&name)
                            && Path::new(f)
                                .extension()
                                .is_some_and(|e| DOC_EXTENSIONS.iter().any(|d| e == *d))
                    })
                    .take(MAX_DOCS)
                    .map(|f| repo.join(f))
                    .filter(|f| f.is_file());
                add_files(&mut found, known, docs);
            }
        }
        Source::Library => {
            let own = paths.config_dir.join("projects").join(&project.id);
            let mut files = Vec::new();
            walk(&own.join("library"), &["md", "toml"], &mut files);
            walk(&own.join("conventions"), &["md", "toml"], &mut files);
            for repo in &repos {
                walk(&repo.join(".termf"), &["md", "toml"], &mut files);
            }
            add_files(&mut found, known, files.into_iter());
        }
        Source::Git => {
            for repo in &repos {
                commits(&mut found, known, repo)?;
            }
        }
        Source::Runs => runs(&mut found, known, paths, project)?,
        Source::Agents => agents(&mut found, known, paths, project, &repos)?,
        Source::Artifacts => {
            let index = termf_claude::artifacts::load(&paths.data_dir.join("artifacts.json"));
            for artifact in index.artifacts {
                let Some(cwd) = artifact.cwd.clone() else {
                    continue;
                };
                if !repos.iter().any(|r| Path::new(&cwd).starts_with(r)) {
                    continue;
                }
                let title = artifact
                    .title
                    .clone()
                    .unwrap_or_else(|| artifact.url.clone());
                let updated = artifact.updated.clone().unwrap_or_default();
                let stamp = format!("{title}|{updated}|{cwd}");
                found.add(known, artifact.id.clone(), stamp, || {
                    Some(Doc {
                        body: format!(
                            "claude.ai artifact: {title}\n{}\npublished or opened from {cwd}\nupdated {updated}\n",
                            artifact.url
                        ),
                        title: title.clone(),
                        locator: artifact.url.clone(),
                        at: artifact.seen as i64,
                        ..Doc::default()
                    })
                });
            }
        }
    }
    Ok(found)
}

fn key(file: &Path) -> String {
    file.to_string_lossy().into_owned()
}

/// Modification time and size: enough to see a file changed.
fn stamp(file: &Path) -> String {
    let meta = fs::metadata(file).ok();
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    format!("{modified}-{}", meta.map_or(0, |m| m.len()))
}

fn modified(file: &Path) -> i64 {
    fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

fn add_files(
    found: &mut Found,
    known: &HashMap<String, String>,
    files: impl Iterator<Item = PathBuf>,
) {
    for file in files {
        found.add(known, key(&file), stamp(&file), || {
            let mut bytes = Vec::new();
            File::open(&file)
                .ok()?
                .take(MAX_BODY as u64)
                .read_to_end(&mut bytes)
                .ok()?;
            Some(Doc {
                title: file.to_string_lossy().into_owned(),
                locator: file.to_string_lossy().into_owned(),
                at: modified(&file),
                body: String::from_utf8_lossy(&bytes).into_owned(),
                ..Doc::default()
            })
        });
    }
}

/// Files with one of `extensions` below `dir`, without following links into other folders.
fn walk(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    fn go(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>, depth: usize) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && depth < 8 {
                go(&path, extensions, out, depth + 1);
            } else if !kind.is_dir()
                && path
                    .extension()
                    .is_some_and(|e| extensions.iter().any(|x| e == *x))
                && out.len() < MAX_DOCS
            {
                out.push(path);
            }
        }
    }
    go(dir, extensions, out, 0);
}

fn files_in(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|e| extensions.iter().any(|x| e == *x))
        })
        .collect();
    files.sort();
    files
}

/// The folders of `projects/` that belong to the repositories and their worktrees, by the path given and the
/// real one (`/tmp` is `/private/tmp` on macOS).
fn claude_folders(claude_dir: &Path, repos: &[PathBuf]) -> Vec<PathBuf> {
    let mut names = Vec::new();
    for repo in repos {
        names.push(folder_name(repo));
        if let Ok(real) = fs::canonicalize(repo) {
            names.push(folder_name(&real));
        }
    }
    let mut folders: Vec<PathBuf> = fs::read_dir(claude_dir.join("projects"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            names.iter().any(|n| belongs(&folder, n))
        })
        .map(|e| e.path())
        .collect();
    folders.sort();
    folders
}

/// The text of a message: a plain string, or the text blocks of a list (never tool calls or their output).
fn message_text(entry: &Value) -> Option<String> {
    let text = match &entry["message"]["content"] {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let text = text.trim();
    // Commands and the notes Claude Code adds for itself are tags, not prompts.
    (!text.is_empty() && !text.starts_with('<')).then(|| text.to_owned())
}

fn transcript(file: &Path) -> Option<Doc> {
    let mut f = File::open(file).ok()?;
    let size = f.metadata().ok()?.len();
    if size > TRANSCRIPT_READ {
        f.seek(SeekFrom::Start(size - TRANSCRIPT_READ)).ok()?;
    }
    let (mut custom, mut generated, mut first, mut cwd) = (None, None, None, None);
    let mut body = String::new();
    // A line cut by the seek does not parse and is skipped.
    for line in BufReader::new(f).split(b'\n').map_while(Result::ok) {
        let Ok(entry) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        let text = |key: &str| {
            entry[key]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned)
        };
        let who = match entry["type"].as_str() {
            Some("custom-title") => {
                custom = text("customTitle").or(custom);
                continue;
            }
            Some("ai-title") => {
                generated = text("aiTitle").or(generated);
                continue;
            }
            Some("summary") => {
                generated = generated.or(text("summary"));
                continue;
            }
            Some("user") if entry["isMeta"] != true => "you",
            Some("assistant") => "claude",
            _ => continue,
        };
        cwd = cwd.or(text("cwd"));
        let Some(said) = message_text(&entry) else {
            continue;
        };
        if who == "you" && first.is_none() {
            first = Some(said.clone());
        }
        let when = text("timestamp")
            .map(|t| format!("[{}] ", t.get(..16).unwrap_or(&t).replace('T', " ")))
            .unwrap_or_default();
        body.push_str(&format!("{when}{who}: {said}\n\n"));
    }
    if body.is_empty() {
        return None;
    }
    let id = file.file_stem()?.to_string_lossy().into_owned();
    let place = cwd.unwrap_or_else(|| file.parent().map(key).unwrap_or_default());
    Some(Doc {
        title: custom.or(generated).or(first).unwrap_or_else(|| id.clone()),
        locator: format!("Claude Code conversation {id} in {place}"),
        at: modified(file),
        body,
        ..Doc::default()
    })
}

fn git(repo: &Path, args: &[&str], input: &[u8]) -> Result<String> {
    let mut cmd = Command::new("git");
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
    cmd.arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    let (status, out, err) = termf_core::process::run(cmd, input, GIT_TIMEOUT)?;
    if !status.success() {
        bail!(
            "git {} in {}: {}",
            args[0],
            repo.display(),
            String::from_utf8_lossy(&err).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// The latest commits of `repo`: those already indexed stay as they are, the others are read in one call.
fn commits(found: &mut Found, known: &HashMap<String, String>, repo: &Path) -> Result<()> {
    let listed = match git(
        repo,
        &["rev-list", &format!("--max-count={MAX_COMMITS}"), "HEAD"],
        b"",
    ) {
        Ok(listed) => listed,
        // A repository without commits has nothing to index.
        Err(_) if git(repo, &["rev-parse", "--git-dir"], b"").is_ok() => return Ok(()),
        Err(e) => return Err(e),
    };
    let name = repo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut new = Vec::new();
    for hash in listed.lines().filter(|h| !h.is_empty()) {
        let key = format!("{}@{hash}", key(repo));
        if known.get(&key).map(String::as_str) != Some(hash) {
            new.push(hash.to_owned());
        }
        found.keys.insert(key);
    }
    if new.is_empty() {
        return Ok(());
    }
    let log = git(
        repo,
        &[
            "log",
            "--stdin",
            "--no-walk=unsorted",
            "--name-only",
            "--format=%x1e%H%x1f%an%x1f%at%x1f%B%x1f",
        ],
        new.join("\n").as_bytes(),
    )?;
    for record in log.split('\x1e').filter(|r| !r.trim().is_empty()) {
        let fields: Vec<&str> = record.splitn(5, '\x1f').collect();
        let [hash, author, at, message, files] = fields[..] else {
            continue;
        };
        let files: Vec<&str> = files.lines().filter(|f| !f.trim().is_empty()).collect();
        found.docs.push(Doc {
            key: format!("{}@{hash}", key(repo)),
            stamp: hash.to_owned(),
            title: message.lines().next().unwrap_or_default().to_owned(),
            locator: format!("commit {} of {name}", &hash[..hash.len().min(10)]),
            at: at.trim().parse().unwrap_or(0),
            body: format!(
                "{}\n\nauthor: {author}\nfiles:\n{}\n",
                message.trim_end(),
                files.join("\n")
            ),
        });
    }
    Ok(())
}

fn open_db(paths: &Paths) -> Result<Option<Connection>> {
    let file = paths.data_dir.join("termf.db");
    if !file.exists() {
        return Ok(None);
    }
    let conn = Connection::open(file)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(Some(conn))
}

fn has_table(conn: &Connection, name: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// Ticket runs: their journal (ticket, events, team lines); team runs: their plan and the steps' reports.
fn runs(
    found: &mut Found,
    known: &HashMap<String, String>,
    paths: &Paths,
    project: &Project,
) -> Result<()> {
    let Some(conn) = open_db(paths)? else {
        return Ok(());
    };
    if has_table(&conn, "runs")? {
        let mut query =
            conn.prepare("SELECT id, ticket_key, title FROM runs WHERE project = ?1")?;
        let rows: Vec<(i64, String, String)> = query
            .query_map([&project.id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, ticket, title) in rows {
            let file = paths
                .data_dir
                .join("runs")
                .join(id.to_string())
                .join("journal.md");
            if !file.is_file() {
                continue;
            }
            found.add(known, format!("run:{id}"), stamp(&file), || {
                Some(Doc {
                    title: format!("run {id} · {ticket} · {title}"),
                    locator: format!("run {id} ({})", file.display()),
                    at: modified(&file),
                    body: fs::read_to_string(&file).ok()?,
                    ..Doc::default()
                })
            });
        }
    }
    if has_table(&conn, "team_runs")? {
        let mut query = conn.prepare("SELECT run, data, updated FROM team_runs")?;
        let rows: Vec<(i64, String, i64)> = query
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (run, data, updated) in rows {
            let Ok(team) = serde_json::from_str::<Value>(&data) else {
                continue;
            };
            if team["target"]["project"] != project.id.as_str() {
                continue;
            }
            found.add(known, format!("team:{run}"), updated.to_string(), || {
                Some(Doc {
                    title: format!(
                        "team of run {run} · {} · {}",
                        team["target"]["ticket"].as_str().unwrap_or_default(),
                        team["target"]["title"].as_str().unwrap_or_default()
                    ),
                    locator: format!("team run {run}"),
                    at: updated,
                    body: team_text(&team),
                    ..Doc::default()
                })
            });
        }
    }
    Ok(())
}

/// The plan and what each step reported or was told, as text.
fn team_text(team: &Value) -> String {
    let mut out = String::new();
    let line = |out: &mut String, label: &str, value: &Value| {
        if let Some(text) = value.as_str().filter(|t| !t.trim().is_empty()) {
            out.push_str(&format!("{label}: {text}\n"));
        }
    };
    let plan = &team["plan"];
    line(&mut out, "plan", &plan["summary"]);
    for (key, label) in [
        ("acceptance", "criterion"),
        ("out_of_scope", "out of scope"),
        ("questions", "question"),
    ] {
        for item in plan[key].as_array().into_iter().flatten() {
            line(&mut out, label, item.get("text").unwrap_or(item));
        }
    }
    for task in plan["tasks"].as_array().into_iter().flatten() {
        let label = format!(
            "task {} ({})",
            task["id"].as_str().unwrap_or_default(),
            task["role"].as_str().unwrap_or_default()
        );
        line(&mut out, &label, &task["goal"]);
    }
    for step in team["steps"].as_array().into_iter().flatten() {
        let key = step["key"].as_str().unwrap_or_default();
        line(
            &mut out,
            &format!("step {key} report"),
            &step["report"]["summary"],
        );
        for question in step["report"]["questions"].as_array().into_iter().flatten() {
            line(&mut out, &format!("step {key} question"), question);
        }
        line(&mut out, &format!("step {key} feedback"), &step["feedback"]);
        line(&mut out, &format!("step {key} note"), &step["note"]);
    }
    out
}

/// TermF's headless conversations of the project: what the user said and what the agent answered.
fn agents(
    found: &mut Found,
    known: &HashMap<String, String>,
    paths: &Paths,
    project: &Project,
    repos: &[PathBuf],
) -> Result<()> {
    let Some(conn) = open_db(paths)? else {
        return Ok(());
    };
    if !has_table(&conn, "agent_sessions")? || !has_table(&conn, "agent_events")? {
        return Ok(());
    }
    let mut query = conn.prepare("SELECT id, spec, updated FROM agent_sessions")?;
    let rows: Vec<(i64, String, i64)> = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, spec, updated) in rows {
        let Ok(spec) = serde_json::from_str::<Value>(&spec) else {
            continue;
        };
        let ours = match spec["project"].as_str() {
            Some(p) => p == project.id,
            None => spec["cwd"]
                .as_str()
                .is_some_and(|cwd| repos.iter().any(|r| Path::new(cwd).starts_with(r))),
        };
        if !ours {
            continue;
        }
        found.add(known, format!("agent:{id}"), updated.to_string(), || {
            let mut events = conn
                .prepare(
                    "SELECT kind, data FROM agent_events
                     WHERE session = ?1 AND kind IN ('user', 'text') ORDER BY id",
                )
                .ok()?;
            let mut body = String::new();
            let rows = events
                .query_map([id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .ok()?;
            for (kind, data) in rows.flatten() {
                let Ok(event) = serde_json::from_str::<Value>(&data) else {
                    continue;
                };
                // A subagent's words are its own, not the conversation's.
                if !event["parent"].is_null() {
                    continue;
                }
                let who = if kind == "user" { "you" } else { "agent" };
                if let Some(text) = event["text"].as_str() {
                    body.push_str(&format!("{who}: {text}\n\n"));
                }
            }
            let role = spec["role"]
                .as_str()
                .map(|r| format!(" · {r}"))
                .unwrap_or_default();
            let title = spec["title"].as_str().unwrap_or_default();
            Some(Doc {
                title: format!("{}{role}", cut(title, 80)),
                locator: format!("TermF conversation {id}"),
                at: updated,
                body,
                ..Doc::default()
            })
        });
    }
    Ok(())
}
