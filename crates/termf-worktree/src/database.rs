//! A worktree's own copy of the project's local database: PostgreSQL through `CREATE DATABASE … TEMPLATE`
//! (dump and restore when the template has open connections), MySQL and MariaDB through a dump, SQLite as a
//! copy-on-write clone of its file. The connection comes from the repository's env file (`DATABASE_URL`,
//! Laravel's `DB_*`) or `config/database.yml`; the worktree's env file is then pointed at the copy. Only a
//! server on this machine (localhost, a local socket) or a service of the repository's Compose file is ever
//! touched: anything else is refused.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::{Outcome, Worktree, cow, files};

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

/// The file of a repository that gives its local database, if any.
pub(crate) fn find(repo: &Path) -> Option<String> {
    for file in [".env", ".env.local"] {
        if let Ok(text) = fs::read_to_string(repo.join(file)) {
            let vars = parse_env(&text);
            if ["DATABASE_URL", "DB_CONNECTION"]
                .iter()
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

pub(crate) fn read(repo: &Path, from: &str) -> Result<Conn> {
    let rel =
        files::relative(from).with_context(|| format!("{from} is not a file of the repository"))?;
    let text = fs::read_to_string(repo.join(rel)).with_context(|| format!("cannot read {from}"))?;
    if from.ends_with(".yml") || from.ends_with(".yaml") {
        return yaml(&text);
    }
    let vars = parse_env(&text);
    match get(&vars, "DATABASE_URL").filter(|u| !u.is_empty()) {
        Some(url) => url_conn(url, "DATABASE_URL"),
        None => laravel(&vars),
    }
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

fn get<'a>(vars: &'a [(String, String)], key: &str) -> Option<&'a str> {
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

fn url_conn(url: &str, key: &str) -> Result<Conn> {
    let (scheme, rest) = url.split_once(':').context("DATABASE_URL is not a URL")?;
    let engine = match scheme.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" | "pgsql" => Engine::Postgres,
        "mysql" | "mysql2" | "mariadb" | "trilogy" => Engine::Mysql,
        "sqlite" | "sqlite3" | "file" => Engine::Sqlite,
        other => bail!("{other} databases are not supported"),
    };
    let mut conn = Conn::new(engine, Key::Url(key.into()));
    if engine == Engine::Sqlite {
        let path = rest.strip_prefix("//").unwrap_or(rest);
        let path = path.split(['?', '#']).next().unwrap_or_default();
        // `sqlite:////abs/path` is an absolute path.
        let path = if path.starts_with("//") {
            &path[1..]
        } else {
            path
        };
        ensure!(!path.is_empty(), "DATABASE_URL names no file");
        conn.name = path.into();
        return Ok(conn);
    }
    let rest = rest
        .strip_prefix("//")
        .context("DATABASE_URL is not a URL")?;
    let rest = rest.split('#').next().unwrap_or_default();
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (userinfo, hostport) = authority.rsplit_once('@').unwrap_or(("", authority));
    let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    let (host, port) = match hostport.strip_prefix('[') {
        Some(v6) => {
            let (host, port) = v6.split_once(']').context("DATABASE_URL has a bad host")?;
            (host, port.strip_prefix(':'))
        }
        None => match hostport.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (hostport, None),
        },
    };
    conn.host = decode(host);
    for pair in query.split('&') {
        if let Some(socket) = pair.strip_prefix("host=").or(pair.strip_prefix("socket=")) {
            conn.host = decode(socket);
        }
    }
    conn.port = port
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .transpose()
        .context("DATABASE_URL has a bad port")?;
    conn.user = decode(user);
    conn.password = decode(password);
    conn.name = decode(path);
    ensure!(!conn.name.is_empty(), "DATABASE_URL names no database");
    Ok(conn)
}

fn laravel(vars: &[(String, String)]) -> Result<Conn> {
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
fn yaml(text: &str) -> Result<Conn> {
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

fn decode(text: &str) -> String {
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
    let point = point_at(env, wt, from, &conn)?;
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
    done.env = point.apply(&conn, &name)?;
    Ok(done)
}

/// Where the worktree learns its database's name.
enum Point {
    /// Its env file, which git does not track.
    File(PathBuf),
    /// The session's `DATABASE_URL`.
    Var,
}

impl Point {
    fn apply(&self, conn: &Conn, value: &str) -> Result<BTreeMap<String, String>> {
        let file = match self {
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
            Point::File(file) => file,
        };
        let text = fs::read_to_string(file)?;
        let text = match &conn.key {
            Key::Url(key) => {
                let url = get(&parse_env(&text), key).unwrap_or_default().to_owned();
                let url = match conn.engine {
                    Engine::Sqlite => url.replacen(&conn.name, value, 1),
                    _ => with_database(&url, value),
                };
                set_env(&text, key, &url)
            }
            _ => set_env(&text, "DB_DATABASE", value),
        };
        fs::write(file, text).with_context(|| format!("cannot write {}", file.display()))?;
        Ok(BTreeMap::new())
    }
}

/// Makes sure the worktree has its own, untracked, env file to point at the copy, before anything is made.
fn point_at(env: &[(String, String)], wt: &Worktree, from: &str, conn: &Conn) -> Result<Point> {
    if conn.key == Key::Yaml {
        return Ok(Point::Var);
    }
    let rel = files::relative(from).context("not a file of the repository")?;
    let file = wt.path.join(&rel);
    ensure!(
        !files::tracked(env, &wt.path, from),
        "{from} is tracked by git: TermF does not rewrite it, so this worktree would share the database"
    );
    if !file.exists() {
        fs::copy(wt.source.join(&rel), &file).with_context(|| format!("cannot copy {from}"))?;
    }
    Ok(Point::File(file))
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
        true => Some(point_at(env, wt, from, conn)?),
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
        done.env = point.apply(conn, &file)?;
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

impl Client<'_> {
    /// The password goes through the environment, never the command line.
    fn command(&self, program: &str) -> Command {
        let c = self.conn;
        let mut vars: Vec<(&str, String)> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        let direct = self.service.is_none();
        if c.engine == Engine::Postgres {
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
            if direct && c.host.starts_with('/') {
                args.push(format!("--socket={}", c.host));
            } else if direct && !c.host.is_empty() {
                args.push(format!("--host={}", c.host));
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
        run(cmd)
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
                if let Err(e) = pipe(dump, restore) {
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
        let mariadb = version
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("MariaDB"));
        let mut dump = self.command("mysqldump");
        dump.args(["--single-transaction", "--routines", "--triggers"]);
        // A MySQL server with GTIDs refuses its own dump's GTID_PURGED; MariaDB's dump has no such option.
        if !mariadb {
            dump.arg("--set-gtid-purged=OFF");
        }
        dump.arg(source);
        let mut restore = self.command("mysql");
        restore.arg(name);
        if let Err(e) = pipe(dump, restore) {
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

fn run(mut cmd: Command) -> Result<()> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd
        .output()
        .with_context(|| format!("cannot run {program}: is it installed?"))?;
    ensure!(
        out.status.success(),
        "{program}: {}",
        crate::tail(String::from_utf8_lossy(&out.stderr).trim(), 600)
    );
    Ok(())
}

/// `dump | restore`. The dump's errors go to a file, so neither side can block on a full pipe.
fn pipe(mut dump: Command, mut restore: Command) -> Result<()> {
    let log = std::env::temp_dir().join(format!(
        "termf-dump-{}-{}.log",
        std::process::id(),
        crate::hash(&format!("{dump:?}"))
    ));
    let program = dump.get_program().to_string_lossy().into_owned();
    dump.stdout(Stdio::piped()).stderr(fs::File::create(&log)?);
    let mut producer = dump
        .spawn()
        .with_context(|| format!("cannot run {program}: is it installed?"))?;
    restore.stdin(producer.stdout.take().context("no pipe")?);
    let restored = restore.output();
    let dumped = producer.wait();
    let errors = fs::read_to_string(&log).unwrap_or_default();
    let _ = fs::remove_file(&log);
    let restored = restored.context("cannot restore the dump")?;
    ensure!(
        dumped?.success(),
        "{program}: {}",
        crate::tail(errors.trim(), 600)
    );
    ensure!(
        restored.status.success(),
        "restore: {}",
        crate::tail(String::from_utf8_lossy(&restored.stderr).trim(), 600)
    );
    Ok(())
}
