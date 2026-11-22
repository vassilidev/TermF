//! Ops (ARCHITECTURE §12.3–§12.5): a project's connections to its servers, databases, clusters and logs, and the
//! recipes that read (or write) through them. A connection lives in the project's file (`[[ops.connections]]`),
//! its secrets as names of the vault's secrets, never values; a recipe is TOML, shipped in `library/recipes/` or
//! the project's own in `projects/<id>/recipes/`. Running one validates its parameters, builds the exact command
//! (arguments apart, each quoted for a remote shell), runs the system's own client (`ssh`, `psql`, `mysql`,
//! `kubectl`, `aws`, `gcloud`, `logcli`, `curl`) within its limits, then masks what it printed ([`redact`]).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

pub mod env;
mod exec;
mod flow;
mod recipe;
pub mod redact;
pub mod sql;
#[cfg(test)]
mod tests;

pub use exec::{Check, Following, Raw, Runner, Secrets, vault_first};
pub use flow::{
    APPROVAL_WAIT, Agent, Approvals, Ops, Request, Seen, SeenConnection, SeenRecipe, Streamed, Use,
};
pub use recipe::{
    Approval, Arg, Limits, Mode, Param, ParamKind, Plan, Recipe, Variant, check as check_param,
    plan, quote,
};
pub use redact::{Output, Part, Redact};
pub use termf_vault::Scope as Env;

/// What an agent may see of a connection or a recipe: nothing, an output where personal data is masked too, or the
/// output with only the secrets masked. Ordered: the effective access is the lower of the two.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum AiAccess {
    #[default]
    None,
    Redacted,
    Full,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Ssh,
    Postgres,
    Mysql,
    Kubernetes,
    Cloudwatch,
    Gcp,
    Loki,
    Datadog,
}

impl Kind {
    /// The system's client TermF runs.
    pub fn program(self) -> &'static str {
        match self {
            Kind::Ssh => "ssh",
            Kind::Postgres => "psql",
            Kind::Mysql => "mysql",
            Kind::Kubernetes => "kubectl",
            Kind::Cloudwatch => "aws",
            Kind::Gcp => "gcloud",
            Kind::Loki => "logcli",
            Kind::Datadog => "curl",
        }
    }

    /// The secrets its connection may name, by role.
    fn secrets(self) -> &'static [&'static str] {
        match self {
            Kind::Postgres | Kind::Mysql => &["password"],
            Kind::Loki => &["token"],
            Kind::Datadog => &["api_key", "app_key"],
            _ => &[],
        }
    }
}

/// What to install when a client is missing.
pub fn install_hint(program: &str) -> &'static str {
    match program {
        "ssh" => "the OpenSSH client (ssh)",
        "psql" => {
            "PostgreSQL's client: brew install libpq, apt install postgresql-client, or the PostgreSQL installer"
        }
        "mysql" => {
            "the MySQL client: brew install mysql-client, apt install mysql-client (or mariadb-client)"
        }
        "kubectl" => "kubectl: https://kubernetes.io/docs/tasks/tools/",
        "aws" => "the AWS CLI v2: https://aws.amazon.com/cli/",
        "gcloud" => "the Google Cloud CLI: https://cloud.google.com/sdk/docs/install",
        "logcli" => "Loki's logcli: https://grafana.com/docs/loki/latest/query/logcli/",
        "curl" => "curl",
        _ => "it",
    }
}

/// A way into an environment of the project. Its keys are those of `[[ops.connections]]`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub name: String,
    pub kind: Kind,
    pub env: Env,
    #[serde(default)]
    pub ai_access: AiAccess,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// SSH: the destination (an alias of `~/.ssh/config`, or a host name); a database: its host, as the SSH
    /// server of `via` sees it when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// SSH: the login; a database: its user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// SSH: `ProxyJump`, a bastion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<String>,
    /// A database: the SSH connection of the project whose server TermF opens a tunnel through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Kubernetes: the kubectl context, and the namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// CloudWatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Google Cloud.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gcp_project: Option<String>,
    /// Loki.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Datadog: `datadoghq.com` by default, `datadoghq.eu`…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    /// Role (`password`, `token`, `api_key`, `app_key`) → the name of a secret of the vault, in the connection's
    /// environment, the project's or else a global one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: BTreeMap<String, String>,
}

/// A name TermF puts on a command line: no option, no space, no control character.
fn word(what: &str, value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 256
            && !value.starts_with('-')
            && !value.chars().any(|c| c.is_whitespace() || c.is_control()),
        "{what} {value:?}: no spaces, nothing that starts with -, 256 characters at most"
    );
    Ok(())
}

/// Like a vault secret's name: letters, digits, `_ . -`, starting with a letter or `_`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
}

impl Connection {
    /// Whether everything its kind needs is there, and nothing could become an option of its client.
    pub fn check(&self) -> Result<()> {
        ensure!(
            valid_name(&self.name),
            "a connection's name holds letters, digits, _ . - and starts with a letter"
        );
        ensure!(
            self.description.chars().count() <= 300,
            "a description has 300 characters at most"
        );
        let fields = [
            ("host", &self.host),
            ("user", &self.user),
            ("jump", &self.jump),
            ("via", &self.via),
            ("database", &self.database),
            ("context", &self.context),
            ("namespace", &self.namespace),
            ("log_group", &self.log_group),
            ("region", &self.region),
            ("profile", &self.profile),
            ("gcp_project", &self.gcp_project),
            ("url", &self.url),
            ("site", &self.site),
        ];
        for (what, value) in fields {
            if let Some(value) = value {
                word(what, value)?;
            }
        }
        let need = |what: &str, value: &Option<String>| -> Result<()> {
            ensure!(value.is_some(), "a {:?} connection needs {what}", self.kind);
            Ok(())
        };
        match self.kind {
            Kind::Ssh => need("host", &self.host)?,
            Kind::Postgres | Kind::Mysql => {
                need("database", &self.database)?;
                need("user", &self.user)?;
                if self.via.is_none() {
                    need("host (or via)", &self.host)?;
                }
            }
            Kind::Kubernetes => need("context", &self.context)?,
            Kind::Cloudwatch => need("log_group", &self.log_group)?,
            Kind::Gcp => {}
            Kind::Loki => {
                need("url", &self.url)?;
                let url = self.url.as_deref().unwrap_or_default();
                ensure!(
                    url.starts_with("https://") || url.starts_with("http://"),
                    "Loki's url starts with https://"
                );
            }
            Kind::Datadog => {
                for key in ["api_key", "app_key"] {
                    ensure!(
                        self.secrets.contains_key(key),
                        "a Datadog connection names its {key} secret"
                    );
                }
                if let Some(site) = &self.site {
                    ensure!(
                        site.chars()
                            .all(|c| c.is_ascii_alphanumeric() || ".-".contains(c)),
                        "Datadog's site is a host name, such as datadoghq.eu"
                    );
                }
            }
        }
        for (role, secret) in &self.secrets {
            ensure!(
                self.kind.secrets().contains(&role.as_str()),
                "a {:?} connection has no {role} secret ({})",
                self.kind,
                self.kind.secrets().join(", ")
            );
            ensure!(
                valid_name(secret),
                "{role} is the name of a secret of the vault, never its value"
            );
        }
        Ok(())
    }

    /// Where it reaches, as the approval card says it.
    pub fn target(&self, all: &[Connection]) -> String {
        let at = |user: &Option<String>, host: &str| match user {
            Some(user) => format!("{user}@{host}"),
            None => host.to_owned(),
        };
        match self.kind {
            Kind::Ssh => {
                let host = at(&self.user, self.host.as_deref().unwrap_or_default());
                match &self.jump {
                    Some(jump) => format!("{host} (via {jump})"),
                    None => host,
                }
            }
            Kind::Postgres | Kind::Mysql => {
                let db = format!(
                    "{}/{}",
                    at(&self.user, self.host.as_deref().unwrap_or("127.0.0.1")),
                    self.database.as_deref().unwrap_or_default()
                );
                match self
                    .via
                    .as_ref()
                    .and_then(|v| all.iter().find(|c| &c.name == v))
                {
                    Some(ssh) => format!("{db} (tunnel through {})", ssh.target(all)),
                    None => db,
                }
            }
            Kind::Kubernetes => match &self.namespace {
                Some(ns) => format!("{} / {ns}", self.context.as_deref().unwrap_or_default()),
                None => self.context.clone().unwrap_or_default(),
            },
            Kind::Cloudwatch => self.log_group.clone().unwrap_or_default(),
            Kind::Gcp => self.gcp_project.clone().unwrap_or_else(|| "gcloud".into()),
            Kind::Loki => self.url.clone().unwrap_or_default(),
            Kind::Datadog => format!("api.{}", self.site.as_deref().unwrap_or("datadoghq.com")),
        }
    }
}

#[derive(Deserialize, Default)]
struct ProjectOps {
    #[serde(default)]
    ops: OpsTable,
}

#[derive(Deserialize, Default)]
struct OpsTable {
    #[serde(default)]
    connections: Vec<Connection>,
}

/// The project's connections, as its file lists them.
pub fn connections(config_dir: &Path, project: &str) -> Result<Vec<Connection>> {
    let file = termf_core::project_file(config_dir, project)?;
    let text = fs::read_to_string(&file)?;
    let ops: ProjectOps =
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
    Ok(ops.ops.connections)
}

/// Adds a connection, or replaces the one named `previous` (its name may change). Comments and the rest of the
/// file stay as they are.
pub fn save_connection(
    config_dir: &Path,
    project: &str,
    connection: &Connection,
    previous: Option<&str>,
) -> Result<Vec<Connection>> {
    connection.check()?;
    let all = connections(config_dir, project)?;
    let taken = all
        .iter()
        .any(|c| c.name == connection.name && Some(c.name.as_str()) != previous);
    ensure!(
        !taken,
        "the project already has a connection named {}",
        connection.name
    );
    if let Some(via) = &connection.via {
        let ssh = all.iter().find(|c| &c.name == via && c.kind == Kind::Ssh);
        ensure!(
            ssh.is_some(),
            "via names one of the project's SSH connections"
        );
    }
    // The databases that open their tunnel through it follow a new name; it stays an SSH connection for them.
    let users: Vec<&str> = previous
        .map(|p| {
            all.iter()
                .filter(|c| c.via.as_deref() == Some(p))
                .map(|c| c.name.as_str())
                .collect()
        })
        .unwrap_or_default();
    if let Some(user) = users.first().filter(|_| connection.kind != Kind::Ssh) {
        bail!(
            "{user} opens its tunnel through {}: it stays an SSH connection",
            previous.unwrap_or_default()
        );
    }
    let renamed = previous.filter(|p| *p != connection.name);
    edit(config_dir, project, |list| {
        if let Some(old) = renamed {
            for table in list.iter_mut() {
                if table.get("via").and_then(Item::as_str) == Some(old) {
                    table["via"] = toml_edit::value(connection.name.as_str());
                }
            }
        }
        let mut table: Table = toml::to_string(connection)?
            .parse::<DocumentMut>()?
            .as_table()
            .clone();
        if let Some(Item::Table(secrets)) = table.remove("secrets") {
            table.insert("secrets", toml_edit::value(secrets.into_inline_table()));
        }
        table.set_implicit(false);
        let at = previous.and_then(|p| list.iter().position(|t| named(t, p)));
        match at {
            Some(i) => *list.get_mut(i).expect("found") = table,
            None => {
                ensure!(
                    previous.is_none(),
                    "no connection named {}",
                    previous.unwrap_or_default()
                );
                list.push(table);
            }
        }
        Ok(())
    })?;
    connections(config_dir, project)
}

pub fn delete_connection(config_dir: &Path, project: &str, name: &str) -> Result<Vec<Connection>> {
    let used = connections(config_dir, project)?
        .into_iter()
        .find(|c| c.via.as_deref() == Some(name));
    if let Some(user) = used {
        bail!(
            "{} opens its tunnel through {name}: change it first",
            user.name
        );
    }
    edit(config_dir, project, |list| {
        let before = list.len();
        list.retain(|t| !named(t, name));
        ensure!(list.len() < before, "no connection named {name}");
        Ok(())
    })?;
    connections(config_dir, project)
}

fn named(table: &Table, name: &str) -> bool {
    table.get("name").and_then(Item::as_str) == Some(name)
}

fn edit(
    config_dir: &Path,
    project: &str,
    change: impl FnOnce(&mut ArrayOfTables) -> Result<()>,
) -> Result<()> {
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
    let list = ops
        .entry("connections")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .with_context(|| {
            format!(
                "`ops.connections` in {} must be [[ops.connections]]",
                file.display()
            )
        })?;
    change(list)?;
    termf_core::write_atomic(&file, &doc.to_string())
}

/// Shipped recipes, then the project's (`projects/<id>/recipes/*.toml`), which replace a shipped one of the
/// same name. A file that does not read is reported in `errors`, the others still listed.
pub fn recipes(config_dir: &Path, project: Option<&str>) -> (Vec<Recipe>, Vec<String>) {
    let mut found: BTreeMap<String, Recipe> = BTreeMap::new();
    let mut errors = Vec::new();
    for (file, text) in termf_library::shipped_files("recipes") {
        match Recipe::parse(text, "shipped") {
            Ok(r) => {
                found.insert(r.name.clone(), r);
            }
            Err(e) => errors.push(format!("recipes/{file}: {e:#}")),
        }
    }
    if let Some(project) = project {
        let dir = project_recipes(config_dir, project);
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        files.sort();
        for path in files {
            match fs::read_to_string(&path)
                .map_err(anyhow::Error::from)
                .and_then(|text| Recipe::parse(&text, "project"))
            {
                Ok(r) => {
                    found.insert(r.name.clone(), r);
                }
                Err(e) => errors.push(format!("{}: {e:#}", path.display())),
            }
        }
    }
    (found.into_values().collect(), errors)
}

pub fn project_recipes(config_dir: &Path, project: &str) -> PathBuf {
    config_dir.join("projects").join(project).join("recipes")
}

/// Whether `recipe` runs on `connection`: named by it, or of a kind one of its variants handles.
pub fn runs_on(recipe: &Recipe, connection: &Connection) -> bool {
    match &recipe.connection {
        Some(name) => name == &connection.name,
        None => recipe.run.iter().any(|v| v.kind == connection.kind),
    }
}

/// What an agent may see of a recipe on a connection: the lower of the two, `None` hiding it.
pub fn ai_access(recipe: &Recipe, connection: &Connection) -> AiAccess {
    recipe.ai_access.min(connection.ai_access)
}
