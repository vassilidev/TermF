//! SSH key manager (architecture §12.7): the keys of `~/.ssh`, the agent, the config hosts and the
//! repositories that use them. Private key contents never leave this crate: only public data is returned.

mod config;
mod formats;
mod keys;
mod ppk;
mod tools;

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Serialize};
use ssh_key::public::KeyData;
use ssh_key::{EcdsaCurve, HashAlg, PrivateKey, PublicKey};
use zeroize::Zeroizing;

pub use config::{Host, HostEntry};
pub use keys::{Algo, Check, CheckKind, Format, Generated, KeyFile, Locked, Spec, Usage};
pub use tools::{
    ASKPASS_ENV, Agent, AgentKey, HostTest, Remote, RepoKeys, askpass, clipboard_clear, test_host,
};

pub const TRASH: &str = ".termf-trash";

// Private keys are a few KB: anything larger in ~/.ssh is something else.
const MAX_KEY_BYTES: u64 = 64 * 1024;

// Files of ~/.ssh that are not keys, and names a new key must not take.
const NOT_KEYS: &[&str] = &[
    "config",
    "known_hosts",
    "known_hosts.old",
    "authorized_keys",
    "authorized_keys2",
    "environment",
    "rc",
];

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub dir: String,
    pub keys: Vec<Key>,
    pub agent: Agent,
    pub hosts: Vec<Host>,
    pub repos: Vec<RepoKeys>,
    /// Files that could not be read or parsed; everything else is still listed.
    pub problems: Vec<Problem>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    /// The private key, or the `.pub` of an orphan: what the actions take.
    pub path: String,
    pub name: String,
    pub public_path: Option<String>,
    /// A `.pub` without its private key.
    pub orphan: bool,
    /// `ssh-ed25519`, `ecdsa-sha2-nistp256`, `ssh-rsa`…
    pub algorithm: String,
    pub bits: Option<u32>,
    /// `SHA256:…`, as `ssh-add -l` shows it.
    pub fingerprint: Option<String>,
    pub comment: String,
    /// The `.pub` line, to copy or upload.
    pub public_key: Option<String>,
    /// `None` for an orphan.
    pub encrypted: Option<bool>,
    /// Unix permission bits of the private key (of the `.pub` for an orphan).
    pub mode: Option<u32>,
    /// Readable by others: ssh refuses such a key. Never set for a symlink, which "fix permissions" leaves alone.
    pub too_open: bool,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
    pub in_agent: bool,
    /// Patterns of the config hosts naming this key.
    pub hosts: Vec<String>,
    pub repos: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub path: String,
    pub error: String,
}

impl Problem {
    fn new(path: &Path, error: impl std::fmt::Display) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            error: error.to_string(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    #[default]
    Ed25519,
    /// NIST P-256.
    Ecdsa,
    /// 4096 bits, for old servers.
    Rsa,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rotation {
    /// The new private key, which exists whatever failed afterwards.
    pub key: String,
    pub uploaded: bool,
    /// Config hosts (config and included files) now naming the new key.
    pub hosts: Vec<String>,
    /// Repositories whose `core.sshCommand` now names the new key.
    pub repos: Vec<String>,
    /// Set when hosts or repositories were repointed without an upload: no server knows the new key yet.
    pub warning: Option<String>,
    /// Steps that failed; everything listed above is done. A failed upload repoints nothing.
    pub errors: Vec<String>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostPreview {
    /// Empty when the config already says so.
    pub diff: String,
    /// Why the block may not fully apply (an earlier block matching the alias, an included file).
    pub warnings: Vec<String>,
}

struct Private {
    /// `None` for PEM keys, which ssh-key cannot read: their `.pub` has the details.
    public: Option<PublicKey>,
    encrypted: bool,
    algorithm: &'static str,
}

#[derive(Clone, Debug)]
pub struct Ssh {
    home: PathBuf,
    dir: PathBuf,
}

impl Ssh {
    pub fn new(home: PathBuf) -> Self {
        Self {
            dir: home.join(".ssh"),
            home,
        }
    }

    /// `~/.ssh`, which is `%USERPROFILE%\.ssh` on Windows as for OpenSSH.
    pub fn current() -> Result<Self> {
        Ok(Self::new(dirs::home_dir().context("no home folder")?))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn config(&self) -> PathBuf {
        self.dir.join("config")
    }

    /// The server `host` reaches: the first `HostName` the config gives it (an alias), else `host` itself.
    pub fn host_name(&self, host: &str) -> String {
        config::hosts(&self.dir, &self.home)
            .into_iter()
            .filter(|h| h.matches(host))
            .find_map(|h| h.host_name)
            .unwrap_or_else(|| host.to_owned())
    }

    /// `repos`: the known repositories, whose remotes are mapped to keys.
    pub fn inventory(&self, repos: &[PathBuf]) -> Inventory {
        self.collect(repos, tools::agent())
    }

    fn collect(&self, repos: &[PathBuf], agent: Agent) -> Inventory {
        let (mut keys, problems) = self.scan();
        let hosts = config::hosts(&self.dir, &self.home);
        let repos = tools::repos_keys(repos, &hosts, &self.home);
        for key in &mut keys {
            let files: Vec<PathBuf> = [Some(&key.path), key.public_path.as_ref()]
                .into_iter()
                .flatten()
                .map(PathBuf::from)
                .collect();
            let uses = |f: &String| files.iter().any(|k| same_file(Path::new(f), k));
            key.in_agent = key
                .fingerprint
                .as_ref()
                .is_some_and(|f| agent.keys.iter().any(|k| &k.fingerprint == f));
            key.hosts = hosts
                .iter()
                .filter(|h| h.identity_files.iter().any(uses))
                .map(|h| h.patterns.join(" "))
                .collect();
            key.repos = repos
                .iter()
                .filter(|r| r.identity_files.iter().any(uses))
                .map(|r| r.path.clone())
                .collect();
        }
        Inventory {
            dir: self.dir.to_string_lossy().into_owned(),
            keys,
            agent,
            hosts,
            repos,
            problems,
        }
    }

    fn scan(&self) -> (Vec<Key>, Vec<Problem>) {
        let mut keys = Vec::new();
        let mut problems = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return (keys, problems);
        };
        let mut files = Vec::new();
        for path in entries.flatten().map(|e| e.path()).filter(|p| p.is_file()) {
            let name = file_name(&path);
            if name.starts_with('.') || NOT_KEYS.contains(&name.as_str()) {
                continue;
            }
            // Every action refuses a symlink leading out of ~/.ssh, so it is reported rather than listed.
            match self.resolve(&path.to_string_lossy()) {
                Ok(_) => files.push(path),
                Err(_) => problems.push(Problem::new(
                    &path,
                    "links to a file outside ~/.ssh, which TermF leaves alone",
                )),
            }
        }
        files.sort();
        for path in &files {
            if file_name(path).ends_with(".pub") {
                continue;
            }
            match read_private(path) {
                Ok(Some(private)) => keys.push(key(path, Some(private), &mut problems)),
                Ok(None) => {}
                Err(e) => problems.push(Problem::new(path, format!("{e:#}"))),
            }
        }
        for path in &files {
            let name = file_name(path);
            let listed = keys
                .iter()
                .any(|k| k.public_path.as_deref() == Some(&*path.to_string_lossy()));
            if !name.ends_with(".pub") || name.ends_with("-cert.pub") || listed {
                continue;
            }
            match PublicKey::read_openssh_file(path) {
                Ok(_) => keys.push(key(path, None, &mut problems)),
                Err(e) => problems.push(Problem::new(path, e)),
            }
        }
        (keys, problems)
    }

    /// A path from the UI. Both the entry (once `..` and symlinked folders are resolved) and the file it
    /// leads to must be inside `~/.ssh` and outside the trash, so a symlink cannot lead out. Returns the
    /// entry, not its target, under `~/.ssh` as the user knows it.
    pub fn resolve(&self, path: &str) -> Result<PathBuf> {
        let dir = self
            .dir
            .canonicalize()
            .with_context(|| format!("cannot open {}", self.dir.display()))?;
        let joined = dir.join(path);
        let outside = || anyhow!("{path} is outside {}", self.dir.display());
        let name = joined.file_name().ok_or_else(outside)?;
        let entry = joined
            .parent()
            .ok_or_else(outside)?
            .canonicalize()
            .with_context(|| format!("cannot open {path}"))?
            .join(name);
        let target = entry
            .canonicalize()
            .with_context(|| format!("cannot open {path}"))?;
        for file in [&entry, &target] {
            let inside = file.strip_prefix(&dir).map_err(|_| outside())?;
            ensure!(inside.components().next().is_some(), "{path} is not a file");
            ensure!(
                inside.components().next() != Some(Component::Normal(TRASH.as_ref())),
                "{path} is in the trash"
            );
        }
        ensure!(target.is_file(), "{path} is not a file");
        Ok(self.dir.join(entry.strip_prefix(&dir)?))
    }

    /// The private and public files of the key at `path` (either of them). The other half goes through
    /// the same guard, so a sibling symlinked out of `~/.ssh` is ignored.
    fn pair(&self, path: &str) -> Result<(Option<PathBuf>, Option<PathBuf>)> {
        let file = self.resolve(path)?;
        let sibling = |p: PathBuf| self.resolve(&p.to_string_lossy()).ok();
        Ok(match file.to_string_lossy().strip_suffix(".pub") {
            Some(private) => (sibling(PathBuf::from(private)), Some(file)),
            None => {
                let public = sibling(with_pub(&file));
                (Some(file), public)
            }
        })
    }

    fn public_key(&self, path: &str) -> Result<PublicKey> {
        match self.pair(path)? {
            (_, Some(public)) => Ok(PublicKey::read_openssh_file(&public)?),
            (Some(private), None) => read_private(&private)?
                .and_then(|p| p.public)
                .context("this key has no .pub file"),
            (None, None) => unreachable!("resolve found a file"),
        }
    }

    /// `~/.ssh/<name>`, as the config and other machines read it.
    fn portable(&self, file: &Path) -> String {
        let inside = file.strip_prefix(&self.dir).unwrap_or(file);
        let parts: Vec<_> = inside.iter().map(|p| p.to_string_lossy()).collect();
        format!("~/.ssh/{}", parts.join("/"))
    }

    /// In-process, so the passphrase is never handed to another program. Returns the private key's path.
    pub fn generate(
        &self,
        name: &str,
        kind: KeyType,
        comment: &str,
        passphrase: Option<&str>,
    ) -> Result<PathBuf> {
        let algo = match kind {
            KeyType::Ed25519 => Algo::Ed25519,
            KeyType::Ecdsa => Algo::P256,
            KeyType::Rsa => Algo::Rsa4096,
        };
        let spec = Spec {
            usage: Usage::Ssh,
            format: Format::Openssh,
            name: name.into(),
            comment: comment.into(),
            passphrase: passphrase.map(str::to_owned),
            dir: None,
        };
        let generated = self.create(algo, &spec)?;
        Ok(PathBuf::from(
            generated.files[0]
                .path
                .as_ref()
                .expect("OpenSSH keys are written"),
        ))
    }

    pub fn agent_add(
        &self,
        path: &str,
        passphrase: Option<&str>,
        askpass: Option<&Path>,
    ) -> Result<()> {
        let (private, _) = self.pair(path)?;
        let private = private.context("the private key is missing")?;
        tools::agent_add(&private, passphrase, askpass)
    }

    pub fn agent_remove(&self, path: &str) -> Result<()> {
        // The public half is enough, and never asks for a passphrase.
        let (private, public) = self.pair(path)?;
        tools::agent_remove(&public.or(private).expect("resolve found a file"))
    }

    /// An empty title falls back to the key's comment, then its file name.
    pub fn github_upload(&self, path: &str, title: &str) -> Result<()> {
        let key = self.public_key(path)?;
        let title = [title, key.comment()]
            .into_iter()
            .find(|t| !t.trim().is_empty())
            .map_or_else(
                || file_name(&self.resolve(path).unwrap_or_default()),
                str::to_owned,
            );
        tools::github_upload(&key.to_openssh()?, &title)
    }

    fn host_edit(&self, entry: &HostEntry) -> Result<(String, String)> {
        let mut entry = entry.clone();
        if let Some(key) = &entry.identity_file {
            entry.identity_file = Some(self.portable(&self.resolve(key)?));
        }
        let before = match fs::read_to_string(self.config()) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e).context("cannot read ~/.ssh/config"),
        };
        let after = config::upsert(&before, &entry)?;
        Ok((before, after))
    }

    /// What `host_save` would change.
    pub fn host_preview(&self, entry: &HostEntry) -> Result<HostPreview> {
        let (before, after) = self.host_edit(entry)?;
        Ok(HostPreview {
            diff: config::diff(&before, &after),
            warnings: config::warnings(&after, entry, &self.dir, &self.home),
        })
    }

    pub fn host_save(&self, entry: &HostEntry) -> Result<()> {
        let (before, after) = self.host_edit(entry)?;
        if before != after {
            create_private_dir(&self.dir)?;
            config::write(&self.config(), &after)?;
        }
        Ok(())
    }

    /// `repo` must already be checked (see `known_repo`); `key: None` removes the assignment.
    pub fn assign(&self, repo: &Path, key: Option<&str>) -> Result<()> {
        let key = key.map(|k| self.pair(k)).transpose()?;
        let private = match key {
            Some((Some(private), _)) => Some(private),
            Some((None, _)) => bail!("the private key is missing"),
            None => None,
        };
        tools::assign(repo, private.as_deref())
    }

    /// Returns what changed. Symlinks are left alone: their target may be anyone's file.
    pub fn fix_permissions(&self) -> Result<Vec<String>> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut fixed = Vec::new();
            let mut fix = |path: &Path, too_open: u32, mode: u32| -> Result<()> {
                let Ok(meta) = fs::symlink_metadata(path) else {
                    return Ok(());
                };
                if !meta.file_type().is_symlink() && meta.permissions().mode() & too_open != 0 {
                    fs::set_permissions(path, fs::Permissions::from_mode(mode))
                        .with_context(|| format!("cannot change {}", path.display()))?;
                    fixed.push(path.to_string_lossy().into_owned());
                }
                Ok(())
            };
            // The folder itself may be a symlink to a dotfiles folder: that one is fixed.
            fix(&self.dir.canonicalize()?, 0o077, 0o700)?;
            fix(&self.config(), 0o022, 0o600)?;
            for key in self.scan().0 {
                if !key.orphan {
                    fix(Path::new(&key.path), 0o077, 0o600)?;
                }
                if let Some(public) = &key.public_path {
                    fix(Path::new(public), 0o022, 0o644)?;
                }
            }
            Ok(fixed)
        }
        // Windows protects ~/.ssh with the profile folder's ACLs.
        #[cfg(not(unix))]
        {
            Ok(Vec::new())
        }
    }

    /// Replaces a key: generates `name`, uploads it to GitHub when asked, then points the config hosts and
    /// the repositories using the old key at it. The old key stays until the user trashes it. Once the new
    /// key exists, failures are collected in the result instead of hiding what was done.
    #[allow(clippy::too_many_arguments)]
    pub fn rotate(
        &self,
        old: &str,
        name: &str,
        kind: KeyType,
        comment: &str,
        passphrase: Option<&str>,
        upload: bool,
        repos: &[PathBuf],
    ) -> Result<Rotation> {
        let (old_private, old_public) = self.pair(old)?;
        let old: Vec<PathBuf> = [old_private, old_public].into_iter().flatten().collect();
        let new = self.generate(name, kind, comment, passphrase)?;
        let mut rotation = Rotation {
            key: new.to_string_lossy().into_owned(),
            uploaded: false,
            hosts: Vec::new(),
            repos: Vec::new(),
            warning: None,
            errors: Vec::new(),
        };
        if upload {
            if let Err(e) = self.github_upload(&rotation.key, comment) {
                // Hosts pointing at a key the server refuses would stop working.
                rotation
                    .errors
                    .push(format!("upload to GitHub: {e:#}; nothing was repointed"));
                return Ok(rotation);
            }
            rotation.uploaded = true;
        }
        let portable = self.portable(&new);
        for file in config::files(&self.dir, &self.home) {
            let Ok(before) = fs::read_to_string(&file) else {
                continue;
            };
            let (after, hosts) = config::replace_identity(&before, &self.home, &old, &portable);
            if after == before {
                continue;
            }
            match config::write(&file, &after) {
                Ok(()) => rotation.hosts.extend(hosts),
                Err(e) => rotation.errors.push(format!("{e:#}")),
            }
        }
        for repo in repos {
            let uses_old = tools::local_ssh_command(repo).is_some_and(|c| {
                tools::identity_args(&c).iter().any(|f| {
                    let file = config::expand(f, &self.home);
                    old.iter().any(|o| same_file(&file, o))
                })
            });
            if !uses_old {
                continue;
            }
            match tools::assign(repo, Some(&new)) {
                Ok(()) => rotation.repos.push(repo.to_string_lossy().into_owned()),
                Err(e) => rotation.errors.push(format!("{}: {e:#}", repo.display())),
            }
        }
        if !upload && !(rotation.hosts.is_empty() && rotation.repos.is_empty()) {
            rotation.warning = Some(
                "no server knows the new key yet: add it to each one (GitHub, GitLab, authorized_keys) \
                 before relying on the hosts and repositories now using it"
                    .into(),
            );
        }
        Ok(rotation)
    }

    pub fn trash(&self, path: &str) -> Result<PathBuf> {
        let (private, public) = self.pair(path)?;
        let root = self.dir.join(TRASH);
        create_private_dir(&root)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        // A fresh folder per call, so a file trashed earlier can never be overwritten.
        let dir = (0..100)
            .map(|n| match n {
                0 => root.join(stamp.to_string()),
                n => root.join(format!("{stamp}-{n}")),
            })
            .find(|dir| fs::create_dir(dir).is_ok())
            .with_context(|| format!("cannot create a folder in {}", root.display()))?;
        let mut files: Vec<PathBuf> = [private, public].into_iter().flatten().collect();
        files.dedup();
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        for file in files {
            let to = dir.join(file_name(&file));
            if let Err(e) = fs::rename(&file, &to) {
                // Both halves go to the trash, or neither does.
                for (from, to) in moved.iter().rev() {
                    let _ = fs::rename(to, from);
                }
                return Err(e).with_context(|| format!("cannot move {}", file.display()));
            }
            moved.push((file, to));
        }
        Ok(dir)
    }
}

/// `repo` when it is one of the `known` repositories (canonical paths), so the UI cannot reconfigure
/// an arbitrary folder.
pub fn known_repo(repo: &str, known: &[PathBuf]) -> Result<PathBuf> {
    let canonical = Path::new(repo)
        .canonicalize()
        .with_context(|| format!("cannot open {repo}"))?;
    ensure!(
        known.contains(&canonical),
        "{repo} is not a known repository"
    );
    Ok(PathBuf::from(repo))
}

fn read_private(path: &Path) -> Result<Option<Private>> {
    if fs::metadata(path)?.len() > MAX_KEY_BYTES {
        return Ok(None);
    }
    let bytes = Zeroizing::new(fs::read(path)?);
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(None);
    };
    let Some(header) = text
        .lines()
        .next()
        .map(str::trim)
        .filter(|l| l.starts_with("-----BEGIN ") && l.ends_with("PRIVATE KEY-----"))
    else {
        return Ok(None);
    };
    if header == "-----BEGIN OPENSSH PRIVATE KEY-----" {
        let key = PrivateKey::from_openssh(text)?;
        return Ok(Some(Private {
            encrypted: key.is_encrypted(),
            public: Some(key.public_key().clone()),
            algorithm: "",
        }));
    }
    // PEM keys (PKCS#1, PKCS#8, SEC1): ssh-key only reads OpenSSH's format, so the details come from the .pub.
    let algorithm = if header.contains("RSA") {
        "ssh-rsa"
    } else if header.contains(" EC ") {
        "ecdsa"
    } else if header.contains("DSA") {
        "ssh-dss"
    } else {
        "unknown"
    };
    Ok(Some(Private {
        public: None,
        encrypted: header.contains("ENCRYPTED") || text.contains("Proc-Type: 4,ENCRYPTED"),
        algorithm,
    }))
}

fn key(path: &Path, private: Option<Private>, problems: &mut Vec<Problem>) -> Key {
    let orphan = private.is_none();
    let public_path = match orphan {
        true => Some(path.to_path_buf()),
        false => Some(with_pub(path)).filter(|p| p.is_file()),
    };
    let from_file = public_path
        .as_ref()
        .and_then(|p| match PublicKey::read_openssh_file(p) {
            Ok(key) => Some(key),
            Err(e) => {
                problems.push(Problem::new(p, e));
                None
            }
        });
    let comment = [
        from_file.as_ref(),
        private.as_ref().and_then(|p| p.public.as_ref()),
    ]
    .into_iter()
    .flatten()
    .map(PublicKey::comment)
    .find(|c| !c.is_empty())
    .unwrap_or_default()
    .to_owned();
    let mut public = private
        .as_ref()
        .and_then(|p| p.public.clone())
        .or(from_file);
    if let Some(public) = &mut public {
        public.set_comment(&comment);
    }
    let meta = fs::metadata(path).ok();
    #[cfg(unix)]
    let mode = meta.as_ref().map(|m| {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode() & 0o777
    });
    #[cfg(not(unix))]
    let mode = None;
    Key {
        path: path.to_string_lossy().into_owned(),
        name: file_name(path),
        public_path: public_path.map(|p| p.to_string_lossy().into_owned()),
        orphan,
        algorithm: match (&public, &private) {
            (Some(key), _) => key.algorithm().as_str().to_owned(),
            (None, Some(private)) => private.algorithm.to_owned(),
            (None, None) => "unknown".into(),
        },
        bits: public.as_ref().and_then(|k| bits(k.key_data())),
        fingerprint: public
            .as_ref()
            .map(|k| k.fingerprint(HashAlg::Sha256).to_string()),
        public_key: public.as_ref().and_then(|k| k.to_openssh().ok()),
        comment,
        encrypted: private.map(|p| p.encrypted),
        mode,
        too_open: !orphan
            && mode.is_some_and(|m| m & 0o077 != 0)
            && !fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()),
        modified: meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
        in_agent: false,
        hosts: Vec::new(),
        repos: Vec::new(),
    }
}

fn bits(key: &KeyData) -> Option<u32> {
    let size = |n: &ssh_key::Mpint| {
        n.as_positive_bytes()
            .map(|b| b.len() as u32 * 8 - b.first().map_or(0, |x| x.leading_zeros()))
    };
    if let Some(rsa) = key.rsa() {
        return size(&rsa.n);
    }
    if let Some(dsa) = key.dsa() {
        return size(&dsa.p);
    }
    if let Some(ecdsa) = key.ecdsa() {
        return Some(match ecdsa.curve() {
            EcdsaCurve::NistP256 => 256,
            EcdsaCurve::NistP384 => 384,
            EcdsaCurve::NistP521 => 521,
        });
    }
    (key.is_ed25519() || key.is_sk_ed25519() || key.is_sk_ecdsa_p256()).then_some(256)
}

fn with_pub(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".pub");
    name.into()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

fn create_private_dir(dir: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(dir)
        .with_context(|| format!("cannot create {}", dir.display()))
}

#[cfg(test)]
mod tests;
