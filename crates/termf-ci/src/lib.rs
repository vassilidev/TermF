//! CI/CD through GitHub Actions and the GitHub CLI (`gh`), which brings its own sign-in: runs by repository,
//! branch or pull request, their jobs, steps and logs, rerun, cancel and manual dispatch, and the message that
//! tells a ticket run's agent why the CI of its pull request failed; the repository's webhooks and their
//! deliveries; and the deployments of the tools a project deploys with, through their own CLIs ([`deploy`]). GitLab
//! CI is not supported yet.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, anyhow, bail, ensure};
use serde::Serialize;
use serde_json::Value;
use termf_tickets::{RunStatus, Tickets};

pub mod deploy;
mod log;
#[cfg(test)]
mod tests;
mod workflow;

pub use log::{JobLog, is_error, parse_log};
pub use workflow::{Input, Node, parse_graph, parse_inputs};

/// Runs per page of a listing.
const PAGE: u32 = 30;
/// Pages of 100 jobs or workflows read at most.
const MAX_PAGES: u32 = 10;

pub struct Ci {
    /// Where the GitHub CLI is, and the ticket runs whose pull requests have checks.
    pub tickets: Tickets,
    /// A query → its last ETag and answer.
    cache: Mutex<HashMap<String, (String, Value)>>,
    me: OnceLock<Option<String>>,
    /// gh answered that it does not know `--allow-escape-sequences`: not asked again.
    old_gh: AtomicBool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    /// `owner/name`.
    pub repo: String,
    pub id: u64,
    /// The workflow's name.
    pub workflow: String,
    /// Its file, `.github/workflows/ci.yml`.
    pub path: String,
    /// The commit message or pull request title it runs for.
    pub title: String,
    pub number: u64,
    pub attempt: u64,
    /// `push`, `pull_request`, `workflow_dispatch`…
    pub event: String,
    /// `queued`, `in_progress`, `waiting`, `completed`…
    pub status: String,
    /// Once completed: `success`, `failure`, `cancelled`, `skipped`, `timed_out`…
    pub conclusion: Option<String>,
    pub branch: Option<String>,
    pub sha: String,
    pub actor: Option<String>,
    /// Who started this attempt, such as the one who reran it.
    pub triggered_by: Option<String>,
    pub pull_requests: Vec<u64>,
    pub url: String,
    /// ISO 8601, as GitHub gives them.
    pub created: String,
    pub updated: String,
    pub started: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started: Option<String>,
    pub completed: Option<String>,
    pub url: String,
    pub runner: Option<String>,
    pub steps: Vec<Step>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub number: u64,
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub started: Option<String>,
    pub completed: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunPage {
    pub repo: String,
    pub runs: Vec<WorkflowRun>,
    /// Runs matching the filter, on every page.
    pub total: u64,
    /// The pull request's last commit, when the filter names one.
    pub head: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunDetail {
    pub run: WorkflowRun,
    pub jobs: Vec<Job>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    /// `.github/workflows/ci.yml`.
    pub path: String,
    /// `active`, `disabled_manually`…
    pub state: String,
}

/// A webhook of the repository, and what its last delivery got back.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hook {
    pub id: u64,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    /// The HTTP code of its last delivery; none before the first one.
    pub last_code: Option<u64>,
    /// GitHub's word on it: `active`, `unused`, or why it failed.
    pub last_status: Option<String>,
    pub updated: String,
}

/// One time GitHub called a webhook.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub id: u64,
    pub event: String,
    pub action: Option<String>,
    /// The HTTP code the hook answered, 0 when it could not be reached.
    pub code: u64,
    /// `OK`, or what went wrong.
    pub status: String,
    pub redelivery: bool,
    /// Seconds.
    pub duration: f64,
    pub delivered: String,
}

/// Starts the error of a webhooks listing GitHub refused for want of admin rights: the UI says it in its words.
pub const NO_ADMIN: &str = "admin rights needed";

/// What tells a ticket run's agent why its CI failed: `text` is in `file`, `prompt` points the agent at it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessage {
    pub text: String,
    pub file: String,
    pub prompt: String,
}

#[derive(Default, Debug)]
pub struct Filter {
    pub branch: Option<String>,
    pub status: Option<String>,
    /// Runs on the pull request's last commit; the branch is ignored then.
    pub pr: Option<u64>,
    /// From 1.
    pub page: u32,
}

struct Answer {
    ok: bool,
    out: String,
    err: String,
}

impl WorkflowRun {
    /// `Some(true)` once it passed, `Some(false)` once it failed; `None` while it runs, or when it was cancelled
    /// or skipped.
    pub fn passed(&self) -> Option<bool> {
        match self.conclusion.as_deref()? {
            "success" => Some(true),
            c if failed(c) => Some(false),
            _ => None,
        }
    }
}

pub(crate) fn failed(conclusion: &str) -> bool {
    matches!(
        conclusion,
        "failure" | "timed_out" | "startup_failure" | "action_required"
    )
}

fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn run_from(v: &Value, repo: &str) -> WorkflowRun {
    let prs = v["pull_requests"].as_array().into_iter().flatten();
    WorkflowRun {
        repo: repo.to_owned(),
        id: v["id"].as_u64().unwrap_or_default(),
        workflow: text(&v["name"]).unwrap_or_default(),
        path: text(&v["path"]).unwrap_or_default(),
        title: text(&v["display_title"]).unwrap_or_default(),
        number: v["run_number"].as_u64().unwrap_or_default(),
        attempt: v["run_attempt"].as_u64().unwrap_or(1),
        event: text(&v["event"]).unwrap_or_default(),
        status: text(&v["status"]).unwrap_or_default(),
        conclusion: text(&v["conclusion"]),
        branch: text(&v["head_branch"]),
        sha: text(&v["head_sha"]).unwrap_or_default(),
        actor: text(&v["actor"]["login"]),
        triggered_by: text(&v["triggering_actor"]["login"]),
        pull_requests: prs.filter_map(|p| p["number"].as_u64()).collect(),
        url: text(&v["html_url"]).unwrap_or_default(),
        created: text(&v["created_at"]).unwrap_or_default(),
        updated: text(&v["updated_at"]).unwrap_or_default(),
        started: text(&v["run_started_at"]),
    }
}

fn job_from(v: &Value) -> Job {
    let steps = v["steps"].as_array().into_iter().flatten();
    Job {
        id: v["id"].as_u64().unwrap_or_default(),
        name: text(&v["name"]).unwrap_or_default(),
        status: text(&v["status"]).unwrap_or_default(),
        conclusion: text(&v["conclusion"]),
        started: text(&v["started_at"]),
        completed: text(&v["completed_at"]),
        url: text(&v["html_url"]).unwrap_or_default(),
        runner: text(&v["runner_name"]),
        steps: steps
            .map(|s| Step {
                number: s["number"].as_u64().unwrap_or_default(),
                name: text(&s["name"]).unwrap_or_default(),
                status: text(&s["status"]).unwrap_or_default(),
                conclusion: text(&s["conclusion"]),
                started: text(&s["started_at"]),
                completed: text(&s["completed_at"]),
            })
            .collect(),
    }
}

/// The `owner/name` and number of a pull request's address.
fn pull_request_of(url: &str) -> Result<(String, u64)> {
    let path = url.strip_prefix("https://github.com/").unwrap_or_default();
    match path.split('/').collect::<Vec<_>>().as_slice() {
        [owner, name, "pull", n, ..] if valid_slug(&format!("{owner}/{name}")) => Ok((
            format!("{owner}/{name}").to_ascii_lowercase(),
            n.parse()
                .with_context(|| format!("{url} is not a GitHub pull request"))?,
        )),
        _ => bail!("{url} is not a GitHub pull request"),
    }
}

/// `owner/name`, with what GitHub allows in each.
fn valid_slug(slug: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p != "."
            && p != ".."
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    matches!(slug.split('/').collect::<Vec<_>>().as_slice(), [owner, name] if part(owner) && part(name))
}

/// A webhook's address and events, as they go to GitHub's API.
fn check_hook(url: &str, events: &[String]) -> Result<()> {
    ensure!(
        (url.starts_with("https://") || url.starts_with("http://"))
            && url.len() <= 2048
            && !url.chars().any(|c| c.is_whitespace() || c.is_control()),
        "{url} is not a web address"
    );
    ensure!(!events.is_empty(), "pick at least one event");
    for event in events {
        ensure!(
            event == "*"
                || (!event.is_empty()
                    && event.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')),
            "{event} is not a GitHub event"
        );
    }
    Ok(())
}

/// GitHub answers 404 to whoever may not see a repository's webhooks.
fn admin(slug: &str, error: anyhow::Error) -> anyhow::Error {
    let said = format!("{error:#}");
    // Only gh's status counts, not the path or the slug it quotes; a 403 for the rate limit is no refusal.
    if !matches!(http_status(&said), Some(403 | 404))
        || said.to_ascii_lowercase().contains("rate limit")
    {
        return error;
    }
    anyhow!(
        "{NO_ADMIN}: only the admins of {slug} see its webhooks, and gh's token needs the admin:repo_hook scope \
         (gh auth refresh -s admin:repo_hook). GitHub answered: {said}"
    )
}

/// The status gh ends its error with: `gh: Not Found (HTTP 404)`.
fn http_status(error: &str) -> Option<u16> {
    let at = error.rfind("(HTTP ")? + "(HTTP ".len();
    error.get(at..at + 3)?.parse().ok()
}

fn failure(what: &str, error: &str) -> anyhow::Error {
    if error.contains("gh auth login") {
        return anyhow!("gh is not signed in: run `gh auth login`");
    }
    anyhow!("{what} failed: {error}")
}

/// Whether a run concerns the user: they triggered it, or it is on the branch of one of their ticket runs.
pub fn concerns(run: &WorkflowRun, me: Option<&str>, branches: &[String]) -> bool {
    let by_me = |who: &Option<String>| {
        me.is_some_and(|me| who.as_deref().is_some_and(|w| w.eq_ignore_ascii_case(me)))
    };
    by_me(&run.actor)
        || by_me(&run.triggered_by)
        || run.branch.as_ref().is_some_and(|b| branches.contains(b))
}

/// The runs followed until they end, to tell the user when one passes or fails.
#[derive(Default, Debug)]
pub struct Watch {
    tracked: HashSet<(String, u64)>,
}

impl Watch {
    /// Takes runs just read: follows those still running that `wanted` accepts, and returns the followed ones
    /// that have now passed or failed. A cancelled or skipped run is dropped without a word.
    pub fn observe<'a>(
        &mut self,
        runs: impl IntoIterator<Item = &'a WorkflowRun>,
        wanted: impl Fn(&WorkflowRun) -> bool,
    ) -> Vec<WorkflowRun> {
        let mut finished = Vec::new();
        for run in runs {
            let key = (run.repo.clone(), run.id);
            if run.status != "completed" {
                if wanted(run) {
                    self.tracked.insert(key);
                }
            } else if self.tracked.remove(&key) && run.passed().is_some() {
                finished.push(run.clone());
            }
        }
        finished
    }

    /// `(owner/name, run id)` of each run followed.
    pub fn tracked(&self) -> Vec<(String, u64)> {
        self.tracked.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.tracked.clear();
    }
}

impl Ci {
    pub fn new(tickets: Tickets) -> Self {
        Self {
            tickets,
            cache: Mutex::default(),
            me: OnceLock::new(),
            old_gh: AtomicBool::new(false),
        }
    }

    fn gh(&self, args: &[&str], input: Option<&str>) -> Result<Answer> {
        let program = &self.tickets.gh;
        #[allow(unused_mut)]
        let mut cmd = Command::new(program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        // Launched from the Finder, TermF lacks the PATH of the shell profile (Homebrew…), which gh also needs.
        if program == Path::new("gh")
            && let Some(path) = termf_core::shell::login_path()
        {
            cmd.env("PATH", path);
        }
        cmd.args(args)
            .envs(self.tickets.env.iter().map(|(k, v)| (k, v)))
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
        Ok(Answer {
            ok: out.status.success(),
            out: String::from_utf8_lossy(&out.stdout).into_owned(),
            err: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        })
    }

    /// What gh printed, or why it failed.
    fn call(&self, args: &[&str], input: Option<&str>) -> Result<String> {
        let answer = self.gh(args, input)?;
        if !answer.ok {
            return Err(failure(&format!("gh {}", args.join(" ")), &answer.err));
        }
        Ok(answer.out)
    }

    /// A GET of the REST API, answered from the cache when GitHub says nothing changed (ETag): such an answer
    /// costs no rate limit, so polling stays cheap.
    fn get(&self, path: &str, fields: &[(&str, String)]) -> Result<Value> {
        let mut args: Vec<String> = ["api", "--include", "--method", "GET", path]
            .map(String::from)
            .to_vec();
        for (name, value) in fields {
            args.extend(["-f".into(), format!("{name}={value}")]);
        }
        let key = args.join(" ");
        let cached = self.cache.lock().unwrap().get(&key).cloned();
        if let Some((etag, _)) = &cached {
            args.extend(["-H".into(), format!("If-None-Match: {etag}")]);
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let answer = self.gh(&args, None)?;
        let out = answer.out.as_str();
        let (head, body) = out
            .split_once("\r\n\r\n")
            .or_else(|| out.split_once("\n\n"))
            .unwrap_or((out, ""));
        // gh exits with an error on 304, after printing the headers.
        if let (Some("304"), Some((_, value))) = (head.split_whitespace().nth(1), &cached) {
            return Ok(value.clone());
        }
        if !answer.ok {
            return Err(failure(&format!("gh api {path}"), &answer.err));
        }
        let value: Value =
            serde_json::from_str(body).with_context(|| format!("gh api {path} gave no JSON"))?;
        let etag = head.lines().find_map(|line| {
            let (name, etag) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("etag")
                .then(|| etag.trim().to_owned())
        });
        if let Some(etag) = etag {
            let mut cache = self.cache.lock().unwrap();
            // shortcut: forgets every answer past 256 queries; keep the most used ones if that ever shows.
            if cache.len() >= 256 {
                cache.clear();
            }
            cache.insert(key, (etag, value.clone()));
        }
        Ok(value)
    }

    /// Every item under `key`, page after page.
    fn all(&self, path: &str, key: &str, fields: &[(&str, String)]) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        for page in 1..=MAX_PAGES {
            let mut fields = fields.to_vec();
            fields.extend([("per_page", "100".into()), ("page", page.to_string())]);
            let answer = self.get(path, &fields)?;
            let found = answer[key].as_array().cloned().unwrap_or_default();
            let done = found.is_empty();
            items.extend(found);
            if done || items.len() as u64 >= answer["total_count"].as_u64().unwrap_or(0) {
                break;
            }
        }
        Ok(items)
    }

    /// The repository's `owner/name` on GitHub: its first GitHub remote. `owner/name` itself is taken too, as a
    /// check gives it: a pull request may live in another repository than the folder's first remote.
    pub fn slug(&self, repo: &str) -> Result<String> {
        if !Path::new(repo).is_dir() {
            ensure!(valid_slug(repo), "{repo} is not a folder");
            return Ok(repo.to_ascii_lowercase());
        }
        self.tickets.github_slugs(repo).into_iter().next().with_context(|| {
            format!(
                "{repo} has no GitHub remote: TermF follows GitHub Actions only for now (GitLab CI comes later)"
            )
        })
    }

    /// One page of a repository's runs, newest first.
    pub fn runs(&self, repo: &str, filter: &Filter) -> Result<RunPage> {
        self.runs_of(&self.slug(repo)?, filter)
    }

    fn runs_of(&self, slug: &str, filter: &Filter) -> Result<RunPage> {
        let mut fields = vec![
            ("per_page", PAGE.to_string()),
            ("page", filter.page.max(1).to_string()),
        ];
        let mut head = None;
        let given = |v: &Option<String>| {
            let v = v.as_deref().map(str::trim);
            v.filter(|v| !v.is_empty()).map(str::to_owned)
        };
        if let Some(pr) = filter.pr {
            let pull = self.get(&format!("repos/{slug}/pulls/{pr}"), &[])?;
            let sha =
                text(&pull["head"]["sha"]).with_context(|| format!("{slug}#{pr} has no commit"))?;
            fields.push(("head_sha", sha.clone()));
            head = Some(sha);
        } else if let Some(branch) = given(&filter.branch) {
            fields.push(("branch", branch));
        }
        if let Some(status) = given(&filter.status) {
            fields.push(("status", status));
        }
        let page = self.get(&format!("repos/{slug}/actions/runs"), &fields)?;
        let runs = page["workflow_runs"].as_array().into_iter().flatten();
        Ok(RunPage {
            repo: slug.to_owned(),
            runs: runs.map(|r| run_from(r, slug)).collect(),
            total: page["total_count"].as_u64().unwrap_or_default(),
            head,
        })
    }

    /// One run, without its jobs: what polling a running run needs.
    pub fn workflow_run(&self, slug: &str, id: u64) -> Result<WorkflowRun> {
        let run = self.get(&format!("repos/{slug}/actions/runs/{id}"), &[])?;
        Ok(run_from(&run, slug))
    }

    /// A run and the jobs of its latest attempt, with their steps.
    pub fn run(&self, repo: &str, id: u64) -> Result<RunDetail> {
        let slug = self.slug(repo)?;
        let jobs = self.all(
            &format!("repos/{slug}/actions/runs/{id}/jobs"),
            "jobs",
            &[("filter", "latest".into())],
        )?;
        Ok(RunDetail {
            run: self.workflow_run(&slug, id)?,
            jobs: jobs.iter().map(job_from).collect(),
        })
    }

    fn raw_log(&self, slug: &str, job: u64) -> Result<String> {
        let path = format!("repos/{slug}/actions/jobs/{job}/logs");
        // Recent gh refuses a body with terminal escapes (colored logs) without this flag, which older ones do not
        // know; the escapes are removed by `parse_log`.
        let mut args = vec!["api", &path];
        if !self.old_gh.load(Ordering::Relaxed) {
            args.push("--allow-escape-sequences");
        }
        let mut answer = self.gh(&args, None)?;
        if !answer.ok && answer.err.contains("unknown flag") && args.len() == 3 {
            self.old_gh.store(true, Ordering::Relaxed);
            args.pop();
            answer = self.gh(&args, None)?;
        }
        if answer.ok {
            return Ok(answer.out);
        }
        if answer.err.contains("HTTP 404") {
            bail!("the log of job {job} is not there yet: GitHub gives it once the job has ended");
        }
        Err(failure(&format!("gh {}", args.join(" ")), &answer.err))
    }

    /// A job's log, downloaded now: its last lines, and which of them are errors.
    pub fn log(&self, repo: &str, job: u64) -> Result<JobLog> {
        Ok(parse_log(&self.raw_log(&self.slug(repo)?, job)?))
    }

    /// Runs the run again: all its jobs, or only those that failed.
    pub fn rerun(&self, repo: &str, id: u64, failed_only: bool) -> Result<()> {
        let (slug, id) = (self.slug(repo)?, id.to_string());
        let mut args = vec!["run", "rerun", &id, "--repo", &slug];
        if failed_only {
            args.push("--failed");
        }
        self.call(&args, None).map(drop)
    }

    pub fn cancel(&self, repo: &str, id: u64) -> Result<()> {
        let (slug, id) = (self.slug(repo)?, id.to_string());
        self.call(&["run", "cancel", &id, "--repo", &slug], None)
            .map(drop)
    }

    pub fn workflows(&self, repo: &str) -> Result<Vec<Workflow>> {
        let slug = self.slug(repo)?;
        let found = self.all(&format!("repos/{slug}/actions/workflows"), "workflows", &[])?;
        Ok(found
            .iter()
            .map(|w| Workflow {
                id: w["id"].as_u64().unwrap_or_default(),
                name: text(&w["name"]).unwrap_or_default(),
                path: text(&w["path"]).unwrap_or_default(),
                state: text(&w["state"]).unwrap_or_default(),
            })
            .collect())
    }

    /// The inputs a workflow's manual trigger takes, read from its file at `git_ref` (else the default branch).
    /// Refused if the workflow has no `workflow_dispatch` trigger.
    pub fn inputs(&self, repo: &str, path: &str, git_ref: Option<&str>) -> Result<Vec<Input>> {
        parse_inputs(&self.workflow_file(repo, path, git_ref)?).with_context(|| path.to_owned())
    }

    /// The jobs of a workflow file at a commit, with what each one needs: the graph of a run.
    pub fn graph(&self, repo: &str, path: &str, sha: &str) -> Result<Vec<Node>> {
        parse_graph(&self.workflow_file(repo, path, Some(sha))?).with_context(|| path.to_owned())
    }

    fn workflow_file(&self, repo: &str, path: &str, git_ref: Option<&str>) -> Result<String> {
        ensure!(
            path.starts_with(".github/workflows/")
                && !path.contains("..")
                && !path.contains(['?', '#']),
            "{path} is not a workflow file"
        );
        let slug = self.slug(repo)?;
        let mut args = vec![
            "api".to_owned(),
            "--method".into(),
            "GET".into(),
            "-H".into(),
            "Accept: application/vnd.github.raw".into(),
            format!("repos/{slug}/contents/{path}"),
        ];
        if let Some(git_ref) = git_ref.map(str::trim).filter(|r| !r.is_empty()) {
            args.extend(["-f".into(), format!("ref={git_ref}")]);
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.call(&args, None)
    }

    /// The repository's webhooks. GitHub shows them only to its admins, and to a gh token with the
    /// `admin:repo_hook` scope: a refusal says so, starting with [`NO_ADMIN`].
    pub fn hooks(&self, repo: &str) -> Result<Vec<Hook>> {
        let slug = self.slug(repo)?;
        let found = self
            .get(
                &format!("repos/{slug}/hooks"),
                &[("per_page", "100".into())],
            )
            .map_err(|e| admin(&slug, e))?;
        let hooks = found.as_array().into_iter().flatten();
        Ok(hooks
            .map(|h| Hook {
                id: h["id"].as_u64().unwrap_or_default(),
                url: text(&h["config"]["url"]).unwrap_or_default(),
                events: h["events"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(text)
                    .collect(),
                active: h["active"].as_bool().unwrap_or(false),
                last_code: h["last_response"]["code"].as_u64(),
                last_status: text(&h["last_response"]["status"]),
                updated: text(&h["updated_at"]).unwrap_or_default(),
            })
            .collect())
    }

    /// A webhook's last 30 deliveries, newest first.
    pub fn deliveries(&self, repo: &str, hook: u64) -> Result<Vec<Delivery>> {
        let slug = self.slug(repo)?;
        let found = self.get(
            &format!("repos/{slug}/hooks/{hook}/deliveries"),
            &[("per_page", "30".into())],
        )?;
        let deliveries = found.as_array().into_iter().flatten();
        Ok(deliveries
            .map(|d| Delivery {
                id: d["id"].as_u64().unwrap_or_default(),
                event: text(&d["event"]).unwrap_or_default(),
                action: text(&d["action"]),
                code: d["status_code"].as_u64().unwrap_or_default(),
                status: text(&d["status"]).unwrap_or_default(),
                redelivery: d["redelivery"].as_bool().unwrap_or(false),
                duration: d["duration"].as_f64().unwrap_or_default(),
                delivered: text(&d["delivered_at"]).unwrap_or_default(),
            })
            .collect())
    }

    /// Asks GitHub to send a delivery again.
    pub fn redeliver(&self, repo: &str, hook: u64, delivery: u64) -> Result<()> {
        let slug = self.slug(repo)?;
        let path = format!("repos/{slug}/hooks/{hook}/deliveries/{delivery}/attempts");
        self.call(&["api", "--method", "POST", &path], None)
            .map(drop)
    }

    /// Adds a webhook that GitHub calls with JSON on `events`. The UI asks the user first.
    pub fn create_hook(&self, repo: &str, url: &str, events: &[String]) -> Result<Hook> {
        let slug = self.slug(repo)?;
        check_hook(url, events)?;
        let body = serde_json::json!({
            "name": "web",
            "active": true,
            "events": events,
            "config": { "url": url, "content_type": "json" },
        });
        let path = format!("repos/{slug}/hooks");
        let out = self.call(
            &["api", "--method", "POST", &path, "--input", "-"],
            Some(&body.to_string()),
        )?;
        let h: Value = serde_json::from_str(&out).context("GitHub gave no webhook back")?;
        Ok(Hook {
            id: h["id"].as_u64().unwrap_or_default(),
            url: text(&h["config"]["url"]).unwrap_or_default(),
            events: h["events"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(text)
                .collect(),
            active: h["active"].as_bool().unwrap_or(false),
            last_code: None,
            last_status: None,
            updated: text(&h["updated_at"]).unwrap_or_default(),
        })
    }

    /// Changes a webhook's address and events. Its secret and content type stay: only the config's `url` is
    /// sent, to the config's own endpoint. The UI asks the user first.
    pub fn update_hook(&self, repo: &str, hook: u64, url: &str, events: &[String]) -> Result<()> {
        let slug = self.slug(repo)?;
        check_hook(url, events)?;
        let path = format!("repos/{slug}/hooks/{hook}");
        let body = serde_json::json!({ "events": events });
        self.call(
            &["api", "--method", "PATCH", &path, "--input", "-"],
            Some(&body.to_string()),
        )?;
        let body = serde_json::json!({ "url": url });
        self.call(
            &["api", "--method", "PATCH", &format!("{path}/config"), "--input", "-"],
            Some(&body.to_string()),
        )
        .map(drop)
    }

    /// Deletes a webhook, which cannot be undone. The UI asks the user first.
    pub fn delete_hook(&self, repo: &str, hook: u64) -> Result<()> {
        let slug = self.slug(repo)?;
        let path = format!("repos/{slug}/hooks/{hook}");
        self.call(&["api", "--method", "DELETE", &path], None)
            .map(drop)
    }

    /// Starts a workflow by hand (`workflow_dispatch`), on `git_ref` or the default branch.
    pub fn dispatch(
        &self,
        repo: &str,
        workflow: &str,
        git_ref: Option<&str>,
        inputs: &BTreeMap<String, String>,
    ) -> Result<()> {
        let slug = self.slug(repo)?;
        let mut args = vec![
            "workflow".to_owned(),
            "run".into(),
            workflow.into(),
            "--repo".into(),
            slug,
        ];
        if let Some(git_ref) = git_ref.map(str::trim).filter(|r| !r.is_empty()) {
            args.push(format!("--ref={git_ref}"));
        }
        // On stdin, so no value can pass for an option.
        let json = (!inputs.is_empty())
            .then(|| serde_json::to_string(inputs))
            .transpose()?;
        if json.is_some() {
            args.push("--json".into());
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.call(&args, json.as_deref()).map(drop)
    }

    /// The signed-in GitHub user, asked once.
    // shortcut: a failure is kept too, until TermF restarts; ask again if signing in while it runs matters.
    pub fn me(&self) -> Option<String> {
        self.me
            .get_or_init(|| {
                let login = self.call(&["api", "user", "--jq", ".login"], None).ok()?;
                Some(login.trim().to_owned()).filter(|l| !l.is_empty())
            })
            .clone()
    }

    /// The branches of the ticket runs still open.
    pub fn ticket_branches(&self) -> Vec<String> {
        let runs = self.tickets.runs().unwrap_or_default().into_iter();
        runs.filter(|r| matches!(r.status, RunStatus::Running | RunStatus::Delivered))
            .filter_map(|r| r.branch)
            .collect()
    }

    /// The runs on the last commit of a ticket run's pull request: its checks.
    pub fn checks(&self, run: i64) -> Result<Vec<WorkflowRun>> {
        let (slug, pr) = self.pull_request(run)?;
        let filter = Filter {
            pr: Some(pr),
            ..Filter::default()
        };
        Ok(self.runs_of(&slug, &filter)?.runs)
    }

    fn pull_request(&self, run: i64) -> Result<(String, u64)> {
        let run = self.tickets.run(run)?;
        let url = run
            .pr_url
            .with_context(|| format!("run {} has no pull request yet", run.id))?;
        pull_request_of(&url)
    }

    /// Writes, in the folder the run's session reads, why a job of its pull request's CI failed: the job, its
    /// failed step and the end of its log, framed as data. The user sends `prompt` to the session.
    pub fn failure_message(&self, run: i64, job: u64) -> Result<AgentMessage> {
        let (slug, pr) = self.pull_request(run)?;
        let info = self.get(&format!("repos/{slug}/actions/jobs/{job}"), &[])?;
        let source = format!(
            "https://github.com/{slug}/actions/runs/{}/job/{job}",
            info["run_id"].as_u64().unwrap_or_default()
        );
        let workflow = text(&info["workflow_name"]).unwrap_or_default();
        let log = parse_log(&self.raw_log(&slug, job)?);
        let pr_url = format!("https://github.com/{slug}/pull/{pr}");
        let text = log::failure_text(&pr_url, &source, &workflow, &job_from(&info), &log);
        let dir = self.tickets.brief_dir(run)?;
        fs::create_dir_all(&dir)?;
        let file = dir.join(format!("ci-{job}.md"));
        fs::write(&file, &text)?;
        Ok(AgentMessage {
            prompt: format!(
                "Read {} now: the CI of your pull request failed, and this file tells where. Fix it in this \
                 worktree, then commit.",
                file.display()
            ),
            file: file.to_string_lossy().into_owned(),
            text,
        })
    }
}
