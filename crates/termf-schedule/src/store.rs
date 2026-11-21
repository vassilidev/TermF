//! The runs of scheduled tasks, in TermF's SQLite database next to the ticket runs. A run is claimed before it
//! starts: one row per task and scheduled time, so the app and the system's scheduler never both run it.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{Mode, Task};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS schedule_runs (
    id INTEGER PRIMARY KEY,
    task TEXT NOT NULL,
    name TEXT NOT NULL,
    trigger TEXT NOT NULL,
    slot INTEGER,
    mode TEXT NOT NULL,
    status TEXT NOT NULL,
    host TEXT NOT NULL,
    pid INTEGER,
    started INTEGER NOT NULL,
    finished INTEGER,
    cost REAL,
    output TEXT,
    error TEXT,
    UNIQUE (task, slot)
);
CREATE INDEX IF NOT EXISTS schedule_runs_by_task ON schedule_runs(task, id);
CREATE TABLE IF NOT EXISTS schedule_state (
    task TEXT PRIMARY KEY,
    cron TEXT NOT NULL,
    since INTEGER NOT NULL
);
";

const COLUMNS: &str =
    "id, task, name, trigger, slot, mode, status, started, finished, cost, output, error";

/// What started a run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Trigger {
    /// Its scheduled time.
    Schedule,
    /// A scheduled time missed while TermF and the computer were off or asleep.
    Catchup,
    /// Someone asked for it.
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Running,
    Succeeded,
    Failed,
    /// An interactive session opened in a tile; it goes on there.
    Opened,
}

/// Who runs it: the app, or the CLI the system's scheduler started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    App,
    Cli,
}

impl Host {
    fn as_str(self) -> &'static str {
        match self {
            Host::App => "app",
            Host::Cli => "cli",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Execution {
    pub id: i64,
    pub task: String,
    /// The task's name when it ran.
    pub name: String,
    pub trigger: Trigger,
    /// The scheduled time it ran for; `None` when asked by hand.
    pub slot: Option<i64>,
    pub mode: Mode,
    pub status: Status,
    /// Seconds since the Unix epoch.
    pub started: i64,
    pub finished: Option<i64>,
    /// US dollars, as Claude Code counts them.
    pub cost: Option<f64>,
    /// Claude Code's answer (headless runs).
    pub output: Option<String>,
    pub error: Option<String>,
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub status: Status,
    pub cost: Option<f64>,
    pub output: Option<String>,
    pub error: Option<String>,
}

pub(crate) struct Store {
    conn: Connection,
}

/// Enums are stored as their JSON names.
fn text<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn parse<T: DeserializeOwned>(row: &Row, i: usize) -> rusqlite::Result<T> {
    let text: String = row.get(i)?;
    serde_json::from_value(text.into()).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn read(row: &Row) -> rusqlite::Result<Execution> {
    Ok(Execution {
        id: row.get(0)?,
        task: row.get(1)?,
        name: row.get(2)?,
        trigger: parse(row, 3)?,
        slot: row.get(4)?,
        mode: parse(row, 5)?,
        status: parse(row, 6)?,
        started: row.get(7)?,
        finished: row.get(8)?,
        cost: row.get(9)?,
        output: row.get(10)?,
        error: row.get(11)?,
    })
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
        Ok(Self { conn })
    }

    /// Since when the task has been on with its current schedule: only times after that are due.
    /// `None` when it is off.
    pub(crate) fn track(&self, id: &str, task: &Task, now: i64) -> Result<Option<i64>> {
        if !task.enabled {
            self.forget(id)?;
            return Ok(None);
        }
        let known: Option<(String, i64)> = self
            .conn
            .query_row(
                "SELECT cron, since FROM schedule_state WHERE task = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match known {
            Some((cron, since)) if cron == task.cron => Ok(Some(since)),
            _ => {
                self.conn.execute(
                    "INSERT OR REPLACE INTO schedule_state (task, cron, since) VALUES (?1, ?2, ?3)",
                    params![id, task.cron, now],
                )?;
                Ok(Some(now))
            }
        }
    }

    pub(crate) fn forget(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM schedule_state WHERE task = ?1", [id])?;
        Ok(())
    }

    /// A new running run, or `None` when this scheduled time already has one.
    pub(crate) fn claim(
        &self,
        id: &str,
        task: &Task,
        trigger: Trigger,
        slot: Option<i64>,
        host: Host,
        now: i64,
    ) -> Result<Option<Execution>> {
        let added = self.conn.execute(
            "INSERT INTO schedule_runs (task, name, trigger, slot, mode, status, host, started)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT DO NOTHING",
            params![
                id,
                task.name,
                text(trigger),
                slot,
                text(task.mode),
                text(Status::Running),
                host.as_str(),
                now
            ],
        )?;
        if added == 0 {
            return Ok(None);
        }
        self.get(self.conn.last_insert_rowid()).map(Some)
    }

    pub(crate) fn finish(&self, run: i64, outcome: &Outcome, now: i64) -> Result<Execution> {
        self.conn.execute(
            "UPDATE schedule_runs SET status = ?2, finished = ?3, cost = ?4, output = ?5, error = ?6 WHERE id = ?1",
            params![
                run,
                text(outcome.status),
                now,
                outcome.cost,
                outcome.output,
                outcome.error
            ],
        )?;
        self.get(run)
    }

    /// The process a headless run runs in, to tell later whether it is still going.
    pub(crate) fn set_pid(&self, run: i64, pid: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE schedule_runs SET pid = ?2 WHERE id = ?1",
            params![run, pid],
        )?;
        Ok(())
    }

    /// Fails the runs left running whose process is gone (their runner stopped during them), that never got one
    /// within two minutes, or that started longer ago than a run may last.
    pub(crate) fn interrupted(&self, older_than: i64, now: i64) -> Result<()> {
        let mut query = self
            .conn
            .prepare("SELECT id, pid, started FROM schedule_runs WHERE status = ?1")?;
        let rows = query.query_map([text(Status::Running)], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<u32>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut gone = Vec::new();
        for row in rows {
            let (id, pid, started) = row?;
            let dead = match pid {
                Some(pid) => !crate::run::alive(pid),
                None => started < now - 120,
            };
            if dead || started < older_than {
                gone.push(id);
            }
        }
        for id in gone {
            self.conn.execute(
                "UPDATE schedule_runs SET status = ?2, finished = ?3, error = 'interrupted before the end'
                 WHERE id = ?1 AND status = ?4",
                params![id, text(Status::Failed), now, text(Status::Running)],
            )?;
        }
        Ok(())
    }

    pub(crate) fn get(&self, run: i64) -> Result<Execution> {
        self.conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM schedule_runs WHERE id = ?1"),
                [run],
                read,
            )
            .optional()?
            .with_context(|| format!("no run {run}"))
    }

    /// Newest first.
    pub(crate) fn history(&self, task: Option<&str>, limit: usize) -> Result<Vec<Execution>> {
        let mut query = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM schedule_runs WHERE ?1 IS NULL OR task = ?1 ORDER BY id DESC LIMIT ?2"
        ))?;
        let rows = query.query_map(params![task, limit as i64], read)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
