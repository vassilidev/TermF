//! A run's git side: its worktree and branch, its delivery as a pull request, and its cleanup. Agents never
//! push: only `deliver`, which the user confirms after seeing what it sends, does.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use termf_core::TicketWorktrees;
use termf_ssh::Ssh;

use crate::store::{Event, Run, RunEvent, RunStatus, Store};
use crate::{Ticket, Tickets, github, gitlab};

/// A run ready for its session.
pub struct Launch {
    pub run: Run,
    /// The file that holds the session's task: the rules, then the ticket framed as data.
    pub brief: PathBuf,
    /// What the worktree's setup gives the session: its ports, its Compose project name.
    pub env: BTreeMap<String, String>,
    /// What Claude Code takes first: `--session-id <id>` and the first message for a new run, `--resume <id>`
    /// for one taken up again.
    pub args: Vec<String>,
    /// A doubt to tell the user before the session starts.
    pub warning: Option<String>,
}

/// What a check of the delivered runs' pull requests found.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct PrCheck {
    /// The runs whose pull request was merged or closed since the last check.
    pub cleanups: Vec<Cleanup>,
    /// The runs whose pull request GitHub could not tell about: asked again at the next check.
    pub errors: Vec<CheckError>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CheckError {
    pub run: i64,
    pub message: String,
}

/// A run whose pull request was merged or closed.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cleanup {
    pub run: Run,
    /// TermF closed it (`[tickets] cleanup = "auto"`); otherwise the user does.
    pub cleaned: bool,
    /// Why TermF could not close it.
    pub error: Option<String>,
}

/// What GitHub says of a delivered run's open pull request against its base.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PrMerge {
    /// `MERGEABLE`, `CONFLICTING`, or `UNKNOWN` while GitHub works it out.
    pub mergeable: String,
    /// GitHub's `mergeStateStatus`: `CLEAN`, `BEHIND`, `DIRTY`, `BLOCKED`, `UNSTABLE`, `HAS_HOOKS`, `DRAFT`, `UNKNOWN`.
    pub state: String,
    /// It conflicts with its base.
    pub conflict: bool,
    /// Its base moved on since its branch left it.
    pub behind: bool,
}

/// What delivering a run would send, for the user to check first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryPreview {
    /// The worktree's changes, committed by the delivery.
    pub files: Vec<FileChange>,
    /// Commits already made in the worktree and not pushed yet.
    pub commits: Vec<CommitSummary>,
    /// Given back to `deliver`, which refuses if the worktree changed since.
    pub fingerprint: String,
    /// The repository's origin is on a GitLab: the delivery opens a merge request.
    #[serde(default)]
    pub merge_request: bool,
}

/// Where a delivery's request goes: a pull request on GitHub (`owner/name`), or a merge request on a GitLab
/// (its host and `group/project`).
enum Forge {
    GitHub(String),
    GitLab(String, String),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub change: Change,
    /// The former path of a renamed file.
    pub from: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitSummary {
    pub commit: String,
    pub subject: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub run: Run,
    /// Something that failed once the pull request was open, such as commenting on the ticket.
    pub warning: Option<String>,
}

/// A delivery's preview, and the index that holds exactly what it would commit.
struct Snapshot {
    preview: DeliveryPreview,
    index: PathBuf,
}

struct Git<'a>(&'a [(String, String)]);

impl Git<'_> {
    fn output(&self, dir: &Path, args: &[&str]) -> Result<Output> {
        crate::command("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .envs(self.0.iter().map(|(k, v)| (k, v)))
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .output()
            .context("cannot run git: is it installed?")
    }

    /// The output as is, for `-z` listings whose names may start or end with spaces.
    fn raw(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let out = self.output(dir, args)?;
        ensure!(
            out.status.success(),
            "git {} failed: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn run(&self, dir: &Path, args: &[&str]) -> Result<String> {
        Ok(self.raw(dir, args)?.trim().to_owned())
    }

    fn ok(&self, dir: &Path, args: &[&str]) -> bool {
        self.output(dir, args).is_ok_and(|o| o.status.success())
    }

    fn has(&self, dir: &Path, reference: &str) -> bool {
        let commit = format!("{reference}^{{commit}}");
        self.ok(dir, &["rev-parse", "--verify", "-q", &commit])
    }

    fn count(&self, dir: &Path, range: &str) -> Result<u32> {
        Ok(self.run(dir, &["rev-list", "--count", range])?.parse()?)
    }
}

/// The URL of each remote of a repository.
pub(crate) fn remote_urls(repo: &str, env: &[(String, String)]) -> Vec<String> {
    let urls = Git(env).run(
        Path::new(repo),
        &["config", "--get-regexp", r"^remote\..*\.url$"],
    );
    let urls = urls.unwrap_or_default();
    let urls = urls.lines().filter_map(|l| l.split_once(' '));
    urls.map(|(_, url)| url.to_owned()).collect()
}

/// The `owner/name` of each remote on GitHub, lowercased.
pub(crate) fn github_slugs_of(urls: &[String], ssh: &Ssh) -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    for url in urls {
        if let Ok(slug) = github::remote_slug(url, ssh)
            && !slugs.contains(&slug)
        {
            slugs.push(slug);
        }
    }
    slugs
}

/// The `owner/name` of each GitHub remote of a repository, lowercased.
pub(crate) fn remote_slugs(repo: &str, env: &[(String, String)], ssh: &Ssh) -> Vec<String> {
    github_slugs_of(&remote_urls(repo, env), ssh)
}

/// `fix-the-login-button`: ASCII only, at most 40 characters, so it suits any branch name.
pub(crate) fn slug(text: &str) -> String {
    let words: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let mut slug = words.join("-").to_ascii_lowercase();
    slug.truncate(40);
    slug.trim_end_matches('-').to_owned()
}

/// `termf/<ticket>-<title>`. An MCP source's key comes from any tracker: only what a branch name always takes
/// stays of it, and the run's id stands in when nothing does.
pub(crate) fn branch_stem(key: &str, title: &str, run: i64) -> String {
    let id: String = key
        .rsplit('#')
        .next()
        .unwrap_or(key)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let id = match id.trim_matches(['-', '_']) {
        "" => run.to_string(),
        id => id.to_owned(),
    };
    match slug(title) {
        s if s.is_empty() => format!("termf/{id}"),
        s => format!("termf/{id}-{s}"),
    }
}

/// Hides the repository's worktrees from git, and from the tools that read its ignores, in its local
/// `info/exclude`: never committed, unlike `.gitignore`. Only ever appended to. `repo` may be a folder of the
/// git repository: the pattern is anchored at its root.
fn exclude_worktrees(git: &Git, repo: &Path) -> Result<()> {
    let prefix = git.run(repo, &["rev-parse", "--show-prefix"])?;
    // Taken literally: a folder's name may hold what gitignore reads as a pattern.
    let prefix: String = prefix
        .chars()
        .flat_map(|c| ["\\*?[".contains(c).then_some('\\'), Some(c)])
        .flatten()
        .collect();
    let line = format!("/{prefix}.claude/worktrees/");
    let file = repo.join(git.run(repo, &["rev-parse", "--git-path", "info/exclude"])?);
    let text = match fs::read(&file) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", file.display())),
    };
    if text.lines().any(|l| l.trim() == line) {
        return Ok(());
    }
    fs::create_dir_all(file.parent().context("no parent folder")?)?;
    let newline = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .and_then(|mut f| writeln!(f, "{newline}{line}"))
        .with_context(|| format!("cannot write {}", file.display()))
}

/// The branch runs start from: the remote's default branch, else the usual names.
fn base_ref(git: &Git, repo: &Path) -> Result<String> {
    if let Ok(head) = git.run(
        repo,
        &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"],
    ) {
        return Ok(head);
    }
    ["origin/main", "origin/master", "main", "master"]
        .into_iter()
        .find(|r| git.has(repo, r))
        .map(str::to_owned)
        .with_context(|| format!("cannot tell the base branch of {}", repo.display()))
}

/// Why a run was not closed: work that closing would lose. The UI reads it as an error of its own.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Unpushed {
    pub run: i64,
    /// Changes not committed in the worktree.
    pub uncommitted: bool,
    /// Commits on the run's branch that no delivery pushed.
    pub commits: u32,
}

impl Unpushed {
    fn lost(&self) -> Vec<String> {
        let mut lost = Vec::new();
        if self.uncommitted {
            lost.push("changes not committed".to_owned());
        }
        match self.commits {
            0 => {}
            1 => lost.push("1 commit not pushed".to_owned()),
            n => lost.push(format!("{n} commits not pushed")),
        }
        lost
    }
}

impl std::fmt::Display for Unpushed {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "run {} holds work that is not pushed ({}): deliver it, or close it by force",
            self.run,
            self.lost().join(", ")
        )
    }
}

impl std::error::Error for Unpushed {}

/// What closing would lose: changes not committed in the worktree, commits not pushed on the branch.
fn unpushed(
    git: &Git,
    run: i64,
    repo: &Path,
    worktree: &Path,
    branch: &str,
    since: &str,
) -> Result<Unpushed> {
    let uncommitted =
        worktree.exists() && !git.run(worktree, &["status", "--porcelain"])?.is_empty();
    let branch = format!("refs/heads/{branch}");
    let commits = match git.has(repo, &branch) {
        true => git.count(repo, &format!("{since}..{branch}"))?,
        false => 0,
    };
    Ok(Unpushed {
        run,
        uncommitted,
        commits,
    })
}

/// The repository a run of `resolution` works in: `repo` (a name or a path), else the one that is clear.
fn chosen_repo(resolution: &crate::Resolution, repo: Option<&str>) -> Result<crate::RepoChoice> {
    match repo.or(resolution.repo.as_deref()) {
        Some(wanted) => resolution
            .repos
            .iter()
            .find(|r| {
                r.name.eq_ignore_ascii_case(wanted) || Path::new(&r.path) == Path::new(wanted)
            })
            .cloned()
            .with_context(|| format!("no repository {wanted} in project {}", resolution.project)),
        None => {
            let names: Vec<&str> = resolution.repos.iter().map(|r| r.name.as_str()).collect();
            bail!(
                "project {} has several repositories: choose one of {}",
                resolution.project,
                names.join(", ")
            )
        }
    }
}

/// Whether delivering `run`, of a change made of `family`, moves its ticket: once for the whole change (`moved`: a
/// run of it did), by the run of the change's own repository, or by the first pull request of another repository
/// while the change's own has none (its work may all be elsewhere).
fn moves_ticket(run: &Run, family: &[Run], moved: bool) -> bool {
    if moved {
        return false;
    }
    run.parent.is_none()
        || family
            .iter()
            .find(|r| r.parent.is_none())
            .is_none_or(|own| own.pr_url.is_none())
}

/// A conversation id as Claude Code makes them, a UUID: nothing a path could be built from.
fn conversation_id(text: &str) -> bool {
    text.len() == 36 && text.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// The commit last pushed by a delivery of the run.
fn last_push(store: &Store, id: i64) -> Result<Option<String>> {
    let mut events = store.events(id)?.into_iter().rev();
    Ok(events.find_map(|e| match e.event {
        Event::Pushed { commit, .. } => Some(commit),
        _ => None,
    }))
}

/// `git diff --name-status -z` lines.
fn file_changes(listing: &str) -> Vec<FileChange> {
    let mut fields = listing.split('\0').filter(|f| !f.is_empty());
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let (change, two_paths) = match status.chars().next() {
            Some('A') => (Change::Added, false),
            Some('C') => (Change::Added, true),
            Some('D') => (Change::Deleted, false),
            Some('R') => (Change::Renamed, true),
            _ => (Change::Modified, false),
        };
        let from = two_paths.then(|| fields.next()).flatten();
        let Some(path) = fields.next() else { break };
        files.push(FileChange {
            path: path.to_owned(),
            change,
            from: from
                .filter(|_| change == Change::Renamed)
                .map(str::to_owned),
        });
    }
    files
}

impl Tickets {
    /// Resolves the ticket, reads it, and prepares its run: a worktree on a new branch `termf/<ticket>-<slug>`
    /// from the repository's base branch, and the brief of its session. `repo` (a name or a path) picks the
    /// repository when the project has several.
    pub fn start(&self, input: &str, project: Option<&str>, repo: Option<&str>) -> Result<Launch> {
        self.open(input, project, repo, true)
    }

    /// Prepares the run as `start` does, for sessions other than its own (its agent team): no Claude Code
    /// conversation is kept, so the run has none to resume.
    pub fn start_without_session(
        &self,
        input: &str,
        project: Option<&str>,
        repo: Option<&str>,
    ) -> Result<Run> {
        Ok(self.open(input, project, repo, false)?.run)
    }

    fn open(
        &self,
        input: &str,
        project: Option<&str>,
        repo: Option<&str>,
        session: bool,
    ) -> Result<Launch> {
        let resolution = self.resolve(input, project)?;
        let chosen = chosen_repo(&resolution, repo)?;
        let ticket = self
            .provider(&resolution.project, &resolution.ticket.provider)?
            .fetch(&resolution.ticket)?;
        self.open_run(&resolution.project, &chosen, &ticket, session, None)
    }

    /// The `owner/name` on GitHub of repository `name` of `project`, where its pull request opens: refused, saying
    /// why, when it has no `origin` or its `origin` is not on GitHub.
    pub fn github_repo(&self, project: &str, name: &str) -> Result<String> {
        let info = self.projects(Some(project))?.remove(0);
        let repo = info
            .repos
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
            .with_context(|| format!("no repository {name} in project {project}"))?;
        let origin = Git(&self.env)
            .run(Path::new(&repo.path), &["config", "--get", "remote.origin.url"])
            .map_err(|_| {
                anyhow::anyhow!(
                    "repository {name} has no remote origin: a run that spans several repositories pushes the branch of \
                     each to its origin and opens its pull request there"
                )
            })?;
        github::remote_slug(&origin, &Ssh::new(self.home.clone())).map_err(|_| {
            anyhow::anyhow!(
                "repository {name} is not on GitHub (its origin is {origin}): a run that spans several repositories \
                 opens a pull request in each, linked together"
            )
        })
    }

    /// Opens the run of repository `name` of the project for run `parent`, whose team's plan spans several: a
    /// worktree on a branch of its own from its base, prepared as any run's, without a session; delivered and
    /// cleaned up on its own, its pull request linked to the others. Refused when it is not on GitHub; the one already
    /// open comes back.
    pub fn open_repo(&self, parent: i64, name: &str) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        let main = store.run(parent)?;
        ensure!(
            main.parent.is_none(),
            "run {parent} works for run {}: open the repository from that one",
            main.parent.unwrap_or_default()
        );
        let info = self.projects(Some(&main.project))?.remove(0);
        let repo = info
            .repos
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(name))
            .with_context(|| format!("no repository {name} in project {}", main.project))?;
        // One whose worktree never came (TermF quit while preparing it) is not open.
        let open = store.children(parent)?.into_iter().find(|c| {
            Path::new(&c.repo) == Path::new(&repo.path)
                && matches!(c.status, RunStatus::Running | RunStatus::Delivered)
                && c.worktree.as_deref().is_some_and(|w| Path::new(w).is_dir())
        });
        if let Some(open) = open {
            return Ok(open);
        }
        self.github_repo(&main.project, name)?;
        let ticket = Ticket {
            reference: main.ticket.clone(),
            title: main.title.clone(),
            body: String::new(),
            state: "open".into(),
            author: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            comments: Vec::new(),
            links: Vec::new(),
        };
        let chosen = crate::RepoChoice {
            name: repo.name.clone(),
            path: repo.path.clone(),
        };
        Ok(self
            .open_run(&main.project, &chosen, &ticket, false, Some(parent))?
            .run)
    }

    /// The runs of the other repositories of run `id`, oldest first.
    pub fn children(&self, id: i64) -> Result<Vec<Run>> {
        Store::open(&self.data_dir)?.children(id)
    }

    /// Prepares a run, as `start` does, for a request the user wrote in TermF instead of a tracker's ticket: its
    /// ticket is the request, of the provider `request`, with no address; delivering it touches no tracker.
    pub fn start_request(&self, text: &str, project: &str, repo: Option<&str>) -> Result<Run> {
        let text = text.trim();
        ensure!(!text.is_empty(), "say what the request is");
        let reference = crate::TicketRef {
            provider: crate::REQUEST.into(),
            key: crate::REQUEST.into(),
            url: String::new(),
        };
        let info = self.projects(Some(project))?.remove(0);
        let chosen = chosen_repo(&info.resolution(reference.clone(), None), repo)?;
        let first = text.lines().next().unwrap_or_default().trim();
        let ticket = Ticket {
            reference,
            title: crate::truncate(first, 80),
            body: text.to_owned(),
            state: "open".into(),
            author: None,
            labels: Vec::new(),
            assignees: Vec::new(),
            comments: Vec::new(),
            links: crate::links([text], ""),
        };
        Ok(self.open_run(project, &chosen, &ticket, false, None)?.run)
    }

    /// The run of `ticket` in `project`'s repository `chosen`: its worktree, its brief, its preparation; `parent`:
    /// the run it works for, in another repository.
    fn open_run(
        &self,
        project: &str,
        chosen: &crate::RepoChoice,
        ticket: &Ticket,
        session: bool,
        parent: Option<i64>,
    ) -> Result<Launch> {
        let store = Store::open(&self.data_dir)?;
        let mut run = store.create(project, ticket, &chosen.path)?;
        if let Some(parent) = parent {
            store.set_parent(run.id, parent)?;
            run.parent = Some(parent);
        }
        let conversation = uuid::Uuid::new_v4().to_string();
        let (path, branch, base) = match self.worktree(&run, &chosen.name, ticket) {
            Ok(created) => created,
            Err(e) => {
                store.fail(run.id, &format!("{e:#}"))?;
                return Err(e);
            }
        };
        run.worktree = Some(path.clone());
        run.branch = Some(branch.clone());
        run.base = Some(base.clone());
        store.update(&mut run)?;
        if session {
            store.set_conversation(run.id, &conversation)?;
            run.claude_session = Some(conversation.clone());
        }
        // Its own folder: the session may read it, nothing else of the run.
        let brief = store.run_dir(run.id).join("brief").join("prompt.md");
        fs::create_dir_all(brief.parent().context("no parent folder")?)?;
        fs::write(&brief, crate::prompt(ticket, &branch, &base))?;
        let worktree = PathBuf::from(&path);
        store.record(run.id, &Event::WorktreeCreated { path, branch, base })?;
        let report = self.setup().run(&termf_worktree::Worktree {
            project: run.project.clone(),
            repo: chosen.name.clone(),
            source: PathBuf::from(&chosen.path),
            path: worktree,
            id: run.id.to_string(),
        });
        store.record(
            run.id,
            &Event::Setup {
                steps: report.steps,
                saved: report.saved,
            },
        )?;
        let kickoff = crate::kickoff(&brief);
        Ok(Launch {
            run,
            brief,
            env: report.env,
            args: vec!["--session-id".into(), conversation, kickoff],
            warning: None,
        })
    }

    /// Takes a run's Claude Code conversation up again in a new session, once TermF restarted or the session
    /// ended: the worktree as its setup left it, the same brief, the conversation resumed. Refused when the run
    /// is over or its worktree gone. Whether Claude Code still has the conversation is only a guess here (the
    /// agent's shell may move Claude Code's folder): a warning, and the session itself says so.
    pub fn resume(&self, id: i64) -> Result<Launch> {
        let store = Store::open(&self.data_dir)?;
        let run = store.run(id)?;
        ensure!(
            matches!(run.status, RunStatus::Running | RunStatus::Delivered),
            "run {id} is {}: there is nothing to resume",
            run.status.as_str()
        );
        let worktree = run.worktree.clone().unwrap_or_default();
        ensure!(
            !worktree.is_empty() && Path::new(&worktree).is_dir(),
            "the worktree of run {id} is gone: close the run"
        );
        let conversation = run.claude_session.clone().with_context(|| {
            format!("run {id} started before TermF kept its Claude Code conversation: it cannot be resumed")
        })?;
        let warning = self.transcript(&conversation).is_none().then(|| {
            format!(
                "Claude Code may no longer have the conversation of run {id} ({conversation}): it is not in {}. \
                 If so, the new session says so; then close the run, or open a session in its worktree",
                self.claude_dir.join("projects").display()
            )
        });
        Ok(Launch {
            env: self.setup().env(Path::new(&worktree)),
            brief: store.run_dir(id).join("brief").join("prompt.md"),
            args: vec!["--resume".into(), conversation],
            warning,
            run,
        })
    }

    /// Claude Code's file of a conversation: `<claude dir>/projects/<folder>/<id>.jsonl`, in any folder.
    // shortcut: TermF's `$CLAUDE_CONFIG_DIR`, which the agent's shell may change; only a hint for that reason.
    fn transcript(&self, conversation: &str) -> Option<PathBuf> {
        if !conversation_id(conversation) {
            return None;
        }
        let folders = fs::read_dir(self.claude_dir.join("projects")).ok()?;
        folders
            .flatten()
            .map(|f| f.path().join(format!("{conversation}.jsonl")))
            .find(|file| file.is_file())
    }

    /// The conversation the run's session is now in: `/clear` starts a new one, which `resume` must take.
    pub fn conversation(&self, id: i64, conversation: &str) -> Result<()> {
        ensure!(
            conversation_id(conversation),
            "{conversation:?} is not a Claude Code conversation"
        );
        let store = Store::open(&self.data_dir)?;
        store.run(id)?;
        store.set_conversation(id, conversation)
    }

    pub fn session_resumed(&self, id: i64, session: u32, conversation: &str) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        let conversation = conversation.to_owned();
        store.record(
            id,
            &Event::SessionResumed {
                session,
                conversation,
            },
        )?;
        store.run(id)
    }

    /// Creates `<repo>/.claude/worktrees/termf-<run>-<branch>`, or `<data>/worktrees/<project>/<run>/<repo>` as
    /// `[tickets] worktrees` says, and returns its path, its branch and its base.
    fn worktree(&self, run: &Run, name: &str, ticket: &Ticket) -> Result<(String, String, String)> {
        let settings = termf_core::load_settings(&self.config_dir).unwrap_or_default();
        let place = settings.tickets.worktrees;
        let git = Git(&self.env);
        let repo = Path::new(&run.repo);
        // Offline, the run starts from what was last fetched.
        let _ = git.output(repo, &["fetch", "-q", "origin"]);
        let base = base_ref(&git, repo)?;
        let stem = branch_stem(&ticket.reference.key, &ticket.title, run.id);
        let branch = (1..)
            .map(|n| match n {
                1 => stem.clone(),
                n => format!("{stem}-{n}"),
            })
            // A branch pushed by an earlier run of the ticket counts too, even once its run is closed.
            .find(|b| {
                !git.has(repo, &format!("refs/heads/{b}"))
                    && !git.has(repo, &format!("refs/remotes/origin/{b}"))
            })
            .unwrap();
        let path = match place {
            TicketWorktrees::Repo => {
                exclude_worktrees(&git, repo)?;
                let name = branch.strip_prefix("termf/").unwrap_or(&branch);
                let folder = repo.join(".claude").join("worktrees");
                folder.join(format!("termf-{}-{name}", run.id))
            }
            TicketWorktrees::Data => self.data_worktrees(run).join(match slug(name) {
                s if s.is_empty() => "repo".to_owned(),
                s => s,
            }),
        };
        fs::create_dir_all(path.parent().context("no parent folder")?)?;
        let path = path.to_string_lossy().into_owned();
        git.run(
            repo,
            &[
                "worktree",
                "add",
                "-q",
                "--no-track",
                "-b",
                &branch,
                &path,
                &base,
            ],
        )?;
        Ok((path, branch, base))
    }

    fn data_worktrees(&self, run: &Run) -> PathBuf {
        let projects = self.data_dir.join("worktrees");
        projects.join(&run.project).join(run.id.to_string())
    }

    pub fn run(&self, id: i64) -> Result<Run> {
        Store::open(&self.data_dir)?.run(id)
    }

    /// Newest first.
    pub fn runs(&self) -> Result<Vec<Run>> {
        Store::open(&self.data_dir)?.runs()
    }

    pub fn events(&self, id: i64) -> Result<Vec<RunEvent>> {
        let store = Store::open(&self.data_dir)?;
        store.run(id)?;
        store.events(id)
    }

    pub fn session_opened(&self, id: i64, session: u32) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        store.record(id, &Event::SessionOpened { session })?;
        store.run(id)
    }

    pub fn fail(&self, id: i64, error: &str) -> Result<Run> {
        Store::open(&self.data_dir)?.fail(id, error)
    }

    /// The `owner/name` of each GitHub remote of a repository, lowercased.
    pub fn github_slugs(&self, repo: &str) -> Vec<String> {
        remote_slugs(repo, &self.env, &Ssh::new(self.home.clone()))
    }

    /// The folder a run's session may read: its task, and what is sent to it later.
    pub fn brief_dir(&self, id: i64) -> Result<PathBuf> {
        let store = Store::open(&self.data_dir)?;
        store.run(id)?;
        Ok(store.run_dir(id).join("brief"))
    }

    /// What `deliver` would commit and push, and the fingerprint that confirms it.
    pub fn deliver_preview(&self, id: i64) -> Result<DeliveryPreview> {
        let store = Store::open(&self.data_dir)?;
        let snapshot = self.snapshot(&store, &store.run(id)?)?;
        let _ = fs::remove_file(&snapshot.index);
        Ok(snapshot.preview)
    }

    /// Takes the worktree as `git add -A` would, in an index of its own: the real one is left alone until the
    /// user confirms.
    fn snapshot(&self, store: &Store, run: &Run) -> Result<Snapshot> {
        let id = run.id;
        ensure!(
            matches!(run.status, RunStatus::Running | RunStatus::Delivered),
            "run {id} is {}: there is nothing to deliver",
            run.status.as_str()
        );
        let (Some(worktree), Some(branch), Some(base)) = (&run.worktree, &run.branch, &run.base)
        else {
            bail!("run {id} has no worktree");
        };
        let (git, dir) = (Git(&self.env), Path::new(worktree));
        let head = git
            .run(dir, &["symbolic-ref", "-q", "HEAD"])
            .unwrap_or_default();
        if head != format!("refs/heads/{branch}") {
            let now = match head.strip_prefix("refs/heads/") {
                Some(other) => format!("on {other}"),
                None => "detached".to_owned(),
            };
            bail!(
                "the worktree of run {id} is {now}, not on its branch {branch}: `git switch {branch}` there first"
            );
        }
        let index = store.run_dir(id).join("delivery.index");
        fs::create_dir_all(index.parent().context("no parent folder")?)?;
        let real = git.run(
            dir,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        )?;
        fs::copy(&real, &index).with_context(|| format!("cannot copy {real}"))?;
        let mut env = self.env.clone();
        env.push(("GIT_INDEX_FILE".into(), index.to_string_lossy().into()));
        let staged = Git(&env);
        staged.run(dir, &["add", "-A"])?;
        let tree = staged.run(dir, &["write-tree"])?;
        let files = file_changes(&staged.raw(
            dir,
            &["diff", "--cached", "--name-status", "-z", "-M", "HEAD"],
        )?);
        let since = last_push(store, id)?.unwrap_or_else(|| base.clone());
        let log = git.run(
            dir,
            &["log", "--format=%H%x1f%s", &format!("{since}..HEAD")],
        )?;
        let commits = log
            .lines()
            .filter_map(|line| line.split_once('\x1f'))
            .map(|(commit, subject)| CommitSummary {
                commit: commit.into(),
                subject: subject.into(),
            })
            .collect();
        let head = git.run(dir, &["rev-parse", "HEAD"])?;
        let origin = git.run(dir, &["config", "--get", "remote.origin.url"]);
        let merge_request =
            origin.is_ok_and(|o| matches!(self.forge(&run.project, &o), Ok(Forge::GitLab(..))));
        Ok(Snapshot {
            preview: DeliveryPreview {
                files,
                commits,
                fingerprint: format!("{head}:{tree}"),
                merge_request,
            },
            index,
        })
    }

    /// Where the request of a repository whose origin is `origin` goes.
    fn forge(&self, project: &str, origin: &str) -> Result<Forge> {
        let ssh = termf_ssh::Ssh::new(self.home.clone());
        let github = match github::remote_slug(origin, &ssh) {
            Ok(repo) => return Ok(Forge::GitHub(repo)),
            Err(e) => e,
        };
        let trackers = self.trackers(project).ok();
        let tracker = trackers.and_then(|t| t.gitlab);
        let hosts: Vec<String> = tracker
            .and_then(|g| gitlab::host(&g.host).ok())
            .into_iter()
            .collect();
        match gitlab::remote_slug(origin, &ssh, &hosts) {
            Some((host, slug)) => Ok(Forge::GitLab(host, slug)),
            None => Err(github.context("the pull request goes to origin")),
        }
    }

    /// `merged` or `closed` once the pull request (or merge request) at `url` is, `None` while it is open.
    fn request_state(&self, url: &str) -> Result<Option<&'static str>> {
        match gitlab::merge_request(url) {
            Some((site, _, _)) => self.gitlab(&site)?.mr_state(url),
            None => self.github().pr_state(url),
        }
    }

    /// The gate of a run, which only the user opens, with the fingerprint of the preview they saw: commits
    /// exactly what it listed, pushes the branch and opens the pull request, linked to the ticket. Refused if
    /// the worktree changed since. Delivering again pushes what changed and retries what failed.
    pub fn deliver(&self, id: i64, confirm: &str) -> Result<Delivery> {
        let store = Store::open(&self.data_dir)?;
        let mut run = store.run(id)?;
        let snapshot = self.snapshot(&store, &run)?;
        let delivered = self.send(&store, &mut run, &snapshot, confirm);
        let _ = fs::remove_file(&snapshot.index);
        delivered
    }

    fn send(
        &self,
        store: &Store,
        run: &mut Run,
        snapshot: &Snapshot,
        confirm: &str,
    ) -> Result<Delivery> {
        let id = run.id;
        ensure!(
            snapshot.preview.fingerprint == confirm,
            "the worktree of run {id} changed since its preview: preview the delivery again"
        );
        let (Some(worktree), Some(branch), Some(base)) =
            (run.worktree.clone(), run.branch.clone(), run.base.clone())
        else {
            bail!("run {id} has no worktree");
        };
        let (git, dir) = (Git(&self.env), Path::new(&worktree));
        if !snapshot.preview.files.is_empty() {
            let message = format!("{}\n\n{}", run.title, run.ticket.url);
            let mut env = self.env.clone();
            env.push((
                "GIT_INDEX_FILE".into(),
                snapshot.index.to_string_lossy().into(),
            ));
            Git(&env).run(dir, &["commit", "-q", "-m", &message])?;
            // The real index catches up with the commit; the files themselves are untouched.
            git.run(dir, &["reset", "-q"])?;
            let commit = git.run(dir, &["rev-parse", "HEAD"])?;
            store.record(id, &Event::Committed { commit, message })?;
        }
        // Commits that change nothing (a merge of the base alone) make no pull request either.
        let changed = git.count(dir, &format!("{base}..HEAD"))? > 0
            && !git.ok(dir, &["diff", "--quiet", &base, "HEAD", "--"]);
        if !changed {
            let others: Vec<String> = self
                .family(store, run)?
                .iter()
                .filter(|r| r.id != id && r.status != RunStatus::Closed)
                .map(|r| format!("run {} ({})", r.id, r.repo))
                .collect();
            match others.is_empty() {
                true => bail!("nothing to deliver: no change since {base}"),
                false => bail!(
                    "nothing to deliver: no change since {base} in this repository; the change is in {}: deliver \
                     those",
                    others.join(", ")
                ),
            }
        }
        // The commit itself is pushed, so the one recorded is the one sent.
        let commit = git.run(dir, &["rev-parse", &format!("refs/heads/{branch}")])?;
        let refspec = format!("{commit}:refs/heads/{branch}");
        git.run(dir, &["push", "-q", "origin", &refspec])?;
        store.record(
            id,
            &Event::Pushed {
                branch: branch.clone(),
                commit,
            },
        )?;
        run.status = RunStatus::Delivered;
        // Sent again to a merged or closed pull request: its state is asked again.
        store.forget_pr_state(id)?;
        run.pr_state = None;
        let mut warnings = Vec::new();
        if run.pr_url.is_none() {
            let origin = git.run(dir, &["config", "--get", "remote.origin.url"])?;
            let target = base.strip_prefix("origin/").unwrap_or(&base);
            let url = match self.forge(&run.project, &origin)? {
                Forge::GitHub(repo) => {
                    let body = self.pr_body(store, run, &repo, false)?;
                    self.github()
                        .create_pr(dir, &repo, target, &branch, &run.title, &body)?
                }
                Forge::GitLab(host, repo) => {
                    let body = self.pr_body(store, run, &repo, true)?;
                    self.gitlab(&host)?
                        .create_mr(&repo, target, &branch, &run.title, &body)?
                }
            };
            run.pr_url = Some(url.clone());
            store.update(run)?;
            store.record(id, &Event::PrOpened { url: url.clone() })?;
            // The pull requests of a change that spans several repositories link to each other.
            // shortcut: GitHub's only; a merge request lists the others when it opens, but is not edited later.
            let family = self.family(store, run)?;
            for other in family.iter().filter(|r| r.id != id && r.pr_state.is_none()) {
                let Some(other_url) = &other.pr_url else {
                    continue;
                };
                if gitlab::merge_request(other_url).is_some() {
                    continue;
                }
                let linked = self
                    .slug(&other.repo)
                    .and_then(|slug| self.pr_body(store, other, &slug, false))
                    .and_then(|body| self.github().edit_pr_body(other_url, &body));
                if let Err(e) = linked {
                    warnings.push(format!(
                        "the pull request is open, but linking it from {other_url} failed: {e:#}"
                    ));
                }
            }
        }
        let events = store.events(id)?;
        let commented = events
            .iter()
            .any(|e| matches!(e.event, Event::TicketCommented { .. }));
        let family = self.family(store, run)?;
        let mut moved = false;
        for other in &family {
            let events = store.events(other.id)?;
            moved |= events
                .iter()
                .any(|e| matches!(e.event, Event::TicketMoved { .. }));
        }
        let (comment, transition) = self.writes(&run.project, &run.ticket.provider);
        if let Some(url) = run.pr_url.clone()
            && comment
            && !commented
        {
            let linked = self
                .provider(&run.project, &run.ticket.provider)
                .and_then(|p| p.link_pr(&run.ticket, &url));
            match linked {
                Ok(()) => {
                    store.record(id, &Event::TicketCommented { url })?;
                }
                Err(e) => warnings.push(format!(
                    "the pull request is open, but commenting on the ticket failed (tried again at the next delivery): {e:#}"
                )),
            }
        }
        let moves = moves_ticket(run, &family, moved);
        if let Some(to) = transition.filter(|_| run.pr_url.is_some() && moves) {
            let done = self
                .provider(&run.project, &run.ticket.provider)
                .and_then(|p| p.transition(&run.ticket, &to));
            match done {
                Ok(()) => {
                    store.record(id, &Event::TicketMoved { to })?;
                }
                Err(e) => warnings.push(format!(
                    "the pull request is open, but moving the ticket to {to} failed (tried again at the next delivery): {e:#}"
                )),
            }
        }
        let warning = (!warnings.is_empty()).then(|| warnings.join("\n"));
        store.update(run)?;
        Ok(Delivery {
            run: run.clone(),
            warning,
        })
    }

    /// The `owner/name` of the GitHub repository `repo`'s origin is.
    fn slug(&self, repo: &str) -> Result<String> {
        let origin =
            Git(&self.env).run(Path::new(repo), &["config", "--get", "remote.origin.url"])?;
        github::remote_slug(&origin, &Ssh::new(self.home.clone()))
    }

    /// The body of the pull request of `run` in GitHub repository `slug`, or of its merge request in GitLab project
    /// `slug` (`on_gitlab`): what closes or names its ticket, then the pull requests of the same change in the
    /// project's other repositories.
    fn pr_body(&self, store: &Store, run: &Run, slug: &str, on_gitlab: bool) -> Result<String> {
        let ticket = &run.ticket;
        let mut body = match (on_gitlab, ticket.provider.as_str()) {
            (false, "github") => match github::split(&ticket.key) {
                Ok((repo, number)) if repo.eq_ignore_ascii_case(slug) => {
                    format!("Closes #{number}")
                }
                _ => format!("Ticket: {}", ticket.url),
            },
            (true, "gitlab") => match gitlab::split(&ticket.key) {
                Ok((repo, number)) if repo.eq_ignore_ascii_case(slug) => {
                    format!("Closes #{number}")
                }
                _ => format!("Closes {}", ticket.key),
            },
            _ if ticket.url.is_empty() => String::new(),
            _ => format!("Ticket: {}", ticket.url),
        };
        let others: Vec<String> = self
            .family(store, run)?
            .into_iter()
            .filter(|r| r.id != run.id)
            .filter_map(|r| r.pr_url.map(|url| format!("- {url}")))
            .collect();
        if !others.is_empty() {
            if !body.is_empty() {
                body.push_str("\n\n");
            }
            body.push_str("Pull requests of this change in the other repositories:\n");
            body.push_str(&others.join("\n"));
        }
        Ok(body)
    }

    /// `run` and the runs of the other repositories of the same change, the change's own first.
    fn family(&self, store: &Store, run: &Run) -> Result<Vec<Run>> {
        let head = run.parent.unwrap_or(run.id);
        let mut family = vec![store.run(head)?];
        family.extend(store.children(head)?);
        Ok(family
            .into_iter()
            .filter(|r| r.status != RunStatus::Failed)
            .collect())
    }

    /// What GitHub says of the open pull request of delivered run `id` against its base; `None` when it has none
    /// open.
    pub fn pr_merge(&self, id: i64) -> Result<Option<PrMerge>> {
        let run = Store::open(&self.data_dir)?.run(id)?;
        let (RunStatus::Delivered, Some(url), None) = (run.status, &run.pr_url, &run.pr_state)
        else {
            return Ok(None);
        };
        if let Some((site, _, _)) = gitlab::merge_request(url) {
            let (conflict, status) = self.gitlab(&site)?.mr_merge(url)?;
            let mergeable = if conflict { "CONFLICTING" } else { "MERGEABLE" };
            return Ok(Some(PrMerge {
                mergeable: mergeable.into(),
                behind: status == "need_rebase",
                state: status.to_ascii_uppercase(),
                conflict,
            }));
        }
        let (mergeable, state) = self.github().pr_merge(url)?;
        Ok(Some(PrMerge {
            conflict: mergeable == "CONFLICTING" || state == "DIRTY",
            behind: state == "BEHIND",
            mergeable,
            state,
        }))
    }

    /// Asks GitHub about the pull request of each delivered run whose fate is not known yet, and records those
    /// merged or closed. With `auto`, closes those merged too, unless their worktree holds work that is not pushed
    /// or `busy` says their session is at work; `stop_session` ends the session of the run it closes first. A pull
    /// request closed without a merge may be taken up again, so it is only told. A pull request GitHub cannot tell
    /// about is in `errors`, and asked again next time.
    pub fn after_merge(
        &self,
        auto: bool,
        busy: impl Fn(i64) -> bool,
        stop_session: impl Fn(i64),
    ) -> Result<PrCheck> {
        let store = Store::open(&self.data_dir)?;
        let mut check = PrCheck::default();
        for run in store.runs()? {
            let run = match self.record_pr(&store, &run) {
                Ok(Some(run)) => run,
                Ok(None) => continue,
                Err(e) => {
                    check.errors.push(CheckError {
                        run: run.id,
                        message: format!("{e:#}"),
                    });
                    continue;
                }
            };
            let merged = run.pr_state.as_deref() == Some("merged");
            let (run, cleaned, error) = if auto && merged && !busy(run.id) {
                match self.close_one(run.id, false, || stop_session(run.id)) {
                    Ok(closed) => (closed, true, None),
                    Err(e) => (run, false, Some(format!("{e:#}"))),
                }
            } else {
                (run, false, None)
            };
            check.cleanups.push(Cleanup {
                run,
                cleaned,
                error,
            });
        }
        Ok(check)
    }

    /// The run, once GitHub says its pull request was merged or closed and that is recorded; `None` while it
    /// is open, or when that is already known.
    pub(crate) fn record_pr(&self, store: &Store, run: &Run) -> Result<Option<Run>> {
        let (RunStatus::Delivered, Some(url), None) = (run.status, &run.pr_url, &run.pr_state)
        else {
            return Ok(None);
        };
        match self.request_state(url)? {
            Some(state) => store.pr_done(run.id, state, url),
            None => Ok(None),
        }
    }

    /// Closes a run whose pull request was merged or closed, as `close` does without force: refused while its
    /// worktree holds work that is not pushed, or while its pull request is open. A pull request known as
    /// closed is asked about again, as it may have been reopened.
    pub fn cleanup(&self, id: i64, stop_session: impl FnOnce()) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        let run = store.run(id)?;
        ensure!(
            run.status == RunStatus::Delivered,
            "run {id} is {}: only a delivered run is cleaned up",
            run.status.as_str()
        );
        let url = run.pr_url.clone();
        let url = url.with_context(|| format!("run {id} has no pull request to wait for"))?;
        if run.pr_state.as_deref() != Some("merged") {
            match self.request_state(&url)? {
                Some(state) => {
                    store.pr_done(id, state, &url)?;
                }
                None => {
                    store.forget_pr_state(id)?;
                    bail!("the pull request of run {id} is open: {url}");
                }
            }
        }
        self.close_one(id, false, stop_session)
    }

    /// Removes the run's worktree, and first those of the runs of its other repositories. Refused while one holds
    /// work that is not pushed, unless `force`: then the branch stays, so the commits it drops from the worktree can
    /// still be found. `stop_session` runs once the checks pass, before anything is removed, so no agent is left
    /// working in a deleted folder.
    pub fn close(&self, id: i64, force: bool, stop_session: impl FnOnce()) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        let children: Vec<Run> = store
            .children(id)?
            .into_iter()
            .filter(|c| c.status != RunStatus::Closed)
            .collect();
        // All of the change closes, or none of it.
        self.closable(&store, &store.run(id)?, force)?;
        for child in &children {
            self.closable(&store, child, force)?;
        }
        for child in &children {
            self.close_one(child.id, force, || {})?;
        }
        self.close_one(id, force, stop_session)
    }

    /// What closing `run` drops: refused when it is closed already, or, without `force`, when it holds work that is
    /// not pushed.
    fn closable(&self, store: &Store, run: &Run, force: bool) -> Result<Vec<String>> {
        let id = run.id;
        ensure!(
            run.status != RunStatus::Closed,
            "run {id} is already closed"
        );
        let (Some(worktree), Some(branch), Some(base)) = (&run.worktree, &run.branch, &run.base)
        else {
            return Ok(Vec::new());
        };
        let pushed = last_push(store, id)?;
        let since = pushed.as_deref().unwrap_or(base);
        let git = Git(&self.env);
        let repo = Path::new(&run.repo);
        let left = unpushed(&git, id, repo, Path::new(worktree), branch, since)?;
        let dropped = left.lost();
        if !force && !dropped.is_empty() {
            return Err(left.into());
        }
        Ok(dropped)
    }

    /// Closes the one run `id`, as `close` says: the merge of its own pull request cleans up its repository only.
    fn close_one(&self, id: i64, force: bool, stop_session: impl FnOnce()) -> Result<Run> {
        let store = Store::open(&self.data_dir)?;
        let mut run = store.run(id)?;
        let dropped = self.closable(&store, &run, force)?;
        let git = Git(&self.env);
        let repo = Path::new(&run.repo);
        let parts = (run.worktree.clone(), run.branch.clone(), run.base.clone());
        stop_session();
        if let (Some(worktree), Some(branch), Some(_)) = &parts {
            let worktree = Path::new(worktree);
            // Even once the folder is gone: the database copy lives elsewhere.
            let report = self.setup().teardown(worktree, &id.to_string());
            if !report.steps.is_empty() {
                let steps = report.steps;
                store.record(id, &Event::Teardown { steps })?;
            }
            if worktree.exists() {
                let path = worktree.to_string_lossy();
                let mut args = vec!["worktree", "remove"];
                if force {
                    args.push("--force");
                }
                args.push(&path);
                git.run(repo, &args)?;
            } else {
                git.run(repo, &["worktree", "prune"])?;
            }
            if dropped.is_empty() && git.has(repo, &format!("refs/heads/{branch}")) {
                git.run(repo, &["branch", "-D", branch])?;
            }
            // The folders that held it, if nothing else is there: `remove_dir` leaves a folder that is not empty.
            let _ = fs::remove_dir(self.data_worktrees(&run));
            let claude = repo.join(".claude");
            let _ = fs::remove_dir(claude.join("worktrees")).and_then(|_| fs::remove_dir(claude));
        }
        run.status = RunStatus::Closed;
        store.update(&mut run)?;
        store.record(id, &Event::Closed { dropped })?;
        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_name_status_listings() {
        let files =
            file_changes("M\0src/a b.rs\0A\0 new\0D\0old.txt\0R087\0x.md\0y.md\0C100\0p\0q\0");
        let change = |path: &str, change, from: Option<&str>| FileChange {
            path: path.into(),
            change,
            from: from.map(Into::into),
        };
        assert_eq!(
            files,
            [
                change("src/a b.rs", Change::Modified, None),
                change(" new", Change::Added, None),
                change("old.txt", Change::Deleted, None),
                change("y.md", Change::Renamed, Some("x.md")),
                change("q", Change::Added, None),
            ]
        );
    }

    #[test]
    fn a_change_moves_its_ticket_once_even_when_its_work_is_all_elsewhere() {
        let run = |id: i64, parent: Option<i64>, pr: bool| Run {
            id,
            project: "acme".into(),
            ticket: crate::TicketRef {
                provider: "github".into(),
                key: "acme/web#1".into(),
                url: String::new(),
            },
            title: "Export".into(),
            repo: format!("/repos/{id}"),
            worktree: None,
            branch: None,
            base: None,
            status: RunStatus::Running,
            pr_url: pr.then(|| format!("https://github.com/acme/r/pull/{id}")),
            created: 0,
            updated: 0,
            claude_session: None,
            pr_state: None,
            parent,
            session_id: None,
        };
        let (own, other) = (run(1, None, true), run(2, Some(1), true));
        assert!(moves_ticket(&own, &[own.clone(), other.clone()], false));
        assert!(
            !moves_ticket(&other, &[own.clone(), other.clone()], false),
            "the change's own pull request moves it"
        );
        let bare = run(1, None, false);
        assert!(
            moves_ticket(&other, &[bare.clone(), other.clone()], false),
            "nothing to deliver in the change's own repository: the first other pull request moves it"
        );
        assert!(
            !moves_ticket(&bare, &[bare.clone(), other.clone()], true),
            "once for the change"
        );
    }
}
