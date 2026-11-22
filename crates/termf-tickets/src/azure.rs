//! Azure DevOps Boards through its REST API, with curl and a personal access token kept in the system's
//! keychain. Work items hold HTML, which becomes plain Markdown.

use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use crate::http::{Answers, Body, Http, encode};
use crate::store::iso_utc;
use crate::{Comment, Ticket, TicketProvider, TicketRef, TicketSummary};

/// The inbox's WIQL when the project names none: the work items of the project assigned to the user, not done.
pub const DEFAULT_WIQL: &str = "SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project AND [System.AssignedTo] = @me AND [System.State] NOT IN ('Closed', 'Done', 'Removed', 'Resolved') ORDER BY [System.ChangedDate] DESC";

const VERSION: &str = "api-version=7.1";
const COMMENTS_VERSION: &str = "api-version=7.1-preview.4";
/// Work items the inbox reads at most; the batch API takes 200 at a time.
const MAX_ITEMS: usize = 1000;
const BATCH: usize = 200;

/// The organization's name, from `acme`, `dev.azure.com/acme` or `acme.visualstudio.com`.
fn name(organization: &str) -> Result<String> {
    let text = organization.trim().trim_end_matches('/');
    let text = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))
        .unwrap_or(text);
    let text = text
        .strip_prefix("dev.azure.com/")
        .or_else(|| text.strip_suffix(".visualstudio.com"))
        .unwrap_or(text);
    ensure!(
        !text.is_empty()
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
        "{organization:?} is not an Azure DevOps organization: write its name, acme for dev.azure.com/acme"
    );
    Ok(text.to_ascii_lowercase())
}

/// `https://dev.azure.com/acme`, or a local `http://127.0.0.1:<port>` kept as it is, for tests.
pub(crate) fn base(organization: &str) -> Result<String> {
    let site = organization.trim().trim_end_matches('/');
    if crate::http::local(site) {
        return Ok(site.to_owned());
    }
    Ok(format!("https://dev.azure.com/{}", name(site)?))
}

/// The keychain account of a personal access token: one per organization.
pub(crate) fn account(organization: &str) -> Result<String> {
    let base = base(organization)?;
    let name = base.rsplit_once("dev.azure.com/").map_or(base.as_str(), |(_, n)| n);
    Ok(format!("azure:{name}"))
}

pub(crate) fn reference(organization: &str, project: &str, id: u64) -> Result<TicketRef> {
    Ok(TicketRef {
        provider: "azure".into(),
        key: format!("AB#{id}"),
        url: format!(
            "{}/{}/_workitems/edit/{id}",
            base(organization)?,
            encode(project.trim())
        ),
    })
}

/// The number of `AB#123`.
pub(crate) fn number(key: &str) -> Option<u64> {
    key.trim()
        .strip_prefix("AB#")
        .and_then(crate::github::number)
}

/// The organization's address (`https://dev.azure.com/acme`) and the number of a pasted work item: its page
/// (`…/_workitems/edit/123`, or any page whose address says `workitem=123`), or `AB#123`, which names no
/// organization.
pub(crate) fn parse(input: &str) -> Option<(Option<String>, u64)> {
    let input = input.trim();
    if let Some(id) = number(input) {
        return Some((None, id));
    }
    let (scheme, rest) = input.split_once("://")?;
    if !matches!(scheme, "https" | "http") {
        return None;
    }
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let site = if host == "dev.azure.com" && !parts.is_empty() {
        format!("https://dev.azure.com/{}", parts.remove(0).to_ascii_lowercase())
    } else if let Some(org) = host.strip_suffix(".visualstudio.com") {
        format!("https://dev.azure.com/{org}")
    } else if crate::http::local(&format!("{scheme}://{host}")) {
        format!("{scheme}://{host}")
    } else {
        return None;
    };
    let edited = match parts.as_slice() {
        [_, "_workitems", "edit", id, ..] => crate::github::number(id),
        _ => None,
    };
    let asked = query
        .split(['&', '#'])
        .find_map(|pair| pair.strip_prefix("workitem="))
        .and_then(crate::github::number);
    Some((Some(site), edited.or(asked)?))
}

/// The text of an HTML fragment, its links kept as Markdown links.
pub(crate) fn text(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    let mut link: Option<String> = None;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = &rest[open..];
            break;
        };
        let tag = &rest[open + 1..open + close];
        rest = &rest[open + close + 1..];
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        let closing = tag.starts_with('/');
        match (name.as_str(), closing) {
            ("br", _) => out.push('\n'),
            ("p" | "div" | "tr" | "ul" | "ol" | "table", true) => out.push_str("\n\n"),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                let level = name[1..].parse().unwrap_or(1);
                out.push_str(&format!("\n\n{} ", "#".repeat(level)));
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => out.push_str("\n\n"),
            ("li", false) => out.push_str("\n- "),
            ("td" | "th", true) => out.push_str(" | "),
            ("a", false) => {
                link = tag.split(['"', '\'']).nth(1).map(str::to_owned);
                out.push('[');
            }
            ("a", true) => match link.take() {
                Some(href) => out.push_str(&format!("]({href})")),
                None => out.push(']'),
            },
            ("img", _) => out.push_str("(image)"),
            _ => {}
        }
    }
    out.push_str(rest);
    let decoded = entities(&out);
    let lines: Vec<&str> = decoded.lines().map(str::trim_end).collect();
    let mut text = lines.join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.trim().to_owned()
}

fn entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.find(';').filter(|e| *e <= 10);
        let decoded = end.and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => ' ',
                _ => {
                    let code = entity.strip_prefix('#')?;
                    let code = match code.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => code.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Plain text as HTML, which Azure DevOps comments hold.
fn html(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    escaped.replace('\n', "<br>")
}

fn display(identity: &Value) -> Option<String> {
    identity["displayName"].as_str().map(str::to_owned)
}

fn tags(fields: &Value) -> Vec<String> {
    let tags = fields["System.Tags"].as_str().unwrap_or_default();
    tags.split(';')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

pub struct Azure {
    http: Http,
    /// What `organization` says, for the tickets' addresses.
    organization: String,
    project: String,
}

impl Azure {
    pub(crate) fn new(
        organization: &str,
        project: &str,
        token: String,
        curl: PathBuf,
        answers: Answers,
    ) -> Result<Self> {
        let project = project.trim();
        ensure!(!project.is_empty(), "Azure DevOps: name the project");
        Ok(Self {
            http: Http {
                base: base(organization)?,
                name: "Azure DevOps",
                auth: vec![("user", format!(":{token}"))],
                refused: format!(
                    "Azure DevOps refused the personal access token of {}: check it (Work Items, read and write), or save a new one in the project's Tickets section",
                    organization.trim()
                ),
                curl,
                answers,
            },
            organization: organization.trim().to_owned(),
            project: project.to_owned(),
        })
    }

    /// The work items `wiql` finds, in its order, at most `limit`.
    pub(crate) fn issues(&self, wiql: &str, limit: Option<usize>, fresh: bool) -> Result<Vec<TicketSummary>> {
        let top = limit.unwrap_or(MAX_ITEMS).min(MAX_ITEMS);
        let path = format!(
            "/{}/_apis/wit/wiql?{VERSION}&$top={top}",
            encode(&self.project)
        );
        let query = json!({ "query": wiql });
        let found = self.http.read("POST", &path, Some(Body::json(&query)), fresh)?;
        let ids: Vec<u64> = found["workItems"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|w| w["id"].as_u64())
            .take(top)
            .collect();
        let mut tickets = Vec::new();
        for ids in ids.chunks(BATCH) {
            let asked = json!({
                "ids": ids,
                "fields": ["System.Id", "System.Title", "System.State", "System.Tags", "System.AssignedTo", "System.ChangedDate", "System.TeamProject"],
                "errorPolicy": "Omit",
            });
            let path = format!("/_apis/wit/workitemsbatch?{VERSION}");
            let batch = self.http.read("POST", &path, Some(Body::json(&asked)), fresh)?;
            for item in batch["value"].as_array().into_iter().flatten() {
                let fields = &item["fields"];
                let Some(id) = item["id"].as_u64() else {
                    continue;
                };
                let project = fields["System.TeamProject"].as_str().unwrap_or(&self.project);
                tickets.push(TicketSummary {
                    reference: reference(&self.organization, project, id)?,
                    title: fields["System.Title"].as_str().unwrap_or_default().into(),
                    state: fields["System.State"].as_str().unwrap_or_default().into(),
                    labels: tags(fields),
                    assignees: display(&fields["System.AssignedTo"]).into_iter().collect(),
                    updated: iso_utc(fields["System.ChangedDate"].as_str().unwrap_or_default()),
                    project: None,
                    source: "azure".into(),
                });
            }
        }
        Ok(tickets)
    }

    fn id(ticket: &TicketRef) -> Result<u64> {
        number(&ticket.key).with_context(|| format!("not an Azure DevOps work item: {}", ticket.key))
    }

    fn item(&self, id: u64, relations: bool) -> Result<Value> {
        let expand = if relations { "&$expand=relations" } else { "" };
        let path = format!("/_apis/wit/workitems/{id}?{VERSION}{expand}");
        self.http.call("GET", &path, None)
    }

    fn patch(&self, id: u64, operations: Value) -> Result<()> {
        let path = format!("/_apis/wit/workitems/{id}?{VERSION}");
        let body = Body {
            kind: "application/json-patch+json",
            value: &operations,
        };
        self.http.call("PATCH", &path, Some(body))?;
        Ok(())
    }
}

impl TicketProvider for Azure {
    fn resolve(&self, input: &str) -> Option<TicketRef> {
        let (site, id) = parse(input)?;
        if site.is_some_and(|s| Some(s) != base(&self.organization).ok()) {
            return None;
        }
        reference(&self.organization, &self.project, id).ok()
    }

    fn fetch(&self, ticket: &TicketRef) -> Result<Ticket> {
        let id = Self::id(ticket)?;
        let item = self.item(id, false)?;
        let fields = &item["fields"];
        let project = fields["System.TeamProject"].as_str().unwrap_or(&self.project);
        let path = format!(
            "/{}/_apis/wit/workItems/{id}/comments?{COMMENTS_VERSION}&$top=200&order=asc",
            encode(project)
        );
        let listed = self.http.call("GET", &path, None)?;
        let comments: Vec<Comment> = listed["comments"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| Comment {
                author: display(&c["createdBy"]),
                created: iso_utc(c["createdDate"].as_str().unwrap_or_default()),
                body: text(c["text"].as_str().unwrap_or_default()),
            })
            .collect();
        let mut body = String::new();
        for (field, title) in [
            ("System.Description", ""),
            ("Microsoft.VSTS.TCM.ReproSteps", "Repro steps"),
            ("Microsoft.VSTS.Common.AcceptanceCriteria", "Acceptance criteria"),
        ] {
            let part = text(fields[field].as_str().unwrap_or_default());
            if part.is_empty() {
                continue;
            }
            if !title.is_empty() {
                body.push_str(&format!("\n\n### {title}\n\n"));
            }
            body.push_str(&part);
        }
        let body = body.trim().to_owned();
        let texts = std::iter::once(body.as_str()).chain(comments.iter().map(|c| c.body.as_str()));
        Ok(Ticket {
            links: crate::links(texts, &ticket.url),
            title: fields["System.Title"].as_str().unwrap_or_default().into(),
            state: fields["System.State"].as_str().unwrap_or_default().into(),
            author: display(&fields["System.CreatedBy"]),
            labels: tags(fields),
            assignees: display(&fields["System.AssignedTo"]).into_iter().collect(),
            body,
            comments,
            reference: ticket.clone(),
        })
    }

    /// `query` is WIQL.
    fn search(&self, query: &str) -> Result<Vec<TicketSummary>> {
        self.issues(query, None, false)
    }

    fn comment(&self, ticket: &TicketRef, body: &str) -> Result<()> {
        let id = Self::id(ticket)?;
        let item = self.item(id, false)?;
        let project = item["fields"]["System.TeamProject"].as_str().unwrap_or(&self.project);
        let path = format!(
            "/{}/_apis/wit/workItems/{id}/comments?{COMMENTS_VERSION}",
            encode(project)
        );
        let text = json!({ "text": html(body) });
        self.http.call("POST", &path, Some(Body::json(&text)))?;
        Ok(())
    }

    /// `to` is a state of the work item's type, as Azure DevOps spells it.
    fn transition(&self, ticket: &TicketRef, to: &str) -> Result<()> {
        let id = Self::id(ticket)?;
        let to = to.trim();
        ensure!(!to.is_empty(), "name the state to move {} to", ticket.key);
        self.patch(
            id,
            json!([{ "op": "add", "path": "/fields/System.State", "value": to }]),
        )
    }

    /// A hyperlink on the work item, once: linking the same address again changes nothing.
    fn link_pr(&self, ticket: &TicketRef, url: &str) -> Result<()> {
        let id = Self::id(ticket)?;
        let item = self.item(id, true)?;
        let linked = item["relations"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|r| r["rel"] == "Hyperlink" && r["url"].as_str() == Some(url));
        if linked {
            return Ok(());
        }
        let link = json!({
            "rel": "Hyperlink",
            "url": url,
            "attributes": { "comment": crate::gitlab::request_title(url) },
        });
        self.patch(
            id,
            json!([{ "op": "add", "path": "/relations/-", "value": link }]),
        )
    }
}
