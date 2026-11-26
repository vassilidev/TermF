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
//!
//! A second factor: once security keys are enrolled ([`fido`]), the data key is sealed by the password's key
//! mixed with a random factor secret, which each key's `hmac-secret` seals in turn: the password alone opens
//! nothing. A recovery key seals the data key (and the factor secret) on its own, for a lost key or password.
//! Touch ID or Windows Hello reopen it on one machine for some days from a key kept in the system keychain,
//! read once the system said yes: a gate in front of the keychain, not a cryptographic bond (an unsigned app
//! cannot ask the keychain to require it).
//!
//! The sync carries the sealed file as it is and leaves the other machines' version beside it; the vault merges
//! it item by item when it opens ([`merge`], [`synced`]). An export is the same content sealed by its own
//! password ([`transfer`]); files exported by other managers come in through [`foreign`].

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use termf_sync::KeyStore;
use termf_sync::crypto::{self, Kdf, Key};
use zeroize::Zeroize;
pub use zeroize::Zeroizing;

pub mod audit;
pub mod fido;
mod foreign;
mod generate;
mod item;
mod mask;
mod merge;
mod reference;
mod synced;
#[cfg(test)]
mod tests;
mod totp;
mod transfer;
mod zip;

pub use audit::{Action, Actor, ActorKind, Entry, Event, Verification};
pub use fido::SecurityKeys;
pub use generate::{Recipe, generate};
pub use item::{Field, ItemDraft, ItemInfo, Kind, TotpInfo};
pub use mask::{Masked, Masker};
pub use merge::{Change, Merged, summary};
pub use reference::check as check_reference;
pub use synced::{ConflictInfo, Synced};
pub use totp::{Algorithm, Totp};
pub use transfer::{ExportWrite, ForeignChoice, ForeignPreview, ForeignRow, digest, is_export};

use item::Item;
use merge::{Conflicted, Tombstone, new_rev};

/// The keychain's service name for the vault's key.
pub const KEYCHAIN: &str = "dev.termf.vault";
pub const MIN_PASSWORD: usize = 8;
pub const DEFAULT_AUTO_LOCK_MINUTES: u32 = 10;
const DIR: &str = "vault";
const FILE: &str = "vault.json";
/// This machine's Touch ID / Windows Hello slot, never synced: the key that opens it is in this keychain.
const QUICK: &str = "quick.json";
/// What only this machine knows of the vault (its keychain keeps the password's key), never synced: the synced
/// file stays the same on every machine.
const MACHINE: &str = "machine.json";
/// 2: the password manager's items. 3: security keys and the recovery key. 4: each element's history, the
/// tombstones and the conflicts of the item-by-item merge, and when the way it opens last changed. A TermF that
/// knows an older format would drop what it does not know when it writes, so it refuses a newer one.
const FORMAT: u32 = 4;
const PROD_REAUTH: u64 = 5 * 60;
pub const MAX_KEYS: usize = 10;
/// Touch ID or Windows Hello reopens the vault for this many days after the password, at most.
pub const QUICK_DAYS: std::ops::RangeInclusive<u32> = 1..=30;
pub const DEFAULT_QUICK_DAYS: u32 = 7;
/// The wall clock ran this much more than the monotonic one, which stands still while the machine sleeps.
const SLEEP_GAP: u64 = 30;
pub(crate) const MAX_VALUE: usize = 64 << 10;
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
        ensure!(
            word(&self.name)
                && self
                    .name
                    .starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'),
            "a secret's name is letters, digits, _ . and - (128 at most), starting with a letter or _"
        );
        if let Some(project) = &self.project {
            ensure!(valid_project(project), "invalid project id");
        }
        Ok(())
    }
}

fn word(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
}

pub(crate) fn valid_project(project: &str) -> bool {
    word(project) && !project.starts_with(['.', '-'])
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
    /// Other versions a merge kept beside it, for the user to choose from.
    pub conflicts: usize,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub initialized: bool,
    pub unlocked: bool,
    /// The key is in the system keychain: unlocking on this machine needs no password.
    pub remembered: bool,
    pub auto_lock_minutes: u32,
    /// It locks as soon as the user leaves its view ([`Vault::leave`]).
    pub lock_on_leave: bool,
    /// Until when (Unix seconds) the prod scope needs no password; `None`: its next use asks for it.
    pub prod_until: Option<u64>,
    /// Security keys enrolled: with any, the master password alone no longer opens the vault.
    pub keys: usize,
    /// A recovery key opens the vault without the password nor a security key.
    pub recovery: bool,
    /// Touch ID or Windows Hello reopens the vault on this machine, for this many days after the password;
    /// `None`: off.
    pub quick_days: Option<u32>,
    /// Until when (Unix seconds) it does; `None`: the master password first.
    pub quick_until: Option<u64>,
    /// Until when (Unix seconds) the vault's security can change without the master password again.
    pub confirmed_until: Option<u64>,
    /// Another machine's version, brought by the sync, waits here.
    pub synced: Option<Synced>,
}

/// A security key enrolled as the vault's second factor.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    pub id: String,
    pub name: String,
    pub created: u64,
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

/// Changing how the vault opens: the master password again first ([`Vault::reauth`]), valid 5 minutes.
#[derive(Debug)]
pub struct Unconfirmed;

impl fmt::Display for Unconfirmed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("this change needs the master password again")
    }
}

impl std::error::Error for Unconfirmed {}

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

pub(crate) fn hmac256(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    let mut mac =
        <Hmac<sha2::Sha256> as Mac>::new_from_slice(key).expect("HMAC takes any key size");
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

/// What unlocking yields: the data key, the password's key and, with security keys, the factor secret. Kept
/// whole so the vault's security can change without asking for each part again.
#[derive(Clone)]
struct Material {
    data: Key,
    kek: Key,
    factor: Option<Key>,
}

impl Material {
    /// What seals the data key in the file: the password's key, mixed with the factor secret when there is one.
    fn wrapping(&self) -> Key {
        match &self.factor {
            None => self.kek.clone(),
            Some(factor) => Zeroizing::new(hmac256(
                self.kek.as_ref(),
                &[b"termf-vault factor", factor.as_ref()],
            )),
        }
    }

    fn bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new([self.data.as_ref(), self.kek.as_ref()].concat());
        if let Some(factor) = &self.factor {
            bytes.extend_from_slice(factor.as_ref());
        }
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let part = |i: usize| -> Option<Key> {
            let mut key = Zeroizing::new([0u8; 32]);
            key.copy_from_slice(bytes.get(i * 32..i * 32 + 32)?);
            Some(key)
        };
        match bytes.len() {
            64 | 96 => Some(Self {
                data: part(0)?,
                kek: part(1)?,
                factor: part(2),
            }),
            _ => None,
        }
    }
}

/// The keys while unlocked: kept out of swap where the system allows it (Unix), wiped when dropped. One
/// allocation, as unlocking one page in `drop` would unlock it for whatever else lives there.
struct LockedKey {
    bytes: Box<[u8; 96]>,
    factor: bool,
}

impl LockedKey {
    fn new(material: &Material) -> Self {
        let mut boxed = Box::new([0u8; 96]);
        #[cfg(unix)]
        // SAFETY: the box owns these bytes until `drop`, which unlocks them; a failure only means no lock.
        unsafe {
            libc::mlock(boxed.as_ptr().cast(), boxed.len());
        }
        let bytes = material.bytes();
        boxed[..bytes.len()].copy_from_slice(&bytes);
        Self {
            bytes: boxed,
            factor: material.factor.is_some(),
        }
    }

    fn material(&self) -> Material {
        let len = if self.factor { 96 } else { 64 };
        Material::from_bytes(&self.bytes[..len]).expect("64 or 96 bytes")
    }
}

impl Drop for LockedKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
        #[cfg(unix)]
        // SAFETY: the same bytes `new` locked, still owned by the box.
        unsafe {
            libc::munlock(self.bytes.as_ptr().cast(), self.bytes.len());
        }
    }
}

struct Session {
    id: String,
    keys: LockedKey,
    last: Stamp,
    /// When the user last proved it for the prod scope: the master password, a security key, Touch ID.
    prod: Option<Stamp>,
    /// When the master password was last typed, for changes to how the vault opens.
    typed: Option<Stamp>,
}

fn fresh(stamp: Option<Stamp>, now: Stamp) -> Option<u64> {
    let stamp = stamp.filter(|s| !s.slept(now))?;
    let left = PROD_REAUTH
        .checked_sub(stamp.elapsed(now))
        .filter(|l| *l > 0)?;
    Some(now.wall + left)
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
    /// The password's key is in this machine's keychain: kept in `machine.json`, read here from older files.
    #[serde(default, skip_serializing)]
    remembered: bool,
    /// The content, sealed by the data key.
    data: String,
    /// Security keys: `key` is then sealed by the password's key mixed with the factor secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    factor: Option<Factor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery: Option<Recovery>,
    /// When the way it opens last changed (password, security keys, recovery key): the newest wins in a merge.
    #[serde(default)]
    secured: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Factor {
    /// The PRF input every key's `hmac-secret` is read with.
    salt: String,
    keys: Vec<EnrolledKey>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct EnrolledKey {
    id: String,
    name: String,
    /// The credential the key made for the vault.
    credential: String,
    /// The factor secret, sealed by this key's `hmac-secret`.
    secret: String,
    created: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Recovery {
    kdf: Kdf,
    salt: String,
    /// The data key, and the factor secret when there was one, sealed by the recovery key's key.
    key: String,
    created: u64,
}

/// `machine.json`.
#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Machine {
    vault: String,
    remembered: bool,
}

/// `quick.json`: the session's keys, sealed by a key in this machine's keychain, until `until`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Quick {
    vault: String,
    days: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    until: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

fn key32(bytes: &[u8]) -> Option<Key> {
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(bytes.get(..32).filter(|_| bytes.len() == 32)?);
    Some(key)
}

fn quick_aad(id: &str, until: u64) -> Vec<u8> {
    aad("quick", &format!("{id} {until}"))
}

fn quick_account(id: &str) -> String {
    format!("{id}.quick")
}

impl VaultFile {
    fn derive(&self, password: &str) -> Result<Key> {
        let salt = crypto::unhex(&self.salt).context(DAMAGED)?;
        crypto::derive(password.as_bytes(), &salt, self.kdf)
    }

    fn unwrap(&self, wrapping: &Key) -> Option<Key> {
        let sealed = crypto::unhex(&self.key)?;
        let plain = Zeroizing::new(crypto::open(wrapping, &aad("key", &self.id), &sealed)?);
        key32(&plain)
    }

    fn seal_key(&mut self, material: &Material) {
        let sealed = crypto::seal(
            &material.wrapping(),
            &aad("key", &self.id),
            material.data.as_ref(),
        );
        self.key = crypto::hex(&sealed);
    }

    /// A new password: its own salt and today's derivation parameters.
    fn set_password(&mut self, password: &str, material: &mut Material) -> Result<()> {
        let salt: [u8; 16] = crypto::random();
        self.kdf = kdf();
        self.salt = crypto::hex(&salt);
        material.kek = crypto::derive(password.as_bytes(), &salt, self.kdf)?;
        self.seal_key(material);
        Ok(())
    }

    fn credentials(&self) -> Result<Vec<Vec<u8>>> {
        self.factor
            .iter()
            .flat_map(|f| &f.keys)
            .map(|k| crypto::unhex(&k.credential).context(DAMAGED))
            .collect()
    }

    fn factor_salt(&self) -> Result<[u8; 32]> {
        let salt = self
            .factor
            .as_ref()
            .map(|f| f.salt.as_str())
            .unwrap_or_default();
        crypto::unhex(salt)
            .and_then(|s| s.try_into().ok())
            .context(DAMAGED)
    }

    /// A new recovery key, which replaces the one before: returned to be shown once.
    fn seal_recovery(&mut self, material: &Material, now: u64) -> Result<Secret> {
        let text = crypto::recovery_key();
        let salt: [u8; 16] = crypto::random();
        let kdf = kdf();
        let key = crypto::derive(crypto::normalize_recovery_key(&text).as_bytes(), &salt, kdf)?;
        let mut plain = Zeroizing::new(material.data.to_vec());
        if let Some(factor) = &material.factor {
            plain.extend_from_slice(factor.as_ref());
        }
        self.recovery = Some(Recovery {
            kdf,
            salt: crypto::hex(&salt),
            key: crypto::hex(&crypto::seal(&key, &aad("recovery", &self.id), &plain)),
            created: now,
        });
        Ok(Secret::new(text))
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

fn factor_aad(id: &str, key: &str) -> Vec<u8> {
    aad("factor", &format!("{id} {key}"))
}

fn check_password(password: &str) -> Result<()> {
    ensure!(
        password.chars().count() >= MIN_PASSWORD,
        "the master password needs {MIN_PASSWORD} characters or more"
    );
    Ok(())
}

fn security(actor: &Actor, detail: String) -> Event {
    Event {
        detail: Some(detail),
        ..Event::new(actor, Action::Security)
    }
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

#[derive(Serialize, Deserialize, Default, Clone)]
struct Content {
    secrets: Vec<Stored>,
    #[serde(default)]
    items: Vec<Item>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tombstones: Vec<Tombstone>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    conflicts: Vec<Conflicted>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Stored {
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
    /// Its versions, newest last: see [`merge`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    revs: Vec<String>,
}

/// `secret:<scope>:<project>:<name>`, as the interface names it.
fn secret_key(name: &str, scope: Scope, project: Option<&str>) -> String {
    format!(
        "secret:{}:{}:{name}",
        scope.as_str(),
        project.unwrap_or_default()
    )
}

impl Content {
    fn conflicts_of(&self, key: &str) -> usize {
        self.conflicts.iter().filter(|c| c.key() == key).count()
    }
}

impl Stored {
    fn is(&self, id: &SecretId) -> bool {
        self.name == id.name && self.scope == id.scope && self.project == id.project
    }

    pub(crate) fn key(&self) -> String {
        secret_key(&self.name, self.scope, self.project.as_deref())
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
            conflicts: 0,
        }
    }
}

pub struct Vault {
    dir: PathBuf,
    data_dir: PathBuf,
    store: Box<dyn KeyStore>,
    clock: Box<dyn Clock>,
    /// `PATH` for the external managers' CLIs, which keep this process's environment; `None`: the login shell's
    /// `PATH` and the variables each manager needs, which an app opened from the Finder or a desktop menu lacks.
    path: Option<OsString>,
    auto_lock: AtomicU64,
    lock_on_leave: AtomicBool,
    session: Mutex<Option<Session>>,
    on_lock: Option<Told>,
    keys: Box<dyn SecurityKeys>,
    /// This process keeps the Touch ID / Windows Hello slot fresh: the app only, since the keychain lets the
    /// process that wrote an item read it without asking, and the app could not read one the CLI wrote.
    quick: bool,
}

type Told = Box<dyn Fn(&Status) + Send + Sync>;

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
            lock_on_leave: AtomicBool::new(true),
            session: Mutex::new(None),
            on_lock: None,
            keys: Box::new(fido::Unavailable),
            quick: false,
        }
    }

    /// The app: an opening by password reopens Touch ID or Windows Hello for its days.
    pub fn with_quick(mut self) -> Self {
        self.quick = true;
        self
    }

    /// `keys`: [`fido::system`], the security keys this machine reaches.
    pub fn with_keys(mut self, keys: Box<dyn SecurityKeys>) -> Self {
        self.keys = keys;
        self
    }

    /// TermF asks for a security key's PIN itself (macOS, Linux); Windows asks in its own window.
    pub fn asks_pin(&self) -> bool {
        self.keys.asks_pin()
    }

    pub fn with_path(mut self, path: Option<OsString>) -> Self {
        self.path = path;
        self
    }

    /// Told when the vault locks itself (idle, sleep), whatever call noticed it first.
    pub fn on_lock(mut self, told: impl Fn(&Status) + Send + Sync + 'static) -> Self {
        self.on_lock = Some(Box::new(told));
        self
    }

    fn now(&self) -> Stamp {
        Stamp {
            wall: self.clock.wall(),
            mono: self.clock.mono(),
        }
    }

    /// A vault here, or one the sync brought from another machine.
    pub fn initialized(&self) -> bool {
        self.dir.join(FILE).is_file() || self.dir.join(synced::INCOMING).is_file()
    }

    fn read(&self) -> Result<VaultFile> {
        let bytes = match fs::read(self.dir.join(FILE)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("there is no vault yet (termf vault init)")
            }
            other => other?,
        };
        let mut file: VaultFile = serde_json::from_slice(&bytes).map_err(|_| anyhow!(DAMAGED))?;
        ensure!(
            (1..=FORMAT).contains(&file.format),
            "this vault was written by a newer TermF"
        );
        file.remembered |= self
            .machine()
            .is_some_and(|m| m.vault == file.id && m.remembered);
        Ok(file)
    }

    fn machine(&self) -> Option<Machine> {
        serde_json::from_slice(&fs::read(self.dir.join(MACHINE)).ok()?).ok()
    }

    fn remember_here(&self, vault: &str, remembered: bool) -> Result<()> {
        let machine = Machine {
            vault: vault.into(),
            remembered,
        };
        if self.machine().as_ref() == Some(&machine) {
            return Ok(());
        }
        self.write_bytes(MACHINE, &serde_json::to_vec_pretty(&machine)?)
    }

    fn write(&self, file: &VaultFile) -> Result<()> {
        self.remember_here(&file.id, file.remembered)?;
        self.write_bytes(FILE, &serde_json::to_vec_pretty(file)?)
    }

    /// Through a temporary file readable only by the user, renamed over the old one.
    fn write_bytes(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let tmp = self.dir.join(format!(".{name}.tmp"));
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut out = options.open(&tmp)?;
        #[cfg(unix)]
        out.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        out.write_all(bytes)?;
        out.sync_all()?;
        fs::rename(&tmp, self.dir.join(name)).context("cannot write the vault")
    }

    /// Before the first write in a newer format, the file as it was is copied to `vault.v<format>.json`, until the
    /// new one reads back ([`Vault::drop_backup`]).
    fn back_up(&self, file: &mut VaultFile) -> Result<Option<String>> {
        if file.format == FORMAT {
            return Ok(None);
        }
        let backup = format!("vault.v{}.json", file.format);
        // A copy left by an earlier vault is replaced: this file is the one to keep.
        self.write_bytes(&backup, &fs::read(self.dir.join(FILE))?)?;
        file.format = FORMAT;
        Ok(Some(backup))
    }

    /// The copy goes once the new file reads back as written: it would keep deleted secrets readable.
    fn drop_backup(&self, backup: &str, key: &Key, content: &Content) -> Result<()> {
        let written = Zeroizing::new(serde_json::to_vec(content)?);
        let read = self
            .read()
            .and_then(|f| f.open(key))
            .and_then(|c| Ok(Zeroizing::new(serde_json::to_vec(&c)?)));
        match read {
            Ok(read) if *read == *written => Ok(fs::remove_file(self.dir.join(backup))?),
            _ => bail!(
                "the vault was rewritten in its new format but does not read back as written: its previous file is kept as vault/{backup}"
            ),
        }
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

    pub fn set_lock_on_leave(&self, on: bool) {
        self.lock_on_leave.store(on, Ordering::Relaxed);
    }

    /// The user left the vault's view: it locks, unless that is turned off. Never refused, like a lock.
    pub fn leave(&self) -> Result<Status> {
        let left = self.lock_on_leave.load(Ordering::Relaxed)
            && self.session.lock().unwrap().take().is_some();
        if left {
            let _ = self.record(Event {
                detail: Some("left".into()),
                ..Event::new(&Actor::termf(), Action::Lock)
            });
        }
        let status = self.status()?;
        if let (true, Some(told)) = (left, &self.on_lock) {
            told(&status);
        }
        Ok(status)
    }

    pub fn status(&self) -> Result<Status> {
        self.check();
        // A synced vault that cannot be taken here yet (a newer TermF): unlocking says so.
        let _ = self.adopt();
        // A damaged file still has a status; unlocking says what is wrong.
        let file = self.read().ok();
        let quick = file.as_ref().and_then(|f| self.quick(&f.id));
        let now = self.now();
        let session = self.session.lock().unwrap();
        Ok(Status {
            initialized: self.initialized(),
            unlocked: session.is_some(),
            remembered: file.as_ref().is_some_and(|f| f.remembered),
            auto_lock_minutes: (self.auto_lock.load(Ordering::Relaxed) / 60) as u32,
            lock_on_leave: self.lock_on_leave.load(Ordering::Relaxed),
            prod_until: session.as_ref().and_then(|s| fresh(s.prod, now)),
            keys: file
                .as_ref()
                .and_then(|f| f.factor.as_ref())
                .map_or(0, |f| f.keys.len()),
            recovery: file.as_ref().is_some_and(|f| f.recovery.is_some()),
            quick_days: quick.as_ref().map(|q| q.days),
            quick_until: quick
                .and_then(|q| q.key.and(q.until))
                .filter(|until| *until > now.wall),
            confirmed_until: session.as_ref().and_then(|s| fresh(s.typed, now)),
            synced: self.synced(file.as_ref()),
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
        // Locked whatever happens to its entry: a lock is never refused.
        let _ = self.record(Event {
            detail: Some(reason.into()),
            ..Event::new(&Actor::termf(), Action::Lock)
        });
        if let (Some(told), Ok(status)) = (&self.on_lock, self.status()) {
            told(&status);
        }
        true
    }

    fn start(&self, id: &str, material: &Material, typed: bool) {
        let now = self.now();
        let mut session = self.session.lock().unwrap();
        // The old keys unlock their memory page when dropped, which may be the new ones': gone first.
        *session = None;
        *session = Some(Session {
            id: id.into(),
            keys: LockedKey::new(material),
            last: now,
            prod: typed.then_some(now),
            typed: typed.then_some(now),
        });
    }

    /// The data key, unless locked: counts as activity.
    fn key(&self) -> Result<(String, Key)> {
        self.key_for(true)
    }

    /// `active: false`: a use that must not keep the vault open, like a TOTP code shown on screen, refreshed
    /// while nobody is there.
    fn key_for(&self, active: bool) -> Result<(String, Key)> {
        let (id, material) = self.material(active)?;
        Ok((id, material.data))
    }

    fn material(&self, active: bool) -> Result<(String, Material)> {
        self.check();
        let now = self.now();
        let mut session = self.session.lock().unwrap();
        let session = session.as_mut().ok_or(Locked)?;
        if active {
            session.last = now;
        }
        Ok((session.id.clone(), session.keys.material()))
    }

    /// The session's keys, once the master password was typed in the last 5 minutes without a sleep since:
    /// what changing how the vault opens asks for.
    fn confirmed(&self) -> Result<(String, Material)> {
        let (id, material) = self.material(true)?;
        let now = self.now();
        let typed = self.session.lock().unwrap().as_ref().and_then(|s| s.typed);
        ensure!(fresh(typed, now).is_some(), Unconfirmed);
        Ok((id, material))
    }

    /// The session's keys after a change, and the password just typed when there was one.
    fn restart(&self, material: &Material, typed: bool) {
        let now = self.now();
        let mut session = self.session.lock().unwrap();
        let Some(Session {
            id,
            keys,
            prod,
            typed: was_typed,
            ..
        }) = session.take()
        else {
            return;
        };
        // As in `start`: the old keys go before the new ones lock their page.
        drop(keys);
        *session = Some(Session {
            id,
            keys: LockedKey::new(material),
            last: now,
            prod: if typed { Some(now) } else { prod },
            typed: if typed { Some(now) } else { was_typed },
        });
    }

    /// Creates the vault and unlocks it. `remember`: keep the password's key in the system keychain.
    pub fn init(&self, actor: &Actor, password: &str, remember: bool) -> Result<Status> {
        check_password(password)?;
        let _lock = self.exclusive()?;
        ensure!(!self.initialized(), "there is already a vault");
        let id = crypto::hex(&crypto::random::<16>());
        let mut material = Material {
            data: Zeroizing::new(crypto::random()),
            kek: Zeroizing::new([0; 32]),
            factor: None,
        };
        let mut file = VaultFile {
            format: FORMAT,
            id,
            kdf: kdf(),
            salt: String::new(),
            key: String::new(),
            remembered: false,
            data: String::new(),
            factor: None,
            recovery: None,
            secured: self.clock.wall(),
        };
        file.set_password(password, &mut material)?;
        file.seal(&material.data, &Content::default())?;
        file.remembered = remember && self.store.set(&file.id, material.kek.as_ref()).is_ok();
        self.write(&file)?;
        // No entry, no vault.
        if let Err(e) = self.record(Event::new(actor, Action::Init)) {
            let _ = fs::remove_file(self.dir.join(FILE));
            let _ = self.store.delete(&file.id);
            return Err(e);
        }
        self.start(&file.id, &material, true);
        self.status()
    }

    /// The factor secret from the security key the user touches, and which key it was.
    fn touch(&self, file: &VaultFile, pin: Option<&str>) -> Result<(String, Key)> {
        let credentials = file.credentials()?;
        let (used, output) = self.keys.secret(&credentials, &file.factor_salt()?, pin)?;
        let key = file
            .factor
            .iter()
            .flat_map(|f| &f.keys)
            .find(|k| crypto::unhex(&k.credential).as_ref() == Some(&used))
            .context(DAMAGED)?;
        let factor = crypto::unhex(&key.secret)
            .and_then(|sealed| crypto::open(&output, &factor_aad(&file.id, &key.id), &sealed))
            .map(Zeroizing::new)
            .and_then(|plain| key32(&plain))
            .context("this security key no longer opens the vault")?;
        Ok((key.id.clone(), factor))
    }

    /// With the master password, or without it from the system keychain when it keeps the key; and the security
    /// key the user touches when some are enrolled (`pin`: its PIN, when TermF asks for it). `remember`: keep the
    /// password's key in the keychain from now on, or forget it; `None` leaves it as it is.
    pub fn unlock(
        &self,
        actor: &Actor,
        password: Option<&str>,
        remember: Option<bool>,
        pin: Option<&str>,
    ) -> Result<Status> {
        self.adopt()?;
        let (file, opened) = self.opening()?;
        ensure!(
            file.factor.is_none() || remember != Some(true),
            "the keychain cannot stand in for a security key"
        );
        let kek = match password {
            Some(password) => file.derive(password)?,
            None => {
                let kept = (file.remembered && file.factor.is_none())
                    .then(|| self.store.get(&file.id).ok().flatten())
                    .flatten()
                    .and_then(|k| <[u8; 32]>::try_from(k).ok());
                Zeroizing::new(kept.context("the master password is needed")?)
            }
        };
        // The password cannot be checked without the key's part: the key is touched first either way.
        let factor = match &file.factor {
            Some(_) => {
                let touched = self.touch(&file, pin).inspect_err(|_| {
                    let _ = self.record(Event {
                        detail: Some("security key".into()),
                        ..Event::new(actor, Action::UnlockFailed)
                    });
                })?;
                Some(touched.1)
            }
            None => None,
        };
        let mut material = Material {
            data: Zeroizing::new([0; 32]),
            kek,
            factor,
        };
        let Some(data_key) = file.unwrap(&material.wrapping()) else {
            let _ = self.record(Event::new(actor, Action::UnlockFailed));
            if password.is_none() {
                bail!(
                    "the keychain's key no longer opens the vault: the master password is needed"
                );
            }
            if opened.is_some() && password.is_some_and(|p| self.changed_elsewhere(p)) {
                bail!("the master password changed on another machine: use the new one");
            }
            bail!("wrong master password");
        };
        material.data = data_key;
        file.open(&material.data)?;
        let absorbed = self.absorb(actor, &material.data, opened.as_ref())?;
        // The newer way to open came with a version that does not open: this machine's own file decides.
        if matches!(absorbed, synced::Absorbed::SetAside) && opened.is_some() {
            return self.unlock(actor, password, remember, pin);
        }
        let detail = match (password, &file.factor) {
            (None, _) => Some("keychain"),
            (Some(_), Some(_)) => Some("security key"),
            (Some(_), None) => None,
        };
        self.record(Event {
            detail: detail.map(Into::into),
            ..Event::new(actor, Action::Unlock)
        })?;
        if let Some(remember) = remember {
            let _lock = self.exclusive()?;
            let mut file = self.read()?;
            if remember {
                file.remembered = self.store.set(&file.id, material.kek.as_ref()).is_ok();
            } else {
                file.remembered = false;
                let _ = self.store.delete(&file.id);
            }
            self.write(&file)?;
        }
        if password.is_some() {
            self.reopen_quick(&file.id, &material);
        }
        self.start(&file.id, &material, password.is_some());
        self.status()
    }

    /// Never refused: locked even when its audit entry cannot be written.
    pub fn lock(&self, actor: &Actor) -> Result<Status> {
        if self.session.lock().unwrap().take().is_some() {
            let _ = self.record(Event::new(actor, Action::Lock));
        }
        self.status()
    }

    /// The master password again: the prod scope needs no other for 5 minutes, nor do changes to how the vault
    /// opens.
    pub fn reauth(&self, actor: &Actor, password: &str) -> Result<Status> {
        let (id, material) = self.material(true)?;
        let file = self.read()?;
        ensure!(file.id == id, Locked);
        let typed = file
            .derive(password)
            .ok()
            .map(|kek| Material {
                kek,
                ..material.clone()
            })
            .filter(|m| file.unwrap(&m.wrapping()).is_some());
        let Some(material) = typed else {
            let _ = self.record(Event::new(actor, Action::ReauthFailed));
            bail!("wrong master password");
        };
        self.record(Event::new(actor, Action::Reauth))?;
        self.restart(&material, true);
        self.status()
    }

    /// A security key the user touches, for the prod scope (5 minutes); not for changes to how the vault opens.
    pub fn reauth_key(&self, actor: &Actor, pin: Option<&str>) -> Result<Status> {
        let (id, material) = self.material(true)?;
        let file = self.read()?;
        ensure!(file.id == id, Locked);
        ensure!(file.factor.is_some(), "no security key is enrolled");
        let failed = |e: anyhow::Error| {
            let _ = self.record(Event {
                detail: Some("security key".into()),
                ..Event::new(actor, Action::ReauthFailed)
            });
            e
        };
        let (_, factor) = self.touch(&file, pin).map_err(failed)?;
        if material.factor.as_ref().map(|f| f.as_ref()) != Some(factor.as_ref()) {
            return Err(failed(anyhow!("this security key does not open the vault")));
        }
        self.record(Event {
            detail: Some("security key".into()),
            ..Event::new(actor, Action::Reauth)
        })?;
        self.prove();
        self.status()
    }

    /// Touch ID or Windows Hello, once the app asked it, for the prod scope (5 minutes): only while it could
    /// reopen the vault, so no weaker than locking and reopening.
    pub fn reauth_quick(&self, actor: &Actor) -> Result<Status> {
        let (id, _) = self.material(true)?;
        ensure!(
            self.quick_until(&id).is_some(),
            "the master password is needed"
        );
        self.record(Event {
            detail: Some("biometry".into()),
            ..Event::new(actor, Action::Reauth)
        })?;
        self.prove();
        self.status()
    }

    /// The user proved it is them just now, for the prod scope.
    fn prove(&self) {
        let now = self.now();
        if let Some(session) = self.session.lock().unwrap().as_mut() {
            session.prod = Some(now);
        }
    }

    /// The security keys enrolled, oldest first.
    pub fn keys(&self) -> Result<Vec<KeyInfo>> {
        let (id, _) = self.key()?;
        let file = self.read()?;
        ensure!(file.id == id, Locked);
        Ok(file
            .factor
            .iter()
            .flat_map(|f| &f.keys)
            .map(|k| KeyInfo {
                id: k.id.clone(),
                name: k.name.clone(),
                created: k.created,
            })
            .collect())
    }

    /// Writes a change to how the vault opens, `event` recorded first: no entry, no change.
    fn rewrite(&self, file: &mut VaultFile, material: &Material, event: Event) -> Result<()> {
        let content = file.open(&material.data)?;
        file.secured = self.clock.wall();
        let backup = self.back_up(file)?;
        self.record(event)?;
        self.write(file)?;
        if let Some(backup) = backup {
            self.drop_backup(&backup, &material.data, &content)?;
        }
        Ok(())
    }

    /// The file this session opened, under the writers' lock.
    fn locked_file(&self, id: &str) -> Result<(fs::File, VaultFile)> {
        let lock = self.exclusive()?;
        let file = self.read()?;
        ensure!(file.id == id, Locked);
        Ok((lock, file))
    }

    /// Enrolls the security key the user touches (twice), under `name`. The first one turns the second factor
    /// on: from then on the master password alone opens nothing, the keychain forgets the password's key, and
    /// a new recovery key is returned, to show once. Touch ID or Windows Hello waits for the password again.
    pub fn add_key(
        &self,
        actor: &Actor,
        name: &str,
        pin: Option<&str>,
    ) -> Result<(KeyInfo, Option<Secret>)> {
        let name = name.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 64,
            "a security key needs a name (64 characters at most)"
        );
        let (id, mut material) = self.confirmed()?;
        let before = self.read()?;
        ensure!(before.id == id, Locked);
        ensure!(
            before.factor.as_ref().map_or(0, |f| f.keys.len()) < MAX_KEYS,
            "{MAX_KEYS} security keys at most"
        );
        let first = before.factor.is_none();
        ensure!(first || material.factor.is_some(), Locked);
        let salt = if first {
            crypto::random()
        } else {
            before.factor_salt()?
        };
        let factor = material
            .factor
            .clone()
            .unwrap_or_else(|| Zeroizing::new(crypto::random()));
        let user = crypto::unhex(&id).context(DAMAGED)?;
        // Touched before taking the lock: the CLI must not wait on a key left on the desk.
        let (credential, output) = self
            .keys
            .enroll(&user, &before.credentials()?, &salt, pin)?;
        let (_lock, mut file) = self.locked_file(&id)?;
        let unchanged = match &file.factor {
            None => first,
            Some(f) => !first && f.salt == crypto::hex(&salt),
        };
        ensure!(unchanged, "the vault changed meanwhile: try again");
        let now = self.clock.wall();
        let key_id = crypto::hex(&crypto::random::<8>());
        let sealed = crypto::seal(&output, &factor_aad(&id, &key_id), factor.as_ref());
        let info = KeyInfo {
            id: key_id.clone(),
            name: name.into(),
            created: now,
        };
        file.factor
            .get_or_insert_with(|| Factor {
                salt: crypto::hex(&salt),
                keys: Vec::new(),
            })
            .keys
            .push(EnrolledKey {
                id: key_id.clone(),
                name: name.into(),
                credential: crypto::hex(&credential),
                secret: crypto::hex(&sealed),
                created: now,
            });
        let mut recovery = None;
        if first {
            material.factor = Some(factor);
            file.seal_key(&material);
            recovery = Some(file.seal_recovery(&material, now)?);
            if file.remembered {
                file.remembered = false;
                let _ = self.store.delete(&id);
            }
        }
        let event = security(actor, format!("security key added {key_id}"));
        self.rewrite(&mut file, &material, event)?;
        self.drop_quick(&id);
        self.restart(&material, false);
        Ok((info, recovery))
    }

    /// Removes a security key; without the last one, the master password alone opens the vault again. A copy
    /// of the file made before still opens with that key and the password of then: change the password too.
    pub fn remove_key(&self, actor: &Actor, key: &str) -> Result<Status> {
        let (id, mut material) = self.confirmed()?;
        let (_lock, mut file) = self.locked_file(&id)?;
        let factor = file.factor.as_mut().context("no such security key")?;
        let before = factor.keys.len();
        factor.keys.retain(|k| k.id != key);
        ensure!(factor.keys.len() < before, "no such security key");
        if factor.keys.is_empty() {
            file.factor = None;
            material.factor = None;
            file.seal_key(&material);
        }
        let event = security(actor, format!("security key removed {key}"));
        self.rewrite(&mut file, &material, event)?;
        self.drop_quick(&id);
        self.restart(&material, false);
        self.status()
    }

    /// A new master password. The keychain, when it keeps the password's key, keeps the new one.
    pub fn change_password(&self, actor: &Actor, password: &str) -> Result<Status> {
        check_password(password)?;
        let (id, mut material) = self.confirmed()?;
        let (_lock, mut file) = self.locked_file(&id)?;
        file.set_password(password, &mut material)?;
        if file.remembered {
            file.remembered = self.store.set(&id, material.kek.as_ref()).is_ok();
        }
        self.rewrite(
            &mut file,
            &material,
            security(actor, "password changed".into()),
        )?;
        self.drop_quick(&id);
        self.restart(&material, true);
        self.status()
    }

    /// A new recovery key, returned to show once: the one before stops working.
    pub fn new_recovery(&self, actor: &Actor) -> Result<Secret> {
        let (id, material) = self.confirmed()?;
        let (_lock, mut file) = self.locked_file(&id)?;
        let key = file.seal_recovery(&material, self.clock.wall())?;
        let event = security(actor, "recovery key renewed".into());
        self.rewrite(&mut file, &material, event)?;
        Ok(key)
    }

    /// Opens the vault with the recovery key and sets a new master password; the security keys stay enrolled
    /// (remove a lost one next). The recovery key used stops working: the new one is returned, to show once.
    pub fn recover(
        &self,
        actor: &Actor,
        recovery_key: &str,
        password: &str,
    ) -> Result<(Status, Secret)> {
        check_password(password)?;
        self.adopt()?;
        let lock = self.exclusive()?;
        let (mut file, _) = self.opening()?;
        let recovery = file
            .recovery
            .clone()
            .context("this vault has no recovery key")?;
        let typed = Zeroizing::new(crypto::normalize_recovery_key(recovery_key));
        let salt = crypto::unhex(&recovery.salt).context(DAMAGED)?;
        let key = crypto::derive(typed.as_bytes(), &salt, recovery.kdf)?;
        let plain = crypto::unhex(&recovery.key)
            .and_then(|sealed| crypto::open(&key, &aad("recovery", &file.id), &sealed))
            .map(Zeroizing::new)
            .filter(|p| p.len() == 32 || p.len() == 64);
        let Some(plain) = plain else {
            let _ = self.record(Event {
                detail: Some("recovery key".into()),
                ..Event::new(actor, Action::UnlockFailed)
            });
            bail!("wrong recovery key");
        };
        let factor = match file.factor {
            Some(_) => Some(plain.get(32..).and_then(key32).context(DAMAGED)?),
            None => None,
        };
        let mut material = Material {
            data: key32(&plain[..32]).context(DAMAGED)?,
            kek: Zeroizing::new([0; 32]),
            factor,
        };
        file.set_password(password, &mut material)?;
        if file.remembered {
            file.remembered = false;
            let _ = self.store.delete(&file.id);
        }
        let new = file.seal_recovery(&material, self.clock.wall())?;
        let event = Event {
            detail: Some("recovery key".into()),
            ..Event::new(actor, Action::Unlock)
        };
        self.rewrite(&mut file, &material, event)?;
        drop(lock);
        // Its security is now the newest: only the synced content is still to merge.
        self.absorb(actor, &material.data, None)?;
        self.drop_quick(&file.id);
        self.start(&file.id, &material, true);
        Ok((self.status()?, new))
    }

    fn quick(&self, id: &str) -> Option<Quick> {
        let quick: Quick = serde_json::from_slice(&fs::read(self.dir.join(QUICK)).ok()?).ok()?;
        (quick.vault == id).then_some(quick)
    }

    /// Until when Touch ID or Windows Hello reopens the vault, unless it is off or the password is due.
    fn quick_until(&self, id: &str) -> Option<u64> {
        let quick = self.quick(id)?;
        quick
            .key
            .and(quick.until)
            .filter(|until| *until > self.clock.wall())
    }

    /// Seals the session's keys for Touch ID or Windows Hello, for `days`, by a new key in the keychain.
    fn seal_quick(&self, id: &str, days: u32, material: &Material) -> Result<()> {
        let key: Key = Zeroizing::new(crypto::random());
        self.store.set(&quick_account(id), key.as_ref())?;
        let until = self.clock.wall() + u64::from(days) * 86_400;
        let sealed = crypto::seal(&key, &quick_aad(id, until), &material.bytes());
        let quick = Quick {
            vault: id.into(),
            days,
            until: Some(until),
            key: Some(crypto::hex(&sealed)),
        };
        self.write_bytes(QUICK, &serde_json::to_vec_pretty(&quick)?)
    }

    /// The password was just typed: Touch ID or Windows Hello reopens the vault for its days again.
    fn reopen_quick(&self, id: &str, material: &Material) {
        if let Some(quick) = self.quick(id).filter(|_| self.quick) {
            // A convenience: without the keychain, the status says it is not ready.
            let _ = self.seal_quick(id, quick.days, material);
        }
    }

    /// After a change to how the vault opens: the setting stays, the slot and its key go, so the password
    /// comes first again.
    fn drop_quick(&self, id: &str) {
        if let Some(quick) = self.quick(id).filter(|q| q.key.is_some()) {
            let kept = Quick {
                until: None,
                key: None,
                ..quick
            };
            if let Ok(bytes) = serde_json::to_vec_pretty(&kept) {
                let _ = self.write_bytes(QUICK, &bytes);
            }
        }
        let _ = self.store.delete(&quick_account(id));
    }

    /// Touch ID or Windows Hello reopens the vault on this machine for `days` after the password (`None`: off).
    pub fn set_quick(&self, actor: &Actor, days: Option<u32>) -> Result<Status> {
        if let Some(days) = days {
            ensure!(
                QUICK_DAYS.contains(&days),
                "Touch ID or Windows Hello reopens the vault for 1 to 30 days"
            );
        }
        let (id, material) = self.confirmed()?;
        match days {
            Some(days) => {
                self.seal_quick(&id, days, &material)?;
                // No entry, no change.
                if let Err(e) = self.record(security(actor, format!("biometry on, {days} days"))) {
                    self.drop_quick(&id);
                    return Err(e);
                }
            }
            None => {
                self.drop_quick(&id);
                match fs::remove_file(self.dir.join(QUICK)) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                    _ => {}
                }
                // Like a lock, turning it off is never refused.
                let _ = self.record(security(actor, "biometry off".into()));
            }
        }
        self.status()
    }

    /// Whether changing how the vault opens would go through now, so the app asks the system only then.
    pub fn can_change(&self) -> Result<()> {
        self.confirmed().map(|_| ())
    }

    /// Whether Touch ID or Windows Hello could reopen the vault now (or confirm prod), so the app asks the
    /// system only then.
    pub fn can_unlock_quick(&self) -> Result<()> {
        let file = self.read()?;
        self.ready_quick(&file.id).map(|_| ())
    }

    /// The slot's deadline and sealed keys, unless it is off or the password is due (then the slot goes).
    fn ready_quick(&self, id: &str) -> Result<(u64, String)> {
        let quick = self.quick(id).context("the master password is needed")?;
        let (Some(until), Some(sealed)) = (quick.until, quick.key) else {
            bail!("the master password is needed");
        };
        if until <= self.clock.wall() {
            self.drop_quick(id);
            bail!("{} days went by: the master password is needed", quick.days);
        }
        Ok((until, sealed))
    }

    /// Reopens the vault from this machine's keychain, once the app had Touch ID or Windows Hello say yes.
    pub fn unlock_quick(&self, actor: &Actor) -> Result<Status> {
        self.adopt()?;
        let file = self.read()?;
        if self.synced(Some(&file)) == Some(Synced::Secured) {
            self.drop_quick(&file.id);
            bail!("the vault's security changed on another machine: the master password is needed");
        }
        let (until, sealed) = self.ready_quick(&file.id)?;
        let sealed = sealed.as_str();
        let material = self
            .store
            .get(&quick_account(&file.id))
            .ok()
            .flatten()
            .and_then(|key| key32(&key))
            .zip(crypto::unhex(sealed))
            .and_then(|(key, sealed)| crypto::open(&key, &quick_aad(&file.id, until), &sealed))
            .map(Zeroizing::new)
            .and_then(|plain| Material::from_bytes(&plain))
            .filter(|m| {
                file.unwrap(&m.wrapping())
                    .is_some_and(|data| *data == *m.data)
            });
        let Some(material) = material else {
            let _ = self.record(Event {
                detail: Some("biometry".into()),
                ..Event::new(actor, Action::UnlockFailed)
            });
            self.drop_quick(&file.id);
            bail!("the keychain's key no longer opens the vault: the master password is needed");
        };
        file.open(&material.data)?;
        self.record(Event {
            detail: Some("biometry".into()),
            ..Event::new(actor, Action::Unlock)
        })?;
        self.absorb(actor, &material.data, None)?;
        self.start(&file.id, &material, false);
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
        self.open_for(true)
    }

    fn open_for(&self, active: bool) -> Result<Content> {
        let (id, key) = self.key_for(active)?;
        let file = self.read()?;
        ensure!(file.id == id, Locked);
        file.open(&key)
    }

    /// `event` is recorded once the change is made and before it is written: no entry, no change.
    fn modify<T>(&self, event: Event, change: impl FnOnce(&mut Content) -> Result<T>) -> Result<T> {
        let (id, key) = self.key()?;
        let _lock = self.exclusive()?;
        let mut file = self.read()?;
        ensure!(file.id == id, Locked);
        let mut content = file.open(&key)?;
        let out = change(&mut content)?;
        content.settle(self.clock.wall());
        let backup = self.back_up(&mut file)?;
        file.seal(&key, &content)?;
        self.record(event)?;
        self.write(&file)?;
        if let Some(backup) = backup {
            self.drop_backup(&backup, &key, &content)?;
        }
        Ok(out)
    }

    /// Every secret, or those `project` sees (its own and the global ones), sorted.
    pub fn list(&self, scope: Option<Scope>, project: Option<&str>) -> Result<Vec<SecretInfo>> {
        let content = self.open()?;
        let mut list: Vec<SecretInfo> = content
            .secrets
            .iter()
            .filter(|s| scope.is_none_or(|scope| s.scope == scope))
            .filter(|s| project.is_none() || s.project.is_none() || s.project.as_deref() == project)
            .map(|s| SecretInfo {
                conflicts: content.conflicts_of(&s.key()),
                ..s.info()
            })
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
        let event = Event {
            target: Some(id.clone()),
            size: Some(value.len() as u64),
            ..Event::new(actor, Action::Write)
        };
        self.put(id, description, Some(value), None, event)
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
        let event = Event {
            target: Some(id.clone()),
            detail: Some(uri.into()),
            ..Event::new(actor, Action::Reference)
        };
        self.put(id, description, None, Some(uri.into()), event)
    }

    fn put(
        &self,
        id: &SecretId,
        description: Option<&str>,
        value: Option<Secret>,
        reference: Option<String>,
        event: Event,
    ) -> Result<SecretInfo> {
        let now = self.clock.wall();
        self.modify(event, |content| {
            let i = match content.secrets.iter().position(|s| s.is(id)) {
                Some(i) => i,
                None => {
                    let key = secret_key(&id.name, id.scope, id.project.as_deref());
                    let revs = content.unbury(&key);
                    content.secrets.push(Stored {
                        name: id.name.clone(),
                        scope: id.scope,
                        project: id.project.clone(),
                        description: String::new(),
                        created: now,
                        updated: now,
                        value: None,
                        reference: None,
                        revs,
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
            new_rev(&mut stored.revs);
            Ok(stored.info())
        })
    }

    pub fn delete(&self, actor: &Actor, id: &SecretId) -> Result<()> {
        self.prod_gate(actor, id)?;
        let event = Event {
            target: Some(id.clone()),
            ..Event::new(actor, Action::Delete)
        };
        let now = self.clock.wall();
        self.modify(event, |content| {
            let i = content
                .secrets
                .iter()
                .position(|s| s.is(id))
                .with_context(|| format!("no secret {id}"))?;
            let gone = content.secrets.remove(i);
            content.bury(gone.key(), gone.project, gone.revs, now);
            Ok(())
        })
    }

    /// A secret's value, or what its reference resolves to. Audited before it is returned: no entry, no value.
    pub fn value(&self, actor: &Actor, id: &SecretId, access: Access) -> Result<Secret> {
        if access == Access::Reveal && actor.kind != ActorKind::Human {
            return Err(self.refuse(
                Event {
                    target: Some(id.clone()),
                    ..Event::new(actor, Action::Refused)
                },
                "only the user reveals a secret",
            ));
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
                reference::resolve(uri, self.path.as_deref())?,
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

    /// Recorded, then returned as the error.
    fn refuse(&self, event: Event, why: &str) -> anyhow::Error {
        let _ = self.record(Event {
            detail: Some(why.into()),
            ..event
        });
        anyhow!("{why}")
    }

    /// Masks every secret's value (not the references, unknown until resolved: [`Masker::add`]), and from the
    /// items the passwords of 8 characters or more, the TOTP secrets and the notes' lines that look like keys.
    pub fn masker(&self) -> Result<Masker> {
        let content = self.open()?;
        let others = content.conflicts.iter();
        let secrets = content
            .secrets
            .iter()
            .chain(others.clone().filter_map(|c| c.secret.as_ref()));
        let mut masker = Masker::new(secrets.filter_map(|s| s.value.as_ref()));
        for item in content
            .items
            .iter()
            .chain(others.filter_map(|c| c.item.as_ref()))
        {
            // A short password ("admin", "1234") or a note's prose would hide ordinary words everywhere.
            if let Some(password) = item.password.as_ref().filter(|p| p.len() >= 8) {
                masker.add(password);
            }
            if let Some(totp) = &item.totp {
                masker.add(&totp.secret);
            }
            if let Some(notes) = &item.notes {
                masker.add_key_lines(notes);
            }
        }
        Ok(masker)
    }

    /// Every item, or those `project` sees (its own and the global ones), by title.
    pub fn items(&self, project: Option<&str>) -> Result<Vec<ItemInfo>> {
        let content = self.open()?;
        let mut list: Vec<ItemInfo> = content
            .items
            .iter()
            .filter(|i| project.is_none() || i.project.is_none() || i.project.as_deref() == project)
            .map(|i| ItemInfo {
                conflicts: content.conflicts_of(&i.id),
                ..i.info()
            })
            .collect();
        list.sort_by_cached_key(|i| (i.title.to_lowercase(), i.id.clone()));
        Ok(list)
    }

    /// Creates an item (`draft.id: None`) or changes one.
    pub fn save_item(&self, actor: &Actor, draft: ItemDraft) -> Result<ItemInfo> {
        let totp = draft.check()?;
        let now = self.clock.wall();
        let id = match &draft.id {
            Some(id) => id.clone(),
            None => crypto::hex(&crypto::random::<16>()),
        };
        let event = Event {
            item: Some(id.clone()),
            ..Event::new(actor, Action::Write)
        };
        self.modify(event, |content| {
            let i = match content.items.iter().position(|i| i.id == id) {
                Some(i) => i,
                None => {
                    ensure!(draft.id.is_none(), "no such item");
                    content.items.push(Item {
                        id: id.clone(),
                        kind: draft.kind,
                        project: None,
                        title: String::new(),
                        urls: Vec::new(),
                        username: String::new(),
                        password: None,
                        totp: None,
                        notes: None,
                        agents: false,
                        created: now,
                        updated: now,
                        revs: Vec::new(),
                    });
                    content.items.len() - 1
                }
            };
            let item = &mut content.items[i];
            item.apply(draft, totp, now);
            new_rev(&mut item.revs);
            Ok(ItemInfo {
                conflicts: content.conflicts_of(&id),
                ..content.items[i].info()
            })
        })
    }

    pub fn delete_item(&self, actor: &Actor, id: &str) -> Result<()> {
        let event = Event {
            item: Some(id.into()),
            ..Event::new(actor, Action::Delete)
        };
        let now = self.clock.wall();
        self.modify(event, |content| {
            let i = content
                .items
                .iter()
                .position(|i| i.id == id)
                .context("no such item")?;
            let gone = content.items.remove(i);
            content.bury(gone.id, gone.project, gone.revs, now);
            Ok(())
        })
    }

    /// An item's value, as [`Vault::value`] gives a secret's: only the user reveals one, and an agent uses one
    /// only from an item the user opened to agents. Audited before it is returned: no entry, no value.
    pub fn item_value(
        &self,
        actor: &Actor,
        id: &str,
        field: Field,
        access: Access,
    ) -> Result<Secret> {
        let refused = Event {
            item: Some(id.into()),
            ..Event::new(actor, Action::Refused)
        };
        let agent = actor.kind != ActorKind::Human;
        if access == Access::Reveal && agent {
            return Err(self.refuse(refused, "only the user reveals a secret"));
        }
        let item = self
            .open_for(field != Field::Totp)?
            .items
            .into_iter()
            .find(|i| i.id == id)
            .context("no such item")?;
        if agent && !item.agents {
            return Err(self.refuse(refused, "this item is not open to agents"));
        }
        let value = match field {
            Field::Password => item.password,
            Field::Notes => item.notes,
            Field::Seed => item.totp.map(|t| t.secret),
            Field::Totp => item.totp.map(|t| t.code(self.clock.wall())),
        }
        .with_context(|| format!("this item has no {}", field.as_str()))?;
        let action = match access {
            Access::Reveal => Action::Reveal,
            Access::Use => Action::Read,
        };
        self.record(Event {
            item: Some(id.into()),
            size: Some(value.len() as u64),
            detail: Some(field.as_str().into()),
            ..Event::new(actor, action)
        })?;
        Ok(value)
    }
}
