//! The vault's audit log (ARCHITECTURE §12.5): append-only, in `termf.db`, each entry holding the SHA-256 of the
//! entry before it, so [`Log::verify`] finds an entry changed, removed or inserted. It cannot tell the last
//! entries removed, nor a whole chain written again: that would need its head kept somewhere else.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Scope, SecretId};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS vault_audit (
    id INTEGER PRIMARY KEY,
    -- The whole entry as JSON, its hash included: what verify checks.
    entry TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS vault_audit_no_update BEFORE UPDATE ON vault_audit
BEGIN SELECT RAISE(ABORT, 'the vault audit is append-only'); END;
CREATE TRIGGER IF NOT EXISTS vault_audit_no_delete BEFORE DELETE ON vault_audit
BEGIN SELECT RAISE(ABORT, 'the vault audit is append-only'); END;
";

/// What the first entry holds as the hash before it.
pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    Human,
    Agent,
    /// TermF itself: the automatic lock.
    Termf,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub kind: ActorKind,
    /// The ticket run an agent works on.
    pub run: Option<i64>,
    /// The TermF session (tile or headless agent) it came from.
    pub session: Option<String>,
}

impl Actor {
    pub fn human() -> Self {
        Self {
            kind: ActorKind::Human,
            run: None,
            session: None,
        }
    }

    pub fn agent(run: Option<i64>, session: Option<String>) -> Self {
        Self {
            kind: ActorKind::Agent,
            run,
            session,
        }
    }

    pub fn termf() -> Self {
        Self {
            kind: ActorKind::Termf,
            ..Self::human()
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Init,
    Unlock,
    UnlockFailed,
    /// `detail`: `idle` or `sleep` when TermF locked it.
    Lock,
    Reauth,
    ReauthFailed,
    Write,
    Reference,
    Delete,
    /// The value shown to the user.
    Reveal,
    /// The value used on the user's or an agent's behalf, never shown.
    Read,
    /// A reference resolved by its manager's CLI; `detail`: `reveal` or `use`, and the URI.
    Resolve,
    /// Asked and not done; `detail` says why.
    Refused,
    /// How the vault opens changed; `detail`: `security key added <id>`, `security key removed <id>`,
    /// `password changed`, `recovery key renewed`, `biometry on, <n> days` or `biometry off`.
    Security,
}

/// What [`Log::append`] records.
#[derive(Debug, Clone)]
pub struct Event {
    pub actor: Actor,
    pub action: Action,
    pub target: Option<SecretId>,
    /// A password manager item's id.
    pub item: Option<String>,
    /// Bytes of the value read or written.
    pub size: Option<u64>,
    /// How many values were masked in an output.
    pub masks: Option<u64>,
    pub detail: Option<String>,
}

impl Event {
    pub fn new(actor: &Actor, action: Action) -> Self {
        Self {
            actor: actor.clone(),
            action,
            target: None,
            item: None,
            size: None,
            masks: None,
            detail: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    /// Unix seconds.
    pub at: u64,
    pub actor: Actor,
    pub action: Action,
    /// The secret's name.
    pub target: Option<String>,
    pub scope: Option<Scope>,
    pub project: Option<String>,
    /// A password manager item's id. Left out when empty, so the entries written before it hash the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub size: Option<u64>,
    pub masks: Option<u64>,
    pub detail: Option<String>,
    /// The hash of the entry before.
    pub prev: String,
    /// SHA-256 of this entry as JSON with an empty `hash`.
    pub hash: String,
}

impl Entry {
    fn digest(&self) -> String {
        let unhashed = Entry {
            hash: String::new(),
            ..self.clone()
        };
        let json = serde_json::to_vec(&unhashed).expect("an entry is always JSON");
        termf_sync::crypto::hex(&Sha256::digest(json))
    }
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    pub ok: bool,
    pub entries: u64,
    /// The first entry that does not hold, and why.
    pub broken: Option<i64>,
    pub error: Option<String>,
}

pub(crate) struct Log {
    conn: Connection,
}

impl Log {
    pub(crate) fn open(data_dir: &Path) -> Result<Self> {
        fs::create_dir_all(data_dir)
            .with_context(|| format!("cannot create {}", data_dir.display()))?;
        let file = data_dir.join("termf.db");
        let conn =
            Connection::open(&file).with_context(|| format!("cannot open {}", file.display()))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// One writer at a time (app and CLI alike), so the chain never forks.
    pub(crate) fn append(&mut self, at: u64, event: Event) -> Result<Entry> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let last: Option<(i64, String)> = tx
            .query_row(
                "SELECT id, entry FROM vault_audit ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (id, prev) = match last {
            Some((id, entry)) => {
                let entry: Entry = serde_json::from_str(&entry)
                    .context("the vault audit is damaged (termf vault audit verify)")?;
                (id + 1, entry.hash)
            }
            None => (1, GENESIS.into()),
        };
        let target = event.target;
        let mut entry = Entry {
            id,
            at,
            actor: event.actor,
            action: event.action,
            target: target.as_ref().map(|t| t.name.clone()),
            scope: target.as_ref().map(|t| t.scope),
            project: target.and_then(|t| t.project),
            item: event.item,
            size: event.size,
            masks: event.masks,
            detail: event.detail,
            prev,
            hash: String::new(),
        };
        entry.hash = entry.digest();
        tx.execute(
            "INSERT INTO vault_audit (id, entry) VALUES (?1, ?2)",
            (id, serde_json::to_string(&entry)?),
        )?;
        tx.commit()?;
        Ok(entry)
    }

    /// The newest first.
    pub(crate) fn list(&self, limit: usize) -> Result<Vec<Entry>> {
        let mut query = self
            .conn
            .prepare("SELECT entry FROM vault_audit ORDER BY id DESC LIMIT ?1")?;
        let rows = query.query_map([limit as i64], |r| r.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    pub(crate) fn verify(&self) -> Result<Verification> {
        let mut query = self
            .conn
            .prepare("SELECT id, entry FROM vault_audit ORDER BY id")?;
        let rows = query.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let (mut prev, mut expected, mut entries) = (GENESIS.to_owned(), 1, 0);
        for row in rows {
            let (id, text) = row?;
            let error = match serde_json::from_str::<Entry>(&text) {
                Err(_) => Some("not an audit entry"),
                Ok(_) if id != expected => Some("an entry is missing before it"),
                Ok(e) if e.id != id => Some("its number was changed"),
                Ok(e) if e.prev != prev => Some("the entry before it was changed or removed"),
                Ok(e) if e.digest() != e.hash => Some("it was changed"),
                Ok(e) => {
                    prev = e.hash;
                    None
                }
            };
            if let Some(error) = error {
                return Ok(Verification {
                    ok: false,
                    entries,
                    broken: Some(id),
                    error: Some(error.into()),
                });
            }
            expected += 1;
            entries += 1;
        }
        Ok(Verification {
            ok: true,
            entries,
            broken: None,
            error: None,
        })
    }

    pub(crate) fn export(&self, out: &mut dyn Write) -> Result<u64> {
        let mut query = self
            .conn
            .prepare("SELECT entry FROM vault_audit ORDER BY id")?;
        let mut count = 0;
        for row in query.query_map([], |r| r.get::<_, String>(0))? {
            writeln!(out, "{}", row?)?;
            count += 1;
        }
        Ok(count)
    }
}
