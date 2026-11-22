//! Claude Code's own skills, agents, commands, output styles and rules, and its memory (`CLAUDE.md` by scope,
//! automatic memory): read, edit with a diff, create, and delete into TermF's trash. Only Markdown files under
//! Claude Code's own folders are ever touched (CLAUDE-CODE.md: there is no command for these, the files are
//! the interface).

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use crate::settings::{self, Scope};
use crate::{Env, KINDS, mask};

/// A file bigger than this is not a skill or a memory: TermF does not open it.
const MAX_SIZE: u64 = 1024 * 1024;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Text {
    pub path: String,
    /// Masked unless revealed.
    pub text: String,
    /// Masking changed something: this text cannot be saved back.
    pub masked: bool,
    /// Managed by the organization: never written.
    pub read_only: bool,
    /// The file as read (`settings::fingerprint`), which saving gives back; `None`: not there yet.
    pub hash: Option<String>,
}

/// Where each kind lives in a scope: `~/.claude/<kind>` or `<repo>/.claude/<kind>`.
fn kind_dir(env: &Env, kind: &str, scope: Scope) -> Result<PathBuf> {
    ensure!(KINDS.contains(&kind), "unknown kind {kind}");
    Ok(match scope {
        Scope::User => env.claude_dir.join(kind),
        Scope::Project => env
            .repo
            .as_ref()
            .context("choose a repository for the project scope")?
            .join(".claude")
            .join(kind),
        _ => bail!("{kind} live in the user or the project scope"),
    })
}

/// The memory files of each scope, whether they exist or not.
fn memory_files(env: &Env) -> Vec<PathBuf> {
    let mut files = vec![env.claude_dir.join("CLAUDE.md")];
    if let Some(repo) = &env.repo {
        files.extend([
            repo.join("CLAUDE.md"),
            repo.join(".claude/CLAUDE.md"),
            repo.join("CLAUDE.local.md"),
        ]);
    }
    files
}

/// `autoMemoryDirectory` from the scopes Claude Code reads it in: a cloned repository's shared settings could
/// otherwise point TermF at any folder. Empty or relative, it is ignored.
pub fn auto_memory_dir(settings: &settings::Settings) -> Option<PathBuf> {
    [Scope::Managed, Scope::Local, Scope::User]
        .into_iter()
        .find_map(|scope| {
            let file = settings.files.iter().find(|f| f.scope == scope)?;
            file.value.as_ref()?["autoMemoryDirectory"]
                .as_str()
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .map(termf_core::expand_tilde)
        })
        .filter(|d| d.is_absolute())
}

/// Automatic memory: one folder per project, or the one `autoMemoryDirectory` names.
fn memory_dirs(env: &Env) -> Vec<PathBuf> {
    match auto_memory_dir(&settings::read(env)) {
        Some(dir) => vec![dir],
        None => fs::read_dir(env.claude_dir.join("projects"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path().join("memory"))
            .collect(),
    }
}

/// `path` with every link on the way followed; for what is not there yet, its folder's.
fn real(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => real(dir).join(name),
        _ => path.to_path_buf(),
    })
}

/// `path` as given, once checked to be a Markdown file TermF may write: in a kind's folder, a memory file or an
/// automatic memory folder. Nothing else, whatever the page sends. Links are followed: what they point to must
/// be such a file too, and a repository's links must stay in the repository (a clone could bring a `SKILL.md`
/// that points to `~/.ssh/authorized_keys`).
pub fn writable(env: &Env, path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute() && !path.components().any(|c| c == Component::ParentDir),
        "not a path TermF writes: {}",
        path.display()
    );
    let md = |p: &Path| p.extension().is_some_and(|e| e == "md");
    ensure!(md(path), "only Markdown files are edited here");
    let target = real(path);
    let mut dirs: Vec<PathBuf> = KINDS
        .iter()
        .flat_map(|k| [Scope::User, Scope::Project].map(|s| kind_dir(env, k, s)))
        .flatten()
        .collect();
    dirs.extend(memory_dirs(env));
    let inside = dirs.iter().any(|d| {
        let root = real(d);
        path.starts_with(d) && path != d && target.starts_with(&root) && target != root
    });
    ensure!(
        inside || memory_files(env).iter().any(|f| f == path),
        "not one of Claude Code's skills, agents, commands, styles, rules or memory files: {}",
        path.display()
    );
    let repo = env.repo.as_ref().filter(|r| path.starts_with(r));
    ensure!(
        md(&target) && repo.is_none_or(|r| target.starts_with(real(r))),
        "{} is a link to {}, which TermF does not write",
        path.display(),
        target.display()
    );
    Ok(path.to_path_buf())
}

fn managed_memory(env: &Env, path: &Path) -> bool {
    path == env.managed_dir.join("CLAUDE.md")
}

fn read_text(path: &Path) -> Result<Option<String>> {
    match fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
        Ok(m) => ensure!(
            m.len() <= MAX_SIZE,
            "{} is too big to open here",
            path.display()
        ),
    }
    fs::read_to_string(path)
        .map(Some)
        .with_context(|| format!("cannot read {}", path.display()))
}

/// Masks line by line, like an export.
pub fn mask_lines(text: &str) -> String {
    text.split_inclusive('\n').map(mask::mask_text).collect()
}

/// A file's text, masked unless `reveal` (asked to edit it). A file not there yet reads as empty.
pub fn read(env: &Env, path: &Path, reveal: bool) -> Result<Text> {
    let read_only = managed_memory(env, path);
    if !read_only {
        writable(env, path)?;
    }
    let found = read_text(path)?;
    let hash = found.as_deref().map(settings::fingerprint);
    let text = found.unwrap_or_default();
    let shown = if reveal && !read_only {
        text.clone()
    } else {
        mask_lines(&text)
    };
    Ok(Text {
        path: path.to_string_lossy().into_owned(),
        masked: shown != text,
        text: shown,
        read_only,
        hash,
    })
}

/// What saving `text` would change. `hash` is the file as the editor read it (`None`: a file to create): a file
/// changed on disk since, or created meanwhile, is never overwritten.
pub fn preview(
    env: &Env,
    path: &Path,
    text: &str,
    hash: Option<&str>,
) -> Result<settings::Preview> {
    let path = writable(env, path)?;
    ensure!(
        !text.contains(mask::MASK),
        "this text still holds masked values ({}): they would replace your secrets",
        mask::MASK
    );
    let before = read_text(&path)?;
    match (before.as_deref(), hash) {
        (Some(_), None) => bail!("{} exists already", path.display()),
        (Some(now), Some(hash)) if settings::fingerprint(now) != hash => {
            bail!(
                "{} changed since you opened it: open it again",
                path.display()
            )
        }
        (None, Some(_)) => bail!("{} was deleted since you opened it", path.display()),
        _ => {}
    }
    let text = if text.is_empty() || text.ends_with('\n') {
        text.to_owned()
    } else {
        format!("{text}\n")
    };
    Ok(settings::Preview {
        path: path.to_string_lossy().into_owned(),
        diff: line_diff(before.as_deref().unwrap_or_default(), &text),
        hash: settings::fingerprint(before.as_deref().unwrap_or_default()),
        text,
    })
}

/// A line diff of a whole document (`-` removed, `+` added, ` ` context), three lines of context around each
/// change and `…` between distant ones; empty when nothing changes. Too big to compare line by line, it is one
/// block of removed then added lines.
pub fn line_diff(before: &str, after: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > 4_000_000 {
        return settings::diff(before, after);
    }
    // Longest common subsequence from the end, so the walk below goes forward.
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut ops = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            ops.push((' ', a[i]));
            (i, j) = (i + 1, j + 1);
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            ops.push(('-', a[i]));
            i += 1;
        } else {
            ops.push(('+', b[j]));
            j += 1;
        }
    }
    let mut shown = vec![false; ops.len()];
    for (k, _) in ops.iter().enumerate().filter(|(_, (c, _))| *c != ' ') {
        shown[k.saturating_sub(3)..(k + 4).min(ops.len())].fill(true);
    }
    let mut out = String::new();
    let mut gap = false;
    for ((c, line), show) in ops.into_iter().zip(shown) {
        if !show {
            gap = true;
            continue;
        }
        if gap && !out.is_empty() {
            out.push_str("…\n");
        }
        gap = false;
        out.push(c);
        out.push_str(line);
        out.push('\n');
    }
    out
}

pub fn save(env: &Env, path: &Path, text: &str, hash: Option<&str>) -> Result<settings::Preview> {
    let preview = preview(env, path, text, hash)?;
    settings::write_atomic(Path::new(&preview.path), &preview.text)?;
    Ok(preview)
}

/// A name Claude Code takes as a command or a file name: lowercase letters, digits and dashes.
pub fn check_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 64
            && !name.starts_with('-')
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'),
        "a name is lowercase letters, digits, - and _ (64 at most): {name}"
    );
    Ok(())
}

/// The file a new item of `kind` named `name` gets in `scope`: `skills/<name>/SKILL.md`, else `<kind>/<name>.md`.
pub fn item_path(env: &Env, kind: &str, scope: Scope, name: &str) -> Result<PathBuf> {
    check_name(name)?;
    let dir = kind_dir(env, kind, scope)?;
    Ok(if kind == "skills" {
        dir.join(name).join("SKILL.md")
    } else {
        dir.join(format!("{name}.md"))
    })
}

/// The text a new item starts with: the frontmatter Claude Code reads for its kind.
pub fn template(kind: &str, name: &str, description: &str) -> String {
    // A JSON string is a valid YAML scalar, whatever the description holds.
    let description = serde_json::Value::from(description.trim()).to_string();
    match kind {
        "commands" => format!("---\ndescription: {description}\n---\n\n"),
        "rules" => format!("# {name}\n\n"),
        _ => format!("---\nname: {name}\ndescription: {description}\n---\n\n"),
    }
}

pub fn create(
    env: &Env,
    kind: &str,
    scope: Scope,
    name: &str,
    description: &str,
) -> Result<PathBuf> {
    let path = item_path(env, kind, scope, name)?;
    let taken = if kind == "skills" {
        path.parent().unwrap_or(&path).exists()
    } else {
        path.exists()
    };
    ensure!(!taken, "{} exists already", path.display());
    save(env, &path, &template(kind, name, description), None)?;
    Ok(path)
}

/// Moves `path` into `trash/<seconds>-<name>`, never over something: a rename, else a copy then a removal
/// (another disk).
pub fn trash(path: &Path, trash: &Path) -> Result<PathBuf> {
    fs::create_dir_all(trash)?;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut dest = trash.join(format!("{secs}-{name}"));
    let mut n = 1;
    while dest.exists() {
        dest = trash.join(format!("{secs}-{n}-{name}"));
        n += 1;
    }
    if fs::rename(path, &dest).is_err() {
        copy(path, &dest)?;
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(dest)
}

/// Puts `staged`, written beforehand on the same disk, in place of `dest`, whose old version goes to the trash:
/// if the swap fails, the old one comes back. Returns where the old one went.
pub fn replace(dest: &Path, staged: &Path, trash_dir: &Path) -> Result<PathBuf> {
    let old = trash(dest, trash_dir)?;
    if let Err(e) = fs::rename(staged, dest) {
        if fs::rename(&old, dest).is_err() {
            let _ = copy(&old, dest);
        }
        return Err(e).with_context(|| format!("cannot write {}", dest.display()));
    }
    Ok(old)
}

fn copy(src: &Path, dest: &Path) -> Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        fs::copy(src, dest).with_context(|| format!("cannot copy {}", src.display()))?;
    }
    Ok(())
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Deleted {
    /// Where it went in TermF's trash.
    pub trashed: String,
    /// `MEMORY.md`, whose line pointing to a deleted memory was removed.
    pub index: Option<String>,
}

/// Deletes an item into `trash_dir`: a skill's whole folder, any other file alone. An automatic memory also
/// leaves the index, `MEMORY.md`, whose lines linking to it go.
pub fn delete(env: &Env, path: &Path, trash_dir: &Path) -> Result<Deleted> {
    let path = writable(env, path)?;
    ensure!(path.is_file(), "{} is not there", path.display());
    let skills: Vec<PathBuf> = [Scope::User, Scope::Project]
        .into_iter()
        .filter_map(|s| kind_dir(env, "skills", s).ok())
        .collect();
    let target = match path.parent() {
        Some(dir)
            if path.file_name().is_some_and(|n| n == "SKILL.md")
                && skills.iter().any(|s| dir.parent() == Some(s)) =>
        {
            dir.to_path_buf()
        }
        _ => path.clone(),
    };
    let memory = memory_dirs(env)
        .into_iter()
        .find(|d| path.parent() == Some(d))
        .filter(|_| path.file_name().is_some_and(|n| n != "MEMORY.md"));
    let trashed = trash(&target, trash_dir)?;
    let mut index = None;
    if let Some(dir) = memory {
        let file = dir.join("MEMORY.md");
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if let Some(text) = read_text(&file)? {
            let link = format!("]({name})");
            let kept: String = text
                .split_inclusive('\n')
                .filter(|l| !l.contains(&link))
                .collect();
            if kept != text {
                settings::write_atomic(&file, &kept)?;
                index = Some(file.to_string_lossy().into_owned());
            }
        }
    }
    Ok(Deleted {
        trashed: trashed.to_string_lossy().into_owned(),
        index,
    })
}
