//! Ports for each worktree, from 41000 to 41999 (ARCHITECTURE §9), kept in `<data>/ports.json` so that a
//! worktree keeps its ports and two worktrees never share one.

use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;
use std::path::Path;

use anyhow::{Context, Result};

const FIRST: u16 = 41000;
const LAST: u16 = 41999;

type Given = BTreeMap<String, BTreeMap<String, u16>>;

/// `web-app` → `web_app`: what follows `TERMF_PORT_`, lowercase.
pub(crate) fn name(service: &str) -> String {
    service
        .trim()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

/// The same worktree whichever way its path is written (`/var` or `/private/var` on macOS).
fn key(worktree: &Path) -> String {
    fs::canonicalize(worktree)
        .unwrap_or_else(|_| worktree.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn load(data_dir: &Path) -> Given {
    fs::read_to_string(data_dir.join("ports.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn store(data_dir: &Path, given: &Given) -> Result<()> {
    fs::create_dir_all(data_dir)?;
    termf_core::write_atomic(
        &data_dir.join("ports.json"),
        &serde_json::to_string_pretty(given)?,
    )
}

/// A port for each of `names`, the same as last time for the names the worktree already had.
// shortcut: the app and the CLI allocating at the very same moment could pick the same port; a lock file would
// settle it if that ever happens.
pub(crate) fn allocate(
    data_dir: &Path,
    worktree: &Path,
    names: &[String],
) -> Result<BTreeMap<String, u16>> {
    let mut given = load(data_dir);
    // Worktrees that are gone give their ports back.
    given.retain(|path, _| Path::new(path).exists());
    let key = key(worktree);
    let mut taken: Vec<u16> = given
        .iter()
        .filter(|(path, _)| **path != key)
        .flat_map(|(_, ports)| ports.values().copied())
        .collect();
    let mut mine = given.remove(&key).unwrap_or_default();
    mine.retain(|name, _| names.contains(name));
    taken.extend(mine.values().copied());
    for name in names {
        if mine.contains_key(name) {
            continue;
        }
        let port = (FIRST..=LAST)
            .find(|p| !taken.contains(p) && TcpListener::bind(("127.0.0.1", *p)).is_ok())
            .context("no free port left between 41000 and 41999")?;
        taken.push(port);
        mine.insert(name.clone(), port);
    }
    given.insert(key, mine.clone());
    store(data_dir, &given)?;
    Ok(mine)
}

/// Gives the worktree's ports back; `None` when it had none.
pub(crate) fn release(data_dir: &Path, worktree: &Path) -> Result<Option<BTreeMap<String, u16>>> {
    let mut given = load(data_dir);
    let freed = given.remove(&key(worktree));
    if freed.is_some() {
        store(data_dir, &given)?;
    }
    Ok(freed)
}
