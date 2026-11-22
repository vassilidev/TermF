//! Whole team runs with the real Claude Code on Haiku against a GitHub repository the tester owns and may write to
//! (`TERMF_GH_SANDBOX=owner/name`: a small Node project whose checks are `npm test`; the real `gh` signed in), from
//! the ticket to the pull request, step by step. Everything made on GitHub is removed at the end (issues deleted,
//! pull requests closed, branches deleted), and the transcripts of the throwaway folders with `claude purge`.
//! Throwaway config, data and clone; your Claude Code account. Tens of cents per test.
//! `cargo test -p termf-cli --test sandbox -- --ignored --nocapture --test-threads 1`
#![cfg(unix)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use termf_agents::{AgentEvent, Agents};
use termf_team::plan::QuestionKind;
use termf_team::{Options, Question, RunState, StepKind, StepState, Team, TeamRun};
use termf_tickets::Tickets;

const ROLES: &[&str] = &[
    "po",
    "pm",
    "cto",
    "lead",
    "architect",
    "ux",
    "seo",
    "back",
    "front",
    "qa",
    "secops",
    "reviewer",
    "integrator",
    "expert",
    "brand",
    "copy",
    "motion",
    "dba",
    "docs",
    "integrations",
    "sre",
    "a11y",
    "ai",
    "analytics",
    "legal",
];

struct Lab {
    slug: String,
    root: PathBuf,
    config: PathBuf,
    data: PathBuf,
    repo: PathBuf,
    env: Vec<(String, String)>,
    issues: RefCell<Vec<String>>,
    branches: RefCell<Vec<String>>,
    prs: RefCell<Vec<String>>,
}

impl Lab {
    /// `None` when no sandbox repository is named. `project` is added to the project's file, `settings` to
    /// `termf.toml`.
    fn new(name: &str, project: &str, settings: &str) -> Option<Lab> {
        let Ok(slug) = std::env::var("TERMF_GH_SANDBOX") else {
            eprintln!("TERMF_GH_SANDBOX=owner/name is not set: skipped");
            return None;
        };
        let root = std::env::temp_dir().join(format!("termf-lab-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (config, data) = (root.join("config"), root.join("data"));
        std::fs::create_dir_all(config.join("projects")).unwrap();
        std::fs::create_dir_all(root.join("home")).unwrap();
        let gitconfig = root.join("gitconfig");
        std::fs::write(
            &gitconfig,
            "[user]\n\tname = TermF QA\n\temail = qa@termf.invalid\n\
             [credential \"https://github.com\"]\n\thelper = \n\thelper = !gh auth git-credential\n",
        )
        .unwrap();
        let env: Vec<(String, String)> = [
            ("GIT_CONFIG_GLOBAL", gitconfig.to_string_lossy().as_ref()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .to_vec();
        let repo = root.join("dépôt QA/sandbox");
        std::fs::create_dir_all(repo.parent().unwrap()).unwrap();
        let url = format!("https://github.com/{slug}.git");
        let lab = Lab {
            slug,
            root,
            config,
            data,
            repo,
            env,
            issues: RefCell::default(),
            branches: RefCell::default(),
            prs: RefCell::default(),
        };
        lab.git(
            &lab.root,
            &["clone", "-q", &url, &lab.repo.to_string_lossy()],
        );
        let mut termf = "[agents]\nclaude = \"claude\"\n".to_owned();
        for role in ROLES {
            termf.push_str(&format!(
                "[roles.{role}]\nmodel = \"haiku\"\neffort = \"low\"\neffort_range = [\"low\", \"low\"]\n"
            ));
        }
        termf.push_str(settings);
        std::fs::write(lab.config.join("termf.toml"), termf).unwrap();
        let text = format!(
            "name = \"QA\"\n[[repos]]\nname = \"sandbox\"\npath = {:?}\nchecks = \"npm test\"\n\
             [trackers.github]\ncomment = true\n{project}",
            lab.repo
        );
        std::fs::write(lab.config.join("projects/qa.toml"), text).unwrap();
        Some(lab)
    }

    /// As `new`, but the clone's origin is a local bare copy of the sandbox: its `main` may move, and nothing
    /// reaches GitHub.
    fn local(name: &str, project: &str, settings: &str) -> Option<Lab> {
        let lab = Lab::new(name, project, settings)?;
        let remote = lab.root.join("remote.git");
        let (repo, bare) = (lab.repo.to_string_lossy(), remote.to_string_lossy());
        lab.git(&lab.root, &["clone", "-q", "--bare", &repo, &bare]);
        lab.git(&lab.repo, &["remote", "set-url", "origin", &bare]);
        lab.git(&lab.repo, &["fetch", "-q", "origin"]);
        lab.git(&lab.repo, &["remote", "set-head", "origin", "main"]);
        Some(lab)
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn gh(&self, args: &[&str]) -> String {
        let out = Command::new("gh").args(args).output().unwrap();
        assert!(out.status.success(), "gh {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A new issue assigned to the tester; its number.
    fn issue(&self, title: &str, body: &str) -> String {
        let url = self.gh(&[
            "issue",
            "create",
            "-R",
            &self.slug,
            "--title",
            title,
            "--body",
            body,
            "--assignee",
            "@me",
        ]);
        let number = url.rsplit('/').next().unwrap().to_owned();
        self.issues.borrow_mut().push(number.clone());
        number
    }

    fn tickets(&self) -> Tickets {
        let mut t = Tickets::new(self.config.clone(), self.data.clone());
        t.env = self.env.clone();
        t.home = self.root.join("home");
        t
    }

    /// The app's agents and team on this lab, with the real `termf guard`.
    fn team(&self) -> (Agents, Team) {
        let (notifier, inbox) = termf_team::channel();
        let agents = Agents::build(
            self.config.clone(),
            self.data.clone(),
            self.env.clone(),
            move |u| {
                if let termf_agents::Update::Session(s) = u {
                    notifier.session(s.id);
                }
            },
        );
        agents.serve_guard(env!("CARGO_BIN_EXE_termf").into());
        let team = Team::build(
            self.config.clone(),
            self.data.clone(),
            agents.clone(),
            inbox,
            self.env.clone(),
            |_| {},
        );
        (agents, team)
    }

    /// Delivers ticket run `id` as the user does once the team is done, and returns the pull request.
    fn deliver(&self, id: i64) -> String {
        let t = self.tickets();
        let preview = t.deliver_preview(id).unwrap();
        println!(
            "delivery of run {id}: {} files, {} commits",
            preview.files.len(),
            preview.commits.len()
        );
        let run = t.run(id).unwrap();
        self.branches.borrow_mut().push(run.branch.clone().unwrap());
        let delivery = t.deliver(id, &preview.fingerprint).unwrap();
        assert_eq!(delivery.warning, None);
        let pr = delivery.run.pr_url.unwrap();
        self.prs.borrow_mut().push(pr.clone());
        pr
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        for pr in self.prs.borrow().iter() {
            let _ = Command::new("gh").args(["pr", "close", pr]).output();
        }
        for branch in self.branches.borrow().iter() {
            let refs = format!("repos/{}/git/refs/heads/{branch}", self.slug);
            let _ = Command::new("gh")
                .args(["api", "-X", "DELETE", &refs])
                .output();
        }
        for number in self.issues.borrow().iter() {
            let delete = ["issue", "delete", number, "-R", &self.slug, "--yes"];
            let _ = Command::new("gh").args(delete).output();
        }
        let _ = Command::new("claude")
            .args(["purge", &self.root.to_string_lossy(), "-y"])
            .output();
        let worktrees = self.data.join("worktrees");
        for folder in [&self.repo, &worktrees] {
            let _ = Command::new("claude")
                .args(["purge", &folder.to_string_lossy(), "-y"])
                .output();
        }
        if std::env::var_os("TERMF_LAB_KEEP").is_none() {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// What the tester answers: the CTO's proposal when there is one, else the first option, else a short text.
fn answer(q: &Question) -> Vec<String> {
    if !q.proposed.is_empty() {
        return match q.kind {
            QuestionKind::Multi => q.proposed.clone(),
            _ => q.proposed[..1].to_vec(),
        };
    }
    match q.kind {
        QuestionKind::Bool => vec!["yes".into()],
        _ if !q.options.is_empty() => vec![q.options[0].clone()],
        _ => vec!["Keep it as small as you can; your call.".into()],
    }
}

/// What the driver did, step by step, for the test to check.
#[derive(Default)]
struct Drive {
    approved: Vec<String>,
    answered: Vec<String>,
    permissions: Vec<String>,
}

/// Plays the user until the team stops being at work or `until` says so: approves each gate but those of `hold`
/// (the delivery gate with a reason when the definition of done is not met), answers every questionnaire, allows
/// each permission request. Prints each event of the run as it comes.
fn drive(
    agents: &Agents,
    team: &Team,
    id: i64,
    minutes: u64,
    hold: &[&str],
    mut until: impl FnMut(&TeamRun) -> bool,
) -> (TeamRun, Drive) {
    let deadline = Instant::now() + Duration::from_secs(minutes * 60);
    let (mut seen, mut drive) = (0, Drive::default());
    loop {
        for e in team.events(id, seen).unwrap() {
            seen = e.id;
            let message: String = e.message.chars().take(220).collect();
            println!(
                "  [{}] {} {}",
                e.kind,
                e.step.unwrap_or_default(),
                message.replace('\n', " / ")
            );
        }
        let run = team.run(id).unwrap();
        if until(&run) || !matches!(run.state, RunState::Running | RunState::Waiting) {
            return (run, drive);
        }
        assert!(Instant::now() < deadline, "the run took too long: {run:#?}");
        for (i, step) in run.steps.iter().enumerate() {
            let key = step.key.clone();
            match step.state {
                StepState::Waiting if hold.contains(&key.as_str()) => continue,
                StepState::Waiting => {
                    let passed = match team.approve(id, &key) {
                        Err(e) if step.kind == StepKind::Connector => {
                            println!("  (delivery gate refused: {e:#})");
                            team.approve_anyway(id, &key, "QA run: delivered as it is")
                        }
                        other => other,
                    };
                    passed.unwrap();
                    drive.approved.push(key);
                }
                StepState::Blocked if !run.questions(i).is_empty() => {
                    let answers: BTreeMap<String, Vec<String>> = run
                        .questions(i)
                        .iter()
                        .map(|q| (q.id.clone(), answer(q)))
                        .collect();
                    println!("  (answering {key}: {answers:?})");
                    team.answer(id, &key, answers, "").unwrap();
                    drive.answered.push(key);
                }
                StepState::Blocked => {
                    println!("  (blocked step {key}: {:?}; accepted as it is)", step.note);
                    team.approve(id, &key).unwrap();
                    drive.approved.push(key);
                }
                _ => continue,
            }
            break;
        }
        for session in agents.sessions().unwrap() {
            if session.state != termf_agents::State::Waiting {
                continue;
            }
            let events = agents.events(session.id, 0).unwrap();
            let asked = events.into_iter().rev().find_map(|e| match e.event {
                AgentEvent::Permission { request, tool, .. } => Some((request, tool)),
                _ => None,
            });
            if let Some((request, tool)) = asked {
                drive.permissions.push(tool);
                let _ = agents.answer(session.id, &request, true, None);
            }
        }
        std::thread::sleep(Duration::from_millis(700));
    }
}

fn summary(run: &TeamRun) {
    println!(
        "run {}: {} on {}, ${:.4}, routing {:?}",
        run.run,
        run.state.as_str(),
        run.workflow,
        run.cost,
        run.routing
            .as_ref()
            .map(|r| (&r.kind, &r.size, &r.squads, &r.skip, &r.add))
    );
    for s in &run.steps {
        println!(
            "  {:<22} {:<9} {:<10} ${:.4} {}",
            s.key,
            format!("{:?}", s.state),
            s.role.clone().unwrap_or_default(),
            s.cost,
            s.note
                .clone()
                .unwrap_or_default()
                .chars()
                .take(120)
                .collect::<String>()
        );
    }
}

/// Scenario 1: a small bug ticket, from the CTO's triage to the pull request that closes it.
#[test]
#[ignore]
fn real_sandbox_bug_to_pull_request() {
    let Some(lab) = Lab::new("bug", "", "") else {
        return;
    };
    let number = lab.issue(
        "divide(a, 0) should throw instead of returning Infinity",
        "`divide(1, 0)` in `src/calc.js` returns `Infinity`. It should throw a `RangeError` with the message \
         `division by zero`. Add a test in `test/calc.test.js`.",
    );
    let t = lab.tickets();
    let run = t
        .start_without_session(&format!("#{number}"), Some("qa"), None)
        .unwrap();
    let target = termf_team::Target::of(&run, &lab.config, &lab.data).unwrap();
    let (agents, team) = lab.team();
    team.start(target, Options::default()).unwrap();
    let (done, drive) = drive(&agents, &team, run.id, 20, &[], |_| false);
    summary(&done);
    team.shutdown();
    agents.shutdown();
    assert_eq!(done.state, RunState::Done, "{:?}", done.note);
    assert!(done.routing.is_some(), "the CTO routed the run");
    assert!(done.plan.as_ref().is_some_and(|p| !p.tasks.is_empty()));
    let step = |key: &str| done.steps.iter().find(|s| s.key == key).unwrap();
    assert_eq!(step("checks").state, StepState::Done, "npm test passed");
    assert_eq!(step("review").state, StepState::Done);
    assert!(
        drive.approved.contains(&"deliver".to_owned()),
        "{:?}",
        drive.approved
    );
    let worktree = PathBuf::from(run.worktree.clone().unwrap());
    let tests = Command::new("npm")
        .arg("test")
        .current_dir(&worktree)
        .output()
        .unwrap();
    assert!(tests.status.success(), "{tests:?}");
    let calc = std::fs::read_to_string(worktree.join("src/calc.js")).unwrap();
    assert!(
        calc.contains("RangeError") || calc.contains("throw"),
        "{calc}"
    );
    let journal = std::fs::read_to_string(lab.data.join(format!("runs/{}/journal.md", run.id)));
    println!("journal: {} bytes", journal.map(|j| j.len()).unwrap_or(0));

    let pr = lab.deliver(run.id);
    println!("pull request: {pr}");
    let body = lab.gh(&["pr", "view", &pr, "--json", "body,files", "-q", ".body"]);
    assert!(body.contains(&format!("Closes #{number}")), "{body}");
    let files = lab.gh(&["pr", "view", &pr, "--json", "files", "-q", ".files[].path"]);
    assert!(files.contains("src/calc.js"), "{files}");
    println!("real_sandbox_bug_to_pull_request: ${:.4}", done.cost);
}

/// Scenario 2: a vague request. The CTO asks its questions before it routes; once answered, the plan is made
/// (`plan` only, so the run stops there).
#[test]
#[ignore]
fn real_sandbox_vague_request_is_asked_about_then_planned() {
    let Some(lab) = Lab::new("vague", "", "") else {
        return;
    };
    let t = lab.tickets();
    let run = t
        .start_request("Make the calculator better.", "qa", None)
        .unwrap();
    let target = termf_team::Target::of(&run, &lab.config, &lab.data).unwrap();
    let (agents, team) = lab.team();
    let options = Options {
        plan_only: true,
        ..Options::default()
    };
    team.start(target, options).unwrap();
    let (done, drive) = drive(&agents, &team, run.id, 15, &[], |_| false);
    summary(&done);
    team.shutdown();
    agents.shutdown();
    t.close(run.id, true, || {}).unwrap();
    println!("answers: {:#?}", done.answers);
    println!("real_sandbox_vague_request: ${:.4}", done.cost);
    assert!(
        drive.answered.iter().any(|k| k == termf_team::TRIAGE),
        "the CTO asked before routing: {:?}",
        drive.answered
    );
    assert!(!done.answers.is_empty());
    assert_eq!(done.state, RunState::Paused, "{:?}", done.note);
    assert!(done.plan.as_ref().is_some_and(|p| !p.tasks.is_empty()));
}

/// Scenario 3: a ticket for several roles (the PO frames it, the architect decides, back, front and QA tasks,
/// the reviewer, secops), with the plan gate and the project's definition of done, to the pull request.
#[test]
#[ignore]
fn real_sandbox_many_roles_with_plan_gate_and_done() {
    let done_list = "[team]\ndone = [\"checks\", \"review\", \"security\"]\n";
    let Some(lab) = Lab::new("roles", done_list, "") else {
        return;
    };
    let workflows = lab.config.join("library/workflows");
    std::fs::create_dir_all(&workflows).unwrap();
    std::fs::write(
        workflows.join("full.toml"),
        "extends = \"default\"\n[steps.architecture]\nenabled = true\n",
    )
    .unwrap();
    let number = lab.issue(
        "Percentages: percent(value, total) and a small page to try it",
        "1. Back: add `percent(value, total)` to `src/calc.js` (value / total * 100, a `RangeError` \
         `total is zero` when total is 0).\n2. Front: a static page `web/index.html` with two number inputs and \
         the result, importing `../src/calc.js` as an ES module.\n3. QA: tests in `test/` for percent, and a test \
         that `web/index.html` exists and imports `calc.js`.\nThe architect decides how the page loads the module.",
    );
    let t = lab.tickets();
    let run = t
        .start_without_session(&format!("#{number}"), Some("qa"), None)
        .unwrap();
    let target = termf_team::Target::of(&run, &lab.config, &lab.data).unwrap();
    let (agents, team) = lab.team();
    let options = Options {
        workflow: Some("full".into()),
        ..Options::default()
    };
    team.start(target, options).unwrap();
    let (done, drive) = drive(&agents, &team, run.id, 30, &[], |_| false);
    summary(&done);
    team.shutdown();
    agents.shutdown();
    let roles: Vec<String> = done
        .plan
        .iter()
        .flat_map(|p| p.tasks.iter().map(|t| t.role.clone()))
        .collect();
    println!(
        "task roles: {roles:?}; approved {:?}; definition of done {:#?}",
        drive.approved, done.done
    );
    assert_eq!(done.state, RunState::Done, "{:?}", done.note);
    assert!(drive.approved.contains(&"approve-plan".to_owned()));
    let ran = |key: &str| {
        let step = done.steps.iter().find(|s| s.key == key);
        step.is_some_and(|s| s.state == StepState::Done && !s.sessions.is_empty())
    };
    for key in ["frame", "plan", "review", "security"] {
        assert!(ran(key), "{key} ran");
    }
    let gate = done
        .done
        .as_ref()
        .expect("the delivery gate read the definition of done");
    let ids: Vec<&str> = gate.criteria.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["checks", "review", "security"]);
    let pr = lab.deliver(run.id);
    let files = lab.gh(&["pr", "view", &pr, "--json", "files", "-q", ".files[].path"]);
    println!("pull request {pr}: {files}");
    assert!(
        files.contains("src/calc.js") && files.contains("web/index.html"),
        "{files}"
    );
    println!("real_sandbox_many_roles: ${:.4}", done.cost);
}

/// Scenario 4: nits are sent back too (`review_nits = "fix"`), so a finding makes a second round; meanwhile `main`
/// moves on the very lines the run changed, and the integrator merges it in. On a local copy of the sandbox.
#[test]
#[ignore]
fn real_local_review_rounds_and_integrator_on_a_real_conflict() {
    let Some(lab) = Lab::local("merge", "[team]\nreview_nits = \"fix\"\n", "") else {
        return;
    };
    let t = lab.tickets();
    let request = "In src/calc.js, divide(a, 0) must throw a RangeError(\"division by zero\"); add its test in \
                   test/calc.test.js. Nothing else; everything is decided.";
    let run = t.start_request(request, "qa", None).unwrap();
    let target = termf_team::Target::of(&run, &lab.config, &lab.data).unwrap();
    let (agents, team) = lab.team();
    team.start(target, Options::default()).unwrap();
    let checked = |r: &TeamRun| {
        r.steps
            .iter()
            .any(|s| s.key == "checks" && s.state == StepState::Done)
    };
    let (mid, _) = drive(&agents, &team, run.id, 15, &[], checked);
    assert!(checked(&mid), "{:?}", mid.state);

    // Someone changes the same line of divide on main.
    let other = lab.root.join("other");
    let remote = lab.root.join("remote.git");
    lab.git(
        &lab.root,
        &[
            "clone",
            "-q",
            &remote.to_string_lossy(),
            &other.to_string_lossy(),
        ],
    );
    let calc = other.join("src/calc.js");
    let text = std::fs::read_to_string(&calc).unwrap();
    let moved = text.replace("return a / b;", "return Number(a) / Number(b);");
    assert_ne!(moved, text);
    std::fs::write(&calc, moved).unwrap();
    lab.git(&other, &["commit", "-qam", "divide: coerce its operands"]);
    lab.git(&other, &["push", "-q", "origin", "main"]);

    let (done, drive) = drive(&agents, &team, run.id, 25, &[], |_| false);
    summary(&done);
    team.shutdown();
    agents.shutdown();
    for review in &done.reviews {
        let fates: Vec<String> = review
            .findings
            .iter()
            .map(|f| format!("{} {:?} {}:{} → {:?}", f.id, f.fate, f.file, f.line, f.to))
            .collect();
        println!(
            "review round {}: {} {fates:?}",
            review.round,
            review.summary.chars().take(160).collect::<String>()
        );
    }
    println!(
        "real_local_review_rounds_and_integrator: ${:.4}, approved {:?}, answered {:?}",
        done.cost, drive.approved, drive.answered
    );
    assert_eq!(done.state, RunState::Done, "{:?}", done.note);
    let update = done.steps.iter().find(|s| s.key == "update-base").unwrap();
    assert!(
        !update.sessions.is_empty(),
        "the integrator resolved the conflict: {update:#?}"
    );
    let worktree = PathBuf::from(run.worktree.clone().unwrap());
    let merged = std::fs::read_to_string(worktree.join("src/calc.js")).unwrap();
    assert!(
        merged.contains("Number(a)") && merged.contains("RangeError"),
        "both sides kept: {merged}"
    );
    let tests = Command::new("npm")
        .arg("test")
        .current_dir(&worktree)
        .output()
        .unwrap();
    assert!(tests.status.success(), "{tests:?}");
    let base = lab.git(&worktree, &["rev-parse", "origin/main"]);
    lab.git(&worktree, &["merge-base", "--is-ancestor", &base, "HEAD"]);
    t.close(run.id, true, || {}).unwrap();
}

/// Scenario 5: a ticket that changes a page. The visual gate captures it before and after with the real Chrome
/// (the repository's dev server, a desktop and a mobile viewport), the tester approves each screen, and the gate
/// passes. On a local copy of the sandbox, whose base gets the page first.
#[test]
#[ignore]
fn real_local_visual_gate_with_chrome() {
    let visual_settings = "[visual]\nroutes = [{ path = \"/web/index.html\", viewports = [\"desktop\", \"mobile\"] }]\n";
    let Some(lab) = Lab::local("visual", visual_settings, "") else {
        return;
    };
    let file = lab.config.join("projects/qa.toml");
    let project = std::fs::read_to_string(&file).unwrap();
    let dev = "dev = \"python3 -m http.server {{ port }} --bind 127.0.0.1\"\n";
    let project = project.replace(
        "checks = \"npm test\"\n",
        &format!("checks = \"npm test\"\n{dev}"),
    );
    std::fs::write(&file, project).unwrap();
    std::fs::create_dir_all(lab.repo.join("web")).unwrap();
    std::fs::write(
        lab.repo.join("web/index.html"),
        "<!doctype html>\n<meta charset=\"utf-8\">\n<title>Calculator</title>\n<h1>Calculator</h1>\n\
         <p>Add two numbers.</p>\n",
    )
    .unwrap();
    lab.git(&lab.repo, &["add", "-A"]);
    lab.git(&lab.repo, &["commit", "-qm", "web: a page"]);
    lab.git(&lab.repo, &["push", "-q", "origin", "main"]);
    lab.git(&lab.repo, &["fetch", "-q", "origin"]);

    let t = lab.tickets();
    let request = "In web/index.html, add under the heading a button labelled \"Compute\" with a blue background \
                   (#1d4ed8) and white text. Only that file; its look changes, so it is a UI change.";
    let run = t.start_request(request, "qa", None).unwrap();
    let target = termf_team::Target::of(&run, &lab.config, &lab.data).unwrap();
    let (agents, team) = lab.team();
    let team = std::sync::Arc::new(team);
    let visual = termf_visual::Visual::new(lab.config.clone(), lab.data.clone(), |_| {});
    let gate = team.clone();
    visual.on_gate(move |run, step, outcome| termf_visual::to_team(&gate, run, step, outcome));
    let hook = visual.clone();
    team.on_visual(move |run, step| hook.gate(run, step));
    team.start(target, Options::default()).unwrap();
    let waits = |r: &TeamRun| {
        let visual = r.steps.iter().find(|s| s.kind == StepKind::Visual);
        visual.is_some_and(|s| s.state == StepState::Waiting)
    };
    let (mid, _) = drive(&agents, &team, run.id, 20, &["visual"], waits);
    summary(&mid);
    assert!(
        waits(&mid),
        "the visual gate opened: ui_changed {}",
        mid.ui_changed
    );

    let deadline = Instant::now() + Duration::from_secs(300);
    let review = loop {
        let reviews = visual.reviews().unwrap();
        let found = reviews.into_iter().find(|r| r.run == Some(run.id));
        if let Some(r) = found.filter(|r| {
            r.versions
                .last()
                .is_some_and(|v| v.state != termf_visual::VersionState::Capturing)
        }) {
            break r;
        }
        assert!(Instant::now() < deadline, "no capture");
        std::thread::sleep(Duration::from_secs(2));
    };
    let version = review.versions.last().unwrap();
    println!(
        "visual review {}: {:?} {:?} {:?}",
        review.id, version.state, version.error, version.notes
    );
    for s in &version.screens {
        println!(
            "  {} {}x{} {} changed {} error {:?}",
            s.id, s.width, s.height, s.theme, s.changed, s.error
        );
    }
    assert_eq!(version.state, termf_visual::VersionState::Ready);
    assert!(version.screens.iter().all(|s| s.error.is_none()));
    assert!(
        version.screens.iter().any(|s| s.changed),
        "the button shows"
    );
    for s in &version.screens {
        let approved = termf_visual::Decision::Approved;
        visual
            .decide(review.id, &s.id, approved, None, None)
            .unwrap();
    }
    let (done, drive) = drive(&agents, &team, run.id, 20, &[], |_| false);
    summary(&done);
    visual.shutdown();
    team.shutdown();
    agents.shutdown();
    println!(
        "real_local_visual_gate_with_chrome: ${:.4}, approved {:?}",
        done.cost, drive.approved
    );
    let step = done
        .steps
        .iter()
        .find(|s| s.kind == StepKind::Visual)
        .unwrap();
    assert_eq!(
        step.state,
        StepState::Done,
        "the screens approved, the gate passed"
    );
    assert_eq!(done.state, RunState::Done, "{:?}", done.note);
    t.close(run.id, true, || {}).unwrap();
}

/// Scenario 7: two squads over two repositories. The CTO plans one batch per squad, each squad's lead splits its
/// batch into a sub-plan the tester approves, the tasks are grafted, and the second repository gets a run of its
/// own. On local copies (the sandbox, and a second repository with a local remote).
#[test]
#[ignore]
fn real_local_sub_plans_over_two_repositories() {
    let Some(lab) = Lab::local("s2", "", "") else {
        return;
    };
    let web = lab.root.join("web");
    let web_remote = lab.root.join("web.git");
    std::fs::create_dir_all(&web).unwrap();
    lab.git(
        &lab.root,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            &web_remote.to_string_lossy(),
        ],
    );
    lab.git(&web, &["init", "-q", "-b", "main"]);
    std::fs::write(
        web.join("README.md"),
        "# Web\n\nThe calculator's web page.\n",
    )
    .unwrap();
    lab.git(&web, &["add", "-A"]);
    lab.git(&web, &["commit", "-qm", "init"]);
    lab.git(
        &web,
        &["remote", "add", "origin", &web_remote.to_string_lossy()],
    );
    lab.git(&web, &["push", "-q", "-u", "origin", "main"]);
    lab.git(&web, &["remote", "set-head", "origin", "main"]);
    let squads = format!(
        "[[repos]]\nname = \"web\"\npath = {web:?}\n\
         [[squads]]\nid = \"server\"\nroles = {{ back = 1, qa = 1 }}\nrepos = [\"sandbox\"]\ndomains = [\"server\"]\n\
         [[squads]]\nid = \"client\"\nroles = {{ front = 1, qa = 1 }}\nrepos = [\"web\"]\ndomains = [\"interface\"]\n"
    );
    let file = lab.config.join("projects/qa.toml");
    let mut project = std::fs::read_to_string(&file).unwrap();
    project.push_str(&squads);
    std::fs::write(&file, project).unwrap();

    let t = lab.tickets();
    let request = "Two parts, one per squad, each split by its squad's lead into small tasks:\n\
                   - server, in the repository sandbox: add multiply(a, b) to src/calc.js, and its test in \
                   test/calc.test.js;\n\
                   - client, in the repository web: create index.html with a heading \"Multiply\" and two number \
                   inputs.\nEverything is decided.";
    let run = t.start_request(request, "qa", Some("sandbox")).unwrap();
    let target = termf_team::Target::of(&run, &lab.config, &lab.data).unwrap();
    let (agents, team) = lab.team();
    let options = Options {
        workflow: Some("squads".into()),
        ..Options::default()
    };
    team.start(target, options).unwrap();
    let (done, drive) = drive(&agents, &team, run.id, 30, &[], |_| false);
    summary(&done);
    team.shutdown();
    agents.shutdown();
    let plan = done
        .plan
        .clone()
        .unwrap_or_else(|| panic!("no plan: {done:#?}"));
    let tasks: Vec<String> = plan
        .tasks
        .iter()
        .map(|t| format!("{} {} {} {}", t.id, t.role, t.squad, t.repo))
        .collect();
    println!("tasks: {tasks:#?}");
    let batches: Vec<&termf_team::Step> = done.steps.iter().filter(|s| s.batch).collect();
    for b in &batches {
        let sub = b.subplan.as_ref().map(|p| p.tasks.len());
        println!(
            "batch {} by {:?}: {:?} tasks, {:?}",
            b.key, b.role, sub, b.state
        );
    }
    println!(
        "real_local_sub_plans_over_two_repositories: ${:.4}, approved {:?}, repos {:?}",
        done.cost,
        drive.approved,
        done.repos.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
    assert_eq!(done.state, RunState::Done, "{:?}", done.note);
    assert!(!batches.is_empty(), "the CTO gave the leads their batches");
    assert!(
        batches.iter().all(|b| b.subplan.is_some()),
        "each lead made its sub-plan"
    );
    assert!(
        plan.tasks.iter().any(|t| t.id.contains('.')),
        "the sub-plans' tasks were grafted"
    );
    assert!(
        done.repos.iter().any(|r| r.name == "web"),
        "the run took in web"
    );
    let children = t.children(run.id).unwrap();
    assert_eq!(children.len(), 1, "web got a run of its own");
    let web_tree = PathBuf::from(children[0].worktree.clone().unwrap());
    assert!(web_tree.join("index.html").is_file());
    let calc =
        std::fs::read_to_string(PathBuf::from(run.worktree.clone().unwrap()).join("src/calc.js"));
    assert!(calc.unwrap().contains("multiply"));
    t.close(run.id, true, || {}).unwrap();
}

/// Sessions still at work: a turn under way or a tool waiting for approval.
fn busy(agents: &Agents) -> Vec<i64> {
    let sessions = agents.sessions().unwrap().into_iter();
    let working = |s: &termf_agents::AgentSession| {
        matches!(
            s.state,
            termf_agents::State::Working | termf_agents::State::Waiting
        )
    };
    sessions.filter(working).map(|s| s.id).collect()
}

fn task_at_work(run: &TeamRun) -> bool {
    run.steps
        .iter()
        .any(|s| s.kind == StepKind::Task && s.state == StepState::Running && s.session.is_some())
}

/// Scenario 6: a budget spent stops the run cleanly; TermF restarting in the middle of a task pauses the run, and
/// starting it again takes the task up; an emergency stop ends every session at once; the run then goes on to the
/// end.
#[test]
#[ignore]
fn real_sandbox_budget_restart_and_emergency_stop() {
    let Some(lab) = Lab::new("stops", "", "") else {
        return;
    };
    let t = lab.tickets();
    let request = "In src/calc.js, divide(a, 0) must throw a RangeError(\"division by zero\"); add its test in \
                   test/calc.test.js. Nothing else; everything is decided.";

    // A budget smaller than one step.
    let poor = t.start_request(request, "qa", None).unwrap();
    let target = termf_team::Target::of(&poor, &lab.config, &lab.data).unwrap();
    let (agents, team) = lab.team();
    let options = Options {
        budget: Some(0.003),
        ..Options::default()
    };
    team.start(target, options).unwrap();
    let (spent, _) = drive(&agents, &team, poor.id, 5, &[], |_| false);
    summary(&spent);
    assert_eq!(spent.state, RunState::Budget, "{:?}", spent.note);
    assert_eq!(busy(&agents), Vec::<i64>::new(), "no session left at work");
    t.close(poor.id, true, || {}).unwrap();

    // TermF quits while a task is at work.
    let run = t.start_request(request, "qa", None).unwrap();
    let target = termf_team::Target::of(&run, &lab.config, &lab.data).unwrap();
    team.start(target.clone(), Options::default()).unwrap();
    let (mid, _) = drive(&agents, &team, run.id, 10, &[], task_at_work);
    assert!(task_at_work(&mid), "{:?}", mid.state);
    let before = mid.cost;
    team.shutdown();
    agents.shutdown();
    let (agents, team) = lab.team();
    let paused = team.run(run.id).unwrap();
    summary(&paused);
    assert_eq!(paused.state, RunState::Paused);
    let note = paused.note.clone().unwrap_or_default();
    assert!(note.contains("restarted"), "{note}");
    team.start(target.clone(), Options::default()).unwrap();

    // Then the user stops everything at once.
    let (again, _) = drive(&agents, &team, run.id, 10, &[], task_at_work);
    assert!(
        task_at_work(&again),
        "the task was taken up: {:?}",
        again.state
    );
    team.stop(Some(run.id)).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(team.run(run.id).unwrap().state, RunState::Stopped);
    assert_eq!(
        busy(&agents),
        Vec::<i64>::new(),
        "the stop ended every session"
    );

    team.start(target, Options::default()).unwrap();
    let (done, drive) = drive(&agents, &team, run.id, 20, &[], |_| false);
    summary(&done);
    team.shutdown();
    agents.shutdown();
    println!(
        "real_sandbox_budget_restart_and_emergency_stop: ${:.4} + ${:.4} (${before:.4} before the restart), approved {:?}",
        spent.cost, done.cost, drive.approved
    );
    assert_eq!(done.state, RunState::Done, "{:?}", done.note);
    let worktree = PathBuf::from(run.worktree.clone().unwrap());
    let tests = Command::new("npm")
        .arg("test")
        .current_dir(&worktree)
        .output()
        .unwrap();
    assert!(tests.status.success(), "{tests:?}");
    t.close(run.id, true, || {}).unwrap();
}
