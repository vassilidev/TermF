//! What the encrypted sync carries of `~/.ssh` when asked to (§5.4): the key pairs, and `~/.ssh/config` whole.
//! A key already there is never replaced.

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use zeroize::Zeroizing;

use crate::config;
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

    /// `~/.ssh/config` as it is, `None` when there is none. Unreadable is an error, never `None`: the sync would
    /// take it as deleted.
    pub fn config_file(&self) -> Result<Option<Vec<u8>>> {
        match fs::read(self.config()) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("cannot read ~/.ssh/config"),
        }
    }

    /// Replaces `~/.ssh/config` whole, as the Hosts tab writes it, or deletes it.
    pub fn set_config_file(&self, bytes: Option<&[u8]>) -> Result<()> {
        let Some(bytes) = bytes else {
            return match fs::remove_file(self.config()) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            };
        };
        create_private_dir(&self.dir)?;
        config::write(&self.config(), bytes)
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
