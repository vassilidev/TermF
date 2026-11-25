//! Laravel Forge through its API, with `curl` as for Jira: the account's token goes to curl on its standard input,
//! never on a command line. A project names its Forge account in its file (`[ops.forge]`: a label and the name of
//! the vault's secret that holds the token); importing its servers and sites gives the project's Ops connections,
//! the vault's database passwords, and deploy targets.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use toml_edit::DocumentMut;

use crate::{AiAccess, Connection, Env, Kind, valid_name};

const API: &str = "https://forge.laravel.com/api/v1";

/// The project's Forge account: its label, and the vault's (global, prod) secret that holds its token.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub label: String,
    pub token: String,
}

/// The vault's name for an account's token: `FORGE_TOKEN_` and its label, as a secret's name may be.
pub fn token_name(label: &str) -> String {
    let word: String = label
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("FORGE_TOKEN_{}", word.trim_matches('_'))
}

#[derive(Deserialize, Default)]
struct Ops {
    #[serde(default)]
    ops: Table,
}

#[derive(Deserialize, Default)]
struct Table {
    forge: Option<Account>,
}

/// The project's Forge account, if it names one.
pub fn account(config_dir: &Path, project: &str) -> Result<Option<Account>> {
    let file = termf_core::project_file(config_dir, project)?;
    let ops: Ops = toml::from_str(&fs::read_to_string(&file)?)
        .with_context(|| format!("invalid {}", file.display()))?;
    Ok(ops.ops.forge)
}

/// Names the project's account (`None` forgets it); the rest of the file and its comments stay.
pub fn save_account(config_dir: &Path, project: &str, account: Option<&Account>) -> Result<()> {
    if let Some(a) = account {
        ensure!(
            !a.label.trim().is_empty()
                && a.label.chars().count() <= 60
                && !a.label.chars().any(char::is_control),
            "an account's label holds 60 characters at most, on one line"
        );
        ensure!(
            valid_name(&a.token),
            "{} is not a name the vault takes",
            a.token
        );
    }
    let file = termf_core::project_file(config_dir, project)?;
    let mut doc: DocumentMut = fs::read_to_string(&file)?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    let ops = doc
        .entry("ops")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .with_context(|| format!("`ops` in {} must be a table", file.display()))?;
    ops.set_implicit(true);
    match account {
        Some(a) => {
            let mut table = toml_edit::Table::new();
            table["label"] = toml_edit::value(a.label.trim());
            table["token"] = toml_edit::value(a.token.as_str());
            ops["forge"] = toml_edit::Item::Table(table);
        }
        None => {
            ops.remove("forge");
        }
    }
    termf_core::write_atomic(&file, &doc.to_string())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Server {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub ip_address: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Site {
    pub id: u64,
    pub server_id: u64,
    /// Its domain.
    pub name: String,
    #[serde(default)]
    pub directory: Option<String>,
    /// `acme/api`.
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub repository_branch: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Deployment {
    pub id: u64,
    /// `finished`, `failed`, `deploying`…
    pub status: String,
    #[serde(default)]
    pub commit_hash: Option<String>,
    #[serde(default)]
    pub commit_message: Option<String>,
    #[serde(default)]
    pub commit_author: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
}

/// An account's API.
pub struct Forge {
    base: String,
    token: String,
    curl: PathBuf,
}

impl Forge {
    pub fn new(token: String, curl: PathBuf) -> Self {
        Self::at(API, token, curl)
    }

    /// Another base: a local server, for tests.
    pub fn at(base: &str, token: String, curl: PathBuf) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            token,
            curl,
        }
    }

    /// One call; its body. `--disable` keeps the user's `.curlrc` out of it.
    fn call(&self, method: &str, path: &str) -> Result<String> {
        let quoted = |v: &str| format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""));
        ensure!(
            !self.token.chars().any(char::is_control),
            "the Forge token holds a line break: it cannot go in a header"
        );
        let config = [
            format!("url = {}", quoted(&format!("{}{path}", self.base))),
            format!("request = {}", quoted(method)),
            format!(
                "header = {}",
                quoted(&format!("Authorization: Bearer {}", self.token))
            ),
            "header = \"Accept: application/json\"".into(),
            "write-out = \"\\n%{http_code}\"".into(),
            "max-time = 60".into(),
            "silent\nshow-error\n".into(),
        ]
        .join("\n");
        let mut child = Command::new(&self.curl)
            .args(["--disable", "--config", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("cannot run curl, which TermF needs to reach Forge")?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(config.as_bytes());
        }
        let out = child.wait_with_output()?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() {
            bail!(
                "cannot reach Forge: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
        match code.trim() {
            "200" | "201" | "202" | "204" => Ok(body.to_owned()),
            "401" => bail!("Forge refused the token: check it in the vault"),
            "404" => bail!("Forge has no such server, site or deployment"),
            "429" => bail!("Forge asks to wait: too many calls for now"),
            other => bail!(
                "Forge answered {other}: {}",
                body.chars().take(300).collect::<String>()
            ),
        }
    }

    fn json<T: for<'de> Deserialize<'de>>(&self, path: &str, field: &str) -> Result<T> {
        let body: Value =
            serde_json::from_str(&self.call("GET", path)?).context("Forge's answer is not JSON")?;
        serde_json::from_value(body[field].clone())
            .with_context(|| format!("Forge's answer has no {field}"))
    }

    pub fn servers(&self) -> Result<Vec<Server>> {
        self.json("/servers", "servers")
    }

    pub fn sites(&self, server: u64) -> Result<Vec<Site>> {
        self.json(&format!("/servers/{server}/sites"), "sites")
    }

    /// The site's `.env`, which holds its database's password: read only to import it.
    pub fn env(&self, server: u64, site: u64) -> Result<String> {
        self.call("GET", &format!("/servers/{server}/sites/{site}/env"))
    }

    pub fn deployments(&self, server: u64, site: u64) -> Result<Vec<Deployment>> {
        self.json(
            &format!("/servers/{server}/sites/{site}/deployment-history"),
            "deployments",
        )
    }

    pub fn output(&self, server: u64, site: u64, deployment: u64) -> Result<String> {
        self.json(
            &format!("/servers/{server}/sites/{site}/deployment-history/{deployment}/output"),
            "output",
        )
    }

    /// Starts the site's deployment script; Forge answers before it ends.
    pub fn deploy(&self, server: u64, site: u64) -> Result<()> {
        self.call(
            "POST",
            &format!("/servers/{server}/sites/{site}/deployment/deploy"),
        )
        .map(drop)
    }
}

/// A name the project's connections take, from a server's or a site's.
fn named(name: &str) -> String {
    let n: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let n = n
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_owned();
    if n.starts_with(|c: char| c.is_ascii_alphabetic()) {
        n
    } else {
        format!("forge-{n}")
    }
}

/// What importing a site makes: its server's SSH connection (as `forge`), its database's through that server, and the
/// password that goes to the vault, never to the project.
#[derive(Debug, Default, PartialEq)]
pub struct Imported {
    pub connections: Vec<Connection>,
    /// The vault's secret name, and its value.
    pub secrets: Vec<(String, String)>,
}

fn blank(name: String, kind: Kind) -> Connection {
    Connection {
        name,
        kind,
        env: Env::Prod,
        ai_access: AiAccess::Redacted,
        description: String::new(),
        host: None,
        port: None,
        user: None,
        jump: None,
        via: None,
        database: None,
        sslmode: None,
        context: None,
        namespace: None,
        log_group: None,
        region: None,
        profile: None,
        gcp_project: None,
        url: None,
        site: None,
        path: None,
        secrets: BTreeMap::new(),
    }
}

/// The server's SSH connection, with the site's folder when it is the only one imported there.
pub fn server_connection(server: &Server, folder: Option<&str>) -> Result<Connection> {
    let mut ssh = blank(named(&server.name), Kind::Ssh);
    ssh.host = Some(
        server
            .ip_address
            .clone()
            .context("Forge gives this server no IP address")?,
    );
    ssh.user = Some("forge".into());
    ssh.path = folder.map(str::to_owned);
    ssh.description = format!("Forge · {}", server.name);
    ssh.check()?;
    Ok(ssh)
}

/// The site's database, from its `.env`, through the server's SSH connection `via`; none for SQLite or no database.
pub fn site_database(site: &Site, env: &BTreeMap<String, String>, via: &str) -> Result<Imported> {
    let kind = match env.get("DB_CONNECTION").map(String::as_str) {
        Some("mysql") => Kind::Mysql,
        Some("mariadb") => Kind::Mariadb,
        Some("pgsql") => Kind::Postgres,
        _ => return Ok(Imported::default()),
    };
    let Some(database) = env.get("DB_DATABASE").filter(|d| !d.is_empty()) else {
        return Ok(Imported::default());
    };
    let name = format!("{}-db", named(&site.name));
    let mut db = blank(name.clone(), kind);
    db.host = Some(
        env.get("DB_HOST")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".into()),
    );
    db.port = env.get("DB_PORT").and_then(|p| p.parse().ok());
    db.database = Some(database.clone());
    db.user = env.get("DB_USERNAME").cloned();
    db.via = Some(via.to_owned());
    db.description = format!("Forge · {}", site.name);
    let mut out = Imported::default();
    if let Some(password) = env.get("DB_PASSWORD").filter(|p| !p.is_empty()) {
        let secret: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>()
            + "_PASSWORD";
        db.secrets.insert("password".into(), secret.clone());
        out.secrets.push((secret, password.clone()));
    }
    db.check()?;
    out.connections.push(db);
    Ok(out)
}
