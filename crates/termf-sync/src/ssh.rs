//! The SSH keys scope, only on explicit request: each key pair of `~/.ssh` travels as one record, sealed by the
//! sync's data key like every file, and the Host blocks TermF added to `~/.ssh/config` as `ssh/hosts.toml`.
//! A private key is never written in clear outside `~/.ssh`: the merge base keeps a digest of each pair, and a
//! conflict shows public keys only. A key here is never replaced by another machine's: a different pair under
//! the same name is a conflict, a deletion elsewhere is not applied.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use termf_ssh::{HostEntry, Pair, Ssh};
use zeroize::Zeroizing;

use crate::record::{self, Entry, Record};
use crate::{Conflict, Resolution};

pub const HOSTS: &str = "ssh/hosts.toml";
const KEYS: &str = "ssh/keys/";

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Hosts {
    #[serde(default)]
    hosts: BTreeMap<String, HostEntry>,
}

/// `ssh/hosts.toml` as this machine has it. An alias written by hand here keeps the version last synced, so
/// the sync neither carries nor deletes it.
pub fn read_hosts(ssh: &Ssh, base: Option<&Record>) -> Result<Entry> {
    let mut hosts = Hosts::default();
    for host in ssh.marked_hosts() {
        hosts.hosts.insert(host.alias.clone(), host);
    }
    let synced: Hosts = base
        .and_then(|r| r.entry.as_ref())
        .and_then(|e| toml::from_str(std::str::from_utf8(&e.bytes).ok()?).ok())
        .unwrap_or_default();
    for (alias, host) in synced.hosts {
        if !hosts.hosts.contains_key(&alias) && ssh.hand_written(&alias) {
            hosts.hosts.insert(alias, host);
        }
    }
    Ok(Entry {
        bytes: toml::to_string(&hosts)?.into_bytes(),
        exec: false,
    })
}

pub fn write_hosts(ssh: &Ssh, entry: Option<&Entry>) -> Result<()> {
    let hosts: Hosts = match entry {
        Some(e) => {
            toml::from_str(std::str::from_utf8(&e.bytes)?).context("invalid ssh/hosts.toml")?
        }
        None => Hosts::default(),
    };
    let list: Vec<HostEntry> = hosts
        .hosts
        .into_iter()
        .map(|(alias, host)| HostEntry { alias, ..host })
        .collect();
    ssh.apply_hosts(&list)
}

pub fn is_key(path: &str) -> bool {
    path.starts_with(KEYS)
}

/// A pair as one record: private key length and bytes, then the public key, if any.
fn bundle(pair: &Pair) -> Zeroizing<Vec<u8>> {
    let mut out = Zeroizing::new(Vec::new());
    out.extend((pair.private.len() as u32).to_le_bytes());
    out.extend(pair.private.iter());
    if let Some(public) = &pair.public {
        out.extend(public);
    }
    out
}

fn unbundle(name: &str, bytes: &[u8]) -> Result<Pair> {
    let len = bytes
        .get(..4)
        .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize)
        .context("truncated key record")?;
    let private = bytes.get(4..4 + len).context("truncated key record")?;
    let public = &bytes[4 + len..];
    Ok(Pair {
        name: name.to_owned(),
        private: Zeroizing::new(private.to_vec()),
        public: (!public.is_empty()).then(|| public.to_vec()),
    })
}

/// What the merge base keeps of a pair: its digest, never its content.
fn digest(bytes: &[u8]) -> Entry {
    Entry {
        bytes: crate::crypto::hex(&Sha256::digest(bytes)).into_bytes(),
        exec: false,
    }
}

/// The public key, as a conflict shows it.
fn describe(pair: &Pair) -> String {
    match &pair.public {
        Some(public) => String::from_utf8_lossy(public).trim().to_owned(),
        None => format!(
            "{} (no public key file) · SHA-256 {}",
            pair.name,
            &crate::crypto::hex(&Sha256::digest(&pair.private))[..16]
        ),
    }
}

pub struct Outcome {
    pub outgoing: Vec<Record>,
    pub conflicts: Vec<Conflict>,
    /// Pairs that could not be written here (a file in the way).
    pub skipped: Vec<crate::Skipped>,
}

/// Merges the key pairs: a new pair goes either way; a pair changed or deleted here is pushed; one that only
/// exists elsewhere is written here; a different pair under a name taken here waits for the user's choice.
pub fn sync_keys(
    ssh: &Ssh,
    theirs: &BTreeMap<String, Record>,
    base: &mut BTreeMap<String, Record>,
    pulled: &mut BTreeSet<String>,
    device: &str,
) -> Result<Outcome> {
    let mut mine: BTreeMap<String, Zeroizing<Vec<u8>>> = ssh
        .pairs()?
        .iter()
        .map(|p| (format!("{KEYS}{}", p.name), bundle(p)))
        .collect();
    let paths: BTreeSet<String> = mine
        .keys()
        .chain(theirs.keys().filter(|p| is_key(p)))
        .chain(base.keys().filter(|p| is_key(p)))
        .cloned()
        .collect();
    let mut outcome = Outcome {
        outgoing: Vec::new(),
        conflicts: Vec::new(),
        skipped: Vec::new(),
    };
    for path in paths {
        let name = &path[KEYS.len()..];
        if !termf_ssh::valid_name(name) {
            continue;
        }
        let l = mine.remove(&path);
        let l_digest = l.as_deref().map(|b| digest(b));
        let b = base.get(&path);
        let b_clock = b.map(|r| r.clock.clone()).unwrap_or_default();
        let r = theirs.get(&path);
        let r_digest = r.and_then(|r| r.entry.as_ref()).map(|e| digest(&e.bytes));
        let local_changed = b.map(|b| b.entry.as_ref()) != Some(l_digest.as_ref());
        let remote_changed = r.is_some_and(|r| b.is_none_or(|b| b.clock != r.clock));
        let seen = |clock: record::Clock| Record {
            path: path.clone(),
            entry: l_digest.clone(),
            clock,
        };
        let push = |r_clock: &record::Clock| Record {
            path: path.clone(),
            entry: l.as_ref().map(|bytes| Entry {
                bytes: bytes.to_vec(),
                exec: false,
            }),
            clock: record::next(&b_clock, r_clock, device),
        };
        if let Some(r) = r.filter(|_| l_digest == r_digest) {
            base.insert(path.clone(), seen(r.clock.clone()));
        } else if remote_changed {
            let r = r.expect("changed remotely");
            match (&l, &r.entry) {
                // New elsewhere, or changed elsewhere while deleted here: a change wins over a deletion.
                (None, Some(e)) => match ssh.restore(&unbundle(name, &e.bytes)?) {
                    Ok(()) => {
                        pulled.insert(path.clone());
                        base.insert(
                            path.clone(),
                            Record {
                                path: path.clone(),
                                entry: r_digest.clone(),
                                clock: r.clock.clone(),
                            },
                        );
                    }
                    Err(e) => outcome.skipped.push(crate::Skipped {
                        path: path.clone(),
                        error: format!("{e:#}"),
                    }),
                },
                // Deleted elsewhere: kept here, offered again only once changed here.
                (Some(_), None) if local_changed => outcome.outgoing.push(push(&r.clock)),
                (Some(l), Some(e)) => outcome.conflicts.push(Conflict {
                    path: path.clone(),
                    keys: Vec::new(),
                    local: Some(describe(&unbundle(name, l)?)),
                    remote: Some(describe(&unbundle(name, &e.bytes)?)),
                }),
                _ => {
                    base.insert(path.clone(), seen(r.clock.clone()));
                }
            }
        } else if local_changed {
            outcome
                .outgoing
                .push(push(r.map_or(&b_clock, |r| &r.clock)));
        }
    }
    Ok(outcome)
}

/// The base entry of a pushed key record: its digest.
pub fn pushed(record: &Record) -> Record {
    Record {
        path: record.path.clone(),
        entry: record.entry.as_ref().map(|e| digest(&e.bytes)),
        clock: record.clock.clone(),
    }
}

/// Settles a conflict on a key: `local` keeps this machine's pair here without offering it to the others,
/// `remote` puts this machine's pair in `~/.ssh/.termf-trash/` and writes the other one.
pub fn resolve(
    ssh: &Ssh,
    path: &str,
    theirs: &Record,
    resolution: &Resolution,
    base: &mut BTreeMap<String, Record>,
) -> Result<()> {
    let name = &path[KEYS.len()..];
    ensure!(termf_ssh::valid_name(name), "invalid key name {name}");
    let mine = ssh.pairs()?.into_iter().find(|p| p.name == name);
    let entry = match resolution {
        Resolution::Local => mine.as_ref().map(|p| digest(&bundle(p))),
        Resolution::Remote => {
            let Some(remote) = &theirs.entry else {
                bail!("the other version of {name} was deleted: keep this one");
            };
            if mine.is_some() {
                ssh.trash(name)?;
            }
            ssh.restore(&unbundle(name, &remote.bytes)?)?;
            Some(digest(&remote.bytes))
        }
        Resolution::Merged { .. } => bail!("a key cannot be merged: keep one of the two"),
    };
    base.insert(
        path.into(),
        Record {
            path: path.into(),
            entry,
            clock: theirs.clock.clone(),
        },
    );
    Ok(())
}
