//! Laravel Forge through its API, with `curl` as for Jira: the account's token goes to curl on its standard input,
//! never on a command line. A project names its Forge account in its file (`[ops.forge]`: a label, the name of
//! the vault's secret that holds the token, and its organization); importing its servers and sites gives the
//! project's Ops connections, the vault's database passwords, and deploy targets.

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

/// Today's API, by organization (JSON:API, cursor pages). The legacy `/api/v1` answers 404 to an organization's token.
const API: &str = "https://forge.laravel.com/api";

/// The project's Forge account: its label, the vault's (global, prod) secret that holds its token, and the slug of
/// its organization (none: the only one the token sees).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub label: String,
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
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
        ensure!(
            a.organization.as_deref().is_none_or(slug),
            "a Forge organization is named by its slug, such as acme"
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
            if let Some(org) = &a.organization {
                table["organization"] = toml_edit::value(org.as_str());
            }
            ops["forge"] = toml_edit::Item::Table(table);
        }
        None => {
            ops.remove("forge");
        }
    }
    termf_core::write_atomic(&file, &doc.to_string())
}

/// The other projects whose Forge account keeps its token under `token`: removing it would cut them off too.
pub fn sharing(config_dir: &Path, token: &str, except: &str) -> Result<Vec<String>> {
    Ok(termf_core::load_projects(config_dir)?
        .into_iter()
        .filter(|p| p.id != except)
        .filter(|p| {
            account(config_dir, &p.id)
                .ok()
                .flatten()
                .is_some_and(|a| a.token == token)
        })
        .map(|p| p.name)
        .collect())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Organization {
    pub slug: String,
    pub name: String,
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
    /// Its folder on the server, `/home/forge/acme.com`, ending in `/current` with zero-downtime deployments.
    #[serde(default)]
    pub directory: Option<String>,
    /// As Forge gives it: `acme/api`, or the repository's URL.
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

/// Why a call failed: Forge out of reach, or what it answered.
#[derive(Debug)]
pub enum Problem {
    Unreachable(String),
    Answered(u16, String),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreachable(why) => write!(f, "cannot reach Forge: {why}"),
            Self::Answered(401, _) => write!(f, "Forge refused the token: check it in the vault"),
            Self::Answered(403, _) => write!(
                f,
                "Forge forbids this to the token: give it the right to view organizations and servers"
            ),
            Self::Answered(404, _) => write!(
                f,
                "Forge has no such organization, server, site or deployment"
            ),
            Self::Answered(429, _) => write!(f, "Forge asks to wait: too many calls for now"),
            Self::Answered(code, body) => write!(
                f,
                "Forge answered {code}: {}",
                body.chars().take(300).collect::<String>()
            ),
        }
    }
}

impl std::error::Error for Problem {}

/// What testing an account's token found, for the user to read in words.
#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum Check {
    /// The token works: its organization, and how many servers Forge lists there.
    Ok {
        organization: Organization,
        servers: usize,
    },
    /// Forge refused the token (401).
    Refused,
    /// The token may not read the organizations or the servers (403).
    Forbidden,
    /// No organization to use: `wanted` is none of those the token sees, or it sees several (or none) and none was
    /// named.
    Organization {
        wanted: Option<String>,
        organizations: Vec<Organization>,
    },
    Unreachable {
        detail: String,
    },
    Failed {
        detail: String,
    },
}

impl Check {
    fn of(e: anyhow::Error) -> Self {
        match e.downcast_ref::<Problem>() {
            Some(Problem::Unreachable(why)) => Self::Unreachable {
                detail: why.clone(),
            },
            Some(Problem::Answered(401, _)) => Self::Refused,
            Some(Problem::Answered(403, _)) => Self::Forbidden,
            _ => Self::Failed {
                detail: format!("{e:#}"),
            },
        }
    }
}

/// An organization's slug, as Forge makes it: it goes in the API's paths.
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn id(item: &Value) -> Result<u64> {
    let id = &item["id"];
    id.as_u64()
        .or_else(|| id.as_str()?.parse().ok())
        .context("Forge's answer has a resource without its id")
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

/// The token and organization the Forge CLI keeps on this machine (`~/.laravel-forge/config.json`, as `forge login`
/// and `forge org:switch` write it), when it has a token.
pub fn cli_login(file: &Path) -> Option<(String, Option<String>)> {
    let config: Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    let token = config["token"]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())?;
    Some((token.to_owned(), text(&config["organization"])))
}

/// An account's API, in one of its organizations.
pub struct Forge {
    base: String,
    token: String,
    curl: PathBuf,
    org: String,
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
            org: String::new(),
        }
    }

    /// One call; its body. `--disable` keeps the user's `.curlrc` out of it.
    fn call(&self, method: &str, path: &str) -> Result<String> {
        let quoted = |v: &str| format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""));
        ensure!(
            !self.token.chars().any(char::is_control),
            "the Forge token holds a line break: it cannot go in a header"
        );
        let mut config = vec![
            format!("url = {}", quoted(&format!("{}{path}", self.base))),
            format!("request = {}", quoted(method)),
            format!(
                "header = {}",
                quoted(&format!("Authorization: Bearer {}", self.token))
            ),
            "header = \"Accept: application/json\"".into(),
            "header = \"Content-Type: application/json\"".into(),
            "write-out = \"\\n%{http_code}\"".into(),
            "max-time = 60".into(),
            "silent\nshow-error\n".into(),
        ];
        if method != "GET" {
            config.push("data = \"{}\"".into());
        }
        let mut child = Command::new(&self.curl)
            .args(["--disable", "--config", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("cannot run curl, which TermF needs to reach Forge")?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(config.join("\n").as_bytes());
        }
        let out = child.wait_with_output()?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
            return Err(Problem::Unreachable(why).into());
        }
        let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
        match code.trim().parse::<u16>() {
            Ok(200..=299) => Ok(body.to_owned()),
            Ok(code) => Err(Problem::Answered(code, body.to_owned()).into()),
            Err(_) => bail!("Forge's answer has no status"),
        }
    }

    fn get(&self, path: &str) -> Result<Value> {
        serde_json::from_str(&self.call("GET", path)?).context("Forge's answer is not JSON")
    }

    /// A list's resources, page after page.
    fn list(&self, path: &str) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut page = path.to_owned();
        // shortcut: 100 pages of 30 at most, enough for an account's servers and a server's sites.
        for _ in 0..100 {
            let mut body = self.get(&page)?;
            match body["data"].take() {
                Value::Array(items) => out.extend(items),
                _ => bail!("Forge's answer has no data"),
            }
            let Some(cursor) = body["meta"]["next_cursor"]
                .as_str()
                .filter(|c| !c.is_empty())
            else {
                break;
            };
            let cursor: String = cursor
                .bytes()
                .map(|b| match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect();
            page = format!("{path}?page%5Bcursor%5D={cursor}");
        }
        Ok(out)
    }

    /// A path of the organization's servers: `/orgs/<slug>/servers<rest>`.
    fn servers_path(&self, rest: &str) -> String {
        format!("/orgs/{}/servers{rest}", self.org)
    }

    /// One field of a single resource's attributes.
    fn attribute(&self, path: &str, field: &str) -> Result<String> {
        text(&self.get(path)?["data"]["attributes"][field])
            .with_context(|| format!("Forge's answer has no {field}"))
    }

    pub fn organizations(&self) -> Result<Vec<Organization>> {
        self.list("/orgs")?
            .iter()
            .map(|o| {
                Ok(Organization {
                    slug: text(&o["attributes"]["slug"])
                        .context("Forge's answer has an organization without its slug")?,
                    name: text(&o["attributes"]["name"]).unwrap_or_default(),
                })
            })
            .collect()
    }

    /// The organization the calls go to: `wanted`, else the only one the token sees.
    pub fn within(mut self, wanted: Option<&str>) -> Result<Self> {
        self.org = match wanted {
            Some(org) => {
                ensure!(slug(org), "{org} is not a Forge organization's slug");
                org.to_owned()
            }
            None => match self.organizations()?.as_slice() {
                [one] => one.slug.clone(),
                [] => bail!("this Forge token sees no organization"),
                many => bail!(
                    "this Forge token sees several organizations ({}): choose one in the Forge tab of Ops",
                    many.iter()
                        .map(|o| o.slug.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            },
        };
        Ok(self)
    }

    /// Tests the token: the organization `wanted` (else the only one it sees), and its servers.
    pub fn check(self, wanted: Option<&str>) -> Check {
        let organizations = match self.organizations() {
            Ok(o) => o,
            Err(e) => return Check::of(e),
        };
        let found = match wanted {
            Some(w) => organizations.iter().find(|o| o.slug == w),
            None if organizations.len() == 1 => organizations.first(),
            None => None,
        };
        let Some(organization) = found.cloned() else {
            return Check::Organization {
                wanted: wanted.map(str::to_owned),
                organizations,
            };
        };
        let api = Self {
            org: organization.slug.clone(),
            ..self
        };
        match api.servers() {
            Ok(servers) => Check::Ok {
                organization,
                servers: servers.len(),
            },
            Err(e) => Check::of(e),
        }
    }

    pub fn servers(&self) -> Result<Vec<Server>> {
        self.list(&self.servers_path(""))?
            .iter()
            .map(|s| {
                Ok(Server {
                    id: id(s)?,
                    name: text(&s["attributes"]["name"]).unwrap_or_default(),
                    ip_address: text(&s["attributes"]["ip_address"]),
                })
            })
            .collect()
    }

    pub fn sites(&self, server: u64) -> Result<Vec<Site>> {
        self.list(&self.servers_path(&format!("/{server}/sites")))?
            .iter()
            .map(|s| {
                let a = &s["attributes"];
                let name =
                    text(&a["name"]).context("Forge's answer has a site without its domain")?;
                let user = a["user"]
                    .as_str()
                    .filter(|u| !u.is_empty())
                    .unwrap_or("forge");
                let current = if a["zero_downtime_deployments"] == true {
                    "/current"
                } else {
                    ""
                };
                Ok(Site {
                    id: id(s)?,
                    server_id: server,
                    directory: Some(format!("/home/{user}/{name}{current}")),
                    repository: text(&a["repository"]["url"]),
                    repository_branch: text(&a["repository"]["branch"]),
                    name,
                })
            })
            .collect()
    }

    /// The site's `.env`, which holds its database's password: read only to import it.
    pub fn env(&self, server: u64, site: u64) -> Result<String> {
        self.attribute(
            &self.servers_path(&format!("/{server}/sites/{site}/environment")),
            "content",
        )
    }

    /// The site's latest deployments, newest first.
    pub fn deployments(&self, server: u64, site: u64) -> Result<Vec<Deployment>> {
        let body = self.get(&self.servers_path(&format!(
            "/{server}/sites/{site}/deployments?sort=-created_at"
        )))?;
        body["data"]
            .as_array()
            .context("Forge's answer has no data")?
            .iter()
            .map(|d| {
                let a = &d["attributes"];
                Ok(Deployment {
                    id: id(d)?,
                    status: text(&a["status"]).unwrap_or_default(),
                    commit_hash: text(&a["commit"]["hash"]),
                    commit_message: text(&a["commit"]["message"]),
                    commit_author: text(&a["commit"]["author"]),
                    started_at: text(&a["started_at"]),
                    ended_at: text(&a["ended_at"]),
                })
            })
            .collect()
    }

    pub fn output(&self, server: u64, site: u64, deployment: u64) -> Result<String> {
        self.attribute(
            &self.servers_path(&format!(
                "/{server}/sites/{site}/deployments/{deployment}/log"
            )),
            "output",
        )
    }

    /// Starts the site's deployment script; Forge answers before it ends.
    pub fn deploy(&self, server: u64, site: u64) -> Result<()> {
        self.call(
            "POST",
            &self.servers_path(&format!("/{server}/sites/{site}/deployments")),
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
