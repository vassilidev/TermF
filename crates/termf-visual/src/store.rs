//! Reviews in `termf.db`, one row each holding the whole review (only the app writes it, under its lock); their
//! images under `<data>/reviews/<id>/`.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{Review, now};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS visual_reviews (
    id INTEGER PRIMARY KEY,
    run INTEGER,
    data TEXT NOT NULL,
    updated INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS visual_reviews_by_run ON visual_reviews(run);
";

pub(crate) struct Store {
    conn: Connection,
}

/// Every review, newest first. Reads the database only: TermF need not run.
pub fn reviews(data_dir: &Path) -> Result<Vec<Review>> {
    Store::open(data_dir)?.all()
}

pub fn review(data_dir: &Path, id: i64) -> Result<Review> {
    Store::open(data_dir)?
        .get(id)?
        .with_context(|| format!("no review {id}"))
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
        Ok(Self { conn })
    }

    /// Gives the review its id, which its JSON holds: both in one transaction, so no row is left half written.
    pub(crate) fn insert(&self, review: &mut Review) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO visual_reviews (run, data, updated) VALUES (?1, '{}', ?2)",
            params![review.run, now()],
        )?;
        review.id = tx.last_insert_rowid();
        self.save(review)?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn save(&self, review: &mut Review) -> Result<()> {
        // In milliseconds and never twice the same: the app keeps the copy saved last.
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        review.updated = ms.max(review.updated + 1);
        self.conn.execute(
            "UPDATE visual_reviews SET data = ?2, updated = ?3 WHERE id = ?1",
            params![review.id, serde_json::to_string(review)?, review.updated],
        )?;
        Ok(())
    }

    pub(crate) fn get(&self, id: i64) -> Result<Option<Review>> {
        let data: Option<String> = self
            .conn
            .query_row("SELECT data FROM visual_reviews WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        data.map(|d| serde_json::from_str(&d).context("unreadable review"))
            .transpose()
    }

    pub(crate) fn of_run(&self, run: i64) -> Result<Option<Review>> {
        Ok(self.select("WHERE run = ?1", [run])?.into_iter().next())
    }

    pub(crate) fn all(&self) -> Result<Vec<Review>> {
        self.select("", params![])
    }

    /// Newest first. A row that cannot be read is left out, so one broken review never hides the others.
    fn select(&self, filter: &str, values: impl rusqlite::Params) -> Result<Vec<Review>> {
        let mut query = self.conn.prepare(&format!(
            "SELECT data FROM visual_reviews {filter} ORDER BY id DESC"
        ))?;
        let rows = query.query_map(values, |r| r.get::<_, String>(0))?;
        Ok(rows
            .filter_map(|d| serde_json::from_str(&d.ok()?).ok())
            .collect())
    }
}
