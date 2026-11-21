//! A run's git side: its worktree and branch, its delivery as a pull request, and its cleanup. Agents never
//! push: only `deliver`, which the user confirms after seeing what it sends, does.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use termf_ssh::Ssh;

use crate::github;
use crate::store::{Event, Run, RunEvent, RunStatus, Store};
use crate::{Ticket, Tickets};

/// A run ready for its session.
pub struct Launch {
    pub run: Run,
    /// The file that holds the session's task: the rules, then the ticket framed as data.
    pub brief: PathBuf,
}

/// What delivering a run would send, for the user to check first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryPreview {
    /// The worktree's changes, committed by the delivery.
    pub files: Vec<FileChange>,
    /// Commits already made in the worktree and not pushed yet.
    pub commits: Vec<CommitSummary>,
    /// Given back to `deliver`, which refuses if the worktree changed since.
    pub fingerprint: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub change: Change,
    /// The former path of a renamed file.
    pub from: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    pub commit: String,
    pub subject: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub run: Run,
    /// Something that failed once the pull request was open, such as commenting on the ticket.
    pub warning: Option<String>,
}

/// A delivery's preview, and the index that holds exactly what it would commit.
struct Snapshot {
    preview: DeliveryPreview,
    index: PathBuf,
}

struct Git<'a>(&'a [(String, String)]);

impl Git<'_> {
    fn output(&self, dir: &Path, args: &[&str]) -> Result<Output> {
        crate::command("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .envs(self.0.iter().map(|(k, v)| (k, v)))
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .context("cannot run git: is it installed?")
    }

    /// The output as is, for `-z` listings whose names may start or end with spaces.
    fn raw(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let out = self.output(dir, args)?;
        ensure!(
            out.status.success(),
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn run(&self, dir: &Path, args: &[&str]) -> Result<String> {
        Ok(self.raw(dir, args)?.trim().to_owned())
    }

    fn ok(&self, dir: &Path, args: &[&str]) -> bool {
        self.output(dir, args).is_ok_and(|o| o.status.success())
    }

    fn has(&self, dir: &Path, reference: &str) -> bool {
        let commit = format!("{reference}^{{commit}}");
        self.ok(dir, &["rev-parse", "--verify", "-q", &commit])
    }

    fn count(&self, dir: &Path, range: &str) -> Result<u32> {
        Ok(self.run(dir, &["rev-list", "--count", range])?.parse()?)
    }
}

/// The `owner/name` of each GitHub remote of a repository, lowercased.
pub(crate) fn remote_slugs(repo: &str, env: &[(String, String)], ssh: &Ssh) -> Vec<String> {
    let urls = Git(env).run(
        Path::new(repo),
        &["config", "--get-regexp", r"^remote\..*\.url$"],
    );
    let urls = urls.unwrap_or_default();
    let mut slugs: Vec<String> = Vec::new();
    for line in urls.lines() {
        let Some((_, url)) = line.split_once(' ') else {
            continue;
        };
        if let Ok(slug) = github::remote_slug(url, ssh)
            && !slugs.contains(&slug)
        {
            slugs.push(slug);
        }
    }
    slugs
}

/// `fix-the-login-button`: ASCII only, at most 40 characters, so it suits any branch name.
pub(crate) fn slug(text: &str) -> String {
    let words: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut slug = words.join("-").to_ascii_lowercase();
    slug.truncate(40);
    slug.trim_end_matches('-').to_owned()
}

/// The branch runs start from: the remote's default branch, else the usual names.
fn base_ref(git: &Git, repo: &Path) -> Result<String> {
    if let Ok(head) = git.run(
        repo,
        &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"],
    ) {
        return Ok(head);
    }
    ["origin/main", "origin/master", "main", "master"]
        .into_iter()
        .find(|r| git.has(repo, r))
        .map(str::to_owned)
        .with_context(|| format!("cannot tell the base branch of {}", repo.display()))
}

/// What closing would lose: changes not committed in the worktree, commits not pushed on the branch.
fn unpushed(
    git: &Git,
    repo: &Path,
    worktree: &Path,
    branch: &str,
    since: &str,
) -> Result<Vec<String>> {
    let mut lost = Vec::new();
    if worktree.exists() && !git.run(worktree, &["status", "--porcelain"])?.is_empty() {
        lost.push("changes not committed".to_owned());
    }
    let branch = format!("refs/heads/{branch}");
    if git.has(repo, &branch) {
        match git.count(repo, &format!("{since}..{branch}"))? {
            0 => {}
            1 => lost.push("1 commit not pushed".to_owned()),
            n => lost.push(format!("{n} commits not pushed")),
        }
    }
    Ok(lost)
}

/// The commit last pushed by a delivery of the run.
fn last_push(store: &Store, id: i64) -> Result<Option<String>> {
    let mut events = store.events(id)?.into_iter().rev();
    Ok(events.find_map(|e| match e.event {
        Event::Pushed { commit, .. } => Some(commit),
        _ => None,
    }))
}

/// `git diff --name-status -z` lines.
fn file_changes(listing: &str) -> Vec<FileChange> {
    let mut fields = listing.split('\0').filter(|f| !f.is_empty());
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let (change, two_paths) = match status.chars().next() {
            Some('A') => (Change::Added, false),
            Some('C') => (Change::Added, true),
            Some('D') => (Change::Deleted, false),
            Some('R') => (Change::Renamed, true),
            _ => (Change::Modified, false),
        };
        let from = two_paths.then(|| fields.next()).flatten();
        let Some(path) = fields.next() else { break };
        files.push(FileChange {
            path: path.to_owned(),
            change,
            from: from
                .filter(|_| change == Change::Renamed)
                .map(str::to_owned),
        });
    }
    files
}

impl Tickets {
    /// Resolves the ticket, reads it, and prepares its run: a worktree on a new branch `termf/<ticket>-<slug>`
    /// from the repository's base branch, and the brief of its session. `repo` (a name or a path) picks the
    /// repository when the project has several.
    pub fn start(&self, input: &str, project: Option<&str>, repo: Option<&str>) -> Result<Launch> {
        let resolution = self.resolve(input, project)?;
        let chosen = match repo.or(resolution.repo.as_deref()) {
            Some(wanted) => resolution
                .repos
                .iter()
                .find(|r| {
                    r.name.eq_ignore_ascii_case(wanted) || Path::new(&r.path) == Path::new(wanted)
                })
                .with_context(|| {
                    format!("no repository {wanted} in project {}", resolution.project)
                })?,
            None => {
                let names: Vec<&str> = resolution.repos.iter().map(|r| r.name.as_str()).collect();
                bail!(
                    "project {} has several repositories: choose one of {}",
                    resolution.project,
                    names.join(", ")
                )
            }
        };
        let ticket = self
            .provider(&resolution.ticket.provider)?
            .fetch(&resolution.ticket)?;
        let store = Store::open(&self.data_dir)?;
        let mut run = store.create(&resolution.project, &ticket, &chosen.path)?;
        let (path, branch, base) = match self.worktree(&run, &chosen.name, &ticket) {
            Ok(created) => created,
            Err(e) => {
                store.fail(run.id, &format!("{e:#}"))?;
                return Err(e);
            }
        };
        run.worktree = Some(path.clone());
        run.branch = Some(branch.clone());
        run.base = Some(base.clone());
        store.update(&mut run)?;
        // Its own folder: the session may read it, nothing else of the run.
        let brief = store.run_dir(run.id).join("brief").join("prompt.md");
        fs::create_dir_all(brief.parent().context("no parent folder")?)?;
        fs::write(&brief, crate::prompt(&ticket, &branch, &base))?;
        store.record(run.id, &Event::WorktreeCreated { path, branch, base })?;
        Ok(Launch { run, brief })
    }

    /// Creates `<data>/worktrees/<project>/<run>/<repo>` and returns its path, its branch and its base.
    fn worktree(&self, run: &Run, name: &str, ticket: &Ticket) -> Result<(String, String, String)> {
        let git = Git(&self.env);
        let repo = Path::new(&run.repo);
        // Offline, the run starts from what was last fetched.
        let _ = git.output(repo, &["fetch", "-q", "origin"]);
        let base = base_ref(&git, repo)?;
        let key = &ticket.reference.key;
        let id = key.rsplit('#').next().unwrap_or(key);
        let stem = match slug(&ticket.title) {
            s if s.is_empty() => format!("termf/{id}"),
            s => format!("termf/{id}-{s}"),
        };
        let branch = (1..)
            .map(|n| match n {
                1 => stem.clone(),
                n => format!("{stem}-{n}"),
            })
            // A branch pushed by an earlier run of the ticket counts too, even once its run is closed.
            .find(|b| {
                !git.has(repo, &format!("refs/heads/{b}"))
                    && !git.has(repo, &format!("refs/remotes/origin/{b}"))
            })
            .unwrap();
        let folder = match slug(name) {
            s if s.is_empty() => "repo".to_owned(),
            s => s,
        };
        let path = self
            .data_dir
            .join("worktrees")
            .join(&run.project)
            .join(run.id.to_string())
            .join(folder);
        fs::create_dir_all(path.parent().context("no parent folder")?)?;
        let path = path.to_string_lossy().into_owned();
        git.run(
            repo,
            &[
                "worktree",
                "add",
                "-q",
                "--no-track",
                "-b",
                &branch,
                &path,
                &base,
            ],
        )?;
        Ok((path, branch, base))
    }

    pub fn run(&self, id: i64) -> Result<Run> {
        Store::open(&self.data_dir)?.run(id)
    }

    /// Newest first.
    pub fn runs(&self) -> Result<Vec<Run>> {
        Store::open(&self.data_dir)?.runs()
    }

    pub fn events(&self, id: i64) -> Result<Vec<RunEvent>> {
        let store = Store::open(&self.data_dir)?;
        store.run(id)?;
        store.events(id)
    }

    pub fn session_opened(&self, id: i64, session: u32) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        store.record(id, &Event::SessionOpened { session })?;
        store.run(id)
    }

    pub fn fail(&self, id: i64, error: &str) -> Result<Run> {
        Store::open(&self.data_dir)?.fail(id, error)
    }

    /// What `deliver` would commit and push, and the fingerprint that confirms it.
    pub fn deliver_preview(&self, id: i64) -> Result<DeliveryPreview> {
        let store = Store::open(&self.data_dir)?;
        let snapshot = self.snapshot(&store, &store.run(id)?)?;
        let _ = fs::remove_file(&snapshot.index);
        Ok(snapshot.preview)
    }

    /// Takes the worktree as `git add -A` would, in an index of its own: the real one is left alone until the
    /// user confirms.
    fn snapshot(&self, store: &Store, run: &Run) -> Result<Snapshot> {
        let id = run.id;
        ensure!(
            matches!(run.status, RunStatus::Running | RunStatus::Delivered),
            "run {id} is {}: there is nothing to deliver",
            run.status.as_str()
        );
        let (Some(worktree), Some(branch), Some(base)) = (&run.worktree, &run.branch, &run.base)
        else {
            bail!("run {id} has no worktree");
        };
        let (git, dir) = (Git(&self.env), Path::new(worktree));
        let head = git
            .run(dir, &["symbolic-ref", "-q", "HEAD"])
            .unwrap_or_default();
        if head != format!("refs/heads/{branch}") {
            let now = match head.strip_prefix("refs/heads/") {
                Some(other) => format!("on {other}"),
                None => "detached".to_owned(),
            };
            bail!(
                "the worktree of run {id} is {now}, not on its branch {branch}: `git switch {branch}` there first"
            );
        }
        let index = store.run_dir(id).join("delivery.index");
        fs::create_dir_all(index.parent().context("no parent folder")?)?;
        let real = git.run(
            dir,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        )?;
        fs::copy(&real, &index).with_context(|| format!("cannot copy {real}"))?;
        let mut env = self.env.clone();
        env.push(("GIT_INDEX_FILE".into(), index.to_string_lossy().into()));
        let staged = Git(&env);
        staged.run(dir, &["add", "-A"])?;
        let tree = staged.run(dir, &["write-tree"])?;
        let files = file_changes(&staged.raw(
            dir,
            &["diff", "--cached", "--name-status", "-z", "-M", "HEAD"],
        )?);
        let since = last_push(store, id)?.unwrap_or_else(|| base.clone());
        let log = git.run(
            dir,
            &["log", "--format=%H%x1f%s", &format!("{since}..HEAD")],
        )?;
        let commits = log
            .lines()
            .filter_map(|line| line.split_once('\x1f'))
            .map(|(commit, subject)| CommitSummary {
                commit: commit.into(),
                subject: subject.into(),
            })
            .collect();
        let head = git.run(dir, &["rev-parse", "HEAD"])?;
        Ok(Snapshot {
            preview: DeliveryPreview {
                files,
                commits,
                fingerprint: format!("{head}:{tree}"),
            },
            index,
        })
    }

    /// The gate of a run, which only the user opens, with the fingerprint of the preview they saw: commits
    /// exactly what it listed, pushes the branch and opens the pull request, linked to the ticket. Refused if
    /// the worktree changed since. Delivering again pushes what changed and retries what failed.
    pub fn deliver(&self, id: i64, confirm: &str) -> Result<Delivery> {
        let store = Store::open(&self.data_dir)?;
        let mut run = store.run(id)?;
        let snapshot = self.snapshot(&store, &run)?;
        let delivered = self.send(&store, &mut run, &snapshot, confirm);
        let _ = fs::remove_file(&snapshot.index);
        delivered
    }

    fn send(
        &self,
        store: &Store,
        run: &mut Run,
        snapshot: &Snapshot,
        confirm: &str,
    ) -> Result<Delivery> {
        let id = run.id;
        ensure!(
            snapshot.preview.fingerprint == confirm,
            "the worktree of run {id} changed since its preview: preview the delivery again"
        );
        let (Some(worktree), Some(branch), Some(base)) =
            (run.worktree.clone(), run.branch.clone(), run.base.clone())
        else {
            bail!("run {id} has no worktree");
        };
        let (git, dir) = (Git(&self.env), Path::new(&worktree));
        if !snapshot.preview.files.is_empty() {
            let message = format!("{}\n\n{}", run.title, run.ticket.url);
            let mut env = self.env.clone();
            env.push((
                "GIT_INDEX_FILE".into(),
                snapshot.index.to_string_lossy().into(),
            ));
            Git(&env).run(dir, &["commit", "-q", "-m", &message])?;
            // The real index catches up with the commit; the files themselves are untouched.
            git.run(dir, &["reset", "-q"])?;
            let commit = git.run(dir, &["rev-parse", "HEAD"])?;
            store.record(id, &Event::Committed { commit, message })?;
        }
        ensure!(
            git.count(dir, &format!("{base}..HEAD"))? > 0,
            "nothing to deliver: no change since {base}"
        );
        // The commit itself is pushed, so the one recorded is the one sent.
        let commit = git.run(dir, &["rev-parse", &format!("refs/heads/{branch}")])?;
        let refspec = format!("{commit}:refs/heads/{branch}");
        git.run(dir, &["push", "-q", "origin", &refspec])?;
        store.record(
            id,
            &Event::Pushed {
                branch: branch.clone(),
                commit,
            },
        )?;
        run.status = RunStatus::Delivered;
        if run.pr_url.is_none() {
            let origin = git.run(dir, &["config", "--get", "remote.origin.url"])?;
            let repo = github::remote_slug(&origin, &termf_ssh::Ssh::new(self.home.clone()))
                .context("the pull request goes to origin")?;
            let body = match github::split(&run.ticket.key) {
                Ok((slug, number))
                    if run.ticket.provider == "github" && slug.eq_ignore_ascii_case(&repo) =>
                {
                    format!("Closes #{number}")
                }
                _ => format!("Ticket: {}", run.ticket.url),
            };
            let target = base.strip_prefix("origin/").unwrap_or(&base);
            let url = self
                .github()
                .create_pr(dir, &repo, target, &branch, &run.title, &body)?;
            run.pr_url = Some(url.clone());
            store.update(run)?;
            store.record(id, &Event::PrOpened { url })?;
        }
        let mut warning = None;
        let commented = store
            .events(id)?
            .iter()
            .any(|e| matches!(e.event, Event::TicketCommented { .. }));
        if let Some(url) = run.pr_url.clone()
            && !commented
            && self.comments_on(&run.project)
        {
            match self
                .provider(&run.ticket.provider)?
                .link_pr(&run.ticket, &url)
            {
                Ok(()) => {
                    store.record(id, &Event::TicketCommented { url })?;
                }
                Err(e) => {
                    warning = Some(format!(
                        "the pull request is open, but commenting on the ticket failed (tried again at the next delivery): {e:#}"
                    ))
                }
            }
        }
        store.update(run)?;
        Ok(Delivery {
            run: run.clone(),
            warning,
        })
    }

    /// Removes the run's worktree. Refused while it holds work that is not pushed, unless `force`: then the
    /// branch stays, so the commits it drops from the worktree can still be found. `stop_session` runs once
    /// the checks pass, before anything is removed, so no agent is left working in a deleted folder.
    pub fn close(&self, id: i64, force: bool, stop_session: impl FnOnce()) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        let mut run = store.run(id)?;
        ensure!(
            run.status != RunStatus::Closed,
            "run {id} is already closed"
        );
        let git = Git(&self.env);
        let repo = Path::new(&run.repo);
        let parts = (run.worktree.clone(), run.branch.clone(), run.base.clone());
        let mut dropped = Vec::new();
        if let (Some(worktree), Some(branch), Some(base)) = &parts {
            let pushed = last_push(&store, id)?;
            let since = pushed.as_deref().unwrap_or(base);
            dropped = unpushed(&git, repo, Path::new(worktree), branch, since)?;
            ensure!(
                force || dropped.is_empty(),
                "run {id} holds work that is not pushed ({}): deliver it, or close it by force",
                dropped.join(", ")
            );
        }
        stop_session();
        if let (Some(worktree), Some(branch), Some(_)) = &parts {
            let worktree = Path::new(worktree);
            if worktree.exists() {
                let path = worktree.to_string_lossy();
                let mut args = vec!["worktree", "remove"];
                if force {
                    args.push("--force");
                }
                args.push(&path);
                git.run(repo, &args)?;
            } else {
                git.run(repo, &["worktree", "prune"])?;
            }
            if dropped.is_empty() && git.has(repo, &format!("refs/heads/{branch}")) {
                git.run(repo, &["branch", "-D", branch])?;
            }
            // The run's folder under worktrees/, now empty unless something else was put there.
            if let Some(folder) = worktree.parent() {
                let _ = fs::remove_dir(folder);
            }
        }
        run.status = RunStatus::Closed;
        store.update(&mut run)?;
        store.record(id, &Event::Closed { dropped })?;
        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_status_listings() {
        let files =
            file_changes("M\0src/a b.rs\0A\0 new\0D\0old.txt\0R087\0x.md\0y.md\0C100\0p\0q\0");
        let change = |path: &str, change, from: Option<&str>| FileChange {
            path: path.into(),
            change,
            from: from.map(Into::into),
        };
        assert_eq!(
            files,
            [
                change("src/a b.rs", Change::Modified, None),
                change(" new", Change::Added, None),
                change("old.txt", Change::Deleted, None),
                change("y.md", Change::Renamed, Some("x.md")),
                change("q", Change::Added, None),
            ]
        );
    }
}
