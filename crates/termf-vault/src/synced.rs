//! Another machine's version of the vault, which the sync leaves beside it as `incoming.json` without ever
//! opening it: merged item by item when the vault opens, or right away while it is open. When the way it opens
//! changed there (password, security keys), only what opens the new way opens the vault: a vault protected by a
//! security key stays protected by it everywhere. Another vault altogether is refused until the user joins it.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::Serialize;
use termf_sync::crypto::Kdf;
use zeroize::Zeroizing;

use crate::item::Item;
use crate::merge::{Merged, summary};
use crate::{
    Action, Actor, Content, Event, FILE, FORMAT, Factor, ItemInfo, Key, Locked, Recovery, SecretId,
    SecretInfo, Status, Stored, Vault, VaultFile,
};

/// The synced version's name beside `vault.json`.
pub const INCOMING: &str = termf_sync::VAULT_INCOMING;
/// A synced version that does not open here, set aside (never deleted) so the vault opens without it.
pub const DAMAGED_INCOMING: &str = "incoming.damaged.json";

/// What merging the synced version did.
pub(crate) enum Absorbed {
    /// None to merge here (none, or another vault).
    Nothing,
    Merged(Vec<Merged>),
    /// It did not open: set aside, the vault stays as it was.
    SetAside,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Synced {
    /// Merged when the vault opens.
    Pending,
    /// Its master password or security keys changed on another machine: only the new ones open it.
    Secured,
    /// Another vault: never merged into this one, until the user joins it.
    Other,
    /// Written by a newer TermF.
    Newer,
    /// A version that does not open here came: set aside as `vault/incoming.damaged.json`.
    Damaged,
}

/// How a vault file opens: what a newer security brings along.
#[derive(Clone, PartialEq)]
pub(crate) struct Opening {
    kdf: Kdf,
    salt: String,
    key: String,
    factor: Option<Factor>,
    recovery: Option<Recovery>,
    secured: u64,
}

impl VaultFile {
    fn opening(&self) -> Opening {
        Opening {
            kdf: self.kdf,
            salt: self.salt.clone(),
            key: self.key.clone(),
            factor: self.factor.clone(),
            recovery: self.recovery.clone(),
            secured: self.secured,
        }
    }

    fn set_opening(&mut self, o: Opening) {
        self.kdf = o.kdf;
        self.salt = o.salt;
        self.key = o.key;
        self.factor = o.factor;
        self.recovery = o.recovery;
        self.secured = o.secured;
    }

    /// The same password and security keys open both (a new recovery key aside).
    fn opens_like(&self, other: &VaultFile) -> bool {
        self.kdf == other.kdf
            && self.salt == other.salt
            && self.key == other.key
            && self.factor == other.factor
    }
}

/// Another version of an element both sides changed: what it says, never a value.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub id: String,
    /// When the merge found it.
    pub at: u64,
    pub item: Option<ItemInfo>,
    pub secret: Option<SecretInfo>,
    /// The fields that differ from the current version, by name.
    pub differs: Vec<&'static str>,
}

fn item_differs(a: &Item, b: &Item) -> Vec<&'static str> {
    [
        ("title", a.title != b.title),
        ("project", a.project != b.project),
        ("username", a.username != b.username),
        ("urls", a.urls != b.urls),
        ("password", a.password != b.password),
        ("totp", a.totp != b.totp),
        ("notes", a.notes != b.notes),
        ("agents", a.agents != b.agents),
        ("kind", a.kind != b.kind),
    ]
    .into_iter()
    .filter_map(|(name, differs)| differs.then_some(name))
    .collect()
}

fn secret_differs(a: &Stored, b: &Stored) -> Vec<&'static str> {
    [
        ("value", a.value != b.value),
        ("reference", a.reference != b.reference),
        ("description", a.description != b.description),
    ]
    .into_iter()
    .filter_map(|(name, differs)| differs.then_some(name))
    .collect()
}

impl Vault {
    fn incoming_path(&self) -> PathBuf {
        self.dir.join(INCOMING)
    }

    /// The synced version waiting here, if any, with its bytes as they came.
    fn incoming(&self) -> Result<Option<(Vec<u8>, VaultFile)>> {
        let bytes = match fs::read(self.incoming_path()) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            other => other?,
        };
        let file = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow!("the vault synced from another machine is damaged"))?;
        Ok(Some((bytes, file)))
    }

    /// A synced version that does not open here: renamed beside the vault, never deleted, and audited. The
    /// vault opens without it; the status says so.
    fn set_aside(&self, why: &str) -> Result<()> {
        let _ = self.record(Event {
            detail: Some(format!("{why}: set aside as vault/{DAMAGED_INCOMING}")),
            ..Event::new(&Actor::termf(), Action::Merge)
        });
        Ok(fs::rename(
            self.incoming_path(),
            self.dir.join(DAMAGED_INCOMING),
        )?)
    }

    pub(crate) fn synced(&self, file: Option<&VaultFile>) -> Option<Synced> {
        let damaged = || {
            self.dir
                .join(DAMAGED_INCOMING)
                .is_file()
                .then_some(Synced::Damaged)
        };
        let theirs = match self.incoming() {
            Ok(Some((_, theirs))) => theirs,
            Ok(None) => return damaged(),
            Err(_) => return Some(Synced::Damaged),
        };
        let file = file?;
        Some(if theirs.format > FORMAT {
            Synced::Newer
        } else if theirs.id != file.id {
            Synced::Other
        } else if theirs.secured > file.secured && !theirs.opens_like(file) {
            Synced::Secured
        } else {
            Synced::Pending
        })
    }

    /// No vault here yet: the synced one becomes this machine's, opened with its own password.
    pub(crate) fn adopt(&self) -> Result<()> {
        if self.dir.join(FILE).exists() || !self.incoming_path().exists() {
            return Ok(());
        }
        let _lock = self.exclusive()?;
        if self.dir.join(FILE).exists() {
            return Ok(());
        }
        let (raw, mut file) = match self.incoming() {
            Ok(Some(found)) => found,
            Ok(None) => return Ok(()),
            Err(e) => return self.set_aside(&e.to_string()),
        };
        ensure!(
            file.format <= FORMAT,
            "this vault was written by a newer TermF"
        );
        self.record(Event {
            detail: Some("synced from another machine".into()),
            ..Event::new(&Actor::termf(), Action::Merge)
        })?;
        // This machine's keychain keeps nothing for it; the bytes stay as synced, so nothing goes back.
        if file.remembered || file.format != FORMAT {
            file.remembered = false;
            self.write(&file)?;
        } else {
            self.remember_here(&file.id, false)?;
            self.write_bytes(FILE, &raw)?;
        }
        fs::remove_file(self.incoming_path())?;
        Ok(())
    }

    /// The file as the password and the security key must open it: the way the synced version opens when that
    /// one's security is newer (returned too, so the merge after takes only that one).
    pub(crate) fn opening(&self) -> Result<(VaultFile, Option<Opening>)> {
        let mut file = self.read()?;
        let newer = self
            .incoming()
            .ok()
            .flatten()
            .map(|(_, theirs)| theirs)
            .filter(|t| t.id == file.id && t.format <= FORMAT && t.secured > file.secured)
            .map(|t| t.opening());
        if let Some(opening) = &newer {
            file.set_opening(opening.clone());
        }
        Ok((file, newer))
    }

    /// The password opens what this file opened before the sync brought a new way: it changed elsewhere.
    pub(crate) fn changed_elsewhere(&self, password: &str) -> bool {
        self.read().is_ok_and(|old| {
            old.factor.is_none()
                && old
                    .derive(password)
                    .is_ok_and(|kek| old.unwrap(&kek).is_some())
        })
    }

    /// Merges the synced version into the file, under the writers' lock: its content item by item, and how it
    /// opens when that is newer and is what was just opened (`opened`). A version that does not open is set
    /// aside: the vault stays as it was.
    pub(crate) fn absorb(
        &self,
        actor: &Actor,
        data: &Key,
        opened: Option<&Opening>,
    ) -> Result<Absorbed> {
        let _lock = self.exclusive()?;
        let (raw, theirs) = match self.incoming() {
            Ok(Some(found)) => found,
            Ok(None) => return Ok(Absorbed::Nothing),
            Err(e) => {
                self.set_aside(&e.to_string())?;
                return Ok(Absorbed::SetAside);
            }
        };
        let mut file = self.read()?;
        if theirs.id != file.id || theirs.format > FORMAT {
            return Ok(Absorbed::Nothing);
        }
        let newer = theirs.secured > file.secured;
        let opens_new = newer && !theirs.opens_like(&file);
        if opens_new && opened != Some(&theirs.opening()) {
            bail!("the vault changed on another machine meanwhile: unlock it again");
        }
        let Ok(incoming) = theirs.open(data) else {
            self.set_aside(
                "the vault synced from another machine does not open with this one's key",
            )?;
            return Ok(Absorbed::SetAside);
        };
        let mut content = file.open(data)?;
        let before = Zeroizing::new(serde_json::to_vec(&content)?);
        let rows = content.merge(incoming.clone(), self.clock.wall());
        let changed = *before != *Zeroizing::new(serde_json::to_vec(&content)?);
        if newer {
            file.set_opening(theirs.opening());
        }
        // This machine holds nothing the other version lacks: its bytes are taken as they came, so the remote
        // gets nothing back and two machines never send the vault to each other forever.
        let caught_up = content.equivalent(&incoming)
            && theirs.format == FORMAT
            && !theirs.remembered
            && file.opening() == theirs.opening();
        let mut detail = format!("synced: {}", summary(&rows));
        if opens_new {
            detail.push_str("; opens as set on another machine");
        }
        let event = Event {
            detail: Some(detail),
            ..Event::new(actor, Action::Merge)
        };
        if opens_new && file.remembered {
            file.remembered = false;
            let _ = self.store.delete(&file.id);
        }
        if caught_up {
            self.record(event)?;
            self.remember_here(&file.id, file.remembered)?;
            self.write_bytes(FILE, &raw)?;
        } else if changed || newer {
            let backup = self.back_up(&mut file)?;
            file.seal(data, &content)?;
            self.record(event)?;
            self.write(&file)?;
            if let Some(backup) = backup {
                self.drop_backup(&backup, data, &content)?;
            }
        } else {
            self.record(event)?;
        }
        if opens_new {
            self.drop_quick(&file.id);
        }
        fs::remove_file(self.incoming_path())?;
        Ok(Absorbed::Merged(rows))
    }

    /// The sync brought another machine's version while the vault is open: merged now. When the way it opens
    /// changed there, the vault locks instead, to be opened the new way.
    pub fn merge_synced(&self, actor: &Actor) -> Result<Option<Vec<Merged>>> {
        let Ok((id, material)) = self.material(false) else {
            return Ok(None);
        };
        let file = self.read()?;
        ensure!(file.id == id, Locked);
        match self.synced(Some(&file)) {
            Some(Synced::Pending | Synced::Damaged) if self.incoming_path().exists() => {
                match self.absorb(actor, &material.data, None)? {
                    Absorbed::Merged(rows) => Ok(Some(rows)),
                    _ => Ok(None),
                }
            }
            Some(Synced::Secured) => {
                if self.session.lock().unwrap().take().is_some() {
                    let _ = self.record(Event {
                        detail: Some("its security changed on another machine".into()),
                        ..Event::new(&Actor::termf(), Action::Lock)
                    });
                }
                if let (Some(told), Ok(status)) = (&self.on_lock, self.status()) {
                    told(&status);
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Sets this machine's vault aside (kept as `vault.<id>.json`) and takes the synced one, another vault, in
    /// its place: it opens with its own password. Export this one first, to import it into the joined one.
    pub fn join_synced(&self, actor: &Actor) -> Result<Status> {
        let lock = self.exclusive()?;
        let file = self.read()?;
        let (_, mut theirs) = self
            .incoming()?
            .context("no vault synced from another machine")?;
        ensure!(theirs.id != file.id, "this vault is the synced one already");
        ensure!(
            theirs.format <= FORMAT,
            "this vault was written by a newer TermF"
        );
        let aside = format!("vault.{}.json", file.id);
        self.record(Event {
            detail: Some(format!(
                "joined the synced vault {}; this machine's is kept as vault/{aside}",
                theirs.id
            )),
            ..Event::new(actor, Action::Merge)
        })?;
        *self.session.lock().unwrap() = None;
        fs::rename(self.dir.join(FILE), self.dir.join(&aside))?;
        let _ = self.store.delete(&file.id);
        self.drop_quick(&file.id);
        theirs.remembered = false;
        self.write(&theirs)?;
        fs::remove_file(self.incoming_path())?;
        drop(lock);
        self.status()
    }

    /// The other versions a merge kept beside an element (an item's id, or `secret:<scope>:<project>:<name>`).
    pub fn conflicts(&self, key: &str) -> Result<Vec<ConflictInfo>> {
        let content = self.open()?;
        let item = content.items.iter().find(|i| i.id == key);
        let secret = content.secrets.iter().find(|s| s.key() == key);
        Ok(content
            .conflicts
            .iter()
            .filter(|c| c.key() == key)
            .map(|c| ConflictInfo {
                id: c.id.clone(),
                at: c.at,
                item: c.item.as_ref().map(Item::info),
                secret: c.secret.as_ref().map(Stored::info),
                differs: match (&c.item, item, &c.secret, secret) {
                    (Some(a), Some(b), _, _) => item_differs(a, b),
                    (_, _, Some(a), Some(b)) => secret_differs(a, b),
                    _ => Vec::new(),
                },
            })
            .collect())
    }

    /// Keeps one version of an element in conflict (`keep`: another version's id; `None`: the current one),
    /// which then supersedes every other, here and on the other machines.
    pub fn resolve(&self, actor: &Actor, key: &str, keep: Option<&str>) -> Result<()> {
        let content = self.open()?;
        ensure!(content.conflicts_of(key) > 0, "no such conflict");
        let target = content
            .secrets
            .iter()
            .find(|s| s.key() == key)
            .map(|s| SecretId {
                name: s.name.clone(),
                scope: s.scope,
                project: s.project.clone(),
            });
        drop(content);
        if let Some(id) = &target {
            self.prod_gate(actor, id)?;
        }
        let event = Event {
            item: target.is_none().then(|| key.to_owned()),
            target: target.clone(),
            detail: Some("conflict resolved".into()),
            ..Event::new(actor, Action::Write)
        };
        let now = self.clock.wall();
        self.modify(event, |content| {
            if let Some(i) = Content::resolve(&mut content.items, &mut content.conflicts, key, keep)
            {
                content.items[i].updated = now;
            } else if let Some(i) =
                Content::resolve(&mut content.secrets, &mut content.conflicts, key, keep)
            {
                content.secrets[i].updated = now;
            } else {
                bail!("no such conflict");
            }
            Ok(())
        })
    }
}
