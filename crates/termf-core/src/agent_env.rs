//! What an agent's process gets of TermF's environment (ARCHITECTURE §12.2): a short allowlist, what its own CLI
//! needs, the variables the user allows (`[agents] env_allow`) and the vault's values given to the project's agents
//! (`[agents.secrets]`). A shell the user opens in a tile keeps everything.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail, ensure};
use toml_edit::{Array, DocumentMut, Item, value};

use crate::claude::Tool;

/// What `termf agent-env` keeps beyond the allowlist: `<tool>;<pattern>,<pattern>…`.
pub const SPEC: &str = "TERMF_AGENT_ENV";

/// Every agent gets these, when TermF has them. Windows' own are there for cmd.exe and Node to start.
pub const BASE: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "LC_*",
    "TERM",
    "COLORTERM",
    "TMPDIR",
    "TZ",
    "XDG_*",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "USERPROFILE",
    "USERNAME",
    "USERDOMAIN",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "ALLUSERSPROFILE",
    "PROGRAMFILES*",
    "PROGRAMW6432",
    "COMMONPROGRAMFILES*",
    "COMMONPROGRAMW6432",
    "PUBLIC",
    "TEMP",
    "TMP",
    "OS",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
    "COMPUTERNAME",
];

/// What each agent CLI reads to sign in and to set itself up (code.claude.com/docs/en/env-vars read 2026-11-22,
/// docs/AGENTS-CLI.md).
fn own(tool: Tool) -> &'static [&'static str] {
    match tool {
        Tool::Claude => &[
            "ANTHROPIC_*",
            "CLAUDE_*",
            "DISABLE_TELEMETRY",
            "DISABLE_ERROR_REPORTING",
            "DISABLE_AUTOUPDATER",
            "DO_NOT_TRACK",
            "MAX_THINKING_TOKENS",
            "MCP_TIMEOUT",
            "MCP_TOOL_TIMEOUT",
            "API_TIMEOUT_MS",
            "BASH_DEFAULT_TIMEOUT_MS",
            "BASH_MAX_TIMEOUT_MS",
            "BASH_MAX_OUTPUT_LENGTH",
        ],
        Tool::Codex => &["CODEX_*", "OPENAI_*"],
        Tool::Gemini => &[
            "GEMINI_*",
            "GOOGLE_API_KEY",
            "GOOGLE_GENAI_*",
            "GOOGLE_CLOUD_PROJECT",
            "GOOGLE_CLOUD_LOCATION",
            "GOOGLE_APPLICATION_CREDENTIALS",
        ],
    }
}

/// Claude Code on Amazon Bedrock or Google Vertex AI signs in with the cloud's own variables: given only when the
/// switch is on.
const CLOUDS: &[(&str, &[&str])] = &[
    ("CLAUDE_CODE_USE_BEDROCK", &["AWS_*"]),
    (
        "CLAUDE_CODE_USE_VERTEX",
        &[
            "GOOGLE_*",
            "CLOUD_ML_REGION",
            "VERTEX_REGION_*",
            "CLOUDSDK_*",
        ],
    ),
];

/// Never a key an agent asks for nor one given from the vault: TermF's and the agents' own, and what changes how
/// programs start, load code or reach a server.
const DENIED: &[&str] = &[
    "TERMF_*",
    "LD_*",
    "DYLD_*",
    "NODE_OPTIONS",
    "NODE_PATH",
    "NODE_TLS_REJECT_UNAUTHORIZED",
    "NPM_CONFIG_*",
    "PYTHONPATH",
    "PYTHONHOME",
    "PYTHONSTARTUP",
    "PERL5LIB",
    "PERL5OPT",
    "RUBYOPT",
    "RUBYLIB",
    "BASH_ENV",
    "ENV",
    "ZDOTDIR",
    "PROMPT_COMMAND",
    "IFS",
    "PS4",
    "SHELLOPTS",
    "BASHOPTS",
    "GIT_*",
    "SSH_*",
    "GNUPGHOME",
    "GPG_*",
    "JAVA_TOOL_OPTIONS",
    "_JAVA_OPTIONS",
    "JDK_JAVA_OPTIONS",
    "EDITOR",
    "VISUAL",
    "PAGER",
    "BROWSER",
    "DOCKER_HOST",
    "KUBECONFIG",
    "SSL_*",
    "CURL_CA_BUNDLE",
    "REQUESTS_CA_BUNDLE",
    "GCONV_PATH",
];

/// `[agents]` in `termf.toml` and in a project's file, for its agents.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentEnv {
    /// `[agents] env = "inherit"` in `termf.toml`: every agent gets all of TermF's environment.
    pub inherit: bool,
    /// `[agents] env_allow`: names, or prefixes ending with `*`; the global ones, then the project's.
    pub allow: Vec<String>,
    /// `[agents.secrets]`: the project's over the global ones.
    pub secrets: Vec<Grant>,
    /// What the files hold that TermF leaves out, and why.
    pub problems: Vec<String>,
}

/// A variable the project's agents get from the vault.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub variable: String,
    /// The vault's secret, by its name, in `scope`: the project's first, else the global one.
    pub secret: String,
    pub scope: String,
    /// Set in the project's file rather than in `termf.toml`.
    pub project: bool,
}

static CONFIG: OnceLock<(PathBuf, Option<PathBuf>)> = OnceLock::new();

/// Where the settings are, and the `termf` CLI that filters what a login shell adds: set once by the app and the CLI.
/// Without it, agents get the allowlist alone.
pub fn configure(config_dir: PathBuf, cli: Option<PathBuf>) {
    let _ = CONFIG.set((config_dir, cli));
}

/// The settings for `project` from the configured folder.
pub fn current(project: Option<&str>) -> AgentEnv {
    match CONFIG.get() {
        Some((config_dir, _)) => load(config_dir, project),
        None => AgentEnv::default(),
    }
}

/// The configured `termf` CLI.
pub fn cli() -> Option<&'static Path> {
    CONFIG.get().and_then(|(_, cli)| cli.as_deref())
}

/// The global settings, then the project's. What does not read is left out and said in `problems`: an agent then
/// gets less, never more.
pub fn load(config_dir: &Path, project: Option<&str>) -> AgentEnv {
    let mut env = AgentEnv::default();
    if let Err(e) = read(&mut env, &config_dir.join("termf.toml"), false) {
        env.problems.push(format!("termf.toml: {e:#}"));
    }
    if let Some(project) = project
        && let Ok(file) = crate::project_file(config_dir, project)
        && let Err(e) = read(&mut env, &file, true)
    {
        env.problems.push(format!("{project}.toml: {e:#}"));
    }
    env
}

fn read(env: &mut AgentEnv, file: &Path, from_project: bool) -> Result<()> {
    let Ok(text) = fs::read_to_string(file) else {
        return Ok(());
    };
    let doc: toml::Table = toml::from_str(&text)?;
    let Some(agents) = doc.get("agents").and_then(toml::Value::as_table) else {
        return Ok(());
    };
    if !from_project {
        match agents.get("env").map(|v| v.as_str()) {
            None | Some(Some("allowlist")) => {}
            Some(Some("inherit")) => env.inherit = true,
            Some(_) => env
                .problems
                .push("[agents] env: \"inherit\" or \"allowlist\"".into()),
        }
    }
    let place = if from_project { "the project's " } else { "" };
    for item in agents
        .get("env_allow")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        match item.as_str().map(|p| check_pattern(p).map(|()| p)) {
            Some(Ok(p)) => env.allow.push(p.to_owned()),
            Some(Err(e)) => env
                .problems
                .push(format!("{place}[agents] env_allow: {e:#}")),
            None => env
                .problems
                .push(format!("{place}[agents] env_allow: names only")),
        }
    }
    for (variable, secret) in agents
        .get("secrets")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
    {
        let grant = secret
            .as_str()
            .context("the name of a vault secret")
            .and_then(|s| grant(variable, s, from_project));
        match grant {
            Ok(grant) => {
                env.secrets.retain(|g| g.variable != grant.variable);
                env.secrets.push(grant);
            }
            Err(e) => env
                .problems
                .push(format!("{place}[agents.secrets] {variable}: {e:#}")),
        }
    }
    Ok(())
}

fn grant(variable: &str, secret: &str, project: bool) -> Result<Grant> {
    check_variable(variable)?;
    let (scope, name) = match secret.split_once('/') {
        Some((scope, name)) => (scope, name),
        None => ("dev", secret),
    };
    match scope {
        "dev" | "staging" => {}
        "prod" => bail!("agents never get a prod secret: give a dev or staging one"),
        other => bail!("unknown scope {other} (dev, staging)"),
    }
    ensure!(
        !name.is_empty()
            && name.len() <= 128
            && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)),
        "{name:?} is not a vault secret's name"
    );
    Ok(Grant {
        variable: variable.into(),
        secret: name.into(),
        scope: scope.into(),
        project,
    })
}

fn matches(pattern: &str, name: &str) -> bool {
    let same = |a: &str, b: &str| match cfg!(windows) {
        true => a.eq_ignore_ascii_case(b),
        false => a == b,
    };
    match pattern.strip_suffix('*') {
        Some(prefix) => name
            .get(..prefix.len())
            .is_some_and(|start| same(start, prefix)),
        None => same(pattern, name),
    }
}

/// An `env_allow` entry: a name, or a prefix ending with `*`.
pub fn check_pattern(pattern: &str) -> Result<()> {
    let name = pattern.strip_suffix('*').unwrap_or(pattern);
    ensure!(
        name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.len() <= 128
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "{pattern:?}: a variable's name, or a prefix ending with *"
    );
    ensure!(
        !matches("TERMF_*", name),
        "{pattern:?}: TermF sets its own variables"
    );
    Ok(())
}

/// A variable that may hold a key from the vault: capitals, digits and `_`, none of TermF's or the agents' own, nor
/// one that changes how programs run.
pub fn check_variable(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 128
            && name.starts_with(|c: char| c.is_ascii_uppercase() || c == '_')
            && name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
        "{name:?} is not a variable name: capital letters, digits and _, not starting with a digit"
    );
    let owned = BASE
        .iter()
        .chain(Tool::ALL.iter().flat_map(|t| own(*t)))
        .chain(CLOUDS.iter().flat_map(|(_, p)| p.iter()))
        .chain(DENIED);
    ensure!(
        !owned.into_iter().any(|p| matches(p, name)),
        "{name} belongs to TermF, to the agents themselves or to how programs run: it never holds a key from the vault"
    );
    Ok(())
}

impl AgentEnv {
    fn keeps(&self, tool: Option<Tool>, name: &str, all: &[(OsString, OsString)]) -> bool {
        let set = |switch: &str| {
            all.iter()
                .find(|(k, _)| matches(switch, &k.to_string_lossy()))
                .is_some_and(|(_, v)| on(v))
        };
        let listed = BASE
            .iter()
            .chain(tool.map_or(&[][..], own))
            .copied()
            .chain(self.allow.iter().map(String::as_str));
        listed.into_iter().any(|p| matches(p, name))
            || (tool == Some(Tool::Claude)
                && CLOUDS
                    .iter()
                    .any(|(switch, p)| set(switch) && p.iter().any(|p| matches(p, name))))
    }

    /// The variables of `all` an agent running `tool` gets; all of them with `inherit`.
    pub fn keep(
        &self,
        tool: Option<Tool>,
        all: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Vec<(OsString, OsString)> {
        let all: Vec<_> = all.into_iter().collect();
        if self.inherit {
            return all;
        }
        all.iter()
            .filter(|(k, _)| self.keeps(tool, &k.to_string_lossy(), &all))
            .cloned()
            .collect()
    }

    /// `cmd` starts with what [`AgentEnv::keep`] keeps of TermF's environment, plus what was set on it.
    pub fn apply(&self, cmd: &mut Command, tool: Option<Tool>) {
        if self.inherit {
            return;
        }
        let set: Vec<(OsString, Option<OsString>)> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(OsStr::to_owned)))
            .collect();
        cmd.env_clear();
        cmd.envs(self.keep(tool, std::env::vars_os()));
        for (key, value) in set {
            match value {
                Some(value) => cmd.env(key, value),
                None => cmd.env_remove(key),
            };
        }
    }

    /// A command a login shell runs (`$SHELL -l -i -c`), whose profile may export anything again: run through
    /// `<cli> agent-env`, which keeps the same allowlist, TermF's own variables and `set` (the names TermF itself
    /// gives the agent), then runs the command with `sh -c`. The command, and the value of [`SPEC`] to give its
    /// process; `None` with `inherit`, without the CLI and on Windows, whose cmd.exe reads no profile.
    pub fn wrap(
        &self,
        command: &str,
        tool: Option<Tool>,
        cli: Option<&Path>,
        set: &[String],
        quote: impl Fn(&str) -> String,
    ) -> Option<(String, String)> {
        let cli = cli.filter(|_| !self.inherit && cfg!(unix))?;
        let patterns: Vec<&str> = self
            .allow
            .iter()
            .chain(set)
            .map(String::as_str)
            .chain(["TERMF_*"])
            .collect();
        let tool = tool.map_or("", Tool::program);
        Some((
            format!(
                "{} agent-env {}",
                quote(&cli.to_string_lossy()),
                quote(command)
            ),
            format!("{tool};{}", patterns.join(",")),
        ))
    }
}

fn on(value: &OsStr) -> bool {
    let value = value.to_string_lossy();
    !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
}

/// `termf agent-env <command>`: runs `command` with `sh -c`, with only what [`SPEC`] keeps of this environment.
pub fn exec(command: &str) -> Result<()> {
    let spec = std::env::var(SPEC).context("TermF did not start this command")?;
    let (tool, patterns) = spec.split_once(';').unwrap_or(("", &spec));
    let env = AgentEnv {
        allow: patterns
            .split(',')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect(),
        ..AgentEnv::default()
    };
    let tool = Tool::ALL.into_iter().find(|t| t.program() == tool);
    let kept = env.keep(tool, std::env::vars_os().filter(|(k, _)| k != SPEC));
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(command).env_clear().envs(kept);
    #[cfg(unix)]
    return Err(std::os::unix::process::CommandExt::exec(&mut cmd))
        .context("cannot start the agent");
    #[cfg(not(unix))]
    bail!("termf agent-env only runs under Unix: cmd.exe reads no profile");
}

/// `[agents] env_allow` in `termf.toml`, the rest of the file kept.
pub fn save_allow(config_dir: &Path, allow: &[String]) -> Result<()> {
    for pattern in allow {
        check_pattern(pattern)?;
    }
    crate::edit_settings(config_dir, |doc| {
        let agents = crate::table_in(doc.as_table_mut(), "agents");
        match allow.is_empty() {
            true => agents.remove("env_allow"),
            false => agents.insert("env_allow", value(Array::from_iter(allow))),
        };
    })
}

/// Gives the project's agents `variable` from the vault's `secret` (`<scope>/<name>`), in its file.
pub fn give(config_dir: &Path, project: &str, variable: &str, secret: &str) -> Result<()> {
    grant(variable, secret, true)?;
    let file = crate::project_file(config_dir, project)?;
    edit(&file, |doc| {
        let agents = crate::table_in(doc.as_table_mut(), "agents");
        // No empty `[agents]` header above `[agents.secrets]`.
        if agents.iter().all(|(_, item)| item.is_table()) {
            agents.set_implicit(true);
        }
        crate::table_in(agents, "secrets").insert(variable, value(secret));
    })
}

/// No longer gives `variable`: in the project's file, or in `termf.toml` without a project.
pub fn take_back(config_dir: &Path, project: Option<&str>, variable: &str) -> Result<()> {
    let file = match project {
        Some(project) => crate::project_file(config_dir, project)?,
        None => config_dir.join("termf.toml"),
    };
    edit(&file, |doc| {
        let Some(agents) = doc.get_mut("agents").and_then(Item::as_table_like_mut) else {
            return;
        };
        if let Some(secrets) = agents.get_mut("secrets").and_then(Item::as_table_like_mut) {
            secrets.remove(variable);
            if secrets.is_empty() {
                agents.remove("secrets");
            }
        }
        if agents.is_empty() && project.is_some() {
            doc.remove("agents");
        }
    })
}

fn edit(file: &Path, change: impl FnOnce(&mut DocumentMut)) -> Result<()> {
    let mut doc: DocumentMut = fs::read_to_string(file)
        .with_context(|| format!("cannot read {}", file.display()))?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    change(&mut doc);
    crate::write_atomic(file, &doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn vars(list: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        list.iter().map(|(k, v)| (k.into(), v.into())).collect()
    }

    fn names(kept: &[(OsString, OsString)]) -> Vec<String> {
        kept.iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect()
    }

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("termf-agent-env-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("projects")).unwrap();
        dir
    }

    #[test]
    fn an_agent_gets_the_allowlist_what_its_cli_needs_and_what_the_user_allows() {
        let all = vars(&[
            ("PATH", "/usr/bin"),
            ("HOME", "/home/alex"),
            ("LC_ALL", "fr_FR.UTF-8"),
            ("STRIPE_SECRET_KEY", "sk_live_x"),
            ("AWS_SECRET_ACCESS_KEY", "aws"),
            ("ANTHROPIC_API_KEY", "sk-ant"),
            ("OPENAI_API_KEY", "sk-oa"),
            ("CORP_TOKEN", "c"),
            ("ACME_ONE", "1"),
            ("ACME_TWO", "2"),
            ("TERMF_TOKEN", "app's own"),
        ]);
        let plain = AgentEnv::default();
        assert_eq!(
            names(&plain.keep(Some(Tool::Claude), all.clone())),
            ["PATH", "HOME", "LC_ALL", "ANTHROPIC_API_KEY"]
        );
        assert_eq!(
            names(&plain.keep(Some(Tool::Codex), all.clone())),
            ["PATH", "HOME", "LC_ALL", "OPENAI_API_KEY"]
        );
        assert_eq!(
            names(&plain.keep(None, all.clone())),
            ["PATH", "HOME", "LC_ALL"]
        );
        let allowed = AgentEnv {
            allow: vec!["CORP_TOKEN".into(), "ACME_*".into()],
            ..AgentEnv::default()
        };
        assert_eq!(
            names(&allowed.keep(None, all.clone())),
            [
                "PATH",
                "HOME",
                "LC_ALL",
                "CORP_TOKEN",
                "ACME_ONE",
                "ACME_TWO"
            ]
        );
        let inherit = AgentEnv {
            inherit: true,
            ..AgentEnv::default()
        };
        assert_eq!(inherit.keep(None, all.clone()), all);
    }

    #[test]
    fn the_cloud_claude_code_signs_in_with_comes_only_when_it_is_on() {
        let mut all = vars(&[
            ("AWS_PROFILE", "dev"),
            ("GOOGLE_APPLICATION_CREDENTIALS", "/k.json"),
        ]);
        let env = AgentEnv::default();
        assert!(env.keep(Some(Tool::Claude), all.clone()).is_empty());
        all.push(("CLAUDE_CODE_USE_BEDROCK".into(), "0".into()));
        assert_eq!(
            names(&env.keep(Some(Tool::Claude), all.clone())),
            ["CLAUDE_CODE_USE_BEDROCK"]
        );
        all.pop();
        all.push(("CLAUDE_CODE_USE_BEDROCK".into(), "1".into()));
        assert_eq!(
            names(&env.keep(Some(Tool::Claude), all.clone())),
            ["AWS_PROFILE", "CLAUDE_CODE_USE_BEDROCK"]
        );
        // Codex is not Claude Code: Bedrock's switch gives it nothing.
        assert!(names(&env.keep(Some(Tool::Codex), all)).is_empty());
    }

    #[test]
    fn a_command_keeps_what_termf_sets_on_it() {
        let mut cmd = Command::new("true");
        cmd.env("TERMF_MCP_TOKEN", "t")
            .env("PEXELS_API_KEY", "p")
            .env_remove("HOME");
        AgentEnv::default().apply(&mut cmd, Some(Tool::Claude));
        let set: BTreeMap<_, _> = cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert_eq!(set["TERMF_MCP_TOKEN"].as_deref(), Some("t"));
        assert_eq!(set["PEXELS_API_KEY"].as_deref(), Some("p"));
        assert!(!set.contains_key("HOME"), "removed on the command, so not filled in again");
        assert!(set.contains_key("PATH"));
    }

    #[test]
    fn the_wrapper_gets_the_same_list_and_termf_s_names() {
        let env = AgentEnv {
            allow: vec!["ACME_*".into()],
            ..AgentEnv::default()
        };
        let quote = |s: &str| format!("'{s}'");
        let wrapped = env.wrap(
            "claude --x",
            Some(Tool::Claude),
            Some(Path::new("/opt/termf")),
            &["PEXELS_API_KEY".into()],
            quote,
        );
        if cfg!(unix) {
            assert_eq!(
                wrapped,
                Some((
                    "'/opt/termf' agent-env 'claude --x'".into(),
                    "claude;ACME_*,PEXELS_API_KEY,TERMF_*".into()
                ))
            );
        } else {
            assert_eq!(wrapped, None);
        }
        assert_eq!(env.wrap("claude", None, None, &[], quote), None);
        let inherit = AgentEnv {
            inherit: true,
            ..AgentEnv::default()
        };
        assert_eq!(
            inherit.wrap("claude", None, Some(Path::new("/opt/termf")), &[], quote),
            None
        );
    }

    #[test]
    fn names_an_agent_may_never_get_a_key_in() {
        for ok in [
            "PEXELS_API_KEY",
            "PISTE_CLIENT_ID",
            "_X",
            "UNSPLASH_ACCESS_KEY",
        ] {
            check_variable(ok).unwrap();
        }
        for bad in [
            "PATH",
            "HOME",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "NODE_OPTIONS",
            "TERMF_TOKEN",
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CONFIG_DIR",
            "OPENAI_BASE_URL",
            "AWS_SECRET_ACCESS_KEY",
            "GIT_SSH_COMMAND",
            "HTTPS_PROXY",
            "LC_ALL",
            "pexels_key",
            "1KEY",
            "A-B",
            "",
            "A B",
        ] {
            assert!(check_variable(bad).is_err(), "{bad}");
        }
        for bad in ["*", "TERMF_*", "TERMF_TOKEN", "A-B", "A*B"] {
            assert!(check_pattern(bad).is_err(), "{bad}");
        }
        check_pattern("ACME_*").unwrap();
        check_pattern("http_proxy").unwrap();
    }

    #[test]
    fn reads_the_global_then_the_project_s_settings() {
        let config = dir("load");
        fs::write(
            config.join("termf.toml"),
            "[agents]\nclaude = \"claude\"\nenv_allow = [\"CORP_TOKEN\", \"bad name\"]\n\n[agents.secrets]\nPEXELS_API_KEY = \"pexels\"\nLIVE_KEY = \"prod/live\"\n",
        )
        .unwrap();
        fs::write(
            config.join("projects/acme.toml"),
            "name = \"acme\"\n\n[agents]\nenv_allow = [\"ACME_*\"]\n\n[agents.secrets]\nPEXELS_API_KEY = \"staging/acme-pexels\"\nPATH = \"x\"\n",
        )
        .unwrap();
        let env = load(&config, Some("acme"));
        assert!(!env.inherit);
        assert_eq!(env.allow, ["CORP_TOKEN", "ACME_*"]);
        assert_eq!(
            env.secrets,
            [Grant {
                variable: "PEXELS_API_KEY".into(),
                secret: "acme-pexels".into(),
                scope: "staging".into(),
                project: true
            }]
        );
        let problems = env.problems.join("\n");
        assert!(problems.contains("\"bad name\""), "{problems}");
        assert!(
            problems.contains("LIVE_KEY: agents never get a prod secret"),
            "{problems}"
        );
        assert!(
            problems.contains("the project's [agents.secrets] PATH"),
            "{problems}"
        );
        assert_eq!(load(&config, None).secrets[0].secret, "pexels");
        // The settings still read, `[agents]` keeping its commands only.
        let settings = crate::load_settings(&config).unwrap();
        assert_eq!(
            settings.agents,
            BTreeMap::from([("claude".into(), "claude".into())])
        );
        fs::write(config.join("termf.toml"), "[agents]\nenv = \"inherit\"\n").unwrap();
        assert!(load(&config, None).inherit);
        fs::remove_dir_all(&config).unwrap();
    }

    #[test]
    fn grants_and_allowlist_are_written_with_the_rest_of_the_files_kept() {
        let config = dir("write");
        let project = "# acme\nname = \"acme\" # the client\n\n[[repos]]\npath = \"~/acme\"\n";
        fs::write(config.join("projects/acme.toml"), project).unwrap();
        fs::write(
            config.join("termf.toml"),
            "# mine\n[agents]\nclaude = \"claude\" # keep\n",
        )
        .unwrap();
        give(&config, "acme", "PEXELS_API_KEY", "dev/PEXELS_API_KEY").unwrap();
        let written = fs::read_to_string(config.join("projects/acme.toml")).unwrap();
        assert_eq!(
            written,
            format!("{project}\n[agents.secrets]\nPEXELS_API_KEY = \"dev/PEXELS_API_KEY\"\n")
        );
        assert!(give(&config, "acme", "LIVE", "prod/live").is_err());
        assert!(give(&config, "acme", "NODE_OPTIONS", "x").is_err());
        take_back(&config, Some("acme"), "PEXELS_API_KEY").unwrap();
        assert_eq!(
            fs::read_to_string(config.join("projects/acme.toml"))
                .unwrap()
                .trim_end(),
            project.trim_end()
        );
        save_allow(&config, &["ACME_*".into()]).unwrap();
        assert_eq!(
            fs::read_to_string(config.join("termf.toml")).unwrap(),
            "# mine\n[agents]\nclaude = \"claude\" # keep\nenv_allow = [\"ACME_*\"]\n"
        );
        assert!(save_allow(&config, &["TERMF_*".into()]).is_err());
        save_allow(&config, &[]).unwrap();
        assert_eq!(
            fs::read_to_string(config.join("termf.toml")).unwrap(),
            "# mine\n[agents]\nclaude = \"claude\" # keep\n"
        );
        fs::remove_dir_all(&config).unwrap();
    }
}
