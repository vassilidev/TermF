//! The project memory the expert searches: a local full-text index, per project, of what the project kept —
//! Claude Code conversations of its repositories and their worktrees, ticket runs and team plans and reports,
//! TermF's headless conversations, the project's library and conventions, `CLAUDE.md` and automatic memory,
//! the repositories' docs and git history, and the claude.ai artifacts TermF saw there. SQLite FTS5 in
//! `termf.db`, updated incrementally: a document is read again only when its stamp (modification time and size,
//! commit, update time) changed. Every text is masked before it is stored; nothing leaves the machine.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

mod sources;
#[cfg(test)]
mod tests;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS memory_docs (
    id INTEGER PRIMARY KEY,
    project TEXT NOT NULL,
    source TEXT NOT NULL,
    key TEXT NOT NULL,
    stamp TEXT NOT NULL,
    title TEXT NOT NULL,
    locator TEXT NOT NULL,
    at INTEGER NOT NULL,
    size INTEGER NOT NULL,
    UNIQUE (project, source, key)
);
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(title, body, tokenize = 'unicode61 remove_diacritics 2');
CREATE TABLE IF NOT EXISTS memory_state (
    project TEXT PRIMARY KEY,
    indexed INTEGER NOT NULL,
    errors TEXT NOT NULL
);
";

/// What one document keeps, masked; a read gives at most [`PAGE`] of it at a time.
pub const MAX_BODY: usize = 256 << 10;
pub const PAGE: usize = 24 << 10;
const MAX_TITLE: usize = 160;
const MAX_HITS: usize = 50;

/// Where a project's memory is read from.
#[derive(Clone, Debug)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    /// `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
    pub claude_dir: PathBuf,
}

impl Paths {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Self {
        Self {
            config_dir,
            data_dir,
            claude_dir: termf_claude::claude_dir(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Claude Code conversations of the project's repositories and their worktrees.
    Claude,
    /// TermF's headless conversations.
    Agents,
    /// Ticket runs' journals, team plans and reports.
    Runs,
    /// The project's library and conventions.
    Library,
    /// `CLAUDE.md`, `.claude/rules` and Claude Code's automatic memory.
    Memory,
    /// The repositories' documentation files.
    Docs,
    /// The repositories' commits: messages and files touched.
    Git,
    /// claude.ai artifacts TermF saw published or opened in the project.
    Artifacts,
}

impl Source {
    pub const ALL: [Source; 8] = [
        Source::Claude,
        Source::Agents,
        Source::Runs,
        Source::Library,
        Source::Memory,
        Source::Docs,
        Source::Git,
        Source::Artifacts,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Source::Claude => "claude",
            Source::Agents => "agents",
            Source::Runs => "runs",
            Source::Library => "library",
            Source::Memory => "memory",
            Source::Docs => "docs",
            Source::Git => "git",
            Source::Artifacts => "artifacts",
        }
    }

    pub fn parse(id: &str) -> Result<Self> {
        Source::ALL
            .into_iter()
            .find(|s| s.id() == id)
            .with_context(|| format!("unknown memory source {id}"))
    }
}

/// One document as a source gives it, before it is masked.
#[derive(Debug, Default)]
pub(crate) struct Doc {
    pub key: String,
    /// Changes whenever the content may have.
    pub stamp: String,
    pub title: String,
    /// What the expert cites: a path, a conversation, a run, a commit, a URL.
    pub locator: String,
    /// When its content dates from, in Unix seconds.
    pub at: i64,
    pub body: String,
}

/// What a source holds now: every key, and the documents whose stamp is new.
#[derive(Default)]
pub(crate) struct Found {
    pub keys: HashSet<String>,
    pub docs: Vec<Doc>,
}

impl Found {
    /// Keeps `key`, and reads it only when `stamp` is not the one indexed.
    pub(crate) fn add(
        &mut self,
        known: &HashMap<String, String>,
        key: String,
        stamp: String,
        load: impl FnOnce() -> Option<Doc>,
    ) {
        if known.get(&key) != Some(&stamp)
            && let Some(doc) = load()
        {
            self.docs.push(Doc {
                key: key.clone(),
                stamp,
                ..doc
            });
        }
        self.keys.insert(key);
    }
}

/// A search result.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: i64,
    pub source: Source,
    pub title: String,
    pub locator: String,
    pub at: i64,
    /// The matching passage, terms between `[` and `]`.
    pub snippet: String,
}

/// A part of one document: [`PAGE`] at most from `offset`, `next` where the following part starts.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Page {
    pub id: i64,
    pub source: Source,
    pub title: String,
    pub locator: String,
    pub at: i64,
    pub size: usize,
    pub offset: usize,
    pub next: Option<usize>,
    pub text: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub source: Source,
    pub included: bool,
    pub docs: u64,
    pub bytes: u64,
    /// Why the last indexing could not read it; what it held before stays.
    pub error: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub project: String,
    pub enabled: bool,
    /// When it was last indexed, in Unix seconds.
    pub indexed: Option<i64>,
    pub sources: Vec<SourceStatus>,
}

/// What an indexing changed.
#[derive(Serialize, Debug, Default, Clone, Copy, PartialEq)]
pub struct Indexed {
    pub changed: usize,
    pub removed: usize,
}

pub struct Memory {
    conn: Connection,
}

impl Memory {
    pub fn open(data_dir: &Path) -> Result<Self> {
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

    /// Brings `project`'s index up to date with its sources; `mask` hides more than the patterns do (the
    /// vault's values). A source that cannot be read keeps what it had and says why; one switched off, or the
    /// whole memory, is emptied.
    pub fn index(
        &self,
        paths: &Paths,
        project: &str,
        mask: &dyn Fn(&str) -> String,
    ) -> Result<Indexed> {
        let settings = termf_core::load_settings(&paths.config_dir)?.memory;
        let projects = termf_core::load_projects(&paths.config_dir)?;
        let project = projects
            .iter()
            .find(|p| p.id == project)
            .with_context(|| format!("unknown project {project}"))?;
        let mut done = Indexed::default();
        let mut errors = HashMap::new();
        for source in Source::ALL {
            let known = self.stamps(&project.id, source)?;
            let found = if !settings.enabled || settings.exclude.iter().any(|s| s == source.id()) {
                Found::default()
            } else {
                match sources::collect(source, paths, project, &known) {
                    Ok(found) => found,
                    Err(e) => {
                        errors.insert(source.id(), format!("{e:#}"));
                        continue;
                    }
                }
            };
            let tx = self.conn.unchecked_transaction()?;
            for key in known.keys().filter(|k| !found.keys.contains(*k)) {
                self.remove(&project.id, source, key)?;
                done.removed += 1;
            }
            for doc in found.docs {
                self.put(&project.id, source, doc, mask)?;
                done.changed += 1;
            }
            tx.commit()?;
        }
        self.conn.execute(
            "INSERT INTO memory_state (project, indexed, errors) VALUES (?1, ?2, ?3)
             ON CONFLICT (project) DO UPDATE SET indexed = ?2, errors = ?3",
            params![project.id, now(), serde_json::to_string(&errors)?],
        )?;
        Ok(done)
    }

    /// Indexes every project; one that fails does not stop the others.
    pub fn index_all(
        &self,
        paths: &Paths,
        mask: &dyn Fn(&str) -> String,
    ) -> Vec<(String, Result<Indexed>)> {
        termf_core::load_projects(&paths.config_dir)
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let result = self.index(paths, &p.id, mask);
                (p.id, result)
            })
            .collect()
    }

    fn stamps(&self, project: &str, source: Source) -> Result<HashMap<String, String>> {
        let mut query = self
            .conn
            .prepare("SELECT key, stamp FROM memory_docs WHERE project = ?1 AND source = ?2")?;
        let rows = query.query_map(params![project, source.id()], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn remove(&self, project: &str, source: Source, key: &str) -> Result<()> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM memory_docs WHERE project = ?1 AND source = ?2 AND key = ?3",
                params![project, source.id(), key],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = id {
            self.conn
                .execute("DELETE FROM memory_fts WHERE rowid = ?1", [id])?;
            self.conn
                .execute("DELETE FROM memory_docs WHERE id = ?1", [id])?;
        }
        Ok(())
    }

    fn put(
        &self,
        project: &str,
        source: Source,
        doc: Doc,
        mask: &dyn Fn(&str) -> String,
    ) -> Result<()> {
        self.remove(project, source, &doc.key)?;
        let title = clean(&cut(&doc.title, MAX_TITLE), mask);
        let body = clean(tail(&doc.body, MAX_BODY), mask);
        self.conn.execute(
            "INSERT INTO memory_docs (project, source, key, stamp, title, locator, at, size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                project,
                source.id(),
                doc.key,
                doc.stamp,
                title,
                clean(&doc.locator, mask),
                doc.at,
                body.len() as i64
            ],
        )?;
        self.conn.execute(
            "INSERT INTO memory_fts (rowid, title, body) VALUES (?1, ?2, ?3)",
            params![self.conn.last_insert_rowid(), title, body],
        )?;
        Ok(())
    }

    /// The documents of `project` that best match `query`, from the sources named (all when none).
    pub fn search(
        &self,
        project: &str,
        query: &str,
        sources: &[Source],
        limit: usize,
    ) -> Result<Vec<Hit>> {
        let Some(matching) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let wanted: Vec<&str> = if sources.is_empty() {
            Source::ALL.iter().map(|s| s.id()).collect()
        } else {
            sources.iter().map(|s| s.id()).collect()
        };
        let mut query = self.conn.prepare(
            "SELECT d.id, d.source, d.title, d.locator, d.at,
                    snippet(memory_fts, 1, '[', ']', '…', 24)
             FROM memory_fts JOIN memory_docs d ON d.id = memory_fts.rowid
             WHERE memory_fts MATCH ?1 AND d.project = ?2
               AND d.source IN (SELECT value FROM json_each(?3))
             ORDER BY bm25(memory_fts, 4.0, 1.0) LIMIT ?4",
        )?;
        let rows = query.query_map(
            params![
                matching,
                project,
                serde_json::to_string(&wanted)?,
                limit.clamp(1, MAX_HITS) as i64
            ],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (id, source, title, locator, at, snippet) = row?;
            Ok(Hit {
                id,
                source: Source::parse(&source)?,
                title,
                locator,
                at,
                snippet,
            })
        })
        .collect()
    }

    /// One document of `project`, from `offset` (bytes, moved back to a character's start).
    pub fn read(&self, project: &str, id: i64, offset: usize) -> Result<Page> {
        let row = self
            .conn
            .query_row(
                "SELECT d.source, d.title, d.locator, d.at, f.body
                 FROM memory_docs d JOIN memory_fts f ON f.rowid = d.id
                 WHERE d.id = ?1 AND d.project = ?2",
                params![id, project],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((source, title, locator, at, body)) = row else {
            bail!("no document {id} in this project's memory");
        };
        let start = body.floor_char_boundary(offset.min(body.len()));
        let end = body.floor_char_boundary((start + PAGE).min(body.len()));
        Ok(Page {
            id,
            source: Source::parse(&source)?,
            title,
            locator,
            at,
            size: body.len(),
            offset: start,
            next: (end < body.len()).then_some(end),
            text: body[start..end].to_owned(),
        })
    }

    pub fn status(&self, config_dir: &Path, project: &str) -> Result<Status> {
        let settings = termf_core::load_settings(config_dir)?.memory;
        let state: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT indexed, errors FROM memory_state WHERE project = ?1",
                [project],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let errors: HashMap<String, String> = state
            .as_ref()
            .and_then(|(_, e)| serde_json::from_str(e).ok())
            .unwrap_or_default();
        let mut counts = HashMap::new();
        let mut query = self.conn.prepare(
            "SELECT source, COUNT(*), SUM(size) FROM memory_docs WHERE project = ?1 GROUP BY source",
        )?;
        for row in query.query_map([project], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
            ))
        })? {
            let (source, docs, bytes) = row?;
            counts.insert(source, (docs, bytes));
        }
        Ok(Status {
            project: project.to_owned(),
            enabled: settings.enabled,
            indexed: state.map(|(at, _)| at),
            sources: Source::ALL
                .into_iter()
                .map(|source| {
                    let (docs, bytes) = counts.get(source.id()).copied().unwrap_or_default();
                    SourceStatus {
                        source,
                        included: !settings.exclude.iter().any(|s| s == source.id()),
                        docs,
                        bytes,
                        error: errors.get(source.id()).cloned(),
                    }
                })
                .collect(),
        })
    }
}

/// The project whose repository holds `path`: the first that lists it.
pub fn project_of(config_dir: &Path, path: &Path) -> Result<Option<String>> {
    Ok(termf_core::load_projects(config_dir)?
        .into_iter()
        .find(|p| p.repos.iter().any(|r| path.starts_with(&r.path)))
        .map(|p| p.id))
}

/// Each word of a question as a prefix, any of them: FTS5's own syntax never comes from the caller, and BM25
/// ranks the documents with the most and rarest words first.
fn fts_query(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .take(32)
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

/// The vault's values, the private keys, then every secret the patterns of the Claude view recognize.
pub fn clean(text: &str, mask: &dyn Fn(&str) -> String) -> String {
    termf_claude::mask::mask_text(&private_keys(&mask(text)))
}

/// A PEM private key's lines are base64 that the word patterns do not all catch.
fn private_keys(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        let armor = line.trim();
        if armor.starts_with("-----BEGIN") && armor.contains("PRIVATE KEY") {
            inside = true;
            out.push_str(line);
            out.push_str(termf_claude::mask::MASK);
            out.push('\n');
        } else if inside && armor.starts_with("-----END") {
            inside = false;
            out.push_str(line);
        } else if !inside {
            out.push_str(line);
        }
    }
    out
}

/// The last `max` bytes: in a long conversation or journal, the latest part matters most.
fn tail(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    &text[text.ceil_char_boundary(text.len() - max)..]
}

pub(crate) fn cut(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text,
    }
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
