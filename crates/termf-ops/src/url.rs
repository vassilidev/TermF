//! A database added in a few seconds: from a URL pasted (`postgres://`, `mysql://`, `mariadb://`, `sqlite:`,
//! `clickhouse://`, with `sslmode` and its kin), or from what the project's repositories declare
//! (`termf_worktree::declared`). The password a URL holds is handed apart, for the vault: a connection names its
//! secret, never its value.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::{AiAccess, Connection, Env, Kind, valid_name};

/// A connection read from a URL, and what it held besides.
#[derive(Debug, PartialEq)]
pub struct Parsed {
    pub connection: Connection,
    /// For the vault only.
    pub password: Option<String>,
    /// The URL's options TermF does not pass on.
    pub ignored: Vec<String>,
}

pub fn parse_url(text: &str) -> Result<Parsed> {
    from_url(&termf_worktree::parse_url(text)?)
}

/// A name for the connection: the database's (a SQLite file's stem), else the host's, as a vault name takes it.
fn suggested(kind: Kind, url: &termf_worktree::Url) -> String {
    let base = match kind {
        Kind::Sqlite => Path::new(&url.database)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        _ if !url.database.is_empty() => url.database.clone(),
        _ => url.host.clone(),
    };
    let mut name: String = base
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "_.-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    name = name.trim_matches('-').to_owned();
    if !valid_name(&name) {
        name = format!("db-{name}");
    }
    if !valid_name(&name) {
        name = format!("{kind:?}").to_lowercase();
    }
    name.chars().take(64).collect()
}

/// `sslmode=require`, MySQL's `ssl-mode=REQUIRED`, `ssl=true`… in Postgres' words.
fn sslmode(value: &str) -> Option<&'static str> {
    Some(match value.to_lowercase().replace('_', "-").as_str() {
        "disable" | "disabled" | "false" | "0" => "disable",
        "allow" | "prefer" | "preferred" => "prefer",
        "require" | "required" | "true" | "1" => "require",
        "verify-ca" => "verify-ca",
        "verify-full" | "verify-identity" => "verify-full",
        _ => return None,
    })
}

pub fn from_url(url: &termf_worktree::Url) -> Result<Parsed> {
    let kind = Kind::from_scheme(&url.scheme)?;
    let local = kind == Kind::Sqlite
        || matches!(url.host.as_str(), "" | "localhost" | "127.0.0.1" | "::1")
        || url.host.starts_with('/');
    let some = |v: &str| (!v.is_empty()).then(|| v.to_owned());
    let mut connection = Connection {
        name: suggested(kind, url),
        kind,
        // A server elsewhere may well be prod: its strictest scope until the user says otherwise.
        env: if local { Env::Dev } else { Env::Prod },
        ai_access: AiAccess::None,
        description: String::new(),
        host: if kind == Kind::Sqlite {
            None
        } else {
            some(&url.host)
        },
        port: url.port,
        user: some(&url.user),
        jump: None,
        via: None,
        database: some(&url.database),
        sslmode: None,
        context: None,
        namespace: None,
        log_group: None,
        region: None,
        profile: None,
        gcp_project: None,
        url: None,
        site: None,
        secrets: Default::default(),
    };
    let mut ignored = Vec::new();
    for (key, value) in &url.params {
        let tls = matches!(
            key.to_lowercase().as_str(),
            "sslmode" | "ssl-mode" | "ssl_mode" | "ssl" | "tls" | "secure"
        );
        match sslmode(value).filter(|_| tls && kind != Kind::Sqlite) {
            Some(mode) => connection.sslmode = Some(mode.into()),
            None => ignored.push(key.clone()),
        }
    }
    let password = some(&url.password).filter(|_| kind != Kind::Sqlite);
    if password.is_some() {
        connection.secrets.insert(
            "password".into(),
            format!(
                "{}_PASSWORD",
                connection.name.to_uppercase().replace(['.', '-'], "_")
            ),
        );
    }
    Ok(Parsed {
        connection,
        password,
        ignored,
    })
}

/// A database a repository of the project declares, as a connection to add, or why TermF cannot take it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// `acme-api/.env:DATABASE_URL`.
    pub source: String,
    pub connection: Option<Connection>,
    pub refused: Option<String>,
    /// It holds a password, which goes to the vault when it is added.
    pub has_password: bool,
    #[serde(skip)]
    pub password: Option<String>,
}

/// What the repositories `(name, path)` declare, read from their files only.
pub fn declared(repos: &[(String, std::path::PathBuf)]) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (repo, path) in repos {
        for found in termf_worktree::declared(path) {
            let source = format!("{repo}/{}", found.source);
            match from_url(&found.url) {
                Ok(parsed) => out.push(Candidate {
                    source,
                    has_password: parsed.password.is_some(),
                    password: parsed.password,
                    connection: Some(parsed.connection),
                    refused: None,
                }),
                Err(e) => out.push(Candidate {
                    source,
                    connection: None,
                    refused: Some(format!("{e:#}")),
                    has_password: false,
                    password: None,
                }),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_fills_the_connection_and_its_password_goes_apart() {
        let p = parse_url("postgresql://reader:s3cr%2Ft@db.acme.dev:6543/acme_prod?sslmode=verify-full&application_name=x").unwrap();
        let c = &p.connection;
        assert_eq!(
            (
                c.kind,
                c.env,
                c.host.as_deref(),
                c.port,
                c.user.as_deref(),
                c.database.as_deref(),
                c.sslmode.as_deref()
            ),
            (
                Kind::Postgres,
                Env::Prod,
                Some("db.acme.dev"),
                Some(6543),
                Some("reader"),
                Some("acme_prod"),
                Some("verify-full")
            )
        );
        assert_eq!(c.name, "acme_prod");
        assert_eq!(p.password.as_deref(), Some("s3cr/t"));
        assert_eq!(c.secrets["password"], "ACME_PROD_PASSWORD");
        assert_eq!(p.ignored, ["application_name"]);
        c.check().unwrap();
        let toml = toml::to_string(c).unwrap();
        assert!(
            !toml.contains("s3cr"),
            "the password never reaches the file: {toml}"
        );

        let m = parse_url("mysql://root@127.0.0.1/shop?ssl-mode=REQUIRED")
            .unwrap()
            .connection;
        assert_eq!(
            (m.kind, m.env, m.sslmode.as_deref()),
            (Kind::Mysql, Env::Dev, Some("require"))
        );
        assert_eq!(
            parse_url("mariadb://u@h/d").unwrap().connection.kind,
            Kind::Mariadb
        );
        assert_eq!(
            parse_url("clickhouse://default@localhost:9000/events?secure=true")
                .unwrap()
                .connection
                .sslmode
                .as_deref(),
            Some("require")
        );
        let s = parse_url("sqlite:////srv/acme/db.sqlite3")
            .unwrap()
            .connection;
        assert_eq!(
            (
                s.kind,
                s.name.as_str(),
                s.database.as_deref(),
                s.host.as_deref()
            ),
            (Kind::Sqlite, "db", Some("/srv/acme/db.sqlite3"), None)
        );
        s.check().unwrap();
        for (url, why) in [
            ("sqlserver://sa@h/db", "SQL Server is not supported"),
            ("mongodb+srv://u:p@cluster/db", "MongoDB is not supported"),
            ("redis://localhost:6379", "Redis is not supported"),
            ("ftp://x/y", "not a database"),
        ] {
            let e = format!("{:#}", parse_url(url).unwrap_err());
            assert!(e.contains(why), "{url}: {e}");
        }
    }
}
