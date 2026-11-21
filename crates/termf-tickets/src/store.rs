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

use crate::{Ticket, TicketRef, TicketSummary};

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
CREATE TABLE IF NOT EXISTS listed (
    project TEXT NOT NULL,
    source TEXT NOT NULL,
    tickets TEXT NOT NULL,
    fetched INTEGER NOT NULL,
    PRIMARY KEY (project, source)
);
PRAGMA user_version = 2;
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
    /// The terminal session of the run, while the app that opened it runs: never stored.
    #[serde(default)]
    pub session_id: Option<u32>,
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
    /// The delivered run moved its ticket to the state `to`.
    TicketMoved {
        to: String,
    },
    /// `dropped`: what a forced close threw away.
    Closed {
        dropped: Vec<String>,
    },
    Failed {
        error: String,
    },
    /// The worktree's preparation (ARCHITECTURE §9); `saved`: the project's recipe, not only detection.
    Setup {
        steps: Vec<termf_worktree::Step>,
        saved: bool,
    },
    /// Its database copy dropped and its ports given back.
    Teardown {
        steps: Vec<termf_worktree::Step>,
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
            Event::TicketMoved { to } => format!("ticket moved to {to}"),
            Event::Closed { dropped } if dropped.is_empty() => "closed, worktree removed".into(),
            Event::Closed { dropped } => {
                format!("closed by force, dropping {}", dropped.join(", "))
            }
            Event::Failed { error } => format!("failed: {error}"),
            Event::Setup { steps, .. } => format!("prepared the worktree: {}", steps_text(steps)),
            Event::Teardown { steps } => format!("cleaned up the worktree: {}", steps_text(steps)),
        }
    }
}

fn steps_text(steps: &[termf_worktree::Step]) -> String {
    match steps.is_empty() {
        true => "nothing to do".into(),
        false => steps
            .iter()
            .map(|s| s.describe())
            .collect::<Vec<_>>()
            .join("; "),
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

    /// What an MCP source of the project listed last time.
    pub(crate) fn cached(&self, project: &str, source: &str) -> Result<Option<Vec<TicketSummary>>> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT tickets FROM listed WHERE project = ?1 AND source = ?2",
                params![project, source],
                |r| r.get(0),
            )
            .optional()?;
        Ok(text.map(|t| serde_json::from_str(&t)).transpose()?)
    }

    pub(crate) fn cache(
        &self,
        project: &str,
        source: &str,
        tickets: &[TicketSummary],
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO listed (project, source, tickets, fetched) VALUES (?1, ?2, ?3, ?4)",
            params![project, source, serde_json::to_string(tickets)?, now()],
        )?;
        Ok(())
    }

    pub(crate) fn forget_cache(&self, project: &str, source: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM listed WHERE project = ?1 AND source = ?2",
            [project, source],
        )?;
        Ok(())
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
        session_id: None,
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
    let (year, month, day) = civil(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// The date `days` after 1970-01-01, by Howard Hinnant's days-to-civil conversion.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// The inverse of [`civil`].
fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A tracker's ISO 8601 date, such as Jira's `2026-11-20T10:00:00.000+0100`, as `2026-11-20T09:00:00Z`, so the
/// dates of every source sort as text. What does not read as such a date is kept as it is.
pub(crate) fn iso_utc(text: &str) -> String {
    let parsed = || -> Option<i64> {
        let (date, time) = text.trim().split_once('T')?;
        let mut parts = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
        let (year, month, day) = (parts.next()??, parts.next()??, parts.next()??);
        let clock: Vec<i64> = time
            .get(..8)?
            .split(':')
            .map(|p| p.parse().ok())
            .collect::<Option<_>>()?;
        let [hour, minute, second] = clock[..] else {
            return None;
        };
        let zone = time[8..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
        let offset = match zone {
            "Z" | "" => 0,
            _ => {
                let sign = match zone.as_bytes()[0] {
                    b'+' => 1,
                    b'-' => -1,
                    _ => return None,
                };
                let digits = zone[1..].replace(':', "");
                let hours: i64 = digits.get(..2)?.parse().ok()?;
                let minutes: i64 = digits.get(2..4)?.parse().ok()?;
                sign * (hours * 3600 + minutes * 60)
            }
        };
        let valid = (1..=12).contains(&month)
            && (1..=31).contains(&day)
            && hour < 24
            && minute < 60
            && second < 61;
        valid.then(|| days(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset)
    };
    match parsed() {
        Some(secs) => {
            let (year, month, day) = civil(secs.div_euclid(86_400));
            let rest = secs.rem_euclid(86_400);
            format!(
                "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
                rest / 3600,
                rest % 3600 / 60,
                rest % 60
            )
        }
        None => text.to_owned(),
    }
}
