//! The databases a repository declares, read from its files only (Ops proposes them as connections): URLs in its
//! env files, Laravel's `DB_*` (or the defaults of `config/database.php`), Rails' `config/database.yml`, Prisma's
//! datasource, and the database servers of its Compose file.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use yaml_rust2::{Yaml, YamlLoader};

use crate::database::{Conn, decode, get, laravel, parse_env, yaml};

/// A database URL's parts, percent-decoded: `postgres://u:p@host:5432/app?sslmode=require`,
/// `sqlite:///abs/path.db`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Url {
    /// Lowercase, as written: `postgresql`, `mysql2`, `file`…
    pub scheme: String,
    /// Empty for the default socket; a path for another socket.
    pub host: String,
    pub port: Option<u16>,
    pub user: String,
    pub password: String,
    /// The database's name, or a SQLite file's path as written.
    pub database: String,
    /// The query's pairs (`sslmode`…), a socket's `host=` apart.
    pub params: Vec<(String, String)>,
}

fn file_scheme(scheme: &str) -> bool {
    matches!(scheme, "sqlite" | "sqlite3" | "file")
}

pub fn parse_url(url: &str) -> Result<Url> {
    let (scheme, rest) = url.trim().split_once(':').context("not a URL")?;
    let scheme = scheme.to_ascii_lowercase();
    ensure!(
        !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)),
        "not a URL"
    );
    let mut out = Url {
        scheme,
        ..Url::default()
    };
    if file_scheme(&out.scheme) {
        let path = rest.strip_prefix("//").unwrap_or(rest);
        let path = path.split(['?', '#']).next().unwrap_or_default();
        // `sqlite:////abs/path` is an absolute path.
        let path = if path.starts_with("//") {
            &path[1..]
        } else {
            path
        };
        ensure!(!path.is_empty(), "the URL names no file");
        out.database = path.into();
        return Ok(out);
    }
    let rest = rest.strip_prefix("//").context("not a URL")?;
    let rest = rest.split('#').next().unwrap_or_default();
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (userinfo, hostport) = authority.rsplit_once('@').unwrap_or(("", authority));
    let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    let (host, port) = match hostport.strip_prefix('[') {
        Some(v6) => {
            let (host, port) = v6.split_once(']').context("the URL has a bad host")?;
            (host, port.strip_prefix(':'))
        }
        None => match hostport.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (hostport, None),
        },
    };
    out.host = decode(host);
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let (key, value) = (decode(key), decode(value));
        if key == "host" || key == "socket" {
            out.host = value;
        } else {
            out.params.push((key, value));
        }
    }
    out.port = port
        .filter(|p| !p.is_empty())
        .map(str::parse)
        .transpose()
        .context("the URL has a bad port")?;
    out.user = decode(user);
    out.password = decode(password);
    out.database = decode(path);
    Ok(out)
}

/// A database a repository declares, and where: `.env:DATABASE_URL`, `.env:DB_CONNECTION`, `config/database.yml`,
/// `config/database.php`, `prisma/schema.prisma`, `compose.yaml:db`. The scheme says its kind (`postgres`, `mysql`,
/// `mariadb`, `sqlite`, `clickhouse`, `sqlserver`, `mongodb`, `redis`, or another as written); a SQLite path is
/// absolute.
#[derive(Clone, Debug, PartialEq)]
pub struct Declared {
    pub source: String,
    pub url: Url,
}

fn normalized(scheme: &str) -> &str {
    match scheme {
        "postgresql" | "pgsql" | "postgis" => "postgres",
        "mysql2" | "trilogy" => "mysql",
        "sqlite3" | "file" => "sqlite",
        "clickhouse+native" | "clickhouse+http" | "ch" => "clickhouse",
        "mssql" => "sqlserver",
        "mongodb+srv" => "mongodb",
        "rediss" => "redis",
        other => other,
    }
}

/// The databases `repo` declares, each once.
pub fn declared(repo: &Path) -> Vec<Declared> {
    let mut found: Vec<Declared> = Vec::new();
    let prisma = fs::read_to_string(repo.join("prisma/schema.prisma")).ok();
    let mut add = |source: String, mut url: Url| {
        url.scheme = normalized(&url.scheme).to_owned();
        if url.scheme == "sqlite" && !Path::new(&url.database).is_absolute() {
            // Prisma reads a relative `file:` from its schema's folder.
            let from = if prisma.is_some() && url.database.starts_with("./") {
                repo.join("prisma")
            } else {
                repo.to_path_buf()
            };
            let path = url.database.strip_prefix("./").unwrap_or(&url.database);
            url.database = from.join(path).to_string_lossy().into_owned();
        }
        let same = |d: &Declared| {
            (
                &d.url.scheme,
                &d.url.host,
                d.url.port,
                &d.url.database,
                &d.url.user,
            ) == (&url.scheme, &url.host, url.port, &url.database, &url.user)
        };
        if !found.iter().any(same) {
            found.push(Declared { source, url });
        }
    };
    let mut files: Vec<String> = fs::read_dir(repo)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|f| {
            f.starts_with(".env")
                && !["example", "sample", "template", "dist"]
                    .iter()
                    .any(|w| f.contains(w))
        })
        .collect();
    files.sort();
    let mut vars = Vec::new();
    for file in &files {
        let own = parse_env(&fs::read_to_string(repo.join(file)).unwrap_or_default());
        for (key, value) in &own {
            let named =
                key.contains("DATABASE") || key.starts_with("DB_") || key.ends_with("_DB_URL");
            let url_like =
                value.contains("://") || value.starts_with("sqlite:") || value.starts_with("file:");
            if named
                && url_like
                && let Ok(url) = parse_url(value)
            {
                add(format!("{file}:{key}"), url);
            }
        }
        if let (Some(driver), Ok(conn)) = (get(&own, "DB_CONNECTION"), laravel(&own)) {
            add(format!("{file}:DB_CONNECTION"), conn_url(driver, conn));
        }
        vars.extend(own);
    }
    if let Some(conn) = fs::read_to_string(repo.join("config/database.yml"))
        .ok()
        .and_then(|t| yaml(&t).ok())
    {
        let engine = conn.engine.label();
        add("config/database.yml".into(), conn_url(engine, conn));
    }
    if get(&vars, "DB_CONNECTION").is_none()
        && let Ok(php) = fs::read_to_string(repo.join("config/database.php"))
    {
        let defaults = php_defaults(&php);
        if let (Some(driver), Ok(conn)) = (get(&defaults, "DB_CONNECTION"), laravel(&defaults)) {
            add("config/database.php".into(), conn_url(driver, conn));
        }
    }
    if let Some(url) = prisma
        .as_deref()
        .and_then(|p| prisma_url(p, &vars))
        .and_then(|u| parse_url(&u).ok())
    {
        add("prisma/schema.prisma".into(), url);
    }
    for (source, url) in compose(repo, &vars) {
        add(source, url);
    }
    found
}

fn conn_url(driver: &str, conn: Conn) -> Url {
    let scheme = match driver {
        "pgsql" => "postgres",
        other => normalized(other),
    };
    Url {
        scheme: scheme.into(),
        host: conn.host,
        port: conn.port,
        user: conn.user,
        password: conn.password,
        database: conn.name,
        params: Vec::new(),
    }
}

/// `env('DB_HOST', '127.0.0.1')` in a Laravel config: the first literal default of each key.
fn php_defaults(php: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let unquote = |s: &str| s.trim().trim_matches(['\'', '"']).to_owned();
    for piece in php.split("env(").skip(1) {
        let args: Vec<&str> = piece
            .split(')')
            .next()
            .unwrap_or_default()
            .split(',')
            .collect();
        if let [key, default] = args.as_slice() {
            let key = unquote(key);
            if default.trim().starts_with(['\'', '"']) && get(&out, &key).is_none() {
                out.push((key, unquote(default)));
            }
        }
    }
    out
}

/// The URL of a Prisma schema's datasource: written in it, or from the env variable it names.
fn prisma_url(schema: &str, vars: &[(String, String)]) -> Option<String> {
    let block = schema.split("datasource").nth(1)?.split('}').next()?;
    let line = block.lines().find(|l| l.trim_start().starts_with("url"))?;
    let value = line.split_once('=')?.1.trim();
    match value.strip_prefix("env(") {
        Some(name) => get(vars, name.trim_end_matches(')').trim_matches('"')).map(str::to_owned),
        None => Some(value.trim_matches('"').to_owned()),
    }
}

/// The database servers of the repository's Compose file as this machine reaches them: on 127.0.0.1 at their
/// published port, with the user, password and database of their image's environment.
fn compose(repo: &Path, vars: &[(String, String)]) -> Vec<(String, Url)> {
    let Some((file, doc)) = crate::detect::COMPOSE_FILES.iter().find_map(|f| {
        let text = fs::read_to_string(repo.join(f)).ok()?;
        Some((
            *f,
            YamlLoader::load_from_str(&text).ok()?.into_iter().next()?,
        ))
    }) else {
        return Vec::new();
    };
    let Yaml::Hash(services) = &doc["services"] else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (name, service) in services {
        let (Some(name), Some(image)) = (name.as_str(), service["image"].as_str()) else {
            continue;
        };
        let image = image.rsplit('/').next().unwrap_or(image).to_lowercase();
        let image = image.split(':').next().unwrap_or_default();
        let (scheme, port) = match image {
            i if i.starts_with("postgres") || i.starts_with("postgis") || i == "timescaledb" => {
                ("postgres", 5432)
            }
            i if i.starts_with("mariadb") => ("mariadb", 3306),
            i if i.starts_with("mysql") || i.starts_with("percona") => ("mysql", 3306),
            i if i.starts_with("clickhouse") => ("clickhouse", 9000),
            i if i.starts_with("mssql") || i.starts_with("azure-sql-edge") => ("sqlserver", 1433),
            i if i.starts_with("mongo") => ("mongodb", 27017),
            i if i.starts_with("redis") || i.starts_with("valkey") => ("redis", 6379),
            _ => continue,
        };
        let env = environment(&service["environment"], vars);
        let var = |keys: &[&str]| keys.iter().find_map(|k| get(&env, k).map(str::to_owned));
        let (user, password, database) = match scheme {
            "postgres" => {
                let user = var(&["POSTGRES_USER"]).unwrap_or_else(|| "postgres".into());
                let db = var(&["POSTGRES_DB"]).unwrap_or_else(|| user.clone());
                (user, var(&["POSTGRES_PASSWORD"]), db)
            }
            "mysql" | "mariadb" => {
                let db = var(&["MYSQL_DATABASE", "MARIADB_DATABASE"]).unwrap_or_default();
                match var(&["MYSQL_USER", "MARIADB_USER"]) {
                    Some(user) => (user, var(&["MYSQL_PASSWORD", "MARIADB_PASSWORD"]), db),
                    None => (
                        "root".into(),
                        var(&["MYSQL_ROOT_PASSWORD", "MARIADB_ROOT_PASSWORD"]),
                        db,
                    ),
                }
            }
            "clickhouse" => (
                var(&["CLICKHOUSE_USER"]).unwrap_or_else(|| "default".into()),
                var(&["CLICKHOUSE_PASSWORD"]),
                var(&["CLICKHOUSE_DB"]).unwrap_or_else(|| "default".into()),
            ),
            _ => (String::new(), None, String::new()),
        };
        let published = service["ports"]
            .as_vec()
            .into_iter()
            .flatten()
            .find_map(|p| published(p, port));
        out.push((
            format!("{file}:{name}"),
            Url {
                scheme: scheme.into(),
                host: "127.0.0.1".into(),
                port: Some(published.unwrap_or(port)),
                user,
                password: password.unwrap_or_default(),
                database,
                params: Vec::new(),
            },
        ));
    }
    out
}

/// The host's port a Compose `ports` entry publishes for the server's `port`: `"5433:5432"`,
/// `"127.0.0.1:5433:5432/tcp"`, or the long form.
fn published(entry: &Yaml, port: u16) -> Option<u16> {
    if let Yaml::Hash(_) = entry {
        let target = entry["target"].as_i64()?;
        return (target == i64::from(port))
            .then(|| {
                entry["published"]
                    .as_i64()
                    .and_then(|p| u16::try_from(p).ok())
            })
            .flatten();
    }
    let text = match entry {
        Yaml::String(s) => s.clone(),
        Yaml::Integer(i) => i.to_string(),
        _ => return None,
    };
    let parts: Vec<&str> = text.split('/').next()?.split(':').collect();
    let (host, inner) = match parts.as_slice() {
        // The container's port alone: Docker publishes it on a port of its choosing.
        [_] => return None,
        [host, inner] | [_, host, inner] => (*host, *inner),
        _ => return None,
    };
    (inner.parse::<u16>().ok()? == port)
        .then(|| host.parse().ok())
        .flatten()
}

/// A Compose service's environment, a map or a list of `KEY=value`, with `${VAR}` and `${VAR:-default}` read from
/// the repository's env files.
fn environment(env: &Yaml, vars: &[(String, String)]) -> Vec<(String, String)> {
    let pairs: Vec<(String, String)> = match env {
        Yaml::Hash(map) => map
            .iter()
            .filter_map(|(k, v)| {
                let value = match v {
                    Yaml::String(s) => s.clone(),
                    Yaml::Integer(i) => i.to_string(),
                    Yaml::Boolean(b) => b.to_string(),
                    _ => String::new(),
                };
                Some((k.as_str()?.to_owned(), value))
            })
            .collect(),
        Yaml::Array(list) => list
            .iter()
            .filter_map(|v| {
                let (k, v) = v.as_str()?.split_once('=')?;
                Some((k.to_owned(), v.to_owned()))
            })
            .collect(),
        _ => Vec::new(),
    };
    pairs
        .into_iter()
        .map(|(k, v)| {
            let value = match v.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
                Some(inner) => {
                    let (name, default) = inner
                        .split_once(":-")
                        .or_else(|| inner.split_once('-'))
                        .unwrap_or((inner, ""));
                    get(vars, name).unwrap_or(default).to_owned()
                }
                None => v,
            };
            (k, value)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_read_with_their_options_and_a_socket() {
        let u = parse_url(
            "postgresql://alex:p%40ss@db.acme.dev:6543/acme?sslmode=require&application_name=x",
        )
        .unwrap();
        assert_eq!(
            (
                u.scheme.as_str(),
                u.host.as_str(),
                u.port,
                u.user.as_str(),
                u.password.as_str(),
                u.database.as_str()
            ),
            (
                "postgresql",
                "db.acme.dev",
                Some(6543),
                "alex",
                "p@ss",
                "acme"
            )
        );
        assert_eq!(u.params[0], ("sslmode".into(), "require".into()));
        let u = parse_url("mysql://root@[::1]:3307/app").unwrap();
        assert_eq!((u.host.as_str(), u.port), ("::1", Some(3307)));
        let u = parse_url("postgres:///app?host=/var/run/postgresql").unwrap();
        assert_eq!(
            (u.host.as_str(), u.database.as_str()),
            ("/var/run/postgresql", "app")
        );
        assert_eq!(
            parse_url("sqlite:////srv/app.db").unwrap().database,
            "/srv/app.db"
        );
        assert_eq!(parse_url("sqlite:./dev.db").unwrap().database, "./dev.db");
        assert!(parse_url("postgres://h:notaport/db").is_err());
        assert!(parse_url("just words").is_err());
    }

    #[test]
    fn a_repository_declares_its_databases_in_many_places() {
        let dir = std::env::temp_dir().join(format!("termf-declared-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("prisma")).unwrap();
        fs::create_dir_all(dir.join("config")).unwrap();
        fs::write(
            dir.join(".env"),
            "DATABASE_URL=postgres://acme:secret@127.0.0.1:5432/acme\nANALYTICS_DATABASE_URL=clickhouse://default@localhost:9000/events\nDB_CONNECTION=mariadb\nDB_HOST=127.0.0.1\nDB_DATABASE=shop\nDB_USERNAME=shop\nDB_PASSWORD=pw\nDB_PASS=${X}\n",
        )
        .unwrap();
        fs::write(
            dir.join(".env.example"),
            "DATABASE_URL=postgres://u:p@example/db\n",
        )
        .unwrap();
        fs::write(
            dir.join("prisma/schema.prisma"),
            "datasource db {\n  provider = \"sqlite\"\n  url      = \"file:./dev.db\"\n}\n",
        )
        .unwrap();
        fs::write(
            dir.join("docker-compose.yml"),
            "services:\n  db:\n    image: postgres:16\n    environment:\n      POSTGRES_PASSWORD: ${PGPW:-devpw}\n      POSTGRES_DB: acme_dev\n    ports:\n      - \"5433:5432\"\n  cache:\n    image: redis:7\n  web:\n    image: node:22\n",
        )
        .unwrap();
        let found = declared(&dir);
        let by = |source: &str| {
            found
                .iter()
                .find(|d| d.source == source)
                .unwrap_or_else(|| panic!("{source}: {found:?}"))
        };
        let pg = by(".env:DATABASE_URL");
        assert_eq!(
            (pg.url.scheme.as_str(), pg.url.password.as_str()),
            ("postgres", "secret")
        );
        assert_eq!(by(".env:ANALYTICS_DATABASE_URL").url.scheme, "clickhouse");
        let maria = by(".env:DB_CONNECTION");
        assert_eq!(
            (maria.url.scheme.as_str(), maria.url.database.as_str()),
            ("mariadb", "shop")
        );
        let lite = by("prisma/schema.prisma");
        assert_eq!(lite.url.scheme, "sqlite");
        assert_eq!(
            lite.url.database,
            dir.join("prisma/dev.db").to_string_lossy()
        );
        let compose = by("docker-compose.yml:db");
        assert_eq!(
            (
                compose.url.port,
                compose.url.user.as_str(),
                compose.url.password.as_str(),
                compose.url.database.as_str()
            ),
            (Some(5433), "postgres", "devpw", "acme_dev")
        );
        assert_eq!(by("docker-compose.yml:cache").url.scheme, "redis");
        assert!(
            found.iter().all(|d| !d.source.contains("example")),
            "{found:?}"
        );
        assert!(found.iter().all(|d| !d.source.ends_with(":web")));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_compose_port_counts_only_when_published_on_a_port_of_the_host() {
        let s = |text: &str| Yaml::String(text.into());
        assert_eq!(published(&s("5433:5432"), 5432), Some(5433));
        assert_eq!(published(&s("127.0.0.1:5433:5432/tcp"), 5432), Some(5433));
        assert_eq!(
            published(&s("5432"), 5432),
            None,
            "Docker picks the host's port"
        );
        assert_eq!(published(&Yaml::Integer(5432), 5432), None);
        assert_eq!(published(&s("8080:80"), 5432), None);
        let long = YamlLoader::load_from_str("target: 5432\npublished: 6432\n").unwrap();
        assert_eq!(published(&long[0], 5432), Some(6432));
    }

    #[test]
    fn laravel_defaults_come_from_its_config_without_an_env_file() {
        let php = "'default' => env('DB_CONNECTION', 'sqlite'),\n'database' => env('DB_DATABASE', database_path('database.sqlite')),\n'host' => env('DB_HOST', '127.0.0.1'),";
        let d = php_defaults(php);
        assert_eq!(get(&d, "DB_CONNECTION"), Some("sqlite"));
        assert_eq!(get(&d, "DB_HOST"), Some("127.0.0.1"));
        assert_eq!(
            get(&d, "DB_DATABASE"),
            None,
            "a computed default is not a value"
        );
    }
}
