//! The daily work in a repository or a worktree: changes staged by file, hunk or line, commits, stashes, merges,
//! rebases, cherry-picks and reverts with their conflicts, tags, a file's history and blame, resets and leased
//! force pushes. As in the rest of the crate, git's machine formats only, and git's own refusals.

use std::collections::BTreeMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use super::{
    Change, Commit, Git, Tip, Unique, checked_name, parse_log, parse_status, short_ref,
};

/// The most diff lines read at once; the files past it are only counted.
const MAX_DIFF_LINES: usize = 20_000;
/// The most lines a blame lists.
const MAX_BLAME_LINES: usize = 20_000;
/// An untracked file larger than this is not shown line by line.
const MAX_UNTRACKED_BYTES: u64 = 1 << 20;
/// The most commits of one file's history.
pub const MAX_FILE_COMMITS: usize = 5000;

/// Options that make `git diff` the same whatever the user's config: no color, no external or text-converting
/// driver (their output would not apply back), the usual prefixes and context.
const DIFF_ARGS: [&str; 6] = [
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "-U3",
];

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    /// `+`, `-` or ` `.
    pub kind: char,
    pub text: String,
    /// Its numbers in the old and the new version.
    pub old: Option<u32>,
    pub new: Option<u32>,
    /// The file ends there without a newline.
    pub no_eol: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// What git names after `@@`, such as the enclosing function.
    pub section: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    /// The path before a rename or a copy.
    pub from: Option<String>,
    /// `A` added, `D` deleted, `R` renamed, `C` copied, `M` otherwise.
    pub status: char,
    pub binary: bool,
    /// A plain change of the content: its hunks and lines can be staged one by one.
    pub partial: bool,
    pub added: u32,
    pub removed: u32,
    pub hunks: Vec<Hunk>,
    /// Past the size cap: counted, not listed.
    pub cut: bool,
    /// Not tracked yet: the file itself, or the files of the folder.
    pub untracked: Vec<String>,
    /// What it was when read: an action on it is refused once it changed.
    pub id: String,
    #[serde(skip)]
    header: Vec<String>,
}

/// Lines of a hunk to stage, unstage or discard; all of them when `lines` is missing.
#[derive(Deserialize, Debug, Clone)]
pub struct Pick {
    pub hunk: usize,
    #[serde(default)]
    pub lines: Option<Vec<usize>>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

impl OperationKind {
    fn verb(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
        }
    }
}

/// A merge, rebase, cherry-pick or revert that stopped, for conflicts or otherwise.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    pub kind: OperationKind,
    /// The commit merged, picked, reverted, or replayed by the rebase.
    pub theirs: Option<String>,
    /// A rebase: the branch rebased, short, and the commit it goes onto.
    pub branch: Option<String>,
    pub onto: Option<String>,
    /// A rebase: the commit being replayed, of how many.
    pub step: u32,
    pub total: u32,
}

/// What an operation that may stop on conflicts did.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// git said it succeeded.
    pub ok: bool,
    /// An operation is still in progress, or conflicts are left to resolve.
    pub pending: bool,
    /// What git said, its errors included.
    pub output: String,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Step {
    Continue,
    Skip,
    Abort,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Resolution {
    /// The version of the current branch (in a rebase: the branch rebased onto).
    Ours,
    /// The version merged in (in a rebase: the commit replayed).
    Theirs,
    /// The file as edited, its markers removed.
    Edited,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stash {
    /// `n` of `stash@{n}`.
    pub index: usize,
    pub oid: String,
    /// git's: `On main: message`, or `WIP on main: <commit>` without one.
    pub message: String,
    pub date: i64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,
    /// What the tag ref holds: the tag object of an annotated tag, else the commit.
    pub oid: String,
    /// The commit it designates.
    pub target: String,
    pub annotated: bool,
    /// An annotated tag's own message, first line; empty for a lightweight one.
    pub message: String,
    pub tagger: String,
    /// When it was made (annotated), else its commit's date.
    pub date: i64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetail {
    pub commit: Commit,
    pub email: String,
    /// The whole message, subject and body.
    pub message: String,
    /// Against its first parent.
    pub files: Vec<FileDiff>,
    /// A remote branch that already holds it.
    pub remote: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileCommit {
    #[serde(flatten)]
    pub commit: Commit,
    /// The file's path in that commit: it follows renames.
    pub path: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileHistory {
    pub commits: Vec<FileCommit>,
    pub more: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlameLine {
    /// The commit that last changed it; zeros when not committed yet.
    pub oid: String,
    pub text: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Blame {
    pub lines: Vec<BlameLine>,
    pub commits: BTreeMap<String, Tip>,
    /// Lines past the cap were left out.
    pub cut: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlan {
    /// The commits replayed: the newest ones, and how many.
    pub commits: Vec<Commit>,
    pub total: usize,
    /// How many of them a remote branch already holds: rewriting them calls for a force push.
    pub pushed: usize,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResetPlan {
    pub branch: String,
    /// Commits the branch leaves behind.
    pub leaving: usize,
    /// Of them, those no other branch or tag holds.
    pub lost: Unique,
    /// Changes to tracked files, which a hard reset loses.
    pub changes: Vec<Change>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForcePlan {
    pub remote: String,
    /// The remote branch's commit as last fetched: the lease.
    pub theirs: String,
    /// Its commits the local branch does not have: the push drops them.
    pub lost: Unique,
}

fn hash(parts: &[&[u8]]) -> String {
    let mut h = DefaultHasher::new();
    parts.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// A path as git writes it in a diff: C-quoted when it holds special characters.
fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return s.to_owned();
    };
    let mut out = Vec::new();
    let mut bytes = inner.bytes();
    while let Some(b) = bytes.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        match bytes.next() {
            Some(b'n') => out.push(b'\n'),
            Some(b't') => out.push(b'\t'),
            Some(b'r') => out.push(b'\r'),
            Some(b'a') => out.push(7),
            Some(b'b') => out.push(8),
            Some(b'f') => out.push(12),
            Some(b'v') => out.push(11),
            Some(d @ b'0'..=b'7') => {
                let mut n = u32::from(d - b'0');
                for _ in 0..2 {
                    match bytes.clone().next() {
                        Some(d @ b'0'..=b'7') => {
                            n = n * 8 + u32::from(d - b'0');
                            bytes.next();
                        }
                        _ => break,
                    }
                }
                out.push(n as u8);
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The path of a `---` / `+++` line, without its `a/` or `b/`; `None` for `/dev/null`.
fn side_path(rest: &str) -> Option<String> {
    // git ends a name holding a space with a tab.
    let rest = rest.strip_suffix('\t').unwrap_or(rest);
    if rest == "/dev/null" {
        return None;
    }
    let name = unquote(rest);
    Some(name.get(2..).unwrap_or_default().to_owned())
}

fn parse_hunk_header(line: &str) -> Option<Hunk> {
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, section) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let range = |r: &str| -> Option<(u32, u32)> {
        match r.split_once(',') {
            Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
            None => Some((r.parse().ok()?, 1)),
        }
    };
    let ((old_start, old_lines), (new_start, new_lines)) = (range(old)?, range(new)?);
    Some(Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        section: section.trim().to_owned(),
        lines: Vec::new(),
    })
}

/// Files of a `git diff` (one or many), `max_lines` diff lines kept at most.
fn parse_diff(out: &str, max_lines: usize) -> Vec<FileDiff> {
    struct Raw {
        diff: FileDiff,
        text: String,
        old_side: Option<String>,
        new_side: Option<String>,
        git_line: String,
    }
    let mut files: Vec<Raw> = Vec::new();
    let mut left = max_lines;
    let mut in_hunk = false;
    let (mut old, mut new) = (0, 0);
    let mut pieces: Vec<&str> = out.split('\n').collect();
    if pieces.last() == Some(&"") {
        pieces.pop();
    }
    for line in pieces {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            in_hunk = false;
            files.push(Raw {
                diff: FileDiff {
                    status: 'M',
                    header: vec![line.to_owned()],
                    cut: left == 0,
                    ..FileDiff::default()
                },
                text: format!("{line}\n"),
                old_side: None,
                new_side: None,
                git_line: rest.to_owned(),
            });
            continue;
        }
        let Some(raw) = files.last_mut() else {
            continue;
        };
        raw.text.push_str(line);
        raw.text.push('\n');
        let file = &mut raw.diff;
        if line.starts_with("@@ ") {
            if let Some(hunk) = parse_hunk_header(line) {
                (old, new) = (hunk.old_start, hunk.new_start);
                in_hunk = true;
                if !file.cut {
                    file.hunks.push(hunk);
                }
            }
            continue;
        }
        if !in_hunk {
            if let Some(rest) = line.strip_prefix("--- ") {
                raw.old_side = side_path(rest);
            } else if let Some(rest) = line.strip_prefix("+++ ") {
                raw.new_side = side_path(rest);
            } else if line.starts_with("new file mode") {
                file.status = 'A';
            } else if line.starts_with("deleted file mode") {
                file.status = 'D';
            } else if let Some(p) = line.strip_prefix("rename from ") {
                (file.status, file.from) = ('R', Some(unquote(p)));
            } else if let Some(p) = line.strip_prefix("copy from ") {
                (file.status, file.from) = ('C', Some(unquote(p)));
            } else if let Some(p) = line
                .strip_prefix("rename to ")
                .or_else(|| line.strip_prefix("copy to "))
            {
                file.path = unquote(p);
            } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
                file.binary = true;
            }
            file.header.push(line.to_owned());
            continue;
        }
        let Some(kind) = line.chars().next() else {
            continue;
        };
        if kind == '\\' {
            if let Some(last) = file.hunks.last_mut().and_then(|h| h.lines.last_mut()) {
                last.no_eol = true;
            }
            continue;
        }
        let (o, n) = match kind {
            ' ' => (Some(old), Some(new)),
            '-' => (Some(old), None),
            '+' => (None, Some(new)),
            _ => continue,
        };
        old += u32::from(o.is_some());
        new += u32::from(n.is_some());
        file.added += u32::from(kind == '+');
        file.removed += u32::from(kind == '-');
        if file.cut {
            continue;
        }
        if left == 0 {
            // A file is shown whole or not at all.
            file.cut = true;
            file.hunks.clear();
            continue;
        }
        left -= 1;
        if let Some(hunk) = file.hunks.last_mut() {
            hunk.lines.push(DiffLine {
                kind,
                text: line[1..].to_owned(),
                old: o,
                new: n,
                no_eol: false,
            });
        }
    }
    files
        .into_iter()
        .map(|raw| {
            let mut file = raw.diff;
            if file.path.is_empty() {
                file.path = raw
                    .new_side
                    .or(raw.old_side)
                    .unwrap_or_else(|| same_halves(&raw.git_line));
            }
            file.binary |= file.header.iter().any(|h| h == "GIT binary patch");
            file.partial = !file.binary
                && !file.cut
                && file.status == 'M'
                && !file.hunks.is_empty()
                && file.header.iter().all(|h| {
                    ["diff --git ", "index ", "--- ", "+++ "]
                        .iter()
                        .any(|p| h.starts_with(p))
                });
            file.id = hash(&[raw.text.as_bytes()]);
            file
        })
        .collect()
}

/// The path of `diff --git a/<p> b/<p>` when nothing else names it (a mode change, a binary file): both halves are
/// the same path.
fn same_halves(rest: &str) -> String {
    let n = rest.len().saturating_sub(5) / 2;
    unquote(rest.get(2..2 + n).unwrap_or(rest))
}

/// The patch of the picked lines of `diff`, for `git apply` (`reverse`: for `git apply --reverse`, so the lines
/// not picked are those of the side applied to). git checks every context line, so a patch built from a diff that
/// no longer matches the file is refused rather than misapplied.
fn build_patch(diff: &FileDiff, picks: &[Pick], reverse: bool) -> Result<String> {
    ensure!(
        diff.partial,
        "{} cannot be staged in parts: stage the whole file",
        diff.path
    );
    let mut out = String::new();
    for h in &diff.header {
        out.push_str(h);
        out.push('\n');
    }
    // How far the new side has moved from the old one through the hunks already written.
    let mut shift: i64 = 0;
    let mut any = false;
    for (i, hunk) in diff.hunks.iter().enumerate() {
        let Some(pick) = picks.iter().find(|p| p.hunk == i) else {
            continue;
        };
        let mut body = String::new();
        let (mut olds, mut news, mut changed) = (0u32, 0u32, false);
        for (j, line) in hunk.lines.iter().enumerate() {
            let picked = pick.lines.as_ref().is_none_or(|l| l.contains(&j));
            let kind = match (line.kind, picked) {
                (' ', _) => ' ',
                (k, true) => {
                    changed = true;
                    k
                }
                // A line left as it is stays on the side applied to, and is missing from the other.
                ('+', false) if reverse => ' ',
                ('-', false) if !reverse => ' ',
                _ => continue,
            };
            olds += u32::from(kind != '+');
            news += u32::from(kind != '-');
            body.push(kind);
            body.push_str(&line.text);
            body.push('\n');
            if line.no_eol {
                body.push_str("\\ No newline at end of file\n");
            }
        }
        if !changed {
            continue;
        }
        any = true;
        // git numbers an empty side from the line before it.
        let first = |start: u32, count: u32| i64::from(start) - i64::from(count > 0);
        let start = |first: i64, count: u32| first + i64::from(count > 0);
        let (old_start, new_start) = if reverse {
            let new_first = first(hunk.new_start, hunk.new_lines);
            (start(new_first - shift, olds), start(new_first, news))
        } else {
            let old_first = first(hunk.old_start, hunk.old_lines);
            (start(old_first, olds), start(old_first + shift, news))
        };
        shift += i64::from(news) - i64::from(olds);
        out.push_str(&format!(
            "@@ -{old_start},{olds} +{new_start},{news} @@\n{body}"
        ));
    }
    ensure!(any, "no change picked");
    Ok(out)
}

fn parse_blame(out: &str) -> Blame {
    let mut blame = Blame {
        lines: Vec::new(),
        commits: BTreeMap::new(),
        cut: false,
    };
    let mut oid = String::new();
    for line in out.lines() {
        if let Some(text) = line.strip_prefix('\t') {
            if blame.lines.len() == MAX_BLAME_LINES {
                blame.cut = true;
                break;
            }
            blame.lines.push(BlameLine {
                oid: oid.clone(),
                text: text.to_owned(),
            });
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key.len() >= 40 && key.bytes().all(|b| b.is_ascii_hexdigit()) {
            oid = key.to_owned();
            blame.commits.entry(oid.clone()).or_insert_with(|| Tip {
                oid: oid.clone(),
                author: String::new(),
                date: 0,
                subject: String::new(),
            });
            continue;
        }
        let Some(tip) = blame.commits.get_mut(&oid) else {
            continue;
        };
        match key {
            "author" => tip.author = value.to_owned(),
            "author-time" => tip.date = value.parse().unwrap_or(0),
            "summary" => tip.subject = value.to_owned(),
            _ => {}
        }
    }
    blame
}

/// Records of `log --follow -z --name-status` with `HISTORY_FORMAT`.
fn parse_history(out: &str, path: &str) -> Vec<FileCommit> {
    let mut last = path.to_owned();
    out.split('\x01')
        .filter(|r| !r.is_empty())
        .filter_map(|record| {
            let f: Vec<&str> = record.split('\0').collect();
            let commit = parse_log(&f.get(..5)?.join("\0")).pop()?;
            // The status letters (after a newline), then the file's paths: the last one is its name in this commit.
            if let Some(name) = f[5..]
                .iter()
                .rfind(|s| !s.is_empty() && !s.starts_with('\n'))
            {
                last = (*name).to_owned();
            }
            Some(FileCommit {
                commit,
                path: last.clone(),
            })
        })
        .collect()
}

const HISTORY_FORMAT: &str = "--format=%x01%H%x00%P%x00%an%x00%at%x00%s";
const TAG_FORMAT: &str = "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(objecttype)%00%(taggername)%00%(creatordate:unix)%00%(contents:subject)%00";

fn parse_tags(out: &str) -> Vec<Tag> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .as_chunks::<7>()
        .0
        .iter()
        .map(|f| {
            let annotated = f[3] == "tag";
            Tag {
                name: short_ref(f[0].trim_start_matches('\n')),
                oid: f[1].to_owned(),
                target: if f[2].is_empty() { f[1] } else { f[2] }.to_owned(),
                annotated,
                message: if annotated { f[6] } else { "" }.to_owned(),
                tagger: f[4].to_owned(),
                date: f[5].parse().unwrap_or(0),
            }
        })
        .collect()
}

fn parse_stashes(out: &str) -> Vec<Stash> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .as_chunks::<4>()
        .0
        .iter()
        .filter_map(|f| {
            let index = f[1]
                .trim_start_matches('\n')
                .strip_prefix("stash@{")?
                .strip_suffix('}')?
                .parse()
                .ok()?;
            Some(Stash {
                index,
                oid: f[0].trim_start_matches('\n').to_owned(),
                message: f[2].to_owned(),
                date: f[3].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// A path given by the UI, read directly: relative and inside the folder.
fn inside(dir: &Path, path: &str) -> Result<PathBuf> {
    let rel = Path::new(path);
    ensure!(
        !path.is_empty()
            && rel
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "invalid path: {path:?}"
    );
    Ok(dir.join(rel))
}

/// The first line that still holds a conflict marker, counted from 1.
fn marker_line(text: &str) -> Option<usize> {
    text.lines()
        .position(|l| {
            ["<<<<<<<", ">>>>>>>"].iter().any(|m| {
                l.strip_prefix(m)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
            })
        })
        .map(|n| n + 1)
}

impl Git {
    /// git with `input` on its standard input.
    fn run_input(&self, dir: &Path, args: &[&str], input: &str) -> Result<String> {
        let mut child = self
            .git(dir, args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("cannot run git: is it installed?")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        ensure!(
            out.status.success(),
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn untracked(&self, dir: &Path, path: &str) -> Result<Vec<String>> {
        let out = self.run(
            dir,
            &["ls-files", "-z", "--others", "--exclude-standard", "--", path],
        )?;
        Ok(out
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect())
    }

    /// A file's diff: the staged one (index against HEAD; `from`, a rename's source), else the work tree's against
    /// the index, an untracked file as all new.
    pub fn diff(
        &self,
        dir: &Path,
        path: &str,
        from: Option<&str>,
        staged: bool,
    ) -> Result<FileDiff> {
        inside(dir, path)?;
        if !staged {
            let untracked = self.untracked(dir, path)?;
            if !untracked.is_empty() {
                return self.untracked_diff(dir, path, untracked);
            }
        }
        let mut args = vec!["diff"];
        args.extend(DIFF_ARGS);
        if staged {
            args.extend(["--cached", "-M"]);
        }
        args.push("--");
        args.extend(from);
        args.push(path);
        let out = self.run(dir, &args)?;
        Ok(parse_diff(&out, MAX_DIFF_LINES)
            .into_iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| FileDiff {
                path: path.to_owned(),
                status: 'M',
                id: hash(&[]),
                ..FileDiff::default()
            }))
    }

    fn untracked_diff(&self, dir: &Path, path: &str, files: Vec<String>) -> Result<FileDiff> {
        let list = files.join("\0");
        let mut diff = FileDiff {
            path: path.to_owned(),
            status: 'A',
            id: hash(&[list.as_bytes()]),
            ..FileDiff::default()
        };
        let file = inside(dir, path)?;
        if files.len() == 1 && files[0] == path && file.is_file() {
            let size = file.metadata()?.len();
            if size > MAX_UNTRACKED_BYTES {
                diff.cut = true;
            } else {
                let bytes = fs::read(&file)?;
                diff.id = hash(&[list.as_bytes(), &bytes]);
                diff.binary = bytes.iter().take(8000).any(|b| *b == 0);
                if !diff.binary && !bytes.is_empty() {
                    let text = String::from_utf8_lossy(&bytes);
                    let lines: Vec<DiffLine> = text
                        .split_inclusive('\n')
                        .enumerate()
                        .map(|(i, l)| DiffLine {
                            kind: '+',
                            text: l.strip_suffix('\n').unwrap_or(l).to_owned(),
                            old: None,
                            new: Some(i as u32 + 1),
                            no_eol: !l.ends_with('\n'),
                        })
                        .collect();
                    diff.added = lines.len() as u32;
                    diff.hunks.push(Hunk {
                        old_start: 0,
                        old_lines: 0,
                        new_start: 1,
                        new_lines: diff.added,
                        section: String::new(),
                        lines,
                    });
                }
            }
        }
        diff.untracked = files;
        Ok(diff)
    }

    /// The diff of `path` again, refused when it is no longer the one the user saw (`id`).
    fn same_diff(
        &self,
        dir: &Path,
        path: &str,
        from: Option<&str>,
        staged: bool,
        id: &str,
    ) -> Result<FileDiff> {
        let diff = self.diff(dir, path, from, staged)?;
        ensure!(
            diff.id == id,
            "{path} changed since you looked: look at it again"
        );
        Ok(diff)
    }

    fn apply(&self, dir: &Path, patch: &str, flags: &[&str]) -> Result<()> {
        // `--whitespace=nowarn`: an `apply.whitespace = fix` in the user's config would change the lines.
        let args = [&["apply", "--whitespace=nowarn"][..], flags, &["-"]].concat();
        self.run_input(dir, &args, patch)?;
        Ok(())
    }

    /// Stages a whole file (its deletion too), or the picked lines of its work tree diff, seen as `id`.
    pub fn stage(
        &self,
        dir: &Path,
        path: &str,
        picks: Option<&[Pick]>,
        id: Option<&str>,
    ) -> Result<()> {
        inside(dir, path)?;
        match picks {
            None => {
                self.run(dir, &["add", "-A", "--", path])?;
            }
            Some(picks) => {
                let diff = self.same_diff(dir, path, None, false, id.unwrap_or_default())?;
                self.apply(dir, &build_patch(&diff, picks, false)?, &["--cached"])?;
            }
        }
        Ok(())
    }

    /// Unstages a whole file (and a rename's source), or the picked lines of its staged diff, seen as `id`.
    pub fn unstage(
        &self,
        dir: &Path,
        path: &str,
        from: Option<&str>,
        picks: Option<&[Pick]>,
        id: Option<&str>,
    ) -> Result<()> {
        inside(dir, path)?;
        match picks {
            None => {
                let mut args = if self.head(dir)?.is_some() {
                    vec!["restore", "--staged", "--", path]
                } else {
                    // Before the first commit there is no HEAD to restore from.
                    vec!["rm", "--cached", "-r", "-q", "--", path]
                };
                args.extend(from);
                self.run(dir, &args)?;
            }
            Some(picks) => {
                let diff = self.same_diff(dir, path, from, true, id.unwrap_or_default())?;
                self.apply(
                    dir,
                    &build_patch(&diff, picks, true)?,
                    &["--cached", "--reverse"],
                )?;
            }
        }
        Ok(())
    }

    /// Drops the work tree's changes to a file (back to what is staged), or the picked lines of them, or deletes
    /// an untracked file or folder; only when its diff is still the one the user saw (`id`). What is staged stays.
    pub fn discard(&self, dir: &Path, path: &str, picks: Option<&[Pick]>, id: &str) -> Result<()> {
        let diff = self.same_diff(dir, path, None, false, id)?;
        match picks {
            _ if !diff.untracked.is_empty() => {
                ensure!(picks.is_none(), "an untracked file is discarded whole");
                self.run(dir, &["clean", "-f", "-d", "-q", "--", path])?;
            }
            None => {
                self.run(dir, &["restore", "--worktree", "--", path])?;
            }
            Some(picks) => {
                self.apply(dir, &build_patch(&diff, picks, true)?, &["--reverse"])?;
            }
        }
        Ok(())
    }

    /// The remote branch that already holds `oid`, if any.
    fn on_remote(&self, dir: &Path, oid: &str) -> Result<Option<String>> {
        let out = self.run(
            dir,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(symref)",
                "--contains",
                oid,
                "refs/remotes",
            ],
        )?;
        Ok(out
            .lines()
            .filter_map(|l| l.split_once('\0'))
            .find(|(_, symref)| symref.is_empty())
            .map(|(name, _)| short_ref(name)))
    }

    /// Commits what is staged, hooks included, and returns the new commit. `amend` replaces HEAD, which must still
    /// be `head` and on no remote branch.
    pub fn commit(
        &self,
        dir: &Path,
        message: &str,
        amend: bool,
        head: Option<&str>,
    ) -> Result<String> {
        ensure!(!message.trim().is_empty(), "the commit needs a message");
        let mut args = vec!["commit", "-m", message];
        if amend {
            let now = self
                .head(dir)?
                .context("there is no commit to amend yet")?;
            ensure!(
                head == Some(now.as_str()),
                "HEAD moved since you looked: look again before amending"
            );
            if let Some(remote) = self.on_remote(dir, &now)? {
                bail!(
                    "{} is already on {remote}: amending would rewrite a pushed commit, so make a new commit instead",
                    &now[..7.min(now.len())]
                );
            }
            args.push("--amend");
        }
        self.run(dir, &args)?;
        self.head(dir)?.context("no commit was made")
    }

    pub fn stashes(&self, dir: &Path) -> Result<Vec<Stash>> {
        let out = self.run(
            dir,
            &["stash", "list", "-z", "--format=%H%x00%gd%x00%gs%x00%at"],
        )?;
        Ok(parse_stashes(&out))
    }

    fn stash_ref(&self, dir: &Path, index: usize, oid: &str) -> Result<String> {
        let name = format!("stash@{{{index}}}");
        let now = self.lookup(dir, &["rev-parse", "-q", "--verify", &name])?;
        ensure!(
            now.as_deref() == Some(oid),
            "the stash list changed since you looked: look again"
        );
        Ok(name)
    }

    /// Stashes the changes, untracked files too when asked.
    pub fn stash_push(&self, dir: &Path, message: &str, untracked: bool) -> Result<()> {
        let before = self.lookup(dir, &["rev-parse", "-q", "--verify", "refs/stash"])?;
        let mut args = vec!["stash", "push"];
        if untracked {
            args.push("--include-untracked");
        }
        let message = message.trim();
        if !message.is_empty() {
            args.extend(["-m", message]);
        }
        let out = self.run(dir, &args)?;
        let after = self.lookup(dir, &["rev-parse", "-q", "--verify", "refs/stash"])?;
        // With nothing to stash, git says so and succeeds.
        ensure!(after != before, "git stash: {}", out.trim());
        Ok(())
    }

    /// Applies a stash, dropping it when `pop` and it applied cleanly; conflicts are left to resolve.
    pub fn stash_apply(&self, dir: &Path, index: usize, oid: &str, pop: bool) -> Result<Outcome> {
        let name = self.stash_ref(dir, index, oid)?;
        self.operate(dir, &["stash", if pop { "pop" } else { "apply" }, &name])
    }

    pub fn stash_drop(&self, dir: &Path, index: usize, oid: &str) -> Result<()> {
        let name = self.stash_ref(dir, index, oid)?;
        self.run(dir, &["stash", "drop", "-q", &name])?;
        Ok(())
    }

    /// What a stash holds, untracked files included.
    pub fn stash_show(&self, dir: &Path, index: usize, oid: &str) -> Result<Vec<FileDiff>> {
        let name = self.stash_ref(dir, index, oid)?;
        let mut args = vec!["stash", "show", "-p", "--include-untracked", "-M"];
        args.extend(DIFF_ARGS);
        args.push(&name);
        Ok(parse_diff(&self.run(dir, &args)?, MAX_DIFF_LINES))
    }

    /// The operation stopped in `dir`, read from git's own files.
    pub fn operation(&self, dir: &Path) -> Result<Option<Operation>> {
        let names = [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "REBASE_HEAD",
            "rebase-merge",
            "rebase-apply",
        ];
        let mut args = vec!["rev-parse"];
        for name in names {
            args.extend(["--git-path", name]);
        }
        let paths: Vec<PathBuf> = self.run(dir, &args)?.lines().map(|l| dir.join(l)).collect();
        ensure!(paths.len() == names.len(), "unexpected git rev-parse output");
        let read = |p: &Path| {
            fs::read_to_string(p)
                .ok()
                .and_then(|s| s.lines().next().map(|l| l.trim().to_owned()))
                .filter(|s| !s.is_empty())
        };
        let number = |p: PathBuf| read(&p).and_then(|n| n.parse().ok()).unwrap_or(0);
        // `rebase-apply` without `rebasing` is a `git am`, which TermF leaves alone.
        let merge_backend = paths[4].is_dir();
        if merge_backend || paths[5].join("rebasing").exists() {
            let state = if merge_backend { &paths[4] } else { &paths[5] };
            let (step, total) = if merge_backend {
                ("msgnum", "end")
            } else {
                ("next", "last")
            };
            return Ok(Some(Operation {
                kind: OperationKind::Rebase,
                theirs: read(&paths[3]),
                branch: read(&state.join("head-name"))
                    .filter(|h| h.starts_with("refs/"))
                    .map(|h| short_ref(&h)),
                onto: read(&state.join("onto")),
                step: number(state.join(step)),
                total: number(state.join(total)),
            }));
        }
        let kinds = [
            OperationKind::Merge,
            OperationKind::CherryPick,
            OperationKind::Revert,
        ];
        Ok(kinds.into_iter().zip(&paths).find_map(|(kind, p)| {
            read(p).map(|theirs| Operation {
                kind,
                theirs: Some(theirs),
                branch: None,
                onto: None,
                step: 0,
                total: 0,
            })
        }))
    }

    fn conflicted(&self, dir: &Path) -> Result<bool> {
        Ok(!self.run(dir, &["ls-files", "-u", "-z"])?.is_empty())
    }

    /// Refuses to start an operation over one in progress.
    fn idle(&self, dir: &Path) -> Result<()> {
        if let Some(op) = self.operation(dir)? {
            bail!(
                "a {} is in progress here: continue or abort it first",
                op.kind.verb()
            );
        }
        Ok(())
    }

    /// Runs an operation that may stop on conflicts: what git said either way, an error only when it failed
    /// leaving nothing to resolve.
    fn operate(&self, dir: &Path, args: &[&str]) -> Result<Outcome> {
        let out = self.output(dir, args)?;
        let output = [out.stdout, out.stderr]
            .iter()
            .map(|b| String::from_utf8_lossy(b).trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let pending = self.operation(dir)?.is_some() || self.conflicted(dir)?;
        ensure!(
            out.status.success() || pending,
            "git {} failed: {output}",
            args[0]
        );
        Ok(Outcome {
            ok: out.status.success(),
            pending,
            output,
        })
    }

    /// Merges a branch or commit into the current branch, fast-forward when git can.
    pub fn merge(&self, dir: &Path, branch: &str) -> Result<Outcome> {
        let branch = checked_name(branch)?;
        self.idle(dir)?;
        self.operate(dir, &["merge", "--no-edit", branch])
    }

    /// What rebasing the current branch onto `onto` replays, and how much of it is already pushed.
    pub fn rebase_plan(&self, dir: &Path, onto: &str) -> Result<RebasePlan> {
        let onto = checked_name(onto)?;
        let replayed = self.only_in(dir, &["HEAD", "--not", onto])?;
        let unpushed: usize = self
            .run(dir, &["rev-list", "--count", "HEAD", "--not", onto, "--remotes"])?
            .trim()
            .parse()
            .unwrap_or(0);
        Ok(RebasePlan {
            pushed: replayed.total.saturating_sub(unpushed),
            commits: replayed.commits,
            total: replayed.total,
        })
    }

    /// Rebases the current branch onto `onto`, when HEAD is still `head`, the commit the user saw.
    pub fn rebase(&self, dir: &Path, onto: &str, head: &str) -> Result<Outcome> {
        let onto = checked_name(onto)?;
        self.idle(dir)?;
        ensure!(
            self.head(dir)?.as_deref() == Some(head),
            "HEAD moved since you looked: look again before rebasing"
        );
        self.operate(dir, &["rebase", onto])
    }

    pub fn cherry_pick(&self, dir: &Path, oid: &str) -> Result<Outcome> {
        let oid = checked_name(oid)?;
        self.idle(dir)?;
        self.operate(dir, &["cherry-pick", oid])
    }

    pub fn revert(&self, dir: &Path, oid: &str) -> Result<Outcome> {
        let oid = checked_name(oid)?;
        self.idle(dir)?;
        self.operate(dir, &["revert", "--no-edit", oid])
    }

    /// Continues, skips or aborts the operation in progress.
    pub fn proceed(&self, dir: &Path, step: Step) -> Result<Outcome> {
        let op = self
            .operation(dir)?
            .context("no merge, rebase, cherry-pick or revert is in progress here")?;
        ensure!(
            !(op.kind == OperationKind::Merge && step == Step::Skip),
            "a merge cannot be skipped: continue or abort it"
        );
        let flag = match step {
            Step::Continue => "--continue",
            Step::Skip => "--skip",
            Step::Abort => "--abort",
        };
        self.operate(dir, &[op.kind.verb(), flag])
    }

    /// Settles a conflicted file: one side whole (a side that deleted it deletes it), or the file as edited once
    /// no conflict marker is left.
    pub fn resolve(&self, dir: &Path, path: &str, how: Resolution) -> Result<()> {
        let file = inside(dir, path)?;
        let stages: Vec<String> = self
            .run(dir, &["ls-files", "-u", "-z", "--", path])?
            .split('\0')
            .filter_map(|e| e.split_once('\t'))
            .filter_map(|(meta, _)| meta.split(' ').nth(2).map(str::to_owned))
            .collect();
        ensure!(!stages.is_empty(), "{path} is not in conflict");
        let side = match how {
            Resolution::Ours => Some(("--ours", "2")),
            Resolution::Theirs => Some(("--theirs", "3")),
            Resolution::Edited => None,
        };
        match side {
            Some((flag, stage)) if stages.iter().any(|s| s == stage) => {
                self.run(dir, &["checkout", flag, "--", path])?;
                self.run(dir, &["add", "--", path])?;
            }
            Some(_) => {
                self.run(dir, &["rm", "-q", "--", path])?;
            }
            None => {
                if file.is_file() {
                    let text = String::from_utf8_lossy(&fs::read(&file)?).into_owned();
                    if let Some(line) = marker_line(&text) {
                        bail!(
                            "{path} still has conflict markers (line {line}): edit them out, or keep one side"
                        );
                    }
                }
                self.run(dir, &["add", "-A", "--", path])?;
            }
        }
        Ok(())
    }

    pub fn tags(&self, dir: &Path) -> Result<Vec<Tag>> {
        Ok(parse_tags(&self.run(
            dir,
            &["for-each-ref", "--sort=-creatordate", TAG_FORMAT, "refs/tags"],
        )?))
    }

    /// A lightweight tag, or an annotated one with `message`.
    pub fn tag_create(
        &self,
        dir: &Path,
        name: &str,
        target: &str,
        message: Option<&str>,
    ) -> Result<()> {
        let (name, target) = (checked_name(name)?, checked_name(target)?);
        ensure!(
            self.lookup(dir, &["check-ref-format", &format!("refs/tags/{name}")])?
                .is_some(),
            "{name:?} is not a valid tag name"
        );
        let mut args = vec!["tag"];
        if let Some(message) = message.map(str::trim).filter(|m| !m.is_empty()) {
            args.extend(["-a", "-m", message]);
        }
        args.extend([name, target]);
        self.run(dir, &args)?;
        Ok(())
    }

    /// The commits only the tag holds: deleting it would lose them.
    pub fn tag_unique(&self, dir: &Path, name: &str) -> Result<Unique> {
        let full = format!("refs/tags/{}", checked_name(name)?);
        let exclude = format!("--exclude={name}");
        self.only_in(
            dir,
            &[&full, "--not", "--branches", "--remotes", &exclude, "--tags"],
        )
    }

    /// Deletes a tag still at `oid`, the one the user saw.
    pub fn tag_delete(&self, dir: &Path, name: &str, oid: &str) -> Result<()> {
        let full = format!("refs/tags/{}", checked_name(name)?);
        let now = self.lookup(dir, &["rev-parse", "-q", "--verify", &full])?;
        ensure!(
            now.as_deref() == Some(oid),
            "the tag {name} changed since you looked: look again"
        );
        self.run(dir, &["tag", "-d", name])?;
        Ok(())
    }

    /// Pushes a tag to the remote git would push to, never forced; returns that remote.
    pub fn tag_push(&self, dir: &Path, name: &str) -> Result<String> {
        let full = format!("refs/tags/{}", checked_name(name)?);
        ensure!(self.exists(dir, &full)?, "no tag {name}");
        let branch = self
            .lookup(dir, &["symbolic-ref", "-q", "--short", "HEAD"])?
            .unwrap_or_default();
        let remote = self.push_remote(dir, &branch, None)?;
        self.run_remote(dir, &["push", &remote, &format!("{full}:{full}")])?;
        Ok(remote)
    }

    /// A file's commits, newest first, following its renames.
    pub fn history(&self, dir: &Path, path: &str, limit: usize) -> Result<FileHistory> {
        inside(dir, path)?;
        let limit = limit.clamp(1, MAX_FILE_COMMITS);
        let count = format!("-n{}", limit + 1);
        let out = self.run(
            dir,
            &[
                "log",
                "--follow",
                "-M",
                "-z",
                "--name-status",
                HISTORY_FORMAT,
                &count,
                "--",
                path,
            ],
        )?;
        let mut commits = parse_history(&out, path);
        let more = commits.len() > limit;
        commits.truncate(limit);
        Ok(FileHistory { commits, more })
    }

    /// Who last changed each line of a file, its uncommitted changes included.
    pub fn blame(&self, dir: &Path, path: &str) -> Result<Blame> {
        inside(dir, path)?;
        let blame = parse_blame(&self.run(dir, &["blame", "--porcelain", "--", path])?);
        ensure!(
            !blame.lines.iter().any(|l| l.text.contains('\0')),
            "{path} is a binary file"
        );
        Ok(blame)
    }

    /// A commit's message and changes against its first parent, and whether it is pushed.
    pub fn show(&self, dir: &Path, rev: &str) -> Result<CommitDetail> {
        let rev = checked_name(rev)?;
        let out = self.run(
            dir,
            &[
                "log",
                "-1",
                "--format=%H%x00%P%x00%an%x00%at%x00%s%x00%ae%x00%B",
                rev,
                "--",
            ],
        )?;
        let f: Vec<&str> = out.splitn(7, '\0').collect();
        ensure!(f.len() == 7, "no commit {rev}");
        let commit = parse_log(&f[..5].join("\0"))
            .pop()
            .with_context(|| format!("no commit {rev}"))?;
        let mut args = vec!["diff-tree", "-p", "-M", "--no-commit-id"];
        args.extend(DIFF_ARGS);
        match commit.parents.first() {
            Some(parent) => args.push(parent),
            None => args.push("--root"),
        }
        args.push(&commit.oid);
        let files = parse_diff(&self.run(dir, &args)?, MAX_DIFF_LINES);
        Ok(CommitDetail {
            remote: self.on_remote(dir, &commit.oid)?,
            email: f[5].to_owned(),
            message: f[6].trim_end().to_owned(),
            files,
            commit,
        })
    }

    fn current(&self, dir: &Path) -> Result<String> {
        self.lookup(dir, &["symbolic-ref", "-q", "--short", "HEAD"])?
            .context("HEAD is detached: switch to a branch first")
    }

    /// What resetting the current branch to `target` leaves behind and loses.
    pub fn reset_plan(&self, dir: &Path, target: &str) -> Result<ResetPlan> {
        let target = checked_name(target)?;
        let branch = self.current(dir)?;
        let leaving = self
            .run(dir, &["rev-list", "--count", "HEAD", "--not", target])?
            .trim()
            .parse()
            .unwrap_or(0);
        let exclude = format!("--exclude={branch}");
        let lost = self.only_in(
            dir,
            &[
                "HEAD",
                "--not",
                target,
                &exclude,
                "--branches",
                "--remotes",
                "--tags",
            ],
        )?;
        let (changes, _) = parse_status(&self.run(
            dir,
            &["status", "--porcelain=v2", "-z", "--untracked-files=no"],
        )?);
        Ok(ResetPlan {
            branch,
            leaving,
            lost,
            changes,
        })
    }

    /// Moves the current branch to `target`, when HEAD is still `head`, the commit the user saw.
    pub fn reset(&self, dir: &Path, target: &str, mode: ResetMode, head: &str) -> Result<()> {
        let target = checked_name(target)?;
        self.current(dir)?;
        ensure!(
            self.head(dir)?.as_deref() == Some(head),
            "HEAD moved since you looked: look again before resetting"
        );
        let mode = match mode {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        };
        self.run(dir, &["reset", "-q", mode, target])?;
        Ok(())
    }

    /// What force-pushing a local branch would drop on its push remote; never the remote's default branch.
    pub fn force_plan(&self, repo: &Path, branch: &str) -> Result<ForcePlan> {
        let full = format!("refs/heads/{}", checked_name(branch)?);
        let r = self
            .refs(repo, &[&full])?
            .into_iter()
            .find(|r| r.name == full)
            .with_context(|| format!("no local branch {branch}"))?;
        let upstream =
            (!r.upstream.is_empty() && r.upstream_remote != ".").then_some(&r.upstream_remote);
        let remote = self.push_remote(repo, branch, upstream)?;
        ensure!(
            !self.remote_default(repo, &remote, branch)?,
            "{branch} is the default branch of {remote}: TermF never forces it"
        );
        let tracking = format!("refs/remotes/{remote}/{branch}");
        let theirs = self
            .lookup(repo, &["rev-parse", "-q", "--verify", &tracking])?
            .with_context(|| {
                format!("{remote}/{branch} is not known here: fetch, or push it normally")
            })?;
        let lost = self.only_in(repo, &[&tracking, "--not", &full])?;
        Ok(ForcePlan {
            remote,
            theirs,
            lost,
        })
    }

    /// Force-pushes a local branch with a lease: git refuses when the remote branch is no longer `theirs`, the
    /// commit the user saw, or holds commits never integrated here (`--force-if-includes`). Returns the remote.
    pub fn force_push(&self, repo: &Path, branch: &str, theirs: &str) -> Result<String> {
        let plan = self.force_plan(repo, branch)?;
        ensure!(
            plan.theirs == theirs,
            "{}/{branch} moved since you looked: look again before forcing",
            plan.remote
        );
        let full = format!("refs/heads/{branch}");
        self.run_remote(
            repo,
            &[
                "push",
                &format!("--force-with-lease={full}"),
                "--force-if-includes",
                &plan.remote,
                &format!("{full}:{full}"),
            ],
        )?;
        Ok(plan.remote)
    }
}
