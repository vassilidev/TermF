//! What the engine asks of git: a worktree per task, started from the run's branch, whose work is committed
//! then merged back into the run's worktree.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

pub struct Git<'a>(pub &'a [(String, String)]);

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
        self.run(main, &["worktree", "add", "-q", "--detach", &path, &base])?;
        Ok(base)
    }

    pub fn remove_worktree(&self, main: &Path, path: &Path) {
        let _ = self.run(
            main,
            &["worktree", "remove", "--force", &path.to_string_lossy()],
        );
    }

    /// Commits everything that changed in `dir`, when anything did.
    pub fn commit_all(&self, dir: &Path, message: &str) -> Result<()> {
        self.run(dir, &["add", "-A"])?;
        let staged = !self
            .output(dir, &["diff", "--cached", "--quiet"])?
            .status
            .success();
        if staged {
            self.run(dir, &["commit", "-q", "-m", message])?;
        }
        Ok(())
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
