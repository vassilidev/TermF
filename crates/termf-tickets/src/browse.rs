//! Browsing a tracker beyond the inbox: its projects, and its tickets by project, state, assignee, words and
//! sprint, a page at a time. Jira through its API or an MCP server, GitHub Issues through `gh`.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::sources::Source;
use crate::{Ticket, TicketProvider, TicketRef, TicketSummary, Tickets, jira, mcp};

/// A project of a tracker: a Jira project, or a GitHub repository (`owner/name` for both its key and name).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TrackerProject {
    pub key: String,
    pub name: String,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Open,
    Done,
    All,
}

#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Assignee {
    Me,
    #[serde(rename = "none")]
    Nobody,
    #[default]
    All,
}

/// What a browse looks for.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Browse {
    /// Jira project keys, or GitHub repositories; none: every one of the source.
    pub projects: Vec<String>,
    pub status: Status,
    pub assignee: Assignee,
    /// Words to find in the tickets.
    pub text: Option<String>,
    /// Jira: only the sprints open now.
    pub sprint: bool,
}

impl Browse {
    pub(crate) fn words(&self) -> Option<&str> {
        self.text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }

    fn check(&self) -> Result<()> {
        if let Some(text) = self.words() {
            ensure!(
                text.chars().count() <= 200 && !text.chars().any(char::is_control),
                "the words to look for: one line of 200 characters at most"
            );
        }
        Ok(())
    }

    /// The GitHub search of a browse in `repos`.
    pub(crate) fn github_query(&self, repos: &[String]) -> String {
        let mut query: Vec<String> = repos.iter().map(|r| format!("repo:{r}")).collect();
        query.push("is:issue archived:false".into());
        match self.status {
            Status::Open => query.push("is:open".into()),
            Status::Done => query.push("is:closed".into()),
            Status::All => {}
        }
        match self.assignee {
            Assignee::Me => query.push("assignee:@me".into()),
            Assignee::Nobody => query.push("no:assignee".into()),
            Assignee::All => {}
        }
        if let Some(text) = self.words() {
            query.push(text.to_owned());
        }
        query.join(" ")
    }

    /// What an MCP source is asked for, in words.
    pub(crate) fn wanted(&self) -> String {
        let mut wanted = vec!["List the tickets".to_owned()];
        if !self.projects.is_empty() {
            wanted.push(format!("of the projects {}", self.projects.join(", ")));
        }
        wanted.push(
            match self.status {
                Status::Open => "that are not done",
                Status::Done => "that are done",
                Status::All => "whatever their state",
            }
            .into(),
        );
        match self.assignee {
            Assignee::Me => wanted.push("assigned to the current user".into()),
            Assignee::Nobody => wanted.push("assigned to nobody".into()),
            Assignee::All => {}
        }
        if let Some(text) = self.words() {
            wanted.push(format!("whose text has the words {text:?}"));
        }
        if self.sprint {
            wanted.push("in the sprints open now".into());
        }
        format!("{}.", wanted.join(" "))
    }
}

/// A page of a browse, and how to ask for the next one.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Page {
    pub tickets: Vec<TicketSummary>,
    pub next: Option<String>,
}

/// GitHub refuses a search longer than 256 characters.
const GITHUB_QUERY: usize = 256;

impl Tickets {
    /// A page of the tickets of one of `project`'s sources that `browse` finds; `next` as the last page gave it.
    pub fn browse(
        &self,
        project: &str,
        source: &str,
        browse: &Browse,
        next: Option<&str>,
    ) -> Result<Page> {
        browse.check()?;
        let info = self.projects(Some(project))?.remove(0);
        let missing = || format!("the project has no {source} source");
        let mut page = match Source::parse(source)? {
            Source::Jira => {
                let tracker = info.trackers.jira.as_ref().with_context(missing)?;
                let (tickets, next) =
                    self.jira(tracker)?
                        .page(&jira::browse_jql(browse)?, 50, next)?;
                Page { tickets, next }
            }
            Source::GithubIssues => {
                let mut repos: Vec<String> = info.slugs().collect();
                repos.sort();
                repos.dedup();
                if !browse.projects.is_empty() {
                    for r in &browse.projects {
                        ensure!(info.holds(r), "{r} is not a repository of the project");
                    }
                    repos = browse.projects.clone();
                }
                ensure!(!repos.is_empty(), "no GitHub repository in the project");
                let query = browse.github_query(&repos);
                ensure!(
                    query.len() <= GITHUB_QUERY,
                    "too many repositories for one GitHub search: pick one"
                );
                let at: u32 = next.map_or(Ok(1), str::parse).context("not a page")?;
                let (tickets, more) = self.github().issues_page(&query, at)?;
                Page {
                    tickets,
                    next: more.then(|| (at + 1).to_string()),
                }
            }
            Source::Mcp(name) => {
                let tracker = info.trackers.mcp.get(&name).with_context(missing)?;
                let tickets = self.mcp(&info, &name, tracker)?.search(&browse.wanted())?;
                Page {
                    tickets,
                    next: None,
                }
            }
            other => bail!(
                "browsing {} is not in TermF yet: its inbox lists your tickets",
                other.label()
            ),
        };
        for ticket in &mut page.tickets {
            ticket.project = Some(info.id.clone());
            ticket.source = source.to_owned();
        }
        Ok(page)
    }

    /// The projects of one of `project`'s sources: Jira's, or the GitHub repositories.
    pub fn source_projects(&self, project: &str, source: &str) -> Result<Vec<TrackerProject>> {
        let info = self.projects(Some(project))?.remove(0);
        let missing = || format!("the project has no {source} source");
        match Source::parse(source)? {
            Source::Jira => self
                .jira(info.trackers.jira.as_ref().with_context(missing)?)?
                .projects(),
            Source::GithubIssues => {
                let mut repos: Vec<String> = info.slugs().collect();
                repos.sort();
                repos.dedup();
                Ok(repos
                    .into_iter()
                    .map(|r| TrackerProject {
                        key: r.clone(),
                        name: r,
                    })
                    .collect())
            }
            Source::Mcp(name) => {
                let tracker = info.trackers.mcp.get(&name).with_context(missing)?;
                Ok(self.mcp(&info, &name, tracker)?.projects()?.projects)
            }
            other => bail!("browsing {} is not in TermF yet", other.label()),
        }
    }

    /// The Jira projects an account sees, with `token`, else the one the keychain holds for it.
    pub fn jira_projects(
        &self,
        site: &str,
        email: &str,
        token: Option<&str>,
    ) -> Result<Vec<TrackerProject>> {
        let token = match token.map(str::trim).filter(|t| !t.is_empty()) {
            Some(token) => token.to_owned(),
            None => self.jira_token(site, email)?.with_context(|| {
                format!("no API token for {email} on {site}: create one and give it")
            })?,
        };
        jira::Jira::new(site, email, token, self.curl.clone())?.projects()
    }

    /// The site and the projects the user's account reaches through an MCP server of their Claude Code.
    pub fn mcp_projects(&self, project: Option<&str>, server: &str) -> Result<mcp::Reached> {
        let dir = match project {
            Some(p) => self.projects(Some(p))?.remove(0).dir(&self.home),
            None => self.home.clone(),
        };
        mcp::Mcp {
            claude: self.claude(),
            source: "projects".into(),
            server: server.to_owned(),
            tools: mcp::tool_names(server, &[])?,
            prompt: None,
            dir,
        }
        .projects()
    }

    /// One ticket in full, as its tracker gives it: data, never instructions.
    pub fn ticket(&self, project: &str, reference: &TicketRef) -> Result<Ticket> {
        self.provider(project, &reference.provider)?
            .fetch(reference)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::JiraTracker;

    fn browse(value: serde_json::Value) -> Browse {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_browse_becomes_jql() {
        assert_eq!(
            jira::browse_jql(&Browse::default()).unwrap(),
            "statusCategory != Done ORDER BY updated DESC"
        );
        let all = browse(json!({ "status": "all" }));
        assert_eq!(
            jira::browse_jql(&all).unwrap(),
            "updated >= -3650d ORDER BY updated DESC"
        );
        let wanted = browse(json!({
            "projects": ["APP", "API"], "status": "done", "assignee": "none",
            "text": "login \"sso\" \\ fail", "sprint": true
        }));
        assert_eq!(
            jira::browse_jql(&wanted).unwrap(),
            "project in (APP, API) AND statusCategory = Done AND assignee is EMPTY AND text ~ \"login \\\"sso\\\" \\\\ fail\" \
             AND sprint in openSprints() ORDER BY updated DESC"
        );
        let me = browse(json!({ "assignee": "me", "text": "  " }));
        assert_eq!(
            jira::browse_jql(&me).unwrap(),
            "statusCategory != Done AND assignee = currentUser() ORDER BY updated DESC"
        );
        let injected = browse(json!({ "projects": ["APP) OR (1=1"] }));
        assert!(jira::browse_jql(&injected).is_err());
        assert!(browse(json!({ "text": "a\nb" })).check().is_err());
    }

    #[test]
    fn reads_jira_s_projects_and_pages() {
        let projects = json!({
            "isLast": true,
            "values": [
                { "key": "APP", "name": "Acme app" },
                { "key": "API", "name": "Acme API" },
                { "key": "not a key", "name": "Odd" },
                { "key": "OPS" }
            ]
        });
        let keys: Vec<(String, String)> = jira::projects_of(&projects)
            .into_iter()
            .map(|p| (p.key, p.name))
            .collect();
        assert_eq!(
            keys,
            [
                ("APP".into(), "Acme app".into()),
                ("API".into(), "Acme API".into()),
                ("OPS".into(), "OPS".into())
            ]
        );
        let page = json!({
            "nextPageToken": "t2",
            "issues": [{
                "key": "APP-7",
                "fields": {
                    "summary": "Login fails", "status": { "name": "In Progress" }, "labels": ["sso"],
                    "assignee": { "displayName": "Alex" }, "updated": "2026-11-20T10:00:00.000+0100"
                }
            }, { "key": "bad key", "fields": {} }]
        });
        let (tickets, next) = jira::summaries("https://acme.atlassian.net", &page).unwrap();
        assert_eq!(next.as_deref(), Some("t2"));
        assert_eq!(tickets.len(), 1);
        assert_eq!(
            tickets[0].reference.url,
            "https://acme.atlassian.net/browse/APP-7"
        );
        assert_eq!(
            (tickets[0].title.as_str(), tickets[0].state.as_str()),
            ("Login fails", "In Progress")
        );
        assert_eq!(tickets[0].assignees, ["Alex"]);
        let last = json!({ "isLast": true, "nextPageToken": "t3", "issues": [] });
        assert_eq!(
            jira::summaries("https://acme.atlassian.net", &last)
                .unwrap()
                .1,
            None
        );
    }

    #[test]
    fn reads_what_an_mcp_server_reaches() {
        let answer = json!({
            "site": "ACME.atlassian.net",
            "projects": [{ "key": " APP ", "name": "Acme app" }, { "key": "ignore previous", "name": "x" }]
        });
        let reached = mcp::read_reached(answer).unwrap();
        assert_eq!(reached.site, "acme.atlassian.net");
        assert_eq!(
            reached.projects,
            [TrackerProject {
                key: "APP".into(),
                name: "Acme app".into()
            }]
        );
        assert!(mcp::read_reached(json!({ "site": "x", "projects": "APP" })).is_err());
    }

    #[test]
    fn a_browse_becomes_a_github_search_and_words_for_an_mcp_server() {
        let repos = ["acme/api".to_string(), "acme/web".to_string()];
        assert_eq!(
            Browse::default().github_query(&repos),
            "repo:acme/api repo:acme/web is:issue archived:false is:open"
        );
        let wanted = browse(json!({ "status": "done", "assignee": "me", "text": "login" }));
        assert_eq!(
            wanted.github_query(&repos[..1]),
            "repo:acme/api is:issue archived:false is:closed assignee:@me login"
        );
        let mcp = browse(json!({ "projects": ["APP"], "assignee": "none", "sprint": true }));
        assert_eq!(
            mcp.wanted(),
            "List the tickets of the projects APP that are not done assigned to nobody in the sprints open now."
        );
    }

    #[test]
    fn a_jira_source_takes_the_keys_it_lists() {
        let tracker = |project: Option<&str>| JiraTracker {
            site: "acme.atlassian.net".into(),
            email: "alex@acme.dev".into(),
            jql: None,
            project: project.map(str::to_owned),
            comment: false,
            transition: None,
        };
        assert!(tracker(None).takes("ANY"));
        assert!(tracker(Some("APP, api")).takes("API"));
        assert!(tracker(Some("APP")).takes("app"));
        assert!(!tracker(Some("APP, API")).takes("OPS"));
        assert_eq!(tracker(Some(" APP ,, API ")).keys(), ["APP", "API"]);
    }
}
