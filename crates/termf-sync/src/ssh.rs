//! The SSH keys scope, only on explicit request: each key pair of `~/.ssh` travels as one record, sealed by the
//! sync's data key like every file; `~/.ssh/config` as `ssh/config`, merged host by host (`lines.rs`), and
//! `~/.ssh/known_hosts` as `ssh/known_hosts`, the lines of every machine together.
//! A private key is never written in clear outside `~/.ssh`: the merge base keeps a digest of each pair. A key
//! here is never replaced by another machine's: a different pair under the same name lands beside it under a
//! free name, and a deletion elsewhere is not applied.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use termf_ssh::{Pair, Ssh};
use zeroize::Zeroizing;

use crate::dev::{localize, portable};
use crate::record::{self, Entry, Record};
use crate::{Renamed, Skipped};

pub const CONFIG: &str = "ssh/config";
pub const KNOWN_HOSTS: &str = "ssh/known_hosts";
const KEYS: &str = "ssh/keys/";

/// `ssh/config` as this machine has it, the home folder's paths made portable.
pub fn read_config(ssh: &Ssh, home: &std::path::Path) -> Result<Option<Entry>> {
    Ok(ssh.config_file()?.map(|bytes| Entry {
        bytes: portable(home, bytes),
        exec: false,
    }))
}

pub fn write_config(ssh: &Ssh, home: &std::path::Path, entry: Option<&Entry>) -> Result<()> {
    let bytes = entry.map(|e| localize(home, e.bytes.clone()));
    ssh.set_config_file(bytes.as_deref())
}

pub fn is_key(path: &str) -> bool {
    path.starts_with(KEYS)
}

/// `ssh/config` or `ssh/known_hosts`: a file of `~/.ssh` merged as text, not a key pair.
pub fn is_file(path: &str) -> bool {
    path == CONFIG || path == KNOWN_HOSTS
}

pub fn read_known_hosts(ssh: &Ssh) -> Result<Option<Entry>> {
    Ok(ssh
        .known_hosts_file()?
        .map(|bytes| Entry { bytes, exec: false }))
}

/// Every line either side has, once, this machine's first: a host key learnt on one machine is known on all,
/// and none is ever removed or replaced (hashed lines too, each as it is).
pub fn known_hosts(local: Option<&Entry>, remote: Option<&Entry>) -> Option<Entry> {
    let (Some(l), Some(r)) = (local, remote) else {
        return local.or(remote).cloned();
    };
    fn lines(e: &Entry) -> impl Iterator<Item = &[u8]> {
        e.bytes.split(|&b| b == b'\n').map(<[u8]>::trim_ascii)
    }
    let mut known: BTreeSet<&[u8]> = lines(l).collect();
    let mut bytes = l.bytes.clone();
    for line in lines(r) {
        if line.is_empty() || !known.insert(line) {
            continue;
        }
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        bytes.extend(line);
        bytes.push(b'\n');
    }
    Some(Entry { bytes, exec: false })
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

fn write(ssh: &Ssh, pair: &Pair, dry: bool) -> Result<()> {
    if dry { Ok(()) } else { ssh.restore(pair) }
}

#[derive(Default)]
pub struct Outcome {
    pub outgoing: Vec<Record>,
    /// Another machine's pair kept beside this machine's under a free name.
    pub renamed: Vec<Renamed>,
    /// Pairs that could not be written here (a file in the way).
    pub skipped: Vec<Skipped>,
}

/// The first name free in `~/.ssh` for another machine's `name`: `name-2`, `name-3`…
fn free_name(ssh: &Ssh, name: &str, taken: &BTreeSet<String>) -> Option<String> {
    (2..100).map(|n| format!("{name}-{n}")).find(|n| {
        termf_ssh::valid_name(n)
            && !taken.contains(&format!("{KEYS}{n}"))
            && [n.clone(), format!("{n}.pub")]
                .iter()
                .all(|f| std::fs::symlink_metadata(ssh.dir().join(f)).is_err())
    })
}

/// Merges the key pairs: a new pair goes either way; a pair changed or deleted here is pushed; one that only
/// exists elsewhere is written here; a different pair under a name taken here is written beside it, under a
/// free name, and kept on this machine only. `dry`: nothing is written, the outcome says what would be.
pub fn sync_keys(
    ssh: &Ssh,
    theirs: &BTreeMap<String, Record>,
    base: &mut BTreeMap<String, Record>,
    pulled: &mut BTreeSet<String>,
    device: &str,
    dry: bool,
) -> Result<Outcome> {
    let mut mine: BTreeMap<String, Zeroizing<Vec<u8>>> = ssh
        .pairs()?
        .iter()
        .map(|p| (format!("{KEYS}{}", p.name), bundle(p)))
        .collect();
    // What this machine already has, under any name: the same pair is never written twice.
    let here: BTreeSet<Vec<u8>> = mine.values().map(|b| digest(b).bytes).collect();
    let paths: BTreeSet<String> = mine
        .keys()
        .chain(theirs.keys().filter(|p| is_key(p)))
        .chain(base.keys().filter(|p| is_key(p)))
        .cloned()
        .collect();
    // A copy's name is taken by no machine, so it never meets another key there.
    let mut taken = paths.clone();
    let mut outcome = Outcome::default();
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
                (None, Some(e)) => match write(ssh, &unbundle(name, &e.bytes)?, dry) {
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
                    Err(e) => outcome.skipped.push(Skipped {
                        path: path.clone(),
                        error: format!("{e:#}"),
                    }),
                },
                // Deleted elsewhere: kept here, offered again only once changed here.
                (Some(_), None) if local_changed => outcome.outgoing.push(push(&r.clock)),
                // Same name, another pair: both stay. This machine's is not offered to the others; theirs lands
                // beside it, unless this machine already has it.
                (Some(_), Some(e)) => {
                    let r_digest = r_digest.clone().expect("a pair");
                    if !here.contains(&r_digest.bytes) {
                        let Some(free) = free_name(ssh, name, &taken) else {
                            bail!("no free name in ~/.ssh for another machine's {name}");
                        };
                        let pair = Pair {
                            name: free.clone(),
                            ..unbundle(name, &e.bytes)?
                        };
                        if let Err(e) = write(ssh, &pair, dry) {
                            outcome.skipped.push(Skipped {
                                path: path.clone(),
                                error: format!("{e:#}"),
                            });
                            continue;
                        }
                        let copy = format!("{KEYS}{free}");
                        taken.insert(copy.clone());
                        pulled.insert(copy.clone());
                        outcome.renamed.push(Renamed {
                            path: path.clone(),
                            to: copy.clone(),
                        });
                        // Known here as it is, so never sent: the other machine has it under its own name.
                        base.insert(
                            copy.clone(),
                            Record {
                                path: copy,
                                entry: Some(r_digest),
                                clock: r.clock.clone(),
                            },
                        );
                    }
                    base.insert(path.clone(), seen(r.clock.clone()));
                }
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
