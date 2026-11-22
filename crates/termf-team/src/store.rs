//! Team runs in `termf.db`, next to the runs they work on: one row per run holding its whole state (only the
//! app writes it, under the engine's lock), and their events, also written to the run's `journal.md`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{TeamEvent, TeamRun, now};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS team_runs (
    run INTEGER PRIMARY KEY,
    state TEXT NOT NULL,
    data TEXT NOT NULL,
    updated INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS team_events (
    id INTEGER PRIMARY KEY,
    run INTEGER NOT NULL,
    at INTEGER NOT NULL,
    step TEXT,
    kind TEXT NOT NULL,
    message TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS team_events_by_run ON team_events(run, id);
CREATE TABLE IF NOT EXISTS team_decisions (
    id INTEGER PRIMARY KEY,
    project TEXT NOT NULL,
    run INTEGER NOT NULL,
    step TEXT NOT NULL,
    by TEXT NOT NULL,
    text TEXT NOT NULL,
    replaces INTEGER,
    at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS team_decisions_by_project ON team_decisions(project, id);
";

/// A decision of a project's journal: a ruling of the CTO, or what the user decided at a gate, kept for the runs to
/// come. Replaced only by a later decision that says so.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    /// Shown as `D<id>`.
    pub id: i64,
    pub project: String,
    /// The run and the step where it was made.
    pub run: i64,
    pub step: String,
    /// `cto` or `you`.
    pub by: String,
    pub text: String,
    pub replaces: Option<i64>,
    pub replaced_by: Option<i64>,
    /// Seconds since the Unix epoch.
    pub at: i64,
}

pub(crate) struct Store {
    conn: Connection,
    data_dir: PathBuf,
}

/// Every team run, newest first. Reads the database only: TermF need not run.
pub fn runs(data_dir: &Path) -> Result<Vec<TeamRun>> {
    Store::open(data_dir)?.runs()
}

pub fn run(data_dir: &Path, id: i64) -> Result<TeamRun> {
    Store::open(data_dir)?
        .get(id)?
        .with_context(|| format!("run {id} has no team"))
}

/// A team run's events after the event `after` (0: all).
pub fn events(data_dir: &Path, run: i64, after: i64) -> Result<Vec<TeamEvent>> {
    Store::open(data_dir)?.events(run, after)
}

/// A project's decisions, newest first, each with the one that replaced it. Reads the database only.
pub fn decisions(data_dir: &Path, project: &str) -> Result<Vec<Decision>> {
    Store::open(data_dir)?.decisions(project)
}

impl Store {
    pub(crate) fn open(data_dir: &Path) -> Result<Self> {
        fs::create_dir_all(data_dir)
            .with_context(|| format!("cannot create {}", data_dir.display()))?;
        let file = data_dir.join("termf.db");
        let conn =
            Connection::open(&file).with_context(|| format!("cannot open {}", file.display()))?;
        conn.busy_timeout(Duration::from_secs(5))?;
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            conn,
            data_dir: data_dir.into(),
        })
    }

    pub(crate) fn get(&self, run: i64) -> Result<Option<TeamRun>> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM team_runs WHERE run = ?1", [run], |r| {
                r.get(0)
            })
            .optional()?;
        data.map(|d| serde_json::from_str(&d).context("unreadable team run"))
            .transpose()
    }

    pub(crate) fn runs(&self) -> Result<Vec<TeamRun>> {
        let mut query = self
            .conn
            .prepare("SELECT data FROM team_runs ORDER BY run DESC")?;
        let rows = query.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|d| Ok(serde_json::from_str(&d?)?)).collect()
    }

    pub(crate) fn save(&self, run: &mut TeamRun) -> Result<()> {
        run.updated = now();
        self.conn.execute(
            "INSERT INTO team_runs (run, state, data, updated) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(run) DO UPDATE SET state = ?2, data = ?3, updated = ?4",
            params![
                run.run,
                run.state.as_str(),
                serde_json::to_string(run)?,
                run.updated
            ],
        )?;
        Ok(())
    }

    pub(crate) fn record(
        &self,
        run: i64,
        step: Option<&str>,
        kind: &str,
        message: &str,
    ) -> Result<TeamEvent> {
        let at = now();
        self.conn.execute(
            "INSERT INTO team_events (run, at, step, kind, message) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![run, at, step, kind, message],
        )?;
        let event = TeamEvent {
            id: self.conn.last_insert_rowid(),
            run,
            at,
            step: step.map(str::to_owned),
            kind: kind.to_owned(),
            message: message.to_owned(),
        };
        // The run's own journal, when the run has one.
        let journal = self
            .data_dir
            .join("runs")
            .join(run.to_string())
            .join("journal.md");
        if journal.is_file() {
            let step = step.map(|s| format!(" {s}")).unwrap_or_default();
            let line = format!("- {}: team{step}: {message}\n", termf_tickets::utc(at));
            let _ = OpenOptions::new()
                .append(true)
                .open(&journal)
                .and_then(|mut f| f.write_all(line.as_bytes()));
        }
        Ok(event)
    }

    pub(crate) fn decide(
        &self,
        project: &str,
        run: i64,
        step: &str,
        by: &str,
        text: &str,
        replaces: Option<i64>,
    ) -> Result<Decision> {
        let at = now();
        self.conn.execute(
            "INSERT INTO team_decisions (project, run, step, by, text, replaces, at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![project, run, step, by, text, replaces, at],
        )?;
        Ok(Decision {
            id: self.conn.last_insert_rowid(),
            project: project.to_owned(),
            run,
            step: step.to_owned(),
            by: by.to_owned(),
            text: text.to_owned(),
            replaces,
            replaced_by: None,
            at,
        })
    }

    pub(crate) fn decisions(&self, project: &str) -> Result<Vec<Decision>> {
        let mut query = self.conn.prepare(
            "SELECT id, run, step, by, text, replaces, at FROM team_decisions WHERE project = ?1 ORDER BY id DESC",
        )?;
        let rows = query.query_map([project], |r| {
            Ok(Decision {
                id: r.get(0)?,
                project: project.to_owned(),
                run: r.get(1)?,
                step: r.get(2)?,
                by: r.get(3)?,
                text: r.get(4)?,
                replaces: r.get(5)?,
                replaced_by: None,
                at: r.get(6)?,
            })
        })?;
        let mut list: Vec<Decision> = rows.collect::<rusqlite::Result<_>>()?;
        let replaced: Vec<(i64, i64)> = list
            .iter()
            .filter_map(|d| Some((d.replaces?, d.id)))
            .collect();
        for d in &mut list {
            d.replaced_by = replaced
                .iter()
                .find(|(old, _)| *old == d.id)
                .map(|(_, by)| *by);
        }
        Ok(list)
    }

    fn events(&self, run: i64, after: i64) -> Result<Vec<TeamEvent>> {
        let mut query = self.conn.prepare(
            "SELECT id, at, step, kind, message FROM team_events WHERE run = ?1 AND id > ?2 ORDER BY id",
        )?;
        let rows = query.query_map(params![run, after], |r| {
            Ok(TeamEvent {
                id: r.get(0)?,
                run,
                at: r.get(1)?,
                step: r.get(2)?,
                kind: r.get(3)?,
                message: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
