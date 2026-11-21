//! A run's git side: its worktree and branch, its delivery as a pull request, and its cleanup. Agents never
//! push: only `deliver`, which the user triggers, does.

use std::fs;
use std::path::Path;
use std::process::{Output, Stdio};

use anyhow::{Context, Result, bail, ensure};

use crate::github;
use crate::store::{Event, Run, RunEvent, RunStatus, Store};
use crate::{Ticket, Tickets};

/// A run ready for its session.
pub struct Launch {
    pub run: Run,
    /// The session's first message, with the ticket framed in it.
    pub prompt: String,
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

    fn run(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let out = self.output(dir, args)?;
        ensure!(
            out.status.success(),
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
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

/// The `owner/name` of each remote of a repository, lowercased.
pub(crate) fn remote_slugs(repo: &str, env: &[(String, String)]) -> Vec<String> {
    let urls = Git(env).run(
        Path::new(repo),
        &["config", "--get-regexp", r"^remote\..*\.url$"],
    );
    let urls = urls.unwrap_or_default();
    let mut slugs: Vec<String> = Vec::new();
    for slug in urls
        .lines()
        .filter_map(|line| github::remote_slug(line.split_once(' ')?.1))
    {
        if !slugs.contains(&slug) {
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

impl Tickets {
    /// Resolves the ticket, reads it, and prepares its run: a worktree on a new branch `termf/<ticket>-<slug>`
    /// from the repository's base branch. `repo` (a name or a path) picks the repository when the project has
    /// several.
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
        let prompt = crate::prompt(&ticket, &branch, &base);
        store.record(run.id, &Event::WorktreeCreated { path, branch, base })?;
        Ok(Launch { run, prompt })
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

    /// The gate of a run, which only the user opens: commits what is left, pushes the branch and opens the
    /// pull request, linked to the ticket. Delivering again pushes what changed since.
    pub fn deliver(&self, id: i64) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        let mut run = store.run(id)?;
        ensure!(
            matches!(run.status, RunStatus::Running | RunStatus::Delivered),
            "run {id} is {}: there is nothing to deliver",
            run.status.as_str()
        );
        let (Some(worktree), Some(branch), Some(base)) =
            (run.worktree.clone(), run.branch.clone(), run.base.clone())
        else {
            bail!("run {id} has no worktree");
        };
        let (git, dir) = (Git(&self.env), Path::new(&worktree));
        git.run(dir, &["add", "-A"])?;
        if !git.ok(dir, &["diff", "--cached", "--quiet"]) {
            let message = format!("{}\n\n{}", run.title, run.ticket.url);
            git.run(dir, &["commit", "-q", "-m", &message])?;
            let commit = git.run(dir, &["rev-parse", "HEAD"])?;
            store.record(id, &Event::Committed { commit, message })?;
        }
        ensure!(
            git.count(dir, &format!("{base}..HEAD"))? > 0,
            "nothing to deliver: no change since {base}"
        );
        git.run(dir, &["push", "-q", "-u", "origin", &branch])?;
        let commit = git.run(dir, &["rev-parse", "HEAD"])?;
        store.record(
            id,
            &Event::Pushed {
                branch: branch.clone(),
                commit,
            },
        )?;
        run.status = RunStatus::Delivered;
        if run.pr_url.is_none() {
            let origin = git.run(dir, &["remote", "get-url", "origin"])?;
            let repo = github::remote_slug(&origin)
                .with_context(|| format!("origin ({origin}) is not a GitHub repository"))?;
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
            store.update(&mut run)?;
            store.record(id, &Event::PrOpened { url: url.clone() })?;
            if self.comments_on(&run.project) {
                self.provider(&run.ticket.provider)?
                    .link_pr(&run.ticket, &url)
                    .context("the pull request is open, but commenting on the ticket failed")?;
                store.record(id, &Event::TicketCommented { url })?;
            }
        }
        store.update(&mut run)?;
        Ok(run)
    }

    /// Removes the run's worktree. Refused while it holds work that is not pushed, unless `force`: then the
    /// branch stays, so the commits it drops from the worktree can still be found.
    pub fn close(&self, id: i64, force: bool) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        let mut run = store.run(id)?;
        ensure!(
            run.status != RunStatus::Closed,
            "run {id} is already closed"
        );
        let mut dropped = Vec::new();
        if let (Some(worktree), Some(branch), Some(base)) = (&run.worktree, &run.branch, &run.base)
        {
            let (git, repo, worktree) = (Git(&self.env), Path::new(&run.repo), Path::new(worktree));
            let pushed = store
                .events(id)?
                .into_iter()
                .rev()
                .find_map(|e| match e.event {
                    Event::Pushed { commit, .. } => Some(commit),
                    _ => None,
                });
            dropped = unpushed(
                &git,
                repo,
                worktree,
                branch,
                pushed.as_deref().unwrap_or(base),
            )?;
            ensure!(
                force || dropped.is_empty(),
                "run {id} holds work that is not pushed ({}): deliver it, or close it by force",
                dropped.join(", ")
            );
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
