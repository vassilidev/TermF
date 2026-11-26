//! Encrypted sync of the config between a user's machines (§5.4), through a remote of their choice that only
//! ever sees encrypted blobs with hidden names.
//!
//! The remote holds `termf-sync.json` (key derivation parameters and the data key, wrapped by the password and
//! by the recovery key), `files/<keyed hash of the path>` and `devices/<keyed hash of the id>`. This machine
//! keeps, under `<config>/.sync/` (never synced nor exported), its settings, the last synced version of each
//! file for three-way merges, and the git clone. The key derived from the password lives in the system
//! keychain; without one (a headless Linux) it stays in memory for the process and the password is asked
//! again next time: it is never written to disk.
//!
//! The vault's file, already sealed by its own key, travels as it is: the sync never opens it. Another
//! machine's version lands beside it (`incoming.json`), for the vault to merge item by item when it opens;
//! until then this machine's version is not pushed, as it would not hold the other one's changes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

pub mod crypto;
mod dev;
mod http;
mod lines;
mod local;
mod merge;
mod objects;
mod record;
mod remote;
mod ssh;
mod store;
#[cfg(test)]
mod tests;

use crypto::{DataKeys, Kdf, Key};
use record::{Entry, Record};
pub use remote::{Credentials, Folder, Git, Remote, RemoteSpec};
pub use store::{KeyStore, OsKeyStore};
pub use termf_library::archive::MissingRepo;

/// The vault's file as synced, and the name the other machines' version takes beside it until the vault merges
/// it (`termf-vault`): the sync never opens it.
pub const VAULT_PATH: &str = "vault/vault.json";
pub const VAULT_INCOMING: &str = "incoming.json";
const VAULT_FILE: &str = "vault.json";
const KEYS: &str = "termf-sync.json";
const FILES: &str = "files";
const DEVICES: &str = "devices";
const STATE_DIR: &str = ".sync";
/// What each sync replaced here, under `.sync/`, for [`Engine::undo`].
const BACKUPS: &str = "backups";
/// How many syncs can be undone, one after the other.
const KEEP_BACKUPS: usize = 10;
const FILE: &str = "f";
const DEVICE: &str = "d";
/// A machine that only pulls still says it is alive once a day.
const HEARTBEAT: u64 = 24 * 3600;
const ATTEMPTS: usize = 5;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    /// `termf.toml`, `layouts.toml` and the global library.
    Config,
    /// `projects/<id>.toml` and each project's library.
    Projects,
    /// `projects/<id>/conventions/`.
    Conventions,
    /// The vault's sealed file: `<data>/vault/vault.json`, never `quick.json` (this machine's Touch ID).
    Vault,
    /// Only ever on explicit request: the key pairs of `~/.ssh` and `~/.ssh/config` (`ssh.rs`).
    SshKeys,
    /// Only on request: Claude Code's user config and auto memory, the MCP servers of `~/.claude.json`, the
    /// global git config (`dev.rs`).
    DevConfig,
}

impl Scope {
    pub const AVAILABLE: [Scope; 6] = [
        Scope::Config,
        Scope::Projects,
        Scope::Conventions,
        Scope::Vault,
        Scope::SshKeys,
        Scope::DevConfig,
    ];
    /// What a setup syncs unless told otherwise: the vault, the SSH keys and the dev config only when asked.
    pub const DEFAULT: [Scope; 3] = [Scope::Config, Scope::Projects, Scope::Conventions];
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub path: String,
    /// The TOML keys both machines changed; empty when the whole file conflicts.
    pub keys: Vec<String>,
    /// `None`: deleted on that side. Repository paths are shown as synced.
    pub local: Option<String>,
    pub remote: Option<String>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub pulled: Vec<String>,
    pub pushed: Vec<String>,
    /// Kept as they are here until resolved; the remote keeps its version. Each side of a conflict on some keys
    /// is the file as keeping that side writes it: the other changes of both are in both.
    pub conflicts: Vec<Conflict>,
    pub missing_repos: Vec<MissingRepo>,
    /// Not synced, either way, until fixed here; `Status.last_error` says it too.
    pub skipped: Vec<Skipped>,
    /// Pulled and pushed both: this machine's version and another's, combined.
    pub merged: Vec<String>,
    /// Another machine's key pair written beside this machine's, which has the same name.
    pub renamed: Vec<Renamed>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Renamed {
    pub path: String,
    pub to: String,
}

/// What [`Engine::undo`] did.
#[derive(Serialize, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Undone {
    /// As they were before the last sync; each waits as a conflict until the user chooses.
    pub restored: Vec<String>,
    /// Changed here since that sync: left as they are.
    pub changed: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub path: String,
    pub error: String,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub remote: Option<RemoteSpec>,
    pub scopes: Vec<Scope>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    /// False: the password is needed on this machine (`unlock`).
    pub unlocked: bool,
    /// The key is in the system keychain; false: only in memory, until TermF quits.
    pub remembered: bool,
    pub last_sync: Option<u64>,
    pub last_error: Option<String>,
    pub conflicts: Vec<Conflict>,
    pub missing_repos: Vec<MissingRepo>,
    /// Another machine forgot this one: nothing syncs here until sync is set up again.
    pub forgotten: bool,
    /// When the last sync that can be undone changed files here.
    pub last_backup: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub os: String,
    /// When it last published, or said it was alive (daily).
    pub last_sync: u64,
    #[serde(default)]
    pub current: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    /// Only when this setup created the remote: shown once, never stored.
    pub recovery_key: Option<String>,
    pub report: Report,
    /// The first sync failed (as `status` also says). Sync is set up all the same, so the recovery key is not
    /// lost behind an error.
    pub error: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "keep", rename_all = "lowercase")]
pub enum Resolution {
    Local,
    Remote,
    /// A text written by the user, with repository paths as synced.
    Merged {
        text: String,
    },
}

/// The password is needed: it changed on another machine, or the keychain lost the key.
#[derive(Debug)]
pub struct Locked;

impl std::fmt::Display for Locked {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("this machine needs the sync password again")
    }
}

impl std::error::Error for Locked {}

/// Another machine removed this one from the sync and changed the data key.
#[derive(Debug)]
pub struct Forgotten;

impl std::fmt::Display for Forgotten {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(
            "another machine removed this one from the sync: turn sync off here, then set it up again to rejoin",
        )
    }
}

impl std::error::Error for Forgotten {}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Wrapped {
    salt: String,
    key: String,
}

/// `termf-sync.json`: nothing in it is secret.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct KeyFile {
    format: u32,
    /// Names this sync in the keychain; kept across password changes.
    sync_id: String,
    /// Changes with each data key.
    key_id: String,
    kdf: Kdf,
    password: Wrapped,
    recovery: Wrapped,
    /// The machines forgotten (`forgotten_tag` of their id), so that each one says so instead of asking for a
    /// password.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    forgotten: Vec<String>,
}

/// A device id as `forgotten` lists it: the remote learns nothing it could link to the device's blob.
fn forgotten_tag(device_id: &str) -> String {
    use sha2::Digest;
    crypto::hex(&sha2::Sha256::digest(format!(
        "termf-sync forgotten {device_id}"
    )))[..32]
        .to_owned()
}

impl KeyFile {
    /// A new data key wrapped by `password` and by a new recovery key; returns them with the password's key.
    fn create(
        sync_id: String,
        password: &str,
        forgotten: Vec<String>,
    ) -> Result<(KeyFile, [u8; 32], Key, String)> {
        let data_key: [u8; 32] = crypto::random();
        let key_id = crypto::hex(&crypto::random::<{ crypto::KEY_ID }>());
        let kdf = Kdf::default();
        let recovery_key = crypto::recovery_key();
        let wrap = |secret: &str| -> Result<(Wrapped, Key)> {
            let salt: [u8; 16] = crypto::random();
            let kek = crypto::derive(secret.as_bytes(), &salt, kdf)?;
            let key = crypto::seal(&kek, &wrap_aad(&key_id), &data_key);
            let wrapped = Wrapped {
                salt: crypto::hex(&salt),
                key: crypto::hex(&key),
            };
            Ok((wrapped, kek))
        };
        let (password, kek) = wrap(password)?;
        let (recovery, _) = wrap(&crypto::normalize_recovery_key(&recovery_key))?;
        let keys = KeyFile {
            format: 1,
            sync_id,
            key_id,
            kdf,
            password,
            recovery,
            forgotten,
        };
        Ok((keys, data_key, kek, recovery_key))
    }

    fn derive(&self, wrapped: &Wrapped, secret: &str) -> Result<Key> {
        let salt = crypto::unhex(&wrapped.salt).context("invalid termf-sync.json")?;
        crypto::derive(secret.as_bytes(), &salt, self.kdf)
    }

    fn unwrap(&self, wrapped: &Wrapped, kek: &Key) -> Option<[u8; 32]> {
        let sealed = crypto::unhex(&wrapped.key)?;
        crypto::open(kek, &wrap_aad(&self.key_id), &sealed)?
            .try_into()
            .ok()
    }

    fn data_keys(&self, data_key: &[u8; 32]) -> Result<DataKeys> {
        let id = crypto::unhex(&self.key_id)
            .and_then(|id| id.try_into().ok())
            .context("invalid termf-sync.json")?;
        Ok(DataKeys::new(data_key, id))
    }
}

fn wrap_aad(key_id: &str) -> Vec<u8> {
    format!("termf-sync key {key_id}").into_bytes()
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    remote: RemoteSpec,
    scopes: BTreeSet<Scope>,
    device_id: String,
    device_name: String,
    sync_id: String,
    remembered: bool,
    #[serde(default)]
    needs_password: bool,
    #[serde(default)]
    last_sync: Option<u64>,
    #[serde(default)]
    last_device_write: Option<u64>,
    #[serde(default)]
    last_error: Option<String>,
    #[serde(default)]
    conflicts: Vec<Conflict>,
    #[serde(default)]
    missing_repos: Vec<MissingRepo>,
    #[serde(default)]
    forgotten: bool,
    /// Put back by an undo: shown as conflicts, never merged nor sent, until the user chooses.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    held: BTreeSet<String>,
}

impl State {
    fn new(
        remote: RemoteSpec,
        scopes: BTreeSet<Scope>,
        sync_id: String,
        device_name: String,
    ) -> Self {
        State {
            remote,
            scopes,
            device_id: crypto::hex(&crypto::random::<8>()),
            device_name,
            sync_id,
            remembered: false,
            needs_password: false,
            last_sync: None,
            last_device_write: None,
            last_error: None,
            conflicts: Vec::new(),
            missing_repos: Vec::new(),
            forgotten: false,
            held: BTreeSet::new(),
        }
    }
}

/// This machine's synced files, as they are synced ([`Engine::read_mine`]).
struct Mine {
    entries: BTreeMap<String, Entry>,
    /// Those that hold what only makes sense here, as they are before [`merge::pin`].
    pinned: BTreeMap<String, Entry>,
    skipped: Vec<Skipped>,
}

impl Mine {
    /// The file as it is here, what only makes sense here included.
    fn file(&self, path: &str) -> Option<&Entry> {
        self.pinned.get(path).or(self.entries.get(path))
    }
}

/// A keychain that holds nothing and keeps nothing.
struct Nowhere;

impl KeyStore for Nowhere {
    fn get(&self, _: &str) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &[u8]) -> Result<()> {
        bail!("not kept")
    }
    fn delete(&self, _: &str) -> Result<()> {
        Ok(())
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// What this machine is called: its name in the sync, else its host name (the sync's default name).
pub fn device_name(config_dir: &Path) -> String {
    let file = config_dir.join(STATE_DIR).join("state.json");
    fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<State>(&bytes).ok())
        .map_or_else(hostname, |state| state.device_name)
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| {
            let out = Command::new("hostname").output().ok()?;
            String::from_utf8(out.stdout).ok()
        })
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::consts::OS.into())
}

fn check_scopes(scopes: &BTreeSet<Scope>) -> Result<()> {
    ensure!(!scopes.is_empty(), "choose at least one thing to sync");
    for scope in scopes {
        ensure!(
            Scope::AVAILABLE.contains(scope),
            "syncing {scope:?} is not available yet"
        );
    }
    Ok(())
}

fn text(entry: Option<&Entry>) -> Option<String> {
    entry.map(|e| String::from_utf8_lossy(&e.bytes).into_owned())
}

/// Writes through a hidden temporary file, so readers of a shared folder never see half a blob.
fn put(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let target = root.join(name);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = target.file_name().unwrap_or_default().to_string_lossy();
    let tmp = target.with_file_name(format!(".{file_name}.tmp"));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, &target).with_context(|| format!("cannot write {}", target.display()))
}

/// The blobs of `dir` (`files` or `devices`), by name. Anything else a sync service drops there (conflicted
/// copies, temporary files) is ignored.
fn blobs(root: &Path, dir: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root.join(dir)) else {
        return Ok(found);
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.len() == 32
            && name
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            let data = fs::read(entry.path())?;
            if crypto::is_blob(&data) {
                found.push((format!("{dir}/{name}"), data));
            }
        }
    }
    Ok(found)
}

fn read_keys(root: &Path) -> Result<Option<KeyFile>> {
    match fs::read(root.join(KEYS)) {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("invalid termf-sync.json on the remote")?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_keys(root: &Path, keys: &KeyFile) -> Result<()> {
    put(root, KEYS, &serde_json::to_vec_pretty(keys)?)
}

/// Every file record of the current key generation, whatever its path.
fn read_files(root: &Path, keys: &DataKeys) -> Result<BTreeMap<String, Record>> {
    let mut records = BTreeMap::new();
    for (name, data) in blobs(root, FILES)? {
        let Some(plain) = keys.open_blob(&name, &data)? else {
            continue;
        };
        let record = record::decode(&mut plain.as_slice())?;
        // The name is a hash of the path: a record filed under another name was moved there.
        ensure!(
            name == format!("{FILES}/{}", keys.name(FILE, &record.path)),
            "{name} is not where TermF put it"
        );
        records.insert(record.path.clone(), record);
    }
    Ok(records)
}

fn write_file(root: &Path, keys: &DataKeys, record: &Record) -> Result<()> {
    let name = format!("{FILES}/{}", keys.name(FILE, &record.path));
    let mut plain = Vec::new();
    record::encode(record, &mut plain);
    put(root, &name, &keys.seal_blob(&name, &plain))
}

fn read_devices(root: &Path, keys: &DataKeys) -> Result<Vec<Device>> {
    let mut devices = Vec::new();
    for (name, data) in blobs(root, DEVICES)? {
        if let Some(plain) = keys.open_blob(&name, &data)? {
            devices.push(serde_json::from_slice(&plain)?);
        }
    }
    Ok(devices)
}

fn write_device(root: &Path, keys: &DataKeys, device: &Device) -> Result<()> {
    let name = format!("{DEVICES}/{}", keys.name(DEVICE, &device.id));
    put(
        root,
        &name,
        &keys.seal_blob(&name, &serde_json::to_vec(device)?),
    )
}

pub struct Engine {
    config_dir: PathBuf,
    store: Box<dyn KeyStore>,
    /// The password's key for this process: a cache of the keychain, or its only copy without one.
    memory: Mutex<Option<(String, Key)>>,
    /// Extra environment for git and curl, so tests never read the user's git or curl config.
    git_env: Vec<(String, String)>,
    /// `<data>/vault`, for the vault scope.
    vault_dir: Option<PathBuf>,
    /// The home folder, for the SSH keys and dev config scopes.
    home: Option<PathBuf>,
    /// An S3 or WebDAV remote's credentials for this process: a cache of the keychain, or its only copy.
    credentials: Mutex<Option<(String, Credentials)>>,
}

impl Engine {
    pub fn new(config_dir: PathBuf, store: Box<dyn KeyStore>) -> Self {
        Self {
            config_dir,
            store,
            memory: Mutex::new(None),
            git_env: Vec::new(),
            vault_dir: None,
            home: None,
            credentials: Mutex::new(None),
        }
    }

    /// Where the vault lives (`<data>/vault`): without it, the vault scope syncs nothing.
    pub fn with_vault(mut self, dir: PathBuf) -> Self {
        self.vault_dir = Some(dir);
        self
    }

    /// The home folder of `~/.ssh`, `~/.claude` and `~/.gitconfig`: without it, the SSH keys and dev config
    /// scopes sync nothing.
    pub fn with_home(mut self, home: PathBuf) -> Self {
        self.home = Some(home);
        self
    }

    fn ssh(&self) -> Option<(termf_ssh::Ssh, &Path)> {
        let home = self.home.as_deref()?;
        Some((termf_ssh::Ssh::new(home.to_path_buf()), home))
    }

    /// Git's environment, so the tests of a crate built on this one never read the user's git config.
    #[doc(hidden)]
    pub fn with_git_env(mut self, env: Vec<(String, String)>) -> Self {
        self.git_env = env;
        self
    }

    fn dir(&self) -> PathBuf {
        self.config_dir.join(STATE_DIR)
    }

    pub fn enabled(&self) -> bool {
        self.dir().join("state.json").is_file()
    }

    fn load(&self) -> Result<State> {
        ensure!(self.enabled(), "sync is not set up on this machine");
        let bytes = fs::read(self.dir().join("state.json"))?;
        serde_json::from_slice(&bytes).context("invalid sync state")
    }

    fn save(&self, state: &State) -> Result<()> {
        termf_core::write_atomic(
            &self.dir().join("state.json"),
            &serde_json::to_string_pretty(state)?,
        )
    }

    fn load_base(&self) -> Result<BTreeMap<String, Record>> {
        match fs::read(self.dir().join("base")) {
            Ok(data) => record::decode_all(&data),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn save_base(&self, base: &BTreeMap<String, Record>) -> Result<()> {
        let mut data = Vec::new();
        for record in base.values() {
            record::encode(record, &mut data);
        }
        put(&self.dir(), "base", &data)
    }

    /// One sync at a time, across the app and the CLI.
    fn lock(&self) -> Result<File> {
        fs::create_dir_all(self.dir())?;
        // The merge bases and the backups hold the synced files in clear: the user's only.
        #[cfg(unix)]
        fs::set_permissions(
            self.dir(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir().join("lock"))?;
        file.lock()?;
        Ok(file)
    }

    fn locked(&self) -> Result<(File, State)> {
        ensure!(self.enabled(), "sync is not set up on this machine");
        let lock = self.lock()?;
        Ok((lock, self.load()?))
    }

    fn remote(&self, spec: &RemoteSpec) -> Result<Box<dyn Remote>> {
        let work = self.dir().join("http");
        let store: Box<dyn objects::Store> = match spec {
            RemoteSpec::Git { url } => {
                return Ok(Box::new(Git::new(
                    url.clone(),
                    self.dir().join("git"),
                    self.git_env.clone(),
                )));
            }
            RemoteSpec::Folder { path } => {
                return Ok(Box::new(Folder(termf_core::expand_tilde(path))));
            }
            RemoteSpec::S3 {
                endpoint,
                bucket,
                region,
                prefix,
                ..
            } => Box::new(http::S3::new(
                endpoint,
                bucket,
                region,
                prefix,
                &self.remote_credentials(spec)?,
                work,
                self.git_env.clone(),
            )?),
            RemoteSpec::Webdav { url, .. } => Box::new(http::Webdav::new(
                url,
                self.remote_credentials(spec).ok().as_ref(),
                work,
                self.git_env.clone(),
            )?),
        };
        Ok(Box::new(objects::Objects::new(
            store,
            self.dir().join("objects"),
            spec.label(),
        )))
    }

    fn credentials_account(spec: &RemoteSpec) -> String {
        format!("remote {}", spec.label())
    }

    /// Those given with `spec` (setting up), else this process's, else the keychain's.
    fn remote_credentials(&self, spec: &RemoteSpec) -> Result<Credentials> {
        let given = match spec {
            RemoteSpec::S3 { credentials, .. } | RemoteSpec::Webdav { credentials, .. } => {
                credentials.clone()
            }
            _ => None,
        };
        if let Some(given) = given {
            return Ok(given);
        }
        let account = Self::credentials_account(spec);
        let mut memory = self.credentials.lock().unwrap();
        if let Some((a, c)) = memory.as_ref()
            && *a == account
        {
            return Ok(c.clone());
        }
        let stored = self
            .store
            .get(&account)
            .ok()
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<Credentials>(&bytes).ok())
            .with_context(|| {
                format!(
                    "the credentials of {} are not on this machine: turn sync off here and set it up again",
                    spec.label()
                )
            })?;
        *memory = Some((account, stored.clone()));
        Ok(stored)
    }

    /// Keeps the credentials given at setup: in the keychain, and for this process.
    fn keep_credentials(&self, spec: &RemoteSpec) -> Result<()> {
        if let RemoteSpec::S3 {
            credentials: Some(c),
            ..
        }
        | RemoteSpec::Webdav {
            credentials: Some(c),
            ..
        } = spec
        {
            let account = Self::credentials_account(spec);
            // Without a keychain they stay in memory until TermF quits, like the password's key.
            let _ = self.store.set(&account, &serde_json::to_vec(c)?);
            *self.credentials.lock().unwrap() = Some((account, c.clone()));
        }
        Ok(())
    }

    fn kek(&self, sync_id: &str) -> Option<Key> {
        let mut memory = self.memory.lock().unwrap();
        if let Some((id, key)) = memory.as_ref()
            && id == sync_id
        {
            return Some(key.clone());
        }
        let secret: [u8; 32] = self.store.get(sync_id).ok()??.try_into().ok()?;
        let key = Key::new(secret);
        *memory = Some((sync_id.into(), key.clone()));
        Some(key)
    }

    fn remember(&self, sync_id: &str, kek: &Key) -> bool {
        *self.memory.lock().unwrap() = Some((sync_id.into(), kek.clone()));
        self.store.set(sync_id, kek.as_ref()).is_ok()
    }

    fn data_keys(&self, keys: &KeyFile) -> Result<DataKeys> {
        let open = || {
            let kek = self.kek(&keys.sync_id)?;
            keys.unwrap(&keys.password, &kek)
        };
        let data_key = open()
            .or_else(|| {
                // The cached key is stale when another process (the CLI) changed the password or unlocked.
                *self.memory.lock().unwrap() = None;
                open()
            })
            .ok_or(Locked)?;
        keys.data_keys(&data_key)
    }

    pub fn status(&self) -> Result<Status> {
        if !self.enabled() {
            return Ok(Status {
                enabled: false,
                remote: None,
                scopes: Vec::new(),
                device_id: None,
                device_name: None,
                unlocked: false,
                remembered: false,
                last_sync: None,
                last_error: None,
                conflicts: Vec::new(),
                missing_repos: Vec::new(),
                forgotten: false,
                last_backup: None,
            });
        }
        let state = self.load()?;
        let in_memory = self
            .memory
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|(id, _)| *id == state.sync_id);
        Ok(Status {
            enabled: true,
            unlocked: !state.needs_password && (in_memory || state.remembered),
            remembered: state.remembered,
            scopes: state.scopes.iter().copied().collect(),
            device_id: Some(state.device_id),
            device_name: Some(state.device_name),
            remote: Some(state.remote),
            last_sync: state.last_sync,
            last_error: state.last_error,
            conflicts: state.conflicts,
            missing_repos: state.missing_repos,
            forgotten: state.forgotten,
            last_backup: self
                .backups()
                .last()
                .and_then(|d| d.file_name()?.to_str()?.parse::<u64>().ok())
                .map(|millis| millis / 1000),
        })
    }

    /// Whether `spec` already holds TermF sync data: setting up then joins it with its password, instead of
    /// creating it (and a recovery key).
    pub fn probe(&self, spec: &RemoteSpec) -> Result<bool> {
        ensure!(!self.enabled(), "sync is already set up: disable it first");
        let _lock = self.lock()?;
        let mut remote = self.remote(spec)?;
        remote.begin()?;
        remote.check()?;
        Ok(read_keys(remote.root())?.is_some())
    }

    /// Creates the remote, or joins it when it already holds TermF sync data, then syncs.
    pub fn setup(
        &self,
        spec: RemoteSpec,
        password: &str,
        scopes: &[Scope],
        device_name: Option<&str>,
    ) -> Result<Setup> {
        ensure!(!self.enabled(), "sync is already set up: disable it first");
        let scopes: BTreeSet<Scope> = scopes.iter().copied().collect();
        check_scopes(&scopes)?;
        let lock = self.lock()?;
        let mut remote = self.remote(&spec)?;
        remote.check()?;
        let (mut recovery_key, mut joined) = (None, None);
        for _ in 0..ATTEMPTS {
            remote.begin()?;
            let root = remote.root();
            if let Some(keys) = read_keys(root)? {
                let kek = keys.derive(&keys.password, password)?;
                ensure!(
                    keys.unwrap(&keys.password, &kek).is_some(),
                    "wrong password for this remote"
                );
                joined = Some((keys.sync_id, kek));
                break;
            }
            let foreign = remote::foreign(root, &[KEYS, FILES, DEVICES])?;
            ensure!(
                foreign.is_empty(),
                "{} already holds other files ({}): choose an empty folder or repository",
                root.display(),
                foreign.join(", ")
            );
            check_password(password)?;
            let sync_id = crypto::hex(&crypto::random::<16>());
            let (keys, _, kek, recovery) = KeyFile::create(sync_id.clone(), password, Vec::new())?;
            write_keys(root, &keys)?;
            if remote.commit()? {
                recovery_key = Some(recovery);
                joined = Some((sync_id, kek));
                break;
            }
        }
        let (sync_id, kek) = joined.context("the remote kept changing: try again")?;
        self.keep_credentials(&spec)?;
        let remembered = self.remember(&sync_id, &kek);
        let name = device_name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(hostname, str::to_owned);
        let mut state = State::new(spec, scopes, sync_id, name);
        state.remembered = remembered;
        self.save(&state)?;
        let (report, error) = match self.sync_locked(&mut state, true, &lock) {
            Ok(report) => (report, None),
            Err(e) => (Report::default(), Some(format!("{e:#}"))),
        };
        Ok(Setup {
            recovery_key,
            report,
            error,
        })
    }

    /// What joining `spec` would do on this machine, without doing it: what it receives (`pulled`), combines with
    /// another machine's version (`merged`), finds in conflict, and sends (`pushed`). Nothing is written, here or
    /// on the remote, and the password's key is not kept.
    pub fn preview(&self, spec: RemoteSpec, password: &str, scopes: &[Scope]) -> Result<Report> {
        ensure!(!self.enabled(), "sync is already set up: disable it first");
        let scopes: BTreeSet<Scope> = scopes.iter().copied().collect();
        check_scopes(&scopes)?;
        let _lock = self.lock()?;
        let mut remote = self.remote(&spec)?;
        remote.begin()?;
        let keys = read_keys(remote.root())?.context("this remote holds no TermF sync yet")?;
        let kek = keys.derive(&keys.password, password)?;
        ensure!(
            keys.unwrap(&keys.password, &kek).is_some(),
            "wrong password for this remote"
        );
        let look = Engine {
            config_dir: self.config_dir.clone(),
            store: Box::new(Nowhere),
            memory: Mutex::new(Some((keys.sync_id.clone(), kek))),
            git_env: self.git_env.clone(),
            vault_dir: self.vault_dir.clone(),
            home: self.home.clone(),
            credentials: Mutex::new(None),
        };
        let mut state = State::new(spec, scopes, keys.sync_id, String::new());
        look.run(&mut state, false, true)
    }

    /// What a sync would do now, without doing it: what it would receive (`pulled`), send (`pushed`), combine
    /// (`merged`) and find in conflict. Nothing is written, here or on the remote; `last_error` is left as it is.
    pub fn compare(&self) -> Result<Report> {
        let (_lock, mut state) = self.locked()?;
        self.run(&mut state, true, true)
    }

    /// Takes the password on a machine that lost it (changed elsewhere, no keychain).
    pub fn unlock(&self, password: &str) -> Result<()> {
        let (_lock, mut state) = self.locked()?;
        let mut remote = self.remote(&state.remote)?;
        remote.begin()?;
        let keys = read_keys(remote.root())?.context("the remote holds no TermF sync data")?;
        self.not_forgotten(&keys, &mut state)?;
        let kek = keys.derive(&keys.password, password)?;
        ensure!(
            keys.unwrap(&keys.password, &kek).is_some(),
            "wrong password"
        );
        state.remembered = self.remember(&keys.sync_id, &kek);
        state.sync_id = keys.sync_id;
        state.needs_password = false;
        self.save(&state)
    }

    /// Refuses a machine the key file says was forgotten, and keeps it for `status` (whatever the command).
    fn not_forgotten(&self, keys: &KeyFile, state: &mut State) -> Result<()> {
        if !keys.forgotten.contains(&forgotten_tag(&state.device_id)) {
            return Ok(());
        }
        if !state.forgotten {
            state.forgotten = true;
            self.save(state)?;
        }
        Err(Forgotten.into())
    }

    /// Pulls, merges, and with `publish` pushes this machine's changes. Errors are kept for `status`.
    pub fn sync(&self, publish: bool) -> Result<Report> {
        let (lock, mut state) = self.locked()?;
        self.sync_locked(&mut state, publish, &lock)
    }

    fn sync_locked(&self, state: &mut State, publish: bool, _lock: &File) -> Result<Report> {
        let result = self.run(state, publish, false);
        if let Err(e) = &result {
            state.last_error = Some(format!("{e:#}"));
            state.needs_password |= e.downcast_ref::<Locked>().is_some();
            state.forgotten |= e.downcast_ref::<Forgotten>().is_some();
            self.save(state)?;
        }
        result
    }

    /// `dry`: decides as a sync would, writes nothing anywhere, and says what it would do.
    fn run(&self, state: &mut State, publish: bool, dry: bool) -> Result<Report> {
        let mut remote = self.remote(&state.remote)?;
        let mut base = self.load_base()?;
        let mut mapping = local::load_mapping(&self.config_dir)?;
        let mut pulled = BTreeSet::new();
        for _ in 0..ATTEMPTS {
            remote.begin()?;
            let root = remote.root().to_path_buf();
            let keys_file =
                read_keys(&root)?.context("the remote holds no TermF sync data anymore")?;
            self.not_forgotten(&keys_file, state)?;
            let keys = self.data_keys(&keys_file)?;
            let theirs = read_files(&root, &keys)?;
            let before = mapping.clone();
            let mine = self.read_mine(&state.scopes, &base, &mut mapping)?;
            if mapping != before && !dry {
                local::save_mapping(&self.config_dir, &mapping)?;
            }
            let mut skipped = mine.skipped.clone();
            let ssh = self
                .ssh()
                .filter(|_| state.scopes.contains(&Scope::SshKeys));
            let home = self
                .home
                .as_deref()
                .filter(|_| state.scopes.contains(&Scope::DevConfig));
            let paths: BTreeSet<&String> = mine
                .entries
                .keys()
                .chain(theirs.keys())
                .chain(base.keys())
                .collect();
            let paths: Vec<String> = paths
                .into_iter()
                .filter(|p| {
                    let ours = if dev::is_dev(p) {
                        home.is_some_and(|h| dev::valid(p) && dev::placed(h, p, &self.git_env))
                    } else {
                        local::valid(p) || (ssh::is_file(p) && ssh.is_some())
                    };
                    ours && state.scopes.contains(&local::scope_of(p))
                        && !termf_library::archive::through_link(&self.config_dir, p)
                        && !skipped.iter().any(|s| {
                            p.strip_prefix(&s.path)
                                .is_some_and(|r| r.is_empty() || r.starts_with('/'))
                        })
                })
                .cloned()
                .collect();

            let mut conflicts = Vec::new();
            let mut outgoing = Vec::new();
            let mut merged = Vec::new();
            // Written once every file is decided, after a copy of what they replace.
            let mut writes = Vec::new();
            for path in paths {
                let b = base.get(&path);
                let b_entry = b.and_then(|r| r.entry.as_ref());
                let b_clock = b.map(|r| r.clock.clone()).unwrap_or_default();
                let l = mine.entries.get(&path);
                // A blob missing from the remote says nothing: a sync service may not have delivered it yet.
                let r = theirs.get(&path);
                let r_entry = r.map_or(b_entry, |r| r.entry.as_ref());
                if state.held.contains(&path) {
                    if !merge::same(&path, l, r_entry) {
                        conflicts.push(Conflict {
                            keys: Vec::new(),
                            local: text(l),
                            remote: text(r_entry),
                            path,
                        });
                        continue;
                    }
                    state.held.remove(&path);
                }
                let r_clock = r.map_or(&b_clock, |r| &r.clock);
                let follows = record::descends(r_clock, &b_clock);
                let clock = || record::next(&b_clock, r_clock, &state.device_id);

                if path == ssh::KNOWN_HOSTS {
                    // Lines, in any order: written only when one is missing here, sent only when one is missing there.
                    let all = ssh::known_hosts(l, r_entry);
                    if !ssh::knows_all(l, all.as_ref()) {
                        writes.push((path.clone(), all.clone()));
                        pulled.insert(path.clone());
                    }
                    if !ssh::knows_all(r_entry, all.as_ref()) {
                        outgoing.push(Record {
                            path,
                            entry: all,
                            clock: clock(),
                        });
                    } else if let Some(r) = r {
                        base.insert(path, r.clone());
                    }
                } else if merge::same(&path, l, r_entry) {
                    if let Some(r) = r {
                        base.insert(path, r.clone());
                    }
                } else if merge::same(&path, r_entry, b_entry) {
                    outgoing.push(Record {
                        path,
                        entry: l.cloned(),
                        clock: clock(),
                    });
                } else if follows
                    && merge::same(&path, l, b_entry)
                    // Deleted elsewhere while it holds what only makes sense here: the user chooses.
                    && (r_entry.is_some() || !mine.pinned.contains_key(&path))
                {
                    writes.push((path.clone(), r_entry.cloned()));
                    base.insert(path.clone(), r.expect("changed remotely").clone());
                    pulled.insert(path);
                } else {
                    // Without a common history (a version written without this one), nothing is taken as base.
                    let common = if follows { b_entry } else { None };
                    let combined = match (l, r_entry) {
                        (Some(l), Some(re)) => merge::merge3(&path, common, l, re),
                        _ => Err(Vec::new()),
                    };
                    match combined {
                        Ok(entry) => {
                            writes.push((path.clone(), Some(entry.clone())));
                            base.insert(path.clone(), r.expect("changed remotely").clone());
                            pulled.insert(path.clone());
                            merged.push(path.clone());
                            outgoing.push(Record {
                                path,
                                entry: Some(entry),
                                clock: clock(),
                            });
                        }
                        Err(keys) => {
                            let side = |keep_remote| match (l, r_entry) {
                                (Some(l), Some(re)) if !keys.is_empty() => {
                                    merge::settle(&path, common, l, re, keep_remote)
                                }
                                _ => None,
                            };
                            let (local, remote) = (side(false), side(true));
                            conflicts.push(Conflict {
                                local: text(local.as_ref().or(mine.file(&path))),
                                remote: text(remote.as_ref().or(r_entry)),
                                keys,
                                path,
                            });
                        }
                    }
                }
            }
            if !dry {
                self.apply(&writes, &mine, &mapping)?;
            }
            if state.scopes.contains(&Scope::Vault)
                && let Some(dir) = &self.vault_dir
                && let Some(record) =
                    sync_vault(dir, &theirs, &mut base, &mut pulled, &state.device_id, dry)?
            {
                outgoing.push(record);
            }
            let mut renamed = Vec::new();
            if let Some((ssh, _)) = &ssh {
                // Unreadable keys skip the keys this time: never taken as deleted.
                match ssh::sync_keys(ssh, &theirs, &mut base, &mut pulled, &state.device_id, dry) {
                    Ok(keys) => {
                        outgoing.extend(keys.outgoing);
                        renamed = keys.renamed;
                        skipped.extend(keys.skipped);
                    }
                    Err(e) => skipped.push(Skipped {
                        path: "ssh/keys".into(),
                        error: format!("{e:#}"),
                    }),
                }
            }
            if dry {
                return Ok(Report {
                    pulled: pulled.into_iter().collect(),
                    pushed: outgoing.into_iter().map(|r| r.path).collect(),
                    conflicts,
                    missing_repos: Vec::new(),
                    skipped,
                    merged,
                    renamed,
                });
            }
            // What came from the remote is in place: kept even if publishing fails.
            self.save_base(&base)?;

            let mut pushed = Vec::new();
            let heartbeat = state
                .last_device_write
                .is_none_or(|t| now().saturating_sub(t) >= HEARTBEAT);
            if publish && (!outgoing.is_empty() || heartbeat) {
                for record in &outgoing {
                    write_file(&root, &keys, record)?;
                }
                let device = Device {
                    id: state.device_id.clone(),
                    name: state.device_name.clone(),
                    os: std::env::consts::OS.into(),
                    last_sync: now(),
                    current: false,
                };
                write_device(&root, &keys, &device)?;
                if !remote.commit()? {
                    continue;
                }
                state.last_device_write = Some(device.last_sync);
                for record in outgoing {
                    pushed.push(record.path.clone());
                    // A key pair's base is its digest: no private key in clear outside ~/.ssh.
                    let record = if ssh::is_key(&record.path) {
                        ssh::pushed(&record)
                    } else {
                        record
                    };
                    base.insert(record.path.clone(), record);
                }
                self.save_base(&base)?;
            }

            let missing_repos = if state.scopes.contains(&Scope::Projects) {
                local::missing_repos(&self.config_dir)?
            } else {
                Vec::new()
            };
            state.last_sync = Some(now());
            state.last_error = (!skipped.is_empty()).then(|| {
                skipped
                    .iter()
                    .map(|s| format!("{} is not synced: {}", s.path, s.error))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            state.needs_password = false;
            state.forgotten = false;
            state.conflicts = conflicts.clone();
            state.missing_repos = missing_repos.clone();
            self.save(state)?;
            return Ok(Report {
                pulled: pulled.into_iter().collect(),
                pushed,
                conflicts,
                missing_repos,
                skipped,
                merged,
                renamed,
            });
        }
        bail!("the remote kept changing during the sync: try again")
    }

    /// This machine's files of `scopes`, as they are synced. What cannot be read is skipped, never taken as
    /// deleted.
    fn read_mine(
        &self,
        scopes: &BTreeSet<Scope>,
        base: &BTreeMap<String, Record>,
        mapping: &mut local::Mapping,
    ) -> Result<Mine> {
        let (mut entries, mut skipped) = local::read(&self.config_dir, scopes, base, mapping)?;
        if let Some((ssh, home)) = self.ssh().filter(|_| scopes.contains(&Scope::SshKeys)) {
            let files = [
                (ssh::CONFIG, ssh::read_config(&ssh, home)),
                (ssh::KNOWN_HOSTS, ssh::read_known_hosts(&ssh)),
            ];
            for (path, read) in files {
                match read {
                    Ok(entry) => entries.extend(entry.map(|e| (path.to_owned(), e))),
                    Err(e) => skipped.push(Skipped {
                        path: path.into(),
                        error: format!("{e:#}"),
                    }),
                }
            }
        }
        if let Some(home) = self
            .home
            .as_deref()
            .filter(|_| scopes.contains(&Scope::DevConfig))
        {
            let (dev, unread) = dev::read(home, &self.git_env);
            entries.extend(dev);
            skipped.extend(unread);
        }
        let mut pinned = BTreeMap::new();
        for (path, entry) in &mut entries {
            let synced = base.get(path).and_then(|r| r.entry.as_ref());
            if let Some(synced) = merge::pin(path, entry, synced) {
                pinned.insert(path.clone(), std::mem::replace(entry, synced));
            }
        }
        Ok(Mine {
            entries,
            pinned,
            skipped,
        })
    }

    /// Writes what a sync settled here, what only makes sense here kept, after a copy of the files as they are
    /// and as they will be (`.sync/backups/`), for [`Engine::undo`]. Key pairs never come here: a sync only ever
    /// adds them.
    fn apply(
        &self,
        writes: &[(String, Option<Entry>)],
        mine: &Mine,
        mapping: &local::Mapping,
    ) -> Result<()> {
        if writes.is_empty() {
            return Ok(());
        }
        let landed: Vec<(&String, Option<Entry>)> = writes
            .iter()
            .map(|(path, entry)| {
                let entry = entry
                    .as_ref()
                    .map(|e| merge::unpin(path, e, mine.pinned.get(path)));
                (path, entry)
            })
            .collect();
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for (path, entry) in &landed {
            let record = |entry: Option<Entry>| Record {
                path: (*path).clone(),
                entry,
                clock: Default::default(),
            };
            record::encode(&record(mine.file(path).cloned()), &mut before);
            record::encode(&record(entry.clone()), &mut after);
        }
        self.save_backup(&before, &after)?;
        for (path, entry) in &landed {
            self.write_local(path, entry.as_ref(), mapping)?;
        }
        Ok(())
    }

    /// The backups, oldest first.
    fn backups(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = fs::read_dir(self.dir().join(BACKUPS))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|d| d.join("after").is_file())
            .collect();
        dirs.sort();
        dirs
    }

    fn save_backup(&self, before: &[u8], after: &[u8]) -> Result<()> {
        let root = self.dir().join(BACKUPS);
        fs::create_dir_all(&root)?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let dir = (millis..millis + 1000)
            .map(|m| root.join(format!("{m:013}")))
            .find(|d| fs::create_dir(d).is_ok())
            .context("cannot keep a copy of the files before syncing")?;
        put(&dir, "before", before)?;
        // Last: a folder without it is no backup.
        put(&dir, "after", after)?;
        let all = self.backups();
        for old in &all[..all.len().saturating_sub(KEEP_BACKUPS)] {
            fs::remove_dir_all(old)?;
        }
        Ok(())
    }

    /// Puts back the files the last sync (or conflict choice) changed on this machine, as they were before; a
    /// file changed here since is left as it is. What is put back is neither merged nor sent: each file waits as
    /// a conflict, side by side, until the user chooses. Each call goes one sync further back.
    pub fn undo(&self) -> Result<Undone> {
        let (lock, mut state) = self.locked()?;
        let dir = self
            .backups()
            .pop()
            .context("no sync to undo on this machine")?;
        let read = |name: &str| -> Result<BTreeMap<String, Record>> {
            record::decode_all(&fs::read(dir.join(name))?)
        };
        let (before, after) = (read("before")?, read("after")?);
        let mapping = local::load_mapping(&self.config_dir)?;
        let base = self.load_base()?;
        let mine = self.read_mine(&state.scopes, &base, &mut mapping.clone())?;
        let mut undone = Undone::default();
        for (path, was) in before {
            let wrote = after.get(&path).and_then(|r| r.entry.as_ref());
            // Byte for byte: a comment added since is a change too. Put back as it was, whole.
            if mine.file(&path).map(|e| &e.bytes) == wrote.map(|e| &e.bytes) {
                self.write_local(&path, was.entry.as_ref(), &mapping)?;
                state.held.insert(path.clone());
                undone.restored.push(path);
            } else {
                undone.changed.push(path);
            }
        }
        fs::remove_dir_all(&dir)?;
        self.save(&state)?;
        // The conflicts show at once; a failure stays in the status, as any sync's.
        let _ = self.sync_locked(&mut state, true, &lock);
        Ok(undone)
    }

    /// Settles a conflict, then syncs: the version kept becomes this machine's change on top of the remote's.
    /// Keeping one side of a conflict on some keys keeps the other side's changes to the other keys.
    pub fn resolve(&self, path: &str, resolution: Resolution) -> Result<Report> {
        ensure!(
            local::valid(path) || ssh::is_file(path) || dev::valid(path),
            "invalid path {path}"
        );
        let (lock, mut state) = self.locked()?;
        let mut remote = self.remote(&state.remote)?;
        remote.begin()?;
        let keys_file = read_keys(remote.root())?.context("the remote holds no TermF sync data")?;
        self.not_forgotten(&keys_file, &mut state)?;
        let keys = self.data_keys(&keys_file)?;
        let theirs = read_files(remote.root(), &keys)?
            .remove(path)
            .with_context(|| format!("the remote has no version of {path}"))?;
        let mapping = local::load_mapping(&self.config_dir)?;
        let mut base = self.load_base()?;
        let mine = self.read_mine(&state.scopes, &base, &mut mapping.clone())?;
        let l = mine.entries.get(path);
        // Put back by an undo: the two sides are whole files.
        let held = state.held.remove(path);
        let common = base
            .get(path)
            .filter(|b| record::descends(&theirs.clock, &b.clock))
            .and_then(|b| b.entry.as_ref());
        let side = |keep_remote| match (l, &theirs.entry) {
            (Some(l), Some(r)) if !held => merge::settle(path, common, l, r, keep_remote),
            _ => None,
        };
        let write = match resolution {
            Resolution::Local => side(false).map(Some),
            Resolution::Remote => Some(side(true).or_else(|| theirs.entry.clone())),
            Resolution::Merged { text } => Some(Some(Entry {
                bytes: text.into_bytes(),
                exec: theirs.entry.as_ref().is_some_and(|e| e.exec),
            })),
        };
        if let Some(entry) = write.filter(|e| !merge::same(path, e.as_ref(), l)) {
            self.apply(&[(path.to_owned(), entry)], &mine, &mapping)?;
        }
        base.insert(path.into(), theirs);
        self.save_base(&base)?;
        self.sync_locked(&mut state, true, &lock)
    }

    /// A file of the config folder, of `~/.ssh`, or of the dev config scope, as it is given.
    fn write_local(
        &self,
        path: &str,
        entry: Option<&Entry>,
        mapping: &local::Mapping,
    ) -> Result<()> {
        if ssh::is_file(path) {
            let (ssh, home) = self.ssh().context("no SSH folder on this machine")?;
            return if path == ssh::CONFIG {
                ssh::write_config(&ssh, home, entry)
            } else {
                ssh.set_known_hosts_file(entry.map(|e| e.bytes.as_slice()))
            };
        }
        if dev::is_dev(path) {
            let home = self
                .home
                .as_deref()
                .context("no home folder on this machine")?;
            return dev::write(home, path, entry, &self.git_env);
        }
        local::write(&self.config_dir, path, entry, mapping)
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let (_lock, mut state) = self.locked()?;
        let mut remote = self.remote(&state.remote)?;
        remote.begin()?;
        let keys_file = read_keys(remote.root())?.context("the remote holds no TermF sync data")?;
        self.not_forgotten(&keys_file, &mut state)?;
        let keys = self.data_keys(&keys_file)?;
        let mut devices = read_devices(remote.root(), &keys)?;
        for device in &mut devices {
            device.current = device.id == state.device_id;
        }
        devices.sort_by_key(|d| std::cmp::Reverse(d.last_sync));
        Ok(devices)
    }

    /// A new data key re-encrypts everything: machines that only know the old password can no longer read
    /// anything (this is how a machine is revoked), and the old recovery key stops working.
    pub fn change_password(&self, current: &str, password: &str) -> Result<String> {
        self.rekey(password, None, |keys| {
            let kek = keys.derive(&keys.password, current)?;
            keys.unwrap(&keys.password, &kek)
                .context("wrong current password")
        })
    }

    /// Sets a new password with the recovery key, which then stops working.
    pub fn recover(&self, recovery_key: &str, password: &str) -> Result<String> {
        self.rekey(password, None, |keys| {
            let secret = crypto::normalize_recovery_key(recovery_key);
            let kek = keys.derive(&keys.recovery, &secret)?;
            keys.unwrap(&keys.recovery, &kek)
                .context("wrong recovery key")
        })
    }

    /// Takes `device` off the list and re-encrypts everything with a new data key, still opened by the same
    /// password: the copies that machine kept no longer open what comes next, and it says it was forgotten.
    /// The other machines ask for the password once; the recovery key is replaced (returned).
    pub fn forget(&self, device: &str, password: &str) -> Result<String> {
        self.rekey(password, Some(device), |keys| {
            let kek = keys.derive(&keys.password, password)?;
            keys.unwrap(&keys.password, &kek).context("wrong password")
        })
    }

    fn rekey(
        &self,
        password: &str,
        forget: Option<&str>,
        open: impl Fn(&KeyFile) -> Result<[u8; 32]>,
    ) -> Result<String> {
        check_password(password)?;
        let (_lock, mut state) = self.locked()?;
        ensure!(
            forget != Some(state.device_id.as_str()),
            "this machine cannot forget itself: turn sync off here instead"
        );
        let mut remote = self.remote(&state.remote)?;
        for _ in 0..ATTEMPTS {
            remote.begin()?;
            let root = remote.root().to_path_buf();
            let old_file = read_keys(&root)?.context("the remote holds no TermF sync data")?;
            self.not_forgotten(&old_file, &mut state)?;
            let old = old_file.data_keys(&open(&old_file)?)?;
            let files = read_files(&root, &old)?;
            let mut devices = read_devices(&root, &old)?;
            let mut forgotten = old_file.forgotten.clone();
            if let Some(id) = forget {
                ensure!(
                    devices.iter().any(|d| d.id == id),
                    "no machine {id} in this sync"
                );
                devices.retain(|d| d.id != id);
                forgotten.push(forgotten_tag(id));
            }
            let stale: Vec<String> = [FILES, DEVICES]
                .iter()
                .map(|dir| blobs(&root, dir))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .map(|(name, _)| name)
                .collect();

            let (keys_file, data_key, kek, recovery_key) =
                KeyFile::create(old_file.sync_id.clone(), password, forgotten)?;
            let new = keys_file.data_keys(&data_key)?;
            // New blobs first, then the key file, then the old blobs: an interruption never leaves the
            // remote without a readable version.
            for record in files.values() {
                write_file(&root, &new, record)?;
            }
            for device in &devices {
                write_device(&root, &new, device)?;
            }
            write_keys(&root, &keys_file)?;
            for name in stale {
                fs::remove_file(root.join(name))?;
            }
            if remote.commit()? {
                state.remembered = self.remember(&keys_file.sync_id, &kek);
                state.needs_password = false;
                self.save(&state)?;
                return Ok(recovery_key);
            }
        }
        bail!("the remote kept changing: try again")
    }

    pub fn set_scopes(&self, scopes: &[Scope]) -> Result<()> {
        let scopes: BTreeSet<Scope> = scopes.iter().copied().collect();
        check_scopes(&scopes)?;
        let (_lock, mut state) = self.locked()?;
        state.scopes = scopes;
        self.save(&state)
    }

    /// Clones a missing repository (`project` and `path` as `Status.missing_repos` gives them) into `target`,
    /// with the SSH command another repository of the project uses here. When `target` is not the synced
    /// path, the project points at it on this machine only. Returns the status, without that repository.
    pub fn clone_repo(
        &self,
        project: &str,
        path: &str,
        target: &Path,
        progress: &mut dyn FnMut(&str),
    ) -> Result<Status> {
        ensure!(self.enabled(), "sync is not set up on this machine");
        let repo = local::missing_repos(&self.config_dir)?
            .into_iter()
            .find(|r| r.project == project && r.path == path)
            .context("this repository is not missing anymore")?;
        // Not under the sync's lock: a clone can take minutes.
        local::clone(&self.config_dir, &self.git_env, &repo, target, progress)?;
        {
            let (_lock, mut state) = self.locked()?;
            if termf_core::expand_tilde(&repo.path) != target {
                local::relocate(&self.config_dir, &repo, target)?;
            }
            state.missing_repos = local::missing_repos(&self.config_dir)?;
            self.save(&state)?;
        }
        self.status()
    }

    /// Stops syncing on this machine: its key, settings, merge bases and clone go. The config files, the
    /// repository paths of `local.toml` and the remote stay.
    pub fn disable(&self) -> Result<()> {
        let (lock, state) = self.locked()?;
        // Without a keychain there is nothing to delete.
        let _ = self.store.delete(&state.sync_id);
        let _ = self.store.delete(&Self::credentials_account(&state.remote));
        *self.memory.lock().unwrap() = None;
        *self.credentials.lock().unwrap() = None;
        fs::remove_file(self.dir().join("state.json"))?;
        drop(lock);
        fs::remove_dir_all(self.dir())?;
        Ok(())
    }
}

fn check_password(password: &str) -> Result<()> {
    ensure!(
        password.chars().count() >= 8,
        "the sync password needs at least 8 characters"
    );
    Ok(())
}

/// The vault's writers' lock, so its file and the version brought here are never read half-written.
fn vault_lock(dir: &Path) -> Result<File> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("lock"))?;
    file.lock()?;
    Ok(file)
}

/// Leaves another machine's version of the vault beside it (readable by the user only), or returns this
/// machine's to push: only once the vault merged what came before, so the remote always holds every change.
/// `dry`: writes nothing, says what it would do.
fn sync_vault(
    dir: &Path,
    theirs: &BTreeMap<String, Record>,
    base: &mut BTreeMap<String, Record>,
    pulled: &mut BTreeSet<String>,
    device: &str,
    dry: bool,
) -> Result<Option<Record>> {
    let _lock = if dry { None } else { Some(vault_lock(dir)?) };
    let b = base.get(VAULT_PATH);
    let b_entry = b.and_then(|r| r.entry.as_ref());
    let mine = match fs::read(dir.join(VAULT_FILE)) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    // A deletion is never synced: a vault is not lost through the remote.
    if let Some(r) = theirs.get(VAULT_PATH).filter(|r| r.entry.is_some())
        && r.entry.as_ref() != b_entry
    {
        let bytes = &r.entry.as_ref().expect("filtered").bytes;
        if mine.as_ref() != Some(bytes) {
            if !dry {
                let tmp = dir.join(format!(".{VAULT_INCOMING}.tmp"));
                let mut options = fs::OpenOptions::new();
                options.write(true).create(true).truncate(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
                let mut out = options.open(&tmp)?;
                std::io::Write::write_all(&mut out, bytes)?;
                out.sync_all()?;
                fs::rename(&tmp, dir.join(VAULT_INCOMING))?;
            }
            pulled.insert(VAULT_PATH.into());
        }
        base.insert(VAULT_PATH.into(), r.clone());
        return Ok(None);
    }
    if dir.join(VAULT_INCOMING).exists() {
        return Ok(None);
    }
    let Some(bytes) = mine.filter(|m| Some(m) != b_entry.map(|e| &e.bytes)) else {
        return Ok(None);
    };
    let b_clock = b.map(|r| r.clock.clone()).unwrap_or_default();
    let r_clock = theirs.get(VAULT_PATH).map_or(&b_clock, |r| &r.clock);
    Ok(Some(Record {
        path: VAULT_PATH.into(),
        entry: Some(Entry { bytes, exec: false }),
        clock: record::next(&b_clock, r_clock, device),
    }))
}
