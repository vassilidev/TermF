//! GitHub Issues and Projects v2 boards through the GitHub CLI: `gh` brings its own sign-in (`gh auth login`),
//! so TermF keeps no token.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use termf_ssh::Ssh;

use crate::{Comment, Ticket, TicketProvider, TicketRef, TicketSummary};

pub struct GitHub {
    pub program: PathBuf,
    pub env: Vec<(String, String)>,
    /// The project's board, whose Status `transition` changes.
    pub board: Option<Board>,
}

/// A Projects v2 board: `acme/4`, or its address.
#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    pub owner: String,
    pub number: u64,
}

impl Board {
    /// `owner/number`, or `https://github.com/orgs/<owner>/projects/<number>` (`users/` too).
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim().trim_end_matches('/');
        let path = text
            .strip_prefix("https://github.com/")
            .map(|p| p.split(['?', '#']).next().unwrap_or(p));
        let parts: Vec<&str> = match path {
            Some(path) => path.split('/').collect(),
            None => text.split('/').collect(),
        };
        let (owner, number) = match parts.as_slice() {
            ["orgs" | "users", owner, "projects", n, ..] => (*owner, *n),
            [owner, n] if path.is_none() => (*owner, *n),
            _ => bail!("{text:?} is not a GitHub project: write it owner/number, such as acme/4"),
        };
        let valid = !owner.is_empty()
            && owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
        match number.parse() {
            Ok(number) if valid => Ok(Self {
                owner: owner.to_owned(),
                number,
            }),
            _ => bail!("{text:?} is not a GitHub project: write it owner/number, such as acme/4"),
        }
    }
}

const BOARD_ITEMS: &str = "query($owner: String!, $number: Int!, $cursor: String) {
  viewer { login }
  repositoryOwner(login: $owner) {
    ... on ProjectV2Owner {
      projectV2(number: $number) {
        items(first: 100, after: $cursor) {
          pageInfo { hasNextPage endCursor }
          nodes {
            isArchived
            status: fieldValueByName(name: \"Status\") { ... on ProjectV2ItemFieldSingleSelectValue { name } }
            content {
              ... on Issue {
                url title state updatedAt
                labels(first: 20) { nodes { name } }
                assignees(first: 20) { nodes { login } }
              }
            }
          }
        }
      }
    }
  }
}";

const BOARD_ITEM: &str = "query($owner: String!, $number: Int!, $url: URI!) {
  repositoryOwner(login: $owner) {
    ... on ProjectV2Owner {
      projectV2(number: $number) {
        id
        field(name: \"Status\") { ... on ProjectV2SingleSelectField { id options { id name } } }
      }
    }
  }
  resource(url: $url) {
    ... on Issue { projectItems(first: 50) { nodes { id project { id } } } }
  }
}";

const MOVE_ITEM: &str = "mutation($project: ID!, $item: ID!, $field: ID!, $option: String!) {
  updateProjectV2ItemFieldValue(input: {projectId: $project, itemId: $item, fieldId: $field, value: {singleSelectOptionId: $option}}) {
    projectV2Item { id }
  }
}";

/// Pages of 100 items a board is read for at most.
const MAX_PAGES: usize = 30;

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

const GITHUB_HOSTS: &[&str] = &["github.com", "www.github.com", "ssh.github.com"];

/// The `owner/name` (lowercased) of a remote on github.com, over HTTPS, SSH or an SSH alias whose `HostName`
/// is GitHub. Other hosts and local paths are refused.
pub(crate) fn remote_slug(url: &str, ssh: &Ssh) -> Result<String> {
    let url = url.trim();
    let local = || anyhow!("{url} is a local path, not a GitHub repository");
    let unhost = |authority: &'_ str| -> String {
        let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        host.split(':').next().unwrap_or(host).to_ascii_lowercase()
    };
    let (host, path) = match url.split_once("://") {
        Some((_, rest)) => {
            let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
            (unhost(authority), path)
        }
        // scp-like `[user@]host:path`, as git reads it: a colon before any slash. One letter is a Windows drive.
        None => match url.split_once(':') {
            Some((authority, path)) if !authority.contains(['/', '\\']) => {
                (unhost(authority), path)
            }
            _ => return Err(local()),
        },
    };
    if host.len() < 2 {
        return Err(local());
    }
    let server = if GITHUB_HOSTS.contains(&host.as_str()) {
        host.clone()
    } else {
        ssh.host_name(&host).to_ascii_lowercase()
    };
    ensure!(
        GITHUB_HOSTS.contains(&server.as_str()),
        "{url} is on {host}, not github.com: other hosts (GitHub Enterprise, GitLab…) are not supported yet"
    );
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    ensure!(valid_slug(path), "{url} names no GitHub repository");
    Ok(path.to_ascii_lowercase())
}

/// The `login` of each user in a list.
fn logins(users: &Value) -> Vec<String> {
    let users = users.as_array().into_iter().flatten();
    users
        .filter_map(|u| u["login"].as_str().map(str::to_owned))
        .collect()
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

    /// A GraphQL call; `fields` are `-f name=value` (strings) and `ints` `-F name=value` (numbers).
    fn graphql(&self, query: &str, fields: &[(&str, &str)], ints: &[(&str, u64)]) -> Result<Value> {
        let mut args = vec!["api".to_owned(), "graphql".to_owned()];
        args.extend(["-f".to_owned(), format!("query={query}")]);
        for (name, value) in fields {
            args.extend(["-f".to_owned(), format!("{name}={value}")]);
        }
        for (name, value) in ints {
            args.extend(["-F".to_owned(), format!("{name}={value}")]);
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let answer = self.json(&args)?;
        if let Some(errors) = answer["errors"].as_array().filter(|e| !e.is_empty()) {
            let messages: Vec<&str> = errors
                .iter()
                .filter_map(|e| e["message"].as_str())
                .collect();
            bail!("GitHub: {}", messages.join("; "));
        }
        Ok(answer)
    }

    /// The open issues on a board, every page, at most `limit`: in `columns` (its Status values, any case)
    /// when some are given, and assigned to the user when `mine`. Their state is their column.
    pub(crate) fn board_items(
        &self,
        board: &Board,
        columns: &[String],
        mine: bool,
        limit: Option<usize>,
    ) -> Result<Vec<TicketSummary>> {
        let columns: Vec<&str> = columns
            .iter()
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
            .collect();
        let mut found = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut fields = vec![("owner", board.owner.as_str())];
            if let Some(cursor) = &cursor {
                fields.push(("cursor", cursor));
            }
            let answer = self.graphql(BOARD_ITEMS, &fields, &[("number", board.number)])?;
            let data = &answer["data"];
            let project = &data["repositoryOwner"]["projectV2"];
            ensure!(
                project.is_object(),
                "no GitHub project {}/{} (or gh may not read it: `gh auth refresh -s read:project`)",
                board.owner,
                board.number
            );
            let me = data["viewer"]["login"].as_str().unwrap_or_default();
            let items = &project["items"];
            for item in items["nodes"].as_array().into_iter().flatten() {
                let issue = &item["content"];
                let column = item["status"]["name"].as_str().unwrap_or_default();
                let assigned = issue["assignees"]["nodes"].as_array().into_iter().flatten();
                let wanted = item["isArchived"] != true
                    && issue["state"] == "OPEN"
                    && (columns.is_empty()
                        || columns.iter().any(|c| c.eq_ignore_ascii_case(column)))
                    && (!mine || assigned.clone().any(|a| a["login"] == me));
                let Some(reference) = issue["url"].as_str().and_then(|u| self.resolve(u)) else {
                    continue;
                };
                if !wanted {
                    continue;
                }
                let labels = issue["labels"]["nodes"].as_array().into_iter().flatten();
                found.push(TicketSummary {
                    reference,
                    title: issue["title"].as_str().unwrap_or_default().into(),
                    state: column.into(),
                    labels: labels
                        .filter_map(|l| l["name"].as_str().map(str::to_owned))
                        .collect(),
                    assignees: logins(&issue["assignees"]["nodes"]),
                    updated: issue["updatedAt"].as_str().unwrap_or_default().into(),
                    project: None,
                    source: "githubProject".into(),
                });
            }
            if let Some(limit) = limit
                && found.len() >= limit
            {
                found.truncate(limit);
                break;
            }
            let page = &items["pageInfo"];
            match page["endCursor"].as_str() {
                Some(next) if page["hasNextPage"] == true => cursor = Some(next.to_owned()),
                _ => break,
            }
        }
        Ok(found)
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

    /// Every page; GitHub stops at 1000 results.
    fn search(&self, query: &str) -> Result<Vec<TicketSummary>> {
        let pages = self.json(&[
            "api",
            "--paginate",
            "--slurp",
            "--method",
            "GET",
            "search/issues",
            "-f",
            &format!("q={query}"),
            "-f",
            "per_page=100",
        ])?;
        let pages = pages.as_array().into_iter().flatten();
        let items = pages.flat_map(|page| page["items"].as_array().into_iter().flatten());
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
                    assignees: logins(&item["assignees"]),
                    updated: item["updated_at"].as_str().unwrap_or_default().into(),
                    project: None,
                    source: "github".into(),
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

    /// Sets the issue's Status on the project's board to `to` (any case).
    fn transition(&self, ticket: &TicketRef, to: &str) -> Result<()> {
        let board = self.board.as_ref().context(
            "GitHub Issues have no states to move to: name the project's board in [trackers.github] project",
        )?;
        let answer = self.graphql(
            BOARD_ITEM,
            &[("owner", &board.owner), ("url", &ticket.url)],
            &[("number", board.number)],
        )?;
        let data = &answer["data"];
        let project = &data["repositoryOwner"]["projectV2"];
        let project_id = project["id"]
            .as_str()
            .with_context(|| format!("no GitHub project {}/{}", board.owner, board.number))?;
        let field = &project["field"];
        let options = field["options"].as_array().cloned().unwrap_or_default();
        let option = options
            .iter()
            .find(|o| {
                o["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(to.trim()))
            })
            .and_then(|o| o["id"].as_str())
            .with_context(|| {
                let names: Vec<&str> = options.iter().filter_map(|o| o["name"].as_str()).collect();
                format!(
                    "the board has no Status {to:?}: it has {}",
                    names.join(", ")
                )
            })?;
        let items = data["resource"]["projectItems"]["nodes"].as_array();
        let item = items
            .into_iter()
            .flatten()
            .find(|i| i["project"]["id"] == project_id)
            .and_then(|i| i["id"].as_str())
            .with_context(|| {
                format!(
                    "{} is not on the board {}/{}",
                    ticket.key, board.owner, board.number
                )
            })?;
        let field = field["id"]
            .as_str()
            .context("the board has no Status field")?;
        self.graphql(
            MOVE_ITEM,
            &[
                ("project", project_id),
                ("item", item),
                ("field", field),
                ("option", option),
            ],
            &[],
        )?;
        Ok(())
    }

    /// A pull request that says `Closes #42` is already linked: this tells the ticket's readers too.
    fn link_pr(&self, ticket: &TicketRef, url: &str) -> Result<()> {
        self.comment(ticket, &format!("Pull request: {url}"))
    }
}
