//! GitLab issues and merge requests, on gitlab.com or a GitLab of its own: through `glab` when it is installed
//! and signed in to the host (it keeps its own sign-in, as `gh` does), else the REST API v4 with curl and an
//! access token kept in the system's keychain.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use termf_ssh::Ssh;

use crate::http::{Body, Http, encode};
use crate::store::iso_utc;
use crate::{Comment, Ticket, TicketProvider, TicketRef, TicketSummary};

/// The host a `[trackers.gitlab]` without one reads.
pub const DEFAULT_HOST: &str = "gitlab.com";

/// Pages of 100 a listing reads at most.
const MAX_PAGES: u32 = 10;

/// `https://gitlab.com`, or a local `http://127.0.0.1:<port>` kept as it is, for tests.
pub(crate) fn base(site: &str) -> Result<String> {
    let site = site.trim().trim_end_matches('/');
    if crate::http::local(site) {
        return Ok(site.to_owned());
    }
    let site = if site.is_empty() { DEFAULT_HOST } else { site };
    let host = crate::http::host(site, "GitLab host", "gitlab.com or gitlab.acme.com")?;
    Ok(format!("https://{host}"))
}

/// `gitlab.com`, as `glab` and the keychain name it.
pub(crate) fn host(site: &str) -> Result<String> {
    let base = base(site)?;
    Ok(base
        .split_once("://")
        .map_or(base.clone(), |(_, h)| h.to_owned()))
}

/// The keychain account of an access token: one per host.
pub(crate) fn account(site: &str) -> Result<String> {
    Ok(format!("gitlab:{}", host(site)?))
}

/// `group/sub/project`: two parts or more, each of letters, digits, `-`, `_` and `.`.
fn valid_slug(slug: &str) -> bool {
    let parts: Vec<&str> = slug.split('/').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
}

pub(crate) fn reference(site: &str, slug: &str, iid: u64) -> Result<TicketRef> {
    Ok(TicketRef {
        provider: "gitlab".into(),
        key: format!("{slug}#{iid}"),
        url: format!("{}/{slug}/-/issues/{iid}", base(site)?),
    })
}

/// `group/project` and the number of a key such as `acme/api#42`.
pub(crate) fn split(key: &str) -> Result<(&str, u64)> {
    key.rsplit_once('#')
        .and_then(|(slug, n)| Some((slug, crate::github::number(n)?)))
        .filter(|(slug, _)| valid_slug(slug))
        .with_context(|| format!("not a GitLab issue: {key}"))
}

/// The host, project and number of an address on a GitLab: `…/<group>/<project>/-/<kind>/<n>`, `kind` being
/// `issues` (or `work_items`, its newer name) or `merge_requests`.
fn address<'a>(url: &'a str, kinds: &[&str]) -> Option<(String, &'a str, u64)> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !matches!(scheme, "https" | "http") {
        return None;
    }
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let (host, path) = rest.split_once('/')?;
    let (slug, tail) = path.split_once("/-/")?;
    let mut tail = tail.trim_end_matches('/').split('/');
    let kind = tail.next()?;
    let n = crate::github::number(tail.next()?)?;
    (kinds.contains(&kind) && tail.next().is_none() && valid_slug(slug))
        .then(|| (format!("{scheme}://{}", host.to_ascii_lowercase()), slug, n))
}

/// The site (`https://gitlab.com`), project and number of an issue's address.
pub(crate) fn parse_url(url: &str) -> Option<(String, String, u64)> {
    address(url, &["issues", "work_items"]).map(|(b, s, n)| (b, s.to_owned(), n))
}

/// The site, project and number of a merge request's address.
pub(crate) fn merge_request(url: &str) -> Option<(String, String, u64)> {
    address(url, &["merge_requests"]).map(|(b, s, n)| (b, s.to_owned(), n))
}

/// `Merge request acme/api!7`, `Pull request acme/front#12`: how a tracker titles a link to the delivery.
pub(crate) fn request_title(url: &str) -> String {
    if let Some((_, slug, n)) = merge_request(url) {
        return format!("Merge request {slug}!{n}");
    }
    let title = url
        .strip_prefix("https://github.com/")
        .map_or_else(|| url.to_owned(), |path| path.replacen("/pull/", "#", 1));
    format!("Pull request {title}")
}

/// The host and `group/project` (lowercased) of a remote on a GitLab: a host named in `hosts`, or one whose
/// name says GitLab, directly or through an SSH alias.
pub(crate) fn remote_slug(url: &str, ssh: &Ssh, hosts: &[String]) -> Option<(String, String)> {
    let (host, path) = crate::github::remote_parts(url)?;
    let gitlab = |h: &str| hosts.iter().any(|k| k == h) || h.contains("gitlab");
    let host = if gitlab(&host) {
        host
    } else {
        let named = ssh.host_name(&host).to_ascii_lowercase();
        if !gitlab(&named) {
            return None;
        }
        named
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    valid_slug(path).then(|| (host, path.to_ascii_lowercase()))
}

/// Whether `glab` is installed and signed in to `host`.
pub(crate) fn glab_ready(program: &Path, env: &[(String, String)], host: &str) -> bool {
    let mut cmd = glab_command(program, env);
    cmd.args(["auth", "status", "--hostname", host])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.status().is_ok_and(|s| s.success())
}

fn glab_command(program: &Path, env: &[(String, String)]) -> std::process::Command {
    let mut cmd = crate::command(program);
    // Launched from the Finder, TermF lacks the PATH of the shell profile (Homebrew…), which glab also needs.
    if program == Path::new("glab")
        && let Some(path) = termf_core::shell::login_path()
    {
        cmd.env("PATH", path);
    }
    cmd.envs(env.iter().map(|(k, v)| (k, v)))
        .env("GLAB_NO_PROMPT", "1")
        .env("NO_PROMPT", "1");
    cmd
}

pub(crate) enum Via {
    Glab {
        program: PathBuf,
        env: Vec<(String, String)>,
    },
    /// The REST API v4, `base` ending in `/api/v4`.
    Token(Http),
}

pub struct GitLab {
    /// `https://gitlab.com`.
    pub(crate) base: String,
    pub(crate) via: Via,
}

fn usernames(users: &Value) -> Vec<String> {
    let users = users.as_array().into_iter().flatten();
    users
        .filter_map(|u| u["username"].as_str().map(str::to_owned))
        .collect()
}

fn strings(list: &Value) -> Vec<String> {
    let items = list.as_array().into_iter().flatten();
    items
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

impl GitLab {
    fn host(&self) -> &str {
        self.base.split_once("://").map_or(&self.base, |(_, h)| h)
    }

    /// One call to the API, `path` under `/api/v4/`; `fresh` is `None` for a write.
    fn api(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        fresh: Option<bool>,
    ) -> Result<Value> {
        match &self.via {
            Via::Token(http) => {
                let path = format!("/{path}");
                match fresh {
                    Some(fresh) => http.read(method, &path, body.map(Body::json), fresh),
                    None => http.call(method, &path, body.map(Body::json)),
                }
            }
            Via::Glab { program, env } => {
                let host = self.host();
                let mut cmd = glab_command(program, env);
                cmd.args(["api", "--hostname", host, "-X", method, path]);
                if body.is_some() {
                    cmd.args(["-H", "Content-Type: application/json", "--input", "-"]);
                }
                let mut child = cmd
                    .stdin(if body.is_some() {
                        Stdio::piped()
                    } else {
                        Stdio::null()
                    })
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .context("the GitLab CLI (glab) is not installed")?;
                // glab can exit before reading its input: its exit status and stderr say why.
                if let (Some(body), Some(mut stdin)) = (body, child.stdin.take())
                    && let Err(e) = stdin.write_all(body.to_string().as_bytes())
                    && e.kind() != std::io::ErrorKind::BrokenPipe
                {
                    return Err(e.into());
                }
                let out = child.wait_with_output()?;
                if !out.status.success() {
                    let error = String::from_utf8_lossy(&out.stderr).trim().to_owned();
                    if error.contains("401") || error.contains("auth login") {
                        bail!(
                            "glab is not signed in to {host}: run `glab auth login --hostname {host}`, or save an access token in the project's Tickets section"
                        );
                    }
                    bail!("glab api {path} failed: {error}");
                }
                let text = String::from_utf8_lossy(&out.stdout);
                if text.trim().is_empty() {
                    return Ok(Value::Null);
                }
                serde_json::from_str(&text).with_context(|| format!("glab api {path} gave no JSON"))
            }
        }
    }

    /// Every page of a listing, 100 a page, at most `limit`.
    fn pages(&self, path: &str, limit: Option<usize>, fresh: bool) -> Result<Vec<Value>> {
        let mut found = Vec::new();
        let per = limit.unwrap_or(100).min(100);
        let join = if path.contains('?') { '&' } else { '?' };
        for page in 1..=MAX_PAGES {
            let path = format!("{path}{join}per_page={per}&page={page}");
            let listed = self.api("GET", &path, None, Some(fresh))?;
            let listed = listed.as_array().cloned().unwrap_or_default();
            let done = listed.len() < per;
            found.extend(listed);
            if done || limit.is_some_and(|l| found.len() >= l) {
                break;
            }
        }
        if let Some(limit) = limit {
            found.truncate(limit);
        }
        Ok(found)
    }

    /// The open issues of `slugs`, assigned to the user when `mine`, at most `limit`.
    pub(crate) fn issues(
        &self,
        slugs: &[String],
        mine: bool,
        limit: Option<usize>,
        fresh: bool,
    ) -> Result<Vec<TicketSummary>> {
        let scope = if mine { "assigned_to_me" } else { "all" };
        let mut found = Vec::new();
        for slug in slugs {
            let path = format!(
                "projects/{}/issues?state=opened&scope={scope}&order_by=updated_at",
                encode(slug)
            );
            for issue in self.pages(&path, limit, fresh)? {
                let Some(iid) = issue["iid"].as_u64() else {
                    continue;
                };
                let mut reference = reference(&self.base, slug, iid)?;
                if let Some(full) = issue["references"]["full"].as_str()
                    && split(full).is_ok()
                {
                    reference.key = full.to_owned();
                }
                found.push(TicketSummary {
                    reference,
                    title: issue["title"].as_str().unwrap_or_default().into(),
                    state: issue["state"].as_str().unwrap_or_default().into(),
                    labels: strings(&issue["labels"]),
                    assignees: usernames(&issue["assignees"]),
                    updated: iso_utc(issue["updated_at"].as_str().unwrap_or_default()),
                    project: None,
                    source: "gitlab".into(),
                });
            }
        }
        if let Some(limit) = limit {
            found.truncate(limit);
        }
        Ok(found)
    }

    /// Opens a merge request from `head` into `base` of `slug` and returns its address.
    pub(crate) fn create_mr(
        &self,
        slug: &str,
        target: &str,
        head: &str,
        title: &str,
        body: &str,
    ) -> Result<String> {
        let request = json!({
            "source_branch": head,
            "target_branch": target,
            "title": title,
            "description": body,
        });
        let path = format!("projects/{}/merge_requests", encode(slug));
        let created = self.api("POST", &path, Some(&request), None)?;
        created["web_url"]
            .as_str()
            .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
            .map(str::to_owned)
            .context("GitLab gave no merge request address")
    }

    fn request(&self, url: &str) -> Result<Value> {
        let (_, slug, iid) =
            merge_request(url).with_context(|| format!("{url} is not a GitLab merge request"))?;
        let path = format!("projects/{}/merge_requests/{iid}", encode(&slug));
        self.api("GET", &path, None, Some(false))
    }

    /// `merged` or `closed` once the merge request at `url` is, `None` while it is open.
    pub(crate) fn mr_state(&self, url: &str) -> Result<Option<&'static str>> {
        Ok(match self.request(url)?["state"].as_str() {
            Some("merged") => Some("merged"),
            Some("closed") => Some("closed"),
            Some(_) => None,
            None => bail!("GitLab gave no state for {url}"),
        })
    }

    /// GitLab's conflict flag and `detailed_merge_status` of the merge request at `url`.
    pub(crate) fn mr_merge(&self, url: &str) -> Result<(bool, String)> {
        let request = self.request(url)?;
        let status = request["detailed_merge_status"]
            .as_str()
            .unwrap_or("unchecked")
            .to_owned();
        Ok((request["has_conflicts"] == true, status))
    }
}

impl TicketProvider for GitLab {
    fn resolve(&self, input: &str) -> Option<TicketRef> {
        match parse_url(input) {
            Some((base, slug, iid)) if base == self.base => reference(&base, &slug, iid).ok(),
            Some(_) => None,
            None => {
                let (slug, iid) = split(input.trim()).ok()?;
                reference(&self.base, slug, iid).ok()
            }
        }
    }

    fn fetch(&self, ticket: &TicketRef) -> Result<Ticket> {
        let (slug, iid) = split(&ticket.key)?;
        let path = format!("projects/{}/issues/{iid}", encode(slug));
        let issue = self.api("GET", &path, None, Some(true))?;
        let notes = format!("{path}/notes?sort=asc&order_by=created_at");
        let comments: Vec<Comment> = self
            .pages(&notes, None, true)?
            .iter()
            .filter(|n| n["system"] != true)
            .map(|n| Comment {
                author: n["author"]["username"].as_str().map(str::to_owned),
                created: iso_utc(n["created_at"].as_str().unwrap_or_default()),
                body: n["body"].as_str().unwrap_or_default().to_owned(),
            })
            .collect();
        let body = issue["description"].as_str().unwrap_or_default().to_owned();
        let texts = std::iter::once(body.as_str()).chain(comments.iter().map(|c| c.body.as_str()));
        Ok(Ticket {
            links: crate::links(texts, &ticket.url),
            title: issue["title"].as_str().unwrap_or_default().into(),
            state: issue["state"].as_str().unwrap_or_default().into(),
            author: issue["author"]["username"].as_str().map(str::to_owned),
            labels: strings(&issue["labels"]),
            assignees: usernames(&issue["assignees"]),
            body,
            comments,
            reference: ticket.clone(),
        })
    }

    /// `query` names a project, `group/project`: its open issues assigned to the user.
    fn search(&self, query: &str) -> Result<Vec<TicketSummary>> {
        ensure!(
            valid_slug(query.trim()),
            "{query:?} is not a GitLab project"
        );
        self.issues(&[query.trim().to_owned()], true, None, false)
    }

    fn comment(&self, ticket: &TicketRef, body: &str) -> Result<()> {
        let (slug, iid) = split(&ticket.key)?;
        let path = format!("projects/{}/issues/{iid}/notes", encode(slug));
        self.api("POST", &path, Some(&json!({ "body": body })), None)?;
        Ok(())
    }

    fn transition(&self, ticket: &TicketRef, _to: &str) -> Result<()> {
        Err(anyhow!(
            "{}: GitLab issues have no states to move to; a merge request that says Closes #{} closes it once merged",
            ticket.key,
            split(&ticket.key).map_or(0, |(_, n)| n)
        ))
    }

    /// A merge request that says `Closes #42` is already linked: this tells the ticket's readers too.
    fn link_pr(&self, ticket: &TicketRef, url: &str) -> Result<()> {
        self.comment(ticket, &format!("{}: {url}", request_title(url)))
    }
}
