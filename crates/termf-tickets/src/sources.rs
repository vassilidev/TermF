//! Where a project's tickets come from: `[trackers.*]` in its file. The same shape goes to the UI (JSON) and
//! the file (TOML): every key is one word.

use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::{TicketSummary, Tickets, github, jira, mcp};

/// `[trackers]` of a project.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Trackers {
    #[serde(default)]
    pub github: Option<GithubTracker>,
    #[serde(default)]
    pub jira: Option<JiraTracker>,
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

/// `[trackers.jira]`: Jira Cloud. The API token is in the system's keychain, never in the file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct JiraTracker {
    /// `acme.atlassian.net`.
    pub site: String,
    /// The Atlassian account the API token belongs to.
    pub email: String,
    /// The inbox's JQL; none: [`jira::DEFAULT_JQL`].
    #[serde(default)]
    pub jql: Option<String>,
    /// The Jira project key (`ACME`) a pasted key must have to go to this project; none: any key.
    #[serde(default)]
    pub project: Option<String>,
    /// Link the pull request to the ticket once a run is delivered.
    #[serde(default)]
    pub comment: bool,
    /// The status a delivered run moves its ticket to; none: left alone.
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
    /// Whether the keychain holds an API token for `[trackers.jira]`'s site and e-mail.
    pub jira_token: bool,
    /// The `owner/name` of the project's GitHub repositories, which GitHub Issues reads.
    pub github_repos: Vec<String>,
    /// The read-only tools TermF knows for each MCP source's server, used when the source lists none.
    pub mcp_tools: BTreeMap<String, Vec<String>>,
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

/// A source of the inbox: `github`, `githubProject`, `jira` or `mcp:<name>`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Source {
    GithubIssues,
    GithubProject,
    Jira,
    Mcp(String),
}

impl Source {
    pub(crate) fn parse(id: &str) -> Result<Self> {
        Ok(match id {
            "github" => Source::GithubIssues,
            "githubProject" => Source::GithubProject,
            "jira" => Source::Jira,
            _ => match id.strip_prefix("mcp:") {
                Some(name) if !name.is_empty() => Source::Mcp(name.to_owned()),
                _ => bail!("unknown ticket source {id}"),
            },
        })
    }

    pub(crate) fn id(&self) -> String {
        match self {
            Source::GithubIssues => "github".into(),
            Source::GithubProject => "githubProject".into(),
            Source::Jira => "jira".into(),
            Source::Mcp(name) => format!("mcp:{name}"),
        }
    }

    pub(crate) fn label(&self) -> String {
        match self {
            Source::GithubIssues => "GitHub Issues".into(),
            Source::GithubProject => "GitHub Projects".into(),
            Source::Jira => "Jira".into(),
            Source::Mcp(name) => format!("MCP {name}"),
        }
    }
}

impl Trackers {
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
        if self.jira.is_some() {
            sources.push(Source::Jira);
        }
        sources.extend(self.mcp.keys().map(|name| Source::Mcp(name.clone())));
        sources
    }

    /// Refuses what no source could use, before it reaches the file.
    fn check(&self) -> Result<()> {
        if let Some(github) = &self.github {
            if let Some(repo) = github.repo() {
                github::split(&format!("{repo}#1"))
                    .context("GitHub Issues: the repository is owner/name")?;
            }
            if let Some(board) = &github.project {
                github::Board::parse(board)?;
            }
        }
        if let Some(jira) = &self.jira {
            jira::base(&jira.site)?;
            ensure!(
                jira.email.contains('@'),
                "Jira: {:?} is not an e-mail address",
                jira.email
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
        let jira_token = trackers.jira.as_ref().is_some_and(|j| {
            self.jira_token(&j.site, &j.email)
                .is_ok_and(|t| t.is_some())
        });
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
        Ok(Sources {
            trackers,
            jira_token,
            github_repos,
            mcp_tools,
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
        match &trackers.jira {
            None => {
                root.remove("jira");
            }
            Some(j) => {
                let t = table(root, "jira")?;
                text(t, "site", Some(&j.site));
                text(t, "email", Some(&j.email));
                let jql = j.jql.as_deref().filter(|q| q.trim() != jira::DEFAULT_JQL);
                text(t, "jql", jql);
                text(t, "project", j.project.as_deref());
                flag(t, "comment", j.comment, false);
                text(t, "transition", j.transition.as_deref());
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
