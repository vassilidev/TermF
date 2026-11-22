//! Linear through its GraphQL API, with curl and a personal API key kept in the system's keychain.

use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use crate::http::{Answers, Body, Http};
use crate::store::iso_utc;
use crate::{Comment, LinearTracker, Ticket, TicketProvider, TicketRef, TicketSummary};

/// Linear's API, which tests replace with a server of their own.
pub const API: &str = "https://api.linear.app";

/// Pages of 100 the inbox reads at most.
const MAX_PAGES: usize = 10;

const ISSUES: &str = "query($filter: IssueFilter, $first: Int, $after: String) {
  issues(filter: $filter, first: $first, after: $after, orderBy: updatedAt) {
    nodes { identifier title url updatedAt state { name } labels { nodes { name } } assignee { displayName } }
    pageInfo { hasNextPage endCursor }
  }
}";

// shortcut: the first 250 comments of a ticket, Linear's largest page; page through them if tickets ever hold more.
const ISSUE: &str = "query($id: String!) {
  issue(id: $id) {
    identifier title description url state { name } labels { nodes { name } }
    assignee { displayName } creator { displayName }
    comments(first: 250) { nodes { body createdAt user { displayName } } }
  }
}";

const STATES: &str = "query($id: String!) {
  issue(id: $id) { team { states(first: 100) { nodes { id name } } } }
}";

const COMMENT: &str = "mutation($id: String!, $body: String!) {
  commentCreate(input: { issueId: $id, body: $body }) { success }
}";

const MOVE: &str = "mutation($id: String!, $state: String!) {
  issueUpdate(id: $id, input: { stateId: $state }) { success }
}";

const LINK: &str = "mutation($id: String!, $url: String!, $title: String) {
  attachmentLinkURL(issueId: $id, url: $url, title: $title) { success }
}";

/// The workspace's URL key, as Linear's addresses show it: `acme` in `linear.app/acme/…`.
pub(crate) fn workspace(text: &str) -> Result<String> {
    let text = text.trim().to_ascii_lowercase();
    ensure!(
        !text.is_empty()
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "{text:?} is not a Linear workspace: write the name its addresses show, acme for linear.app/acme"
    );
    Ok(text)
}

/// The keychain account of a personal API key: one per workspace.
pub(crate) fn account(space: &str) -> Result<String> {
    Ok(format!("linear:{}", workspace(space)?))
}

pub(crate) fn reference(space: &str, key: &str) -> TicketRef {
    TicketRef {
        provider: "linear".into(),
        key: key.to_owned(),
        url: format!("https://linear.app/{space}/issue/{key}"),
    }
}

/// The key of a pasted Linear ticket, and the workspace of a pasted URL: `linear.app/<workspace>/issue/ENG-123/…`.
pub(crate) fn parse(input: &str) -> Option<(String, Option<String>)> {
    let input = input.trim();
    let Some(rest) = ["https://", "http://"]
        .iter()
        .find_map(|s| input.strip_prefix(s))
    else {
        return crate::jira::key(input).map(|k| (k, None));
    };
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let path = rest.strip_prefix("linear.app/")?;
    let path = path.split(['?', '#']).next().unwrap_or(path);
    match path.split('/').collect::<Vec<_>>().as_slice() {
        [space, "issue", key, ..] => {
            let key = crate::jira::key(key)?;
            Some((key, Some(workspace(space).ok()?)))
        }
        _ => None,
    }
}

/// An issue's address without the title Linear adds after its key.
fn address(url: &str, space: &str, key: &str) -> String {
    let own = format!("/issue/{key}");
    match url.to_ascii_uppercase().find(&own.to_ascii_uppercase()) {
        Some(at) if url.starts_with("https://linear.app/") => url[..at + own.len()].to_owned(),
        _ => reference(space, key).url,
    }
}

fn names(list: &Value, key: &str) -> Vec<String> {
    let items = list["nodes"].as_array().into_iter().flatten();
    items
        .filter_map(|v| v[key].as_str().map(str::to_owned))
        .collect()
}

fn name(user: &Value) -> Option<String> {
    user["displayName"].as_str().map(str::to_owned)
}

pub struct Linear {
    http: Http,
    workspace: String,
}

impl Linear {
    pub(crate) fn new(
        api: &str,
        space: &str,
        key: String,
        curl: PathBuf,
        answers: Answers,
    ) -> Result<Self> {
        let workspace = workspace(space)?;
        Ok(Self {
            http: Http {
                base: api.trim_end_matches('/').to_owned(),
                name: "Linear",
                auth: vec![("header", format!("Authorization: {key}"))],
                refused: format!(
                    "Linear refused the API key of {workspace}: check it, or save a new one in the project's Tickets section"
                ),
                curl,
                answers,
                scope: format!("linear:{workspace}"),
            },
            workspace,
        })
    }

    /// One GraphQL call; `fresh` is `None` for a write. Linear answers 200 with `errors` too.
    fn graphql(&self, query: &str, variables: Value, fresh: Option<bool>) -> Result<Value> {
        let body = json!({ "query": query, "variables": variables });
        let body = Body::json(&body);
        let answer = match fresh {
            Some(fresh) => self.http.read("POST", "/graphql", Some(body), fresh),
            None => self.http.call("POST", "/graphql", Some(body)),
        };
        let answer = answer.map_err(|e| {
            if format!("{e:#}")
                .to_ascii_lowercase()
                .contains("authenticat")
            {
                anyhow::anyhow!("{}", self.http.refused)
            } else {
                e
            }
        })?;
        if let Some(errors) = answer["errors"].as_array().filter(|e| !e.is_empty()) {
            let messages: Vec<&str> = errors
                .iter()
                .filter_map(|e| e["message"].as_str())
                .collect();
            let messages = messages.join("; ");
            if messages.to_ascii_lowercase().contains("authenticat") {
                bail!("{}", self.http.refused);
            }
            bail!("Linear: {messages}");
        }
        Ok(answer["data"].clone())
    }

    /// The tickets `tracker` asks for, most recently updated first, every page, at most `limit`.
    pub(crate) fn issues(
        &self,
        tracker: &LinearTracker,
        limit: Option<usize>,
        fresh: bool,
    ) -> Result<Vec<TicketSummary>> {
        let mut filter = json!({});
        if tracker.mine {
            filter["assignee"] = json!({ "isMe": { "eq": true } });
        }
        if let Some(team) = tracker.team() {
            filter["team"] = json!({ "key": { "eqIgnoreCase": team } });
        }
        let states: Vec<&str> = tracker
            .states
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        filter["state"] = match states.as_slice() {
            [] => json!({ "type": { "nin": ["completed", "canceled"] } }),
            states => json!({ "name": { "in": states } }),
        };
        let mut found = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let first = limit.unwrap_or(100).min(100);
            let variables = json!({ "filter": filter, "first": first, "after": after });
            let data = self.graphql(ISSUES, variables, Some(fresh))?;
            let issues = &data["issues"];
            for issue in issues["nodes"].as_array().into_iter().flatten() {
                let Some(key) = issue["identifier"].as_str().and_then(crate::jira::key) else {
                    continue;
                };
                found.push(TicketSummary {
                    reference: TicketRef {
                        url: address(
                            issue["url"].as_str().unwrap_or_default(),
                            &self.workspace,
                            &key,
                        ),
                        provider: "linear".into(),
                        key,
                    },
                    title: issue["title"].as_str().unwrap_or_default().into(),
                    state: issue["state"]["name"].as_str().unwrap_or_default().into(),
                    labels: names(&issue["labels"], "name"),
                    assignees: name(&issue["assignee"]).into_iter().collect(),
                    updated: iso_utc(issue["updatedAt"].as_str().unwrap_or_default()),
                    project: None,
                    source: "linear".into(),
                });
            }
            if let Some(limit) = limit
                && found.len() >= limit
            {
                found.truncate(limit);
                break;
            }
            let page = &issues["pageInfo"];
            match page["endCursor"].as_str() {
                Some(next) if page["hasNextPage"] == true => after = Some(next.to_owned()),
                _ => break,
            }
        }
        Ok(found)
    }

    fn key(ticket: &TicketRef) -> Result<String> {
        crate::jira::key(&ticket.key).with_context(|| format!("not a Linear key: {}", ticket.key))
    }
}

impl TicketProvider for Linear {
    fn resolve(&self, input: &str) -> Option<TicketRef> {
        let (key, space) = parse(input)?;
        if space.is_some_and(|s| s != self.workspace) {
            return None;
        }
        Some(reference(&self.workspace, &key))
    }

    fn fetch(&self, ticket: &TicketRef) -> Result<Ticket> {
        let key = Self::key(ticket)?;
        let data = self.graphql(ISSUE, json!({ "id": key }), Some(true))?;
        let issue = &data["issue"];
        ensure!(issue.is_object(), "Linear has no ticket {key}");
        let mut comments: Vec<Comment> = issue["comments"]["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| Comment {
                author: name(&c["user"]),
                created: iso_utc(c["createdAt"].as_str().unwrap_or_default()),
                body: c["body"].as_str().unwrap_or_default().to_owned(),
            })
            .collect();
        comments.sort_by(|a, b| a.created.cmp(&b.created));
        let body = issue["description"].as_str().unwrap_or_default().to_owned();
        let texts = std::iter::once(body.as_str()).chain(comments.iter().map(|c| c.body.as_str()));
        Ok(Ticket {
            links: crate::links(texts, &ticket.url),
            title: issue["title"].as_str().unwrap_or_default().into(),
            state: issue["state"]["name"].as_str().unwrap_or_default().into(),
            author: name(&issue["creator"]),
            labels: names(&issue["labels"], "name"),
            assignees: name(&issue["assignee"]).into_iter().collect(),
            body,
            comments,
            reference: ticket.clone(),
        })
    }

    /// `query` names a team key, or nothing: the tickets assigned to the user that are not done.
    fn search(&self, query: &str) -> Result<Vec<TicketSummary>> {
        let tracker = LinearTracker {
            workspace: self.workspace.clone(),
            team: Some(query.to_owned()),
            mine: true,
            states: Vec::new(),
            comment: false,
            transition: None,
        };
        self.issues(&tracker, None, false)
    }

    fn comment(&self, ticket: &TicketRef, body: &str) -> Result<()> {
        let key = Self::key(ticket)?;
        let data = self.graphql(COMMENT, json!({ "id": key, "body": body }), None)?;
        ensure!(
            data["commentCreate"]["success"] == true,
            "Linear did not add the comment to {key}"
        );
        Ok(())
    }

    /// `to` names a state of the ticket's team, in any case.
    fn transition(&self, ticket: &TicketRef, to: &str) -> Result<()> {
        let key = Self::key(ticket)?;
        let data = self.graphql(STATES, json!({ "id": key }), Some(true))?;
        let states = data["issue"]["team"]["states"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let Some(state) = states
            .iter()
            .find(|s| {
                s["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(to.trim()))
            })
            .and_then(|s| s["id"].as_str())
        else {
            let names: Vec<&str> = states.iter().filter_map(|s| s["name"].as_str()).collect();
            bail!(
                "the team of {key} has no state {to:?}: it has {}",
                names.join(", ")
            );
        };
        let data = self.graphql(MOVE, json!({ "id": key, "state": state }), None)?;
        ensure!(
            data["issueUpdate"]["success"] == true,
            "Linear did not move {key} to {to}"
        );
        Ok(())
    }

    /// A link on the ticket; Linear keeps one per address, so linking again changes nothing.
    fn link_pr(&self, ticket: &TicketRef, url: &str) -> Result<()> {
        let key = Self::key(ticket)?;
        let title = crate::gitlab::request_title(url);
        let data = self.graphql(LINK, json!({ "id": key, "url": url, "title": title }), None)?;
        ensure!(
            data["attachmentLinkURL"]["success"] == true,
            "Linear did not link {url} to {key}"
        );
        Ok(())
    }
}
