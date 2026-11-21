//! Runs and their events: SQLite in WAL mode, shared by the app and the CLI, plus a folder per run with
//! `journal.md` for people and `events.jsonl` for tools.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use crate::{Ticket, TicketRef};

const SCHEMA: &str = "
PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS runs (
    id INTEGER PRIMARY KEY,
    project TEXT NOT NULL,
    provider TEXT NOT NULL,
    ticket_key TEXT NOT NULL,
    ticket_url TEXT NOT NULL,
    title TEXT NOT NULL,
    repo TEXT NOT NULL,
    worktree TEXT,
    branch TEXT,
    base TEXT,
    status TEXT NOT NULL,
    pr_url TEXT,
    created INTEGER NOT NULL,
    updated INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY,
    run INTEGER NOT NULL REFERENCES runs(id),
    at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS events_by_run ON events(run, id);
PRAGMA user_version = 1;
";

const RUN_COLUMNS: &str = "id, project, provider, ticket_key, ticket_url, title, repo, worktree, branch, base, status, pr_url, created, updated";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    /// Its worktree and session are open.
    Running,
    /// Its pull request is open; delivering again pushes what changed since.
    Delivered,
    /// Its worktree is gone.
    Closed,
    /// It could not start.
    Failed,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Delivered => "delivered",
            RunStatus::Closed => "closed",
            RunStatus::Failed => "failed",
        }
    }
}

impl FromSql for RunStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(match value.as_str()? {
            "running" => RunStatus::Running,
            "delivered" => RunStatus::Delivered,
            "closed" => RunStatus::Closed,
            "failed" => RunStatus::Failed,
            _ => return Err(FromSqlError::InvalidType),
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: i64,
    pub project: String,
    pub ticket: TicketRef,
    pub title: String,
    /// The repository the worktree comes from.
    pub repo: String,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    /// What the branch started from, such as `origin/main`; the pull request goes to that branch.
    pub base: Option<String>,
    pub status: RunStatus,
    pub pr_url: Option<String>,
    /// Seconds since the Unix epoch.
    pub created: i64,
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Event {
    WorktreeCreated {
        path: String,
        branch: String,
        base: String,
    },
    SessionOpened {
        session: u32,
    },
    Committed {
        commit: String,
        message: String,
    },
    Pushed {
        branch: String,
        commit: String,
    },
    PrOpened {
        url: String,
    },
    TicketCommented {
        url: String,
    },
    /// `dropped`: what a forced close threw away.
    Closed {
        dropped: Vec<String>,
    },
    Failed {
        error: String,
    },
}

impl Event {
    /// One line of the journal.
    pub fn describe(&self) -> String {
        let short = |commit: &str| commit[..commit.len().min(7)].to_owned();
        match self {
            Event::WorktreeCreated { path, branch, base } => {
                format!("worktree {path} on {branch}, from {base}")
            }
            Event::SessionOpened { session } => format!("Claude Code session {session} opened"),
            Event::Committed { commit, message } => format!(
                "committed {}: {}",
                short(commit),
                message.lines().next().unwrap_or_default()
            ),
            Event::Pushed { branch, commit } => format!("pushed {branch} at {}", short(commit)),
            Event::PrOpened { url } => format!("pull request opened: {url}"),
            Event::TicketCommented { url } => format!("commented {url} on the ticket"),
            Event::Closed { dropped } if dropped.is_empty() => "closed, worktree removed".into(),
            Event::Closed { dropped } => {
                format!("closed by force, dropping {}", dropped.join(", "))
            }
            Event::Failed { error } => format!("failed: {error}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RunEvent {
    pub id: i64,
    pub run: i64,
    /// Seconds since the Unix epoch.
    pub at: i64,
    #[serde(flatten)]
    pub event: Event,
}

pub(crate) struct Store {
    conn: Connection,
    data_dir: PathBuf,
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

    pub(crate) fn run_dir(&self, id: i64) -> PathBuf {
        self.data_dir.join("runs").join(id.to_string())
    }

    /// A new running run; its journal starts with the ticket.
    pub(crate) fn create(&self, project: &str, ticket: &Ticket, repo: &str) -> Result<Run> {
        let r = &ticket.reference;
        self.conn.execute(
            "INSERT INTO runs (project, provider, ticket_key, ticket_url, title, repo, status, created, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![project, r.provider, r.key, r.url, ticket.title, repo, RunStatus::Running.as_str(), now()],
        )?;
        let id = self.conn.last_insert_rowid();
        let dir = self.run_dir(id);
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join("journal.md"),
            format!("{}\n## Journal\n\n", ticket.markdown()),
        )?;
        self.run(id)
    }

    pub(crate) fn update(&self, run: &mut Run) -> Result<()> {
        run.updated = now();
        self.conn.execute(
            "UPDATE runs SET worktree = ?2, branch = ?3, base = ?4, status = ?5, pr_url = ?6, updated = ?7 WHERE id = ?1",
            params![run.id, run.worktree, run.branch, run.base, run.status.as_str(), run.pr_url, run.updated],
        )?;
        Ok(())
    }

    pub(crate) fn run(&self, id: i64) -> Result<Run> {
        self.conn
            .query_row(
                &format!("SELECT {RUN_COLUMNS} FROM runs WHERE id = ?1"),
                [id],
                read_run,
            )
            .optional()?
            .with_context(|| format!("no run {id}"))
    }

    /// Newest first.
    pub(crate) fn runs(&self) -> Result<Vec<Run>> {
        let mut query = self
            .conn
            .prepare(&format!("SELECT {RUN_COLUMNS} FROM runs ORDER BY id DESC"))?;
        let runs = query.query_map([], read_run)?;
        Ok(runs.collect::<rusqlite::Result<_>>()?)
    }

    /// Saves `event` in the database, `events.jsonl` and the journal.
    pub(crate) fn record(&self, run: i64, event: &Event) -> Result<RunEvent> {
        let at = now();
        let data = serde_json::to_value(event)?;
        self.conn.execute(
            "INSERT INTO events (run, at, kind, data) VALUES (?1, ?2, ?3, ?4)",
            params![run, at, data["kind"].as_str(), data.to_string()],
        )?;
        let recorded = RunEvent {
            id: self.conn.last_insert_rowid(),
            run,
            at,
            event: event.clone(),
        };
        let dir = self.run_dir(run);
        fs::create_dir_all(&dir)?;
        let line = serde_json::to_string(&recorded)?;
        append(&dir.join("events.jsonl"), &format!("{line}\n"))?;
        let entry = format!("- {}: {}\n", utc(at), event.describe());
        append(&dir.join("journal.md"), &entry)?;
        Ok(recorded)
    }

    pub(crate) fn events(&self, run: i64) -> Result<Vec<RunEvent>> {
        let mut query = self
            .conn
            .prepare("SELECT id, at, data FROM events WHERE run = ?1 ORDER BY id")?;
        let rows = query.query_map([run], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, at, data) = row?;
            Ok(RunEvent {
                id,
                run,
                at,
                event: serde_json::from_str(&data)?,
            })
        })
        .collect()
    }

    pub(crate) fn fail(&self, id: i64, error: &str) -> Result<Run> {
        let mut run = self.run(id)?;
        run.status = RunStatus::Failed;
        self.update(&mut run)?;
        self.record(
            id,
            &Event::Failed {
                error: error.into(),
            },
        )?;
        Ok(run)
    }
}

fn read_run(row: &Row) -> rusqlite::Result<Run> {
    Ok(Run {
        id: row.get(0)?,
        project: row.get(1)?,
        ticket: TicketRef {
            provider: row.get(2)?,
            key: row.get(3)?,
            url: row.get(4)?,
        },
        title: row.get(5)?,
        repo: row.get(6)?,
        worktree: row.get(7)?,
        branch: row.get(8)?,
        base: row.get(9)?,
        status: row.get(10)?,
        pr_url: row.get(11)?,
        created: row.get(12)?,
        updated: row.get(13)?,
    })
}

fn append(file: &Path, text: &str) -> Result<()> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .with_context(|| format!("cannot write {}", file.display()))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// `2026-11-21 09:30 UTC`.
pub fn utc(secs: i64) -> String {
    // Howard Hinnant's days-to-civil conversion.
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60
    )
}
