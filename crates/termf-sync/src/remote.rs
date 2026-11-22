//! Where the encrypted files live. Every remote is seen through a local folder: a plain folder is used in place,
//! a git repository through a clone under the config folder, an object store (S3, WebDAV) through a mirror of
//! its objects (`objects.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RemoteSpec {
    /// Any git remote the user's `git` can reach (SSH or HTTPS), ideally private.
    Git { url: String },
    /// A folder another tool syncs: iCloud Drive, Dropbox, Syncthing, a network share.
    Folder { path: String },
    /// An S3 bucket (AWS, R2, MinIO, B2…), path-style: `<endpoint>/<bucket>/<prefix><object>`.
    S3 {
        endpoint: String,
        bucket: String,
        region: String,
        #[serde(default)]
        prefix: String,
        /// Given when setting up only: kept in the system keychain, never written with the rest.
        #[serde(default, skip_serializing)]
        credentials: Option<Credentials>,
    },
    /// A WebDAV folder (Nextcloud, ownCloud, Apache, rclone…), over HTTPS or HTTP.
    Webdav {
        url: String,
        #[serde(default, skip_serializing)]
        credentials: Option<Credentials>,
    },
}

impl RemoteSpec {
    /// What the user knows the remote by; also its credentials' name in the keychain.
    pub fn label(&self) -> String {
        match self {
            RemoteSpec::Git { url } | RemoteSpec::Webdav { url, .. } => url.clone(),
            RemoteSpec::Folder { path } => path.clone(),
            RemoteSpec::S3 {
                endpoint,
                bucket,
                prefix,
                ..
            } => format!("{}/{bucket}/{prefix}", endpoint.trim_end_matches('/')),
        }
    }
}

/// S3: access key id and secret access key; WebDAV: user name and password.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub user: String,
    pub secret: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Credentials({}, …)", self.user)
    }
}

pub trait Remote {
    fn root(&self) -> &Path;
    /// Makes `root` show the remote's current state, before reading it.
    fn begin(&mut self) -> Result<()>;
    /// Publishes what was written in `root`. `Ok(false)`: another machine published first; begin again.
    fn commit(&mut self) -> Result<bool>;
    /// Refuses a remote that could let a machine overwrite another's sync, before it is set up.
    fn check(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct Folder(pub PathBuf);

impl Remote for Folder {
    fn root(&self) -> &Path {
        &self.0
    }

    fn begin(&mut self) -> Result<()> {
        // A missing folder (unmounted share, signed-out cloud drive) must not read as an empty remote.
        ensure!(
            self.0.is_dir(),
            "cannot reach the folder {}",
            self.0.display()
        );
        Ok(())
    }

    fn commit(&mut self) -> Result<bool> {
        Ok(true)
    }
}

const BRANCH: &str = "main";

pub struct Git {
    url: String,
    dir: PathBuf,
    env: Vec<(String, String)>,
}

impl Git {
    pub fn new(url: String, dir: PathBuf, env: Vec<(String, String)>) -> Self {
        Self { url, dir, env }
    }

    fn command(&self, cwd: &Path, args: &[&str]) -> Result<Output> {
        let mut cmd = Command::new("git");
        cmd.current_dir(cwd)
            // Encrypted blobs must reach the remote byte for byte; commits are TermF's, never signed or hooked.
            .args(["-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"])
            .args([
                "-c",
                "user.name=TermF",
                "-c",
                "user.email=sync@termf.invalid",
            ])
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.output().context("cannot run git: is it installed?")
    }

    fn git(&self, args: &[&str]) -> Result<String> {
        let out = self.command(&self.dir, args)?;
        if !out.status.success() {
            bail!(
                "git {} failed: {}",
                args[0],
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }

    fn clone(&self) -> Result<()> {
        let _ = fs::remove_dir_all(&self.dir);
        let parent = self.dir.parent().context("no parent folder")?;
        fs::create_dir_all(parent)?;
        let dir = self.dir.to_string_lossy();
        let out = self.command(parent, &["clone", "-q", "--", &self.url, &dir])?;
        ensure!(
            out.status.success(),
            "cannot clone {}: {}",
            self.url,
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(())
    }
}

impl Remote for Git {
    fn root(&self) -> &Path {
        &self.dir
    }

    fn begin(&mut self) -> Result<()> {
        let same_url = self.dir.join(".git").is_dir()
            && self.git(&["config", "--get", "remote.origin.url"]).ok() == Some(self.url.clone());
        if same_url {
            self.git(&["fetch", "-q", "--prune", "origin"])?;
        } else {
            self.clone()?;
        }
        let tip = format!("refs/remotes/origin/{BRANCH}");
        if self.git(&["rev-parse", "-q", "--verify", &tip]).is_ok() {
            // The clone is TermF's own: whatever an interrupted sync left in it goes.
            self.git(&["checkout", "-q", "-f", "-B", BRANCH, &tip])?;
            self.git(&["clean", "-fdxq"])?;
        } else {
            let branches = self.git(&[
                "for-each-ref",
                "--format=%(refname:lstrip=3)",
                "refs/remotes/origin/",
            ])?;
            let branches: Vec<&str> = branches.lines().filter(|b| *b != "HEAD").collect();
            ensure!(
                branches.is_empty(),
                "{} has no {BRANCH} branch but holds others ({}): choose an empty repository",
                self.url,
                branches.join(", ")
            );
            // An empty remote: start `main` from nothing, dropping a commit whose push failed.
            self.git(&["symbolic-ref", "HEAD", &format!("refs/heads/{BRANCH}")])?;
            let _ = self.git(&["update-ref", "-d", &format!("refs/heads/{BRANCH}")]);
            self.git(&["read-tree", "--empty"])?;
            self.git(&["clean", "-fdxq"])?;
            fs::write(self.dir.join(".gitattributes"), "* -text\n")?;
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<bool> {
        self.git(&["add", "-A"])?;
        if self
            .command(&self.dir, &["diff", "--cached", "--quiet"])?
            .status
            .success()
        {
            return Ok(true);
        }
        self.git(&["commit", "-q", "--no-verify", "-m", "termf sync"])?;
        let out = self.command(&self.dir, &["push", "-q", "--no-verify", "origin", BRANCH])?;
        if out.status.success() {
            return Ok(true);
        }
        let error = String::from_utf8_lossy(&out.stderr);
        if error.contains("(fetch first)") || error.contains("(non-fast-forward)") {
            return Ok(false);
        }
        bail!("cannot push to {}: {}", self.url, error.trim())
    }
}

/// What a folder or repository already holds that TermF did not put there, so sync never moves into a real
/// project by mistake. Hidden files, a README and a LICENSE are fine.
pub fn foreign(root: &Path, ours: &[&str]) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        let upper = name.to_uppercase();
        if !name.starts_with('.')
            && !ours.contains(&name.as_str())
            && !upper.starts_with("README")
            && !upper.starts_with("LICENSE")
        {
            found.push(name);
        }
    }
    found.sort();
    Ok(found)
}
