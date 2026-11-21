//! GitHub Issues through the GitHub CLI: `gh` brings its own sign-in (`gh auth login`), so TermF keeps no token.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use crate::{Comment, Ticket, TicketProvider, TicketRef, TicketSummary};

pub struct GitHub {
    pub program: PathBuf,
    pub env: Vec<(String, String)>,
}

pub(crate) fn reference(slug: &str, number: u64) -> TicketRef {
    TicketRef {
        provider: "github".into(),
        key: format!("{slug}#{number}"),
        url: format!("https://github.com/{slug}/issues/{number}"),
    }
}

/// `owner/name` and the number of a key such as `acme/front#42`.
pub(crate) fn split(key: &str) -> Result<(&str, u64)> {
    key.rsplit_once('#')
        .and_then(|(slug, n)| Some((slug, number(n)?)))
        .filter(|(slug, _)| valid_slug(slug))
        .with_context(|| format!("not a GitHub issue: {key}"))
}

pub(crate) fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn valid_slug(slug: &str) -> bool {
    let part = |p: Option<&str>| {
        p.is_some_and(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    };
    let mut parts = slug.split('/');
    part(parts.next()) && part(parts.next()) && parts.next().is_none()
}

/// The `owner/name` (lowercased) a git remote points to, whatever its form: HTTPS, SSH, an SSH alias of
/// `~/.ssh/config`, a local path.
pub(crate) fn remote_slug(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix(".git").unwrap_or(url);
    let mut parts = url.rsplit(['/', ':', '\\']);
    let name = parts.next()?;
    let slug = format!("{}/{name}", parts.next()?);
    valid_slug(&slug).then(|| slug.to_ascii_lowercase())
}

/// Builds a ticket from the REST API's issue and comments.
pub(crate) fn ticket_from(
    reference: TicketRef,
    issue: &Value,
    comments: &[Value],
) -> Result<Ticket> {
    ensure!(
        issue.get("pull_request").is_none(),
        "{} is a pull request, not an issue",
        reference.key
    );
    let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
    let login = |v: &Value| v["login"].as_str().map(str::to_owned);
    let names = |v: &Value, key: &str| -> Vec<String> {
        let items = v.as_array().into_iter().flatten();
        items
            .filter_map(|i| i[key].as_str().map(str::to_owned))
            .collect()
    };
    let comments: Vec<Comment> = comments
        .iter()
        .map(|c| Comment {
            author: login(&c["user"]),
            created: text(&c["created_at"]),
            body: text(&c["body"]),
        })
        .collect();
    let body = text(&issue["body"]);
    let texts = std::iter::once(body.as_str()).chain(comments.iter().map(|c| c.body.as_str()));
    Ok(Ticket {
        links: crate::links(texts, &reference.url),
        title: text(&issue["title"]),
        state: text(&issue["state"]),
        author: login(&issue["user"]),
        labels: names(&issue["labels"], "name"),
        assignees: names(&issue["assignees"], "login"),
        body,
        comments,
        reference,
    })
}

impl GitHub {
    fn run(&self, dir: Option<&Path>, args: &[&str], input: Option<&str>) -> Result<String> {
        let mut cmd = crate::command(&self.program);
        // Launched from the Finder, TermF lacks the PATH of the shell profile (Homebrew…), which gh also needs.
        if self.program == Path::new("gh")
            && let Some(path) = termf_core::shell::login_path()
        {
            cmd.env("PATH", path);
        }
        if let Some(dir) = dir {
            cmd.current_dir(dir);
        }
        cmd.args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .env("GH_PROMPT_DISABLED", "1")
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .context("the GitHub CLI (gh) is not installed")?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin.write_all(input.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        let error = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        if error.contains("gh auth login") {
            bail!("gh is not signed in: run `gh auth login`");
        }
        bail!("gh {} failed: {error}", args[..2.min(args.len())].join(" "))
    }

    fn json(&self, args: &[&str]) -> Result<Value> {
        let out = self.run(None, args, None)?;
        serde_json::from_str(&out).with_context(|| format!("gh {} gave no JSON", args.join(" ")))
    }

    /// Opens a pull request from `head` into `base` of `repo` (`owner/name`) and returns its URL.
    pub(crate) fn create_pr(
        &self,
        dir: &Path,
        repo: &str,
        base: &str,
        head: &str,
        title: &str,
        body: &str,
    ) -> Result<String> {
        // `--flag=value`: a title that starts with `-` stays a value.
        let args = [
            "pr".to_owned(),
            "create".to_owned(),
            format!("--repo={repo}"),
            format!("--base={base}"),
            format!("--head={head}"),
            format!("--title={title}"),
            format!("--body={body}"),
        ];
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = self.run(Some(dir), &args, None)?;
        out.lines()
            .map(str::trim)
            .rfind(|l| l.starts_with("https://"))
            .map(str::to_owned)
            .context("gh pr create gave no pull request URL")
    }
}

impl TicketProvider for GitHub {
    fn resolve(&self, input: &str) -> Option<TicketRef> {
        let input = input.trim();
        let address = ["https://", "http://"]
            .iter()
            .find_map(|scheme| input.strip_prefix(scheme))
            .unwrap_or(input);
        let address = address.strip_prefix("www.").unwrap_or(address);
        let (slug, number) = match address.strip_prefix("github.com/") {
            Some(path) => {
                let path = path.split(['?', '#']).next()?.trim_end_matches('/');
                match path.split('/').collect::<Vec<_>>().as_slice() {
                    [owner, name, "issues", n] => (format!("{owner}/{name}"), number(n)?),
                    _ => return None,
                }
            }
            None => {
                let (slug, n) = input.rsplit_once('#')?;
                (slug.to_owned(), number(n)?)
            }
        };
        valid_slug(&slug).then(|| reference(&slug, number))
    }

    fn fetch(&self, ticket: &TicketRef) -> Result<Ticket> {
        let (slug, number) = split(&ticket.key)?;
        let issue = self.json(&["api", &format!("repos/{slug}/issues/{number}")])?;
        let pages = self.json(&[
            "api",
            "--paginate",
            "--slurp",
            &format!("repos/{slug}/issues/{number}/comments?per_page=100"),
        ])?;
        let comments: Vec<Value> = pages
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|page| page.as_array().cloned().unwrap_or_default())
            .collect();
        ticket_from(ticket.clone(), &issue, &comments)
    }

    fn search(&self, query: &str) -> Result<Vec<TicketSummary>> {
        let found = self.json(&[
            "api",
            "--method",
            "GET",
            "search/issues",
            "-f",
            &format!("q={query}"),
            "-f",
            "per_page=50",
        ])?;
        let items = found["items"].as_array().into_iter().flatten();
        Ok(items
            .filter_map(|item| {
                let labels = item["labels"].as_array().into_iter().flatten();
                Some(TicketSummary {
                    reference: self.resolve(item["html_url"].as_str()?)?,
                    title: item["title"].as_str().unwrap_or_default().into(),
                    state: item["state"].as_str().unwrap_or_default().into(),
                    labels: labels
                        .filter_map(|l| l["name"].as_str().map(str::to_owned))
                        .collect(),
                    updated: item["updated_at"].as_str().unwrap_or_default().into(),
                    project: None,
                })
            })
            .collect())
    }

    fn comment(&self, ticket: &TicketRef, body: &str) -> Result<()> {
        let (slug, number) = split(&ticket.key)?;
        let path = format!("repos/{slug}/issues/{number}/comments");
        let input = json!({ "body": body }).to_string();
        self.run(
            None,
            &["api", "--method", "POST", &path, "--input", "-"],
            Some(&input),
        )?;
        Ok(())
    }

    fn transition(&self, _: &TicketRef, _: &str) -> Result<()> {
        bail!("GitHub Issues have no workflow states to move to (Projects v2 is not supported yet)")
    }

    /// A pull request that says `Closes #42` is already linked: this tells the ticket's readers too.
    fn link_pr(&self, ticket: &TicketRef, url: &str) -> Result<()> {
        self.comment(ticket, &format!("Pull request: {url}"))
    }
}
