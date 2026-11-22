//! Export and import of the configuration as one `.tar.gz`: `termf.toml`, `layouts.toml`, the projects and the
//! libraries. Repository paths are made portable (`~/…` plus the git `remote`), and no secret is ever included
//! in clear: the vault, when asked, comes sealed by its own export password ([`VAULT`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, TableLike, value};

/// What an export holds from the config folder. Anything else (secrets, machine-local state) stays.
const INCLUDED: &[&str] = &[
    "termf.toml",
    "layouts.toml",
    "schedules.toml",
    "projects",
    "library",
];
/// Far above any real config: an archive that would fill the disk is refused.
const MAX_SIZE: u64 = 64 << 20;
/// Hidden, so never exported; removed once the import is done.
const STAGING: &str = ".import";
/// The vault's export, sealed by `termf-vault` before it gets here: the archive never sees a value.
pub const VAULT: &str = "vault.termfvault";

/// `/`-separated path relative to the config folder → content.
pub type Files = BTreeMap<String, Vec<u8>>;

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Added,
    Unchanged,
    /// Only new TOML keys: merging adds them and changes nothing of yours.
    Changed,
    /// Yours differs: merging keeps yours (adding the new keys of a TOML file), replacing takes the archive's.
    Conflict,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// A file, or a skill or rule folder, relative to the config folder.
    pub item: String,
    pub status: Status,
    /// TOML keys only the archive has.
    pub added_keys: Vec<String>,
    pub conflicts: Vec<String>,
    /// Yours is or goes through a symbolic link: importing never writes through it.
    pub linked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Write,
    Replace,
    Merge,
    Keep,
}

impl Change {
    /// Items in `replace` take the archive's version; the others are merged.
    pub fn action(&self, replace: &[String]) -> Action {
        match self.status {
            _ if self.linked => Action::Keep,
            Status::Unchanged => Action::Keep,
            Status::Added => Action::Write,
            _ if replace.contains(&self.item) => Action::Replace,
            _ if !self.added_keys.is_empty() => Action::Merge,
            _ => Action::Keep,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MissingRepo {
    pub project: String,
    pub path: String,
    pub remote: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub changes: Vec<Change>,
    /// Repositories of the archive's projects that are not on this machine.
    pub missing_repos: Vec<MissingRepo>,
    /// The archive holds a sealed vault export, imported by the vault with its password.
    pub vault: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    pub files: usize,
    /// Symbolic links, which an export never follows.
    pub skipped_links: Vec<String>,
}

pub fn export(config_dir: &Path, out: &Path) -> Result<Exported> {
    export_with_vault(config_dir, out, None)
}

/// The same, with a vault export already sealed by its own password.
pub fn export_with_vault(config_dir: &Path, out: &Path, vault: Option<&[u8]>) -> Result<Exported> {
    let (files, skipped_links) = local_files(config_dir, None)?;
    let file = File::create(out).with_context(|| format!("cannot write {}", out.display()))?;
    let mut tar = tar::Builder::new(GzEncoder::new(file, Compression::default()));
    for (path, bytes) in &files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(mode(&config_dir.join(path)));
        tar.append_data(&mut header, path, bytes.as_slice())?;
    }
    if let Some(vault) = vault {
        let mut header = tar::Header::new_gnu();
        header.set_size(vault.len() as u64);
        header.set_mode(0o600);
        tar.append_data(&mut header, VAULT, vault)?;
    }
    tar.into_inner()?.finish()?;
    Ok(Exported {
        files: files.len(),
        skipped_links,
    })
}

pub fn preview(archive: &Path, config_dir: &Path) -> Result<Preview> {
    let (files, _, vault) = read_archive(archive)?;
    Ok(Preview {
        vault: vault.is_some(),
        ..compare(&files, config_dir)?
    })
}

/// The sealed vault export the archive holds, if any.
pub fn vault(archive: &Path) -> Result<Option<Vec<u8>>> {
    Ok(read_archive(archive)?.2)
}

/// Imports `archive`: added items are written, items in `replace` take the archive's version, and the others
/// are merged (see [`Change::action`]). Everything is staged first, so a failure before the first rename
/// changes nothing; a failure after says which items were already imported. Returns the preview it acted on.
pub fn apply(archive: &Path, config_dir: &Path, replace: &[String]) -> Result<Preview> {
    let (incoming, modes, vault) = read_archive(archive)?;
    let preview = Preview {
        vault: vault.is_some(),
        ..compare(&incoming, config_dir)?
    };
    let mut ops = Vec::new();
    for change in &preview.changes {
        let mut writes: Vec<(String, Vec<u8>)> = Vec::new();
        let mut removals = Vec::new();
        match change.action(replace) {
            Action::Write | Action::Replace => {
                let theirs = incoming.iter().filter(|(p, _)| item_of(p) == change.item);
                writes.extend(theirs.map(|(p, b)| (p.clone(), b.clone())));
                // Exported files the archive lacks go; private ones (`.env`, keys…) are never touched.
                let (mine, _) = local_files(config_dir, Some(&change.item))?;
                removals.extend(mine.into_keys().filter(|p| !incoming.contains_key(p)));
            }
            Action::Merge => {
                let target = config_dir.join(&change.item);
                let mut mine: DocumentMut = fs::read_to_string(&target)?.parse()?;
                let theirs: DocumentMut = std::str::from_utf8(&incoming[&change.item])?.parse()?;
                merge(mine.as_table_mut(), theirs.as_table());
                writes.push((change.item.clone(), mine.to_string().into_bytes()));
            }
            Action::Keep => continue,
        }
        ops.push((change.item.clone(), writes, removals));
    }

    let staging = config_dir.join(STAGING);
    let _ = fs::remove_dir_all(&staging);
    let staged = stage(&staging, &ops, &modes);
    let result = staged.and_then(|staged| {
        let mut done: Vec<&str> = Vec::new();
        for ((item, writes, removals), tmps) in ops.iter().zip(staged) {
            let commit = || -> Result<()> {
                for ((path, _), tmp) in writes.iter().zip(tmps) {
                    let target = config_dir.join(path);
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(&tmp, &target)?;
                }
                for path in removals {
                    fs::remove_file(config_dir.join(path))?;
                }
                Ok(())
            };
            if let Err(e) = commit() {
                bail!(
                    "import stopped at {item}: {e:#} (already imported: {})",
                    done.join(", ")
                );
            }
            done.push(item);
        }
        Ok(())
    });
    let _ = fs::remove_dir_all(&staging);
    result.map(|()| preview)
}

type Op = (String, Vec<(String, Vec<u8>)>, Vec<String>);

/// Writes every file to come into `staging`, on the config's file system so that renames are atomic.
fn stage(
    staging: &Path,
    ops: &[Op],
    modes: &BTreeMap<String, u32>,
) -> Result<Vec<Vec<std::path::PathBuf>>> {
    fs::create_dir_all(staging)?;
    let mut count = 0;
    let mut staged = Vec::new();
    for (_, writes, _) in ops {
        let mut tmps = Vec::new();
        for (path, bytes) in writes {
            let tmp = staging.join(count.to_string());
            count += 1;
            fs::write(&tmp, bytes)?;
            if let Some(mode) = modes.get(path) {
                set_mode(&tmp, *mode)?;
            }
            tmps.push(tmp);
        }
        staged.push(tmps);
    }
    Ok(staged)
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).map_or(0o644, |m| m.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn mode(_: &Path) -> u32 {
    0o644
}

/// Group and others never get write access from an archive.
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if mode != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o755))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_: &Path, _: u32) -> Result<()> {
    Ok(())
}

fn compare(incoming: &Files, config_dir: &Path) -> Result<Preview> {
    let items: BTreeSet<String> = incoming.keys().map(|p| item_of(p)).collect();
    let mut changes = Vec::new();
    for item in items {
        let theirs: Files = incoming
            .iter()
            .filter(|(p, _)| item_of(p) == item)
            .map(|(p, b)| (p.clone(), b.clone()))
            .collect();
        let (mine, links) = local_files(config_dir, Some(&item))?;
        let mut change = Change {
            item: item.clone(),
            status: Status::Unchanged,
            added_keys: Vec::new(),
            conflicts: Vec::new(),
            linked: !links.is_empty() || through_link(config_dir, &item),
        };
        // A folder holding only private files is still yours.
        let exists = change.linked || fs::symlink_metadata(config_dir.join(&item)).is_ok();
        change.status = if !exists {
            Status::Added
        } else if mine == theirs {
            Status::Unchanged
        } else if item.ends_with(".toml") && theirs.contains_key(&item) && !change.linked {
            let parse = |bytes: &[u8]| -> Result<toml::Table> {
                Ok(toml::from_str(std::str::from_utf8(bytes)?)?)
            };
            let theirs =
                parse(&theirs[&item]).with_context(|| format!("invalid {item} in the archive"))?;
            match mine.get(&item).map(|b| parse(b)) {
                Some(Ok(mine)) => {
                    diff(&mine, &theirs, "", &mut change);
                    match (change.conflicts.is_empty(), change.added_keys.is_empty()) {
                        (false, _) => Status::Conflict,
                        (true, false) => Status::Changed,
                        (true, true) => Status::Unchanged,
                    }
                }
                _ => Status::Conflict,
            }
        } else {
            Status::Conflict
        };
        changes.push(change);
    }
    Ok(Preview {
        changes,
        missing_repos: missing_repos(incoming)?,
        vault: false,
    })
}

/// Whether the item, or a folder on its way, is a symbolic link.
pub fn through_link(config_dir: &Path, item: &str) -> bool {
    let mut path = config_dir.to_path_buf();
    item.split('/').any(|part| {
        path.push(part);
        path.is_symlink()
    })
}

/// The item a file belongs to: a skill or rule folder (`library/<kind>/<name>`), or the file itself.
fn item_of(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let library = match parts.as_slice() {
        ["library", ..] => 1,
        ["projects", _, "library", ..] => 3,
        _ => return path.into(),
    };
    if parts.len() > library + 2 {
        parts[..library + 2].join("/")
    } else {
        path.into()
    }
}

fn diff(mine: &toml::Table, theirs: &toml::Table, prefix: &str, change: &mut Change) {
    for (key, value) in theirs {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match (mine.get(key), value) {
            (None, _) => change.added_keys.push(path),
            (Some(toml::Value::Table(mine)), toml::Value::Table(theirs)) => {
                diff(mine, theirs, &path, change)
            }
            (Some(mine), theirs) if mine != theirs => change.conflicts.push(path),
            _ => {}
        }
    }
}

/// Adds the keys only `theirs` has; every value of `mine` stays.
fn merge(mine: &mut dyn TableLike, theirs: &dyn TableLike) {
    for (key, value) in theirs.iter() {
        match mine.get_mut(key) {
            None => {
                mine.insert(key, value.clone());
            }
            Some(existing) => {
                if let (Some(existing), Some(value)) =
                    (existing.as_table_like_mut(), value.as_table_like())
                {
                    merge(existing, value);
                }
            }
        }
    }
}

pub fn missing_repos(incoming: &Files) -> Result<Vec<MissingRepo>> {
    let mut missing = Vec::new();
    for (path, bytes) in incoming {
        let Some(project) = path
            .strip_prefix("projects/")
            .and_then(|f| f.strip_suffix(".toml"))
            .filter(|id| !id.contains('/'))
        else {
            continue;
        };
        let table: toml::Table = toml::from_str(std::str::from_utf8(bytes)?)
            .with_context(|| format!("invalid {path} in the archive"))?;
        let repos = table.get("repos").and_then(toml::Value::as_array);
        for repo in repos.into_iter().flatten() {
            let field = |key| repo.get(key).and_then(toml::Value::as_str);
            if let Some(path) = field("path")
                && !termf_core::expand_tilde(path).exists()
            {
                missing.push(MissingRepo {
                    project: project.into(),
                    path: path.into(),
                    remote: field("remote").map(Into::into),
                });
            }
        }
    }
    Ok(missing)
}

/// The files an export takes from the config folder (or only under `item`), made portable, and the links
/// it skipped.
pub fn local_files(config_dir: &Path, item: Option<&str>) -> Result<(Files, Vec<String>)> {
    let (mut files, links) = raw_files(config_dir, item)?;
    for (path, bytes) in files.iter_mut() {
        if let Some(text) = portable(path, bytes)? {
            *bytes = text.into_bytes();
        }
    }
    Ok((files, links))
}

/// The same files, not made portable.
pub fn raw_files(config_dir: &Path, item: Option<&str>) -> Result<(Files, Vec<String>)> {
    let mut files = Files::new();
    let mut links = Vec::new();
    for top in item.map_or(INCLUDED.to_vec(), |item| vec![item]) {
        walk(config_dir, &config_dir.join(top), &mut files, &mut links)?;
    }
    links.sort();
    Ok((files, links))
}

/// Links are not followed: nothing outside the config folder gets in.
fn walk(config_dir: &Path, path: &Path, files: &mut Files, links: &mut Vec<String>) -> Result<()> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    let rel = path
        .strip_prefix(config_dir)?
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if !allowed(&rel) {
        return Ok(());
    }
    if meta.is_symlink() {
        links.push(rel);
    } else if meta.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            walk(config_dir, &entry.path(), files, links)?;
        }
    } else if meta.is_file() {
        files.insert(rel, fs::read(path)?);
    }
    Ok(())
}

pub fn allowed(path: &str) -> bool {
    INCLUDED.contains(&path.split('/').next().unwrap_or_default())
        && path.split('/').all(|name| !private(name))
}

/// Hidden files (`.env`, `.git`…), keys and leftovers of an interrupted write never leave the machine,
/// whatever folder they were dropped in.
fn private(name: &str) -> bool {
    name.is_empty()
        || name.starts_with('.')
        || [".tmp", ".pem", ".key", ".p12", ".pfx", ".age"]
            .iter()
            .any(|ext| name.ends_with(ext))
        || ["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"]
            .iter()
            .any(|key| name.starts_with(key))
}

/// Paths of `termf.toml` and of the projects as `~/…`, and each repository's git `remote`, so they work on
/// another machine.
pub fn portable(path: &str, bytes: &[u8]) -> Result<Option<String>> {
    let project = path
        .strip_prefix("projects/")
        .is_some_and(|f| f.ends_with(".toml") && !f.contains('/'));
    if path != "termf.toml" && !project {
        return Ok(None);
    }
    let mut doc: DocumentMut = std::str::from_utf8(bytes)?
        .parse()
        .with_context(|| format!("invalid {path}"))?;
    let portable = |p: &str| termf_core::portable(&termf_core::expand_tilde(p).to_string_lossy());
    if let Some(roots) = doc.get_mut("roots").and_then(Item::as_array_mut) {
        for root in roots.iter_mut() {
            if let Some(p) = root
                .as_str()
                .map(portable)
                .filter(|p| Some(p.as_str()) != root.as_str())
            {
                *root = p.into();
            }
        }
    }
    if let Some(repos) = doc.get_mut("repos").and_then(Item::as_array_of_tables_mut) {
        for repo in repos.iter_mut() {
            let Some(path) = repo.get("path").and_then(Item::as_str).map(str::to_owned) else {
                continue;
            };
            if portable(&path) != path {
                repo["path"] = value(portable(&path));
            }
            let abs = termf_core::expand_tilde(&path);
            if !repo.contains_key("remote")
                && abs.is_absolute()
                && let Some(remote) = git_remote(&abs)
            {
                repo["remote"] = value(remote);
            }
        }
    }
    Ok(Some(doc.to_string()))
}

fn git_remote(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    let url = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (out.status.success() && !url.is_empty()).then(|| without_credentials(&url))
}

/// Drops the password of `scheme://user:password@host`. An http(s) user name is often a token, so it goes too;
/// other schemes keep theirs (`ssh://git@…`).
fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.into();
    };
    let host_end = rest.find('/').unwrap_or(rest.len());
    let Some(at) = rest[..host_end].rfind('@') else {
        return url.into();
    };
    let user = match rest[..at].split_once(':') {
        _ if scheme.starts_with("http") => "",
        Some((user, _)) => user,
        None => &rest[..at],
    };
    let user = if user.is_empty() {
        String::new()
    } else {
        format!("{user}@")
    };
    format!("{scheme}://{user}{}", &rest[at + 1..])
}

type Archive = (Files, BTreeMap<String, u32>, Option<Vec<u8>>);

/// Reads every file and its mode, and the sealed vault export, refusing any path an export would not hold: no
/// `..`, no absolute path, no link.
fn read_archive(archive: &Path) -> Result<Archive> {
    let file = File::open(archive).with_context(|| format!("cannot open {}", archive.display()))?;
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    let mut files = Files::new();
    let mut modes = BTreeMap::new();
    let mut vault = None;
    let mut size = 0;
    for entry in tar.entries().context("not a TermF export")? {
        let mut entry = entry.context("not a TermF export")?;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            continue;
        }
        let path = entry.path()?.into_owned();
        if kind.is_file() && path == Path::new(VAULT) {
            size += entry.size();
            ensure!(size <= MAX_SIZE, "the archive is too large");
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            vault = Some(bytes);
            continue;
        }
        let rel = path
            .components()
            .map(|c| match c {
                Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(|parts| parts.join("/"))
            .filter(|rel| kind.is_file() && allowed(rel))
            .with_context(|| format!("unexpected {} in the archive", path.display()))?;
        size += entry.size();
        ensure!(size <= MAX_SIZE, "the archive is too large");
        modes.insert(rel.clone(), entry.header().mode().unwrap_or(0));
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        files.insert(rel, bytes);
    }
    Ok((files, modes, vault))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{put, temp_dir};

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn change<'a>(preview: &'a Preview, item: &str) -> &'a Change {
        preview.changes.iter().find(|c| c.item == item).unwrap()
    }

    #[test]
    fn exports_and_imports_a_config() {
        let dir = temp_dir("archive");
        let (from, to) = (dir.join("from"), dir.join("to"));
        let api = termf_core::expand_tilde("~/termf-test-missing/api");
        put(&from.join("termf.toml"), "# mine\nroots = [\"~\"]\n");
        put(&from.join("layouts.toml"), "[[layouts]]\nname = \"m\"\n");
        put(
            &from.join("projects/acme.toml"),
            &format!(
                "name = \"Acme\"\n[[repos]]\npath = {:?}\nremote = \"git@x:acme/api.git\"\n",
                api.to_string_lossy()
            ),
        );
        put(&from.join("projects/acme/library/roles/r.md"), "role\n");
        put(&from.join("library/skills/s/SKILL.md"), "skill\n");
        put(&from.join("library/skills/s/scripts/run.sh"), "run\n");
        put(&from.join("library/commands/c.md"), "command\n");
        for secret in [
            "library/skills/s/.env",
            "library/keys/id_ed25519",
            "library/x.pem",
            "vault.age",
            "local.toml",
            "termf.toml.tmp",
        ] {
            put(&from.join(secret), "SECRET");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let script = from.join("library/skills/s/scripts/run.sh");
            fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let archive = dir.join("config.tar.gz");
        let exported = export(&from, &archive).unwrap();
        assert_eq!((exported.files, exported.skipped_links.len()), (7, 0));
        let (files, _, vault) = read_archive(&archive).unwrap();
        assert!(vault.is_none());
        assert!(files.values().all(|b| b != b"SECRET"), "{:?}", files.keys());
        let project = String::from_utf8(files["projects/acme.toml"].clone()).unwrap();
        assert!(
            project.contains("path = \"~/termf-test-missing/api\""),
            "{project}"
        );

        let preview = preview(&archive, &to).unwrap();
        assert!(preview.changes.iter().all(|c| c.status == Status::Added));
        let items: Vec<_> = preview.changes.iter().map(|c| c.item.as_str()).collect();
        assert_eq!(
            items,
            [
                "layouts.toml",
                "library/commands/c.md",
                "library/skills/s",
                "projects/acme.toml",
                "projects/acme/library/roles/r.md",
                "termf.toml",
            ]
        );
        assert_eq!(
            preview.missing_repos,
            [MissingRepo {
                project: "acme".into(),
                path: "~/termf-test-missing/api".into(),
                remote: Some("git@x:acme/api.git".into()),
            }]
        );

        apply(&archive, &to, &[]).unwrap();
        assert_eq!(read(&to.join("termf.toml")), "# mine\nroots = [\"~\"]\n");
        assert_eq!(read(&to.join("library/skills/s/scripts/run.sh")), "run\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let script = fs::metadata(to.join("library/skills/s/scripts/run.sh")).unwrap();
            assert_eq!(script.permissions().mode() & 0o777, 0o755);
            let skill = fs::metadata(to.join("library/skills/s/SKILL.md")).unwrap();
            assert_eq!(skill.permissions().mode() & 0o111, 0);
        }
        assert!(!to.join("library/skills/s/.env").exists());
        assert!(!to.join(STAGING).exists());
        assert_eq!(
            termf_core::load_projects(&to).unwrap()[0].repos[0].path,
            api.to_string_lossy()
        );
        let again = super::preview(&archive, &to).unwrap();
        assert!(
            again.changes.iter().all(|c| c.status == Status::Unchanged),
            "{:?}",
            again.changes
        );
    }

    #[test]
    fn carries_a_sealed_vault_export_beside_the_config() {
        let dir = temp_dir("archive-vault");
        let (from, to) = (dir.join("from"), dir.join("to"));
        put(&from.join("termf.toml"), "scan_depth = 3\n");
        let archive = dir.join("config.tar.gz");
        export_with_vault(&from, &archive, Some(b"{\"termf\": \"sealed\"}")).unwrap();
        let preview = preview(&archive, &to).unwrap();
        assert!(preview.vault);
        assert_eq!(preview.changes.len(), 1, "not a config item");
        assert_eq!(
            vault(&archive).unwrap().unwrap(),
            b"{\"termf\": \"sealed\"}"
        );
        apply(&archive, &to, &[]).unwrap();
        assert!(
            !to.join(VAULT).exists(),
            "the vault imports it, never the config"
        );
        export(&from, &archive).unwrap();
        assert!(!super::preview(&archive, &to).unwrap().vault);
    }

    #[test]
    fn merges_or_replaces_item_by_item() {
        let dir = temp_dir("archive-merge");
        let (from, to) = (dir.join("from"), dir.join("to"));
        put(
            &from.join("termf.toml"),
            "scan_depth = 3\neditor = \"zed\"\n[library]\nskills = [\"+s\"]\n",
        );
        put(&from.join("library/commands/c.md"), "theirs\n");
        put(&from.join("library/skills/s/SKILL.md"), "theirs\n");
        put(&from.join("library/skills/p/SKILL.md"), "theirs\n");
        put(&to.join("termf.toml"), "# keep me\nscan_depth = 5\n");
        put(&to.join("library/commands/c.md"), "mine\n");
        put(&to.join("library/skills/s/SKILL.md"), "mine\n");
        put(&to.join("library/skills/s/extra.md"), "mine\n");
        for private in [".env", "deploy.pem", "id_ed25519"] {
            put(&to.join("library/skills/s").join(private), "SECRET");
        }
        put(&to.join("library/skills/p/.env"), "SECRET");
        let archive = dir.join("config.tar.gz");
        export(&from, &archive).unwrap();

        let preview = preview(&archive, &to).unwrap();
        let settings = change(&preview, "termf.toml");
        assert_eq!(settings.status, Status::Conflict);
        assert_eq!(settings.conflicts, ["scan_depth"]);
        assert_eq!(settings.added_keys, ["editor", "library"]);
        assert!(
            preview.changes.iter().all(|c| c.status == Status::Conflict),
            "a folder holding only private files is not added: {:?}",
            preview.changes
        );

        apply(&archive, &to, &[]).unwrap();
        let text = read(&to.join("termf.toml"));
        assert!(text.starts_with("# keep me\nscan_depth = 5\n"), "{text}");
        assert!(
            text.contains("editor = \"zed\"") && text.contains("skills = [\"+s\"]"),
            "{text}"
        );
        assert_eq!(read(&to.join("library/commands/c.md")), "mine\n");

        let replace = ["library/skills/s", "library/skills/p", "termf.toml"].map(String::from);
        apply(&archive, &to, &replace).unwrap();
        assert_eq!(read(&to.join("library/skills/s/SKILL.md")), "theirs\n");
        assert!(
            !to.join("library/skills/s/extra.md").exists(),
            "an exported file the archive lacks goes"
        );
        for private in ["s/.env", "s/deploy.pem", "s/id_ed25519", "p/.env"] {
            assert_eq!(
                read(&to.join("library/skills").join(private)),
                "SECRET",
                "{private}"
            );
        }
        assert_eq!(read(&to.join("library/skills/p/SKILL.md")), "theirs\n");
        assert!(read(&to.join("termf.toml")).starts_with("scan_depth = 3\n"));
        assert_eq!(
            read(&to.join("library/commands/c.md")),
            "mine\n",
            "not asked: kept"
        );
    }

    #[cfg(unix)]
    #[test]
    fn never_writes_through_links() {
        use std::os::unix::fs::symlink;
        let dir = temp_dir("archive-links");
        let (from, to, outside) = (dir.join("from"), dir.join("to"), dir.join("outside"));
        put(&from.join("termf.toml"), "scan_depth = 3\n");
        put(&from.join("library/skills/s/SKILL.md"), "theirs\n");
        put(&outside.join("termf.toml"), "scan_depth = 9\n");
        fs::create_dir_all(outside.join("skills")).unwrap();
        fs::create_dir_all(to.join("library")).unwrap();
        symlink(outside.join("termf.toml"), to.join("termf.toml")).unwrap();
        symlink(outside.join("skills"), to.join("library/skills")).unwrap();
        let archive = dir.join("config.tar.gz");
        export(&from, &archive).unwrap();

        let preview = preview(&archive, &to).unwrap();
        for item in ["termf.toml", "library/skills/s"] {
            let change = change(&preview, item);
            assert_eq!(
                (change.status, change.linked),
                (Status::Conflict, true),
                "{item}"
            );
        }
        apply(
            &archive,
            &to,
            &["termf.toml".into(), "library/skills/s".into()],
        )
        .unwrap();
        assert_eq!(read(&outside.join("termf.toml")), "scan_depth = 9\n");
        assert!(to.join("termf.toml").is_symlink());
        assert!(!outside.join("skills/s").exists());

        let exported = export(&to, &dir.join("again.tar.gz")).unwrap();
        assert_eq!(exported.files, 0);
        assert_eq!(exported.skipped_links, ["library/skills", "termf.toml"]);
    }

    #[test]
    fn a_failed_import_says_what_was_done() {
        let dir = temp_dir("archive-failure");
        let (from, to) = (dir.join("from"), dir.join("to"));
        put(&from.join("library/commands/c.md"), "theirs\n");
        put(&from.join("termf.toml"), "scan_depth = 3\n");
        // A file where the archive needs a folder: the first rename fails.
        put(&to.join("library/commands"), "not a folder");
        let archive = dir.join("config.tar.gz");
        export(&from, &archive).unwrap();

        let err = apply(&archive, &to, &[]).unwrap_err().to_string();
        assert!(
            err.contains("import stopped at library/commands/c.md"),
            "{err}"
        );
        assert!(err.contains("already imported: )"), "{err}");
        assert!(!to.join("termf.toml").exists(), "nothing after the failure");
        assert!(!to.join(STAGING).exists());
    }

    #[test]
    fn makes_paths_portable_without_credentials() {
        let dir = temp_dir("portable");
        let repo = dir.join("repo");
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        fs::create_dir_all(&repo).unwrap();
        git(&["init", "-q"]);
        git(&[
            "remote",
            "add",
            "origin",
            "https://user:token@github.com/acme/api.git",
        ]);
        let home_repo = termf_core::expand_tilde("~/src/web")
            .to_string_lossy()
            .into_owned();
        let text = format!(
            "name = \"Acme\" # ours\n[[repos]]\npath = {:?}\n[[repos]]\npath = {home_repo:?}\nremote = \"git@x:web.git\"\n",
            repo.to_string_lossy()
        );
        let out = portable("projects/acme.toml", text.as_bytes())
            .unwrap()
            .unwrap();
        assert!(out.contains("name = \"Acme\" # ours"), "{out}");
        assert!(
            out.contains("remote = \"https://github.com/acme/api.git\""),
            "{out}"
        );
        assert!(
            out.contains("path = \"~/src/web\"\nremote = \"git@x:web.git\""),
            "{out}"
        );
        assert!(!out.contains("token"));

        let settings = format!(
            "roots = [{:?}, \"/srv\"]\n",
            termf_core::expand_tilde("~/code").to_string_lossy()
        );
        let out = portable("termf.toml", settings.as_bytes())
            .unwrap()
            .unwrap();
        assert!(
            out.contains("\"~/code\"") && out.contains("\"/srv\""),
            "{out}"
        );
        assert_eq!(portable("layouts.toml", b"x").unwrap(), None);

        for (url, expected) in [
            ("https://a:b@h.io/x@y", "https://h.io/x@y"),
            ("https://ghp_token@h.io/x", "https://h.io/x"),
            ("ssh://git:secret@h.io/x", "ssh://git@h.io/x"),
            ("git+ssh://me:pw@h.io:22/x", "git+ssh://me@h.io:22/x"),
            ("ftp://:pw@h.io/x", "ftp://h.io/x"),
            ("ssh://git@h.io/x", "ssh://git@h.io/x"),
            ("git@h.io:x.git", "git@h.io:x.git"),
            ("/srv/git/x.git", "/srv/git/x.git"),
        ] {
            assert_eq!(without_credentials(url), expected, "{url}");
        }
    }

    #[test]
    fn refuses_archives_with_unexpected_paths() {
        let dir = temp_dir("archive-unsafe");
        let write = |name: &str, path: &[u8]| {
            let archive = dir.join(name);
            let mut tar = tar::Builder::new(GzEncoder::new(
                File::create(&archive).unwrap(),
                Compression::default(),
            ));
            let mut header = tar::Header::new_old();
            header.as_old_mut().name[..path.len()].copy_from_slice(path);
            header.set_size(1);
            header.set_cksum();
            tar.append(&header, &b"x"[..]).unwrap();
            tar.into_inner().unwrap().finish().unwrap();
            archive
        };
        for (name, path) in [
            ("dotdot", &b"library/../../evil"[..]),
            ("absolute", b"/etc/evil"),
            ("unknown", b"secrets.txt"),
            ("hidden", b"library/.env"),
        ] {
            let err = preview(&write(name, path), &dir.join("config")).unwrap_err();
            assert!(err.to_string().contains("unexpected"), "{name}: {err:#}");
        }
        put(&dir.join("plain.txt"), "not an archive");
        assert!(preview(&dir.join("plain.txt"), &dir).is_err());
        assert!(!dir.join("config").exists());
    }

    #[test]
    fn groups_files_into_items() {
        assert_eq!(item_of("library/skills/s/SKILL.md"), "library/skills/s");
        assert_eq!(item_of("library/skills/s/a/b.md"), "library/skills/s");
        assert_eq!(item_of("library/commands/c.md"), "library/commands/c.md");
        assert_eq!(
            item_of("projects/library/library/rules/r/rule.md"),
            "projects/library/library/rules/r"
        );
        assert_eq!(item_of("projects/acme.toml"), "projects/acme.toml");
        assert_eq!(item_of("termf.toml"), "termf.toml");
    }
}
