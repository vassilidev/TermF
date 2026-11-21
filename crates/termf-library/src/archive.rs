//! Export and import of the configuration as one `.tar.gz`: `termf.toml`, `layouts.toml`, the projects and the
//! libraries. Repository paths are made portable (`~/…` plus the git `remote`), and no secret is ever included.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::Serialize;
use toml_edit::{DocumentMut, Item, TableLike, value};

use crate::write_file;

/// What an export holds from the config folder. Anything else (secrets, machine-local state) stays.
const INCLUDED: &[&str] = &["termf.toml", "layouts.toml", "projects", "library"];
/// Far above any real config: an archive that would fill the disk is refused.
const MAX_SIZE: u64 = 64 << 20;

/// `/`-separated path relative to the config folder → content.
type Files = BTreeMap<String, Vec<u8>>;

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
    /// TOML keys whose value differs.
    pub conflicts: Vec<String>,
}

/// What importing does with an item.
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
            Status::Unchanged => Action::Keep,
            Status::Added => Action::Write,
            _ if replace.contains(&self.item) => Action::Replace,
            _ if !self.added_keys.is_empty() => Action::Merge,
            _ => Action::Keep,
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
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
}

/// Writes the archive to `out` and returns how many files it holds.
pub fn export(config_dir: &Path, out: &Path) -> Result<usize> {
    let files = local_files(config_dir, None)?;
    let file = File::create(out).with_context(|| format!("cannot write {}", out.display()))?;
    let mut tar = tar::Builder::new(GzEncoder::new(file, Compression::default()));
    for (path, bytes) in &files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        tar.append_data(&mut header, path, bytes.as_slice())?;
    }
    tar.into_inner()?.finish()?;
    Ok(files.len())
}

/// What importing `archive` would add, change or conflict with, item by item.
pub fn preview(archive: &Path, config_dir: &Path) -> Result<Preview> {
    compare(&read_archive(archive)?, config_dir)
}

/// Imports `archive`: added items are written, items in `replace` take the archive's version, and the others
/// are merged (see [`Change::action`]). Returns the preview it acted on.
pub fn apply(archive: &Path, config_dir: &Path, replace: &[String]) -> Result<Preview> {
    let incoming = read_archive(archive)?;
    let preview = compare(&incoming, config_dir)?;
    for change in &preview.changes {
        let target = config_dir.join(&change.item);
        match change.action(replace) {
            Action::Write | Action::Replace => {
                if target.is_dir() {
                    fs::remove_dir_all(&target)?;
                }
                for (path, bytes) in incoming.iter().filter(|(p, _)| item_of(p) == change.item) {
                    write_file(&config_dir.join(path), bytes)?;
                }
            }
            Action::Merge => {
                let mut mine: DocumentMut = fs::read_to_string(&target)?.parse()?;
                let theirs: DocumentMut = std::str::from_utf8(&incoming[&change.item])?.parse()?;
                merge(mine.as_table_mut(), theirs.as_table());
                termf_core::write_atomic(&target, &mine.to_string())?;
            }
            Action::Keep => {}
        }
    }
    Ok(preview)
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
        let mine = local_files(config_dir, Some(&item))?;
        let mut change = Change {
            item: item.clone(),
            status: Status::Unchanged,
            added_keys: Vec::new(),
            conflicts: Vec::new(),
        };
        change.status = if mine.is_empty() {
            Status::Added
        } else if mine == theirs {
            Status::Unchanged
        } else if item.ends_with(".toml") && theirs.contains_key(&item) {
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

fn missing_repos(incoming: &Files) -> Result<Vec<MissingRepo>> {
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

/// The files an export takes from the config folder (or only under `item`), made portable.
fn local_files(config_dir: &Path, item: Option<&str>) -> Result<Files> {
    let mut files = Files::new();
    for top in item.map_or(INCLUDED.to_vec(), |item| vec![item]) {
        walk(config_dir, &config_dir.join(top), &mut files)?;
    }
    for (path, bytes) in files.iter_mut() {
        if let Some(text) = portable(path, bytes)? {
            *bytes = text.into_bytes();
        }
    }
    Ok(files)
}

/// Links are not followed: nothing outside the config folder gets in.
fn walk(config_dir: &Path, path: &Path, files: &mut Files) -> Result<()> {
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
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            walk(config_dir, &entry?.path(), files)?;
        }
    } else if meta.is_file() {
        files.insert(rel, fs::read(path)?);
    }
    Ok(())
}

/// Whether an archive may hold this path.
fn allowed(path: &str) -> bool {
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
fn portable(path: &str, bytes: &[u8]) -> Result<Option<String>> {
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

/// The repository's `origin`, without the credentials an https URL can carry.
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

fn without_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://").filter(|(s, _)| s.starts_with("http")) else {
        return url.into();
    };
    let host = &rest[..rest.find('/').unwrap_or(rest.len())];
    match host.rfind('@') {
        Some(at) => format!("{scheme}://{}", &rest[at + 1..]),
        None => url.into(),
    }
}

/// Reads every file, refusing any path an export would not hold: no `..`, no absolute path, no link.
fn read_archive(archive: &Path) -> Result<Files> {
    let file = File::open(archive).with_context(|| format!("cannot open {}", archive.display()))?;
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    let mut files = Files::new();
    let mut size = 0;
    for entry in tar.entries().context("not a TermF export")? {
        let mut entry = entry.context("not a TermF export")?;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            continue;
        }
        let path = entry.path()?.into_owned();
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
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        files.insert(rel, bytes);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{put, temp_dir};

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
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

        let archive = dir.join("config.tar.gz");
        assert_eq!(export(&from, &archive).unwrap(), 7);
        let files = read_archive(&archive).unwrap();
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
        assert!(!to.join("library/skills/s/.env").exists());
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
    fn merges_or_replaces_item_by_item() {
        let dir = temp_dir("archive-merge");
        let (from, to) = (dir.join("from"), dir.join("to"));
        put(
            &from.join("termf.toml"),
            "scan_depth = 3\neditor = \"zed\"\n[library]\nskills = [\"+s\"]\n",
        );
        put(&from.join("library/commands/c.md"), "theirs\n");
        put(&from.join("library/skills/s/SKILL.md"), "theirs\n");
        put(&to.join("termf.toml"), "# keep me\nscan_depth = 5\n");
        put(&to.join("library/commands/c.md"), "mine\n");
        put(&to.join("library/skills/s/SKILL.md"), "mine\n");
        put(&to.join("library/skills/s/extra.md"), "mine\n");
        let archive = dir.join("config.tar.gz");
        export(&from, &archive).unwrap();

        let preview = preview(&archive, &to).unwrap();
        let settings = &preview
            .changes
            .iter()
            .find(|c| c.item == "termf.toml")
            .unwrap();
        assert_eq!(settings.status, Status::Conflict);
        assert_eq!(settings.conflicts, ["scan_depth"]);
        assert_eq!(settings.added_keys, ["editor", "library"]);
        assert!(preview.changes.iter().all(|c| c.status == Status::Conflict));

        apply(&archive, &to, &[]).unwrap();
        let text = read(&to.join("termf.toml"));
        assert!(text.starts_with("# keep me\nscan_depth = 5\n"), "{text}");
        assert!(
            text.contains("editor = \"zed\"") && text.contains("skills = [\"+s\"]"),
            "{text}"
        );
        assert_eq!(read(&to.join("library/commands/c.md")), "mine\n");

        apply(
            &archive,
            &to,
            &["library/skills/s".into(), "termf.toml".into()],
        )
        .unwrap();
        assert_eq!(read(&to.join("library/skills/s/SKILL.md")), "theirs\n");
        assert!(
            !to.join("library/skills/s/extra.md").exists(),
            "a replaced folder is the archive's"
        );
        assert!(read(&to.join("termf.toml")).starts_with("scan_depth = 3\n"));
        assert_eq!(
            read(&to.join("library/commands/c.md")),
            "mine\n",
            "not asked: kept"
        );
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

        assert_eq!(
            without_credentials("https://a:b@h.io/x@y"),
            "https://h.io/x@y"
        );
        assert_eq!(without_credentials("ssh://git@h.io/x"), "ssh://git@h.io/x");
        assert_eq!(without_credentials("git@h.io:x.git"), "git@h.io:x.git");
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
