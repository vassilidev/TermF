//! The vault (ARCHITECTURE §12.2): secrets by scope (dev, staging, prod), global or for a project, and references
//! to an external manager (`op://`, `bw://`, `doppler://`, `aws-sm://`, `vault://`) resolved by its CLI each time
//! they are used, never copied.
//!
//! `<data>/vault/vault.json` holds a random data key, wrapped by a key derived from the master password with
//! Argon2id, and the whole content sealed by the data key with XChaCha20-Poly1305: termf-sync's choices rather
//! than `age`. Once unlocked, a process keeps the data key in memory (never the content) until it locks: by
//! hand, after the idle delay, or once the machine slept. The prod scope asks for the password again, valid 5
//! minutes, like `sudo`. Every access goes to the hash-chained audit log ([`audit`]); no value ever goes to
//! `Debug`, a log or an error.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use termf_sync::KeyStore;
use termf_sync::crypto::{self, Kdf, Key};
use zeroize::{Zeroize, Zeroizing};

pub mod audit;
mod mask;
mod reference;
#[cfg(test)]
mod tests;

pub use audit::{Action, Actor, ActorKind, Entry, Event, Verification};
pub use mask::{Masked, Masker};
pub use reference::check as check_reference;

/// The keychain's service name for the vault's key.
pub const KEYCHAIN: &str = "dev.termf.vault";
pub const MIN_PASSWORD: usize = 8;
pub const DEFAULT_AUTO_LOCK_MINUTES: u32 = 10;
const DIR: &str = "vault";
const FILE: &str = "vault.json";
const FORMAT: u32 = 1;
const PROD_REAUTH: u64 = 5 * 60;
/// The wall clock ran this much more than the monotonic one, which stands still while the machine sleeps.
const SLEEP_GAP: u64 = 30;
const MAX_VALUE: usize = 64 << 10;
const DAMAGED: &str = "the vault file is damaged or was not written by TermF";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Dev,
    Staging,
    /// Each use asks for the master password again (valid 5 minutes).
    Prod,
}

impl Scope {
    pub fn parse(text: &str) -> Result<Self> {
        match text {
            "dev" => Ok(Self::Dev),
            "staging" => Ok(Self::Staging),
            "prod" => Ok(Self::Prod),
            _ => bail!("unknown scope {text} (dev, staging, prod)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Prod => "prod",
        }
    }
}

/// A secret is its name in a scope, global or for one project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecretId {
    pub name: String,
    pub scope: Scope,
    pub project: Option<String>,
}

impl SecretId {
    pub fn new(name: &str, scope: Scope, project: Option<&str>) -> Result<Self> {
        let id = Self {
            name: name.into(),
            scope,
            project: project.map(Into::into),
        };
        id.check()?;
        Ok(id)
    }

    fn check(&self) -> Result<()> {
        let word = |s: &str| {
            !s.is_empty()
                && s.len() <= 128
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
        };
        ensure!(
            word(&self.name)
                && self
                    .name
                    .starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'),
            "a secret's name is letters, digits, _ . and - (128 at most), starting with a letter or _"
        );
        if let Some(project) = &self.project {
            ensure!(
                word(project) && !project.starts_with(['.', '-']),
                "invalid project id"
            );
        }
        Ok(())
    }
}

impl fmt::Display for SecretId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let project = self.project.as_deref().unwrap_or("global");
        write!(f, "{} ({}, {project})", self.name, self.scope.as_str())
    }
}

/// A value from the vault: wiped from memory when dropped, never shown by `Debug`.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Secret(••••••)")
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

/// What the vault says about a secret, never its value.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretInfo {
    pub name: String,
    pub scope: Scope,
    pub project: Option<String>,
    pub description: String,
    pub created: u64,
    pub updated: u64,
    /// An external manager's URI, resolved when used: the vault has no value for it.
    pub reference: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub initialized: bool,
    pub unlocked: bool,
    /// The key is in the system keychain: unlocking on this machine needs no password.
    pub remembered: bool,
    pub auto_lock_minutes: u32,
    /// Until when (Unix seconds) the prod scope needs no password; `None`: its next use asks for it.
    pub prod_until: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Shown to the user, who asked for it: never for an agent.
    Reveal,
    /// Used by TermF on the user's or an agent's behalf (a connection, a recipe), never shown.
    Use,
}

/// The vault must be unlocked first.
#[derive(Debug)]
pub struct Locked;

impl fmt::Display for Locked {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("the vault is locked")
    }
}

impl std::error::Error for Locked {}

/// A prod secret: the master password again first ([`Vault::reauth`]), valid 5 minutes.
#[derive(Debug)]
pub struct NeedsPassword;

impl fmt::Display for NeedsPassword {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("the prod scope needs the master password again")
    }
}

impl std::error::Error for NeedsPassword {}

pub trait Clock: Send + Sync {
    /// Seconds since the Unix epoch: runs on while the machine sleeps.
    fn wall(&self) -> u64;
    /// Since a fixed instant: stands still while the machine sleeps (macOS, Linux).
    fn mono(&self) -> Duration;
}

pub struct SystemClock(Instant);

impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl Clock for SystemClock {
    fn wall(&self) -> u64 {
        now()
    }

    fn mono(&self) -> Duration {
        self.0.elapsed()
    }
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Clone, Copy)]
struct Stamp {
    wall: u64,
    mono: Duration,
}

impl Stamp {
    /// Seconds from `self` to `now`, a sleep in between included.
    fn elapsed(self, now: Stamp) -> u64 {
        let mono = now.mono.saturating_sub(self.mono).as_secs();
        now.wall.saturating_sub(self.wall).max(mono)
    }

    fn slept(self, now: Stamp) -> bool {
        let mono = now.mono.saturating_sub(self.mono).as_secs();
        now.wall.saturating_sub(self.wall) > mono + SLEEP_GAP
    }
}

/// No core dump of this process, which holds the vault's key once unlocked (Unix). Only the soft limit, so a
/// shell it starts can raise it again.
pub fn harden_process() {
    #[cfg(unix)]
    // SAFETY: reads then sets this process's own limit, with a valid `rlimit`.
    unsafe {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_CORE, &mut limit) == 0 {
            limit.rlim_cur = 0;
            libc::setrlimit(libc::RLIMIT_CORE, &limit);
        }
    }
}

/// The data key while unlocked: kept out of swap where the system allows it (Unix), wiped when dropped.
struct LockedKey(Box<[u8; 32]>);

impl LockedKey {
    fn new(key: &[u8; 32]) -> Self {
        let mut boxed = Box::new([0u8; 32]);
        #[cfg(unix)]
        // SAFETY: the box owns these 32 bytes until `drop`, which unlocks them; a failure only means no lock.
        unsafe {
            libc::mlock(boxed.as_ptr().cast(), boxed.len());
        }
        boxed.copy_from_slice(key);
        Self(boxed)
    }

    fn key(&self) -> Key {
        Zeroizing::new(*self.0)
    }
}

impl Drop for LockedKey {
    fn drop(&mut self) {
        self.0.zeroize();
        #[cfg(unix)]
        // SAFETY: the same bytes `new` locked, still owned by the box.
        unsafe {
            libc::munlock(self.0.as_ptr().cast(), self.0.len());
        }
    }
}

struct Session {
    id: String,
    key: LockedKey,
    last: Stamp,
    /// When the master password was last typed, for the prod scope.
    prod: Option<Stamp>,
}

/// `vault.json`: nothing in it is readable without the master password, or the key kept in the keychain.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultFile {
    format: u32,
    /// Names the vault in the keychain.
    id: String,
    kdf: Kdf,
    salt: String,
    /// The data key, sealed by the password's key.
    key: String,
    /// The password's key is in the system keychain.
    #[serde(default)]
    remembered: bool,
    /// The content, sealed by the data key.
    data: String,
}

impl VaultFile {
    fn derive(&self, password: &str) -> Result<Key> {
        let salt = crypto::unhex(&self.salt).context(DAMAGED)?;
        crypto::derive(password.as_bytes(), &salt, self.kdf)
    }

    fn unwrap(&self, kek: &Key) -> Option<Key> {
        let sealed = crypto::unhex(&self.key)?;
        let plain = Zeroizing::new(crypto::open(kek, &aad("key", &self.id), &sealed)?);
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(plain.get(..32).filter(|_| plain.len() == 32)?);
        Some(key)
    }

    fn open(&self, key: &Key) -> Result<Content> {
        let sealed = crypto::unhex(&self.data).context(DAMAGED)?;
        let plain = Zeroizing::new(
            crypto::open(key, &aad("data", &self.id), &sealed).ok_or_else(|| anyhow!(DAMAGED))?,
        );
        // Not the parser's message: it may quote the content.
        serde_json::from_slice(&plain).map_err(|_| anyhow!(DAMAGED))
    }

    fn seal(&mut self, key: &Key, content: &Content) -> Result<()> {
        let plain = Zeroizing::new(serde_json::to_vec(content)?);
        self.data = crypto::hex(&crypto::seal(key, &aad("data", &self.id), &plain));
        Ok(())
    }
}

fn aad(what: &str, id: &str) -> Vec<u8> {
    format!("termf-vault {what} {id}").into_bytes()
}

fn kdf() -> Kdf {
    // The parameters are stored in the file, so tests can derive their many keys faster.
    if cfg!(test) {
        Kdf {
            m: 8 << 10,
            t: 1,
            p: 1,
        }
    } else {
        Kdf::default()
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Content {
    secrets: Vec<Stored>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    name: String,
    scope: Scope,
    project: Option<String>,
    #[serde(default)]
    description: String,
    created: u64,
    updated: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<Secret>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
}

impl Stored {
    fn is(&self, id: &SecretId) -> bool {
        self.name == id.name && self.scope == id.scope && self.project == id.project
    }

    fn info(&self) -> SecretInfo {
        SecretInfo {
            name: self.name.clone(),
            scope: self.scope,
            project: self.project.clone(),
            description: self.description.clone(),
            created: self.created,
            updated: self.updated,
            reference: self.reference.clone(),
        }
    }
}

pub struct Vault {
    dir: PathBuf,
    data_dir: PathBuf,
    store: Box<dyn KeyStore>,
    clock: Box<dyn Clock>,
    /// `PATH` for the external managers' CLIs; `None`: this process's.
    path: Option<OsString>,
    auto_lock: AtomicU64,
    session: Mutex<Option<Session>>,
}

impl Vault {
    /// `store`: `termf_sync::OsKeyStore(KEYCHAIN)`, the system keychain.
    pub fn new(data_dir: &Path, store: Box<dyn KeyStore>, clock: Box<dyn Clock>) -> Self {
        Self {
            dir: data_dir.join(DIR),
            data_dir: data_dir.into(),
            store,
            clock,
            path: None,
            auto_lock: AtomicU64::new(u64::from(DEFAULT_AUTO_LOCK_MINUTES) * 60),
            session: Mutex::new(None),
        }
    }

    pub fn with_path(mut self, path: Option<OsString>) -> Self {
        self.path = path;
        self
    }

    fn now(&self) -> Stamp {
        Stamp {
            wall: self.clock.wall(),
            mono: self.clock.mono(),
        }
    }

    pub fn initialized(&self) -> bool {
        self.dir.join(FILE).is_file()
    }

    fn read(&self) -> Result<VaultFile> {
        let bytes = match fs::read(self.dir.join(FILE)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("there is no vault yet (termf vault init)")
            }
            other => other?,
        };
        let file: VaultFile = serde_json::from_slice(&bytes).map_err(|_| anyhow!(DAMAGED))?;
        ensure!(
            file.format == FORMAT,
            "this vault was written by a newer TermF"
        );
        Ok(file)
    }

    /// Through a temporary file readable only by the user, renamed over the vault.
    fn write(&self, file: &VaultFile) -> Result<()> {
        let tmp = self.dir.join(format!(".{FILE}.tmp"));
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut out = options.open(&tmp)?;
        #[cfg(unix)]
        out.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        out.write_all(&serde_json::to_vec_pretty(file)?)?;
        out.sync_all()?;
        fs::rename(&tmp, self.dir.join(FILE)).context("cannot write the vault")
    }

    /// One writer at a time, across the app and the CLI.
    fn exclusive(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.dir)?;
        #[cfg(unix)]
        fs::set_permissions(
            &self.dir,
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join("lock"))?;
        file.lock()?;
        Ok(file)
    }

    pub fn record(&self, event: Event) -> Result<Entry> {
        audit::Log::open(&self.data_dir)?.append(self.clock.wall(), event)
    }

    pub fn audit(&self, limit: usize) -> Result<Vec<Entry>> {
        audit::Log::open(&self.data_dir)?.list(limit)
    }

    pub fn verify(&self) -> Result<Verification> {
        audit::Log::open(&self.data_dir)?.verify()
    }

    /// The whole log as JSON lines, oldest first; returns how many.
    pub fn export(&self, out: &mut dyn Write) -> Result<u64> {
        audit::Log::open(&self.data_dir)?.export(out)
    }

    pub fn set_auto_lock(&self, minutes: u32) -> Result<()> {
        ensure!(
            (1..=24 * 60).contains(&minutes),
            "the vault locks after 1 minute to 24 hours of inactivity"
        );
        self.auto_lock
            .store(u64::from(minutes) * 60, Ordering::Relaxed);
        Ok(())
    }

    pub fn status(&self) -> Result<Status> {
        self.check();
        let remembered = self.initialized() && self.read()?.remembered;
        let now = self.now();
        let session = self.session.lock().unwrap();
        let prod_until = session.as_ref().and_then(|s| {
            let prod = s.prod.filter(|p| !p.slept(now))?;
            let left = PROD_REAUTH
                .checked_sub(prod.elapsed(now))
                .filter(|l| *l > 0)?;
            Some(now.wall + left)
        });
        Ok(Status {
            initialized: self.initialized(),
            unlocked: session.is_some(),
            remembered,
            auto_lock_minutes: (self.auto_lock.load(Ordering::Relaxed) / 60) as u32,
            prod_until,
        })
    }

    /// Locks once the idle delay passed or the machine slept; true when it did just now.
    pub fn check(&self) -> bool {
        let now = self.now();
        let mut session = self.session.lock().unwrap();
        let reason = match session.as_ref() {
            Some(s) if s.last.slept(now) => "sleep",
            Some(s) if s.last.elapsed(now) >= self.auto_lock.load(Ordering::Relaxed) => "idle",
            _ => return false,
        };
        *session = None;
        drop(session);
        let _ = self.record(Event {
            detail: Some(reason.into()),
            ..Event::new(&Actor::termf(), Action::Lock)
        });
        true
    }

    fn start(&self, id: &str, key: &Key, typed: bool) {
        let now = self.now();
        *self.session.lock().unwrap() = Some(Session {
            id: id.into(),
            key: LockedKey::new(key),
            last: now,
            prod: typed.then_some(now),
        });
    }

    /// The data key, unless locked: counts as activity.
    fn key(&self) -> Result<(String, Key)> {
        self.check();
        let now = self.now();
        let mut session = self.session.lock().unwrap();
        let session = session.as_mut().ok_or(Locked)?;
        session.last = now;
        Ok((session.id.clone(), session.key.key()))
    }

    /// Creates the vault and unlocks it. `remember`: keep the password's key in the system keychain.
    pub fn init(&self, actor: &Actor, password: &str, remember: bool) -> Result<Status> {
        ensure!(
            password.chars().count() >= MIN_PASSWORD,
            "the master password needs {MIN_PASSWORD} characters or more"
        );
        let _lock = self.exclusive()?;
        ensure!(!self.initialized(), "there is already a vault");
        let data_key: Key = Zeroizing::new(crypto::random());
        let id = crypto::hex(&crypto::random::<16>());
        let salt: [u8; 16] = crypto::random();
        let kdf = kdf();
        let kek = crypto::derive(password.as_bytes(), &salt, kdf)?;
        let mut file = VaultFile {
            format: FORMAT,
            key: crypto::hex(&crypto::seal(&kek, &aad("key", &id), data_key.as_ref())),
            id,
            kdf,
            salt: crypto::hex(&salt),
            remembered: false,
            data: String::new(),
        };
        file.seal(&data_key, &Content::default())?;
        file.remembered = remember && self.store.set(&file.id, kek.as_ref()).is_ok();
        self.write(&file)?;
        self.start(&file.id, &data_key, true);
        self.record(Event::new(actor, Action::Init))?;
        self.status()
    }

    /// With the master password, or without it from the system keychain when it keeps the key. `remember`:
    /// keep the key there from now on, or forget it; `None` leaves it as it is.
    pub fn unlock(
        &self,
        actor: &Actor,
        password: Option<&str>,
        remember: Option<bool>,
    ) -> Result<Status> {
        let file = self.read()?;
        let kek = match password {
            Some(password) => file.derive(password)?,
            None => {
                let kept = file
                    .remembered
                    .then(|| self.store.get(&file.id).ok().flatten())
                    .flatten()
                    .and_then(|k| <[u8; 32]>::try_from(k).ok());
                Zeroizing::new(kept.context("the master password is needed")?)
            }
        };
        let Some(data_key) = file.unwrap(&kek) else {
            let _ = self.record(Event::new(actor, Action::UnlockFailed));
            if password.is_none() {
                bail!(
                    "the keychain's key no longer opens the vault: the master password is needed"
                );
            }
            bail!("wrong master password");
        };
        file.open(&data_key)?;
        if let Some(remember) = remember {
            let _lock = self.exclusive()?;
            let mut file = self.read()?;
            if remember {
                file.remembered = self.store.set(&file.id, kek.as_ref()).is_ok();
            } else {
                file.remembered = false;
                let _ = self.store.delete(&file.id);
            }
            self.write(&file)?;
        }
        self.start(&file.id, &data_key, password.is_some());
        self.record(Event {
            detail: password.is_none().then(|| "keychain".into()),
            ..Event::new(actor, Action::Unlock)
        })?;
        self.status()
    }

    pub fn lock(&self, actor: &Actor) -> Result<Status> {
        if self.session.lock().unwrap().take().is_some() {
            self.record(Event::new(actor, Action::Lock))?;
        }
        self.status()
    }

    /// The master password again: the prod scope needs no other for 5 minutes.
    pub fn reauth(&self, actor: &Actor, password: &str) -> Result<Status> {
        let (id, _) = self.key()?;
        let file = self.read()?;
        ensure!(file.id == id, Locked);
        let typed = file
            .derive(password)
            .ok()
            .and_then(|kek| file.unwrap(&kek))
            .is_some();
        if !typed {
            let _ = self.record(Event::new(actor, Action::ReauthFailed));
            bail!("wrong master password");
        }
        if let Some(session) = self.session.lock().unwrap().as_mut() {
            session.prod = Some(self.now());
        }
        self.record(Event::new(actor, Action::Reauth))?;
        self.status()
    }

    /// A prod secret needs the master password typed in the last 5 minutes, without a sleep since.
    fn prod_gate(&self, actor: &Actor, id: &SecretId) -> Result<()> {
        if id.scope != Scope::Prod {
            return Ok(());
        }
        let now = self.now();
        let fresh = match self.session.lock().unwrap().as_ref() {
            None => bail!(Locked),
            Some(s) => s
                .prod
                .is_some_and(|p| !p.slept(now) && p.elapsed(now) < PROD_REAUTH),
        };
        if !fresh {
            let _ = self.record(Event {
                target: Some(id.clone()),
                detail: Some(NeedsPassword.to_string()),
                ..Event::new(actor, Action::Refused)
            });
            bail!(NeedsPassword);
        }
        Ok(())
    }

    fn open(&self) -> Result<Content> {
        let (id, key) = self.key()?;
        let file = self.read()?;
        ensure!(file.id == id, Locked);
        file.open(&key)
    }

    fn modify<T>(&self, change: impl FnOnce(&mut Content) -> Result<T>) -> Result<T> {
        let (id, key) = self.key()?;
        let _lock = self.exclusive()?;
        let mut file = self.read()?;
        ensure!(file.id == id, Locked);
        let mut content = file.open(&key)?;
        let out = change(&mut content)?;
        file.seal(&key, &content)?;
        self.write(&file)?;
        Ok(out)
    }

    /// Every secret, or those `project` sees (its own and the global ones), sorted.
    pub fn list(&self, scope: Option<Scope>, project: Option<&str>) -> Result<Vec<SecretInfo>> {
        let mut list: Vec<SecretInfo> = self
            .open()?
            .secrets
            .iter()
            .filter(|s| scope.is_none_or(|scope| s.scope == scope))
            .filter(|s| project.is_none() || s.project.is_none() || s.project.as_deref() == project)
            .map(Stored::info)
            .collect();
        list.sort_by(|a, b| (&a.project, a.scope, &a.name).cmp(&(&b.project, b.scope, &b.name)));
        Ok(list)
    }

    /// Creates or replaces a value. `description: None` keeps the one it has.
    pub fn set(
        &self,
        actor: &Actor,
        id: &SecretId,
        value: Secret,
        description: Option<&str>,
    ) -> Result<SecretInfo> {
        id.check()?;
        ensure!(!value.is_empty(), "the value is empty");
        ensure!(value.len() <= MAX_VALUE, "the value is longer than 64 KiB");
        self.prod_gate(actor, id)?;
        let size = value.len() as u64;
        let info = self.put(id, description, Some(value), None)?;
        self.record(Event {
            target: Some(id.clone()),
            size: Some(size),
            ..Event::new(actor, Action::Write)
        })?;
        Ok(info)
    }

    /// Creates or replaces a reference to an external manager, checked but not resolved.
    pub fn set_reference(
        &self,
        actor: &Actor,
        id: &SecretId,
        uri: &str,
        description: Option<&str>,
    ) -> Result<SecretInfo> {
        id.check()?;
        reference::check(uri)?;
        self.prod_gate(actor, id)?;
        let info = self.put(id, description, None, Some(uri.into()))?;
        self.record(Event {
            target: Some(id.clone()),
            detail: Some(uri.into()),
            ..Event::new(actor, Action::Reference)
        })?;
        Ok(info)
    }

    fn put(
        &self,
        id: &SecretId,
        description: Option<&str>,
        value: Option<Secret>,
        reference: Option<String>,
    ) -> Result<SecretInfo> {
        let now = self.clock.wall();
        self.modify(|content| {
            let i = match content.secrets.iter().position(|s| s.is(id)) {
                Some(i) => i,
                None => {
                    content.secrets.push(Stored {
                        name: id.name.clone(),
                        scope: id.scope,
                        project: id.project.clone(),
                        description: String::new(),
                        created: now,
                        updated: now,
                        value: None,
                        reference: None,
                    });
                    content.secrets.len() - 1
                }
            };
            let stored = &mut content.secrets[i];
            if let Some(description) = description {
                stored.description = description.trim().into();
            }
            stored.updated = now;
            stored.value = value;
            stored.reference = reference;
            Ok(stored.info())
        })
    }

    pub fn delete(&self, actor: &Actor, id: &SecretId) -> Result<()> {
        self.prod_gate(actor, id)?;
        self.modify(|content| {
            let before = content.secrets.len();
            content.secrets.retain(|s| !s.is(id));
            ensure!(content.secrets.len() < before, "no secret {id}");
            Ok(())
        })?;
        self.record(Event {
            target: Some(id.clone()),
            ..Event::new(actor, Action::Delete)
        })?;
        Ok(())
    }

    /// A secret's value, or what its reference resolves to. Audited before it is returned: no entry, no value.
    pub fn value(&self, actor: &Actor, id: &SecretId, access: Access) -> Result<Secret> {
        if access == Access::Reveal && actor.kind != ActorKind::Human {
            let _ = self.record(Event {
                target: Some(id.clone()),
                detail: Some("only the user reveals a secret".into()),
                ..Event::new(actor, Action::Refused)
            });
            bail!("only the user reveals a secret");
        }
        self.prod_gate(actor, id)?;
        let stored = self
            .open()?
            .secrets
            .into_iter()
            .find(|s| s.is(id))
            .with_context(|| format!("no secret {id}"))?;
        let use_ = match access {
            Access::Reveal => "reveal",
            Access::Use => "use",
        };
        let (secret, action, detail) = match (stored.value, &stored.reference) {
            (Some(value), _) if access == Access::Reveal => (value, Action::Reveal, None),
            (Some(value), _) => (value, Action::Read, None),
            (None, Some(uri)) => (
                reference::resolve(uri, self.path.as_deref().map(OsStr::new))?,
                Action::Resolve,
                Some(format!("{use_} {uri}")),
            ),
            (None, None) => bail!(DAMAGED),
        };
        self.record(Event {
            target: Some(id.clone()),
            size: Some(secret.len() as u64),
            detail,
            ..Event::new(actor, action)
        })?;
        Ok(secret)
    }

    /// Masks every value the vault holds (not the references, unknown until resolved: [`Masker::add`]).
    pub fn masker(&self) -> Result<Masker> {
        Ok(Masker::new(
            self.open()?.secrets.iter().filter_map(|s| s.value.as_ref()),
        ))
    }
}
