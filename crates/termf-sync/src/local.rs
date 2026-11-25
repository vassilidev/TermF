//! The synced files as this machine has them. Projects travel with each repository's `remote` and the path it
//! had where it was added; `local.toml`, never synced, maps a project's remote to this machine's own path when it
//! differs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};
use termf_library::archive::{self, MissingRepo};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::record::{Entry, Record};
use crate::{Scope, Skipped};

const LOCAL: &str = "local.toml";

/// (project id, git remote) → this machine's path for that repository.
pub type Mapping = BTreeMap<(String, String), String>;

pub fn scope_of(path: &str) -> Scope {
    match path.split('/').collect::<Vec<_>>().as_slice() {
        ["projects", _, "conventions", ..] => Scope::Conventions,
        ["projects", ..] => Scope::Projects,
        ["vault", ..] => Scope::Vault,
        ["ssh", ..] => Scope::SshKeys,
        ["dev", ..] => Scope::DevConfig,
        _ => Scope::Config,
    }
}

/// A path of the config folder the remote may name: one the sync carries, which cannot leave the folder on any
/// system.
pub fn valid(path: &str) -> bool {
    archive::synced(path) && !path.contains(['\\', ':'])
}

fn project_id(path: &str) -> Option<&str> {
    path.strip_prefix("projects/")?
        .strip_suffix(".toml")
        .filter(|id| !id.contains('/'))
}

pub fn load_mapping(config_dir: &Path) -> Result<Mapping> {
    let file = config_dir.join(LOCAL);
    let Ok(text) = fs::read_to_string(&file) else {
        return Ok(Mapping::new());
    };
    let table: toml::Table =
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
    let repos = table.get("repos").and_then(toml::Value::as_table);
    Ok(repos
        .into_iter()
        .flatten()
        .filter_map(|(project, remotes)| Some((project, remotes.as_table()?)))
        .flat_map(|(project, remotes)| {
            remotes.iter().filter_map(move |(remote, path)| {
                Some(((project.clone(), remote.clone()), path.as_str()?.to_owned()))
            })
        })
        .collect())
}

/// Rewrites only `[repos]`: anything else in the file stays as written.
pub fn save_mapping(config_dir: &Path, mapping: &Mapping) -> Result<()> {
    let file = config_dir.join(LOCAL);
    let mut doc: DocumentMut = match fs::read_to_string(&file) {
        Ok(text) => text
            .parse()
            .with_context(|| format!("invalid {}", file.display()))?,
        Err(_) => "# This machine's repository paths, by project and git remote. Never synced.\n"
            .parse()?,
    };
    let mut repos = Table::new();
    repos.set_implicit(true);
    for ((project, remote), path) in mapping {
        let project = repos.entry(project).or_insert(Item::Table(Table::new()));
        project[remote.as_str()] = value(path.as_str());
    }
    doc["repos"] = Item::Table(repos);
    termf_core::write_atomic(&file, &doc.to_string())
}

fn same_place(a: &str, b: &str) -> bool {
    termf_core::expand_tilde(a) == termf_core::expand_tilde(b)
}

/// Each repository the last synced version knows (by remote) keeps that version's path; this machine's own
/// path goes to `mapping`.
fn to_synced(
    project: &str,
    bytes: Vec<u8>,
    base: Option<&Record>,
    mapping: &mut Mapping,
) -> Result<Vec<u8>> {
    let hints: BTreeMap<String, String> = base
        .and_then(|r| r.entry.as_ref())
        .and_then(|e| toml::from_str::<toml::Table>(std::str::from_utf8(&e.bytes).ok()?).ok())
        .and_then(|t| t.get("repos")?.as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|repo| {
            let field = |k| repo.get(k)?.as_str().map(str::to_owned);
            Some((field("remote")?, field("path")?))
        })
        .collect();
    let mut doc: DocumentMut = String::from_utf8(bytes)?.parse()?;
    if let Some(repos) = doc.get_mut("repos").and_then(Item::as_array_of_tables_mut) {
        for repo in repos.iter_mut() {
            let field = |k| repo.get(k).and_then(Item::as_str).map(str::to_owned);
            let (Some(remote), Some(path)) = (field("remote"), field("path")) else {
                continue;
            };
            let key = (project.to_owned(), remote);
            match hints.get(&key.1) {
                Some(hint) if !same_place(hint, &path) => {
                    repo["path"] = value(hint.as_str());
                    mapping.insert(key, path);
                }
                _ => {
                    mapping.remove(&key);
                }
            }
        }
    }
    Ok(doc.to_string().into_bytes())
}

fn to_local(project: &str, bytes: &[u8], mapping: &Mapping) -> Vec<u8> {
    let Some(mut doc) = std::str::from_utf8(bytes)
        .ok()
        .and_then(|t| t.parse::<DocumentMut>().ok())
    else {
        return bytes.to_vec();
    };
    if let Some(repos) = doc.get_mut("repos").and_then(Item::as_array_of_tables_mut) {
        for repo in repos.iter_mut() {
            let field = |k| repo.get(k).and_then(Item::as_str).map(str::to_owned);
            if let (Some(remote), Some(path)) = (field("remote"), field("path"))
                && let Some(mine) = mapping.get(&(project.to_owned(), remote))
                && !same_place(mine, &path)
            {
                repo["path"] = value(mine.as_str());
            }
        }
    }
    doc.to_string().into_bytes()
}

/// The files of the enabled scopes, as they are synced: paths made portable (as an export does) and project
/// repositories put back at their synced path. A file that cannot be read that way (invalid TOML) is skipped,
/// so it neither blocks the others nor reads as deleted.
pub fn read(
    config_dir: &Path,
    scopes: &BTreeSet<Scope>,
    base: &BTreeMap<String, Record>,
    mapping: &mut Mapping,
) -> Result<(BTreeMap<String, Entry>, Vec<Skipped>)> {
    let (files, _links) = archive::synced_files(config_dir)?;
    let (mut entries, mut skipped) = (BTreeMap::new(), Vec::new());
    for (path, bytes) in files {
        if !scopes.contains(&scope_of(&path)) || !valid(&path) {
            continue;
        }
        let synced = archive::portable(&path, &bytes).and_then(|text| {
            let bytes = text.map_or(bytes, String::into_bytes);
            match project_id(&path) {
                Some(project) => to_synced(project, bytes, base.get(&path), mapping),
                None => Ok(bytes),
            }
        });
        let bytes = match synced {
            Ok(bytes) => bytes,
            Err(e) => {
                skipped.push(Skipped {
                    error: format!("{e:#}"),
                    path,
                });
                continue;
            }
        };
        let exec = executable(&config_dir.join(&path)).unwrap_or_else(|| {
            base.get(&path)
                .and_then(|r| r.entry.as_ref())
                .is_some_and(|e| e.exec)
        });
        entries.insert(path, Entry { bytes, exec });
    }
    Ok((entries, skipped))
}

#[cfg(unix)]
pub fn executable(path: &Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    Some(fs::metadata(path).ok()?.permissions().mode() & 0o111 != 0)
}

/// Windows has no execute bit: the synced one is kept.
#[cfg(not(unix))]
pub fn executable(_: &Path) -> Option<bool> {
    None
}

/// Writes (or deletes) a file of the config folder, never through a link.
pub fn write(
    config_dir: &Path,
    path: &str,
    entry: Option<&Entry>,
    mapping: &Mapping,
) -> Result<()> {
    anyhow::ensure!(valid(path), "cannot write {path}");
    let entry = entry.map(|entry| Entry {
        bytes: match project_id(path) {
            Some(project) => to_local(project, &entry.bytes, mapping),
            None => entry.bytes.clone(),
        },
        exec: entry.exec,
    });
    write_under(config_dir, path, entry.as_ref())
}

/// Writes (or deletes) `root/rel`, never through a link below `root`; empty folders left behind go, up to `root`.
pub fn write_under(root: &Path, rel: &str, entry: Option<&Entry>) -> Result<()> {
    anyhow::ensure!(!archive::through_link(root, rel), "cannot write {rel}");
    let target = root.join(rel);
    let Some(entry) = entry else {
        match fs::remove_file(&target) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        // Empty folders left behind (a deleted skill) would read as broken items.
        let mut dir = target.parent();
        while let Some(d) = dir.filter(|d| *d != root) {
            if fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
        return Ok(());
    };
    let bytes = &entry.bytes;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let tmp = target.with_file_name(format!(".{name}.sync.tmp"));
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if entry.exec { 0o755 } else { 0o644 };
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))?;
    }
    fs::rename(&tmp, &target).with_context(|| format!("cannot write {}", target.display()))
}

/// This machine's project repositories that are not where the project says. An invalid project is left to
/// `read`, which reports it.
pub fn missing_repos(config_dir: &Path) -> Result<Vec<MissingRepo>> {
    let mut missing = Vec::new();
    for entry in fs::read_dir(config_dir.join("projects"))
        .into_iter()
        .flatten()
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("projects/{name}");
        if project_id(&path).is_some() && entry.file_type()?.is_file() {
            let project = archive::Files::from([(path, fs::read(entry.path())?)]);
            missing.extend(archive::missing_repos(&project).unwrap_or_default());
        }
    }
    Ok(missing)
}

pub fn git(git_env: &[(String, String)]) -> Command {
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .envs(git_env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// The `core.sshCommand` another repository of `project` has on this machine: the key its remotes take.
fn project_ssh_command(
    config_dir: &Path,
    project: &str,
    git_env: &[(String, String)],
) -> Option<String> {
    let text = fs::read_to_string(config_dir.join(format!("projects/{project}.toml"))).ok()?;
    let table: toml::Table = toml::from_str(&text).ok()?;
    table
        .get("repos")?
        .as_array()?
        .iter()
        .filter_map(|r| r.get("path")?.as_str())
        .map(termf_core::expand_tilde)
        .filter(|p| p.join(".git").exists())
        .find_map(|repo| {
            let out = git(git_env)
                .arg("-C")
                .arg(&repo)
                .args(["config", "--local", "--get", "core.sshCommand"])
                .output()
                .ok()?;
            let command = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            (out.status.success() && !command.is_empty()).then_some(command)
        })
}

/// Clones `repo` into `target`, a folder that does not exist yet or is empty, telling git's progress lines to
/// `progress`. Fails with git's own words.
pub fn clone(
    config_dir: &Path,
    git_env: &[(String, String)],
    repo: &MissingRepo,
    target: &Path,
    progress: &mut dyn FnMut(&str),
) -> Result<()> {
    let remote = repo
        .remote
        .as_deref()
        .context("this repository has no git remote to clone from")?;
    ensure!(
        fs::read_dir(target).map_or(true, |mut d| d.next().is_none()),
        "{} already exists and is not empty",
        target.display()
    );
    let mut cmd = git(git_env);
    cmd.args(["clone", "--progress"]);
    if let Some(ssh) = project_ssh_command(config_dir, &repo.project, git_env) {
        cmd.arg("-c").arg(format!("core.sshCommand={ssh}"));
    }
    let mut child = cmd
        .arg("--")
        .arg(remote)
        .arg(target)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run git: is it installed?")?;
    let mut stderr = child.stderr.take().expect("piped");
    let (mut line, mut last) = (Vec::new(), Vec::<String>::new());
    let mut buf = [0u8; 4096];
    loop {
        let n = std::io::Read::read(&mut stderr, &mut buf)?;
        if n == 0 {
            break;
        }
        // git rewrites its progress line with `\r`: each version is a line of its own.
        for &b in &buf[..n] {
            if b == b'\r' || b == b'\n' {
                let text = String::from_utf8_lossy(&line).trim().to_owned();
                line.clear();
                if !text.is_empty() {
                    progress(&text);
                    last.push(text);
                    if last.len() > 20 {
                        last.remove(0);
                    }
                }
            } else {
                line.push(b);
            }
        }
    }
    let rest = String::from_utf8_lossy(&line).trim().to_owned();
    if !rest.is_empty() {
        last.push(rest);
    }
    let status = child.wait()?;
    ensure!(
        status.success(),
        "git clone failed: {}",
        last.iter()
            .filter(|l| !l.contains('%'))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    Ok(())
}

/// Points `repo` of its project at `target` on this machine; the synced path stays for the others.
pub fn relocate(config_dir: &Path, repo: &MissingRepo, target: &Path) -> Result<()> {
    let file = config_dir.join(format!("projects/{}.toml", repo.project));
    let mut doc: DocumentMut = fs::read_to_string(&file)?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    let path = termf_core::portable(&target.to_string_lossy());
    if let Some(repos) = doc.get_mut("repos").and_then(Item::as_array_of_tables_mut) {
        for entry in repos.iter_mut() {
            let field = |k| entry.get(k).and_then(Item::as_str).map(str::to_owned);
            if field("path").as_deref() == Some(&repo.path) && field("remote") == repo.remote {
                entry["path"] = value(path.as_str());
            }
        }
    }
    termf_core::write_atomic(&file, &doc.to_string())
}
