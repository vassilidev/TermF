use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use termf_agents::Agents;
use termf_team::{Level, Options, RunState, Team, TeamUpdate};

use super::*;
use crate::diff::Image;

/// Each test has its own data folder, so its own port allocations: the ones that start dev servers take turns.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// A dev server for tests: Python's, serving the worktree's files.
const DEV: &str = "python3 -m http.server {{ port }} --bind 127.0.0.1";

/// The stand-in for Claude Code of the team's tests: answers each step from `<key>.json`, runs `<key>.sh <n>`
/// first, and keeps each message in `<log>.msg.<key>.<n>`.
const FAKE: &str = r#"
prev=
for a in "$@"; do
  if [ "$prev" = --session-id ] || [ "$prev" = --resume ]; then sid=$a; fi
  prev=$a
done
while IFS= read -r line; do
  case "$line" in
    *'"type":"user"'*) ;;
    *) continue ;;
  esac
  key=$(printf '%s' "$line" | sed -n 's/.*Step `\([^`]*\)`.*/\1/p')
  if [ -n "$key" ]; then printf '%s' "$key" > "$log.sid.$sid"; else key=$(cat "$log.sid.$sid"); fi
  n=$(( $(cat "$log.n.$key" 2>/dev/null || echo 0) + 1 ))
  echo "$n" > "$log.n.$key"
  printf '%s\n' "$line" > "$log.msg.$key.$n"
  printf '{"type":"system","subtype":"init","session_id":"%s","model":"claude-haiku-5-5","permissionMode":"acceptEdits"}\n' "$sid"
  if [ -f "$dir/$key.sh" ]; then sh "$dir/$key.sh" "$n"; fi
  ans="$dir/$key.json"
  if [ -f "$ans" ]; then body=$(cat "$ans"); else
    body="{\"status\":\"done\",\"summary\":\"$key done\",\"covers\":[],\"text_changes\":[],\"affected_views\":[],\"out_of_scope_findings\":[],\"questions\":[]}"
  fi
  printf '{"type":"result","subtype":"success","is_error":false,"result":"ok","total_cost_usd":0.01,"structured_output":%s,"session_id":"%s"}\n' "$body" "$sid"
done
"#;

struct Env {
    root: PathBuf,
    repo: PathBuf,
    config: PathBuf,
    data: PathBuf,
    env: Vec<(String, String)>,
    updates: Arc<Mutex<Vec<Update>>>,
}

fn git(dir: &Path, env: &[(String, String)], args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

const PAGE_BEFORE: &str = "<h1>Invoices</h1>\n<p>Total</p>\n";
const PAGE_AFTER: &str = "<h1>Invoices</h1>\n<p>Total</p>\n<button>Exporter en CSV</button>\n";

impl Env {
    fn new(name: &str, visual: &str) -> Env {
        let root = std::env::temp_dir().join(format!("termf-visual-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (repo, config, data, home) = (
            root.join("repo"),
            root.join("config"),
            root.join("data"),
            root.join("home"),
        );
        for d in [&repo, &config, &home, &data] {
            fs::create_dir_all(d).unwrap();
        }
        let env: Vec<(String, String)> = [
            ("HOME", home.to_string_lossy().into_owned()),
            ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("GIT_AUTHOR_NAME", "alex".into()),
            ("GIT_AUTHOR_EMAIL", "alex@example.invalid".into()),
            ("GIT_COMMITTER_NAME", "alex".into()),
            ("GIT_COMMITTER_EMAIL", "alex@example.invalid".into()),
            // A fake `gh` there.
            (
                "PATH",
                format!(
                    "{}:{}",
                    root.join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            ),
        ]
        .map(|(k, v)| (k.to_owned(), v))
        .to_vec();
        git(&repo, &env, &["init", "-q", "-b", "main"]);
        fs::write(repo.join("index.html"), PAGE_BEFORE).unwrap();
        git(&repo, &env, &["add", "-A"]);
        git(&repo, &env, &["commit", "-q", "-m", "init"]);
        fs::create_dir_all(config.join("projects")).unwrap();
        fs::write(
            config.join("projects/acme.toml"),
            format!(
                "name = \"Acme\"\n[[repos]]\nname = \"web\"\npath = \"{}\"\ndev = \"{DEV}\"\n{visual}",
                repo.display()
            ),
        )
        .unwrap();
        Env {
            root,
            repo,
            config,
            data,
            env,
            updates: Arc::default(),
        }
    }

    fn visual(&self) -> Visual {
        let updates = self.updates.clone();
        Visual::build(
            self.config.clone(),
            self.data.clone(),
            self.env.clone(),
            Some(Arc::new(shoot)),
            move |u| updates.lock().unwrap().push(u),
        )
    }

    fn commit(&self, page: &str) -> String {
        fs::write(self.repo.join("index.html"), page).unwrap();
        git(&self.repo, &self.env, &["commit", "-qam", "change"]);
        git(&self.repo, &self.env, &["rev-parse", "HEAD"])
    }
}

/// Waits for the last version of a review to leave `capturing`.
fn captured(visual: &Visual, id: i64) -> Version {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let review = visual.review(id).unwrap();
        let v = review.versions.last().unwrap().clone();
        if v.state != VersionState::Capturing {
            return v;
        }
        assert!(Instant::now() < deadline, "the capture never ended: {v:#?}");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn get(url: &str) -> Result<String> {
    let rest = url.strip_prefix("http://localhost:").context("not local")?;
    let (port, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let mut stream = TcpStream::connect(("127.0.0.1", port.parse::<u16>()?))?;
    write!(stream, "GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    let mut text = String::new();
    stream.read_to_string(&mut text)?;
    Ok(text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or_default())
}

/// Draws the page the dev server serves: a dark bar for each button, the lines as its texts.
fn shoot(target: &Target) -> Result<Shot> {
    let body = get(&target.url)?;
    let mut image = Image::new(target.width.min(320), 200, [255, 255, 255, 255]);
    for (i, line) in body.lines().enumerate() {
        if line.contains("<button") {
            image.fill(20, 20 + i as u32 * 30, 140, 24, [20, 20, 20, 255]);
        }
    }
    let texts = body
        .lines()
        .map(|l| {
            l.split(['<', '>'])
                .nth(2)
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .filter(|t| !t.is_empty())
        .collect();
    Ok(Shot {
        png: diff::encode(&image)?,
        texts,
    })
}

#[test]
fn reviews_two_commits_version_after_version() {
    let _serial = serial();
    let env = Env::new(
        "commits",
        "[visual]\nroutes = [\"/\"]\nthemes = [\"light\", \"dark\"]\n",
    );
    let base = git(&env.repo, &env.env, &["rev-parse", "HEAD"]);
    let head = env.commit(PAGE_AFTER);
    let visual = env.visual();
    let request = Request {
        repo: Some(env.repo.clone()),
        base: Some(base.clone()),
        head: Some("main".into()),
        ..Request::default()
    };
    let review = visual.start(request.clone()).unwrap();
    assert_eq!(review.versions[0].state, VersionState::Capturing);
    let v1 = captured(&visual, review.id);
    assert_eq!(v1.state, VersionState::Ready, "{v1:#?}");
    assert_eq!(
        (v1.before.as_deref(), v1.after.as_deref()),
        (Some(&*base), Some(&*head))
    );
    assert!(
        v1.notes.iter().any(|n| n.contains("public pages only")),
        "{:?}",
        v1.notes
    );
    let ids: Vec<&str> = v1.screens.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["01-home-desktop-light", "02-home-desktop-dark"]);
    let screen = &v1.screens[0];
    assert!(screen.changed && screen.error.is_none(), "{screen:#?}");
    assert_eq!(screen.rects.len(), 1);
    assert!(screen.rects[0].y <= 80 && screen.rects[0].y + screen.rects[0].h >= 104);
    assert_eq!(screen.texts.len(), 1);
    assert_eq!(
        (
            screen.texts[0].change.as_str(),
            screen.texts[0].text.as_str()
        ),
        ("added", "Exporter en CSV")
    );
    for kind in ["before", "after", "heatmap"] {
        let png = visual.image(review.id, 1, &screen.id, kind).unwrap();
        assert_eq!(diff::decode(&png).unwrap().width, 320);
    }
    assert!(visual.image(review.id, 1, &screen.id, "../x").is_err());
    let progress = env
        .updates
        .lock()
        .unwrap()
        .iter()
        .filter(|u| matches!(u, Update::Progress(_)))
        .count();
    assert!(progress >= 5, "{progress} progress events");

    // Nothing decided twice, nothing changed without saying what.
    let err = visual
        .decide(review.id, &screen.id, Decision::Changes, None, None)
        .unwrap_err();
    assert!(err.to_string().contains("say what to change"), "{err}");
    let pins = vec![Pin {
        x: 30,
        y: 90,
        comment: "8px more".into(),
    }];
    let review_now = visual
        .decide(
            review.id,
            &screen.id,
            Decision::Changes,
            Some("Too close".into()),
            Some(pins),
        )
        .unwrap();
    assert_eq!(
        review_now.versions[0].state,
        VersionState::Ready,
        "one screen is still pending"
    );
    let done = visual
        .decide(review.id, "all", Decision::Approved, None, None)
        .unwrap();
    let v1 = &done.versions[0];
    assert_eq!(
        (v1.state, v1.outcome),
        (VersionState::Done, Some(Decision::Changes))
    );
    assert_eq!(v1.screens[1].decision, Decision::Approved);
    assert_eq!(v1.screens[0].pins[0].comment, "8px more");
    assert!(
        visual
            .decide(review.id, &screen.id, Decision::Approved, None, None)
            .is_err()
    );

    // The next version of the same review, on what changed since, in the cached worktrees.
    let review = visual
        .start(Request {
            routes: vec!["/".into()],
            ..request
        })
        .unwrap();
    let v2 = captured(&visual, review.id);
    assert_eq!(
        (v2.n, v2.state, v2.screens.len()),
        (2, VersionState::Ready, 2)
    );
    assert!(
        folder(&env.data, review.id, 1)
            .join("01-home-desktop-light-heatmap.png")
            .is_file()
    );
    let cached = fs::read_dir(env.data.join("visual/worktrees/acme-web")).unwrap();
    let cached = cached
        .filter(|e| e.as_ref().unwrap().path().is_dir())
        .count();
    assert_eq!(
        cached, 2,
        "the before and after worktrees, cached by commit"
    );
    let stored = crate::reviews(&env.data).unwrap();
    assert_eq!(stored.len(), 1);
    let _ = fs::remove_dir_all(&env.root);
}

#[test]
fn a_dev_server_that_does_not_answer_fails_the_version() {
    let _serial = serial();
    let env = Env::new("dead", "[visual]\nroutes = [\"/\"]\ntimeout = 2\n");
    let file = env.config.join("projects/acme.toml");
    let text = fs::read_to_string(&file)
        .unwrap()
        .replace(DEV, "echo starting; sleep 30");
    fs::write(&file, text).unwrap();
    let head = env.commit(PAGE_AFTER);
    let visual = env.visual();
    let review = visual
        .start(Request {
            repo: Some(env.repo.clone()),
            base: Some("HEAD~1".into()),
            head: Some(head),
            ..Request::default()
        })
        .unwrap();
    let started = Instant::now();
    let v = captured(&visual, review.id);
    assert_eq!(v.state, VersionState::Failed);
    let error = v.error.unwrap();
    assert!(
        error.contains("did not answer") && error.contains("starting"),
        "{error}"
    );
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(
        visual.inner.children.lock().unwrap().is_empty(),
        "the dev server was stopped"
    );
    let err = visual
        .decide(review.id, "all", Decision::Approved, None, None)
        .unwrap_err();
    assert!(err.to_string().contains("failed"), "{err}");
    let _ = fs::remove_dir_all(&env.root);
}

#[test]
fn starts_and_stops_a_dev_server() {
    let dir = std::env::temp_dir().join(format!("termf-visual-server-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("index.html"), "hello").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut server =
        Server::start(DEV, &dir, &BTreeMap::new(), port, &dir.join("dev.log")).unwrap();
    server.wait(Duration::from_secs(30)).unwrap();
    assert!(env::answers(port));
    drop(server);
    let deadline = Instant::now() + Duration::from_secs(5);
    while env::answers(port) {
        assert!(Instant::now() < deadline, "still answering once stopped");
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut ended = Server::start(
        "echo broken >&2; exit 3",
        &dir,
        &BTreeMap::new(),
        port,
        &dir.join("x.log"),
    )
    .unwrap();
    let err = ended.wait(Duration::from_secs(10)).unwrap_err().to_string();
    assert!(err.contains("ended") && err.contains("broken"), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

fn task_plan() -> Value {
    json!({
        "summary": "Export the invoices",
        "acceptance": [{"id": "AC1", "text": "a CSV export button"}],
        "out_of_scope": [],
        "questions": [],
        "tasks": [{"id": "T1", "role": "front", "repo": "web", "size": "S", "covers": ["AC1"],
                   "goal": "add the button", "files_hint": ["index.html"], "depends_on": [], "ui_impact": true}],
    })
}

/// The team of a ticket run, with the fake Claude Code, and the visual review on its gate.
struct TeamEnv {
    env: Env,
    log: String,
    team: Arc<Team>,
    visual: Visual,
    _agents: Agents,
}

impl TeamEnv {
    fn new(name: &str) -> TeamEnv {
        let env = Env::new(name, "[visual]\nroutes = [\"/\"]\n");
        let (root, data, config) = (&env.root, &env.data, &env.config);
        let answers = root.join("answers");
        fs::create_dir_all(&answers).unwrap();
        let worktree = data.join("worktrees/run1");
        git(
            &env.repo,
            &env.env,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "termf/1-export",
                &worktree.to_string_lossy(),
            ],
        );
        // The ticket run, as `termf run` leaves it.
        termf_tickets::Tickets::new(config.clone(), data.clone())
            .runs()
            .unwrap();
        let db = rusqlite::Connection::open(data.join("termf.db")).unwrap();
        db.execute(
            "INSERT INTO runs (id, project, provider, ticket_key, ticket_url, title, repo, worktree, branch, base, status, created, updated)
             VALUES (1, 'acme', 'github', 'acme/web#1', 'https://example.invalid/1', 'Export invoices', ?1, ?2, 'termf/1-export', 'main', 'running', 0, 0)",
            rusqlite::params![env.repo.to_string_lossy(), worktree.to_string_lossy()],
        )
        .unwrap();
        fs::create_dir_all(data.join("runs/1")).unwrap();
        fs::write(data.join("runs/1/journal.md"), "# Export invoices\n").unwrap();
        fs::create_dir_all(data.join("runs/1/brief")).unwrap();
        fs::write(
            data.join("runs/1/brief/prompt.md"),
            "<ticket source=\"https://example.invalid/1\">\n# Export invoices\n\nAdd a CSV export.\n</ticket>",
        )
        .unwrap();

        let fake = root.join("bin/claude");
        fs::create_dir_all(fake.parent().unwrap()).unwrap();
        let log = root.join("fake").to_string_lossy().into_owned();
        fs::write(
            &fake,
            format!(
                "#!/bin/sh\nlog='{log}'\ndir='{}'\n{FAKE}",
                answers.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            config.join("termf.toml"),
            format!("[agents]\nclaude = \"{}\"\n", fake.display()),
        )
        .unwrap();
        fs::create_dir_all(config.join("library/workflows")).unwrap();
        fs::write(
            config.join("library/workflows/visual.toml"),
            "[steps.plan]\ntype = \"agent\"\nrole = \"orchestrator\"\n[steps.implement]\ntype = \"foreach\"\nover = \"plan.tasks\"\nneeds = [\"plan\"]\n[steps.visual]\ntype = \"visual\"\nneeds = [\"implement\"]\n",
        )
        .unwrap();
        fs::write(answers.join("plan.json"), task_plan().to_string()).unwrap();
        let report = json!({"status": "done", "summary": "button added", "covers": ["AC1"],
            "text_changes": [{"file": "index.html", "text": "Exporter en CSV", "justified_by": "AC1"}],
            "affected_views": [{"url": "http://localhost:5173/", "state": "list", "viewports": ["mobile"]}],
            "out_of_scope_findings": [], "questions": []});
        fs::write(answers.join("implement:T1.json"), report.to_string()).unwrap();
        fs::write(
            answers.join("implement:T1.sh"),
            format!("printf '%s' '{PAGE_AFTER}' > index.html; [ \"$1\" = 2 ] && echo '<p>v2</p>' >> index.html; true\n"),
        )
        .unwrap();

        let (notifier, inbox) = termf_team::channel();
        let agents = Agents::build(config.clone(), data.clone(), env.env.clone(), move |u| {
            if let termf_agents::Update::Session(s) = u {
                notifier.session(s.id);
            }
        });
        let team = Team::build(
            config.clone(),
            data.clone(),
            agents.clone(),
            inbox,
            env.env.clone(),
            |_: TeamUpdate| {},
        );
        let visual = env.visual();
        let team = Arc::new(team);
        let gate_team = team.clone();
        visual.on_gate(move |run, step, outcome| to_team(&gate_team, run, step, outcome));
        let hook = visual.clone();
        team.on_visual(move |run, step| hook.gate(run, step));
        TeamEnv {
            env,
            log,
            team,
            visual,
            _agents: agents,
        }
    }

    fn target(&self) -> termf_team::Target {
        let run = termf_tickets::Tickets::new(self.env.config.clone(), self.env.data.clone())
            .run(1)
            .unwrap();
        termf_team::Target::of(&run, &self.env.config, &self.env.data).unwrap()
    }

    /// Waits for the review of run 1 to have version `n` captured.
    fn version(&self, n: u32) -> (i64, Version) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(review) = reviews(&self.env.data)
                .unwrap()
                .into_iter()
                .find(|r| r.run == Some(1))
                && let Some(v) = review.versions.iter().find(|v| v.n == n)
                && v.state != VersionState::Capturing
            {
                return (review.id, v.clone());
            }
            let team = self.team.run(1).ok().map(|r| {
                (
                    r.state,
                    r.steps
                        .iter()
                        .map(|s| (s.key.clone(), s.state, s.note.clone()))
                        .collect::<Vec<_>>(),
                )
            });
            assert!(Instant::now() < deadline, "no v{n}: {team:#?}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn message(&self, key: &str, n: u32) -> String {
        let line = fs::read_to_string(format!("{}.msg.{key}.{n}", self.log)).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        value["message"]["content"].as_str().unwrap().to_owned()
    }
}

#[test]
fn the_team_gate_opens_a_review_and_takes_its_decisions() {
    let _serial = serial();
    let t = TeamEnv::new("gate");
    let options = Options {
        workflow: Some("visual".into()),
        rules: [("visual-review".to_owned(), Level::Warn)].into(),
        ..Options::default()
    };
    t.team.start(t.target(), options).unwrap();
    let (id, v1) = t.version(1);
    assert_eq!(v1.state, VersionState::Ready, "{v1:#?}");
    assert_eq!(v1.step.as_deref(), Some("visual"));
    let ids: Vec<&str> = v1.screens.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        ["01-home-desktop-light", "02-home-mobile-light"],
        "the route, then the agent's view"
    );
    assert_eq!(v1.screens[1].source, Source::Agent);
    let text = &v1.screens[0].texts[0];
    assert_eq!(
        (
            text.text.as_str(),
            text.justified_by.as_deref(),
            text.flagged
        ),
        ("Exporter en CSV", Some("AC1"), false)
    );
    assert!(
        t.team
            .run(1)
            .unwrap()
            .steps
            .last()
            .unwrap()
            .note
            .as_deref()
            .unwrap()
            .contains("termf review show --run 1")
    );

    // Changes: the agent gets the pins, the comment and the images, then v2 comes.
    let pins = vec![Pin {
        x: 40,
        y: 100,
        comment: "8px more on the left".into(),
    }];
    t.visual
        .decide(
            id,
            "01-home-desktop-light",
            Decision::Changes,
            Some("Too close to the title".into()),
            Some(pins),
        )
        .unwrap();
    let review = t
        .visual
        .decide(id, "02-home-mobile-light", Decision::Approved, None, None)
        .unwrap();
    assert_eq!(review.versions[0].outcome, Some(Decision::Changes));
    let (_, v2) = t.version(2);
    assert_eq!(v2.state, VersionState::Ready, "{v2:#?}");
    let message = t.message("implement:T1", 2);
    assert!(
        message.contains("Too close to the title") && message.contains("8px more on the left"),
        "{message}"
    );
    assert!(
        message.contains(
            &folder(&t.env.data, id, 1)
                .join("01-home-desktop-light-after.png")
                .display()
                .to_string()
        ),
        "{message}"
    );
    assert!(
        v2.screens[0].texts.iter().any(|t| t.text == "v2"),
        "{:?}",
        v2.screens[0].texts
    );

    // Approved: the run goes on to its end.
    t.visual
        .decide(id, "all", Decision::Approved, None, None)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while t.team.run(1).unwrap().state != RunState::Done {
        assert!(Instant::now() < deadline, "the run never ended");
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = fs::remove_dir_all(&t.env.root);
}

#[test]
fn rejecting_stops_the_run_and_keeps_its_worktree() {
    let _serial = serial();
    let t = TeamEnv::new("reject");
    let options = Options {
        workflow: Some("visual".into()),
        rules: [("visual-review".to_owned(), Level::Warn)].into(),
        ..Options::default()
    };
    t.team.start(t.target(), options).unwrap();
    let (id, _) = t.version(1);
    t.visual
        .decide(
            id,
            "01-home-desktop-light",
            Decision::Rejected,
            Some("Not this".into()),
            None,
        )
        .unwrap();
    let review = t
        .visual
        .decide(id, "all", Decision::Approved, None, None)
        .unwrap();
    assert_eq!(review.versions[0].outcome, Some(Decision::Rejected));
    let run = t.team.run(1).unwrap();
    assert_eq!(run.state, RunState::Stopped);
    assert!(run.target.worktree.is_dir());
    let _ = fs::remove_dir_all(&t.env.root);
}

#[test]
fn posts_the_images_on_the_pull_request_once_confirmed() {
    let _serial = serial();
    let t = TeamEnv::new("pr");
    let env = &t.env;
    let worktree = env.data.join("worktrees/run1");
    fs::write(worktree.join("index.html"), PAGE_AFTER).unwrap();
    git(&worktree, &env.env, &["commit", "-qam", "button"]);
    // Reviewed without a team: no gate to tell.
    let review = t
        .visual
        .start(Request {
            run: Some(1),
            ..Request::default()
        })
        .unwrap();
    let (id, v1) = t.version(1);
    assert_eq!((id, v1.step.as_deref()), (review.id, None));
    let err = t.visual.pr_preview(id).unwrap_err().to_string();
    assert!(err.contains("pr_images"), "{err}");
    let file = env.config.join("projects/acme.toml");
    fs::write(
        &file,
        fs::read_to_string(&file).unwrap() + "pr_images = true\n",
    )
    .unwrap();
    assert!(
        t.visual
            .pr_preview(id)
            .unwrap_err()
            .to_string()
            .contains("deliver it first")
    );
    let origin = env.root.join("origin.git");
    git(
        &env.root,
        &env.env,
        &["init", "-q", "--bare", &origin.to_string_lossy()],
    );
    git(
        &env.repo,
        &env.env,
        &["remote", "add", "origin", &origin.to_string_lossy()],
    );
    let db = rusqlite::Connection::open(env.data.join("termf.db")).unwrap();
    db.execute(
        "UPDATE runs SET pr_url = 'https://github.com/acme/web/pull/7' WHERE id = 1",
        [],
    )
    .unwrap();
    assert!(
        t.visual
            .pr_preview(id)
            .unwrap_err()
            .to_string()
            .contains("no version decided")
    );
    t.visual
        .decide(id, "all", Decision::Approved, None, None)
        .unwrap();

    let preview = t.visual.pr_preview(id).unwrap();
    assert_eq!(
        preview.fingerprint,
        t.visual.pr_preview(id).unwrap().fingerprint,
        "the same images, the same commit"
    );
    assert_eq!(preview.branch, format!("termf-visual/{id}-v1"));
    assert_eq!(
        preview.files.len(),
        2,
        "before and after of the one screen: {:?}",
        preview.files
    );
    let after = format!(
        "https://github.com/acme/web/blob/{}/v1/01-home-desktop-light-after.png?raw=true",
        preview.fingerprint
    );
    assert!(preview.body.contains(&after), "{}", preview.body);
    assert!(
        !preview.body.contains(&env.data.display().to_string()),
        "no local path"
    );
    assert!(t.visual.pr_comment(id, "0000").is_err());
    let gh = env.root.join("bin/gh");
    fs::write(
        &gh,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{0}.args'\ncat > '{0}.body'\n",
            gh.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    t.visual.pr_comment(id, &preview.fingerprint).unwrap();
    let pushed = git(
        &origin,
        &env.env,
        &["ls-tree", "-r", "--name-only", &preview.branch],
    );
    let mut sent = preview.files.clone();
    sent.sort();
    assert_eq!(pushed.lines().collect::<Vec<_>>(), sent);
    let args = fs::read_to_string(env.root.join("bin/gh.args")).unwrap();
    assert_eq!(
        args.trim(),
        "pr comment https://github.com/acme/web/pull/7 --body-file -"
    );
    assert_eq!(
        fs::read_to_string(env.root.join("bin/gh.body")).unwrap(),
        preview.body
    );
    let _ = fs::remove_dir_all(&env.root);
}

/// With the Chrome of the machine: `cargo test -p termf-visual -- --ignored real_chrome`.
#[test]
#[ignore]
fn real_chrome_captures_a_page() {
    let dir = std::env::temp_dir().join(format!("termf-visual-chrome-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("index.html"),
        "<!doctype html><style>body{margin:0;background:#fff}@media (prefers-color-scheme: dark){body{background:#000;color:#fff}}\
         .spin{animation:spin 1s infinite}@keyframes spin{to{transform:rotate(360deg)}}</style>\
         <h1>Invoices</h1><p class=\"date\">Now</p><input placeholder=\"Search\"><div class=\"spin\">x</div>\
         <div style=\"height:2000px\"></div><p>Bottom</p>",
    )
    .unwrap();
    fs::write(
        dir.join("login.html"),
        "<form action=\"/index.html\"><input id=\"email\" name=\"email\"><input id=\"password\" name=\"password\" type=\"password\">\
         <button>Sign in</button></form>",
    )
    .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut server =
        Server::start(DEV, &dir, &BTreeMap::new(), port, &dir.join("dev.log")).unwrap();
    server.wait(Duration::from_secs(30)).unwrap();
    let mut browser = Browser::launch(&browser::find().unwrap()).unwrap();
    let login = config::Login {
        url: "/login.html".into(),
        fields: [("#email", "alex@example.test"), ("#password", "secret")]
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .into(),
        submit: Some("button".into()),
        wait: Some("/index.html".into()),
    };
    browser
        .login(&format!("http://localhost:{port}"), &login)
        .unwrap();
    let log = fs::read_to_string(dir.join("dev.log")).unwrap();
    assert!(
        log.contains("GET /index.html?email=alex%40example.test&password=secret"),
        "{log}"
    );
    let mut target = Target {
        side: "after",
        url: format!("http://localhost:{port}/"),
        width: 390,
        height: 844,
        mobile: true,
        theme: "light".into(),
        mask: vec![".date".into()],
    };
    let light = browser.capture(&target).unwrap();
    let image = diff::decode(&light.png).unwrap();
    assert_eq!(image.width, 390);
    assert!(image.height > 2000, "the whole page: {}", image.height);
    assert!(
        light.texts.contains(&"Invoices".to_owned()) && light.texts.contains(&"Search".to_owned())
    );
    assert!(light.texts.contains(&"Bottom".to_owned()));
    assert!(
        !light.texts.contains(&"Now".to_owned()),
        "masked: {:?}",
        light.texts
    );
    let again = browser.capture(&target).unwrap();
    assert!(
        diff::compare(&image, &diff::decode(&again.png).unwrap(), diff::THRESHOLD)
            .rects
            .is_empty(),
        "frozen"
    );
    target.theme = "dark".into();
    let dark = diff::decode(&browser.capture(&target).unwrap().png).unwrap();
    assert!(dark.rgba[0] < 30, "dark background");
    drop(browser);
    drop(server);
    let _ = fs::remove_dir_all(&dir);
}
