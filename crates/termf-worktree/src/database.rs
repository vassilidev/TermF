//! A worktree's own copy of the project's local database: PostgreSQL through `CREATE DATABASE … TEMPLATE`
//! (dump and restore when the template has open connections), MySQL and MariaDB through a dump, SQLite as a
//! copy-on-write clone of its file. The connection comes from the repository's env files (`DATABASE_URL`,
//! `DB_URL`, Laravel's `DB_*`, the local and per-mode files over `.env`) or `config/database.yml`; every env file
//! of the worktree that names the original database is then pointed at the copy. Only a server on this machine
//! (localhost, a local socket) or a service of the repository's Compose file is ever touched: anything else is
//! refused.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::{Outcome, Worktree, cow, detect, files};

/// The env files read over one another, the one that wins first (dotenv-flow, Next.js, Vite).
const ENV_FILES: [&str; 4] = [
    ".env.development.local",
    ".env.local",
    ".env.development",
    ".env",
];
/// `DB_URL` is Laravel 11's; either one overrides the separate settings.
const URL_KEYS: [&str; 2] = ["DATABASE_URL", "DB_URL"];
const SQL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const COPY_TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Postgres,
    Mysql,
    Sqlite,
}

impl Engine {
    pub fn label(self) -> &'static str {
        match self {
            Engine::Postgres => "postgres",
            Engine::Mysql => "mysql",
            Engine::Sqlite => "sqlite",
        }
    }
}

/// Where the config names the database.
#[derive(Clone, Debug, PartialEq)]
enum Key {
    /// A URL in this env variable.
    Url(String),
    /// Laravel's `DB_CONNECTION`, `DB_DATABASE`…
    Laravel,
    /// `config/database.yml`, which TermF does not rewrite: the session gets `DATABASE_URL` instead.
    Yaml,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Conn {
    pub engine: Engine,
    /// Empty for the default socket; a path for another socket.
    pub host: String,
    pub port: Option<u16>,
    pub user: String,
    pub password: String,
    /// The database's name, or a SQLite file's path as written.
    pub name: String,
    key: Key,
}

impl Conn {
    fn new(engine: Engine, key: Key) -> Conn {
        Conn {
            engine,
            host: String::new(),
            port: None,
            user: String::new(),
            password: String::new(),
            name: String::new(),
            key,
        }
    }
}

/// A repository's database as the recipe view shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DbView {
    pub engine: Option<Engine>,
    pub host: String,
    pub name: String,
    /// The Compose service its server runs in.
    pub service: Option<String>,
    /// Why TermF will not copy it: a remote server, a config it cannot read.
    pub refused: Option<String>,
}

/// A copy a setup made, dropped at teardown.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Made {
    pub engine: Engine,
    /// The repository and its file that give the connection.
    pub source: PathBuf,
    pub from: String,
    /// The copy's database name, or its file.
    pub name: String,
    pub service: Option<String>,
    /// A SQLite copy made outside the worktree, deleted at teardown.
    pub file: Option<PathBuf>,
}

pub(crate) struct Done {
    pub outcome: Outcome,
    pub target: String,
    pub detail: String,
    /// For the session, when the config cannot be rewritten (`DATABASE_URL` for `database.yml`).
    pub env: BTreeMap<String, String>,
    pub made: Option<Made>,
}

impl Done {
    fn new(outcome: Outcome, target: &str, detail: impl Into<String>) -> Done {
        Done {
            outcome,
            target: target.into(),
            detail: detail.into(),
            env: BTreeMap::new(),
            made: None,
        }
    }
}

/// The file of a repository that gives its local database, if any: the env file that wins first.
pub(crate) fn find(repo: &Path) -> Option<String> {
    for file in ENV_FILES {
        if let Ok(text) = fs::read_to_string(repo.join(file)) {
            let vars = parse_env(&text);
            if URL_KEYS
                .iter()
                .chain(&["DB_CONNECTION"])
                .any(|k| get(&vars, k).is_some_and(|v| !v.is_empty()))
            {
                return Some(file.into());
            }
        }
    }
    repo.join("config/database.yml")
        .is_file()
        .then(|| "config/database.yml".into())
}

fn is_yaml(from: &str) -> bool {
    from.ends_with(".yml") || from.ends_with(".yaml")
}

/// The env files of `repo` read with `from`, the one that wins first.
fn env_files(repo: &Path, from: &str) -> Vec<String> {
    let mut found = Vec::new();
    if !ENV_FILES.contains(&from) {
        found.push(from.to_owned());
    }
    found.extend(
        ENV_FILES
            .iter()
            .filter(|f| repo.join(f).is_file())
            .map(|f| f.to_string()),
    );
    found
}

pub(crate) fn read(repo: &Path, from: &str) -> Result<Conn> {
    let rel =
        files::relative(from).with_context(|| format!("{from} is not a file of the repository"))?;
    if is_yaml(from) {
        let text =
            fs::read_to_string(repo.join(rel)).with_context(|| format!("cannot read {from}"))?;
        return yaml(&text);
    }
    ensure!(repo.join(&rel).is_file(), "cannot read {from}");
    // Read from the weakest file to the one that wins, so `get` sees the winning value last.
    let mut vars = Vec::new();
    for file in env_files(repo, from).iter().rev() {
        vars.extend(parse_env(
            &fs::read_to_string(repo.join(file)).unwrap_or_default(),
        ));
    }
    for key in URL_KEYS {
        if let Some(url) = get(&vars, key).filter(|u| !u.is_empty()) {
            return url_conn(url, key);
        }
    }
    laravel(&vars)
}

pub(crate) fn view(repo: &Path, from: &str) -> DbView {
    let compose = crate::detect::compose(repo)
        .map(|c| c.services)
        .unwrap_or_default();
    match read(repo, from) {
        Ok(conn) => {
            let reached = reach(&conn, &compose);
            DbView {
                engine: Some(conn.engine),
                host: conn.host,
                name: conn.name,
                service: reached.clone().ok().flatten(),
                refused: reached.err(),
            }
        }
        Err(e) => DbView {
            engine: None,
            host: String::new(),
            name: String::new(),
            service: None,
            refused: Some(format!("{e:#}")),
        },
    }
}

/// What the setup agent's sandbox must open for it to reach the worktree's database: localhost ports, or sockets.
#[derive(Debug, PartialEq)]
pub(crate) struct Access {
    pub local_tcp: bool,
    pub sockets: Vec<String>,
}

/// `None` for a SQLite file, a Compose service (the agent does not run Docker) or a database TermF refuses.
pub(crate) fn access(repo: &Path, from: &str) -> Option<Access> {
    let conn = read(repo, from).ok()?;
    let compose = detect::compose(repo)
        .map(|c| c.services)
        .unwrap_or_default();
    if conn.engine == Engine::Sqlite || reach(&conn, &compose) != Ok(None) {
        return None;
    }
    let tcp = Access {
        local_tcp: true,
        sockets: Vec::new(),
    };
    let sockets = |paths: Vec<String>| Access {
        local_tcp: false,
        sockets: paths,
    };
    Some(match (conn.engine, conn.host.as_str()) {
        (Engine::Postgres, host) => {
            let file = format!(".s.PGSQL.{}", conn.port.unwrap_or(5432));
            match host {
                dir if dir.starts_with('/') => sockets(vec![format!("{dir}/{file}")]),
                "" => sockets(vec![
                    format!("/tmp/{file}"),
                    format!("/var/run/postgresql/{file}"),
                ]),
                _ => tcp,
            }
        }
        // For the MySQL tools, `localhost` is the default socket.
        (_, socket) if socket.starts_with('/') => sockets(vec![socket.to_owned()]),
        (_, "" | "localhost") => sockets(vec![
            "/tmp/mysql.sock".into(),
            "/var/run/mysqld/mysqld.sock".into(),
        ]),
        _ => tcp,
    })
}

/// `KEY=value` lines; `export`, quotes and comments allowed.
pub(crate) fn parse_env(text: &str) -> Vec<(String, String)> {
    let mut vars = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            continue;
        }
        vars.push((key.to_owned(), unquote(value.trim())));
    }
    vars
}

fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(rest) = value.strip_prefix(quote) {
            return rest.split(quote).next().unwrap_or_default().to_owned();
        }
    }
    value
        .split(" #")
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned()
}

pub(crate) fn get<'a>(vars: &'a [(String, String)], key: &str) -> Option<&'a str> {
    vars.iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// Sets `key` in an env file's text, keeping its other lines, its `export` and its quotes; appended when absent.
pub(crate) fn set_env(text: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut out: String = text
        .split_inclusive('\n')
        .map(|line| {
            let body = line.trim_end_matches(['\n', '\r']);
            let ending = &line[body.len()..];
            let indent = &body[..body.len() - body.trim_start().len()];
            let rest = body.trim_start();
            let (export, rest) = match rest.strip_prefix("export ") {
                Some(rest) => ("export ", rest),
                None => ("", rest),
            };
            match rest.split_once('=') {
                Some((k, v)) if k.trim() == key => {
                    found = true;
                    let quote = match v.trim_start().chars().next() {
                        Some(q @ ('"' | '\'')) => q.to_string(),
                        _ if value.contains([' ', '#']) => "\"".into(),
                        _ => String::new(),
                    };
                    format!("{indent}{export}{key}={quote}{value}{quote}{ending}")
                }
                _ => line.to_owned(),
            }
        })
        .collect();
    if !found {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        let quote = if value.contains([' ', '#']) { "\"" } else { "" };
        out.push_str(&format!("{key}={quote}{value}{quote}\n"));
    }
    out
}

pub(crate) fn url_conn(url: &str, key: &str) -> Result<Conn> {
    let url = crate::declared::parse_url(url).with_context(|| format!("{key} is not a URL"))?;
    let engine = match url.scheme.as_str() {
        "postgres" | "postgresql" | "pgsql" => Engine::Postgres,
        "mysql" | "mysql2" | "mariadb" | "trilogy" => Engine::Mysql,
        "sqlite" | "sqlite3" | "file" => Engine::Sqlite,
        other => bail!("{other} databases are not supported"),
    };
    let mut conn = Conn::new(engine, Key::Url(key.into()));
    conn.host = url.host;
    conn.port = url.port;
    conn.user = url.user;
    conn.password = url.password;
    conn.name = url.database;
    ensure!(!conn.name.is_empty(), "{key} names no database");
    Ok(conn)
}

pub(crate) fn laravel(vars: &[(String, String)]) -> Result<Conn> {
    let value = |key: &str| get(vars, key).unwrap_or_default().to_owned();
    let engine = match value("DB_CONNECTION").as_str() {
        "mysql" | "mariadb" => Engine::Mysql,
        "pgsql" | "postgres" | "postgresql" => Engine::Postgres,
        "sqlite" => Engine::Sqlite,
        "" => bail!("no DATABASE_URL or DB_CONNECTION"),
        other => bail!("DB_CONNECTION={other} is not supported"),
    };
    let mut conn = Conn::new(engine, Key::Laravel);
    conn.name = value("DB_DATABASE");
    if engine == Engine::Sqlite {
        if conn.name.is_empty() {
            conn.name = "database/database.sqlite".into();
        }
        return Ok(conn);
    }
    conn.host = match value("DB_SOCKET") {
        socket if !socket.is_empty() => socket,
        _ => get(vars, "DB_HOST").unwrap_or("127.0.0.1").to_owned(),
    };
    conn.port = get(vars, "DB_PORT")
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .transpose()
        .context("DB_PORT is not a port")?;
    conn.user = value("DB_USERNAME");
    conn.password = value("DB_PASSWORD");
    ensure!(!conn.name.is_empty(), "no DB_DATABASE");
    Ok(conn)
}

/// The `development` database of `config/database.yml`.
pub(crate) fn yaml(text: &str) -> Result<Conn> {
    let doc = YamlLoader::load_from_str(text)?
        .into_iter()
        .next()
        .context("config/database.yml is empty")?;
    let dev = &doc["development"];
    let field = |key: &str| -> Result<String> {
        let mut value = &dev[key];
        if value.is_badvalue() {
            value = &dev["<<"][key];
        }
        Ok(match value {
            Yaml::String(s) if s.contains("<%") => {
                bail!("config/database.yml computes `{key}` with ERB, which TermF does not run")
            }
            Yaml::String(s) => s.clone(),
            Yaml::Integer(i) => i.to_string(),
            _ => String::new(),
        })
    };
    let engine = match field("adapter")?.as_str() {
        "postgresql" | "postgis" => Engine::Postgres,
        "mysql2" | "trilogy" | "mysql" => Engine::Mysql,
        "sqlite3" | "sqlite" => Engine::Sqlite,
        "" => bail!("config/database.yml has no development adapter"),
        other => bail!("the {other} adapter is not supported"),
    };
    let mut conn = Conn::new(engine, Key::Yaml);
    conn.name = field("database")?;
    conn.host = match field("socket")? {
        socket if !socket.is_empty() => socket,
        _ => field("host")?,
    };
    conn.port = Some(field("port")?)
        .filter(|p| !p.is_empty())
        .map(|p| p.parse())
        .transpose()
        .context("config/database.yml has a bad port")?;
    conn.user = field("username")?;
    conn.password = field("password")?;
    ensure!(
        !conn.name.is_empty(),
        "config/database.yml names no development database"
    );
    Ok(conn)
}

pub(crate) fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `postgres://u:p@localhost:5432/app?sslmode=disable` with another database.
pub(crate) fn with_database(url: &str, name: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.into();
    };
    let (rest, suffix) = rest.split_at(rest.find(['?', '#']).unwrap_or(rest.len()));
    let authority = rest.split('/').next().unwrap_or_default();
    format!("{scheme}://{authority}/{}{suffix}", encode(name))
}

/// `Ok(None)`: this machine; `Ok(Some(service))`: a service of the repository's Compose file; `Err`: refused.
fn reach(conn: &Conn, compose: &[String]) -> Result<Option<String>, String> {
    if conn.engine == Engine::Sqlite {
        return Ok(None);
    }
    let host = conn
        .host
        .trim()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    if host.is_empty()
        || host.starts_with('/')
        || host == "localhost"
        || host == "::1"
        || host.starts_with("127.")
    {
        return Ok(None);
    }
    if let Some(service) = compose.iter().find(|s| s.eq_ignore_ascii_case(&host)) {
        return Ok(Some(service.clone()));
    }
    Err(format!(
        "{} is neither this machine nor a service of the repository's Compose file: TermF only copies a local database",
        conn.host
    ))
}

/// `app_termf_42`, within the 63 bytes PostgreSQL and MySQL allow.
pub(crate) fn copy_name(name: &str, id: &str) -> String {
    let suffix = format!("_termf_{id}");
    let mut end = name.len().min(63usize.saturating_sub(suffix.len()));
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &name[..end])
}

/// Gives the worktree its own copy of the database `from` names, and points the worktree at it.
pub(crate) fn copy(
    env: &[(String, String)],
    wt: &Worktree,
    from: &str,
    compose: &[String],
    previous: Option<&Made>,
) -> Result<Done> {
    let conn = match read(&wt.source, from) {
        Ok(conn) => conn,
        Err(e) => return Ok(Done::new(Outcome::Refused, from, format!("{e:#}"))),
    };
    let service = match reach(&conn, compose) {
        Ok(service) => service,
        Err(why) => return Ok(Done::new(Outcome::Refused, &conn.name, why)),
    };
    if conn.engine == Engine::Sqlite {
        return sqlite(env, wt, from, &conn);
    }
    ensure!(
        !conn.name.starts_with('-'),
        "{} is not a database name TermF uses",
        conn.name
    );
    let name = copy_name(&conn.name, &wt.id);
    let point = match point_at(env, wt, from, &conn) {
        Ok(point) => point,
        Err(e) => return Ok(Done::new(Outcome::Refused, &conn.name, format!("{e:#}"))),
    };
    let client = Client {
        conn: &conn,
        service: service.as_deref(),
        source: &wt.source,
        env,
    };
    let mut done = match previous.filter(|m| m.name == name) {
        Some(_) => Done::new(Outcome::Present, &name, ""),
        None => {
            let how = match conn.engine {
                Engine::Postgres => client.pg_copy(&name)?,
                _ => client.mysql_copy(&name)?,
            };
            let mut done = Done::new(Outcome::Created, &name, how);
            done.made = Some(Made {
                engine: conn.engine,
                source: wt.source.clone(),
                from: from.into(),
                name: name.clone(),
                service: service.clone(),
                file: None,
            });
            done
        }
    };
    done.env = point.apply(wt, &conn, &name, &mut done.detail)?;
    Ok(done)
}

/// Where the worktree learns its database's name.
enum Point {
    /// Its env files that name the original, which git ignores.
    Files(Vec<String>),
    /// The session's `DATABASE_URL`.
    Var,
}

impl Point {
    /// Points the worktree at `value` (the copy's name or file), and says in `detail` which files now name it.
    fn apply(
        &self,
        wt: &Worktree,
        conn: &Conn,
        value: &str,
        detail: &mut String,
    ) -> Result<BTreeMap<String, String>> {
        let files = match self {
            Point::Var => {
                let scheme = match conn.engine {
                    Engine::Postgres => "postgresql",
                    Engine::Mysql => "mysql2",
                    Engine::Sqlite => "sqlite3",
                };
                let (host, query) = match &conn.host {
                    h if h.starts_with('/') => (String::new(), format!("?host={}", encode(h))),
                    h => (h.clone(), String::new()),
                };
                let port = conn.port.map(|p| format!(":{p}")).unwrap_or_default();
                let url = format!("{scheme}://{host}{port}/{}{query}", encode(value));
                return Ok(BTreeMap::from([("DATABASE_URL".to_owned(), url)]));
            }
            Point::Files(files) => files,
        };
        for file in files {
            let path = wt.path.join(file);
            let text = fs::read_to_string(&path)?;
            if let Some(text) = repoint(&text, conn, value) {
                fs::write(&path, text).with_context(|| format!("cannot write {file}"))?;
            }
        }
        if !files.is_empty() {
            let named = format!("{} → {value}", files.join(", "));
            *detail = match detail.is_empty() {
                true => named,
                false => format!("{detail}; {named}"),
            };
        }
        Ok(BTreeMap::new())
    }
}

/// `text` with every setting that names the original database (`DATABASE_URL`, `DB_URL`, `DB_DATABASE`) naming
/// `value` instead; `None` when none does.
fn repoint(text: &str, conn: &Conn, value: &str) -> Option<String> {
    let vars = parse_env(text);
    let mut out = text.to_owned();
    let mut changed = false;
    for key in URL_KEYS {
        let Some(url) = get(&vars, key) else {
            continue;
        };
        let original =
            url_conn(url, key).is_ok_and(|c| c.engine == conn.engine && c.name == conn.name);
        if original {
            let url = match conn.engine {
                Engine::Sqlite => url.replacen(&conn.name, value, 1),
                _ => with_database(url, value),
            };
            out = set_env(&out, key, &url);
            changed = true;
        }
    }
    if get(&vars, "DB_DATABASE") == Some(conn.name.as_str()) {
        out = set_env(&out, "DB_DATABASE", value);
        changed = true;
    }
    changed.then_some(out)
}

/// Finds every env file read with `from` that names the original database, and gives the worktree its own copy of
/// each, before anything is made. Refused when git does not ignore one of them: it would keep naming the original,
/// or end up in the run's pull request.
fn point_at(env: &[(String, String)], wt: &Worktree, from: &str, conn: &Conn) -> Result<Point> {
    if conn.key == Key::Yaml {
        return Ok(Point::Var);
    }
    let mut files = Vec::new();
    for file in env_files(&wt.source, from) {
        let text = fs::read_to_string(wt.source.join(&file)).unwrap_or_default();
        if repoint(&text, conn, "").is_none() {
            continue;
        }
        ensure!(
            detect::ignored(&wt.path, &file, env),
            "{file} names the database but git does not ignore it: TermF does not rewrite it, so this worktree \
             would share the database (or the file would end up in the run's pull request)"
        );
        files.push(file);
    }
    for file in &files {
        let dest = wt.path.join(file);
        if !dest.exists() {
            fs::copy(wt.source.join(file), &dest).with_context(|| format!("cannot copy {file}"))?;
        }
    }
    Ok(Point::Files(files))
}

fn sqlite(env: &[(String, String)], wt: &Worktree, from: &str, conn: &Conn) -> Result<Done> {
    let written = Path::new(&conn.name);
    // shortcut: Prisma reads `file:` paths from its schema's folder; only `prisma/` is tried after the root.
    let candidates = match written.is_absolute() {
        true => vec![written.to_path_buf()],
        false => vec![
            wt.source.join(written),
            wt.source.join("prisma").join(written),
        ],
    };
    let Some(original) = candidates.into_iter().find(|c| c.is_file()) else {
        return Ok(Done::new(
            Outcome::Skipped,
            &conn.name,
            "no database file yet: nothing to copy",
        ));
    };
    let inside = original
        .strip_prefix(&wt.source)
        .ok()
        .map(Path::to_path_buf);
    if let Some(rel) = &inside {
        let rel = rel.to_string_lossy();
        if !detect::ignored(&wt.path, &rel, env) {
            let why = "git does not ignore this database file: its copy would end up in the run's pull request";
            return Ok(Done::new(Outcome::Refused, &rel, why));
        }
    }
    let dest = match &inside {
        Some(rel) => wt.path.join(rel),
        None => {
            let stem = original.file_stem().unwrap_or_default().to_string_lossy();
            let ext = original
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            original.with_file_name(format!("{stem}_termf_{}{ext}", wt.id))
        }
    };
    let file = dest.to_string_lossy().into_owned();
    let target = match &inside {
        Some(rel) => rel.to_string_lossy().into_owned(),
        None => file.clone(),
    };
    // Pointing is only needed when the path leaves the repository or is absolute.
    let point = match inside.is_none() || written.is_absolute() {
        true => match point_at(env, wt, from, conn) {
            Ok(point) => Some(point),
            Err(e) => return Ok(Done::new(Outcome::Refused, &target, format!("{e:#}"))),
        },
        false => None,
    };
    let mut done = if dest.exists() {
        Done::new(Outcome::Present, &target, "")
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let how = copy_file(&original, &dest)?;
        let wal = PathBuf::from(format!("{}-wal", original.display()));
        if wal.is_file() {
            copy_file(&wal, Path::new(&format!("{}-wal", dest.display())))?;
        }
        Done::new(Outcome::Copied, &target, how)
    };
    if inside.is_none() && done.outcome == Outcome::Copied {
        done.made = Some(Made {
            engine: Engine::Sqlite,
            source: wt.source.clone(),
            from: from.into(),
            name: file.clone(),
            service: None,
            file: Some(dest.clone()),
        });
    }
    if let Some(point) = point {
        done.env = point.apply(wt, conn, &file, &mut done.detail)?;
    }
    Ok(done)
}

/// A data file, so a plain copy is fine where copy-on-write is not.
fn copy_file(from: &Path, to: &Path) -> Result<&'static str> {
    if cow::clone(from, to).is_ok() {
        return Ok("copy-on-write clone");
    }
    fs::copy(from, to).with_context(|| format!("cannot copy {}", from.display()))?;
    Ok("copied")
}

/// Drops a copy a setup made. Its credentials come from the repository's config, as when it was made.
pub(crate) fn drop(env: &[(String, String)], made: &Made) -> Result<()> {
    if made.engine == Engine::Sqlite {
        if let Some(file) = &made.file {
            for path in [
                file.clone(),
                PathBuf::from(format!("{}-wal", file.display())),
            ] {
                match fs::remove_file(&path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                        return Err(e).with_context(|| format!("cannot delete {}", path.display()));
                    }
                    _ => {}
                }
            }
        }
        return Ok(());
    }
    ensure!(
        made.name.contains("_termf_"),
        "{} is not a copy TermF made",
        made.name
    );
    let conn = read(&made.source, &made.from)?;
    let client = Client {
        conn: &conn,
        service: made.service.as_deref(),
        source: &made.source,
        env,
    };
    client.drop_db(&made.name)
}

/// Runs a database's client programs, directly or inside its Compose service.
struct Client<'a> {
    conn: &'a Conn,
    service: Option<&'a str>,
    /// Where `docker compose` finds the Compose file.
    source: &'a Path,
    env: &'a [(String, String)],
}

/// What a client would otherwise take from its environment to pick a server: only the repository's config does.
const SERVER_VARS: [&str; 8] = [
    "PGHOST",
    "PGHOSTADDR",
    "PGPORT",
    "PGSERVICE",
    "PGSERVICEFILE",
    "MYSQL_HOST",
    "MYSQL_TCP_PORT",
    "MYSQL_UNIX_PORT",
];

impl Client<'_> {
    /// The password goes through the environment, never the command line.
    fn command(&self, program: &str) -> Command {
        let c = self.conn;
        let mut vars: Vec<(&str, String)> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        let direct = self.service.is_none();
        if c.engine == Engine::Postgres {
            // Without a host, the server is libpq's default local socket.
            if direct && !c.host.is_empty() {
                vars.push(("PGHOST", c.host.clone()));
            }
            if let Some(port) = c.port.filter(|_| direct) {
                vars.push(("PGPORT", port.to_string()));
            }
            if !c.user.is_empty() {
                vars.push(("PGUSER", c.user.clone()));
            }
            if !c.password.is_empty() {
                vars.push(("PGPASSWORD", c.password.clone()));
            }
            // Never ask for a password: there is nobody to answer.
            args.push("-w".into());
        } else {
            if direct {
                // First, as the MySQL tools require: `~/.my.cnf` and the system's option files cannot pick
                // another server.
                args.push("--no-defaults".into());
                match c.host.as_str() {
                    socket if socket.starts_with('/') => args.push(format!("--socket={socket}")),
                    "" => args.push("--host=localhost".into()),
                    host => args.push(format!("--host={host}")),
                }
            }
            if let Some(port) = c.port.filter(|_| direct) {
                args.push(format!("--port={port}"));
            }
            if !c.user.is_empty() {
                args.push(format!("--user={}", c.user));
            }
            if !c.password.is_empty() {
                vars.push(("MYSQL_PWD", c.password.clone()));
            }
        }
        let mut cmd = match self.service {
            None => crate::command(program),
            Some(service) => {
                let mut cmd = crate::command("docker");
                cmd.args(["compose", "exec", "-T"]).current_dir(self.source);
                // `-e NAME` passes the value from this environment, so it stays off the command line.
                for (name, _) in &vars {
                    cmd.args(["-e", name]);
                }
                cmd.arg(service).arg(program);
                cmd
            }
        };
        crate::prepare(&mut cmd, self.env);
        for name in SERVER_VARS {
            cmd.env_remove(name);
        }
        cmd.args(args).envs(vars).stdin(Stdio::null());
        cmd
    }

    fn sql(&self, sql: &str) -> Result<()> {
        let mut cmd = match self.conn.engine {
            Engine::Postgres => self.command("psql"),
            _ => self.command("mysql"),
        };
        match self.conn.engine {
            Engine::Postgres => cmd.args([
                "-X",
                "-q",
                "-v",
                "ON_ERROR_STOP=1",
                "-d",
                "postgres",
                "-c",
                sql,
            ]),
            _ => cmd.args(["-e", sql]),
        };
        run(cmd).map(|_| ())
    }

    fn pg_copy(&self, name: &str) -> Result<String> {
        let source = &self.conn.name;
        let template = format!(
            "CREATE DATABASE {} TEMPLATE {}",
            pg_ident(name),
            pg_ident(source)
        );
        match self.sql(&template) {
            Ok(()) => Ok(format!("template {source}")),
            // A template must have no other connection, which a running app usually holds.
            Err(e) if format!("{e:#}").contains("being accessed by other users") => {
                self.sql(&format!("CREATE DATABASE {}", pg_ident(name)))?;
                let mut dump = self.command("pg_dump");
                dump.args(["--no-owner", "--no-privileges", "-d", source]);
                let mut restore = self.command("psql");
                restore.args(["-X", "-q", "-v", "ON_ERROR_STOP=1", "-d", name]);
                if let Err(e) = pipe(dump, restore, COPY_TIMEOUT) {
                    let _ = self.drop_db(name);
                    return Err(e);
                }
                Ok(format!("dump of {source} (it had open connections)"))
            }
            Err(e) => Err(e),
        }
    }

    fn mysql_copy(&self, name: &str) -> Result<String> {
        let source = &self.conn.name;
        self.sql(&format!("CREATE DATABASE {}", my_ident(name)))?;
        let mut version = self.command("mysqldump");
        version.arg("--version");
        let mariadb =
            run(version).is_ok_and(|out| String::from_utf8_lossy(&out).contains("MariaDB"));
        let mut dump = self.command("mysqldump");
        dump.args(["--single-transaction", "--routines", "--triggers"]);
        // A MySQL server with GTIDs refuses its own dump's GTID_PURGED; MariaDB's dump has no such option.
        if !mariadb {
            dump.arg("--set-gtid-purged=OFF");
        }
        dump.arg(source);
        let mut restore = self.command("mysql");
        restore.arg(name);
        if let Err(e) = pipe(dump, restore, COPY_TIMEOUT) {
            let _ = self.drop_db(name);
            return Err(e);
        }
        Ok(format!("dump of {source}"))
    }

    fn drop_db(&self, name: &str) -> Result<()> {
        match self.conn.engine {
            Engine::Postgres => {
                let plain = format!("DROP DATABASE IF EXISTS {}", pg_ident(name));
                // A session that is still connected keeps a plain drop from working (PostgreSQL 13+ forces it).
                self.sql(&plain)
                    .or_else(|_| self.sql(&format!("{plain} WITH (FORCE)")))
            }
            _ => self.sql(&format!("DROP DATABASE IF EXISTS {}", my_ident(name))),
        }
    }
}

fn pg_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn my_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// Runs a client program with a deadline; returns its output.
fn run(cmd: Command) -> Result<Vec<u8>> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let (status, stdout, stderr) = termf_core::process::run(cmd, b"", SQL_TIMEOUT)
        .with_context(|| format!("cannot run {program}: is it installed?"))?;
    ensure!(
        status.success(),
        "{program}: {}",
        crate::tail(String::from_utf8_lossy(&stderr).trim(), 600)
    );
    Ok(stdout)
}

/// `dump | restore`, both stopped after `timeout`. Errors go to files, so no side blocks on a full pipe.
fn pipe(mut dump: Command, mut restore: Command, timeout: Duration) -> Result<()> {
    let scratch = termf_core::process::scratch_dir()?;
    let (dump_err, restore_err) = (scratch.join("dump.err"), scratch.join("restore.err"));
    let result = (|| {
        let program = dump.get_program().to_string_lossy().into_owned();
        dump.stdout(Stdio::piped())
            .stderr(fs::File::create(&dump_err)?);
        let mut producer = dump
            .spawn()
            .with_context(|| format!("cannot run {program}: is it installed?"))?;
        restore
            .stdin(producer.stdout.take().context("no pipe")?)
            .stdout(Stdio::null())
            .stderr(fs::File::create(&restore_err)?);
        let consumer = restore.spawn();
        // The command keeps its own handle on the pipe: dropped, the restore holds the only reader, so if it
        // stops early the dump gets a broken pipe instead of blocking forever.
        std::mem::drop(restore);
        let mut consumer = match consumer {
            Ok(child) => child,
            Err(e) => {
                let _ = producer.kill();
                let _ = producer.wait();
                return Err(e).context("cannot restore the dump");
            }
        };
        let deadline = Instant::now() + timeout;
        let (mut dumped, mut restored) = (None, None);
        while dumped.is_none() || restored.is_none() {
            if dumped.is_none() {
                dumped = producer.try_wait()?;
            }
            if restored.is_none() {
                restored = consumer.try_wait()?;
            }
            if Instant::now() > deadline {
                for child in [&mut producer, &mut consumer] {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                bail!("the copy took more than {} min", timeout.as_secs() / 60);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let read = |file: &Path| fs::read_to_string(file).unwrap_or_default();
        ensure!(
            restored.is_some_and(|s| s.success()),
            "restore: {}",
            crate::tail(read(&restore_err).trim(), 600)
        );
        ensure!(
            dumped.is_some_and(|s| s.success()),
            "{program}: {}",
            crate::tail(read(&dump_err).trim(), 600)
        );
        Ok(())
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}
