//! What the encrypted sync carries of `~/.ssh` when asked to (§5.4): the key pairs, and the Host blocks TermF
//! added to `~/.ssh/config`. A file already there is never replaced.

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use zeroize::Zeroizing;

use crate::config::{self, HostEntry};
use crate::{NOT_KEYS, Ssh, create_private_dir, file_name, read_private};

pub struct Pair {
    pub name: String,
    pub private: Zeroizing<Vec<u8>>,
    pub public: Option<Vec<u8>>,
}

/// A file name another machine may give a key: directly in `~/.ssh`, never one of its other files.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.ends_with(".pub")
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
        && !NOT_KEYS.contains(&name)
}

/// An `IdentityFile` another machine's block may set here: a file under `~/.ssh`, in a subfolder or not.
pub fn synced_identity(path: &str) -> bool {
    path.strip_prefix("~/.ssh/").is_some_and(|rest| {
        !rest.is_empty()
            && !rest.contains('\\')
            && !rest.chars().any(char::is_control)
            && rest
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
    })
}

impl Ssh {
    /// The private keys right in `~/.ssh` (symlinks left out: their target may be anyone's file), with
    /// their `.pub` when it is a plain file too. A file that cannot be read is an error, never a missing key:
    /// the sync would take it as deleted.
    pub fn pairs(&self) -> Result<Vec<Pair>> {
        let mut pairs = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(pairs),
            Err(e) => return Err(e).with_context(|| format!("cannot read {}", self.dir.display())),
        };
        let plain = |p: &Path| fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file());
        let mut paths = Vec::new();
        for entry in entries {
            paths.push(entry?.path());
        }
        paths.sort();
        for path in paths {
            let name = file_name(&path);
            if !valid_name(&name) || !plain(&path) {
                continue;
            }
            match read_private(&path) {
                Ok(Some(_)) => {}
                Ok(None) => continue,
                // A key ssh-key cannot parse is still a key: it travels as it is.
                Err(e) if e.downcast_ref::<std::io::Error>().is_none() => {}
                Err(e) => return Err(e.context(format!("cannot read {}", path.display()))),
            }
            let public = path.with_file_name(format!("{name}.pub"));
            pairs.push(Pair {
                private: Zeroizing::new(fs::read(&path)?),
                public: plain(&public).then(|| fs::read(&public)).transpose()?,
                name,
            });
        }
        Ok(pairs)
    }

    /// Writes another machine's pair: the private key readable by the user only, its `.pub` beside it.
    /// Nothing is written when either file exists.
    pub fn restore(&self, pair: &Pair) -> Result<()> {
        ensure!(valid_name(&pair.name), "invalid key name {:?}", pair.name);
        create_private_dir(&self.dir)?;
        let private = self.dir.join(&pair.name);
        let public = self.dir.join(format!("{}.pub", pair.name));
        for file in [&private, &public] {
            ensure!(
                fs::symlink_metadata(file).is_err(),
                "{} already exists",
                file.display()
            );
        }
        create(&private, &pair.private, 0o600)?;
        if let Some(bytes) = &pair.public
            && let Err(e) = create(&public, bytes, 0o644)
        {
            let _ = fs::remove_file(&private);
            return Err(e);
        }
        Ok(())
    }

    /// `~/.ssh/config`, empty when there is none. Unreadable (not UTF-8, no access) is an error, never empty:
    /// the sync would take its blocks as deleted.
    fn config_text(&self) -> Result<String> {
        match fs::read_to_string(self.config()) {
            Ok(text) => Ok(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e).context("cannot read ~/.ssh/config"),
        }
    }

    /// The blocks of `~/.ssh/config` TermF added.
    pub fn marked_hosts(&self) -> Result<Vec<HostEntry>> {
        Ok(config::marked(&self.config_text()?))
    }

    /// Whether `alias` has a block written by hand, which the sync never touches.
    pub fn hand_written(&self, alias: &str) -> Result<bool> {
        Ok(config::hand_written(&self.config_text()?, alias))
    }

    /// Makes TermF's blocks those of `hosts`: added, updated, or removed when gone. A block written by hand
    /// for the same alias is left as it is, and a key path outside `~/.ssh` is not written (`synced_identity`).
    pub fn apply_hosts(&self, hosts: &[HostEntry]) -> Result<()> {
        let before = self.config_text()?;
        let mut after = before.clone();
        for old in config::marked(&before) {
            if !hosts
                .iter()
                .any(|h| h.alias.eq_ignore_ascii_case(&old.alias))
            {
                after = config::remove_marked(&after, &old.alias);
            }
        }
        for host in hosts {
            if config::hand_written(&after, &host.alias) {
                continue;
            }
            let mut host = host.clone();
            host.identity_file = host.identity_file.filter(|f| synced_identity(f));
            after = config::upsert(&after, &host)?;
        }
        if after != before {
            create_private_dir(&self.dir)?;
            config::write(&self.config(), &after)?;
        }
        Ok(())
    }
}

fn create(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
    #[cfg(not(unix))]
    let _ = mode;
    let mut file = options
        .open(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
