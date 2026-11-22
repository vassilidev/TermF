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

    /// The synced version waiting here, if any.
    fn incoming(&self) -> Result<Option<VaultFile>> {
        let bytes = match fs::read(self.incoming_path()) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            other => other?,
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| anyhow!("the vault synced from another machine is damaged"))
    }

    pub(crate) fn synced(&self, file: Option<&VaultFile>) -> Option<Synced> {
        let theirs = self.incoming().ok()??;
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
        let Some(mut file) = self.incoming()? else {
            return Ok(());
        };
        ensure!(
            file.format <= FORMAT,
            "this vault was written by a newer TermF"
        );
        // This machine's keychain keeps nothing for it.
        file.remembered = false;
        self.record(Event {
            detail: Some("synced from another machine".into()),
            ..Event::new(&Actor::termf(), Action::Merge)
        })?;
        self.write(&file)?;
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
    /// opens when that is newer and is what was just opened (`opened`). Says what changed; `None`: nothing to
    /// merge (none, or another vault).
    pub(crate) fn absorb(
        &self,
        actor: &Actor,
        data: &Key,
        opened: Option<&Opening>,
    ) -> Result<Option<Vec<Merged>>> {
        let _lock = self.exclusive()?;
        let Some(theirs) = self.incoming()? else {
            return Ok(None);
        };
        let mut file = self.read()?;
        if theirs.id != file.id || theirs.format > FORMAT {
            return Ok(None);
        }
        let newer = theirs.secured > file.secured;
        let opens_new = newer && !theirs.opens_like(&file);
        if opens_new && opened != Some(&theirs.opening()) {
            bail!("the vault changed on another machine meanwhile: unlock it again");
        }
        let incoming = theirs
            .open(data)
            .context("the vault synced from another machine does not open with this one's key")?;
        let mut content = file.open(data)?;
        let before = Zeroizing::new(serde_json::to_vec(&content)?);
        let rows = content.merge(incoming, self.clock.wall());
        let changed = *before != *Zeroizing::new(serde_json::to_vec(&content)?);
        let mut detail = format!("synced: {}", summary(&rows));
        if opens_new {
            detail.push_str("; opens as set on another machine");
        }
        let event = Event {
            detail: Some(detail),
            ..Event::new(actor, Action::Merge)
        };
        if changed || newer {
            if newer {
                file.set_opening(theirs.opening());
            }
            if opens_new && file.remembered {
                file.remembered = false;
                let _ = self.store.delete(&file.id);
            }
            let backup = self.back_up(&mut file)?;
            file.seal(data, &content)?;
            self.record(event)?;
            self.write(&file)?;
            if let Some(backup) = backup {
                self.drop_backup(&backup, data, &content)?;
            }
            if opens_new {
                self.drop_quick(&file.id);
            }
        } else {
            self.record(event)?;
        }
        fs::remove_file(self.incoming_path())?;
        Ok(Some(rows))
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
            Some(Synced::Pending) => self.absorb(actor, &material.data, None),
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
        let mut theirs = self
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
