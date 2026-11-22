//! The SQLite databases TermF shares between the app, the CLI and their threads.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use rusqlite::{Connection, ErrorCode};

/// Opens `file` in WAL mode, waiting up to 5 s for a writer: the app and the CLI may write at the same time.
pub fn open(file: &Path) -> Result<Connection> {
    let conn = Connection::open(file).with_context(|| format!("cannot open {}", file.display()))?;
    conn.busy_timeout(Duration::from_secs(5))?;
    // Turning a new database to WAL takes a lock SQLite does not wait for, so two first openers can collide.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0)) {
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == ErrorCode::DatabaseBusy && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            done => {
                done?;
                return Ok(conn);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_database_opened_from_several_threads_at_once() {
        for round in 0..20 {
            let name = format!("termf-db-{}-{round}", std::process::id());
            let dir = std::env::temp_dir().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join("termf.db");
            let opened: Vec<Result<Connection>> = std::thread::scope(|s| {
                let threads: Vec<_> = (0..8)
                    .map(|i| {
                        let file = &file;
                        s.spawn(move || {
                            let conn = open(file)?;
                            let table = format!("CREATE TABLE IF NOT EXISTS t{i} (x)");
                            conn.execute_batch(&format!(
                                "{table}; CREATE TABLE IF NOT EXISTS t (x)"
                            ))?;
                            Ok(conn)
                        })
                    })
                    .collect();
                threads.into_iter().map(|t| t.join().unwrap()).collect()
            });
            for conn in opened {
                if let Err(e) = conn {
                    panic!("round {round}: {e:#}");
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
