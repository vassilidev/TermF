//! An object store (S3, WebDAV) seen as a local folder: `begin` mirrors its objects under the sync's state
//! folder, `commit` uploads what changed there. Every upload is conditional (`If-None-Match: *` for a new object,
//! `If-Match: <ETag>` over the version mirrored), so a machine never overwrites what another one published since
//! its `begin`: it is told to begin again, and the engine merges.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::remote::Remote;

pub enum Written {
    /// With the new ETag, when the server gave it.
    Done(Option<String>),
    /// The condition failed: another machine wrote first.
    Lost,
}

pub trait Store {
    /// Every object under the remote's root, by name relative to it, with its ETag.
    fn list(&self) -> Result<BTreeMap<String, String>>;
    fn get(&self, name: &str, to: &Path) -> Result<()>;
    /// `expect`: the ETag the object must still have, or `None` for an object that must not exist yet.
    fn put(&self, name: &str, from: &Path, expect: Option<&str>) -> Result<Written>;
    fn delete(&self, name: &str) -> Result<()>;
}

#[derive(Serialize, Deserialize, Default)]
struct Index {
    remote: String,
    seen: BTreeMap<String, Seen>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Seen {
    etag: Option<String>,
    sha256: String,
}

pub struct Objects {
    store: Box<dyn Store>,
    root: PathBuf,
    index_file: PathBuf,
    label: String,
    index: Index,
}

/// What TermF puts on a remote: anything else there is left alone.
fn ours(name: &str) -> bool {
    let blob = |dir: &str| {
        name.strip_prefix(dir).is_some_and(|n| {
            n.len() == 32
                && n.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
    };
    name == crate::KEYS || blob("files/") || blob("devices/")
}

fn sha256(bytes: &[u8]) -> String {
    crate::crypto::hex(&Sha256::digest(bytes))
}

const CHECK: &str = ".termf-check";

impl Objects {
    /// `root`: the mirror; `label` names the remote, so a mirror of another one is never taken for it.
    pub fn new(store: Box<dyn Store>, root: PathBuf, label: String) -> Self {
        Self {
            store,
            index_file: root.with_extension("json"),
            root,
            label,
            index: Index::default(),
        }
    }

    fn load(&self) -> Index {
        fs::read(&self.index_file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Index>(&bytes).ok())
            .filter(|index| index.remote == self.label)
            .unwrap_or_else(|| Index {
                remote: self.label.clone(),
                seen: BTreeMap::new(),
            })
    }

    fn save(&self) -> Result<()> {
        termf_core::write_atomic(&self.index_file, &serde_json::to_string(&self.index)?)
    }

    /// The mirror's files, by object name; hidden ones are temporary files of the engine.
    fn mirrored(&self) -> Result<BTreeMap<String, PathBuf>> {
        let mut found = BTreeMap::new();
        for dir in ["", "files", "devices"] {
            let Ok(entries) = fs::read_dir(self.root.join(dir)) else {
                continue;
            };
            for entry in entries {
                let entry = entry?;
                let leaf = entry.file_name().to_string_lossy().into_owned();
                let name = if dir.is_empty() {
                    leaf
                } else {
                    format!("{dir}/{leaf}")
                };
                if entry.file_type()?.is_file() {
                    found.insert(name, entry.path());
                }
            }
        }
        Ok(found)
    }
}

impl Remote for Objects {
    fn root(&self) -> &Path {
        &self.root
    }

    fn begin(&mut self) -> Result<()> {
        let listed: BTreeMap<String, String> = self
            .store
            .list()?
            .into_iter()
            .filter(|(name, _)| ours(name))
            .collect();
        let mut index = self.load();
        fs::create_dir_all(self.root.join("files"))?;
        fs::create_dir_all(self.root.join("devices"))?;
        let mirrored = self.mirrored()?;
        // Whatever the mirror holds that the remote does not (an upload that failed) goes.
        for (name, path) in &mirrored {
            if !listed.contains_key(name) {
                fs::remove_file(path)?;
            }
        }
        index.seen.retain(|name, _| listed.contains_key(name));
        for (name, etag) in &listed {
            let file = self.root.join(name);
            let current = fs::read(&file).ok().map(|b| sha256(&b));
            let known = index.seen.get(name);
            if known.is_some_and(|s| {
                s.etag.as_ref() == Some(etag) && Some(&s.sha256) == current.as_ref()
            }) {
                continue;
            }
            let leaf = file.file_name().unwrap_or_default().to_string_lossy();
            let tmp = file.with_file_name(format!(".{leaf}.get"));
            self.store.get(name, &tmp)?;
            let bytes = fs::read(&tmp)?;
            fs::rename(&tmp, &file)?;
            index.seen.insert(
                name.clone(),
                Seen {
                    etag: Some(etag.clone()),
                    sha256: sha256(&bytes),
                },
            );
        }
        self.index = index;
        self.save()
    }

    fn commit(&mut self) -> Result<bool> {
        let mirrored: BTreeMap<String, PathBuf> = self
            .mirrored()?
            .into_iter()
            .filter(|(name, _)| ours(name))
            .collect();
        // The key file last: an interruption never leaves it naming blobs that are not there yet.
        let mut names: Vec<&String> = mirrored.keys().collect();
        names.sort_by_key(|name| *name == crate::KEYS);
        let mut lost = false;
        for name in names {
            let bytes = fs::read(&mirrored[name])?;
            let sha = sha256(&bytes);
            let expect = match self.index.seen.get(name) {
                Some(seen) if seen.sha256 == sha => continue,
                Some(Seen {
                    etag: Some(etag), ..
                }) => Some(etag.clone()),
                // Written here without the ETag coming back: the next begin learns it.
                Some(_) => {
                    lost = true;
                    break;
                }
                None => None,
            };
            match self.store.put(name, &mirrored[name], expect.as_deref())? {
                Written::Done(etag) => {
                    self.index
                        .seen
                        .insert(name.clone(), Seen { etag, sha256: sha });
                }
                Written::Lost => {
                    lost = true;
                    break;
                }
            }
        }
        if !lost {
            let gone: Vec<String> = self
                .index
                .seen
                .keys()
                .filter(|name| !mirrored.contains_key(*name))
                .cloned()
                .collect();
            for name in gone {
                self.store.delete(&name)?;
                self.index.seen.remove(&name);
            }
        }
        self.save()?;
        Ok(!lost)
    }

    /// Writes a probe object twice, the second time as if another machine had not seen the first: a server
    /// that accepts both would let two machines overwrite each other.
    fn check(&mut self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        let probe = self.root.with_extension("check");
        fs::write(&probe, b"termf")?;
        let first = self.store.put(CHECK, &probe, None);
        let second = first
            .as_ref()
            .ok()
            .map(|_| self.store.put(CHECK, &probe, None));
        let _ = self.store.delete(CHECK);
        let _ = fs::remove_file(&probe);
        if let Written::Lost = first.context("cannot write to the remote")? {
            // Left over from an interrupted check: the server does refuse a second creation.
            return Ok(());
        }
        match second.expect("the first write succeeded")? {
            Written::Lost => Ok(()),
            Written::Done(_) => bail!(
                "{} ignores conditional writes (If-None-Match): two machines could overwrite each other's sync. \
                 TermF needs a server that honors them",
                self.label
            ),
        }
    }
}
