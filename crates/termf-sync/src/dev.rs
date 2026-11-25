//! The dev config scope, only on explicit request: what it takes to code the same way on another machine.
//! Claude Code's user config (`CLAUDE.md`, `settings.json` with its plugins and marketplaces, `keybindings.json`,
//! skills, agents, commands, output styles, hooks), its auto memory of the projects under the home folder, the
//! MCP servers of `~/.claude.json` (that key only: the rest is the account, the history and each project's
//! state), `~/.gitconfig` and the global excludes file it names in the home folder. Nothing else of `~/.claude`
//! travels: never the credentials, the transcripts, the caches nor the plugins' copies. Links are neither
//! followed nor written through. In what is synced, this machine's home folder is `$TERMF_HOME`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value};
use termf_library::archive;

use crate::Skipped;
use crate::local;
use crate::record::Entry;

const PREFIX: &str = "dev/";
const HOME: &str = "$TERMF_HOME/";
const CLAUDE: &str = "dev/claude/";
const CLAUDE_FILES: [&str; 3] = ["CLAUDE.md", "settings.json", "keybindings.json"];
const CLAUDE_DIRS: [&str; 5] = ["skills", "agents", "commands", "output-styles", "hooks"];
/// The memory of the home folder itself, as a project.
const HOME_PROJECT: &str = "~";
const MCP: &str = "dev/claude.json";
const GITCONFIG: &str = "dev/gitconfig";
const GITIGNORE: &str = "dev/gitignore";

pub fn is_dev(path: &str) -> bool {
    path.starts_with(PREFIX)
}

/// A path of this scope the remote may name: one [`read`] could give.
pub fn valid(path: &str) -> bool {
    let names =
        |rest: &str| !rest.contains(['\\', ':']) && rest.split('/').all(|n| !archive::private(n));
    if [MCP, GITCONFIG, GITIGNORE].contains(&path) {
        return true;
    }
    let Some(rest) = path.strip_prefix(CLAUDE) else {
        return false;
    };
    match rest.split_once('/') {
        None => CLAUDE_FILES.contains(&rest),
        Some(("memory", rest)) => rest.split_once('/').is_some_and(|(project, file)| {
            (project == HOME_PROJECT
                || !project.is_empty()
                    && project
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-'))
                && names(file)
        }),
        Some((dir, rest)) => CLAUDE_DIRS.contains(&dir) && names(rest),
    }
}

/// Claude Code's folder name for a project: every character but ASCII letters and digits becomes `-`.
fn encode(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Whether a folder under `dir` has `encoded` as its encoded path relative to `dir`.
fn exists_encoded(dir: &Path, encoded: &str) -> bool {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            let name = encode(Path::new(&entry.file_name()));
            entry.path().is_dir()
                && (encoded == name
                    || encoded
                        .strip_prefix(&name)
                        .and_then(|rest| rest.strip_prefix('-'))
                        .is_some_and(|rest| exists_encoded(&entry.path(), rest)))
        })
}

/// The excludes file `~/.gitconfig` names, relative to the home folder; `None` outside of it.
fn excludes_file(home: &Path, git_env: &[(String, String)]) -> Option<String> {
    let out = local::git(git_env)
        .arg("config")
        .arg("--file")
        .arg(home.join(".gitconfig"))
        .args(["--get", "core.excludesFile"])
        .output()
        .ok()?;
    let value = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    let rel = match value.strip_prefix("~/") {
        Some(rest) => rest.to_owned(),
        None => Path::new(&value)
            .strip_prefix(home)
            .ok()?
            .to_str()?
            .replace('\\', "/"),
    };
    rel.split('/')
        .all(|n| !n.is_empty() && n != "." && n != "..")
        .then_some(rel)
}

/// Where `path` is on this machine, as a folder and a path below it that no link may cross; `None` when it has
/// no place here: the memory of a project that is not at the same place under the home folder, an excludes file
/// `~/.gitconfig` does not name there.
fn place(home: &Path, path: &str, git_env: &[(String, String)]) -> Option<(PathBuf, String)> {
    let claude = home.join(".claude");
    match path {
        GITCONFIG => Some((home.to_path_buf(), ".gitconfig".into())),
        GITIGNORE => Some((home.to_path_buf(), excludes_file(home, git_env)?)),
        MCP => Some((home.to_path_buf(), ".claude.json".into())),
        _ => {
            let rest = path.strip_prefix(CLAUDE)?;
            let Some(memory) = rest.strip_prefix("memory/") else {
                return Some((claude, rest.into()));
            };
            let (project, file) = memory.split_once('/')?;
            let folder = match project {
                HOME_PROJECT => encode(home),
                project => format!("{}-{project}", encode(home)),
            };
            // Claude Code cuts and hashes a longer name: it could not be found again.
            let here = project == HOME_PROJECT
                || claude.join("projects").join(&folder).is_dir()
                || exists_encoded(home, project);
            (folder.len() <= 200 && here)
                .then(|| (claude, format!("projects/{folder}/memory/{file}")))
        }
    }
}

/// Whether `path` has a place on this machine that no link crosses: if not, it is left aside, neither sent nor
/// taken.
pub fn placed(home: &Path, path: &str, git_env: &[(String, String)]) -> bool {
    place(home, path, git_env).is_some_and(|(root, rel)| !archive::through_link(&root, &rel))
}

/// `bytes` with this machine's home folder as `$TERMF_HOME`, so its paths work on another machine.
pub fn portable(home: &Path, bytes: Vec<u8>) -> Vec<u8> {
    swap(bytes, &home_prefix(home), HOME)
}

/// The reverse of [`portable`].
pub fn localize(home: &Path, bytes: Vec<u8>) -> Vec<u8> {
    swap(bytes, HOME, &home_prefix(home))
}

fn home_prefix(home: &Path) -> String {
    format!("{}/", home.to_string_lossy().trim_end_matches('/'))
}

/// Only in text: a binary file travels as it is.
fn swap(bytes: Vec<u8>, from: &str, to: &str) -> Vec<u8> {
    match String::from_utf8(bytes) {
        Ok(text) if text.contains(from) => text.replace(from, to).into_bytes(),
        Ok(text) => text.into_bytes(),
        Err(e) => e.into_bytes(),
    }
}

/// The files of `dir` and below as `(synced path, file)`, without hidden ones nor links.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
    if !fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type()?;
        let path = format!("{prefix}/{name}");
        if archive::private(&name) {
            continue;
        } else if kind.is_dir() {
            walk(&entry.path(), &path, out)?;
        } else if kind.is_file() {
            out.push((path, entry.path()));
        }
    }
    Ok(())
}

/// The files of the scope as they are synced. What cannot be read is skipped (a folder: all it holds), so it
/// is never taken as deleted.
pub fn read(home: &Path, git_env: &[(String, String)]) -> (BTreeMap<String, Entry>, Vec<Skipped>) {
    let (mut found, mut skipped) = (BTreeMap::new(), Vec::new());
    let skip = |skipped: &mut Vec<Skipped>, path: &str, e: &dyn std::fmt::Display| {
        skipped.push(Skipped {
            path: path.into(),
            error: e.to_string(),
        })
    };
    let claude = home.join(".claude");
    let mut files: Vec<(String, PathBuf)> = CLAUDE_FILES
        .iter()
        .map(|name| (format!("{CLAUDE}{name}"), claude.join(name)))
        .chain([(GITCONFIG.into(), home.join(".gitconfig"))])
        .chain(excludes_file(home, git_env).map(|rel| (GITIGNORE.into(), home.join(rel))))
        .collect();
    let mut folders: Vec<(String, PathBuf)> = CLAUDE_DIRS
        .iter()
        .map(|dir| (format!("{CLAUDE}{dir}"), claude.join(dir)))
        .collect();
    let projects = claude.join("projects");
    let ours = encode(home);
    if fs::symlink_metadata(&projects).is_ok_and(|m| m.is_dir()) {
        match fs::read_dir(&projects) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let project = match name.strip_prefix(&ours) {
                        Some("") => HOME_PROJECT,
                        Some(rest) => match rest.strip_prefix('-') {
                            Some(project) if !project.is_empty() => project,
                            _ => continue,
                        },
                        None => continue,
                    };
                    if entry.file_type().is_ok_and(|t| t.is_dir()) {
                        folders.push((
                            format!("{CLAUDE}memory/{project}"),
                            entry.path().join("memory"),
                        ));
                    }
                }
            }
            Err(e) => skip(&mut skipped, &format!("{CLAUDE}memory"), &e),
        }
    }
    for (prefix, dir) in folders {
        if let Err(e) = walk(&dir, &prefix, &mut files) {
            skip(&mut skipped, &prefix, &e);
        }
    }
    for (path, file) in files {
        match fs::symlink_metadata(&file) {
            Ok(meta) if meta.is_file() => match fs::read(&file) {
                Ok(bytes) => {
                    let exec = local::executable(&file).unwrap_or(false);
                    let bytes = portable(home, bytes);
                    found.insert(path, Entry { bytes, exec });
                }
                Err(e) => skip(&mut skipped, &path, &e),
            },
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => skip(&mut skipped, &path, &e),
            _ => {}
        }
    }
    match read_mcp(home) {
        Ok(Some(bytes)) => _ = found.insert(MCP.into(), Entry { bytes, exec: false }),
        Ok(None) => {}
        Err(e) => skip(&mut skipped, MCP, &format!("{e:#}")),
    }
    (found, skipped)
}

/// The `mcpServers` of `~/.claude.json`, alone in a JSON object.
fn read_mcp(home: &Path) -> Result<Option<Vec<u8>>> {
    let file = home.join(".claude.json");
    if !fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) {
        return Ok(None);
    }
    let doc: Value = serde_json::from_slice(&fs::read(&file)?).context("invalid ~/.claude.json")?;
    let Some(servers) = doc.get("mcpServers") else {
        return Ok(None);
    };
    let synced = serde_json::to_vec_pretty(&serde_json::json!({ "mcpServers": servers }))?;
    Ok(Some(portable(home, synced)))
}

/// Writes (or deletes) a file of the scope where it goes on this machine.
pub fn write(
    home: &Path,
    path: &str,
    entry: Option<&Entry>,
    git_env: &[(String, String)],
) -> Result<()> {
    ensure!(valid(path), "cannot write {path}");
    let entry = entry.map(|e| Entry {
        bytes: localize(home, e.bytes.clone()),
        exec: e.exec,
    });
    if path == MCP {
        return write_mcp(home, entry.as_ref());
    }
    let (root, rel) = place(home, path, git_env)
        .with_context(|| format!("{path} has no place on this machine"))?;
    local::write_under(&root, &rel, entry.as_ref())
}

/// Sets (or removes) `mcpServers` in `~/.claude.json`; every other key stays as it is, in its order.
fn write_mcp(home: &Path, entry: Option<&Entry>) -> Result<()> {
    let file = home.join(".claude.json");
    ensure!(
        !file.is_symlink(),
        "cannot write ~/.claude.json through a link"
    );
    let mut doc: Map<String, Value> = match fs::read(&file) {
        Ok(bytes) => serde_json::from_slice(&bytes).context("invalid ~/.claude.json")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(e) => return Err(e.into()),
    };
    match entry {
        Some(e) => {
            let synced: Value = serde_json::from_slice(&e.bytes).context("invalid MCP servers")?;
            let servers = synced
                .get("mcpServers")
                .filter(|s| s.is_object())
                .context("invalid MCP servers")?;
            doc.insert("mcpServers".into(), servers.clone());
        }
        None => _ = doc.remove("mcpServers"),
    }
    // It holds the account: readable by the user only, as Claude Code keeps it.
    let tmp = home.join(".claude.json.termf-sync.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    std::io::Write::write_all(&mut options.open(&tmp)?, &serde_json::to_vec_pretty(&doc)?)?;
    fs::rename(&tmp, &file).with_context(|| format!("cannot write {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_only_what_read_gives() {
        for ok in [
            "dev/claude/CLAUDE.md",
            "dev/claude/settings.json",
            "dev/claude/skills/review/SKILL.md",
            "dev/claude/hooks/fmt.sh",
            "dev/claude/memory/code-acme/MEMORY.md",
            "dev/claude/memory/~/MEMORY.md",
            "dev/claude.json",
            "dev/gitconfig",
            "dev/gitignore",
        ] {
            assert!(valid(ok), "{ok}");
        }
        for bad in [
            "dev/claude/.credentials.json",
            "dev/claude/todos/x.json",
            "dev/claude/plugins/cache/x",
            "dev/claude/projects/-x/abc.jsonl",
            "dev/claude/skills/../../.ssh/id_ed25519",
            "dev/claude/skills/a/.env",
            "dev/claude/memory/../x/MEMORY.md",
            "dev/claude/memory/a.b/MEMORY.md",
            "dev/claude/memory/x",
            "dev/claude/skills/a\\b",
            "dev/bashrc",
        ] {
            assert!(!valid(bad), "{bad}");
        }
    }

    #[test]
    fn home_paths_travel_as_a_mark() {
        let (a, b) = (Path::new("/Users/alex"), Path::new("/home/alex/"));
        let text =
            b"\"command\": \"/Users/alex/.claude/hooks/fmt.sh\", \"x\": \"/Users/alexandra/y\""
                .to_vec();
        let synced = portable(a, text);
        assert_eq!(
            String::from_utf8_lossy(&synced),
            "\"command\": \"$TERMF_HOME/.claude/hooks/fmt.sh\", \"x\": \"/Users/alexandra/y\""
        );
        assert_eq!(
            String::from_utf8(localize(b, synced)).unwrap(),
            "\"command\": \"/home/alex/.claude/hooks/fmt.sh\", \"x\": \"/Users/alexandra/y\""
        );
        let binary = vec![0xff, b'/', b'U'];
        assert_eq!(portable(a, binary.clone()), binary);
    }
}
