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
mod local;
mod merge;
mod record;
mod remote;
mod store;
#[cfg(test)]
mod tests;

use crypto::{DataKeys, Kdf, Key};
use record::{Entry, Record};
pub use remote::{Folder, Git, Remote, RemoteSpec};
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
    /// Only ever on explicit request.
    SshKeys,
}

impl Scope {
    pub const AVAILABLE: [Scope; 4] = [
        Scope::Config,
        Scope::Projects,
        Scope::Conventions,
        Scope::Vault,
    ];
    /// What a setup syncs unless told otherwise: the vault only when asked.
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
    /// Kept as they are here until resolved; the remote keeps its version.
    pub conflicts: Vec<Conflict>,
    pub missing_repos: Vec<MissingRepo>,
    /// Not synced, either way, until fixed here; `Status.last_error` says it too.
    pub skipped: Vec<Skipped>,
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
}

impl KeyFile {
    /// A new data key wrapped by `password` and by a new recovery key; returns them with the password's key.
    fn create(sync_id: String, password: &str) -> Result<(KeyFile, [u8; 32], Key, String)> {
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
    /// Extra environment for git, so tests never read the user's git config.
    git_env: Vec<(String, String)>,
    /// `<data>/vault`, for the vault scope.
    vault_dir: Option<PathBuf>,
}

impl Engine {
    pub fn new(config_dir: PathBuf, store: Box<dyn KeyStore>) -> Self {
        Self {
            config_dir,
            store,
            memory: Mutex::new(None),
            git_env: Vec::new(),
            vault_dir: None,
        }
    }

    /// Where the vault lives (`<data>/vault`): without it, the vault scope syncs nothing.
    pub fn with_vault(mut self, dir: PathBuf) -> Self {
        self.vault_dir = Some(dir);
        self
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

    fn remote(&self, spec: &RemoteSpec) -> Box<dyn Remote> {
        match spec {
            RemoteSpec::Git { url } => Box::new(Git::new(
                url.clone(),
                self.dir().join("git"),
                self.git_env.clone(),
            )),
            RemoteSpec::Folder { path } => Box::new(Folder(termf_core::expand_tilde(path))),
        }
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
        })
    }

    /// Whether `spec` already holds TermF sync data: setting up then joins it with its password, instead of
    /// creating it (and a recovery key).
    pub fn probe(&self, spec: &RemoteSpec) -> Result<bool> {
        ensure!(!self.enabled(), "sync is already set up: disable it first");
        let _lock = self.lock()?;
        let mut remote = self.remote(spec);
        remote.begin()?;
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
        let mut remote = self.remote(&spec);
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
            let (keys, _, kek, recovery) = KeyFile::create(sync_id.clone(), password)?;
            write_keys(root, &keys)?;
            if remote.commit()? {
                recovery_key = Some(recovery);
                joined = Some((sync_id, kek));
                break;
            }
        }
        let (sync_id, kek) = joined.context("the remote kept changing: try again")?;
        let remembered = self.remember(&sync_id, &kek);
        let mut state = State {
            remote: spec,
            scopes,
            device_id: crypto::hex(&crypto::random::<8>()),
            device_name: device_name
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map_or_else(hostname, str::to_owned),
            sync_id,
            remembered,
            needs_password: false,
            last_sync: None,
            last_device_write: None,
            last_error: None,
            conflicts: Vec::new(),
            missing_repos: Vec::new(),
        };
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

    /// Takes the password on a machine that lost it (changed elsewhere, no keychain).
    pub fn unlock(&self, password: &str) -> Result<()> {
        let (_lock, mut state) = self.locked()?;
        let mut remote = self.remote(&state.remote);
        remote.begin()?;
        let keys = read_keys(remote.root())?.context("the remote holds no TermF sync data")?;
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

    /// Pulls, merges, and with `publish` pushes this machine's changes. Errors are kept for `status`.
    pub fn sync(&self, publish: bool) -> Result<Report> {
        let (lock, mut state) = self.locked()?;
        self.sync_locked(&mut state, publish, &lock)
    }

    fn sync_locked(&self, state: &mut State, publish: bool, _lock: &File) -> Result<Report> {
        let result = self.run(state, publish);
        if let Err(e) = &result {
            state.last_error = Some(format!("{e:#}"));
            state.needs_password |= e.downcast_ref::<Locked>().is_some();
            self.save(state)?;
        }
        result
    }

    fn run(&self, state: &mut State, publish: bool) -> Result<Report> {
        let mut remote = self.remote(&state.remote);
        let mut base = self.load_base()?;
        let mut mapping = local::load_mapping(&self.config_dir)?;
        let mut pulled = BTreeSet::new();
        for _ in 0..ATTEMPTS {
            remote.begin()?;
            let root = remote.root().to_path_buf();
            let keys_file =
                read_keys(&root)?.context("the remote holds no TermF sync data anymore")?;
            let keys = self.data_keys(&keys_file)?;
            let theirs = read_files(&root, &keys)?;
            let before = mapping.clone();
            let (mine, skipped) =
                local::read(&self.config_dir, &state.scopes, &base, &mut mapping)?;
            if mapping != before {
                local::save_mapping(&self.config_dir, &mapping)?;
            }
            let paths: BTreeSet<&String> = mine
                .keys()
                .chain(theirs.keys())
                .chain(base.keys())
                .collect();
            let paths: Vec<String> = paths
                .into_iter()
                .filter(|p| {
                    local::valid(p)
                        && state.scopes.contains(&local::scope_of(p))
                        && !termf_library::archive::through_link(&self.config_dir, p)
                        && !skipped.iter().any(|s| s.path == **p)
                })
                .cloned()
                .collect();

            let mut conflicts = Vec::new();
            let mut outgoing = Vec::new();
            for path in paths {
                let b = base.get(&path);
                let b_entry = b.and_then(|r| r.entry.as_ref());
                let b_clock = b.map(|r| r.clock.clone()).unwrap_or_default();
                let l = mine.get(&path);
                // A blob missing from the remote says nothing: a sync service may not have delivered it yet.
                let r = theirs.get(&path);
                let r_entry = r.map_or(b_entry, |r| r.entry.as_ref());
                let r_clock = r.map_or(&b_clock, |r| &r.clock);
                let follows = record::descends(r_clock, &b_clock);
                let clock = || record::next(&b_clock, r_clock, &state.device_id);

                if merge::same(&path, l, r_entry) {
                    if let Some(r) = r {
                        base.insert(path, r.clone());
                    }
                } else if merge::same(&path, r_entry, b_entry) {
                    outgoing.push(Record {
                        path,
                        entry: l.cloned(),
                        clock: clock(),
                    });
                } else if follows && merge::same(&path, l, b_entry) {
                    local::write(&self.config_dir, &path, r_entry, &mapping)?;
                    base.insert(path.clone(), r.expect("changed remotely").clone());
                    pulled.insert(path);
                } else {
                    // Without a common history (a version written without this one), nothing is taken as base.
                    let common = if follows { b_entry } else { None };
                    let merged = match (l, r_entry) {
                        (Some(l), Some(re)) => merge::merge3(&path, common, l, re),
                        _ => Err(Vec::new()),
                    };
                    match merged {
                        Ok(entry) => {
                            local::write(&self.config_dir, &path, Some(&entry), &mapping)?;
                            base.insert(path.clone(), r.expect("changed remotely").clone());
                            pulled.insert(path.clone());
                            outgoing.push(Record {
                                path,
                                entry: Some(entry),
                                clock: clock(),
                            });
                        }
                        Err(keys) => conflicts.push(Conflict {
                            keys,
                            local: text(l),
                            remote: text(r_entry),
                            path,
                        }),
                    }
                }
            }
            if state.scopes.contains(&Scope::Vault)
                && let Some(dir) = &self.vault_dir
                && let Some(record) =
                    sync_vault(dir, &theirs, &mut base, &mut pulled, &state.device_id)?
            {
                outgoing.push(record);
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
            state.conflicts = conflicts.clone();
            state.missing_repos = missing_repos.clone();
            self.save(state)?;
            return Ok(Report {
                pulled: pulled.into_iter().collect(),
                pushed,
                conflicts,
                missing_repos,
                skipped,
            });
        }
        bail!("the remote kept changing during the sync: try again")
    }

    /// Settles a conflict, then syncs: the version kept becomes this machine's change on top of the remote's.
    pub fn resolve(&self, path: &str, resolution: Resolution) -> Result<Report> {
        ensure!(local::valid(path), "invalid path {path}");
        let (lock, mut state) = self.locked()?;
        let mut remote = self.remote(&state.remote);
        remote.begin()?;
        let keys_file = read_keys(remote.root())?.context("the remote holds no TermF sync data")?;
        let keys = self.data_keys(&keys_file)?;
        let theirs = read_files(remote.root(), &keys)?
            .remove(path)
            .with_context(|| format!("the remote has no version of {path}"))?;
        let mapping = local::load_mapping(&self.config_dir)?;
        match resolution {
            Resolution::Local => {}
            Resolution::Remote => {
                local::write(&self.config_dir, path, theirs.entry.as_ref(), &mapping)?
            }
            Resolution::Merged { text } => {
                let entry = Entry {
                    bytes: text.into_bytes(),
                    exec: theirs.entry.as_ref().is_some_and(|e| e.exec),
                };
                local::write(&self.config_dir, path, Some(&entry), &mapping)?
            }
        }
        let mut base = self.load_base()?;
        base.insert(path.into(), theirs);
        self.save_base(&base)?;
        self.sync_locked(&mut state, true, &lock)
    }

    pub fn devices(&self) -> Result<Vec<Device>> {
        let (_lock, state) = self.locked()?;
        let mut remote = self.remote(&state.remote);
        remote.begin()?;
        let keys_file = read_keys(remote.root())?.context("the remote holds no TermF sync data")?;
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
        self.rekey(password, |keys| {
            let kek = keys.derive(&keys.password, current)?;
            keys.unwrap(&keys.password, &kek)
                .context("wrong current password")
        })
    }

    /// Sets a new password with the recovery key, which then stops working.
    pub fn recover(&self, recovery_key: &str, password: &str) -> Result<String> {
        self.rekey(password, |keys| {
            let secret = crypto::normalize_recovery_key(recovery_key);
            let kek = keys.derive(&keys.recovery, &secret)?;
            keys.unwrap(&keys.recovery, &kek)
                .context("wrong recovery key")
        })
    }

    fn rekey(&self, password: &str, open: impl Fn(&KeyFile) -> Result<[u8; 32]>) -> Result<String> {
        check_password(password)?;
        let (_lock, mut state) = self.locked()?;
        let mut remote = self.remote(&state.remote);
        for _ in 0..ATTEMPTS {
            remote.begin()?;
            let root = remote.root().to_path_buf();
            let old_file = read_keys(&root)?.context("the remote holds no TermF sync data")?;
            let old = old_file.data_keys(&open(&old_file)?)?;
            let files = read_files(&root, &old)?;
            let devices = read_devices(&root, &old)?;
            let stale: Vec<String> = [FILES, DEVICES]
                .iter()
                .map(|dir| blobs(&root, dir))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .map(|(name, _)| name)
                .collect();

            let (keys_file, data_key, kek, recovery_key) =
                KeyFile::create(old_file.sync_id.clone(), password)?;
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

    /// Stops syncing on this machine: its key, settings, merge bases and clone go. The config files, the
    /// repository paths of `local.toml` and the remote stay.
    pub fn disable(&self) -> Result<()> {
        let (lock, state) = self.locked()?;
        // Without a keychain there is nothing to delete.
        let _ = self.store.delete(&state.sync_id);
        *self.memory.lock().unwrap() = None;
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
fn sync_vault(
    dir: &Path,
    theirs: &BTreeMap<String, Record>,
    base: &mut BTreeMap<String, Record>,
    pulled: &mut BTreeSet<String>,
    device: &str,
) -> Result<Option<Record>> {
    let _lock = vault_lock(dir)?;
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
            let tmp = dir.join(format!(".{VAULT_INCOMING}.tmp"));
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            let mut out = options.open(&tmp)?;
            std::io::Write::write_all(&mut out, bytes)?;
            out.sync_all()?;
            fs::rename(&tmp, dir.join(VAULT_INCOMING))?;
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
