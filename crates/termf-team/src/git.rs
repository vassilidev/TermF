//! What the engine asks of git: a worktree per task, started from the run's branch, whose work is committed
//! then merged back into the run's worktree.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

pub struct Git<'a>(pub &'a [(String, String)]);

/// How a block that Next.js manages in an agent instructions file begins; its end names it the same way.
const MANAGED: &str = "<!-- BEGIN:nextjs-";

/// The lines of `text` outside its managed blocks, blank ones left out.
fn unmanaged(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(MANAGED) {
        let name = &rest[start + MANAGED.len()..];
        let Some(name) = name.find("-->").map(|n| &name[..n]) else {
            break;
        };
        let end = format!("<!-- END:nextjs-{name}-->");
        let Some(stop) = rest[start..].find(&end) else {
            break;
        };
        parts.push(&rest[..start]);
        rest = &rest[start + stop + end.len()..];
    }
    parts.push(rest);
    parts
        .into_iter()
        .flat_map(str::lines)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

impl Git<'_> {
    fn output(&self, dir: &Path, args: &[&str]) -> Result<std::process::Output> {
        #[allow(unused_mut)]
        let mut cmd = Command::new("git");
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
        cmd.arg("-C")
            .arg(dir)
            .args(args)
            .envs(self.0.iter().map(|(k, v)| (k, v)))
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .context("cannot run git: is it installed?")
    }

    pub fn run(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let out = self.output(dir, args)?;
        ensure!(
            out.status.success(),
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
    }

    pub fn head(&self, dir: &Path) -> Result<String> {
        self.run(dir, &["rev-parse", "HEAD"])
    }

    /// A detached worktree at `path` from the commit `main` (a worktree of the same repository) is on.
    pub fn add_worktree(&self, main: &Path, path: &Path) -> Result<String> {
        let base = self.head(main)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let path = path.to_string_lossy();
        // A task's folder removed by hand stays registered until pruned, and `add` would refuse its path.
        let _ = self.run(main, &["worktree", "prune"]);
        self.run(main, &["worktree", "add", "-q", "--detach", &path, &base])?;
        Ok(base)
    }

    pub fn remove_worktree(&self, main: &Path, path: &Path) {
        let _ = self.run(
            main,
            &["worktree", "remove", "--force", &path.to_string_lossy()],
        );
    }

    /// Whether `dir` is in the middle of a merge.
    pub fn merging(&self, dir: &Path) -> bool {
        self.run(dir, &["rev-parse", "-q", "--verify", "MERGE_HEAD"])
            .is_ok()
    }

    /// Whether `commit` is part of what `dir` is on.
    pub fn contains(&self, dir: &Path, commit: &str) -> Result<bool> {
        let out = self.output(dir, &["merge-base", "--is-ancestor", commit, "HEAD"])?;
        match out.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => anyhow::bail!(
                "git merge-base failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }
    }

    /// Commits everything that changed in `dir`, when anything did or a merge is under way, unless a conflict
    /// marker is left.
    pub fn commit_all(&self, dir: &Path, message: &str) -> Result<()> {
        self.run(dir, &["add", "-A"])?;
        let check = self.output(dir, &["diff", "--cached", "--check"])?;
        let markers: Vec<&str> = std::str::from_utf8(&check.stdout)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.ends_with("leftover conflict marker"))
            .collect();
        ensure!(
            markers.is_empty(),
            "conflict markers are left: {}",
            markers.join(", ")
        );
        let staged = !self
            .output(dir, &["diff", "--cached", "--quiet"])?
            .status
            .success();
        // A merge resolved to what HEAD already had stages nothing, yet must still be concluded.
        if staged || self.merging(dir) {
            self.run(dir, &["commit", "-q", "-m", message])?;
        }
        Ok(())
    }

    /// Puts back each `AGENTS.md` and `CLAUDE.md` of `dir` whose only change is a block a tool manages on its own
    /// (`next dev` writes `<!-- BEGIN:nextjs-agent-rules -->`…), unless `asked` says the ticket asks for that
    /// path: what it put back. During a merge, what the merged work brings stays.
    pub fn set_aside_managed(
        &self,
        dir: &Path,
        asked: impl Fn(&str) -> bool,
    ) -> Result<Vec<String>> {
        let changed = self.run(dir, &["diff", "--name-only", "-z", "HEAD"])?;
        let new = self.run(dir, &["ls-files", "-o", "--exclude-standard", "-z"])?;
        let merged = match self.run(dir, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]) {
            Ok(_) => self.run(dir, &["diff", "--name-only", "-z", "HEAD", "MERGE_HEAD"])?,
            Err(_) => String::new(),
        };
        let merged: Vec<&str> = merged.split('\0').collect();
        let mut kept = Vec::new();
        for path in changed.split('\0').chain(new.split('\0')) {
            let name = Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if !matches!(name, "AGENTS.md" | "CLAUDE.md") || asked(path) || merged.contains(&path) {
                continue;
            }
            let Ok(now) = std::fs::read_to_string(dir.join(path)) else {
                continue;
            };
            let before = self.run(dir, &["show", &format!("HEAD:{path}")]).ok();
            if !now.contains(MANAGED)
                || unmanaged(&now) != unmanaged(before.as_deref().unwrap_or(""))
            {
                continue;
            }
            match before {
                Some(_) => {
                    self.run(dir, &["checkout", "-q", "HEAD", "--", path])?;
                }
                None => std::fs::remove_file(dir.join(path))?,
            }
            kept.push(path.to_owned());
        }
        Ok(kept)
    }

    /// The files changed between `base` and what `dir` is on, its diff without context, and how many lines it
    /// adds and removes.
    pub fn changes(&self, dir: &Path, base: &str) -> Result<(Vec<String>, String, usize)> {
        let names = self.run(dir, &["diff", "--name-only", "--no-renames", base, "HEAD"])?;
        let diff = self.run(
            dir,
            &["diff", "-U0", "--no-color", "--no-renames", base, "HEAD"],
        )?;
        let stat = self.run(dir, &["diff", "--numstat", "--no-renames", base, "HEAD"])?;
        let lines = stat
            .lines()
            .flat_map(|l| l.split_whitespace().take(2))
            .filter_map(|n| n.parse::<usize>().ok())
            .sum();
        Ok((names.lines().map(str::to_owned).collect(), diff, lines))
    }

    /// Merges `commit` into what `dir` is on, leaving a conflict there: the files in conflict.
    pub fn merge_in(&self, dir: &Path, commit: &str, message: &str) -> Result<Vec<String>> {
        let out = self.output(dir, &["merge", "-q", "--no-edit", "-m", message, commit])?;
        if out.status.success() {
            return Ok(Vec::new());
        }
        let conflicts = self.run(dir, &["diff", "--name-only", "--diff-filter=U"])?;
        if conflicts.is_empty() {
            let _ = self.output(dir, &["merge", "--abort"]);
            anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(conflicts.lines().map(str::to_owned).collect())
    }

    /// Merges `commit` into the branch `dir` is on; a conflict leaves `dir` as it was.
    pub fn merge(&self, dir: &Path, commit: &str, message: &str) -> Result<()> {
        let out = self.output(dir, &["merge", "-q", "--no-edit", "-m", message, commit])?;
        if !out.status.success() {
            let _ = self.output(dir, &["merge", "--abort"]);
            let said = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            anyhow::bail!("{}", termf_core::process::tail(&said));
        }
        Ok(())
    }
}
