//! Where a project's tickets come from: `[trackers.*]` in its file. The same shape goes to the UI (JSON) and
//! the file (TOML): every key is one word.

use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, value};

use crate::{TicketSummary, Tickets, azure, github, gitlab, jira, linear, mcp};

/// `[trackers]` of a project.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Trackers {
    #[serde(default)]
    pub github: Option<GithubTracker>,
    /// Its Jira sites.
    #[serde(default, deserialize_with = "jira_sites")]
    pub jira: Vec<JiraTracker>,
    #[serde(default)]
    pub linear: Option<LinearTracker>,
    #[serde(default)]
    pub gitlab: Option<GitlabTracker>,
    #[serde(default)]
    pub azure: Option<AzureTracker>,
    /// By the source's name.
    #[serde(default)]
    pub mcp: BTreeMap<String, McpTracker>,
    /// The project's file has no `[trackers]`: GitHub Issues on its repositories' GitHub remotes, as before
    /// sources could be chosen. Saving the sources writes them down.
    #[serde(default, skip_deserializing)]
    pub implicit: bool,
}

/// `[trackers.github]`: GitHub Issues and, with `project`, a Projects v2 board.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(from = "GithubFile")]
pub struct GithubTracker {
    /// `owner/name`, where the project's issues are: `#42` is read as `owner/name#42`.
    pub repo: Option<String>,
    /// Comment on the ticket with the pull request's link once a run is delivered.
    pub comment: bool,
    /// The inbox lists the open issues assigned to the user in the project's repositories and `repo`. On
    /// unless a board is named.
    pub issues: bool,
    /// A Projects v2 board, `owner/number`: the inbox lists its open issues.
    pub project: Option<String>,
    /// The board's Status values to list; none: all.
    pub columns: Vec<String>,
    /// Only the board's issues assigned to the user.
    pub mine: bool,
    /// The board's Status a delivered run moves its issue to; none: left alone.
    pub transition: Option<String>,
}

/// [`GithubTracker`] as written, where `issues` defaults by `project`.
#[derive(Deserialize)]
struct GithubFile {
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    comment: bool,
    #[serde(default)]
    issues: Option<bool>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    columns: Vec<String>,
    #[serde(default = "yes")]
    mine: bool,
    #[serde(default)]
    transition: Option<String>,
}

impl From<GithubFile> for GithubTracker {
    fn from(f: GithubFile) -> Self {
        Self {
            issues: f.issues.unwrap_or(f.project.is_none()),
            repo: f.repo,
            comment: f.comment,
            project: f.project,
            columns: f.columns,
            mine: f.mine,
            transition: f.transition,
        }
    }
}

impl GithubTracker {
    /// `repo`, unless left blank.
    pub(crate) fn repo(&self) -> Option<&str> {
        self.repo
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
    }
}

/// `[[trackers.jira]]`: one Jira Cloud site and the spaces (Jira's projects) read there. The API token is in the
/// system's keychain, by site and e-mail, never in the file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct JiraTracker {
    /// `acme.atlassian.net`.
    pub site: String,
    /// The Atlassian account the API token belongs to.
    pub email: String,
    /// The inbox's JQL, an advanced choice; none: the user's open tickets in `spaces`.
    #[serde(default)]
    pub jql: Option<String>,
    /// The spaces' keys (`APP`, `API`): what is read, and the prefixes a pasted key goes here by; none: every
    /// space. `project = "APP, API"`, as a single site was written before, reads the same.
    #[serde(default, alias = "project", deserialize_with = "space_keys")]
    pub spaces: Vec<String>,
    /// Link the pull request to the ticket once a run is delivered.
    #[serde(default)]
    pub comment: bool,
    /// The status a delivered run moves its ticket to; none: left alone.
    #[serde(default)]
    pub transition: Option<String>,
}

/// The spaces as a list, or as the text `APP, API` the file had before sites became several.
fn space_keys<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Keys {
        Text(Option<String>),
        List(Vec<String>),
    }
    let keys = match Keys::deserialize(d)? {
        Keys::Text(text) => text
            .unwrap_or_default()
            .split(',')
            .map(str::to_owned)
            .collect(),
        Keys::List(list) => list,
    };
    Ok(keys
        .iter()
        .map(|k| k.trim().to_ascii_uppercase())
        .filter(|k| !k.is_empty())
        .collect())
}

/// One `[trackers.jira]` table, as a project's file had it, or the `[[trackers.jira]]` of several sites.
fn jira_sites<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<JiraTracker>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Sites {
        One(JiraTracker),
        Many(Vec<JiraTracker>),
        Nothing,
    }
    Ok(match Sites::deserialize(d)? {
        Sites::One(site) => vec![site],
        Sites::Many(sites) => sites,
        Sites::Nothing => Vec::new(),
    })
}

impl JiraTracker {
    /// The spaces read, as keys.
    pub(crate) fn keys(&self) -> Vec<&str> {
        self.spaces.iter().map(String::as_str).collect()
    }

    /// The inbox's JQL: the advanced one when given, else the user's open tickets in the site's spaces.
    pub(crate) fn inbox_jql(&self) -> String {
        if let Some(jql) = self.jql.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            return jql.to_owned();
        }
        let spaces: Vec<&str> = self
            .keys()
            .into_iter()
            .filter(|k| jira::project_key(k))
            .collect();
        match spaces.as_slice() {
            [] => jira::DEFAULT_JQL.to_owned(),
            keys => format!("project in ({}) AND {}", keys.join(", "), jira::DEFAULT_JQL),
        }
    }

    /// Whether a pasted key with this prefix goes to this source: none listed takes any.
    pub(crate) fn takes(&self, prefix: &str) -> bool {
        let keys = self.keys();
        keys.is_empty() || keys.iter().any(|k| k.eq_ignore_ascii_case(prefix))
    }
}

/// `[trackers.linear]`: Linear. The personal API key is in the system's keychain, never in the file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinearTracker {
    /// The workspace's URL key: `acme` for `linear.app/acme`.
    pub workspace: String,
    /// The team key (`ENG`): the inbox lists its tickets only, and a pasted key must have it. None: every team.
    #[serde(default)]
    pub team: Option<String>,
    /// Only the tickets assigned to the user.
    #[serde(default = "yes")]
    pub mine: bool,
    /// The states (their names) to list; none: every state that is not done or canceled.
    #[serde(default)]
    pub states: Vec<String>,
    /// Link the pull request to the ticket once a run is delivered.
    #[serde(default)]
    pub comment: bool,
    /// The state a delivered run moves its ticket to; none: left alone.
    #[serde(default)]
    pub transition: Option<String>,
}

impl LinearTracker {
    /// `team`, unless left blank.
    pub(crate) fn team(&self) -> Option<&str> {
        self.team
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }
}

/// `[trackers.gitlab]`: GitLab issues, on gitlab.com or a GitLab of its own. `glab` signs in, else an access
/// token in the system's keychain.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GitlabTracker {
    /// `gitlab.com`, or the host of a GitLab of its own.
    #[serde(default = "gitlab_host")]
    pub host: String,
    /// `group/project`, where the project's issues are: `#42` is read as `group/project#42`.
    #[serde(default)]
    pub repo: Option<String>,
    /// Only the issues assigned to the user.
    #[serde(default = "yes")]
    pub mine: bool,
    /// Comment on the issue with the merge request's link once a run is delivered.
    #[serde(default)]
    pub comment: bool,
}

impl GitlabTracker {
    /// `repo`, unless left blank.
    pub(crate) fn repo(&self) -> Option<&str> {
        self.repo
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
    }
}

fn gitlab_host() -> String {
    gitlab::DEFAULT_HOST.into()
}

/// `[trackers.azure]`: Azure DevOps Boards. The personal access token is in the system's keychain.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AzureTracker {
    /// `acme` for `dev.azure.com/acme`.
    pub organization: String,
    /// The Azure DevOps project whose work items the inbox lists.
    pub project: String,
    /// The inbox's WIQL; none: [`azure::DEFAULT_WIQL`].
    #[serde(default)]
    pub wiql: Option<String>,
    /// Link the pull request to the work item once a run is delivered.
    #[serde(default)]
    pub comment: bool,
    /// The state a delivered run moves its work item to; none: left alone.
    #[serde(default)]
    pub transition: Option<String>,
}

/// `[trackers.mcp.<name>]`: a server of the user's Claude Code, read through Claude Code.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct McpTracker {
    /// The server's name in Claude Code (`claude mcp list`).
    pub server: String,
    /// The server's tools Claude may call, all of them read-only; none: the known defaults for that server.
    #[serde(default)]
    pub tools: Vec<String>,
    /// What to look for, in the user's words, such as "the ENG team's issues in the current cycle".
    #[serde(default)]
    pub prompt: Option<String>,
}

fn yes() -> bool {
    true
}

/// A project's sources, for its editor.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sources {
    pub trackers: Trackers,
    /// Whether the keychain holds an API token for each Jira site's e-mail, in the order of `trackers.jira`.
    pub jira_tokens: Vec<bool>,
    /// The `owner/name` of the project's GitHub repositories, which GitHub Issues reads.
    pub github_repos: Vec<String>,
    /// The read-only tools TermF knows for each MCP source's server, used when the source lists none.
    pub mcp_tools: BTreeMap<String, Vec<String>>,
    /// Whether the keychain holds a token for the Linear workspace, the GitLab host and the Azure DevOps
    /// organization the sources name.
    pub tokens: Tokens,
    /// The `group/project` of the project's repositories on the GitLab host, which GitLab reads.
    pub gitlab_repos: Vec<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Tokens {
    pub linear: bool,
    pub gitlab: bool,
    pub azure: bool,
}

/// Which sources a project's inbox reads.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Configured {
    pub project: String,
    /// At least one source: without one, the project's inbox is empty.
    pub configured: bool,
    /// Their ids, as [`Source`] names them.
    pub sources: Vec<String>,
}

/// A source of the inbox: `github`, `githubProject`, `gitlab`, `jira:<site's host>` (`jira`: the first site),
/// `linear`, `azure` or `mcp:<name>`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Source {
    GithubIssues,
    GithubProject,
    Gitlab,
    /// A Jira site by its host; empty: the project's first.
    Jira(String),
    Linear,
    Azure,
    Mcp(String),
}

impl Source {
    pub(crate) fn parse(id: &str) -> Result<Self> {
        Ok(match id {
            "github" => Source::GithubIssues,
            "githubProject" => Source::GithubProject,
            "jira" => Source::Jira(String::new()),
            "gitlab" => Source::Gitlab,
            "linear" => Source::Linear,
            "azure" => Source::Azure,
            _ => match (id.strip_prefix("mcp:"), id.strip_prefix("jira:")) {
                (Some(name), _) if !name.is_empty() => Source::Mcp(name.to_owned()),
                (_, Some(host)) if !host.is_empty() => Source::Jira(host.to_ascii_lowercase()),
                _ => bail!("unknown ticket source {id}"),
            },
        })
    }

    pub(crate) fn id(&self) -> String {
        match self {
            Source::GithubIssues => "github".into(),
            Source::GithubProject => "githubProject".into(),
            Source::Jira(host) if host.is_empty() => "jira".into(),
            Source::Jira(host) => format!("jira:{host}"),
            Source::Gitlab => "gitlab".into(),
            Source::Linear => "linear".into(),
            Source::Azure => "azure".into(),
            Source::Mcp(name) => format!("mcp:{name}"),
        }
    }

    pub(crate) fn label(&self) -> String {
        match self {
            Source::GithubIssues => "GitHub Issues".into(),
            Source::GithubProject => "GitHub Projects".into(),
            Source::Jira(host) if host.is_empty() => "Jira".into(),
            Source::Jira(host) => format!("Jira {host}"),
            Source::Gitlab => "GitLab".into(),
            Source::Linear => "Linear".into(),
            Source::Azure => "Azure DevOps".into(),
            Source::Mcp(name) => format!("MCP {name}"),
        }
    }
}

impl Trackers {
    /// The Jira site at `host`; an empty host: the first.
    pub(crate) fn jira_site(&self, host: &str) -> Option<&JiraTracker> {
        if host.is_empty() {
            return self.jira.first();
        }
        self.jira
            .iter()
            .find(|j| jira::host(&j.site).is_ok_and(|h| h.eq_ignore_ascii_case(host)))
    }

    /// What a project without `[trackers]` reads.
    fn implicit() -> Self {
        Self {
            github: Some(GithubTracker::from(GithubFile {
                repo: None,
                comment: false,
                issues: None,
                project: None,
                columns: Vec::new(),
                mine: true,
                transition: None,
            })),
            implicit: true,
            ..Self::default()
        }
    }

    /// The sources that feed the inbox, natives first so they win over an MCP copy of the same ticket, and
    /// the board before Issues so an issue on both keeps its column.
    pub(crate) fn sources(&self) -> Vec<Source> {
        let mut sources = Vec::new();
        if let Some(github) = &self.github {
            if github.project.is_some() {
                sources.push(Source::GithubProject);
            }
            if github.issues {
                sources.push(Source::GithubIssues);
            }
        }
        if self.gitlab.is_some() {
            sources.push(Source::Gitlab);
        }
        // A single site keeps the id `jira` it always had.
        for site in &self.jira {
            let host = match self.jira.len() {
                1 => String::new(),
                _ => jira::host(&site.site).unwrap_or_else(|_| site.site.clone()),
            };
            sources.push(Source::Jira(host));
        }
        if self.linear.is_some() {
            sources.push(Source::Linear);
        }
        if self.azure.is_some() {
            sources.push(Source::Azure);
        }
        sources.extend(self.mcp.keys().map(|name| Source::Mcp(name.clone())));
        sources
    }

    /// Refuses what no source could use, before it reaches the file.
    pub(crate) fn check(&self) -> Result<()> {
        if let Some(github) = &self.github {
            if let Some(repo) = github.repo() {
                github::split(&format!("{repo}#1"))
                    .context("GitHub Issues: the repository is owner/name")?;
            }
            if let Some(board) = &github.project {
                github::Board::parse(board)?;
            }
        }
        let mut hosts = Vec::new();
        for jira in &self.jira {
            jira::base(&jira.site)?;
            ensure!(
                jira.email.contains('@'),
                "Jira: {:?} is not an e-mail address",
                jira.email
            );
            for key in &jira.spaces {
                ensure!(
                    jira::project_key(key),
                    "Jira: {key:?} is not a space key, such as ACME"
                );
            }
            let host = jira::host(&jira.site)?;
            ensure!(
                !hosts.contains(&host),
                "Jira: {host} is twice in the sources"
            );
            hosts.push(host);
        }
        if let Some(linear) = &self.linear {
            linear::workspace(&linear.workspace)?;
            if let Some(team) = linear.team() {
                ensure!(
                    team.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                    "Linear: {team:?} is not a team key, such as ENG"
                );
            }
        }
        if let Some(gitlab) = &self.gitlab {
            gitlab::base(&gitlab.host)?;
            if let Some(repo) = gitlab.repo() {
                gitlab::split(&format!("{repo}#1"))
                    .context("GitLab: the project is group/project")?;
            }
        }
        if let Some(azure) = &self.azure {
            azure::base(&azure.organization)?;
            ensure!(
                !azure.project.trim().is_empty(),
                "Azure DevOps: name the project"
            );
        }
        for (name, source) in &self.mcp {
            ensure!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
                "{name:?} is not a source name: letters, digits, - and _ only"
            );
            mcp::tool_names(&source.server, &source.tools)?;
        }
        Ok(())
    }
}

fn text(table: &mut Table, key: &str, text: Option<&str>) {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        Some(text) => table[key] = value(text),
        None => {
            table.remove(key);
        }
    }
}

fn flag(table: &mut Table, key: &str, on: bool, default: bool) {
    if on == default {
        table.remove(key);
    } else {
        table[key] = value(on);
    }
}

fn list(table: &mut Table, key: &str, items: &[String]) {
    let items: Array = items
        .iter()
        .map(|i| i.trim())
        .filter(|i| !i.is_empty())
        .collect();
    if items.is_empty() {
        table.remove(key);
    } else {
        table[key] = value(items);
    }
}

/// The table `key` of `parent`, created when missing.
fn table<'a>(parent: &'a mut Table, key: &str) -> Result<&'a mut Table> {
    let item = parent.entry(key).or_insert(Item::Table(Table::new()));
    item.as_table_mut()
        .with_context(|| format!("[{key}] must be a table"))
}

/// Writes `trackers` as the file's `[trackers]`: values left at their default are not written, comments and keys
/// TermF does not know are kept.
pub fn write_trackers(doc: &mut DocumentMut, trackers: &Trackers) -> Result<()> {
    let root = table(doc.as_table_mut(), "trackers")?;
    root.set_implicit(true);
    match &trackers.github {
        None => {
            root.remove("github");
        }
        Some(g) => {
            let t = table(root, "github")?;
            text(t, "repo", g.repo());
            flag(t, "comment", g.comment, false);
            flag(t, "issues", g.issues, g.project.is_none());
            text(t, "project", g.project.as_deref());
            list(t, "columns", &g.columns);
            flag(t, "mine", g.mine, true);
            text(t, "transition", g.transition.as_deref());
        }
    }
    // One table per site; a single site's older `[trackers.jira]` becomes the first of them.
    root.remove("jira");
    if !trackers.jira.is_empty() {
        let mut sites = ArrayOfTables::new();
        for j in &trackers.jira {
            let mut t = Table::new();
            text(&mut t, "site", Some(&j.site));
            text(&mut t, "email", Some(&j.email));
            list(&mut t, "spaces", &j.spaces);
            let jql = j.jql.as_deref().filter(|q| q.trim() != jira::DEFAULT_JQL);
            text(&mut t, "jql", jql);
            flag(&mut t, "comment", j.comment, false);
            text(&mut t, "transition", j.transition.as_deref());
            sites.push(t);
        }
        root.insert("jira", Item::ArrayOfTables(sites));
    }
    match &trackers.linear {
        None => {
            root.remove("linear");
        }
        Some(l) => {
            let t = table(root, "linear")?;
            text(t, "workspace", Some(&l.workspace));
            text(t, "team", l.team());
            flag(t, "mine", l.mine, true);
            list(t, "states", &l.states);
            flag(t, "comment", l.comment, false);
            text(t, "transition", l.transition.as_deref());
        }
    }
    match &trackers.gitlab {
        None => {
            root.remove("gitlab");
        }
        Some(g) => {
            let t = table(root, "gitlab")?;
            let host = Some(g.host.trim()).filter(|h| *h != gitlab::DEFAULT_HOST);
            text(t, "host", host);
            text(t, "repo", g.repo());
            flag(t, "mine", g.mine, true);
            flag(t, "comment", g.comment, false);
        }
    }
    match &trackers.azure {
        None => {
            root.remove("azure");
        }
        Some(a) => {
            let t = table(root, "azure")?;
            text(t, "organization", Some(&a.organization));
            text(t, "project", Some(&a.project));
            let wiql = a
                .wiql
                .as_deref()
                .filter(|q| q.trim() != azure::DEFAULT_WIQL);
            text(t, "wiql", wiql);
            flag(t, "comment", a.comment, false);
            text(t, "transition", a.transition.as_deref());
        }
    }
    if trackers.mcp.is_empty() {
        root.remove("mcp");
    } else {
        let all = table(root, "mcp")?;
        all.set_implicit(true);
        all.retain(|name, _| trackers.mcp.contains_key(name));
        for (name, m) in &trackers.mcp {
            let t = table(all, name)?;
            text(t, "server", Some(&m.server));
            list(t, "tools", &m.tools);
            text(t, "prompt", m.prompt.as_deref());
        }
    }
    root.set_implicit(!root.is_empty());
    Ok(())
}

impl Tickets {
    pub(crate) fn trackers(&self, project: &str) -> Result<Trackers> {
        #[derive(Deserialize)]
        struct ProjectFile {
            trackers: Option<Trackers>,
        }
        let file = termf_core::project_file(&self.config_dir, project)?;
        let parsed: ProjectFile = toml::from_str(&fs::read_to_string(&file)?)
            .with_context(|| format!("invalid {}", file.display()))?;
        Ok(parsed.trackers.unwrap_or_else(Trackers::implicit))
    }

    pub fn sources(&self, project: &str) -> Result<Sources> {
        let trackers = self.trackers(project)?;
        let jira_tokens = trackers
            .jira
            .iter()
            .map(|j| {
                self.jira_token(&j.site, &j.email)
                    .is_ok_and(|t| t.is_some())
            })
            .collect();
        let mcp_tools = trackers
            .mcp
            .iter()
            .map(|(name, m)| (name.clone(), mcp::default_tools(&m.server)))
            .collect();
        let info = self.projects(Some(project))?.remove(0);
        let mut github_repos: Vec<String> =
            info.repos.iter().flat_map(|r| r.slugs.clone()).collect();
        github_repos.sort();
        github_repos.dedup();
        let has = |kind: &str, target: Option<&str>| {
            target.is_some_and(|t| self.has_token(kind, t).unwrap_or(false))
        };
        let tokens = Tokens {
            linear: has(
                "linear",
                trackers.linear.as_ref().map(|l| l.workspace.as_str()),
            ),
            gitlab: has("gitlab", trackers.gitlab.as_ref().map(|g| g.host.as_str())),
            azure: has(
                "azure",
                trackers.azure.as_ref().map(|a| a.organization.as_str()),
            ),
        };
        Ok(Sources {
            gitlab_repos: info.gitlab_slugs(),
            trackers,
            jira_tokens,
            github_repos,
            mcp_tools,
            tokens,
        })
    }

    /// Writes the project's `[trackers]`, keeping the file's comments and the keys TermF does not know. Values
    /// left at their default are not written; no source at all is an empty `[trackers]`, which the implicit
    /// GitHub Issues source then no longer fills. An MCP source whose settings changed loses its last answer.
    pub fn save_sources(&self, project: &str, trackers: &Trackers) -> Result<Sources> {
        trackers.check()?;
        let before = self.trackers(project)?;
        let file = termf_core::project_file(&self.config_dir, project)?;
        let mut doc: DocumentMut = fs::read_to_string(&file)?
            .parse()
            .with_context(|| format!("invalid {}", file.display()))?;
        write_trackers(&mut doc, trackers)?;
        termf_core::write_atomic(&file, &doc.to_string())?;
        let store = crate::store::Store::open(&self.data_dir)?;
        for (name, old) in &before.mcp {
            if trackers.mcp.get(name) != Some(old) {
                store.forget_cache(project, name)?;
            }
        }
        self.sources(project)
    }

    /// For each project (or only `project`), the sources its inbox reads. Reads the config and the
    /// repositories' remotes, nothing over the network.
    pub fn configured(&self, project: Option<&str>) -> Result<Vec<Configured>> {
        let found: Vec<Configured> = self
            .projects(project)?
            .iter()
            .map(|p| {
                let sources: Vec<String> = p.sources().iter().map(Source::id).collect();
                Configured {
                    configured: !sources.is_empty(),
                    project: p.id.clone(),
                    sources,
                }
            })
            .collect();
        Ok(found)
    }

    /// A few tickets of one source, with `trackers` as the editor holds them, saved or not.
    pub fn test_source(
        &self,
        project: &str,
        trackers: Trackers,
        source: &str,
    ) -> Result<Vec<TicketSummary>> {
        trackers.check()?;
        let mut info = self.projects(Some(project))?.remove(0);
        info.trackers = trackers;
        let source = Source::parse(source)?;
        let mut found = self.source_inbox(&info, &source, crate::Fetch::Test)?;
        found.truncate(crate::TEST_SIZE);
        Ok(found)
    }
}
