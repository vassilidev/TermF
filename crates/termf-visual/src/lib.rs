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

pub mod browser;
pub mod config;
pub mod diff;
mod env;
pub mod screens;
mod store;
#[cfg(all(test, unix))]
mod tests;

pub use browser::{Shot, Target};
pub use diff::Rect;
pub use store::{review, reviews};

use browser::Browser;
use env::Server;
use store::Store;

const DEV_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub id: i64,
    /// The ticket run reviewed; `None` for two commits of a repository.
    pub run: Option<i64>,
    pub project: String,
    /// The repository's name in the project, and its main checkout.
    pub repo: String,
    pub repo_path: PathBuf,
    pub title: String,
    pub ticket: Option<String>,
    /// What "before" is: the run's base branch (its merge base with the run), or the commit asked for.
    pub base: String,
    /// What "after" is, for two commits; a run's is its worktree.
    pub head: Option<String>,
    pub versions: Vec<Version>,
    pub created: i64,
    pub updated: i64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VersionState {
    Capturing,
    /// Captured: its screens wait for decisions.
    Ready,
    Failed,
    /// Every screen decided, and the outcome sent to the team.
    Done,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub n: u32,
    pub state: VersionState,
    /// The team's visual step it decides.
    pub step: Option<String>,
    /// The routes asked for instead of the project's and the agents'.
    pub only: Vec<String>,
    /// The commits captured.
    pub before: Option<String>,
    pub after: Option<String>,
    pub progress: Progress,
    pub screens: Vec<Screen>,
    /// What was not captured, or captured with a caveat.
    pub notes: Vec<String>,
    pub error: Option<String>,
    /// Rejected if a screen is, else changes if a screen asks for some, else approved.
    pub outcome: Option<Decision>,
    pub created: i64,
    pub finished: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// `prepare`, `before`, `after`, `diff`, `done`.
    pub phase: String,
    /// Captures done out of `total` (two per screen).
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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// `[visual] routes` of the project, or a route asked for.
    Route,
    /// `affected_views` of an agent's report.
    Agent,
    Storybook,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    /// Unique in its version: `03-invoices-mobile-dark`.
    pub id: String,
    pub source: Source,
    /// Path and query on the dev server.
    pub path: String,
    pub state: Option<String>,
    /// `desktop`, `tablet` or `mobile`, and its size.
    pub viewport: String,
    pub width: u32,
    pub height: u32,
    /// `light` or `dark`.
    pub theme: String,
    /// Image files in the version's folder.
    pub before: Option<String>,
    pub after: Option<String>,
    pub heatmap: Option<String>,
    /// Zones that changed, in pixels of the images.
    pub rects: Vec<Rect>,
    /// The share of pixels that changed.
    pub ratio: f64,
    /// Pixels or texts changed.
    pub changed: bool,
    pub texts: Vec<TextChange>,
    /// Why a side could not be captured.
    pub error: Option<String>,
    pub decision: Decision,
    pub comment: String,
    pub pins: Vec<Pin>,
    pub decided: Option<i64>,
}

/// A comment on a point of the "after" image, in its pixels.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pin {
    pub x: u32,
    pub y: u32,
    pub comment: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TextChange {
    /// `added` or `removed`.
    pub change: String,
    pub text: String,
    /// `ticket`, or the criterion an agent's `report.text_changes` gives.
    pub justified_by: Option<String>,
    /// The team's `text-changes` check found it in the code without justification.
    pub flagged: bool,
}

/// What to review: a ticket run, or two commits of a repository.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Request {
    pub run: Option<i64>,
    pub repo: Option<PathBuf>,
    pub base: Option<String>,
    pub head: Option<String>,
    /// Only these routes, instead of the project's and the agents'.
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

/// What the UI hears about.
#[derive(Clone, Debug)]
pub enum Update {
    Review(Box<Review>),
    Progress(ProgressEvent),
}

/// What a decided version tells the team's visual gate.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Approve,
    Reject,
    /// The pins, comments and image paths, for the agent.
    Changes(String),
}

/// Applies an outcome to the team: approve the gate, stop the run (its worktree stays), or revise the agent
/// step before the gate.
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
    /// Added to git's and the dev servers' environment.
    env: Vec<(String, String)>,
    shooter: Option<Shooter>,
    /// Every change of a review goes through it, one at a time.
    lock: Mutex<()>,
    sink: Box<dyn Fn(Update) + Send + Sync>,
    gate: Mutex<Option<Gate>>,
    /// Dev servers and browsers at work, killed when the app quits.
    children: Mutex<BTreeSet<u32>>,
}

/// The texts the ticket and the agents justify, and the ones the team's check flagged.
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
            }),
        };
        if let Err(e) = visual.recover() {
            eprintln!("termf: visual reviews left as they were: {e:#}");
        }
        visual
    }

    /// Where a decided version goes: the team's gate, through [`to_team`].
    pub fn on_gate(
        &self,
        gate: impl Fn(i64, &str, &Outcome) -> Result<()> + Send + Sync + 'static,
    ) {
        *self.inner.gate.lock().unwrap() = Some(Arc::new(gate));
    }

    /// The app quits: the dev servers and browsers at work are killed.
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

    /// The folder of a version's images.
    pub fn folder(&self, id: i64, version: u32) -> PathBuf {
        folder(&self.inner.data_dir, id, version)
    }

    /// Starts a new version of the review of a run (its team's visual gate, when it waits), or a new review of
    /// two commits. The capture goes on in the background.
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
            let review = self.of_run(&store, run)?;
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

    /// The team's hook: its visual gate opened on `step`.
    pub fn gate(&self, team: TeamRun, step: String) {
        let visual = self.clone();
        std::thread::spawn(move || {
            let inner = &visual.inner;
            let started = (|| {
                let _lock = inner.lock.lock().unwrap();
                let store = Store::open(&inner.data_dir)?;
                let review = visual.of_run(&store, team.run)?;
                visual.begin(&store, review, Some(team.clone()), Some(step), Vec::new())
            })();
            if let Err(e) = started {
                eprintln!("termf: visual review of run {}: {e:#}", team.run);
            }
        });
    }

    /// The review of a ticket run, or a new one (unsaved, id 0).
    fn of_run(&self, store: &Store, run: i64) -> Result<Review> {
        if let Some(review) = store.of_run(run)? {
            return Ok(review);
        }
        let inner = &self.inner;
        let ticket_run =
            termf_tickets::Tickets::new(inner.config_dir.clone(), inner.data_dir.clone())
                .run(run)?;
        let config = config::load(&inner.config_dir, Path::new(&ticket_run.repo))?;
        Ok(Review {
            id: 0,
            run: Some(run),
            project: config.project,
            repo: config.repo,
            repo_path: ticket_run.repo.clone().into(),
            title: ticket_run.title.clone(),
            ticket: Some(ticket_run.ticket.key.clone()),
            base: ticket_run
                .base
                .clone()
                .with_context(|| format!("run {run} has no base branch"))?,
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

    /// Changes version `n` of review `id` in the store.
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

    /// Prepares both sides, captures every screen on each, then compares them.
    fn capture(&self, id: i64, n: u32, team: Option<&TeamRun>) -> Result<()> {
        let inner = &self.inner;
        let review = self.review(id)?;
        let version = review
            .versions
            .iter()
            .find(|v| v.n == n)
            .context("no such version")?;
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
            let shots = self.side(side, wt, &config, &dev, &list, &dir, &mut |i, screen| {
                self.progress(id, n, side, k * list.len() + i, total, screen);
            })?;
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
            screen.changed = !screen.rects.is_empty() || !screen.texts.is_empty();
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

    /// Starts the dev server of a side (and Storybook's when a screen is a story), then captures each screen:
    /// one result per screen.
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
        let (server, _tracked) = start("visual", dev)?;
        let story = match list.iter().any(|s| s.source == Source::Storybook) {
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
        Ok(shots)
    }

    /// Records a screen's decision (`all`: every screen still pending), with its comment and pins when given.
    /// Once no screen is pending, the version is decided and its outcome goes to the team's gate.
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
        ensure!(
            decision != Decision::Pending,
            "decide approve, reject or changes"
        );
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
            s.decided = Some(now());
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
            if let (Some(run), Some(step), Some(gate)) = (review.run, version.step.clone(), gate) {
                let sent = match outcome {
                    Decision::Approved => Outcome::Approve,
                    Decision::Rejected => Outcome::Reject,
                    _ => Outcome::Changes(feedback(&inner.data_dir, id, version)),
                };
                if let Err(e) = gate(run, &step, &sent) {
                    store.save(&mut review)?;
                    self.emit(&review);
                    return Err(
                        e.context("your decisions are saved, but the team could not take them")
                    );
                }
            }
            let version = &mut review.versions[last];
            version.outcome = Some(outcome);
            version.state = VersionState::Done;
            version.finished = Some(now());
        }
        store.save(&mut review)?;
        self.emit(&review);
        Ok(review)
    }

    /// An image of a screen: `before`, `after` or `heatmap`.
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
}

/// `<data>/reviews/<id>/v<n>/`.
pub fn folder(data_dir: &Path, id: i64, version: u32) -> PathBuf {
    data_dir
        .join("reviews")
        .join(id.to_string())
        .join(format!("v{version}"))
}

/// What the agent reads: each screen that asks for changes, its comment, pins and images.
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
        // The check says `text "…" changed without …`.
        flagged: team
            .steps
            .iter()
            .flat_map(|s| s.violations.iter())
            .filter_map(|v| {
                let quoted = v.message.strip_prefix("text ")?;
                let end = quoted.find("\" changed")? + 1;
                serde_json::from_str::<String>(&quoted[..end]).ok()
            })
            .map(|t| normal(&t))
            .collect(),
    }
}

impl Justify {
    fn judge(&self, change: &str, text: &str) -> TextChange {
        let t = normal(text);
        let justified_by = match self.ticket.contains(&t) && !t.is_empty() {
            true => Some("ticket".to_owned()),
            false => self
                .declared
                .iter()
                .find(|(d, _)| d.contains(&t) || t.contains(d.as_str()))
                .map(|(_, by)| by.clone()),
        };
        TextChange {
            change: change.into(),
            text: text.into(),
            justified_by,
            flagged: self
                .flagged
                .iter()
                .any(|f| f == &t || (f.contains(&t) && !t.is_empty())),
        }
    }
}

/// A child process the app kills if it quits during a capture.
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

fn kill(pid: u32) {
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
