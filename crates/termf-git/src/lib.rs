//! Git in a project: the commit graph of every local and remote branch, the state of each branch and worktree,
//! and the everyday actions on branches. Everything goes through the git CLI and its machine formats
//! (`for-each-ref --format`, `log -z`, `status --porcelain=v2 -z`, `worktree list --porcelain`), never the
//! output meant for people.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// The most commits one graph reads.
pub const MAX_COMMITS: usize = 5000;
/// The most changes listed per worktree; the others are only counted.
const MAX_CHANGES: usize = 500;
/// The commits a branch alone holds, listed before deleting it; the others are only counted.
const MAX_UNIQUE: usize = 20;

pub struct Git {
    /// The GitHub CLI, asked for the pull request of each branch.
    pub gh: PathBuf,
    /// Variables for every git and gh run (tests: a throwaway HOME and config). A `PATH` here replaces the login
    /// shell's.
    pub env: Vec<(String, String)>,
}

impl Default for Git {
    fn default() -> Self {
        Self {
            gh: "gh".into(),
            env: Vec::new(),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tip {
    pub oid: String,
    pub author: String,
    /// The author date, in seconds since the Unix epoch.
    pub date: i64,
    pub subject: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    /// `main`, or `origin/main` for a remote-tracking branch.
    pub name: String,
    /// The remote of a remote-tracking branch; `None` for a local one.
    pub remote: Option<String>,
    /// Checked out in the repository's folder.
    pub current: bool,
    /// The upstream of a local branch, short (`origin/main`).
    pub upstream: Option<String>,
    /// The upstream is set but no longer exists, such as a branch deleted on the remote then pruned.
    pub gone: bool,
    /// Commits on the branch and not on its upstream, then the reverse.
    pub ahead: u32,
    pub behind: u32,
    pub tip: Tip,
    /// Whether its tip is in the default branch; `None` without a default branch.
    pub merged: Option<bool>,
    /// The worktree it is checked out in, when that is not the repository's folder.
    pub worktree: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub path: String,
    /// The path before a rename or a copy.
    pub from: Option<String>,
    /// git's two status letters: the index against HEAD, then the work tree against the index; `.` unchanged,
    /// `?` untracked.
    pub index: char,
    pub tree: char,
    pub conflict: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: String,
    /// The branch checked out, short; `None` when detached.
    pub branch: Option<String>,
    pub head: Option<String>,
    /// The repository's own folder, listed first.
    pub main: bool,
    pub bare: bool,
    pub locked: bool,
    /// Its folder is gone: `git worktree prune` would forget it.
    pub prunable: bool,
    pub changes: Vec<Change>,
    /// Changes beyond those listed.
    pub more: usize,
    /// Why its status could not be read.
    pub error: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// The branch checked out in the repository's folder; `None` when HEAD is detached.
    pub current: Option<String>,
    /// HEAD's commit; `None` before the first one.
    pub head: Option<String>,
    /// What "merged" is measured against: a remote's default branch (`origin/main`), else a local `main` or
    /// `master`.
    pub default_branch: Option<String>,
    pub remotes: Vec<String>,
    /// Local branches, then remote ones.
    pub branches: Vec<Branch>,
    pub worktrees: Vec<Worktree>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ref {
    /// Short: `main`, `origin/main`, `v1.2.0`.
    pub name: String,
    pub kind: RefKind,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub oid: String,
    pub parents: Vec<String>,
    pub author: String,
    /// The author date, in seconds since the Unix epoch.
    pub date: i64,
    pub subject: String,
    /// The branches and tags that point at it.
    pub refs: Vec<Ref>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    /// Newest first, a commit always before its parents.
    pub commits: Vec<Commit>,
    /// More commits exist beyond those read.
    pub more: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    pub number: u64,
    pub title: String,
    /// `OPEN`, `MERGED` or `CLOSED`, as GitHub says.
    pub state: String,
    pub draft: bool,
    pub url: String,
    /// The branch it comes from.
    pub branch: String,
}

/// What deleting a branch would lose: the commits no other branch or tag holds.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Unique {
    /// The newest ones.
    pub commits: Vec<Commit>,
    pub total: usize,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PullOutcome {
    Updated,
    UpToDate,
    /// Commits on both sides: a fast-forward cannot do it, nothing was changed.
    Diverged,
    NoUpstream,
    /// The upstream no longer exists on its remote.
    Gone,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pull {
    pub outcome: PullOutcome,
    /// Against the upstream, after fetching it.
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Push {
    pub remote: String,
    /// The branch had no upstream of its name, and now tracks the one pushed.
    pub upstream_set: bool,
}

/// One line of `for-each-ref` with `REF_FORMAT`.
#[derive(Debug, Default, Clone, PartialEq)]
struct RawRef {
    name: String,
    oid: String,
    /// What an annotated tag points at.
    peeled: String,
    head: bool,
    symref: String,
    upstream: String,
    upstream_remote: String,
    upstream_ref: String,
    worktree: String,
    author: String,
    date: i64,
    subject: String,
}

const REF_FORMAT: &str = "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(HEAD)%00%(symref)%00%(upstream)%00%(upstream:remotename)%00%(upstream:remoteref)%00%(worktreepath)%00%(authorname)%00%(authordate:unix)%00%(contents:subject)%00";
const REF_FIELDS: usize = 12;

fn parse_refs(out: &str) -> Vec<RawRef> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .as_chunks::<REF_FIELDS>()
        .0
        .iter()
        .map(|f| RawRef {
            // Each line ends with a newline after its last NUL.
            name: f[0].trim_start_matches('\n').to_owned(),
            oid: f[1].to_owned(),
            peeled: f[2].to_owned(),
            head: f[3] == "*",
            symref: f[4].to_owned(),
            upstream: f[5].to_owned(),
            upstream_remote: f[6].to_owned(),
            upstream_ref: f[7].to_owned(),
            worktree: f[8].to_owned(),
            author: f[9].to_owned(),
            date: f[10].parse().unwrap_or(0),
            subject: f[11].to_owned(),
        })
        .collect()
}

const LOG_FORMAT: &str = "--format=%H%x00%P%x00%an%x00%at%x00%s";
const LOG_FIELDS: usize = 5;

fn parse_log(out: &str) -> Vec<Commit> {
    let fields: Vec<&str> = out.split('\0').collect();
    fields
        .as_chunks::<LOG_FIELDS>()
        .0
        .iter()
        .map(|f| Commit {
            oid: f[0].trim_start_matches('\n').to_owned(),
            parents: f[1].split_whitespace().map(str::to_owned).collect(),
            author: f[2].to_owned(),
            date: f[3].parse().unwrap_or(0),
            subject: f[4].to_owned(),
            refs: Vec::new(),
        })
        .collect()
}

fn parse_worktrees(out: &str) -> Vec<Worktree> {
    let mut list: Vec<Worktree> = Vec::new();
    for line in out.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "worktree" {
            list.push(Worktree {
                path: value.to_owned(),
                main: list.is_empty(),
                ..Worktree::default()
            });
            continue;
        }
        let Some(wt) = list.last_mut() else { continue };
        match key {
            "HEAD" => wt.head = Some(value.to_owned()),
            "branch" => wt.branch = Some(short_ref(value)),
            "bare" => wt.bare = true,
            "locked" => wt.locked = true,
            "prunable" => wt.prunable = true,
            _ => {}
        }
    }
    list
}

/// Entries of `status --porcelain=v2 -z`: the first `MAX_CHANGES`, then how many more.
fn parse_status(out: &str) -> (Vec<Change>, usize) {
    let mut changes = Vec::new();
    let mut more = 0;
    let mut tokens = out.split('\0');
    while let Some(entry) = tokens.next() {
        let change = match entry.as_bytes().first() {
            Some(b'1') => entry.splitn(9, ' ').collect::<Vec<_>>(),
            Some(b'2') => entry.splitn(10, ' ').collect(),
            Some(b'u') => entry.splitn(11, ' ').collect(),
            Some(b'?') => vec!["?", "??", entry.get(2..).unwrap_or_default()],
            _ => continue,
        };
        let (Some(xy), Some(path)) = (change.get(1), change.last()) else {
            continue;
        };
        // A rename or a copy is followed by its source path.
        let from = (entry.starts_with('2'))
            .then(|| tokens.next().map(str::to_owned))
            .flatten();
        if changes.len() == MAX_CHANGES {
            more += 1;
            continue;
        }
        let mut letters = xy.chars();
        changes.push(Change {
            path: (*path).to_owned(),
            from,
            index: letters.next().unwrap_or('.'),
            tree: letters.next().unwrap_or('.'),
            conflict: entry.starts_with('u'),
        });
    }
    (changes, more)
}

/// `refs/heads/main` → `main`, `refs/remotes/origin/main` → `origin/main`, `refs/tags/v1` → `v1`.
fn short_ref(name: &str) -> String {
    ["refs/heads/", "refs/remotes/", "refs/tags/"]
        .iter()
        .find_map(|p| name.strip_prefix(p))
        .unwrap_or(name)
        .to_owned()
}

/// The remote a remote-tracking ref belongs to: the longest remote name that prefixes it, since a remote's name
/// may hold a slash.
fn remote_of<'a>(name: &str, remotes: &'a [String]) -> Option<&'a String> {
    let rest = name.strip_prefix("refs/remotes/")?;
    remotes
        .iter()
        .filter(|r| {
            rest.strip_prefix(r.as_str())
                .is_some_and(|b| b.starts_with('/'))
        })
        .max_by_key(|r| r.len())
}

/// The ref "merged" is measured against: a remote's HEAD (`origin` first), else a local `main` or `master`.
fn default_ref(refs: &[RawRef], remotes: &[String]) -> Option<String> {
    let exists = |name: &str| refs.iter().any(|r| r.name == name && r.symref.is_empty());
    let mut ordered: Vec<&String> = remotes.iter().collect();
    ordered.sort_by_key(|r| *r != "origin");
    ordered
        .iter()
        .find_map(|remote| {
            let head = format!("refs/remotes/{remote}/HEAD");
            refs.iter()
                .find(|r| r.name == head && exists(&r.symref))
                .map(|r| r.symref.clone())
        })
        .or_else(|| {
            ["refs/heads/main", "refs/heads/master"]
                .into_iter()
                .find(|n| exists(n))
                .map(str::to_owned)
        })
}

/// `command` in ssh's batch mode when it runs OpenSSH (`ssh`, from any folder); another program (plink…) may not
/// take `-o`, so it is left alone.
fn batch_ssh(command: &str) -> Option<String> {
    let program = command.split_whitespace().next()?;
    let name = program.rsplit(['/', '\\']).next()?.to_ascii_lowercase();
    matches!(name.as_str(), "ssh" | "ssh.exe").then(|| format!("{command} -o BatchMode=yes"))
}

/// Refuses what git would take for an option.
fn checked_name(name: &str) -> Result<&str> {
    ensure!(
        !name.is_empty() && !name.starts_with('-'),
        "invalid branch name: {name:?}"
    );
    Ok(name)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPr {
    number: u64,
    title: String,
    state: String,
    is_draft: bool,
    url: String,
    head_ref_name: String,
    is_cross_repository: bool,
}

impl Git {
    fn command(&self, program: &Path) -> Command {
        #[allow(unused_mut)]
        let mut cmd = Command::new(program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        // Launched from the Finder, TermF lacks the PATH of the shell profile, which credential helpers need too.
        if !self.env.iter().any(|(k, _)| k == "PATH")
            && let Some(path) = termf_core::shell::login_path()
        {
            cmd.env("PATH", path);
        }
        cmd.envs(self.env.iter().map(|(k, v)| (k, v)))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("GH_PROMPT_DISABLED", "1")
            .stdin(Stdio::null());
        cmd
    }

    fn output(&self, dir: &Path, args: &[&str]) -> Result<Output> {
        self.output_with(dir, args, None)
    }

    fn output_with(&self, dir: &Path, args: &[&str], ssh: Option<&str>) -> Result<Output> {
        let mut cmd = self.command(Path::new("git"));
        if let Some(ssh) = ssh {
            cmd.env("GIT_SSH_COMMAND", ssh);
        }
        // No optional locks: a refresh must never make an agent's concurrent git command fail on index.lock.
        cmd.arg("--no-optional-locks")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .context("cannot run git: is it installed?")
    }

    /// The output, or git's own error.
    fn run(&self, dir: &Path, args: &[&str]) -> Result<String> {
        self.run_with(dir, args, None)
    }

    /// A command that reaches a remote. `GIT_TERMINAL_PROMPT` does not stop ssh from asking for a passphrase or a
    /// host key on the terminal TermF was started from: in batch mode it fails with its own error instead.
    fn run_remote(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let var = |name: &str| {
            self.env
                .iter()
                .rev()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .or_else(|| std::env::var(name).ok())
        };
        // git's own order: GIT_SSH_COMMAND, then a GIT_SSH program (left as it is), then core.sshCommand.
        let ssh = match var("GIT_SSH_COMMAND") {
            Some(command) => Some(command),
            None if var("GIT_SSH").is_some() => None,
            None => Some(
                self.lookup(dir, &["config", "--get", "core.sshCommand"])?
                    .unwrap_or_else(|| "ssh".into()),
            ),
        };
        self.run_with(dir, args, ssh.and_then(|c| batch_ssh(&c)).as_deref())
    }

    fn run_with(&self, dir: &Path, args: &[&str], ssh: Option<&str>) -> Result<String> {
        let out = self.output_with(dir, args, ssh)?;
        ensure!(
            out.status.success(),
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// The output when git succeeds, `None` when it says no (`-q` lookups).
    fn lookup(&self, dir: &Path, args: &[&str]) -> Result<Option<String>> {
        let out = self.output(dir, args)?;
        Ok(out
            .status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned()))
    }

    fn refs(&self, repo: &Path, patterns: &[&str]) -> Result<Vec<RawRef>> {
        let mut args = vec!["for-each-ref", REF_FORMAT];
        args.extend(patterns);
        Ok(parse_refs(&self.run(repo, &args)?))
    }

    fn remotes(&self, repo: &Path) -> Result<Vec<String>> {
        Ok(self
            .run(repo, &["remote"])?
            .lines()
            .map(str::to_owned)
            .collect())
    }

    fn head(&self, repo: &Path) -> Result<Option<String>> {
        self.lookup(repo, &["rev-parse", "-q", "--verify", "HEAD^{commit}"])
    }

    fn exists(&self, repo: &Path, name: &str) -> Result<bool> {
        Ok(self
            .lookup(repo, &["show-ref", "--verify", "-q", name])?
            .is_some())
    }

    fn counts(&self, repo: &Path, left: &str, right: &str) -> Result<(u32, u32)> {
        let out = self.run(
            repo,
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("{left}...{right}"),
            ],
        )?;
        let mut n = out.split_whitespace().map(|n| n.parse().unwrap_or(0));
        Ok((n.next().unwrap_or(0), n.next().unwrap_or(0)))
    }

    /// The branches with their upstream, divergence, last commit and whether they are merged, then the worktrees
    /// with their changes.
    pub fn state(&self, repo: &Path) -> Result<State> {
        let remotes = self.remotes(repo)?;
        let refs = self.refs(repo, &["refs/heads", "refs/remotes"])?;
        let default = default_ref(&refs, &remotes);
        let merged: HashSet<String> = match &default {
            Some(target) => self
                .run(
                    repo,
                    &[
                        "for-each-ref",
                        "--format=%(refname)",
                        &format!("--merged={target}"),
                        "refs/heads",
                        "refs/remotes",
                    ],
                )?
                .lines()
                .map(str::to_owned)
                .collect(),
            None => HashSet::new(),
        };
        let names: HashSet<&str> = refs.iter().map(|r| r.name.as_str()).collect();
        let mut branches = Vec::new();
        for r in refs.iter().filter(|r| r.symref.is_empty()) {
            let local = r.name.starts_with("refs/heads/");
            let remote = (!local).then(|| remote_of(&r.name, &remotes).cloned());
            let upstream = (local && !r.upstream.is_empty()).then(|| r.upstream.clone());
            let gone = upstream.as_deref().is_some_and(|u| !names.contains(u));
            let (ahead, behind) = match &upstream {
                Some(u) if !gone => self.counts(repo, &r.name, u)?,
                _ => (0, 0),
            };
            branches.push(Branch {
                name: short_ref(&r.name),
                remote: remote.flatten(),
                current: r.head,
                upstream: upstream.as_deref().map(short_ref),
                gone,
                ahead,
                behind,
                tip: Tip {
                    oid: r.oid.clone(),
                    author: r.author.clone(),
                    date: r.date,
                    subject: r.subject.clone(),
                },
                merged: default.as_ref().map(|_| merged.contains(&r.name)),
                worktree: (!r.head && !r.worktree.is_empty()).then(|| r.worktree.clone()),
            });
        }
        let mut worktrees = parse_worktrees(&self.run(repo, &["worktree", "list", "--porcelain"])?);
        for wt in worktrees.iter_mut().filter(|w| !w.bare && !w.prunable) {
            match self.run(
                Path::new(&wt.path),
                &["status", "--porcelain=v2", "-z", "--untracked-files=normal"],
            ) {
                Ok(out) => (wt.changes, wt.more) = parse_status(&out),
                Err(e) => wt.error = Some(format!("{e:#}")),
            }
        }
        Ok(State {
            current: self.lookup(repo, &["symbolic-ref", "-q", "--short", "HEAD"])?,
            head: self.head(repo)?,
            default_branch: default.as_deref().map(short_ref),
            remotes,
            branches,
            worktrees,
        })
    }

    /// The newest `limit` commits of every local and remote branch, tag and HEAD, children before parents.
    pub fn graph(&self, repo: &Path, limit: usize) -> Result<Graph> {
        let limit = limit.clamp(1, MAX_COMMITS);
        let refs = self.refs(repo, &["refs/heads", "refs/remotes", "refs/tags"])?;
        let count = format!("-n{}", limit + 1);
        let mut args = vec![
            "log",
            "--date-order",
            "-z",
            LOG_FORMAT,
            &count,
            "--branches",
            "--remotes",
            "--tags",
        ];
        // Before the first commit, HEAD names nothing and git would refuse it.
        let head = self.head(repo)?;
        if head.is_some() {
            args.push("HEAD");
        } else if refs.is_empty() {
            return Ok(Graph {
                commits: Vec::new(),
                more: false,
            });
        }
        args.push("--");
        let mut commits = parse_log(&self.run(repo, &args)?);
        let more = commits.len() > limit;
        commits.truncate(limit);
        let mut at: HashMap<&str, Vec<Ref>> = HashMap::new();
        for r in refs.iter().filter(|r| r.symref.is_empty()) {
            let (kind, oid) = match r.name.split('/').nth(1) {
                Some("heads") => (RefKind::Local, &r.oid),
                Some("remotes") => (RefKind::Remote, &r.oid),
                Some("tags") if !r.peeled.is_empty() => (RefKind::Tag, &r.peeled),
                _ => (RefKind::Tag, &r.oid),
            };
            at.entry(oid).or_default().push(Ref {
                name: short_ref(&r.name),
                kind,
            });
        }
        for commit in &mut commits {
            if let Some(refs) = at.remove(commit.oid.as_str()) {
                commit.refs = refs;
            }
        }
        Ok(Graph { commits, more })
    }

    /// The pull requests of the repository's branches, newest first, from `gh`; none when it is missing, not
    /// logged in, or the repository is not on GitHub. Pull requests from forks are left out: their branch name
    /// says nothing about this repository's branches.
    pub fn prs(&self, repo: &Path) -> Vec<Pr> {
        let out = self
            .command(&self.gh)
            .current_dir(repo)
            .args(["pr", "list", "--state", "all", "--limit", "100", "--json"])
            .arg("number,title,state,isDraft,url,headRefName,isCrossRepository")
            .output();
        let out = match out {
            Ok(out) if out.status.success() => out,
            _ => return Vec::new(),
        };
        serde_json::from_slice::<Vec<GhPr>>(&out.stdout)
            .unwrap_or_default()
            .into_iter()
            .filter(|p| !p.is_cross_repository)
            .map(|p| Pr {
                number: p.number,
                title: p.title,
                state: p.state,
                draft: p.is_draft,
                url: p.url,
                branch: p.head_ref_name,
            })
            .collect()
    }

    /// The commits only the local branch `name` holds: deleting it would lose them.
    pub fn unique(&self, repo: &Path, name: &str) -> Result<Unique> {
        let full = format!("refs/heads/{}", checked_name(name)?);
        // `--exclude` applies to the `--branches` right after it, whose names start after `refs/heads/`.
        let exclude = format!("--exclude={name}");
        let others = [
            full.as_str(),
            "--not",
            &exclude,
            "--branches",
            "--remotes",
            "--tags",
        ];
        let total = self
            .run(repo, &[&["rev-list", "--count"][..], &others].concat())?
            .trim()
            .parse()
            .unwrap_or(0);
        let max = format!("-n{MAX_UNIQUE}");
        let commits = parse_log(&self.run(
            repo,
            &[&["log", "-z", LOG_FORMAT, &max][..], &others, &["--"]].concat(),
        )?);
        Ok(Unique { commits, total })
    }

    /// Switches the repository's folder to a local branch, or to a new local branch that tracks a remote one
    /// (`origin/feature` → `feature`). git refuses when changes in the folder would be lost.
    pub fn switch(&self, repo: &Path, branch: &str) -> Result<()> {
        let branch = checked_name(branch)?;
        if self.exists(repo, &format!("refs/heads/{branch}"))? {
            self.run(repo, &["switch", branch])?;
        } else if self.exists(repo, &format!("refs/remotes/{branch}"))? {
            self.run(repo, &["switch", "--track", branch])?;
        } else {
            bail!("no branch {branch}");
        }
        Ok(())
    }

    /// Fetches every remote, forgetting the branches deleted there.
    pub fn fetch(&self, repo: &Path) -> Result<()> {
        self.run_remote(repo, &["fetch", "--all", "--prune"])?;
        Ok(())
    }

    /// Fetches the current branch's upstream (forgotten when deleted on its remote), then moves the branch forward
    /// to it when it has nothing of its own; otherwise says why and changes nothing.
    pub fn pull(&self, repo: &Path) -> Result<Pull> {
        let branch = self
            .lookup(repo, &["symbolic-ref", "-q", "--short", "HEAD"])?
            .context("HEAD is detached: switch to a branch to pull")?;
        let answer = |outcome, (ahead, behind)| Pull {
            outcome,
            ahead,
            behind,
        };
        let Some(remote) = self.lookup(
            repo,
            &["config", "--get", &format!("branch.{branch}.remote")],
        )?
        else {
            return Ok(answer(PullOutcome::NoUpstream, (0, 0)));
        };
        if remote != "." {
            self.run_remote(repo, &["fetch", "--prune", &remote])?;
        }
        if self
            .lookup(repo, &["rev-parse", "-q", "--verify", "@{upstream}"])?
            .is_none()
        {
            return Ok(answer(PullOutcome::Gone, (0, 0)));
        }
        let counts = self.counts(repo, "HEAD", "@{upstream}")?;
        let outcome = match counts {
            (_, 0) => PullOutcome::UpToDate,
            (0, _) => {
                self.run(repo, &["merge", "--ff-only", "@{upstream}"])?;
                PullOutcome::Updated
            }
            _ => PullOutcome::Diverged,
        };
        Ok(answer(outcome, counts))
    }

    /// Pushes a local branch under its own name to its push remote, chosen as git does: the branch's `pushRemote`,
    /// `remote.pushDefault`, its upstream's remote, then `origin` or the only remote. The upstream is kept when it
    /// has that name or lives on another remote (a fork's triangle); otherwise the branch tracks what was pushed.
    /// Never forced: git refuses a push that would drop commits on the remote.
    pub fn push(&self, repo: &Path, branch: &str) -> Result<Push> {
        let full = format!("refs/heads/{}", checked_name(branch)?);
        let r = self
            .refs(repo, &[&full])?
            .into_iter()
            .find(|r| r.name == full)
            .with_context(|| format!("no local branch {branch}"))?;
        let upstream =
            (!r.upstream.is_empty() && r.upstream_remote != ".").then_some(&r.upstream_remote);
        let remote = self.push_remote(repo, branch, upstream)?;
        // Never under the upstream's other name: a branch started from `origin/main` must not push onto main.
        let keep = upstream.is_some_and(|u| *u != remote || r.upstream_ref == full);
        let refspec = format!("{full}:{full}");
        let mut args = vec!["push", remote.as_str(), &refspec];
        if !keep {
            args.insert(1, "--set-upstream");
        }
        self.run_remote(repo, &args)?;
        Ok(Push {
            remote,
            upstream_set: !keep,
        })
    }

    fn push_remote(&self, repo: &Path, branch: &str, upstream: Option<&String>) -> Result<String> {
        for key in [
            format!("branch.{branch}.pushRemote"),
            "remote.pushDefault".into(),
        ] {
            if let Some(remote) = self.lookup(repo, &["config", "--get", &key])? {
                return Ok(remote);
            }
        }
        if let Some(upstream) = upstream {
            return Ok(upstream.clone());
        }
        let remotes = self.remotes(repo)?;
        if remotes.iter().any(|r| r == "origin") {
            return Ok("origin".into());
        }
        match remotes.as_slice() {
            [only] => Ok(only.clone()),
            [] => bail!("the repository has no remote to push to"),
            _ => bail!("several remotes and none is origin: set remote.pushDefault to choose one"),
        }
    }

    /// A branch from a branch or a commit, not tracking anything until its first push; switched to when asked.
    pub fn create(&self, repo: &Path, name: &str, from: &str, checkout: bool) -> Result<()> {
        let (name, from) = (checked_name(name)?, checked_name(from)?);
        self.run(repo, &["check-ref-format", "--branch", name])?;
        if checkout {
            self.run(repo, &["switch", "--no-track", "-c", name, from])?;
        } else {
            self.run(repo, &["branch", "--no-track", name, from])?;
        }
        Ok(())
    }

    pub fn rename(&self, repo: &Path, from: &str, to: &str) -> Result<()> {
        let (from, to) = (checked_name(from)?, checked_name(to)?);
        self.run(repo, &["branch", "-m", from, to])?;
        Ok(())
    }

    /// Deletes a local branch whose tip is still `tip`, the commit the user saw when confirming: what it alone
    /// held is lost.
    pub fn delete(&self, repo: &Path, name: &str, tip: &str) -> Result<()> {
        let full = format!("refs/heads/{}", checked_name(name)?);
        let now = self
            .lookup(repo, &["rev-parse", "-q", "--verify", &full])?
            .with_context(|| format!("no local branch {name}"))?;
        ensure!(
            now == tip,
            "{name} has new commits since you looked: look again before deleting it"
        );
        self.run(repo, &["branch", "-D", name])?;
        Ok(())
    }

    /// Deletes `branch` on `remote` (and its remote-tracking branch here), never the remote's default branch: the
    /// target of `<remote>/HEAD`, or `main` and `master` when it is unknown.
    pub fn delete_remote(&self, repo: &Path, remote: &str, branch: &str) -> Result<()> {
        let (remote, branch) = (checked_name(remote)?, checked_name(branch)?);
        let prefix = format!("refs/remotes/{remote}/");
        let head = self.lookup(repo, &["symbolic-ref", "-q", &format!("{prefix}HEAD")])?;
        let default = match head.as_deref().and_then(|h| h.strip_prefix(&prefix)) {
            Some(default) => default == branch,
            None => matches!(branch, "main" | "master"),
        };
        ensure!(
            !default,
            "{branch} is the default branch of {remote}: TermF does not delete it"
        );
        self.run_remote(
            repo,
            &["push", remote, "--delete", &format!("refs/heads/{branch}")],
        )?;
        Ok(())
    }
}
