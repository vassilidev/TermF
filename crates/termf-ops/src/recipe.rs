//! Recipes (ARCHITECTURE §12.3): what a recipe takes, checked before anything runs, and the exact command it
//! makes on a connection. Parameters never go through a local shell: each is one argument; on a remote shell
//! (SSH) each argument is quoted. A value that starts with `-` is refused, so no parameter becomes an option.

use std::collections::BTreeMap;
use std::fmt;

use anyhow::{Context, Result, bail, ensure};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::exec::{self, Base};
use crate::{AiAccess, Connection, Env, Kind, Redact, valid_name};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Read,
    /// An agent may only propose it; a person runs it after typing the environment's name.
    Write,
}

/// When an agent's call waits for the user's approval: every time, or (`never`) not on dev and staging. `write`
/// is `always` for an agent, whose writes are proposals anyway.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Approval {
    #[default]
    Always,
    Write,
    Never,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ParamKind {
    /// Free text on one line, such as a pattern.
    Text,
    Int,
    /// `15m`, `1h`, `2d`: given to the clients in seconds, minutes or hours (`48h`).
    Duration,
    /// One of `values`.
    Choice,
    /// One of `values`, a whole command line of the recipe's own: its words become arguments.
    Command,
    /// An absolute path.
    Path,
    /// A unit, a pod, a container: letters, digits and `@ . _ : / -`.
    Name,
    /// A query of the SQL console, which comes in a later TermF: refused until then.
    Sql,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Param {
    #[serde(default, skip_deserializing)]
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ParamKind,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(
        default,
        deserialize_with = "scalar",
        skip_serializing_if = "Option::is_none"
    )]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_len: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    /// Lines of output kept.
    #[serde(default = "lines")]
    pub lines: usize,
    /// Bytes of output kept; the client is stopped a little past it.
    #[serde(default = "bytes")]
    pub bytes: usize,
    /// The client is killed after it.
    #[serde(default = "timeout")]
    pub timeout_s: u64,
    /// Rows of a query, for the SQL console to come.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<usize>,
}

fn lines() -> usize {
    500
}
fn bytes() -> usize {
    64 << 10
}
fn timeout() -> u64 {
    30
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            lines: lines(),
            bytes: bytes(),
            timeout_s: timeout(),
            rows: None,
        }
    }
}

const MAX_LINES: usize = 10_000;
const MAX_BYTES: usize = 1 << 20;
const MAX_TIMEOUT: u64 = 600;

/// One argument, or a group of them left out together when one of its parameters has no value.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Arg {
    One(String),
    Group(Vec<String>),
}

/// How a recipe runs on one kind of connection.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub kind: Kind,
    /// Parameters that must have a value, and that must not, for this variant to apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unless: Vec<String>,
    /// The client's arguments, after TermF's own (connection, options); on SSH, the remote command.
    pub args: Vec<Arg>,
    /// SSH: more remote commands, each fed the one before; one whose arguments all drop out is skipped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pipe: Vec<Vec<Arg>>,
    /// Only the output lines that hold this text (TermF filters them), when it has a value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// One connection, by name; without it, any connection of a kind its variants handle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, deserialize_with = "ordered")]
    pub params: Vec<Param>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default = "redacted")]
    pub ai_access: AiAccess,
    #[serde(default)]
    pub approval: Approval,
    #[serde(default)]
    pub redact: Vec<Redact>,
    pub run: Vec<Variant>,
    /// `shipped` or `project`, set when read.
    #[serde(default, skip_deserializing)]
    pub source: String,
}

fn redacted() -> AiAccess {
    AiAccess::Redacted
}

/// `params` as the file orders them, which is the form's order.
fn ordered<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Param>, D::Error> {
    struct Params;
    impl<'de> Visitor<'de> for Params {
        type Value = Vec<Param>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a table of parameters")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Vec<Param>, A::Error> {
            let mut out = Vec::new();
            while let Some((name, param)) = map.next_entry::<String, Param>()? {
                out.push(Param { name, ..param });
            }
            Ok(out)
        }
    }
    d.deserialize_map(Params)
}

/// A default written as a number or a word.
fn scalar<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Scalar {
        Text(String),
        Int(i64),
        Bool(bool),
    }
    Ok(Option::<Scalar>::deserialize(d)?.map(|s| match s {
        Scalar::Text(t) => t,
        Scalar::Int(i) => i.to_string(),
        Scalar::Bool(b) => b.to_string(),
    }))
}

/// The connection's fields a template may name as `{conn.<field>}`.
fn conn_field(conn: &Connection, field: &str) -> Option<String> {
    match field {
        "name" => Some(conn.name.clone()),
        "env" => Some(conn.env.as_str().into()),
        "host" => conn.host.clone(),
        "port" => conn.port.map(|p| p.to_string()),
        "user" => conn.user.clone(),
        "database" => conn.database.clone(),
        "context" => conn.context.clone(),
        "namespace" => conn.namespace.clone(),
        "log_group" => conn.log_group.clone(),
        "region" => conn.region.clone(),
        "profile" => conn.profile.clone(),
        "gcp_project" => conn.gcp_project.clone(),
        "url" => conn.url.clone(),
        "site" => Some(conn.site.clone().unwrap_or_else(|| "datadoghq.com".into())),
        _ => None,
    }
}

const CONN_FIELDS: &[&str] = &[
    "name",
    "env",
    "host",
    "port",
    "user",
    "database",
    "context",
    "namespace",
    "log_group",
    "region",
    "profile",
    "gcp_project",
    "url",
    "site",
];

enum Piece<'a> {
    Text(&'a str),
    Hole(&'a str),
}

/// `{name}` and `{conn.field}` holes; `{{` and `}}` are braces.
fn pieces(template: &str) -> Result<Vec<Piece<'_>>> {
    let mut out = Vec::new();
    let mut rest = template;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("{{") {
            out.push(Piece::Text("{"));
            rest = r;
        } else if let Some(r) = rest.strip_prefix("}}") {
            out.push(Piece::Text("}"));
            rest = r;
        } else if let Some(r) = rest.strip_prefix('{') {
            let end = r
                .find('}')
                .with_context(|| format!("an unclosed {{ in {template:?}"))?;
            out.push(Piece::Hole(&r[..end]));
            rest = &r[end + 1..];
        } else {
            ensure!(!rest.starts_with('}'), "a lone }} in {template:?}");
            let end = rest.find(['{', '}']).unwrap_or(rest.len());
            out.push(Piece::Text(&rest[..end]));
            rest = &rest[end..];
        }
    }
    Ok(out)
}

impl Recipe {
    pub fn parse(text: &str, source: &str) -> Result<Self> {
        let mut recipe: Recipe = toml::from_str(text)?;
        recipe.source = source.into();
        // The form shows the parameters in the file's order, which the TOML reader does not keep.
        let doc: toml_edit::DocumentMut = text.parse()?;
        let order: Vec<&str> = doc
            .get("params")
            .and_then(|p| p.as_table_like())
            .map(|t| t.iter().map(|(k, _)| k).collect())
            .unwrap_or_default();
        recipe
            .params
            .sort_by_key(|p| order.iter().position(|k| *k == p.name));
        recipe.validate()?;
        Ok(recipe)
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            valid_name(&self.name),
            "invalid recipe name {:?}",
            self.name
        );
        ensure!(!self.run.is_empty(), "a recipe needs at least one [[run]]");
        let l = &self.limits;
        ensure!(
            (1..=MAX_LINES).contains(&l.lines)
                && (1..=MAX_BYTES).contains(&l.bytes)
                && (1..=MAX_TIMEOUT).contains(&l.timeout_s),
            "limits: lines 1 to {MAX_LINES}, bytes 1 to {MAX_BYTES}, timeout_s 1 to {MAX_TIMEOUT}"
        );
        for p in &self.params {
            ensure!(valid_name(&p.name), "invalid parameter name {:?}", p.name);
            if matches!(p.kind, ParamKind::Choice | ParamKind::Command) {
                ensure!(!p.values.is_empty(), "{} lists its values", p.name);
            }
            if let Some(default) = &p.default {
                check(p, default).with_context(|| format!("the default of {}", p.name))?;
            }
        }
        let known = |hole: &str| match hole.strip_prefix("conn.") {
            Some(field) => CONN_FIELDS.contains(&field),
            None => self.param(hole).is_some(),
        };
        for v in &self.run {
            ensure!(!v.args.is_empty(), "a [[run]] needs args");
            ensure!(
                v.pipe.is_empty() || v.kind == Kind::Ssh,
                "only an SSH [[run]] pipes commands"
            );
            for name in v.when.iter().chain(&v.unless) {
                ensure!(
                    self.param(name).is_some(),
                    "when / unless name a parameter: {name}"
                );
            }
            let args = v.args.iter().chain(v.pipe.iter().flatten());
            let templates = args
                .flat_map(|a| match a {
                    Arg::One(s) => vec![s],
                    Arg::Group(g) => g.iter().collect(),
                })
                .chain(&v.keep);
            for template in templates {
                for piece in pieces(template)? {
                    if let Piece::Hole(hole) = piece {
                        ensure!(
                            known(hole),
                            "{{{hole}}} is neither a parameter nor a connection field"
                        );
                    }
                }
            }
            // Datadog's keys go to one host only: TermF writes it, the recipe the API path.
            if v.kind == Kind::Datadog {
                let path =
                    matches!(&v.args[0], Arg::One(p) if p.starts_with("/api/") && !p.contains('{'));
                ensure!(
                    path,
                    "a Datadog [[run]] starts with the API path, such as /api/v2/logs/events"
                );
            }
        }
        for p in self.params.iter().filter(|p| p.kind == ParamKind::Command) {
            for value in &p.values {
                for word in value.split_whitespace() {
                    for piece in pieces(word)? {
                        if let Piece::Hole(hole) = piece {
                            ensure!(
                                self.param(hole)
                                    .is_some_and(|h| h.kind != ParamKind::Command),
                                "{value:?} names {hole}, which is not a parameter"
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// A parameter's value as the client gets it, or why it is refused.
pub fn check(p: &Param, raw: &str) -> Result<String> {
    let value = raw.trim();
    ensure!(!value.is_empty(), "{} is empty", p.name);
    ensure!(
        !value.chars().any(char::is_control),
        "{} holds a control character (a new line, a tab…)",
        p.name
    );
    ensure!(
        !value.starts_with('-'),
        "{} cannot start with -: it would become an option",
        p.name
    );
    let chars = |extra: &str| {
        value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
    };
    Ok(match p.kind {
        ParamKind::Text => {
            let max = p.max_len.unwrap_or(200);
            ensure!(
                value.chars().count() <= max,
                "{} has {max} characters at most",
                p.name
            );
            value.to_owned()
        }
        ParamKind::Int => {
            let n: i64 = value
                .parse()
                .with_context(|| format!("{} is a whole number", p.name))?;
            let (min, max) = (p.min.unwrap_or(0), p.max.unwrap_or(i64::MAX));
            ensure!(
                (min..=max).contains(&n),
                "{} goes from {min} to {max}",
                p.name
            );
            n.to_string()
        }
        ParamKind::Duration => {
            let unit = value.chars().last().unwrap_or_default();
            let n: u64 = value[..value.len() - unit.len_utf8()]
                .parse()
                .ok()
                .filter(|n| (1..=99_999).contains(n))
                .with_context(|| format!("{} is a duration such as 15m, 1h or 2d", p.name))?;
            let (n, unit) = match unit {
                's' | 'm' | 'h' => (n, unit),
                'd' => (n * 24, 'h'),
                _ => bail!("{} is a duration such as 15m, 1h or 2d", p.name),
            };
            let hours = match unit {
                's' => n / 3600,
                'm' => n / 60,
                _ => n,
            };
            ensure!(hours <= 90 * 24, "{} goes back 90 days at most", p.name);
            format!("{n}{unit}")
        }
        ParamKind::Choice | ParamKind::Command => {
            ensure!(
                p.values.iter().any(|v| v == value),
                "{} is one of: {}",
                p.name,
                p.values.join(", ")
            );
            value.to_owned()
        }
        ParamKind::Path => {
            ensure!(
                value.starts_with('/')
                    && value.len() <= 256
                    && chars("._/@+-")
                    && !value.split('/').any(|s| s == ".."),
                "{} is an absolute path (letters, digits, . _ / @ + -), without ..",
                p.name
            );
            value.to_owned()
        }
        ParamKind::Name => {
            ensure!(
                value.len() <= 128 && chars("@._:/-"),
                "{} holds letters, digits and @ . _ : / - (128 at most)",
                p.name
            );
            value.to_owned()
        }
        ParamKind::Sql => bail!("SQL queries come with the SQL console, in a later TermF"),
    })
}

/// What will run, checked: what the approval card and the history show, and what [`crate::Runner`] runs.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub recipe: String,
    pub connection: String,
    pub kind: Kind,
    pub env: Env,
    pub mode: Mode,
    pub target: String,
    /// The command as a shell would take it, the vault's secrets by their name.
    pub command: String,
    /// SSH: the command the server's shell runs, as it reads it.
    pub remote: Option<String>,
    /// The SSH tunnel TermF opens first, for a database behind a server.
    pub tunnel: Option<String>,
    pub params: BTreeMap<String, String>,
    /// The vault's secrets it uses, by name.
    pub secrets: Vec<String>,
    pub limits: Limits,
    pub approval: Approval,
    pub redact: Vec<Redact>,
    /// What an agent sees of it.
    pub ai_access: AiAccess,
    #[serde(skip)]
    pub(crate) base: Base,
    #[serde(skip)]
    pub(crate) args: Vec<String>,
    #[serde(skip)]
    pub(crate) keep: Option<String>,
}

struct Values<'a> {
    given: &'a BTreeMap<String, String>,
    conn: &'a Connection,
}

impl Values<'_> {
    fn get(&self, hole: &str) -> Option<String> {
        match hole.strip_prefix("conn.") {
            Some(field) => conn_field(self.conn, field),
            None => self.given.get(hole).cloned(),
        }
    }

    /// The template filled, or `None` when one of its holes has no value.
    fn fill(&self, template: &str) -> Result<Option<String>> {
        let mut out = String::new();
        for piece in pieces(template)? {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Hole(h) => match self.get(h) {
                    Some(v) => out.push_str(&v),
                    None => return Ok(None),
                },
            }
        }
        Ok(Some(out))
    }

    fn args(&self, recipe: &Recipe, args: &[Arg]) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for arg in args {
            match arg {
                Arg::One(t) => {
                    let command = t
                        .strip_prefix('{')
                        .and_then(|t| t.strip_suffix('}'))
                        .and_then(|name| recipe.param(name))
                        .filter(|p| p.kind == ParamKind::Command);
                    if let Some(p) = command {
                        let Some(line) = self.given.get(&p.name) else {
                            bail!("{} is needed", p.name)
                        };
                        for word in line.split_whitespace() {
                            let filled = self.fill(word)?;
                            out.push(filled.with_context(|| {
                                let hole = word.trim_matches(['{', '}']);
                                format!("{line} needs {hole}")
                            })?);
                        }
                        continue;
                    }
                    out.push(self.fill(t)?.with_context(|| format!("{t} has no value"))?);
                }
                Arg::Group(group) => {
                    let filled: Option<Vec<String>> =
                        group.iter().map(|t| self.fill(t)).collect::<Result<_>>()?;
                    out.extend(filled.unwrap_or_default());
                }
            }
        }
        Ok(out)
    }
}

/// POSIX shell quoting: as it is when it holds only safe characters, else in single quotes.
pub fn quote(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./=:@%+,".contains(c));
    if safe {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// Checks the parameters given (names → values as typed) and makes the command `recipe` runs on `conn`.
pub fn plan(
    recipe: &Recipe,
    conn: &Connection,
    all: &[Connection],
    given: &BTreeMap<String, String>,
) -> Result<Plan> {
    ensure!(
        crate::runs_on(recipe, conn),
        "{} does not run on {} (a {:?} connection)",
        recipe.name,
        conn.name,
        conn.kind
    );
    ensure!(
        !(recipe.approval == Approval::Never && conn.env == Env::Prod),
        "{}: approval = \"never\" is not allowed in prod",
        recipe.name
    );
    conn.check()?;
    for name in given.keys() {
        ensure!(
            recipe.param(name).is_some(),
            "{} has no parameter {name}",
            recipe.name
        );
    }
    let mut values = BTreeMap::new();
    for p in &recipe.params {
        let raw = given.get(&p.name).filter(|v| !v.trim().is_empty());
        match raw.or(p.default.as_ref()) {
            Some(raw) => {
                values.insert(p.name.clone(), check(p, raw)?);
            }
            None => ensure!(!p.required, "{} is needed", p.name),
        }
    }
    let set = |name: &String| values.contains_key(name);
    let variant = recipe
        .run
        .iter()
        .find(|v| v.kind == conn.kind && v.when.iter().all(set) && !v.unless.iter().any(set))
        .with_context(|| {
            format!(
                "{} has nothing to run on {} with these parameters",
                recipe.name, conn.name
            )
        })?;
    let filled = Values {
        given: &values,
        conn,
    };
    let mut args = filled.args(recipe, &variant.args)?;
    let base = exec::base(conn, all, recipe.mode, recipe.limits.timeout_s)?;
    let mut remote = None;
    if conn.kind == Kind::Ssh {
        let mut steps = vec![args];
        for step in &variant.pipe {
            let step = filled.args(recipe, step)?;
            if !step.is_empty() {
                steps.push(step);
            }
        }
        let commands: Vec<String> = steps
            .iter()
            .map(|s| s.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" "))
            .collect();
        let line = commands.join(" | ");
        args = vec![line.clone()];
        remote = Some(line);
    }
    if conn.kind == Kind::Datadog {
        let site = conn.site.as_deref().unwrap_or("datadoghq.com");
        args[0] = format!("https://api.{site}{}", args[0]);
    }
    let keep = match &variant.keep {
        Some(t) => filled.fill(t)?.filter(|k| !k.is_empty()),
        None => None,
    };
    let shown: Vec<String> = base
        .env
        .iter()
        .map(|(var, secret)| format!("{var}=<vault: {secret}>"))
        .chain([quote(&base.program)])
        .chain(base.args.iter().chain(&args).map(|a| quote(a)))
        .collect();
    let mut command = shown.join(" ");
    if !base.stdin.is_empty() {
        let names: Vec<&str> = base.stdin.iter().map(|(_, s)| s.as_str()).collect();
        command.push_str(&format!(" <<< <vault: {}>", names.join(", ")));
    }
    let secrets = base
        .env
        .iter()
        .chain(&base.stdin)
        .map(|(_, s)| s.clone())
        .collect();
    Ok(Plan {
        recipe: recipe.name.clone(),
        connection: conn.name.clone(),
        kind: conn.kind,
        env: conn.env,
        mode: recipe.mode,
        target: conn.target(all),
        command,
        remote,
        tunnel: base.tunnel.as_ref().map(|t| t.shown()),
        params: values,
        secrets,
        limits: recipe.limits,
        approval: recipe.approval,
        redact: recipe.redact.clone(),
        ai_access: crate::ai_access(recipe, conn),
        base,
        args,
        keep,
    })
}
