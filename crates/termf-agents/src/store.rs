//! Headless sessions and their events, in `termf.db` next to the runs, and in `agents/<id>/events.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};

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

const COLUMNS: &str = "id, runtime, session_id, spec, state, cost, context, created, updated";

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

impl Store {
    pub(crate) fn open(data_dir: &Path) -> Result<Self> {
        fs::create_dir_all(data_dir)
            .with_context(|| format!("cannot create {}", data_dir.display()))?;
        let file = data_dir.join("termf.db");
        let conn =
            Connection::open(&file).with_context(|| format!("cannot open {}", file.display()))?;
        // The app and the CLI may write at the same time.
        conn.busy_timeout(Duration::from_secs(5))?;
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.execute_batch(SCHEMA)?;
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

    /// No process outlives the app that started it.
    pub(crate) fn stop_all(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE agent_sessions SET state = 'stopped' WHERE state IN ('working', 'waiting', 'idle', 'interactive')",
            [],
        )?;
        Ok(())
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
    })
}
