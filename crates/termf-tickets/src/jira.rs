//! Jira Cloud through its REST API v3, with `curl`: no HTTP or TLS stack built into TermF, the system's
//! certificates and proxy, and the API token handed to curl on its standard input, never on a command line.

use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use crate::store::{iso_utc, utc};
use crate::{Comment, Ticket, TicketProvider, TicketRef, TicketSummary};

/// The inbox's JQL when the project names none.
pub const DEFAULT_JQL: &str =
    "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";

/// Pages of 100 the inbox reads at most.
const MAX_PAGES: usize = 20;

pub struct Jira {
    /// `https://acme.atlassian.net`.
    base: String,
    email: String,
    token: String,
    curl: PathBuf,
}

/// `https://acme.atlassian.net` from what the user typed (`acme.atlassian.net`, its URL…). A local
/// `http://127.0.0.1:<port>` is kept as it is, for tests: nowhere else does the token travel unencrypted.
pub(crate) fn base(site: &str) -> Result<String> {
    let site = site.trim().trim_end_matches('/');
    if ["http://127.0.0.1:", "http://localhost:"]
        .iter()
        .any(|local| site.starts_with(local))
    {
        return Ok(site.to_owned());
    }
    let host = site.strip_prefix("https://").unwrap_or(site);
    ensure!(
        !host.is_empty()
            && host.contains('.')
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-.".contains(&b)),
        "{site:?} is not a Jira site: write it as acme.atlassian.net"
    );
    Ok(format!("https://{}", host.to_ascii_lowercase()))
}

/// `acme.atlassian.net`, as a ticket's URL names it.
pub(crate) fn host(site: &str) -> Result<String> {
    let base = base(site)?;
    Ok(base
        .split_once("://")
        .map_or(base.clone(), |(_, h)| h.to_owned()))
}

/// The keychain account of an API token: one per site and e-mail.
pub(crate) fn account(site: &str, email: &str) -> Result<String> {
    Ok(format!(
        "jira:{}:{}",
        host(site)?,
        email.trim().to_lowercase()
    ))
}

pub(crate) fn reference(site: &str, key: &str) -> Result<TicketRef> {
    Ok(TicketRef {
        provider: "jira".into(),
        key: key.to_owned(),
        url: format!("{}/browse/{key}", base(site)?),
    })
}

/// `ACME-123` (any case), uppercased.
pub(crate) fn key(text: &str) -> Option<String> {
    let (project, number) = text.rsplit_once('-')?;
    let mut letters = project.chars();
    let valid = letters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && letters.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit());
    valid.then(|| text.to_ascii_uppercase())
}

/// The key of a pasted Jira ticket, and the site of a pasted URL: `…/browse/ACME-123`, or any page whose
/// address says `selectedIssue=ACME-123` (boards, filters).
pub(crate) fn parse(input: &str) -> Option<(String, Option<String>)> {
    let input = input.trim();
    let Some((_, rest)) = input
        .split_once("://")
        .filter(|(scheme, _)| matches!(*scheme, "https" | "http"))
    else {
        return key(input).map(|k| (k, None));
    };
    let rest = rest.split('#').next().unwrap_or(rest);
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let browsed = path
        .strip_prefix("browse/")
        .map(|k| k.trim_end_matches('/'));
    let selected = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("selectedIssue="));
    let found = browsed.and_then(key).or_else(|| selected.and_then(key))?;
    Some((found, Some(host.to_ascii_lowercase())))
}

/// Quotes a value for curl's config syntax.
pub(crate) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What Jira says went wrong, from its error body.
fn reason(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return crate::truncate(body.trim(), 300);
    };
    let mut reasons: Vec<String> = value["errorMessages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str().map(str::to_owned))
        .collect();
    if let Some(errors) = value["errors"].as_object() {
        reasons.extend(
            errors
                .iter()
                .map(|(k, v)| format!("{k}: {}", v.as_str().unwrap_or_default())),
        );
    }
    if reasons.is_empty() {
        crate::truncate(body.trim(), 300)
    } else {
        reasons.join("; ")
    }
}

impl Jira {
    pub fn new(site: &str, email: &str, token: String, curl: PathBuf) -> Result<Self> {
        Ok(Self {
            base: base(site)?,
            email: email.trim().to_owned(),
            token,
            curl,
        })
    }

    /// One call to the API. The token goes to curl on its standard input; `--disable` keeps the user's
    /// `.curlrc` (which could trace the request to a file) out of it.
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let mut config = String::new();
        let mut set = |key: &str, value: &str| {
            config.push_str(&format!("{key} = {}\n", quoted(value)));
        };
        set("url", &format!("{}{path}", self.base));
        set("request", method);
        set("user", &format!("{}:{}", self.email, self.token));
        set("header", "Accept: application/json");
        set("write-out", "\n%{http_code}");
        set("max-time", "60");
        if let Some(body) = body {
            set("header", "Content-Type: application/json");
            set("data-raw", &body.to_string());
        }
        config.push_str("silent\nshow-error\n");
        let mut child = crate::command(&self.curl)
            .args(["--disable", "--config", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("cannot run curl, which TermF needs to reach Jira")?;
        // Same as gh: a curl that exits early says why on stderr, which a broken pipe would hide.
        if let Some(mut stdin) = child.stdin.take()
            && let Err(e) = stdin.write_all(config.as_bytes())
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(e.into());
        }
        let out = child.wait_with_output()?;
        let text = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            bail!(
                "cannot reach {}: {}",
                self.base,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let (body, status) = text.rsplit_once('\n').unwrap_or(("", &text));
        let status: u16 = status.trim().parse().unwrap_or(0);
        match status {
            200..=299 if body.trim().is_empty() => Ok(Value::Null),
            200..=299 => serde_json::from_str(body)
                .with_context(|| format!("{} gave no JSON for {path}", self.base)),
            401 | 403 => bail!(
                "Jira refused the e-mail and API token ({status}): check them, or save a new token. {}",
                reason(body)
            ),
            404 => bail!("not found on {} (404): {}", self.base, reason(body)),
            _ => bail!("Jira answered {status}: {}", reason(body)),
        }
    }

    /// The tickets `jql` finds, every page (Jira's `nextPageToken`), at most `limit`.
    pub(crate) fn issues(&self, jql: &str, limit: Option<usize>) -> Result<Vec<TicketSummary>> {
        let mut found = Vec::new();
        let mut next: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut body = json!({
                "jql": jql,
                "fields": ["summary", "status", "labels", "assignee", "updated"],
                "maxResults": limit.unwrap_or(100).min(100),
            });
            if let Some(token) = &next {
                body["nextPageToken"] = json!(token);
            }
            let page = self.call("POST", "/rest/api/3/search/jql", Some(&body))?;
            for issue in page["issues"].as_array().into_iter().flatten() {
                let Some(key) = issue["key"].as_str().and_then(key) else {
                    continue;
                };
                let fields = &issue["fields"];
                found.push(TicketSummary {
                    reference: reference(&self.base, &key)?,
                    title: fields["summary"].as_str().unwrap_or_default().into(),
                    state: fields["status"]["name"].as_str().unwrap_or_default().into(),
                    labels: strings(&fields["labels"]),
                    assignees: name(&fields["assignee"]).into_iter().collect(),
                    updated: iso_utc(fields["updated"].as_str().unwrap_or_default()),
                    project: None,
                    source: "jira".into(),
                });
            }
            if limit.is_some_and(|l| found.len() >= l) {
                found.truncate(limit.unwrap_or_default());
                break;
            }
            match page["nextPageToken"].as_str() {
                Some(token) if page["isLast"] != true => next = Some(token.to_owned()),
                _ => break,
            }
        }
        Ok(found)
    }
}

fn strings(list: &Value) -> Vec<String> {
    let items = list.as_array().into_iter().flatten();
    items
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

fn name(user: &Value) -> Option<String> {
    user["displayName"].as_str().map(str::to_owned)
}

/// A Markdown text as the smallest ADF document: one paragraph per block of lines.
fn document(text: &str) -> Value {
    let paragraphs: Vec<Value> = text
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .map(|p| json!({ "type": "paragraph", "content": [{ "type": "text", "text": p }] }))
        .collect();
    json!({ "type": "doc", "version": 1, "content": paragraphs })
}

impl TicketProvider for Jira {
    fn resolve(&self, input: &str) -> Option<TicketRef> {
        let (key, host) = parse(input)?;
        let own = self.base.split_once("://").map(|(_, h)| h);
        if host.is_some() && host.as_deref() != own {
            return None;
        }
        reference(&self.base, &key).ok()
    }

    fn fetch(&self, ticket: &TicketRef) -> Result<Ticket> {
        let key = key(&ticket.key).with_context(|| format!("not a Jira key: {}", ticket.key))?;
        let issue = self.call(
            "GET",
            &format!(
                "/rest/api/3/issue/{key}?fields=summary,description,status,labels,assignee,reporter"
            ),
            None,
        )?;
        let mut comments = Vec::new();
        loop {
            let page = self.call(
                "GET",
                &format!(
                    "/rest/api/3/issue/{key}/comment?startAt={}&maxResults=100&orderBy=created",
                    comments.len()
                ),
                None,
            )?;
            let listed = page["comments"].as_array().cloned().unwrap_or_default();
            comments.extend(listed.iter().map(|c| Comment {
                author: name(&c["author"]),
                created: iso_utc(c["created"].as_str().unwrap_or_default()),
                body: markdown(&c["body"]),
            }));
            let total = page["total"].as_u64().unwrap_or_default() as usize;
            if listed.is_empty() || comments.len() >= total {
                break;
            }
        }
        let fields = &issue["fields"];
        let body = markdown(&fields["description"]);
        let texts = std::iter::once(body.as_str()).chain(comments.iter().map(|c| c.body.as_str()));
        Ok(Ticket {
            links: crate::links(texts, &ticket.url),
            title: fields["summary"].as_str().unwrap_or_default().into(),
            state: fields["status"]["name"].as_str().unwrap_or_default().into(),
            author: name(&fields["reporter"]),
            labels: strings(&fields["labels"]),
            assignees: name(&fields["assignee"]).into_iter().collect(),
            body,
            comments,
            reference: ticket.clone(),
        })
    }

    /// `query` is JQL.
    fn search(&self, query: &str) -> Result<Vec<TicketSummary>> {
        self.issues(query, None)
    }

    fn comment(&self, ticket: &TicketRef, body: &str) -> Result<()> {
        let key = key(&ticket.key).with_context(|| format!("not a Jira key: {}", ticket.key))?;
        let body = json!({ "body": document(body) });
        self.call(
            "POST",
            &format!("/rest/api/3/issue/{key}/comment"),
            Some(&body),
        )?;
        Ok(())
    }

    /// `to` names a transition or the status it leads to, in any case.
    fn transition(&self, ticket: &TicketRef, to: &str) -> Result<()> {
        let key = key(&ticket.key).with_context(|| format!("not a Jira key: {}", ticket.key))?;
        let path = format!("/rest/api/3/issue/{key}/transitions");
        let listed = self.call("GET", &path, None)?;
        let transitions = listed["transitions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let matches = |v: &Value| {
            v.as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(to.trim()))
        };
        let Some(id) = transitions
            .iter()
            .find(|t| matches(&t["name"]) || matches(&t["to"]["name"]))
            .and_then(|t| t["id"].as_str())
        else {
            let names: Vec<&str> = transitions
                .iter()
                .filter_map(|t| t["to"]["name"].as_str())
                .collect();
            bail!(
                "{key} cannot move to {to:?} from where it is: it can go to {}",
                names.join(", ")
            );
        };
        let body = json!({ "transition": { "id": id } });
        self.call("POST", &path, Some(&body))?;
        Ok(())
    }

    /// A remote link on the ticket; the same pull request linked twice stays one link.
    fn link_pr(&self, ticket: &TicketRef, url: &str) -> Result<()> {
        let key = key(&ticket.key).with_context(|| format!("not a Jira key: {}", ticket.key))?;
        let title = url
            .strip_prefix("https://github.com/")
            .map_or_else(|| url.to_owned(), |path| path.replacen("/pull/", "#", 1));
        let body = json!({
            "globalId": url,
            "object": { "url": url, "title": format!("Pull request {title}") },
        });
        self.call(
            "POST",
            &format!("/rest/api/3/issue/{key}/remotelink"),
            Some(&body),
        )?;
        Ok(())
    }
}

/// An Atlassian Document Format value (descriptions, comments) as Markdown.
pub(crate) fn markdown(adf: &Value) -> String {
    match adf {
        Value::String(text) => text.trim().to_owned(),
        Value::Object(_) => block(adf).trim().to_owned(),
        _ => String::new(),
    }
}

fn children(node: &Value) -> impl Iterator<Item = &Value> {
    node["content"].as_array().into_iter().flatten()
}

fn blocks(node: &Value, between: &str) -> String {
    let rendered: Vec<String> = children(node)
        .map(block)
        .filter(|b| !b.trim().is_empty())
        .collect();
    rendered.join(between)
}

fn block(node: &Value) -> String {
    let attrs = &node["attrs"];
    match node["type"].as_str().unwrap_or_default() {
        "doc" => blocks(node, "\n\n"),
        "paragraph" => inline(node),
        "heading" => {
            let level = attrs["level"].as_u64().unwrap_or(1).clamp(1, 6) as usize;
            format!("{} {}", "#".repeat(level), inline(node))
        }
        "bulletList" => list(node, |_| "- ".into()),
        "orderedList" => {
            let start = attrs["order"].as_u64().unwrap_or(1);
            list(node, |i| format!("{}. ", start + i as u64))
        }
        "taskList" => list(node, |_| String::new()),
        "taskItem" => {
            let done = attrs["state"].as_str() == Some("DONE");
            format!("- [{}] {}", if done { "x" } else { " " }, inline(node))
        }
        "codeBlock" => {
            let language = attrs["language"].as_str().unwrap_or_default();
            let code: String = children(node).filter_map(|t| t["text"].as_str()).collect();
            format!("```{language}\n{}\n```", code.trim_end_matches('\n'))
        }
        "blockquote" | "panel" => quote(&blocks(node, "\n\n")),
        "rule" => "---".into(),
        "expand" | "nestedExpand" => {
            let title = attrs["title"].as_str().unwrap_or_default();
            let inner = blocks(node, "\n\n");
            if title.is_empty() {
                inner
            } else {
                format!("**{title}**\n\n{inner}")
            }
        }
        "table" => table(node),
        "mediaSingle" | "mediaGroup" | "media" => "(attachment)".into(),
        _ if node["content"].is_array() => blocks(node, "\n\n"),
        _ => inline_node(node),
    }
}

fn quote(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|l| {
            if l.is_empty() {
                ">".into()
            } else {
                format!("> {l}")
            }
        })
        .collect();
    lines.join("\n")
}

/// Each item gets its marker; its other lines and nested lists are indented under it.
fn list(node: &Value, marker: impl Fn(usize) -> String) -> String {
    let items: Vec<String> = children(node)
        .enumerate()
        .map(|(i, item)| {
            if item["type"] == "taskItem" {
                return block(item);
            }
            let mark = marker(i);
            let body = blocks(item, "\n");
            let pad = " ".repeat(mark.len());
            let mut lines = body.lines();
            let first = lines.next().unwrap_or_default();
            let rest: Vec<String> = lines
                .map(|l| {
                    if l.is_empty() {
                        String::new()
                    } else {
                        format!("{pad}{l}")
                    }
                })
                .collect();
            std::iter::once(format!("{mark}{first}"))
                .chain(rest)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    items.join("\n")
}

fn table(node: &Value) -> String {
    let rows: Vec<Vec<String>> = children(node)
        .map(|row| {
            children(row)
                .map(|cell| blocks(cell, " ").replace('\n', " ").replace('|', "\\|"))
                .collect()
        })
        .collect();
    let mut lines = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        lines.push(format!("| {} |", row.join(" | ")));
        if i == 0 {
            lines.push(format!("|{}", " --- |".repeat(row.len())));
        }
    }
    lines.join("\n")
}

fn inline(node: &Value) -> String {
    children(node).map(inline_node).collect()
}

fn inline_node(node: &Value) -> String {
    let attrs = &node["attrs"];
    let text = |key: &str| attrs[key].as_str().unwrap_or_default().to_owned();
    match node["type"].as_str().unwrap_or_default() {
        "text" => marked(node["text"].as_str().unwrap_or_default(), &node["marks"]),
        "hardBreak" => "\n".into(),
        "mention" => {
            let name = text("text");
            let name = name.trim_start_matches('@');
            if name.is_empty() {
                format!("@{}", text("id"))
            } else {
                format!("@{name}")
            }
        }
        "emoji" => match text("text") {
            t if t.is_empty() => text("shortName"),
            t => t,
        },
        "inlineCard" | "blockCard" | "embedCard" => text("url"),
        "status" => format!("[{}]", text("text")),
        "date" => {
            let millis: i64 = text("timestamp").parse().unwrap_or_default();
            utc(millis / 1000)[..10].to_owned()
        }
        _ => inline(node),
    }
}

fn marked(text: &str, marks: &Value) -> String {
    let marks: Vec<&Value> = marks.as_array().into_iter().flatten().collect();
    let has = |kind: &str| marks.iter().any(|m| m["type"] == kind);
    let mut out = text.to_owned();
    if has("code") {
        out = format!("`{out}`");
    }
    if has("strike") {
        out = format!("~~{out}~~");
    }
    if has("em") {
        out = format!("*{out}*");
    }
    if has("strong") {
        out = format!("**{out}**");
    }
    if let Some(href) = marks
        .iter()
        .find(|m| m["type"] == "link")
        .and_then(|m| m["attrs"]["href"].as_str())
    {
        out = format!("[{out}]({href})");
    }
    out
}
