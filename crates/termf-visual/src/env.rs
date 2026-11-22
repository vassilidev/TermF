//! The two sides of a review: a worktree each, and its dev server. "Before" is a detached worktree of the base
//! commit, kept under `<data>/visual/worktrees/` and prepared once by the project's recipe, so the next review
//! of the same commit starts at once; "after" is the run's worktree, or another cached one for a review of two
//! commits.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use termf_worktree::{Setup, Worktree};

use crate::browser::stop;

/// Cached worktrees kept per repository.
const KEEP: usize = 3;

pub(crate) fn git(env: &[(String, String)], dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .output()
        .context("cannot run git")?;
    ensure!(
        out.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The commit a ref names.
pub(crate) fn commit(env: &[(String, String)], repo: &Path, r: &str) -> Result<String> {
    ensure!(!r.starts_with('-'), "invalid ref {r}");
    git(
        env,
        repo,
        &["rev-parse", "--verify", &format!("{r}^{{commit}}")],
    )
    .with_context(|| format!("{r} is not a commit of {}", repo.display()))
}

/// A worktree of `repo` at `commit`, prepared by the project's recipe, created on first use. The oldest ones
/// beyond [`KEEP`] are torn down and removed.
pub(crate) fn cached(
    setup: &Setup,
    project: &str,
    repo_name: &str,
    repo: &Path,
    commit: &str,
) -> Result<(PathBuf, Vec<String>)> {
    let short = &commit[..commit.len().min(12)];
    let root = setup
        .data_dir
        .join("visual/worktrees")
        .join(format!("{project}-{repo_name}"));
    let dir = root.join(short);
    let ready = root.join(format!("{short}.ready"));
    let wt = Worktree {
        project: project.to_owned(),
        repo: repo_name.to_owned(),
        source: repo.to_path_buf(),
        path: dir.clone(),
        id: format!("visual-{short}"),
    };
    let mut notes = Vec::new();
    if !ready.exists() {
        if dir.exists() {
            // Left half made: start again.
            let _ = git(
                &setup.env,
                repo,
                &["worktree", "remove", "--force", &dir.to_string_lossy()],
            );
            let _ = fs::remove_dir_all(&dir);
        }
        fs::create_dir_all(&root)?;
        let _ = git(&setup.env, repo, &["worktree", "prune"]);
        git(
            &setup.env,
            repo,
            &[
                "worktree",
                "add",
                "--detach",
                &dir.to_string_lossy(),
                commit,
            ],
        )?;
        let report = setup.run(&wt);
        for step in report.steps.iter().filter(|s| s.outcome.failed()) {
            notes.push(format!(
                "setup of the worktree at {short}: {}",
                step.describe()
            ));
        }
        fs::write(&ready, serde_json::to_string(&report.steps)?)?;
    }
    // Its use time decides which ones go.
    fs::write(&ready, fs::read(&ready)?)?;
    let mut kept: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(&root)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "ready"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    kept.sort_by_key(|k| std::cmp::Reverse(k.0));
    for (_, marker) in kept.into_iter().skip(KEEP) {
        let old = marker.with_extension("");
        let name = old
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        setup.teardown(&old, &format!("visual-{name}"));
        let _ = git(
            &setup.env,
            repo,
            &["worktree", "remove", "--force", &old.to_string_lossy()],
        );
        let _ = fs::remove_dir_all(&old);
        let _ = fs::remove_file(&marker);
    }
    Ok((dir, notes))
}

/// A dev server, stopped with what it started when dropped.
pub(crate) struct Server {
    child: Child,
    log: PathBuf,
    pub port: u16,
}

impl Server {
    /// Starts `command` in `dir` through the login shell, as the worktree's sessions would, with the
    /// environment its setup gave and its own port: `PORT`, and `{{ port }}` in the command.
    pub fn start(
        command: &str,
        dir: &Path,
        env: &BTreeMap<String, String>,
        port: u16,
        log: &Path,
    ) -> Result<Server> {
        // Another server there would be captured instead of this one.
        ensure!(
            !answers(port),
            "port {port} is already taken by another server: stop it, then start again"
        );
        let line = command
            .replace("{{ port }}", &port.to_string())
            .replace("{{port}}", &port.to_string());
        let (program, args) = termf_term::shell_command(Some(&line));
        let file = fs::File::create(log)?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(dir)
            .envs(env)
            .env("PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(file.try_clone()?)
            .stderr(file);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let child = cmd
            .spawn()
            .with_context(|| format!("cannot start the dev server `{line}`"))?;
        Ok(Server {
            child,
            log: log.to_path_buf(),
            port,
        })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Waits until it answers HTTP on its port.
    pub fn wait(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if answers(self.port) {
                return Ok(());
            }
            if let Some(status) = self.child.try_wait()? {
                bail!(
                    "the dev server ended ({status}) before answering on port {}: {}",
                    self.port,
                    self.tail()
                );
            }
            if Instant::now() > deadline {
                bail!(
                    "the dev server did not answer on port {} within {} s: {}",
                    self.port,
                    timeout.as_secs(),
                    self.tail()
                );
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    fn tail(&self) -> String {
        let text = fs::read_to_string(&self.log).unwrap_or_default();
        match termf_core::process::tail(&text) {
            t if t.is_empty() => format!("nothing in {}", self.log.display()),
            t => t,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        stop(&mut self.child);
    }
}

/// Whether something answers HTTP on the port, on IPv4 or IPv6 (`localhost` may be either).
pub(crate) fn answers(port: u16) -> bool {
    let addrs: [SocketAddr; 2] = [
        ([127, 0, 0, 1], port).into(),
        ([0u16, 0, 0, 0, 0, 0, 0, 1], port).into(),
    ];
    addrs.iter().any(|addr| {
        let Ok(mut stream) = TcpStream::connect_timeout(addr, Duration::from_millis(500)) else {
            return false;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut head = [0u8; 5];
        stream
            .write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .and_then(|_| stream.read_exact(&mut head))
            .is_ok()
            && &head == b"HTTP/"
    })
}
