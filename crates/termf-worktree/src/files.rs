//! The files git ignores that a worktree needs, picked with `.worktreeinclude` patterns (gitignore syntax).

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};

use crate::cow;

/// Never copied raw, whatever a pattern says: they are cloned or installed as dependencies.
pub(crate) const DEPENDENCY_DIRS: [&str; 6] = [
    "node_modules",
    "vendor",
    ".venv",
    "venv",
    "target",
    "vendor/bundle",
];

/// The repository's worktrees, TermF's and Claude Code's: never copied into another one.
const WORKTREES: &str = ".claude/worktrees";

/// `rel` as a path inside a repository: relative, without `..` or `.git`.
pub(crate) fn relative(rel: &str) -> Option<PathBuf> {
    let path = Path::new(rel.trim());
    let fine = !rel.trim().is_empty()
        && path.components().all(|c| match c {
            Component::Normal(name) => name != ".git",
            Component::CurDir => true,
            _ => false,
        });
    fine.then(|| path.to_path_buf())
}

/// Copies from `source` into `worktree` what git ignores and `patterns` name, unless the worktree already has
/// it. Dependency folders (`skip`, and the usual ones) are left out. Returns what it copied, and how many it
/// found already there.
pub(crate) fn include(
    env: &[(String, String)],
    source: &Path,
    worktree: &Path,
    patterns: &[String],
    skip: &[&str],
) -> Result<(Vec<String>, usize)> {
    // `--directory` lists an ignored folder once, without walking into it.
    let listing = crate::git(
        env,
        source,
        &[
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ],
    )?;
    let mut entries: Vec<(String, bool)> = Vec::new();
    for entry in listing.split('\0').filter(|e| !e.is_empty()) {
        let path = entry.trim_end_matches('/');
        let is_dir = entry.ends_with('/');
        // A folder ignored as a whole is listed once: walk into it when a pattern names a file inside, or when it
        // holds the worktrees.
        let holds_worktrees = WORKTREES.starts_with(&format!("{path}/"));
        if is_dir
            && (holds_worktrees || !included(patterns, path, true) && names_inside(patterns, path))
        {
            entries.extend(files_under(source, path).into_iter().map(|f| (f, false)));
        } else {
            entries.push((path.to_owned(), is_dir));
        }
    }
    entries.sort();
    entries.dedup();
    let (mut copied, mut present) = (Vec::new(), 0);
    for (path, is_dir) in &entries {
        let path = path.as_str();
        let dependency = [WORKTREES]
            .iter()
            .chain(&DEPENDENCY_DIRS)
            .chain(skip)
            .any(|d| {
                !d.is_empty()
                    && (path == *d
                        || path.starts_with(&format!("{d}/"))
                        || path.ends_with(&format!("/{d}"))
                        || path.contains(&format!("/{d}/")))
            });
        if dependency || !included(patterns, path, *is_dir) {
            continue;
        }
        let Some(rel) = relative(path) else { continue };
        let dest = worktree.join(&rel);
        if dest.exists() {
            present += 1;
            continue;
        }
        let from = source.join(&rel);
        if from.is_dir() {
            copy_tree(&from, &dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            if cow::clone(&from, &dest).is_err() {
                fs::copy(&from, &dest).with_context(|| format!("cannot copy {path}"))?;
            }
        }
        copied.push(path.to_owned());
    }
    Ok((copied, present))
}

/// Whether a pattern names something inside the folder `dir`, such as `.bundle/config` for `.bundle`.
fn names_inside(patterns: &[String], dir: &str) -> bool {
    patterns.iter().filter(|p| !p.starts_with('!')).any(|p| {
        let p = p.trim_start_matches('/');
        let literal = &p[..p.find(['*', '?', '[']).unwrap_or(p.len())];
        literal.starts_with(&format!("{dir}/"))
    })
}

/// The files under `rel` in `root`, relative to `root`, with `/` separators.
fn files_under(root: &Path, rel: &str) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root.join(rel)) else {
        return found;
    };
    for entry in entries.flatten() {
        let name = format!("{rel}/{}", entry.file_name().to_string_lossy());
        match entry.file_type() {
            Ok(kind) if kind.is_dir() && name == WORKTREES => {}
            Ok(kind) if kind.is_dir() => found.extend(files_under(root, &name)),
            Ok(_) => found.push(name),
            Err(_) => {}
        }
    }
    found
}

/// Whether the patterns take `path`: the last pattern that matches it, or one of its folders, decides.
pub(crate) fn included(patterns: &[String], path: &str, is_dir: bool) -> bool {
    let mut take = false;
    for pattern in patterns {
        let (negated, pattern) = match pattern.strip_prefix('!') {
            Some(p) => (true, p),
            None => (false, pattern.as_str()),
        };
        let dir_only = pattern.ends_with('/');
        let pattern = pattern.trim_end_matches('/');
        let anchored = pattern.trim_start_matches('/');
        let full = match pattern.contains('/') {
            true => anchored.to_owned(),
            false => format!("**/{anchored}"),
        };
        // The path itself, then each of its folders: a pattern that takes a folder takes what is inside.
        let mut prefixes: Vec<(&str, bool)> = path
            .match_indices('/')
            .map(|(i, _)| (&path[..i], true))
            .collect();
        prefixes.push((path, is_dir));
        if prefixes
            .iter()
            .any(|(p, dir)| (*dir || !dir_only) && glob(full.as_bytes(), p.as_bytes()))
        {
            take = !negated;
        }
    }
    take
}

/// `*` and `?` stay within a folder, `**` crosses them.
fn glob(pattern: &[u8], text: &[u8]) -> bool {
    if let Some(rest) = pattern.strip_prefix(b"**") {
        let rest = rest.strip_prefix(b"/").unwrap_or(rest);
        return rest.is_empty()
            || (0..=text.len()).any(|i| (i == 0 || text[i - 1] == b'/') && glob(rest, &text[i..]));
    }
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            glob(&pattern[1..], text)
                || (text.first().is_some_and(|&c| c != b'/') && glob(pattern, &text[1..]))
        }
        (Some(b'?'), Some(&c)) if c != b'/' => glob(&pattern[1..], &text[1..]),
        (Some(a), Some(b)) if a == b => glob(&pattern[1..], &text[1..]),
        _ => false,
    }
}

/// Copies a folder of data (local config, a SQLite file), cloning each file when the file system can. Links
/// are copied as links.
pub(crate) fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if cow::clone(from, to).is_ok() {
        return Ok(());
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let (src, dest) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&src, &dest)?;
        } else if kind.is_symlink() {
            copy_link(&src, &dest)?;
        } else {
            fs::copy(&src, &dest).with_context(|| format!("cannot copy {}", src.display()))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_link(src: &Path, dest: &Path) -> Result<()> {
    std::os::unix::fs::symlink(fs::read_link(src)?, dest)?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_link(src: &Path, dest: &Path) -> Result<()> {
    fs::copy(src, dest).with_context(|| format!("cannot copy {}", src.display()))?;
    Ok(())
}
