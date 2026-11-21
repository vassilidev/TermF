//! Claude Code's workspace trust ("Is this a project you trust?") for a project's repositories, only on the
//! owner's explicit choice. The documented way to trust a folder in advance is
//! `projects["<repository root>"].hasTrustDialogAccepted` in `~/.claude.json`
//! (code.claude.com/docs/en/permissions, "What runs before you trust a folder"); a worktree inherits the trust
//! of its main checkout. Claude Code writes that file itself while it runs, so TermF changes only those keys,
//! keeps everything else as written, never rewrites a file it cannot parse, and gives up rather than overwrite a
//! write that happened since it read the file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::settings::{indent, pretty};

const KEY: &str = "hasTrustDialogAccepted";
const ATTEMPTS: usize = 3;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepoTrust {
    pub name: String,
    pub path: String,
    /// The folder Claude Code keys the trust on: the main checkout's root.
    pub root: String,
    pub trusted: bool,
    /// What trusting it loads: `permissions.allow` rules and additional directories of its shared settings.
    pub allow: usize,
    /// Servers of its `.mcp.json`, connected without asking once trusted.
    pub mcp: usize,
}

/// `$CLAUDE_CONFIG_DIR/.claude.json`, else `~/.claude.json`.
pub fn file() -> PathBuf {
    match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir).join(".claude.json"),
        None => termf_core::expand_tilde("~/.claude.json"),
    }
}

/// The repository root Claude Code keys the trust on: the main checkout for a worktree or a sub-folder, the
/// folder itself outside git.
pub fn root(path: &Path) -> PathBuf {
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .ok()?;
        let text = String::from_utf8(out.stdout).ok()?;
        (out.status.success() && !text.trim().is_empty()).then(|| PathBuf::from(text.trim()))
    };
    if let Some(common) = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"])
        && common.file_name().is_some_and(|n| n == ".git")
        && let Some(main) = common.parent()
    {
        return main.to_path_buf();
    }
    // A submodule's git folder lives in its parent's `.git/modules`: its own top level is the root.
    if let Some(top) = git(&["rev-parse", "--show-toplevel"]) {
        return top;
    }
    // Claude Code writes resolved paths (`/private/tmp` on macOS); Windows' canonical form (`\\?\C:\`) is not one.
    if cfg!(windows) {
        path.to_path_buf()
    } else {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }
}

fn read(file: &Path) -> Result<Option<String>> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", file.display())),
    }
}

fn parse(file: &Path, text: Option<&str>) -> Result<Map<String, Value>> {
    match text.map(str::trim) {
        None | Some("") => Ok(Map::new()),
        Some(text) => match serde_json::from_str(text) {
            Ok(Value::Object(map)) => Ok(map),
            _ => bail!(
                "{} is not valid JSON: TermF leaves it alone, fix it first",
                file.display()
            ),
        },
    }
}

fn trusted(config: &Map<String, Value>, root: &Path) -> bool {
    config
        .get("projects")
        .and_then(|p| p.get(root.to_string_lossy().as_ref()))
        .and_then(|e| e.get(KEY))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn count(file: &Path, pointers: &[&str]) -> usize {
    let Ok(text) = fs::read_to_string(file) else {
        return 0;
    };
    let value: Value = serde_json::from_str(&text).unwrap_or_default();
    pointers
        .iter()
        .filter_map(|p| value.pointer(p))
        .map(|v| v.as_array().map_or(0, Vec::len) + v.as_object().map_or(0, Map::len))
        .sum()
}

/// Each repository's trust as `file` holds it.
pub fn state(file: &Path, repos: &[(String, PathBuf)]) -> Result<Vec<RepoTrust>> {
    let config = parse(file, read(file)?.as_deref())?;
    Ok(repos
        .iter()
        .map(|(name, path)| {
            let root = root(path);
            RepoTrust {
                name: name.clone(),
                path: path.to_string_lossy().into_owned(),
                trusted: trusted(&config, &root),
                root: root.to_string_lossy().into_owned(),
                allow: count(
                    &path.join(".claude/settings.json"),
                    &["/permissions/allow", "/permissions/additionalDirectories"],
                ),
                mcp: count(&path.join(".mcp.json"), &["/mcpServers"]),
            }
        })
        .collect())
}

/// Sets or clears the trust of `roots`; a root with no entry has nothing to clear. Returns whether it changed
/// anything.
fn apply(config: &mut Map<String, Value>, roots: &[PathBuf], on: bool) -> Result<bool> {
    let home = termf_core::expand_tilde("~");
    let projects = config
        .entry("projects")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .context("`projects` is not an object")?;
    let mut changed = false;
    for root in roots {
        // Claude Code never keeps the trust of the home folder, so writing it would only mislead.
        if on && *root == home {
            continue;
        }
        let key = root.to_string_lossy().into_owned();
        if !on && !projects.contains_key(&key) {
            continue;
        }
        let entry = projects
            .entry(key)
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .with_context(|| format!("the entry of {} is not an object", root.display()))?;
        if entry.get(KEY).and_then(Value::as_bool) != Some(on) {
            entry.insert(KEY.into(), on.into());
            changed = true;
        }
    }
    Ok(changed)
}

/// Trusts (`on`) or stops trusting the folders Claude Code keys `roots` on.
pub fn set(file: &Path, roots: &[PathBuf], on: bool) -> Result<()> {
    set_with(file, roots, on, &mut || {})
}

/// `before_replace` runs between the last read and the replacement, where a concurrent write would land.
fn set_with(
    file: &Path,
    roots: &[PathBuf],
    on: bool,
    before_replace: &mut dyn FnMut(),
) -> Result<()> {
    // A linked file (dotfiles) is written through, so the link stays.
    let file = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let dir = file.parent().context("no parent folder")?;
    let tmp = dir.join(".claude.json.termf-tmp");
    for _ in 0..ATTEMPTS {
        let before = read(&file)?;
        let mut config = parse(&file, before.as_deref())?;
        if !apply(&mut config, roots, on)? {
            return Ok(());
        }
        let text = pretty(
            &Value::Object(config),
            indent(before.as_deref().unwrap_or("")),
        )?;
        fs::create_dir_all(dir)?;
        fs::write(&tmp, &text).with_context(|| format!("cannot write {}", tmp.display()))?;
        match fs::metadata(&file) {
            Ok(meta) => fs::set_permissions(&tmp, meta.permissions())?,
            Err(_) => private(&tmp)?,
        }
        before_replace();
        // Claude Code rewrites the file while it runs: what it wrote since the read wins, and TermF reads again.
        if read(&file)? != before {
            let _ = fs::remove_file(&tmp);
            continue;
        }
        return fs::rename(&tmp, &file).with_context(|| format!("cannot write {}", file.display()));
    }
    bail!(
        "{} kept changing while TermF wrote it: try again in a moment",
        file.display()
    )
}

#[cfg(unix)]
fn private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::set_permissions(
        path,
        fs::Permissions::from_mode(0o600),
    )?)
}

#[cfg(not(unix))]
fn private(_: &Path) -> Result<()> {
    Ok(())
}

fn repos(config_dir: &Path, id: &str) -> Result<Vec<(String, PathBuf)>> {
    termf_core::project_file(config_dir, id)?;
    let project = termf_core::load_projects(config_dir)?
        .into_iter()
        .find(|p| p.id == id)
        .with_context(|| format!("unknown project {id}"))?;
    Ok(project
        .repos
        .into_iter()
        .map(|r| (r.name, PathBuf::from(r.path)))
        .collect())
}

/// The trust of each repository of project `id`.
pub fn project(config_dir: &Path, id: &str) -> Result<Vec<RepoTrust>> {
    state(&file(), &repos(config_dir, id)?)
}

/// Trusts, or stops trusting, every repository of project `id` (and so their worktrees).
pub fn set_project(config_dir: &Path, id: &str, on: bool) -> Result<Vec<RepoTrust>> {
    let repos = repos(config_dir, id)?;
    let roots: Vec<PathBuf> = repos.iter().map(|(_, path)| root(path)).collect();
    set(&file(), &roots, on)?;
    state(&file(), &repos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("termf-trust-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const CONFIG: &str = "{\n    \"numStartups\": 3,\n    \"projects\": {\n        \"/w/api\": {\n            \"allowedTools\": [],\n            \"lastCost\": 1.68\n        }\n    },\n    \"userID\": \"abc\"\n}";

    #[test]
    fn trusts_keeping_every_other_key_order_and_indentation() {
        let dir = temp("keep");
        let file = dir.join(".claude.json");
        fs::write(&file, CONFIG).unwrap();
        set(
            &file,
            &[PathBuf::from("/w/api"), PathBuf::from("/w/front")],
            true,
        )
        .unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert_eq!(
            text,
            "{\n    \"numStartups\": 3,\n    \"projects\": {\n        \"/w/api\": {\n            \"allowedTools\": [],\n            \"lastCost\": 1.68,\n            \"hasTrustDialogAccepted\": true\n        },\n        \"/w/front\": {\n            \"hasTrustDialogAccepted\": true\n        }\n    },\n    \"userID\": \"abc\"\n}\n"
        );
        // Trusting again changes nothing, not even the file.
        let modified = fs::metadata(&file).unwrap().modified().unwrap();
        set(&file, &[PathBuf::from("/w/api")], true).unwrap();
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);

        set(
            &file,
            &[PathBuf::from("/w/api"), PathBuf::from("/w/gone")],
            false,
        )
        .unwrap();
        let config: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(config["projects"]["/w/api"][KEY], false);
        assert_eq!(config["projects"]["/w/front"][KEY], true);
        assert!(config["projects"].get("/w/gone").is_none());
        assert!(!dir.join(".claude.json.termf-tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn creates_a_private_file_and_keeps_an_existing_one_s_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("mode");
        let file = dir.join(".claude.json");
        set(&file, &[PathBuf::from("/w/api")], true).unwrap();
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let config: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(config["projects"]["/w/api"][KEY], true);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        set(&file, &[PathBuf::from("/w/front")], true).unwrap();
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    fn never_rewrites_a_file_it_cannot_parse() {
        let dir = temp("invalid");
        let file = dir.join(".claude.json");
        for text in ["{ \"projects\": ", "[1, 2]", "{\"projects\": 3}"] {
            fs::write(&file, text).unwrap();
            assert!(
                set(&file, &[PathBuf::from("/w/api")], true).is_err(),
                "{text}"
            );
            assert_eq!(fs::read_to_string(&file).unwrap(), text);
        }
        fs::write(&file, "{ \"projects\": ").unwrap();
        assert!(state(&file, &[("api".into(), dir.clone())]).is_err());
    }

    #[test]
    fn a_concurrent_write_wins_and_the_trust_is_added_on_top_of_it() {
        let dir = temp("concurrent");
        let file = dir.join(".claude.json");
        fs::write(&file, CONFIG).unwrap();
        let mut writes = 0;
        set_with(&file, &[PathBuf::from("/w/api")], true, &mut || {
            if writes == 0 {
                fs::write(&file, "{\"numStartups\": 4, \"projects\": {}}").unwrap();
            }
            writes += 1;
        })
        .unwrap();
        assert_eq!(writes, 2);
        let config: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(config["numStartups"], 4);
        assert_eq!(config["projects"]["/w/api"][KEY], true);

        // A file that never stops changing is left with its last content, and the call fails.
        let mut n = 0;
        let result = set_with(&file, &[PathBuf::from("/w/front")], true, &mut || {
            n += 1;
            fs::write(&file, format!("{{\"numStartups\": {n}}}")).unwrap();
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "{\"numStartups\": 3}");
        assert!(!dir.join(".claude.json.termf-tmp").exists());
    }

    #[test]
    fn keys_the_trust_on_the_main_checkout_and_counts_what_trust_loads() {
        let dir = temp("root");
        let repo = dir.join("api");
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "{args:?}");
        };
        fs::create_dir_all(repo.join("sub")).unwrap();
        git(&["init", "-q", "-b", "main"]);
        git(&[
            "-c",
            "user.email=a@b",
            "-c",
            "user.name=a",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ]);
        let wt = repo.join(".claude/worktrees/termf-1-x");
        git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "termf/x",
            wt.to_str().unwrap(),
        ]);
        let main = root(&repo);
        assert_eq!(root(&repo.join("sub")), main);
        assert_eq!(root(&wt), main);
        // Windows' canonical form (`\\?\C:\`) is not git's (`C:/`).
        #[cfg(unix)]
        {
            assert_eq!(main, fs::canonicalize(&repo).unwrap());
            assert_eq!(root(&dir), fs::canonicalize(&dir).unwrap());
        }

        fs::create_dir_all(repo.join(".claude")).unwrap();
        fs::write(
            repo.join(".claude/settings.json"),
            "{\"permissions\": {\"allow\": [\"Bash(npm test)\", \"Edit\"], \"additionalDirectories\": [\"../x\"]}}",
        )
        .unwrap();
        fs::write(repo.join(".mcp.json"), "{\"mcpServers\": {\"db\": {}}}").unwrap();
        let file = dir.join(".claude.json");
        set(&file, std::slice::from_ref(&main), true).unwrap();
        let found = state(&file, &[("api".into(), repo.clone())]).unwrap();
        assert_eq!(
            found,
            [RepoTrust {
                name: "api".into(),
                path: repo.to_string_lossy().into_owned(),
                root: main.to_string_lossy().into_owned(),
                trusted: true,
                allow: 3,
                mcp: 1,
            }]
        );
    }

    /// The terminal's text without escape sequences, spaces collapsed.
    fn plain(raw: &[u8]) -> String {
        let text = String::from_utf8_lossy(raw);
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '\x1b' {
                out.push(if c.is_whitespace() { ' ' } else { c });
                continue;
            }
            out.push(' ');
            match chars.next() {
                Some('[') => while chars.next().is_some_and(|c| !('@'..='~').contains(&c)) {},
                Some(']') => while chars.next().is_some_and(|c| c != '\x07' && c != '\x1b') {},
                _ => {}
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// What the installed Claude Code shows when it starts in `cwd`, until its prompt or its trust question.
    fn screen(config: &Path, cwd: &Path) -> String {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::io::Read;
        use std::sync::{Arc, Mutex};
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new("claude");
        cmd.cwd(cwd);
        cmd.env("CLAUDE_CONFIG_DIR", config);
        cmd.env(
            "ANTHROPIC_API_KEY",
            format!("sk-ant-api03-{}", "x".repeat(80)),
        );
        cmd.env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
        cmd.env("DISABLE_AUTOUPDATER", "1");
        cmd.env("TERM", "xterm-256color");
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        std::thread::spawn(move || {
            let mut buf = [0; 8192];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut text = String::new();
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(500));
            text = plain(&seen.lock().unwrap());
            if text.contains("trust this folder") || text.contains("for shortcuts") {
                break;
            }
        }
        let _ = child.kill();
        text
    }

    /// `cargo test -p termf-claude -- --ignored real_claude_trust`: the installed Claude Code, in a throwaway
    /// `CLAUDE_CONFIG_DIR` with a dummy API key (no prompt is ever sent), asks for trust in a new repository and no
    /// longer does once TermF trusted it, in the repository, a sub-folder and a worktree of it.
    #[test]
    #[ignore]
    fn real_claude_trust() {
        let dir = temp("real");
        let config = dir.join("claude");
        let repo = dir.join("api");
        fs::create_dir_all(repo.join("sub")).unwrap();
        fs::create_dir_all(&config).unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git").arg("-C").arg(&repo).args(args).output();
            assert!(out.unwrap().status.success(), "{args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&[
            "-c",
            "user.email=a@b",
            "-c",
            "user.name=a",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ]);
        let wt = repo.join(".claude/worktrees/termf-1-x");
        git(&[
            "worktree",
            "add",
            "-q",
            "-b",
            "termf/x",
            wt.to_str().unwrap(),
        ]);
        let file = config.join(".claude.json");
        fs::write(
            &file,
            r#"{"hasCompletedOnboarding": true, "customApiKeyResponses": {"approved": ["xxxxxxxxxxxxxxxxxxxx"], "rejected": []}}"#,
        )
        .unwrap();

        let before = screen(&config, &repo);
        assert!(before.contains("trust this folder"), "{before}");
        set(&file, &[root(&repo)], true).unwrap();
        for cwd in [&repo, &repo.join("sub"), &wt] {
            let after = screen(&config, cwd);
            assert!(
                after.contains("for shortcuts") && !after.contains("trust this folder"),
                "{}: {after}",
                cwd.display()
            );
        }
        set(&file, &[root(&repo)], false).unwrap();
        let revoked = screen(&config, &repo);
        assert!(revoked.contains("trust this folder"), "{revoked}");
    }
}
