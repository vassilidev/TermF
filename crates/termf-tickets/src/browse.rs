//! Browsing a tracker beyond the inbox: its projects, and its tickets by project, state, assignee, words and
//! sprint, a page at a time. Jira through its API or an MCP server, GitHub Issues through `gh`.

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::Ticket;
use crate::sources::Source;
use crate::{TicketProvider, TicketRef, TicketSummary, Tickets, github, jira, mcp};

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

    /// Whether a ticket listed without a search of its own fits the words and the assignee asked for.
    pub(crate) fn matches(&self, ticket: &TicketSummary) -> bool {
        let words = self.words().map(str::to_lowercase);
        let text = format!("{} {}", ticket.reference.key, ticket.title).to_lowercase();
        words.is_none_or(|w| text.contains(&w))
            && (self.assignee != Assignee::Nobody || ticket.assignees.is_empty())
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
            Source::Jira(host) => {
                let tracker = info.trackers.jira_site(&host).with_context(missing)?;
                // Every space the site reads, unless one is picked.
                let mut wanted = browse.clone();
                if wanted.projects.is_empty() {
                    wanted.projects = tracker.spaces.clone();
                }
                let (tickets, next) =
                    self.jira(tracker)?
                        .page(&jira::browse_jql(&wanted)?, 50, next)?;
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
            // shortcut: a board, Linear, GitLab and Azure DevOps read what their inbox reads, filtered here; give
            // them their own search when one is asked for beyond the user's tickets.
            other => {
                let listed = self.source_inbox(&info, &other, crate::Fetch::Refresh)?;
                Page {
                    tickets: listed.into_iter().filter(|t| browse.matches(t)).collect(),
                    next: None,
                }
            }
        };
        for ticket in &mut page.tickets {
            ticket.project = Some(info.id.clone());
            ticket.source = source.to_owned();
        }
        Ok(page)
    }

    /// A source's board: the states it has, as columns, and whether a ticket moves between them.
    pub fn board(&self, project: &str, source: &str) -> Result<Board> {
        let info = self.projects(Some(project))?.remove(0);
        let missing = || format!("the project has no {source} source");
        Ok(match Source::parse(source)? {
            Source::Jira(host) => {
                let tracker = info.trackers.jira_site(&host).with_context(missing)?;
                Board::moving(self.jira(tracker)?.board(&tracker.spaces)?)
            }
            Source::GithubProject => {
                let github = info.trackers.github.as_ref().with_context(missing)?;
                let board = github::Board::parse(github.project.as_deref().with_context(missing)?)?;
                Board::moving(self.github().board_columns(&board)?)
            }
            // Their states come with the tickets.
            Source::Linear | Source::Azure => Board::moving(Vec::new()),
            Source::Mcp(_) => Board {
                columns: Vec::new(),
                movable: false,
                none: false,
            },
            Source::GithubIssues | Source::Gitlab => Board {
                columns: Vec::new(),
                movable: false,
                none: true,
            },
        })
    }

    /// Assigns a ticket in its tracker: `me`, `none`, or someone (a Jira e-mail, a GitHub login).
    pub fn assign(&self, project: &str, reference: &TicketRef, to: &str) -> Result<()> {
        let to = to.trim();
        ensure!(
            !to.is_empty(),
            "say who to assign it to: me, none, or someone"
        );
        match reference.provider.as_str() {
            "jira" => self
                .jira_sites(project)?
                .of(reference)?
                .assign(reference, to),
            "github" => {
                let login = match to {
                    "none" => None,
                    "me" => Some("@me"),
                    login => Some(login.trim_start_matches('@')),
                };
                self.github().assign(reference, login)
            }
            other => {
                bail!("assigning a ticket of {other} is not in TermF yet: assign it in its tracker")
            }
        }
    }
}

/// A board's columns: the source's states in its order.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Board {
    pub columns: Vec<String>,
    /// A ticket dragged to a column moves there in the tracker.
    pub movable: bool,
    /// The source has no states at all (GitHub Issues without a Project, GitLab): the CTO helps instead.
    pub none: bool,
}

impl Board {
    fn moving(columns: Vec<String>) -> Self {
        Self {
            columns,
            movable: true,
            none: false,
        }
    }
}

impl Tickets {
    /// The projects of one of `project`'s sources: Jira's, or the GitHub repositories.
    pub fn source_projects(&self, project: &str, source: &str) -> Result<Vec<TrackerProject>> {
        let info = self.projects(Some(project))?.remove(0);
        let missing = || format!("the project has no {source} source");
        match Source::parse(source)? {
            Source::Jira(host) => {
                let tracker = info.trackers.jira_site(&host).with_context(missing)?;
                let mut spaces = self.jira(tracker)?.projects()?;
                if !tracker.spaces.is_empty() {
                    spaces.retain(|s| tracker.spaces.contains(&s.key));
                }
                Ok(spaces)
            }
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
                let sites = self.mcp(&info, &name, tracker)?.projects()?.sites;
                Ok(sites.into_iter().flat_map(|s| s.spaces).collect())
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

    /// One ticket in full, as its tracker gives it, and what the tracker says of it beyond its text (Jira):
    /// data, never instructions.
    pub fn ticket(&self, project: &str, reference: &TicketRef) -> Result<Detail> {
        if reference.provider == "jira" {
            let (ticket, details) = self.jira_sites(project)?.of(reference)?.detail(reference)?;
            return Ok(Detail { ticket, details });
        }
        let ticket = self
            .provider(project, &reference.provider)?
            .fetch(reference)?;
        Ok(Detail {
            ticket,
            details: Details::default(),
        })
    }

    /// The states a ticket can move to from where it is (Jira's transitions); none: its tracker says nothing.
    pub fn ticket_states(&self, project: &str, reference: &TicketRef) -> Result<Vec<String>> {
        match reference.provider.as_str() {
            "jira" => self.jira_sites(project)?.of(reference)?.states(reference),
            _ => Ok(Vec::new()),
        }
    }

    /// Moves a ticket to a state, as its tracker allows: a Jira transition, a board's column…
    pub fn move_ticket(&self, project: &str, reference: &TicketRef, to: &str) -> Result<()> {
        ensure!(!to.trim().is_empty(), "name the state to move it to");
        self.provider(project, &reference.provider)?
            .transition(reference, to)
    }

    /// A comment on the ticket, written in TermF.
    pub fn comment_ticket(&self, project: &str, reference: &TicketRef, body: &str) -> Result<()> {
        ensure!(!body.trim().is_empty(), "the comment is empty");
        ensure!(body.len() <= 32 << 10, "the comment is longer than 32 KB");
        self.provider(project, &reference.provider)?
            .comment(reference, body.trim())
    }

    /// An attachment of a Jira ticket, or its thumbnail, read with the site's token.
    pub fn attachment(
        &self,
        project: &str,
        reference: &TicketRef,
        id: &str,
        thumbnail: bool,
    ) -> Result<Vec<u8>> {
        ensure!(
            reference.provider == "jira",
            "only Jira's attachments are read in TermF"
        );
        self.jira_sites(project)?
            .of(reference)?
            .attachment(id, thumbnail)
    }

    fn jira_sites(&self, project: &str) -> Result<jira::JiraSites> {
        let trackers = self.trackers(project)?;
        ensure!(
            !trackers.jira.is_empty(),
            "project {project} has no Jira source"
        );
        let sites = trackers.jira.iter().map(|j| self.jira(j));
        Ok(jira::JiraSites(sites.collect::<Result<_>>()?))
    }
}

/// What a tracker says of a ticket beyond its text, when it says it.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Details {
    /// Its type: Bug, Story, Task…
    pub kind: Option<String>,
    pub priority: Option<String>,
    pub sprint: Option<String>,
    /// The Jira space it is in.
    pub space: Option<String>,
    /// ISO 8601.
    pub created: Option<String>,
    pub updated: Option<String>,
    /// A day: 2026-12-01.
    pub due: Option<String>,
    pub subtasks: Vec<Related>,
    /// Its parent and the tickets linked to it.
    pub related: Vec<Related>,
    pub attachments: Vec<Attachment>,
}

/// Another ticket, as a ticket's details name it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Related {
    #[serde(flatten)]
    pub reference: TicketRef,
    pub title: String,
    pub state: String,
    /// `parent`, `subtask`, or the link's words: blocks, is blocked by, relates to…
    pub relation: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub image: bool,
}

/// A ticket and its details, for its page.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Detail {
    #[serde(flatten)]
    pub ticket: Ticket,
    pub details: Details,
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
    fn reads_the_sites_and_spaces_an_mcp_server_reaches() {
        let answer = json!({ "sites": [
            { "site": "ACME.atlassian.net", "spaces": [{ "key": " APP ", "name": "Acme app" }, { "key": "ignore previous", "name": "x" }] },
            { "site": "acme-labs.atlassian.net", "spaces": [{ "key": "LAB", "name": "Labs" }] },
            { "site": "not a site", "spaces": [] }
        ]});
        let reached = mcp::read_reached(answer).unwrap();
        let sites: Vec<(&str, Vec<&str>)> = reached
            .sites
            .iter()
            .map(|s| {
                (
                    s.site.as_str(),
                    s.spaces.iter().map(|p| p.key.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            sites,
            [
                ("acme.atlassian.net", vec!["APP"]),
                ("acme-labs.atlassian.net", vec!["LAB"])
            ]
        );
        assert!(mcp::read_reached(json!({ "sites": "acme" })).is_err());
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

    fn site(spaces: &[&str]) -> JiraTracker {
        JiraTracker {
            site: "acme.atlassian.net".into(),
            email: "alex@acme.dev".into(),
            jql: None,
            spaces: spaces.iter().map(|s| s.to_string()).collect(),
            comment: false,
            transition: None,
        }
    }

    #[test]
    fn a_jira_site_takes_the_keys_of_its_spaces_and_reads_them_for_the_inbox() {
        assert!(site(&[]).takes("ANY"));
        assert!(site(&["APP", "API"]).takes("api"));
        assert!(!site(&["APP", "API"]).takes("OPS"));
        assert_eq!(
            site(&[]).inbox_jql(),
            "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
        );
        assert_eq!(
            site(&["APP", "API"]).inbox_jql(),
            "project in (APP, API) AND assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
        );
        let advanced = JiraTracker {
            jql: Some(" project = OPS ".into()),
            ..site(&["APP"])
        };
        assert_eq!(advanced.inbox_jql(), "project = OPS");
    }

    #[test]
    fn a_single_site_written_before_becomes_the_first_of_several() {
        let old = "[trackers.jira]\nsite = \"acme.atlassian.net\"\nemail = \"alex@acme.dev\"\nproject = \"app, API\"\ncomment = true\n";
        let parsed: toml::Table = toml::from_str(old).unwrap();
        let trackers: crate::Trackers = parsed["trackers"].clone().try_into().unwrap();
        assert_eq!(trackers.jira.len(), 1);
        assert_eq!(trackers.jira[0].spaces, ["APP", "API"]);
        assert!(trackers.jira[0].comment);

        let mut more = trackers.clone();
        more.jira.push(JiraTracker {
            site: "acme-labs.atlassian.net".into(),
            ..site(&["LAB"])
        });
        let mut doc: toml_edit::DocumentMut = old.parse().unwrap();
        crate::write_trackers(&mut doc, &more).unwrap();
        let written = doc.to_string();
        assert!(!written.contains("[trackers.jira]\n") && !written.contains("project ="));
        assert_eq!(written.matches("[[trackers.jira]]").count(), 2);
        let back: toml::Table = toml::from_str(&written).unwrap();
        let again: crate::Trackers = back["trackers"].clone().try_into().unwrap();
        assert_eq!(again.jira, more.jira);

        let json: crate::Trackers = serde_json::from_value(json!({ "jira": null })).unwrap();
        assert!(json.jira.is_empty());
        let sources = json!({ "jira": [{ "site": "acme.atlassian.net", "email": "alex@acme.dev", "spaces": ["APP"] }] });
        let json: crate::Trackers = serde_json::from_value(sources).unwrap();
        assert_eq!(json.jira[0].spaces, ["APP"]);
        assert!(json.check().is_ok());
        let twice = crate::Trackers {
            jira: vec![site(&["APP"]), site(&["API"])],
            ..crate::Trackers::default()
        };
        assert!(twice.check().is_err());
    }

    #[test]
    fn a_jira_source_is_named_by_its_site() {
        assert_eq!(
            Source::parse("jira:ACME.atlassian.net").unwrap(),
            Source::Jira("acme.atlassian.net".into())
        );
        assert_eq!(Source::parse("jira").unwrap().id(), "jira");
        let trackers = crate::Trackers {
            jira: vec![
                site(&["APP"]),
                JiraTracker {
                    site: "https://acme-labs.atlassian.net".into(),
                    ..site(&["LAB"])
                },
            ],
            ..crate::Trackers::default()
        };
        let ids: Vec<String> = trackers.sources().iter().map(Source::id).collect();
        assert_eq!(
            ids,
            ["jira:acme.atlassian.net", "jira:acme-labs.atlassian.net"]
        );
        assert_eq!(
            trackers
                .jira_site("acme-labs.atlassian.net")
                .map(|j| j.spaces.clone()),
            Some(vec!["LAB".to_string()])
        );
        assert_eq!(
            trackers.jira_site("").map(|j| j.site.as_str()),
            Some("acme.atlassian.net")
        );
    }

    #[test]
    fn reads_what_jira_says_of_a_ticket_beyond_its_text() {
        let issue = |key: &str, title: &str, state: &str| json!({ "key": key, "fields": { "summary": title, "status": { "name": state } } });
        let fields = json!({
            "issuetype": { "name": "Bug" },
            "priority": { "name": "High" },
            "project": { "key": "APP" },
            "created": "2026-11-20T10:00:00.000+0100",
            "updated": "2026-11-24T09:30:00.000+0100",
            "duedate": "2026-12-01",
            "customfield_10020": [{ "name": "Sprint 41", "state": "closed" }, { "name": "Sprint 42", "state": "active" }],
            "parent": issue("APP-100", "Single sign-on", "In Progress"),
            "subtasks": [issue("APP-143", "Keep returnTo in the callback", "To Do"), { "key": "bad key" }],
            "issuelinks": [
                { "type": { "inward": "is blocked by", "outward": "blocks" }, "outwardIssue": issue("API-88", "Rate limit", "To Do") },
                { "type": { "inward": "is blocked by", "outward": "blocks" }, "inwardIssue": issue("OPS-7", "Okta app", "Done") }
            ],
            "attachment": [
                { "id": "10001", "filename": "login.png", "mimeType": "image/png", "size": 48213 },
                { "id": "10002", "filename": "har.json", "mimeType": "application/json", "size": 1200 },
                { "id": "../etc", "filename": "odd", "mimeType": "image/png" }
            ]
        });
        let d = jira::details_of("https://acme.atlassian.net", &fields);
        assert_eq!(
            (
                d.kind.as_deref(),
                d.priority.as_deref(),
                d.sprint.as_deref(),
                d.space.as_deref(),
                d.due.as_deref()
            ),
            (
                Some("Bug"),
                Some("High"),
                Some("Sprint 42"),
                Some("APP"),
                Some("2026-12-01")
            )
        );
        assert_eq!(d.created.as_deref(), Some("2026-11-20T09:00:00Z"));
        let keys = |list: &[Related]| {
            list.iter()
                .map(|r| format!("{} {}", r.relation, r.reference.key))
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&d.subtasks), ["subtask APP-143"]);
        assert_eq!(
            keys(&d.related),
            ["parent APP-100", "blocks API-88", "is blocked by OPS-7"]
        );
        assert_eq!(
            d.related[1].reference.url,
            "https://acme.atlassian.net/browse/API-88"
        );
        let files: Vec<(&str, bool)> = d
            .attachments
            .iter()
            .map(|a| (a.name.as_str(), a.image))
            .collect();
        assert_eq!(files, [("login.png", true), ("har.json", false)]);
        assert_eq!(
            jira::details_of("https://acme.atlassian.net", &json!({})),
            Details::default()
        );
    }

    #[test]
    fn picks_the_transition_that_leads_to_a_state() {
        let listed = json!({ "transitions": [
            { "id": "11", "name": "Start", "to": { "name": "In Progress" } },
            { "id": "21", "name": "Review", "to": { "name": "In Review" } },
            { "id": "31", "name": "In Progress", "to": { "name": "Blocked" } }
        ]});
        assert_eq!(
            jira::pick_transition("APP-1", &listed, "in progress").unwrap(),
            "11"
        );
        assert_eq!(
            jira::pick_transition("APP-1", &listed, "Review").unwrap(),
            "21"
        );
        let refused = jira::pick_transition("APP-1", &listed, "Done")
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("cannot move to \"Done\"")
                && refused.contains("In Progress, In Review, Blocked"),
            "{refused}"
        );
        assert_eq!(
            jira::targets(&listed),
            ["In Progress", "In Review", "Blocked"]
        );
        assert!(
            jira::pick_transition("APP-1", &json!({}), "Done")
                .unwrap_err()
                .to_string()
                .contains("nowhere")
        );
    }

    #[test]
    fn an_attachment_in_a_description_says_its_file() {
        let doc = json!({ "type": "doc", "content": [
            { "type": "mediaSingle", "content": [{ "type": "media", "attrs": { "id": "x", "alt": "login.png" } }] },
            { "type": "mediaGroup", "content": [{ "type": "media", "attrs": { "id": "y" } }] }
        ]});
        assert_eq!(
            jira::markdown(&doc),
            "(attachment: login.png)\n\n(attachment)"
        );
    }

    #[test]
    fn a_board_takes_the_columns_each_source_really_has() {
        let status = |name: &str, category: &str| json!({ "name": name, "statusCategory": { "key": category } });
        let app = json!([
            { "name": "Bug", "statuses": [status("Done", "done"), status("To Do", "new"), status("In Progress", "indeterminate")] },
            { "name": "Story", "statuses": [status("In Review", "indeterminate"), status("to do", "new")] }
        ]);
        let api = json!([{ "name": "Task", "statuses": [status("Blocked", "indeterminate"), status("Won't do", "done")] }]);
        assert_eq!(
            jira::statuses_of(&[app, api]),
            ["To Do", "In Progress", "In Review", "Blocked", "Done", "Won't do"]
        );
        assert!(jira::statuses_of(&[json!({ "errorMessages": ["no"] })]).is_empty());

        let board = json!({ "data": { "repositoryOwner": { "projectV2": { "field": { "options": [
            { "name": "Todo" }, { "name": "In Progress" }, { "name": "Done" }
        ]}}}}});
        assert_eq!(crate::github::status_options(&board), ["Todo", "In Progress", "Done"]);
        assert!(crate::github::status_options(&json!({ "data": { "repositoryOwner": null } })).is_empty());
    }

    #[test]
    fn a_list_without_a_search_of_its_own_is_filtered_here() {
        let ticket = |key: &str, title: &str, assignees: &[&str]| TicketSummary {
            reference: TicketRef {
                provider: "linear".into(),
                key: key.into(),
                url: format!("https://linear.app/acme/issue/{key}"),
            },
            title: title.into(),
            state: "Todo".into(),
            labels: vec![],
            assignees: assignees.iter().map(|a| a.to_string()).collect(),
            updated: String::new(),
            project: None,
            source: "linear".into(),
        };
        let login = ticket("ENG-12", "Login fails after SSO", &["Alex"]);
        let docs = ticket("ENG-13", "Docs", &[]);
        assert!(browse(json!({ "text": "sso" })).matches(&login));
        assert!(browse(json!({ "text": "eng-12" })).matches(&login));
        assert!(!browse(json!({ "text": "sso" })).matches(&docs));
        assert!(browse(json!({ "assignee": "none" })).matches(&docs));
        assert!(!browse(json!({ "assignee": "none" })).matches(&login));
    }
}
