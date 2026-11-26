//! Headless sessions and their events, in `termf.db` next to the runs, and in `agents/<id>/events.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use crate::{AgentEvent, AgentSession, SessionSpec, State, StoredEvent, now};

const SCHEMA: &str = "
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS agent_sessions (
    id INTEGER PRIMARY KEY,
    runtime TEXT NOT NULL,
    session_id TEXT NOT NULL,
    spec TEXT NOT NULL,
    state TEXT NOT NULL,
    cost REAL NOT NULL,
    context TEXT,
    -- The process group of its headless process, until it ends.
    pid INTEGER,
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS agent_events (
    id INTEGER PRIMARY KEY,
    session INTEGER NOT NULL REFERENCES agent_sessions(id),
    at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS agent_events_by_session ON agent_events(session, id);
";

const COLUMNS: &str =
    "id, runtime, session_id, spec, state, cost, context, created, updated, archived";

pub(crate) struct Store {
    conn: Connection,
    data_dir: PathBuf,
}

/// Every headless session, newest first. Reads the database only: TermF need not run.
pub fn sessions(data_dir: &Path) -> Result<Vec<AgentSession>> {
    Store::open(data_dir)?.sessions()
}

/// A session's stored events after the event `after` (0: all).
pub fn events(data_dir: &Path, session: i64, after: i64) -> Result<Vec<StoredEvent>> {
    Store::open(data_dir)?.events(session, after)
}

/// A conversation whose messages hold the text searched, with the passage around its latest mention.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Found {
    pub session: i64,
    pub text: String,
}

/// The conversations, archived ones included, whose messages (the user's and the agent's) hold `query`, case aside:
/// the latest mention first, `limit` of them at most. Read from the stored events on demand, without an index.
pub fn search(data_dir: &Path, query: &str, limit: usize) -> Result<Vec<Found>> {
    Store::open(data_dir)?.search(query, limit)
}

impl Store {
    pub(crate) fn open(data_dir: &Path) -> Result<Self> {
        fs::create_dir_all(data_dir)
            .with_context(|| format!("cannot create {}", data_dir.display()))?;
        let file = data_dir.join("termf.db");
        let conn = termf_core::db::open(&file)?;
        conn.execute_batch(SCHEMA)?;
        // Added after the table: an older database reads as nothing archived.
        let has = || -> rusqlite::Result<bool> {
            conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('agent_sessions') WHERE name = 'archived'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
        };
        // The app and the CLI may both add it: the one that loses finds it there.
        if !has()?
            && let Err(e) = conn.execute(
                "ALTER TABLE agent_sessions ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
                [],
            )
            && !has()?
        {
            return Err(e.into());
        }
        Ok(Self {
            conn,
            data_dir: data_dir.into(),
        })
    }

    pub(crate) fn dir(&self, id: i64) -> PathBuf {
        self.data_dir.join("agents").join(id.to_string())
    }

    pub(crate) fn create(
        &self,
        runtime: &str,
        session_id: &str,
        spec: &SessionSpec,
    ) -> Result<AgentSession> {
        self.conn.execute(
            "INSERT INTO agent_sessions (runtime, session_id, spec, state, cost, created, updated)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)",
            params![
                runtime,
                session_id,
                serde_json::to_string(spec)?,
                State::Stopped.as_str(),
                now()
            ],
        )?;
        self.session(self.conn.last_insert_rowid())
    }

    pub(crate) fn session(&self, id: i64) -> Result<AgentSession> {
        self.conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM agent_sessions WHERE id = ?1"),
                [id],
                read,
            )
            .optional()?
            .with_context(|| format!("no agent session {id}"))
    }

    fn sessions(&self) -> Result<Vec<AgentSession>> {
        let mut query = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM agent_sessions ORDER BY id DESC"
        ))?;
        let rows = query.query_map([], read)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub(crate) fn update(&self, session: &mut AgentSession) -> Result<()> {
        session.updated = now();
        let context = session
            .context
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.conn.execute(
            "UPDATE agent_sessions SET session_id = ?2, state = ?3, cost = ?4, context = ?5, updated = ?6 WHERE id = ?1",
            params![session.id, session.session_id, session.state.as_str(), session.cost, context, session.updated],
        )?;
        Ok(())
    }

    pub(crate) fn set_archived(&self, id: i64, archived: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE agent_sessions SET archived = ?2 WHERE id = ?1",
            params![id, archived],
        )?;
        Ok(())
    }

    /// Nothing was ever said in it.
    pub(crate) fn empty(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM agent_events WHERE session = ?1 LIMIT 1",
                [id],
                |_| Ok(()),
            )
            .optional()?
            .is_none())
    }

    /// A session, its stored events and its folder (`events.jsonl`, what its processes were given).
    pub(crate) fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM agent_events WHERE session = ?1", [id])?;
        self.conn
            .execute("DELETE FROM agent_sessions WHERE id = ?1", [id])?;
        match fs::remove_dir_all(self.dir(id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }

    pub(crate) fn set_spec(&self, id: i64, spec: &SessionSpec) -> Result<()> {
        self.conn.execute(
            "UPDATE agent_sessions SET spec = ?2 WHERE id = ?1",
            params![id, serde_json::to_string(spec)?],
        )?;
        Ok(())
    }

    /// The app starts: no session has a process it can talk to.
    pub(crate) fn stop_all(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE agent_sessions SET state = 'stopped' WHERE state IN ('working', 'waiting', 'idle', 'interactive')",
            [],
        )?;
        Ok(())
    }

    pub(crate) fn set_pid(&self, id: i64, pid: Option<u32>) -> Result<()> {
        self.conn.execute(
            "UPDATE agent_sessions SET pid = ?2 WHERE id = ?1",
            params![id, pid],
        )?;
        Ok(())
    }

    /// Forgets `pid` once it ended, unless a newer process of the session replaced it.
    pub(crate) fn clear_pid(&self, id: i64, pid: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE agent_sessions SET pid = NULL WHERE id = ?1 AND pid = ?2",
            params![id, pid],
        )?;
        Ok(())
    }

    pub(crate) fn pid(&self, id: i64) -> Result<Option<u32>> {
        Ok(self
            .conn
            .query_row("SELECT pid FROM agent_sessions WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?
            .flatten())
    }

    /// Each session with a process, and its conversation id.
    pub(crate) fn pids(&self) -> Result<Vec<(i64, u32, String)>> {
        let mut query = self
            .conn
            .prepare("SELECT id, pid, session_id FROM agent_sessions WHERE pid IS NOT NULL")?;
        let rows = query.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Whether the agent took a message, so its conversation exists and can be resumed.
    pub(crate) fn started(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM agent_events WHERE session = ?1 AND kind = 'init' LIMIT 1",
                [id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Saves `event` in the database and in `events.jsonl`.
    pub(crate) fn record(&self, session: i64, event: &AgentEvent) -> Result<StoredEvent> {
        let at = now();
        let data = serde_json::to_value(event)?;
        self.conn.execute(
            "INSERT INTO agent_events (session, at, kind, data) VALUES (?1, ?2, ?3, ?4)",
            params![session, at, data["kind"].as_str(), data.to_string()],
        )?;
        let stored = StoredEvent {
            id: self.conn.last_insert_rowid(),
            session,
            at,
            event: event.clone(),
        };
        let dir = self.dir(session);
        fs::create_dir_all(&dir)?;
        let file = dir.join("events.jsonl");
        let line = serde_json::to_string(&stored)?;
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .and_then(|mut f| writeln!(f, "{line}"))
            .with_context(|| format!("cannot write {}", file.display()))?;
        Ok(stored)
    }

    fn events(&self, session: i64, after: i64) -> Result<Vec<StoredEvent>> {
        let mut query = self.conn.prepare(
            "SELECT id, at, data FROM agent_events WHERE session = ?1 AND id > ?2 ORDER BY id",
        )?;
        let rows = query.query_map([session, after], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, at, data) = row?;
            Ok(StoredEvent {
                id,
                session,
                at,
                event: serde_json::from_str(&data)?,
            })
        })
        .collect()
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<Found>> {
        let needle = query.trim().to_lowercase();
        let mut found: Vec<Found> = Vec::new();
        if needle.is_empty() || limit == 0 {
            return Ok(found);
        }
        let mut select = self.conn.prepare(
            "SELECT session, json_extract(data, '$.text') FROM agent_events
             WHERE kind IN ('user', 'text') ORDER BY id DESC",
        )?;
        let mut rows = select.query([])?;
        while let Some(row) = rows.next()? {
            let session: i64 = row.get(0)?;
            if found.iter().any(|f| f.session == session) {
                continue;
            }
            let Some(text) = row.get::<_, Option<String>>(1)? else {
                continue;
            };
            if let Some(text) = passage(&text, &needle) {
                found.push(Found { session, text });
                if found.len() == limit {
                    break;
                }
            }
        }
        Ok(found)
    }
}

/// The words around the first mention of `needle` (lowercase) in `text`, on one line; None when it is not there.
fn passage(text: &str, needle: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let at = lower.find(needle)?;
    // Lowercasing seldom changes a text's length in characters: counted on it, the passage may only shift a little.
    let start = lower[..at].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let from = start.saturating_sub(40).min(chars.len());
    let to = (start + needle.chars().count() + 80).min(chars.len());
    let words = chars[from..to]
        .iter()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let before = if from > 0 { "…" } else { "" };
    let after = if to < chars.len() { "…" } else { "" };
    Some(format!("{before}{words}{after}"))
}

fn json<T: serde::de::DeserializeOwned>(column: usize, text: String) -> rusqlite::Result<T> {
    serde_json::from_str(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, e.into())
    })
}

fn read(row: &Row) -> rusqlite::Result<AgentSession> {
    let state: String = row.get(4)?;
    Ok(AgentSession {
        id: row.get(0)?,
        runtime: row.get(1)?,
        session_id: row.get(2)?,
        spec: json(3, row.get(3)?)?,
        state: State::parse(&state).unwrap_or_default(),
        cost: row.get(5)?,
        context: row
            .get::<_, Option<String>>(6)?
            .map(|t| json(6, t))
            .transpose()?,
        tile: None,
        created: row.get(7)?,
        updated: row.get(8)?,
        archived: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_the_latest_mention_of_each_conversation() {
        let root = std::env::temp_dir().join(format!("termf-agents-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = Store::open(&root).unwrap();
        let spec = SessionSpec::default();
        let a = store.create("claude", "a", &spec).unwrap().id;
        let b = store.create("claude", "b", &spec).unwrap().id;
        let say = |session, text: String| {
            let user = AgentEvent::User { text, images: 0 };
            store.record(session, &user).unwrap();
        };
        say(a, "Deploy the API to staging".into());
        let answer = AgentEvent::Text {
            text: "Le déploiement\nest fait.".into(),
            parent: None,
        };
        store.record(a, &answer).unwrap();
        say(b, format!("{}then deploy", "word ".repeat(30)));
        store.set_archived(b, true).unwrap();

        let found = search(&root, "  DEPLOY ", 10).unwrap();
        assert_eq!(found.iter().map(|f| f.session).collect::<Vec<_>>(), [b, a]);
        assert!(
            found[0].text.starts_with('…') && found[0].text.ends_with("then deploy"),
            "{found:?}"
        );
        assert_eq!(found[1].text, "Deploy the API to staging");
        assert_eq!(
            search(&root, "DÉPLOIEMENT", 10).unwrap()[0].text,
            "Le déploiement est fait."
        );
        assert_eq!(search(&root, "deploy", 1).unwrap().len(), 1);
        assert!(search(&root, " ", 10).unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
