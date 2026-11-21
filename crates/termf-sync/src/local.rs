//! The synced files as this machine has them. Projects travel with each repository's `remote` and the path it
//! had where it was added; `local.toml`, never synced, maps a remote to this machine's own path when it differs.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use termf_library::archive::{self, MissingRepo};
use toml_edit::{DocumentMut, Item, Table, value};

use crate::Scope;
use crate::record::{Entry, Record};

const LOCAL: &str = "local.toml";

/// Git remote → this machine's path for it.
pub type Mapping = BTreeMap<String, String>;

pub fn scope_of(path: &str) -> Scope {
    match path.split('/').collect::<Vec<_>>().as_slice() {
        ["projects", _, "conventions", ..] => Scope::Conventions,
        ["projects", ..] => Scope::Projects,
        _ => Scope::Config,
    }
}

/// A path the remote may name: one an export would hold, which cannot leave the config folder on any system.
pub fn valid(path: &str) -> bool {
    archive::allowed(path) && !path.contains(['\\', ':'])
}

fn is_project(path: &str) -> bool {
    path.strip_prefix("projects/")
        .is_some_and(|f| f.ends_with(".toml") && !f.contains('/'))
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
        .filter_map(|(remote, path)| Some((remote.clone(), path.as_str()?.to_owned())))
        .collect())
}

/// Rewrites only `[repos]`: anything else in the file stays as written.
pub fn save_mapping(config_dir: &Path, mapping: &Mapping) -> Result<()> {
    let file = config_dir.join(LOCAL);
    let mut doc: DocumentMut = match fs::read_to_string(&file) {
        Ok(text) => text
            .parse()
            .with_context(|| format!("invalid {}", file.display()))?,
        Err(_) => "# This machine's repository paths, by git remote. Never synced.\n".parse()?,
    };
    let mut repos = Table::new();
    for (remote, path) in mapping {
        repos[remote.as_str()] = value(path.as_str());
    }
    doc["repos"] = Item::Table(repos);
    termf_core::write_atomic(&file, &doc.to_string())
}

fn same_place(a: &str, b: &str) -> bool {
    termf_core::expand_tilde(a) == termf_core::expand_tilde(b)
}

/// Each repository the last synced version knows (by remote) keeps that version's path; this machine's own
/// path goes to `mapping`.
fn to_synced(bytes: Vec<u8>, base: Option<&Record>, mapping: &mut Mapping) -> Result<Vec<u8>> {
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
            if let Some(hint) = hints.get(&remote)
                && !same_place(hint, &path)
            {
                mapping.insert(remote, path);
                repo["path"] = value(hint.as_str());
            }
        }
    }
    Ok(doc.to_string().into_bytes())
}

fn to_local(bytes: &[u8], mapping: &Mapping) -> Vec<u8> {
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
                && let Some(mine) = mapping.get(&remote)
                && !same_place(mine, &path)
            {
                repo["path"] = value(mine.as_str());
            }
        }
    }
    doc.to_string().into_bytes()
}

/// The files of the enabled scopes, as they are synced: paths made portable (as an export does) and project
/// repositories put back at their synced path.
pub fn read(
    config_dir: &Path,
    scopes: &BTreeSet<Scope>,
    base: &BTreeMap<String, Record>,
    mapping: &mut Mapping,
) -> Result<BTreeMap<String, Entry>> {
    let (files, _links) = archive::local_files(config_dir, None)?;
    let mut entries = BTreeMap::new();
    for (path, bytes) in files {
        if !scopes.contains(&scope_of(&path)) || !valid(&path) {
            continue;
        }
        let bytes = if is_project(&path) {
            to_synced(bytes, base.get(&path), mapping)?
        } else {
            bytes
        };
        let exec = executable(&config_dir.join(&path)).unwrap_or_else(|| {
            base.get(&path)
                .and_then(|r| r.entry.as_ref())
                .is_some_and(|e| e.exec)
        });
        entries.insert(path, Entry { bytes, exec });
    }
    Ok(entries)
}

#[cfg(unix)]
fn executable(path: &Path) -> Option<bool> {
    use std::os::unix::fs::PermissionsExt;
    Some(fs::metadata(path).ok()?.permissions().mode() & 0o111 != 0)
}

/// Windows has no execute bit: the synced one is kept.
#[cfg(not(unix))]
fn executable(_: &Path) -> Option<bool> {
    None
}

/// Writes (or deletes) a file of the config folder, never through a link.
pub fn write(
    config_dir: &Path,
    path: &str,
    entry: Option<&Entry>,
    mapping: &Mapping,
) -> Result<()> {
    anyhow::ensure!(
        valid(path) && !archive::through_link(config_dir, path),
        "cannot write {path}"
    );
    let target = config_dir.join(path);
    let Some(entry) = entry else {
        match fs::remove_file(&target) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        // Empty folders left behind (a deleted skill) would read as broken items.
        let mut dir = target.parent();
        while let Some(d) = dir.filter(|d| *d != config_dir) {
            if fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
        return Ok(());
    };
    let bytes = if is_project(path) {
        to_local(&entry.bytes, mapping)
    } else {
        entry.bytes.clone()
    };
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

/// This machine's project repositories that are not where the project says.
pub fn missing_repos(config_dir: &Path) -> Result<Vec<MissingRepo>> {
    let mut projects = BTreeMap::new();
    for entry in fs::read_dir(config_dir.join("projects"))
        .into_iter()
        .flatten()
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("projects/{name}");
        if is_project(&path) && entry.file_type()?.is_file() {
            projects.insert(path, fs::read(entry.path())?);
        }
    }
    archive::missing_repos(&projects)
}
