//! Cleaning a repository up: which local branches and worktrees can go, why, and what each would lose.

use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{Branch, Git, Tip, Worktree, checked_name, parse_status, parse_worktrees};

/// A branch whose last commit is older than this is old.
pub const STALE_DAYS: i64 = 90;

/// A TermF run with its worktree or branch in this repository, as the tickets store knows it.
#[derive(Deserialize, Serialize, Debug, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunRef {
    pub id: i64,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    /// TermF still has it open (running, or delivered): cleaning it closes the run.
    pub open: bool,
    /// Its work is over: its pull request was merged or closed, or the run is closed or failed.
    pub done: bool,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// Its tip is in the default branch.
    Merged,
    /// Its pull request was merged (a squash or a rebase leaves its commits out of the default branch).
    PrMerged,
    /// Its upstream was deleted on the remote.
    Gone,
    /// No commit for `STALE_DAYS`.
    Stale,
    /// A TermF run's, whose work is over.
    Run,
    /// A worktree on no branch.
    Detached,
    /// A worktree whose folder is gone.
    Prunable,
}

/// Why something is listed but cannot be selected.
#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Blocked {
    /// A locked worktree.
    Locked,
    /// The worktree or branch of a run still at work, or whose pull request is open.
    RunOpen,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BranchCleanup {
    pub name: String,
    pub tip: Tip,
    pub reasons: Vec<Reason>,
    /// The commits no other branch, remote branch or tag holds: deleting it loses them.
    pub lost: usize,
    pub run: Option<i64>,
    pub blocked: Option<Blocked>,
    /// Can go: done with, and what it loses is merged.
    pub suggested: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeCleanup {
    pub path: String,
    /// The branch deleted with it: when that loses no commit, or its pull request was merged. Otherwise the branch
    /// stays and only the folder goes.
    pub branch: Option<String>,
    /// The tip of that branch, the commit `git_branch_delete` checks; else the worktree's HEAD.
    pub head: Option<String>,
    pub date: i64,
    pub subject: String,
    pub reasons: Vec<Reason>,
    /// The commits lost with its branch.
    pub lost: usize,
    /// The uncommitted changes and untracked files lost with its folder.
    pub changes: usize,
    pub run: Option<i64>,
    /// Cleaning it closes its run in TermF (`tickets_close`), which removes the worktree and the branch.
    pub close_run: bool,
    pub blocked: Option<Blocked>,
    pub suggested: bool,
}

/// What can be cleaned up: local branches checked out nowhere (the default branch aside), then the worktrees other
/// than the repository's folder.
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cleanup {
    pub default_branch: Option<String>,
    pub branches: Vec<BranchCleanup>,
    pub worktrees: Vec<WorktreeCleanup>,
}

/// A reason that says the work is done with.
fn done(reason: &Reason) -> bool {
    matches!(
        reason,
        Reason::Merged | Reason::PrMerged | Reason::Gone | Reason::Run | Reason::Prunable
    )
}

/// The same folder, however it was written (a trailing slash, a symbolic link such as macOS's /tmp), even once gone.
fn same_path(a: &str, b: &str) -> bool {
    let canonical = |p: &str| {
        let p = Path::new(p.trim_end_matches('/'));
        std::fs::canonicalize(p)
            .or_else(|_| {
                let parent = std::fs::canonicalize(p.parent().unwrap_or(p))?;
                Ok::<_, std::io::Error>(parent.join(p.file_name().unwrap_or_default()))
            })
            .unwrap_or_else(|_| p.to_path_buf())
    };
    a.trim_end_matches('/') == b.trim_end_matches('/') || canonical(a) == canonical(b)
}

impl Git {
    /// Every local branch and worktree that could go, with its reasons and what it would lose. `runs`: TermF's runs
    /// of this repository; `merged`: the branches whose pull request was merged; `now`: seconds since the epoch.
    pub fn cleanup(
        &self,
        repo: &Path,
        runs: &[RunRef],
        merged: &[String],
        now: i64,
    ) -> Result<Cleanup> {
        let state = self.state(repo)?;
        let default_local = state.default_branch.as_deref().map(|d| {
            state
                .remotes
                .iter()
                .find_map(|r| d.strip_prefix(&format!("{r}/")))
                .unwrap_or(d)
                .to_owned()
        });
        let run_of_branch = |name: &str| runs.iter().find(|r| r.branch.as_deref() == Some(name));
        let reasons_of = |b: &Branch, run: Option<&RunRef>| {
            let mut reasons = Vec::new();
            if b.merged == Some(true) {
                reasons.push(Reason::Merged);
            }
            if merged.iter().any(|m| *m == b.name) {
                reasons.push(Reason::PrMerged);
            }
            if b.gone {
                reasons.push(Reason::Gone);
            }
            if now - b.tip.date > STALE_DAYS * 86_400 {
                reasons.push(Reason::Stale);
            }
            if run.is_some_and(|r| r.done) {
                reasons.push(Reason::Run);
            }
            reasons
        };
        let blocked_by =
            |run: Option<&RunRef>| run.filter(|r| r.open && !r.done).map(|_| Blocked::RunOpen);
        let suggest =
            |reasons: &[Reason], lost: usize, changes: usize, blocked: Option<Blocked>| {
                blocked.is_none()
                    && changes == 0
                    && ((lost == 0 && reasons.iter().any(done))
                        || reasons.contains(&Reason::PrMerged))
            };

        // A branch checked out in a worktree goes with it, so it is not listed on its own.
        let checked_out: Vec<&str> = state
            .worktrees
            .iter()
            .filter_map(|w| w.branch.as_deref())
            .collect();
        let mut branches = Vec::new();
        for b in state.branches.iter().filter(|b| {
            b.remote.is_none()
                && !checked_out.contains(&b.name.as_str())
                && Some(&b.name) != default_local.as_ref()
        }) {
            let run = run_of_branch(&b.name);
            let reasons = reasons_of(b, run);
            let lost = self.lost(repo, &b.name)?;
            let blocked = blocked_by(run);
            branches.push(BranchCleanup {
                name: b.name.clone(),
                tip: b.tip.clone(),
                suggested: suggest(&reasons, lost, 0, blocked),
                reasons,
                lost,
                run: run.map(|r| r.id),
                blocked,
            });
        }

        let mut worktrees = Vec::new();
        for wt in state.worktrees.iter().filter(|w| !w.main && !w.bare) {
            let run = runs
                .iter()
                .find(|r| {
                    r.worktree
                        .as_deref()
                        .is_some_and(|p| same_path(p, &wt.path))
                })
                .or_else(|| wt.branch.as_deref().and_then(run_of_branch));
            let branch = wt.branch.as_deref().and_then(|name| {
                state
                    .branches
                    .iter()
                    .find(|b| b.remote.is_none() && b.name == name)
            });
            let mut reasons = branch.map(|b| reasons_of(b, run)).unwrap_or_default();
            if branch.is_none() && run.is_some_and(|r| r.done) {
                reasons.push(Reason::Run);
            }
            if wt.branch.is_none() {
                reasons.push(Reason::Detached);
            }
            if wt.prunable {
                reasons.push(Reason::Prunable);
            }
            let changes = wt.changes.len() + wt.more;
            let branch_lost = match branch {
                Some(b) => self.lost(repo, &b.name)?,
                None => 0,
            };
            // The branch goes along unless that loses work its pull request does not hold; the default branch stays.
            let goes = |b: &&Branch| {
                Some(&b.name) != default_local.as_ref()
                    && (branch_lost == 0 || reasons.contains(&Reason::PrMerged))
            };
            let gone_with = branch.filter(goes);
            let lost = if gone_with.is_some() { branch_lost } else { 0 };
            let blocked = if wt.locked {
                Some(Blocked::Locked)
            } else {
                blocked_by(run)
            };
            worktrees.push(WorktreeCleanup {
                path: wt.path.clone(),
                branch: gone_with.map(|b| b.name.clone()),
                head: gone_with
                    .map(|b| b.tip.oid.clone())
                    .or_else(|| wt.head.clone()),
                date: branch.map(|b| b.tip.date).unwrap_or_default(),
                subject: branch.map(|b| b.tip.subject.clone()).unwrap_or_default(),
                suggested: suggest(&reasons, lost, changes, blocked),
                reasons,
                lost,
                changes,
                run: run.map(|r| r.id),
                close_run: run.is_some_and(|r| r.open && r.done),
                blocked,
            });
        }
        Ok(Cleanup {
            default_branch: state.default_branch.clone(),
            branches,
            worktrees,
        })
    }

    /// How many commits only the local branch `name` holds.
    fn lost(&self, repo: &Path, name: &str) -> Result<usize> {
        let full = format!("refs/heads/{}", checked_name(name)?);
        let exclude = format!("--exclude={name}");
        Ok(self
            .run(
                repo,
                &[
                    "rev-list",
                    "--count",
                    &full,
                    "--not",
                    &exclude,
                    "--branches",
                    "--remotes",
                    "--tags",
                ],
            )?
            .trim()
            .parse()
            .unwrap_or(0))
    }

    fn worktree(&self, repo: &Path, path: &str) -> Result<Worktree> {
        parse_worktrees(&self.run(repo, &["worktree", "list", "--porcelain"])?)
            .into_iter()
            .find(|w| same_path(&w.path, path))
            .with_context(|| format!("no worktree at {path}"))
    }

    /// Removes a worktree other than the repository's folder, its uncommitted work with it when it still has the
    /// `changes` seen when confirming: refused when that count changed. A worktree whose folder is gone is
    /// forgotten. Its branch stays.
    pub fn remove_worktree(&self, repo: &Path, path: &str, changes: usize) -> Result<()> {
        let wt = self.worktree(repo, path)?;
        ensure!(
            !wt.main,
            "{path} is the repository's folder: TermF does not remove it"
        );
        ensure!(!wt.locked, "{path} is locked: unlock it to remove it");
        let path = wt.path.as_str();
        if wt.prunable {
            self.run(repo, &["worktree", "remove", path])?;
            return Ok(());
        }
        let (listed, more) = parse_status(&self.run(
            Path::new(path),
            &["status", "--porcelain=v2", "-z", "--untracked-files=normal"],
        )?);
        ensure!(
            listed.len() + more == changes,
            "{path} has changed since you looked: look again before removing it"
        );
        let mut args = vec!["worktree", "remove"];
        if changes > 0 {
            args.push("--force");
        }
        args.push(path);
        self.run(repo, &args)?;
        Ok(())
    }

    /// Puts back a removed worktree: on its branch, else detached at `head`.
    pub fn restore_worktree(
        &self,
        repo: &Path,
        path: &str,
        branch: Option<&str>,
        head: &str,
    ) -> Result<()> {
        ensure!(
            Path::new(path).is_absolute(),
            "not an absolute path: {path:?}"
        );
        ensure!(!Path::new(path).exists(), "{path} exists already");
        match branch {
            Some(branch) => self.run(repo, &["worktree", "add", path, checked_name(branch)?])?,
            None => self.run(
                repo,
                &["worktree", "add", "--detach", path, checked_name(head)?],
            )?,
        };
        Ok(())
    }
}
