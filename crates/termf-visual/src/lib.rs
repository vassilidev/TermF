//! Visual review before / after (ARCHITECTURE §8). For a ticket run, or two commits of a repository, TermF
//! starts the dev server of each side (the base commit in a cached worktree, the run's worktree), captures each
//! screen in headless Chrome, compares the pixels and the page's texts, and keeps a versioned review where each
//! screen gets a decision, pins and a comment. When the team's visual gate opens, a review version starts; once
//! every screen is decided, the gate goes on, stops the run, or sends the changes asked to the agent, whose new
//! work gets the next version.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use termf_team::{StepKind, StepState, TeamRun};
use termf_worktree::Setup;

pub mod agent;
pub mod browser;
pub mod cft;
pub mod config;
pub mod diff;
mod env;
mod pr;
pub mod screens;
mod store;
#[cfg(all(test, unix))]
mod tests;

pub use browser::{Shot, Target};
pub use diff::Rect;
pub use pr::PrPreview;
pub use store::{review, reviews};

use browser::Browser;
use env::Server;
use store::Store;

const DEV_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub id: i64,
    pub run: Option<i64>,
    pub project: String,
    pub repo: String,
    pub repo_path: PathBuf,
    pub title: String,
    pub ticket: Option<String>,
    pub base: String,
    pub head: Option<String>,
    pub versions: Vec<Version>,
    pub created: i64,
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VersionState {
    Capturing,
    Ready,
    Failed,
    Done,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub n: u32,
    pub state: VersionState,
    pub step: Option<String>,
    pub only: Vec<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub progress: Progress,
    pub screens: Vec<Screen>,
    pub notes: Vec<String>,
    pub error: Option<String>,
    /// Why the before side could not be captured (no dev server at the base, say): the screens have their after
    /// image alone, which the user judges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_missing: Option<String>,
    pub outcome: Option<Decision>,
    pub created: i64,
    pub finished: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Pending,
    Approved,
    Rejected,
    Changes,
}

impl Decision {
    pub fn parse(text: &str) -> Result<Decision> {
        Ok(match text.trim().to_lowercase().as_str() {
            "approve" | "approved" => Decision::Approved,
            "reject" | "rejected" => Decision::Rejected,
            "changes" | "change" => Decision::Changes,
            "pending" => Decision::Pending,
            other => bail!("unknown decision {other}: approve, reject or changes"),
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Route,
    Agent,
    Storybook,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    pub id: String,
    pub source: Source,
    pub path: String,
    pub state: Option<String>,
    pub viewport: String,
    pub width: u32,
    pub height: u32,
    pub theme: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub heatmap: Option<String>,
    pub rects: Vec<Rect>,
    pub ratio: f64,
    pub changed: bool,
    pub texts: Vec<TextChange>,
    pub error: Option<String>,
    pub decision: Decision,
    pub comment: String,
    pub pins: Vec<Pin>,
    pub decided: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pin {
    pub x: u32,
    pub y: u32,
    pub comment: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FileDiff {
    pub path: String,
    pub diff: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextChange {
    pub change: String,
    pub text: String,
    pub justified_by: Option<String>,
    pub flagged: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Request {
    pub run: Option<i64>,
    pub repo: Option<PathBuf>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub routes: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub review: i64,
    pub version: u32,
    #[serde(flatten)]
    pub progress: Progress,
}

#[derive(Clone, Debug)]
pub enum Update {
    Review(Box<Review>),
    Progress(ProgressEvent),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Approve,
    Reject,
    Changes(String),
}

pub fn to_team(team: &termf_team::Team, run: i64, step: &str, outcome: &Outcome) -> Result<()> {
    match outcome {
        Outcome::Approve => team.approve(run, step).map(drop),
        Outcome::Reject => team.stop(Some(run)).map(drop),
        Outcome::Changes(feedback) => team.revise(run, step, feedback).map(drop),
    }
}

/// Captures for tests, instead of Chrome.
pub type Shooter = Arc<dyn Fn(&Target) -> Result<Shot> + Send + Sync>;
type Gate = Arc<dyn Fn(i64, &str, &Outcome) -> Result<()> + Send + Sync>;

#[derive(Clone)]
pub struct Visual {
    inner: Arc<Inner>,
}

struct Inner {
    config_dir: PathBuf,
    data_dir: PathBuf,
    env: Vec<(String, String)>,
    shooter: Option<Shooter>,
    /// Every change of a review goes through it, one at a time.
    lock: Mutex<()>,
    sink: Box<dyn Fn(Update) + Send + Sync>,
    gate: Mutex<Option<Gate>>,
    /// Dev servers and browsers at work, killed when the app quits.
    children: Mutex<BTreeSet<u32>>,
    /// One capture at a time: two on the same base commit would share its worktree and port.
    capturing: Mutex<()>,
}

#[derive(Default)]
struct Justify {
    ticket: String,
    declared: Vec<(String, String)>,
    flagged: Vec<String>,
}

impl Visual {
    /// A capture a restart interrupted is marked failed.
    pub fn new(
        config_dir: PathBuf,
        data_dir: PathBuf,
        sink: impl Fn(Update) + Send + Sync + 'static,
    ) -> Visual {
        Self::build(config_dir, data_dir, Vec::new(), None, sink)
    }

    pub fn build(
        config_dir: PathBuf,
        data_dir: PathBuf,
        env: Vec<(String, String)>,
        shooter: Option<Shooter>,
        sink: impl Fn(Update) + Send + Sync + 'static,
    ) -> Visual {
        let visual = Visual {
            inner: Arc::new(Inner {
                config_dir,
                data_dir,
                env,
                shooter,
                lock: Mutex::new(()),
                sink: Box::new(sink),
                gate: Mutex::default(),
                children: Mutex::default(),
                capturing: Mutex::default(),
            }),
        };
        if let Err(e) = visual.recover() {
            eprintln!("termf: visual reviews left as they were: {e:#}");
        }
        visual
    }

    pub fn on_gate(
        &self,
        gate: impl Fn(i64, &str, &Outcome) -> Result<()> + Send + Sync + 'static,
    ) {
        *self.inner.gate.lock().unwrap() = Some(Arc::new(gate));
    }

    pub fn shutdown(&self) {
        for pid in std::mem::take(&mut *self.inner.children.lock().unwrap()) {
            kill(pid);
        }
    }

    fn recover(&self) -> Result<()> {
        let store = Store::open(&self.inner.data_dir)?;
        for mut review in store.all()? {
            let mut changed = false;
            for v in review
                .versions
                .iter_mut()
                .filter(|v| v.state == VersionState::Capturing)
            {
                v.state = VersionState::Failed;
                v.error = Some("TermF stopped during the capture: start it again".into());
                v.finished = Some(now());
                changed = true;
            }
            if changed {
                store.save(&mut review)?;
            }
        }
        Ok(())
    }

    pub fn review(&self, id: i64) -> Result<Review> {
        review(&self.inner.data_dir, id)
    }

    pub fn reviews(&self) -> Result<Vec<Review>> {
        reviews(&self.inner.data_dir)
    }

    pub fn folder(&self, id: i64, version: u32) -> PathBuf {
        folder(&self.inner.data_dir, id, version)
    }

    pub fn start(&self, request: Request) -> Result<Review> {
        let inner = &self.inner;
        let _lock = inner.lock.lock().unwrap();
        let store = Store::open(&inner.data_dir)?;
        if let Some(run) = request.run {
            let team = termf_team::run(&inner.data_dir, run).ok();
            let step = team.as_ref().and_then(|t| {
                t.steps
                    .iter()
                    .find(|s| s.kind == StepKind::Visual && s.state == StepState::Waiting)
                    .map(|s| s.key.clone())
            });
            let review = self.of_run(&store, run, team.as_ref())?;
            return self.begin(&store, review, team, step, request.routes);
        }
        let repo = request
            .repo
            .context("say which run to review, or which repository with a base and a head")?;
        let repo = std::path::absolute(&repo)?;
        let (Some(base), Some(head)) = (request.base, request.head) else {
            bail!("a review of a repository needs a base and a head (commits, branches or tags)");
        };
        let config = config::load(&inner.config_dir, &repo)?;
        env::commit(&inner.env, &repo, &base)?;
        env::commit(&inner.env, &repo, &head)?;
        // The same comparison again is the next version of its review.
        let same = store.all()?.into_iter().find(|r| {
            r.run.is_none()
                && config::same(&r.repo_path, &repo)
                && r.base == base
                && r.head.as_ref() == Some(&head)
        });
        if let Some(review) = same {
            return self.begin(&store, review, None, None, request.routes);
        }
        let review = Review {
            id: 0,
            run: None,
            project: config.project,
            repo: config.repo,
            repo_path: repo,
            title: format!("{base}..{head}"),
            ticket: None,
            base,
            head: Some(head),
            versions: Vec::new(),
            created: now(),
            updated: now(),
        };
        self.begin(&store, review, None, None, request.routes)
    }

    /// The team's hook. A capture already at work is waited for, then the gate gets a version of its own: a
    /// gate whose version never comes would wait forever.
    pub fn gate(&self, team: TeamRun, step: String) {
        let visual = self.clone();
        std::thread::spawn(move || {
            let inner = &visual.inner;
            loop {
                let started = (|| {
                    let _lock = inner.lock.lock().unwrap();
                    let store = Store::open(&inner.data_dir)?;
                    let review = visual.of_run(&store, team.run, Some(&team))?;
                    if review
                        .versions
                        .last()
                        .is_some_and(|v| v.state == VersionState::Capturing)
                    {
                        return Ok(false);
                    }
                    visual.begin(
                        &store,
                        review,
                        Some(team.clone()),
                        Some(step.clone()),
                        Vec::new(),
                    )?;
                    Ok::<_, anyhow::Error>(true)
                })();
                match started {
                    Ok(true) => break,
                    Ok(false) => std::thread::sleep(Duration::from_secs(1)),
                    Err(e) => {
                        eprintln!("termf: visual review of run {}: {e:#}", team.run);
                        break;
                    }
                }
            }
        });
    }

    /// The review of a ticket run, or a new one (unsaved, id 0). With the team's run, it never fails but on the
    /// database: what is missing fails the capture instead, where the review shows it.
    fn of_run(&self, store: &Store, run: i64, team: Option<&TeamRun>) -> Result<Review> {
        if let Some(review) = store.of_run(run)? {
            return Ok(review);
        }
        let inner = &self.inner;
        let ticket =
            termf_tickets::Tickets::new(inner.config_dir.clone(), inner.data_dir.clone()).run(run);
        let (project, repo, repo_path, title, key) = match (team, &ticket) {
            (Some(t), _) => {
                let t = &t.target;
                let path = t.repo_path.clone();
                (
                    t.project.clone(),
                    t.repo.clone(),
                    path,
                    t.title.clone(),
                    t.ticket.clone(),
                )
            }
            (None, Ok(r)) => {
                let config = config::load(&inner.config_dir, Path::new(&r.repo))?;
                let path = PathBuf::from(&r.repo);
                (
                    config.project,
                    config.repo,
                    path,
                    r.title.clone(),
                    r.ticket.key.clone(),
                )
            }
            (None, Err(e)) => bail!("{e:#}"),
        };
        Ok(Review {
            id: 0,
            run: Some(run),
            project,
            repo,
            repo_path,
            title,
            ticket: Some(key),
            base: ticket.ok().and_then(|r| r.base).unwrap_or_default(),
            head: None,
            versions: Vec::new(),
            created: now(),
            updated: now(),
        })
    }

    fn begin(
        &self,
        store: &Store,
        mut review: Review,
        team: Option<TeamRun>,
        step: Option<String>,
        only: Vec<String>,
    ) -> Result<Review> {
        ensure!(
            review
                .versions
                .last()
                .is_none_or(|v| v.state != VersionState::Capturing),
            "review {} is already capturing",
            review.id
        );
        let n = review.versions.last().map_or(1, |v| v.n + 1);
        review.versions.push(Version {
            n,
            state: VersionState::Capturing,
            step,
            only,
            before: None,
            after: None,
            progress: Progress {
                phase: "prepare".into(),
                message: "preparing the worktrees".into(),
                ..Progress::default()
            },
            screens: Vec::new(),
            notes: Vec::new(),
            error: None,
            before_missing: None,
            outcome: None,
            created: now(),
            finished: None,
        });
        match review.id {
            0 => store.insert(&mut review)?,
            _ => store.save(&mut review)?,
        }
        self.emit(&review);
        let visual = self.clone();
        let id = review.id;
        std::thread::spawn(move || {
            if let Err(e) = visual.capture(id, n, team.as_ref()) {
                let _ = visual.update(id, n, |v| {
                    v.state = VersionState::Failed;
                    v.error = Some(format!("{e:#}"));
                    v.finished = Some(now());
                });
            }
        });
        Ok(review)
    }

    fn emit(&self, review: &Review) {
        (self.inner.sink)(Update::Review(Box::new(review.clone())));
    }

    fn update(&self, id: i64, n: u32, change: impl FnOnce(&mut Version)) -> Result<Review> {
        let _lock = self.inner.lock.lock().unwrap();
        let store = Store::open(&self.inner.data_dir)?;
        let mut review = store.get(id)?.with_context(|| format!("no review {id}"))?;
        let version = review
            .versions
            .iter_mut()
            .find(|v| v.n == n)
            .with_context(|| format!("review {id} has no version {n}"))?;
        change(version);
        store.save(&mut review)?;
        self.emit(&review);
        Ok(review)
    }

    fn progress(&self, id: i64, n: u32, phase: &str, done: usize, total: usize, message: &str) {
        let progress = Progress {
            phase: phase.into(),
            done,
            total,
            message: message.into(),
        };
        (self.inner.sink)(Update::Progress(ProgressEvent {
            review: id,
            version: n,
            progress: progress.clone(),
        }));
        let _ = self.update(id, n, |v| v.progress = progress);
    }

    fn capture(&self, id: i64, n: u32, team: Option<&TeamRun>) -> Result<()> {
        let inner = &self.inner;
        let _one = inner.capturing.lock().unwrap_or_else(|e| e.into_inner());
        let review = self.review(id)?;
        let version = review
            .versions
            .iter()
            .find(|v| v.n == n)
            .context("no such version")?;
        ensure!(
            !review.base.is_empty(),
            "the run has no base branch to compare with"
        );
        let config = config::load(&inner.config_dir, &review.repo_path)?;
        let mut setup = Setup::new(inner.config_dir.clone(), inner.data_dir.clone());
        setup.env = inner.env.clone();
        let mut notes = Vec::new();
        let (after_dir, after) = match review.run {
            Some(run) => {
                let tickets =
                    termf_tickets::Tickets::new(inner.config_dir.clone(), inner.data_dir.clone());
                let dir = tickets
                    .run(run)?
                    .worktree
                    .map(PathBuf::from)
                    .filter(|w| w.is_dir())
                    .with_context(|| format!("run {run} has no worktree"))?;
                let head = env::git(&inner.env, &dir, &["rev-parse", "HEAD"])?;
                (dir, head)
            }
            None => {
                let head = review.head.as_deref().unwrap_or("HEAD");
                let commit = env::commit(&inner.env, &review.repo_path, head)?;
                let (dir, more) = env::cached(
                    &setup,
                    &config.project,
                    &config.repo,
                    &review.repo_path,
                    &commit,
                )?;
                notes.extend(more);
                (dir, commit)
            }
        };
        let before = match review.run {
            Some(_) => env::git(
                &inner.env,
                &after_dir,
                &["merge-base", &review.base, "HEAD"],
            )
            .with_context(|| format!("no common commit with {}", review.base))?,
            None => env::commit(&inner.env, &review.repo_path, &review.base)?,
        };
        let (before_dir, more) = env::cached(
            &setup,
            &config.project,
            &config.repo,
            &review.repo_path,
            &before,
        )?;
        notes.extend(more);

        let views: Vec<termf_team::plan::View> = team
            .map(|t| {
                t.steps
                    .iter()
                    .filter_map(|s| s.report.as_ref())
                    .flat_map(|r| r.affected_views.clone())
                    .collect()
            })
            .unwrap_or_default();
        let changed: Vec<String> =
            env::git(&inner.env, &after_dir, &["diff", "--name-only", &before])?
                .lines()
                .map(str::to_owned)
                .collect();
        let (stories, more) = screens::stories(&after_dir, &changed);
        notes.extend(more);
        let (mut list, more) = screens::plan(&config, &views, &version.only, &stories);
        notes.extend(more);
        ensure!(
            !list.is_empty(),
            "nothing to capture: list routes in [visual] of the project, or have the agents declare their affected_views"
        );
        let dev = config.dev.clone().with_context(|| {
            format!(
                "no dev server known: set `dev` for the repository {} in the project (`{{{{ port }}}}` or $PORT is its port)",
                config.repo
            )
        })?;
        if config.settings.login.is_none() {
            notes.push(
                "no test account login in [visual.login] of the project: public pages only".into(),
            );
        }
        let total = list.len() * 2;
        self.update(id, n, |v| {
            v.before = Some(before.clone());
            v.after = Some(after.clone());
            v.screens = list.clone();
            v.notes = notes.clone();
        })?;

        let dir = self.folder(id, n);
        fs::create_dir_all(&dir)?;
        let mut texts: BTreeMap<String, [Vec<String>; 2]> = BTreeMap::new();
        let mut alone = false;
        for (k, (side, wt)) in [("before", &before_dir), ("after", &after_dir)]
            .into_iter()
            .enumerate()
        {
            self.progress(
                id,
                n,
                side,
                k * list.len(),
                total,
                &format!("starting the dev server of {side}"),
            );
            let shots = match self.side(side, wt, &config, &dev, &list, &dir, &mut |i, screen| {
                self.progress(id, n, side, k * list.len() + i, total, screen);
            }) {
                Ok(shots) => shots,
                // A new page's base often has no dev server yet (`Missing script: dev`): the after alone.
                Err(e) if side == "before" => {
                    let reason = format!("{e:#}");
                    self.update(id, n, |v| v.before_missing = Some(reason.clone()))?;
                    alone = true;
                    continue;
                }
                Err(e) => return Err(e),
            };
            for (screen, shot) in list.iter_mut().zip(shots) {
                match shot {
                    Ok(shot) => {
                        let file = format!("{}-{side}.png", screen.id);
                        fs::write(dir.join(&file), &shot.png)?;
                        match side {
                            "before" => screen.before = Some(file),
                            _ => screen.after = Some(file),
                        }
                        texts.entry(screen.id.clone()).or_default()[k] = shot.texts;
                    }
                    Err(e) => {
                        let error = format!("{side}: {e:#}");
                        screen.error = Some(match screen.error.take() {
                            Some(first) => format!("{first}; {error}"),
                            None => error,
                        });
                    }
                }
            }
        }

        self.progress(id, n, "diff", total, total, "comparing");
        let threshold = config.settings.threshold.unwrap_or(diff::THRESHOLD);
        let justify = justify(team);
        for screen in &mut list {
            if let (Some(b), Some(a)) = (&screen.before, &screen.after) {
                let (b, a) = (
                    diff::decode(&fs::read(dir.join(b))?)?,
                    diff::decode(&fs::read(dir.join(a))?)?,
                );
                let found = diff::compare(&b, &a, threshold);
                let file = format!("{}-heatmap.png", screen.id);
                fs::write(dir.join(&file), diff::encode(&found.heatmap)?)?;
                screen.heatmap = Some(file);
                screen.rects = found.rects;
                screen.ratio = found.ratio;
            }
            let [before_texts, after_texts] = texts.remove(&screen.id).unwrap_or_default();
            if screen.before.is_some() && screen.after.is_some() {
                let (added, removed) = diff::texts(&before_texts, &after_texts);
                screen.texts = added
                    .iter()
                    .map(|t| justify.judge("added", t))
                    .chain(removed.iter().map(|t| justify.judge("removed", t)))
                    .collect();
            }
            // Nothing to compare with: the user looks at each page as new.
            screen.changed = (alone && screen.after.is_some())
                || !screen.rects.is_empty()
                || !screen.texts.is_empty();
        }
        let shot = list.iter().filter(|s| s.error.is_none()).count();
        self.update(id, n, |v| {
            v.screens = list;
            v.state = VersionState::Ready;
            v.progress = Progress {
                phase: "done".into(),
                done: total,
                total,
                message: format!("{shot} screen(s) captured"),
            };
            v.finished = Some(now());
        })?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn side(
        &self,
        side: &'static str,
        wt: &Path,
        config: &config::Config,
        dev: &str,
        list: &[Screen],
        dir: &Path,
        step: &mut dyn FnMut(usize, &str),
    ) -> Result<Vec<Result<Shot>>> {
        let inner = &self.inner;
        let mut env = Setup::new(inner.config_dir.clone(), inner.data_dir.clone()).env(wt);
        env.extend(inner.env.iter().cloned());
        let timeout = config
            .settings
            .timeout
            .map_or(DEV_TIMEOUT, Duration::from_secs);
        let start = |name: &str, command: &str| -> Result<(Server, Tracked)> {
            let port = termf_worktree::reserve_port(&inner.data_dir, wt, name)?;
            let log = dir.join(format!("{side}-{name}.log"));
            let mut server = Server::start(command, wt, &env, port, &log)?;
            let tracked = Tracked::new(&inner.children, server.pid());
            server
                .wait(timeout)
                .with_context(|| format!("{side} ({})", wt.display()))?;
            Ok((server, tracked))
        };
        let (mut server, _tracked) = start("visual", dev)?;
        let mut story = match list.iter().any(|s| s.source == Source::Storybook) {
            true => config
                .storybook
                .as_deref()
                .map(|c| start("storybook", c))
                .transpose()?,
            false => None,
        };
        let mut browser = match inner.shooter {
            Some(_) => None,
            None => {
                let browser = Browser::launch(&browser::find()?)?;
                Some((Tracked::new(&inner.children, browser.pid()), browser))
            }
        };
        let base = format!("http://localhost:{}", server.port);
        if let (Some((_, browser)), Some(login)) = (browser.as_mut(), &config.settings.login) {
            browser
                .login(&base, login)
                .with_context(|| format!("{side}: the test account's login failed"))?;
        }
        let mut shots = Vec::new();
        for (i, screen) in list.iter().enumerate() {
            let port = match (screen.source, &story) {
                (Source::Storybook, Some((s, _))) => s.port,
                _ => server.port,
            };
            let target = Target {
                side,
                url: format!("http://localhost:{port}{}", screen.path),
                width: screen.width,
                height: screen.height,
                mobile: screen.viewport != "desktop",
                theme: screen.theme.clone(),
                mask: config.settings.mask.clone(),
            };
            let shot = match (&inner.shooter, browser.as_mut()) {
                (Some(shoot), _) => shoot(&target),
                (None, Some((_, browser))) => browser.capture(&target),
                (None, None) => unreachable!("a browser runs when no shooter does"),
            };
            shots.push(shot);
            step(i + 1, &screen.id);
        }
        // Ours ended (its port taken, say) while another program answered there: the screens are not this worktree's.
        for s in std::iter::once(&mut server).chain(story.as_mut().map(|(s, _)| s)) {
            ensure!(
                s.running(),
                "{side}: the dev server ended during the capture, and another server answered on its port {}: {}",
                s.port,
                s.tail()
            );
        }
        Ok(shots)
    }

    /// `Pending` keeps a screen to decide: it saves its comment and pins, or takes back its decision.
    pub fn decide(
        &self,
        id: i64,
        screen: &str,
        decision: Decision,
        comment: Option<String>,
        pins: Option<Vec<Pin>>,
    ) -> Result<Review> {
        let inner = &self.inner;
        let _lock = inner.lock.lock().unwrap();
        let store = Store::open(&inner.data_dir)?;
        let mut review = store.get(id)?.with_context(|| format!("no review {id}"))?;
        let last = review
            .versions
            .len()
            .checked_sub(1)
            .context("nothing captured yet")?;
        let version = &mut review.versions[last];
        match version.state {
            VersionState::Ready => {}
            VersionState::Capturing => bail!("the capture of v{} is not over", version.n),
            VersionState::Failed => bail!("the capture of v{} failed: start a new one", version.n),
            VersionState::Done => bail!(
                "v{} is decided: start a new capture to review again",
                version.n
            ),
        }
        let chosen: Vec<usize> = match screen {
            "all" => (0..version.screens.len())
                .filter(|&i| version.screens[i].decision == Decision::Pending)
                .collect(),
            id => vec![
                version
                    .screens
                    .iter()
                    .position(|s| s.id == id)
                    .with_context(|| format!("v{} has no screen {id}", version.n))?,
            ],
        };
        for i in chosen {
            let s = &mut version.screens[i];
            if let Some(comment) = &comment {
                s.comment = comment.trim().to_owned();
            }
            if let Some(pins) = &pins {
                s.pins = pins.clone();
            }
            ensure!(
                decision != Decision::Changes || !s.comment.is_empty() || !s.pins.is_empty(),
                "say what to change on {}: a comment or a pin",
                s.id
            );
            s.decision = decision;
            s.decided = (decision != Decision::Pending).then(now);
        }
        if version
            .screens
            .iter()
            .all(|s| s.decision != Decision::Pending)
        {
            let decided = |d: Decision| version.screens.iter().any(|s| s.decision == d);
            let outcome = match () {
                _ if decided(Decision::Rejected) => Decision::Rejected,
                _ if decided(Decision::Changes) => Decision::Changes,
                _ => Decision::Approved,
            };
            let gate = inner.gate.lock().unwrap().clone();
            let mut refused = None;
            if let (Some(run), Some(step), Some(gate)) = (review.run, version.step.clone(), gate) {
                let sent = match outcome {
                    Decision::Approved => Outcome::Approve,
                    Decision::Rejected => Outcome::Reject,
                    _ => Outcome::Changes(feedback(&inner.data_dir, id, version)),
                };
                if let Err(e) = gate(run, &step, &sent) {
                    let waits = termf_team::run(&inner.data_dir, run).is_ok_and(|t| {
                        t.steps
                            .iter()
                            .any(|s| s.key == step && s.state == StepState::Waiting)
                    });
                    // A team paused or stopped takes it once started again: the user decides again then.
                    if waits {
                        store.save(&mut review)?;
                        self.emit(&review);
                        return Err(e.context(
                            "your decisions are saved, but the team could not take them: start it, then decide again",
                        ));
                    }
                    // The gate was settled elsewhere (`termf team`, a stop): nothing left to wait for.
                    refused = Some(format!("the team did not take this outcome: {e:#}"));
                }
            }
            let version = &mut review.versions[last];
            version.error = refused;
            version.outcome = Some(outcome);
            version.state = VersionState::Done;
            version.finished = Some(now());
        }
        store.save(&mut review)?;
        self.emit(&review);
        Ok(review)
    }

    pub fn image(&self, id: i64, version: u32, screen: &str, kind: &str) -> Result<Vec<u8>> {
        let review = self.review(id)?;
        let v = review
            .versions
            .iter()
            .find(|v| v.n == version)
            .with_context(|| format!("review {id} has no version {version}"))?;
        let s = v
            .screens
            .iter()
            .find(|s| s.id == screen)
            .with_context(|| format!("v{version} has no screen {screen}"))?;
        let file = match kind {
            "before" => &s.before,
            "after" => &s.after,
            "heatmap" => &s.heatmap,
            _ => bail!("no image {kind}: before, after or heatmap"),
        }
        .as_ref()
        .with_context(|| format!("{screen} has no {kind} image"))?;
        Ok(fs::read(self.folder(id, version).join(file))?)
    }

    /// The files the version's after commit changed since its before commit, each with its unified diff.
    pub fn files(&self, id: i64, version: u32) -> Result<Vec<FileDiff>> {
        let review = self.review(id)?;
        let v = review
            .versions
            .iter()
            .find(|v| v.n == version)
            .with_context(|| format!("review {id} has no version {version}"))?;
        let (Some(before), Some(after)) = (&v.before, &v.after) else {
            return Ok(Vec::new());
        };
        file_diffs(&self.inner.env, &review.repo_path, before, after)
    }
}

pub(crate) fn file_diffs(
    env: &[(String, String)],
    repo: &Path,
    before: &str,
    after: &str,
) -> Result<Vec<FileDiff>> {
    let names = env::git(env, repo, &["diff", "--name-only", "-z", before, after])?;
    // `--submodule=short`: a `diff.submodule = log` config would print a submodule without its `diff --git` header.
    let patch = env::git(
        env,
        repo,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--submodule=short",
            before,
            after,
        ],
    )?;
    // Both lists come from the same diff, in the same order: one `diff --git` section per name.
    let mut diffs: Vec<String> = Vec::new();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            diffs.push(String::new());
        }
        if let Some(d) = diffs.last_mut() {
            d.push_str(line);
            d.push('\n');
        }
    }
    Ok(names
        .split('\0')
        .filter(|n| !n.is_empty())
        .zip(diffs)
        .map(|(path, diff)| FileDiff {
            path: path.to_owned(),
            diff,
        })
        .collect())
}

pub fn folder(data_dir: &Path, id: i64, version: u32) -> PathBuf {
    data_dir
        .join("reviews")
        .join(id.to_string())
        .join(format!("v{version}"))
}

fn feedback(data_dir: &Path, id: i64, version: &Version) -> String {
    let dir = folder(data_dir, id, version.n);
    let mut text = format!(
        "The user reviewed the screens before and after your change (visual review {id}, v{}) and asks for changes:\n",
        version.n
    );
    for s in version
        .screens
        .iter()
        .filter(|s| s.decision == Decision::Changes)
    {
        let state = s
            .state
            .as_deref()
            .map(|st| format!(" ({st})"))
            .unwrap_or_default();
        text.push_str(&format!("- {}{state}, {} {}", s.path, s.viewport, s.theme));
        if !s.comment.is_empty() {
            text.push_str(&format!(": {}", s.comment));
        }
        text.push('\n');
        for pin in &s.pins {
            text.push_str(&format!(
                "  - at x={}, y={} of the after image: {}\n",
                pin.x, pin.y, pin.comment
            ));
        }
        let path = |f: &Option<String>| f.as_ref().map(|f| dir.join(f).display().to_string());
        let images: Vec<String> = [
            ("before", path(&s.before)),
            ("after", path(&s.after)),
            ("differences", path(&s.heatmap)),
        ]
        .into_iter()
        .filter_map(|(k, p)| Some(format!("{k} {}", p?)))
        .collect();
        if !images.is_empty() {
            text.push_str(&format!("  - images: {}\n", images.join(", ")));
        }
    }
    text
}

fn normal(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn justify(team: Option<&TeamRun>) -> Justify {
    let Some(team) = team else {
        return Justify::default();
    };
    let reports = team.steps.iter().filter_map(|s| s.report.as_ref());
    Justify {
        ticket: normal(&team.target.brief),
        declared: reports
            .flat_map(|r| r.text_changes.iter())
            .map(|t| (normal(&t.text), t.justified_by.clone()))
            .filter(|(t, _)| !t.is_empty())
            .collect(),
        flagged: team
            .steps
            .iter()
            .flat_map(|s| s.violations.iter())
            .filter_map(|v| quoted(&v.message))
            .map(|t| normal(&t))
            .collect(),
    }
}

/// The text the team's `text-changes` check quotes in its message (`text "…" changed …`), which Rust's debug
/// format wrote: its escapes undone.
fn quoted(message: &str) -> Option<String> {
    let mut chars = message.strip_prefix("text \"")?.chars();
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                'u' => {
                    if chars.next()? != '{' {
                        return None;
                    }
                    let hex: String = chars.by_ref().take_while(|c| *c != '}').collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
}

/// Whether `part` is `text`, or a run of whole words of it: "total" is not in "totalement", and a text this short
/// ("OK", "1") must match exactly.
fn has_words(text: &str, part: &str) -> bool {
    if text == part {
        return true;
    }
    if part.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
        return false;
    }
    text.match_indices(part).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + part.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

impl Justify {
    fn judge(&self, change: &str, text: &str) -> TextChange {
        let t = normal(text);
        let justified_by = match !t.is_empty() && has_words(&self.ticket, &t) {
            true => Some("ticket".to_owned()),
            false => self
                .declared
                .iter()
                .find(|(d, _)| has_words(d, &t))
                .map(|(_, by)| by.clone()),
        };
        TextChange {
            change: change.into(),
            text: text.into(),
            justified_by,
            flagged: self.flagged.iter().any(|f| has_words(f, &t)),
        }
    }
}

struct Tracked<'a>(&'a Mutex<BTreeSet<u32>>, u32);

impl<'a> Tracked<'a> {
    fn new(set: &'a Mutex<BTreeSet<u32>>, pid: u32) -> Self {
        set.lock().unwrap().insert(pid);
        Tracked(set, pid)
    }
}

impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.0.lock().unwrap().remove(&self.1);
    }
}

/// Kills `pid` and what it started (its process group under Unix, its tree under Windows).
pub fn kill(pid: u32) {
    #[cfg(unix)]
    termf_core::process::kill_group(pid);
    #[cfg(windows)]
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
