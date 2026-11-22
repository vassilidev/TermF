use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use termf_agents::Agents;

use super::*;

/// A stand-in for Claude Code that reads stream-json and answers each step from files a test writes: for the
/// step `<key>` (read from the brief, or remembered for the conversation), its `<n>`th message runs
/// `<key>.sh <n>` in the session's folder, then answers `<key>.<n>.json`, else `<key>.json`, else a report that
/// says it is done; `NONE` answers plain text. Each turn costs `<key>.cost`, else `cost`, cents (1 by default).
/// It logs each step's begin and end, and the messages.
const FAKE: &str = r#"
prev=
for a in "$@"; do
  if [ "$prev" = --session-id ] || [ "$prev" = --resume ]; then sid=$a; fi
  prev=$a
done
printf '%s\n' "$*" >> "$log.args"
spent=0
cost=$(cat "$dir/cost" 2>/dev/null || echo 1)
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
  echo "$key $n begin" >> "$log.steps"
  printf '{"type":"system","subtype":"init","session_id":"%s","model":"claude-haiku-5-5","permissionMode":"acceptEdits"}\n' "$sid"
  if [ -f "$dir/$key.sh" ]; then sh "$dir/$key.sh" "$n"; fi
  ans="$dir/$key.$n.json"
  [ -f "$ans" ] || ans="$dir/$key.json"
  echo "$key $n end" >> "$log.steps"
  spent=$((spent + $(cat "$dir/$key.cost" 2>/dev/null || echo "$cost")))
  total=$(printf '%d.%02d' $((spent / 100)) $((spent % 100)))
  if [ -f "$ans" ] && [ "$(cat "$ans")" = NONE ]; then
    printf '{"type":"result","subtype":"success","is_error":false,"result":"I did it.","total_cost_usd":%s,"session_id":"%s"}\n' "$total" "$sid"
    continue
  fi
  if [ -f "$ans" ]; then body=$(cat "$ans"); else
    body="{\"status\":\"done\",\"summary\":\"$key done\",\"covers\":[],\"text_changes\":[],\"affected_views\":[],\"out_of_scope_findings\":[],\"questions\":[]}"
  fi
  printf '{"type":"result","subtype":"success","is_error":false,"result":"ok","total_cost_usd":%s,"structured_output":%s,"session_id":"%s"}\n' "$total" "$body" "$sid"
done
"#;

const LEAN: &str = r#"
max_parallel_agents = 3
[steps.plan]
type = "agent"
role = "orchestrator"
[steps.approve-plan]
type = "gate"
needs = ["plan"]
[steps.implement]
type = "foreach"
over = "plan.tasks"
needs = ["approve-plan"]
[steps.rules]
type = "rules"
needs = ["implement"]
"#;

struct Env {
    root: PathBuf,
    config: PathBuf,
    data: PathBuf,
    answers: PathBuf,
    log: String,
    env: Vec<(String, String)>,
    agents: Agents,
    team: Team,
    target: Target,
    events: Arc<Mutex<Vec<TeamUpdate>>>,
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

impl Env {
    fn new(name: &str) -> Env {
        let root = std::env::temp_dir().join(format!("termf-team-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (repo, config, data, home, answers) = (
            root.join("repo"),
            root.join("config"),
            root.join("data"),
            root.join("home"),
            root.join("answers"),
        );
        for d in [&repo, &config, &home, &answers] {
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
        ]
        .map(|(k, v)| (k.to_owned(), v))
        .to_vec();
        git(&repo, &env, &["init", "-q", "-b", "main"]);
        fs::write(repo.join("README.md"), "# Acme\n").unwrap();
        git(&repo, &env, &["add", "-A"]);
        git(&repo, &env, &["commit", "-q", "-m", "init"]);
        fs::create_dir_all(data.join("runs/1")).unwrap();
        fs::write(
            data.join("runs/1/journal.md"),
            "# Export invoices\n\n## Journal\n\n",
        )
        .unwrap();
        let worktree = data.join("worktrees/run1");
        fs::create_dir_all(worktree.parent().unwrap()).unwrap();
        git(
            &repo,
            &env,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "termf/1-export",
                &worktree.to_string_lossy(),
            ],
        );

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
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            config.join("termf.toml"),
            // The tests of the triage turn it on for their project.
            format!(
                "[agents]\nclaude = \"{}\"\n[team]\ntriage = false\n",
                fake.display()
            ),
        )
        .unwrap();
        fs::create_dir_all(config.join("projects")).unwrap();
        fs::write(
            config.join("projects/acme.toml"),
            format!(
                "name = \"Acme\"\n[[repos]]\nname = \"api\"\npath = \"{}\"\nchecks = \"test -f src/api/a.txt\"\n",
                repo.display()
            ),
        )
        .unwrap();
        fs::create_dir_all(config.join("library/workflows")).unwrap();
        fs::write(config.join("library/workflows/lean.toml"), LEAN).unwrap();
        fs::write(
            config.join("library/workflows/nogate.toml"),
            "extends = \"lean\"\n[steps.approve-plan]\nenabled = false\n",
        )
        .unwrap();
        let target = Target {
            run: 1,
            project: "acme".into(),
            repo: "api".into(),
            repo_path: repo.clone(),
            worktree,
            ticket: "acme/api#1".into(),
            title: "Export invoices".into(),
            brief: "<ticket source=\"https://example.invalid/1\">\n# Export invoices\n\nAdd a button labelled \"Exporter en CSV\".\n</ticket>".into(),
        };
        let events: Arc<Mutex<Vec<TeamUpdate>>> = Arc::default();
        let (agents, team) = Self::start(&config, &data, &env, events.clone());
        Env {
            root,
            config,
            data,
            answers,
            log,
            env,
            agents,
            team,
            target,
            events,
        }
    }

    fn start(
        config: &Path,
        data: &Path,
        env: &[(String, String)],
        events: Arc<Mutex<Vec<TeamUpdate>>>,
    ) -> (Agents, Team) {
        let (notifier, inbox) = channel();
        let agents = Agents::build(config.into(), data.into(), env.to_vec(), move |u| {
            if let termf_agents::Update::Session(s) = u {
                notifier.session(s.id);
            }
        });
        let team = Team::build(
            config.into(),
            data.into(),
            agents.clone(),
            inbox,
            env.to_vec(),
            move |u| events.lock().unwrap().push(u),
        );
        (agents, team)
    }

    /// What TermF is after a restart: the processes killed, a new team on the same data.
    fn restart(&mut self) {
        self.team.shutdown();
        self.agents.shutdown();
        // In the app, the threads that saw the processes end are gone with it.
        let deadline = Instant::now() + Duration::from_secs(10);
        while termf_agents::sessions(&self.data).unwrap().iter().any(|s| {
            matches!(
                s.state,
                termf_agents::State::Working | termf_agents::State::Idle
            )
        }) {
            assert!(Instant::now() < deadline, "the killed sessions never ended");
            std::thread::sleep(Duration::from_millis(50));
        }
        let (agents, team) = Self::start(&self.config, &self.data, &self.env, self.events.clone());
        self.agents = agents;
        self.team = team;
    }

    fn answer(&self, file: &str, value: Value) {
        fs::write(self.answers.join(file), value.to_string()).unwrap();
    }

    fn script(&self, key: &str, text: &str) {
        fs::write(self.answers.join(format!("{key}.sh")), text).unwrap();
    }

    fn plan(&self, tasks: Value) -> Value {
        json!({
            "summary": "Export the invoices",
            "acceptance": [{"id": "AC1", "text": "a CSV export"}, {"id": "AC2", "text": "a button"}],
            "out_of_scope": ["PDF"],
            "questions": [],
            "tasks": tasks,
        })
    }

    fn run(&self) -> TeamRun {
        self.team.run(1).unwrap()
    }

    fn until(&self, what: &str, done: impl Fn(&TeamRun) -> bool) -> TeamRun {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let run = self.run();
            if done(&run) {
                return run;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {:#?}\n{}",
                run.steps
                    .iter()
                    .map(|s| (&s.key, s.state, &s.note))
                    .collect::<Vec<_>>(),
                self.team
                    .events(1, 0)
                    .unwrap()
                    .iter()
                    .map(|e| format!("{} {:?} {}", e.kind, e.step, e.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn state(&self, state: RunState) -> TeamRun {
        self.until(&format!("{state:?}"), |r| r.state == state)
    }

    fn step<'a>(&self, run: &'a TeamRun, key: &str) -> &'a Step {
        run.steps
            .iter()
            .find(|s| s.key == key)
            .unwrap_or_else(|| panic!("no step {key}"))
    }

    fn steps_log(&self) -> Vec<String> {
        fs::read_to_string(format!("{}.steps", self.log))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn message(&self, key: &str, n: u32) -> String {
        let line = fs::read_to_string(format!("{}.msg.{key}.{n}", self.log)).unwrap();
        let value: Value = serde_json::from_str(&line).unwrap();
        value["message"]["content"].as_str().unwrap().to_owned()
    }

    fn kinds(&self) -> Vec<String> {
        self.team
            .events(1, 0)
            .unwrap()
            .into_iter()
            .map(|e| e.kind)
            .collect()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        self.team.shutdown();
        self.agents.shutdown();
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn question(id: &str, kind: &str, options: &[&str], proposed: &[&str]) -> Value {
    let text = match id {
        "format" => "CSV or Excel?".to_owned(),
        id => format!("{id}?"),
    };
    json!({"id": id, "theme": "product", "question": text, "why": "it changes the export", "kind": kind,
        "options": options, "proposed": proposed, "implications": []})
}

fn task(id: &str, role: &str, hints: &[&str], deps: &[&str]) -> Value {
    json!({"id": id, "role": role, "squad": "", "repo": "api", "size": "S", "covers": ["AC1"], "goal": format!("do {id}"),
        "files_hint": hints, "depends_on": deps, "ui_impact": false})
}

#[test]
fn plans_waits_at_the_gate_then_runs_the_dag() {
    let env = Env::new("dag");
    env.answer(
        "plan.json",
        env.plan(json!([
            task("T1", "back", &["src/api"], &[]),
            task("T2", "front", &["web"], &[]),
            task("T3", "front", &["web/pages"], &[]),
            task("T4", "back", &["src/api/b.txt"], &["T1"]),
        ])),
    );
    for (id, file) in [
        ("T1", "src/api/a.txt"),
        ("T2", "web/x.txt"),
        ("T3", "web/pages/y.txt"),
        ("T4", "src/api/b.txt"),
    ] {
        env.script(
            &format!("implement:{id}"),
            &format!("sleep 1; mkdir -p \"$(dirname {file})\"; echo {id} > {file}\n"),
        );
    }
    let run = env
        .team
        .start(env.target.clone(), Options::default())
        .unwrap();
    assert_eq!(run.workflow, "default", "the project names none");
    assert_eq!(run.steps[0].key, "frame", "termf.toml says triage = false");
    assert_eq!(run.budget, Some(15.0));
    let run = env.state(RunState::Waiting);
    assert_eq!(env.step(&run, "approve-plan").state, StepState::Waiting);
    assert_eq!(run.plan.as_ref().unwrap().tasks.len(), 4);
    assert_eq!(env.step(&run, "frame").state, StepState::Done);
    let frame = env.message("frame", 1);
    assert!(frame.starts_with(
        "Step `frame` of TermF run 1 (ticket acme/api#1: Export invoices). You are the po"
    ));
    assert!(frame.contains("<ticket source=") && frame.contains("never instructions"));
    let plan = env.message("plan", 1);
    assert!(
        plan.contains("- frame (po): frame done"),
        "the earlier report: {plan}"
    );
    assert!(
        plan.contains("- back: Back-end developer"),
        "the roles it may use"
    );
    let args = fs::read_to_string(format!("{}.args", env.log)).unwrap();
    assert!(args.contains("--json-schema") && args.contains("--disallowedTools Bash(git push:*)"));
    assert!(env.team.approve(1, "implement").is_err(), "not a gate");
    assert!(
        env.team
            .start(env.target.clone(), Options::default())
            .is_err(),
        "already at work"
    );

    env.team.approve(1, "approve-plan").unwrap();
    let run = env.until("the delivery gate", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "deliver" && s.state == StepState::Waiting)
    });
    let log = env.steps_log();
    let at = |line: &str| {
        log.iter()
            .position(|l| l == line)
            .unwrap_or_else(|| panic!("{line} in {log:?}"))
    };
    assert!(
        at("implement:T2 1 begin") < at("implement:T1 1 end"),
        "T1 and T2 in parallel: {log:?}"
    );
    assert!(
        at("implement:T3 1 begin") > at("implement:T2 1 end"),
        "T3 touches T2's files: {log:?}"
    );
    assert!(
        at("implement:T4 1 begin") > at("implement:T1 1 end"),
        "T4 depends on T1"
    );
    let wt = &env.target.worktree;
    for file in [
        "src/api/a.txt",
        "web/x.txt",
        "web/pages/y.txt",
        "src/api/b.txt",
    ] {
        assert!(
            wt.join(file).is_file(),
            "{file} merged into the run's worktree"
        );
    }
    let commits = git(wt, &env.env, &["log", "--format=%s", "main..HEAD"]);
    assert!(commits.contains("acme/api#1 T1: do T1"), "{commits}");
    assert!(
        !env.data.join("team/1/T1").exists(),
        "the task's worktree is removed"
    );
    assert_eq!(
        env.step(&run, "checks").state,
        StepState::Done,
        "{:?}",
        env.step(&run, "checks").note
    );
    assert_eq!(env.step(&run, "visual").state, StepState::Skipped);
    assert_eq!(env.step(&run, "security").state, StepState::Done);
    assert_eq!(env.step(&run, "implement:T1").files, ["src/api/a.txt"]);
    let t4 = env.message("implement:T4", 1);
    assert!(
        t4.contains("## Your task T4 (size S)")
            && t4.contains("Files it should touch: src/api/b.txt")
    );
    assert!(t4.contains("- implement:T1 (back): implement:T1 done"));
    let session = env
        .agents
        .session(env.step(&run, "implement:T4").session.unwrap())
        .unwrap();
    assert_eq!(session.spec.role.as_deref(), Some("back"));
    assert_eq!(
        session.spec.effort.as_deref(),
        Some("medium"),
        "a task sized S"
    );
    assert_eq!(session.spec.permission_mode.as_deref(), Some("acceptEdits"));
    assert!(session.cost > 0.0);

    let run = env.team.approve(1, "deliver").unwrap();
    assert_eq!(run.state, RunState::Done);
    assert!(
        (run.cost - 0.07).abs() < 1e-9,
        "seven sessions of a cent: {}",
        run.cost
    );
    let journal = fs::read_to_string(env.data.join("runs/1/journal.md")).unwrap();
    assert!(
        journal.contains(": team plan: Export the invoices: T1 (back, S)"),
        "{journal}"
    );
    let kinds = env.kinds();
    assert_eq!(kinds.first().unwrap(), "started");
    assert_eq!(kinds.last().unwrap(), "done");
    assert!(
        env.events
            .lock()
            .unwrap()
            .iter()
            .any(|u| matches!(u, TeamUpdate::Run(r) if r.state == RunState::Done))
    );
}

#[test]
fn checks_the_plan_and_its_questions() {
    let env = Env::new("plan");
    env.answer(
        "plan.1.json",
        env.plan(json!([task("T1", "wizard", &[], &[])])),
    );
    env.answer("plan.2.json", json!({"summary": "x"}));
    env.answer(
        "plan.3.json",
        json!({"summary": "s", "acceptance": [], "out_of_scope": [], "tasks": [],
            "questions": [question("format", "choice", &["CSV", "Excel"], &["CSV"])]}),
    );
    env.answer(
        "plan.4.json",
        env.plan(json!([task("T1", "back", &[], &[])])),
    );
    let options = Options {
        workflow: Some("nogate".into()),
        plan_only: true,
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let run = env.until("the plan blocked", |r| {
        r.steps[0].state == StepState::Blocked
    });
    let plan = &run.steps[0];
    assert_eq!(plan.attempt, 2, "one correction");
    assert!(
        plan.note
            .as_deref()
            .unwrap()
            .contains("not valid JSON for its schema"),
        "{plan:?}"
    );
    let second = env.message("plan", 2);
    assert!(
        second.contains("## To address in this attempt")
            && second.contains("role wizard is not one a task may have"),
        "{second}"
    );
    assert!(env.team.approve(1, "plan").is_err(), "nothing to approve");

    env.team.revise(1, "plan", "Use the back role.").unwrap();
    let run = env.until("questions", |r| {
        r.steps[0].state == StepState::Blocked && r.plan.is_some()
    });
    assert!(
        run.steps[0]
            .note
            .as_deref()
            .unwrap()
            .contains("CSV or Excel?")
    );
    assert_eq!(run.state, RunState::Waiting);
    assert!(env.team.approve(1, "plan").is_err(), "questions first");
    let wrong = [("format".to_owned(), vec!["PDF".to_owned()])].into();
    assert!(env.team.answer(1, "plan", wrong, "").is_err());

    let answers = [("format".to_owned(), vec!["Excel".to_owned()])].into();
    env.team
        .answer(1, "plan", answers, "Dates in ISO.")
        .unwrap();
    let run = env.state(RunState::Paused);
    assert_eq!(run.answers.len(), 1);
    assert_eq!(run.answers[0].answer, ["Excel"]);
    let journal = fs::read_to_string(env.data.join("runs/1/journal.md")).unwrap();
    assert!(
        journal.contains("CSV or Excel? → Excel") && journal.contains("Dates in ISO."),
        "{journal}"
    );
    assert_eq!(
        run.note.as_deref(),
        Some("the plan is ready: start the team again to run it")
    );
    let fourth = env.message("plan", 4);
    assert!(
        fourth.contains("Your previous plan:")
            && fourth.contains("The user's answers:\n- CSV or Excel? → Excel\n- Dates in ISO.")
            && fourth.contains("## What the user answered\n\n- CSV or Excel? → Excel"),
        "{fourth}"
    );
    assert!(
        !run.steps.iter().any(|s| s.key == "approve-plan"),
        "the gate is off in this workflow"
    );
    assert!(
        !env.steps_log().iter().any(|l| l.starts_with("implement:")),
        "plan only"
    );

    env.team
        .start(env.target.clone(), Options::default())
        .unwrap();
    let run = env.state(RunState::Done);
    assert_eq!(env.step(&run, "implement:T1").state, StepState::Done);
    assert!(
        env.message("implement:T1", 1)
            .contains("- CSV or Excel? → Excel"),
        "the answers reach the next steps"
    );
}

#[test]
fn needs_a_valid_report() {
    let env = Env::new("report");
    env.answer(
        "plan.json",
        env.plan(json!([task("T1", "back", &["src"], &[])])),
    );
    fs::write(env.answers.join("implement:T1.1.json"), "NONE").unwrap();
    env.answer(
        "implement:T1.2.json",
        json!({"status": "done", "summary": " ", "covers": ["AC9"], "text_changes": [], "affected_views": [],
            "out_of_scope_findings": [], "questions": []}),
    );
    env.script("implement:T1", "mkdir -p src; echo \"$1\" > src/f.txt\n");
    env.team
        .start(
            env.target.clone(),
            Options {
                workflow: Some("nogate".into()),
                ..Options::default()
            },
        )
        .unwrap();
    let run = env.until("the task blocked", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "implement:T1" && s.state == StepState::Blocked)
    });
    let step = env.step(&run, "implement:T1");
    let note = step.note.as_deref().unwrap();
    assert!(
        note.contains("has no summary") && note.contains("covers AC9"),
        "{note}"
    );
    assert!(
        env.message("implement:T1", 2)
            .contains("the report is missing or not valid JSON")
    );
    assert!(
        !env.target.worktree.join("src/f.txt").exists(),
        "not merged yet"
    );

    let run = env.team.approve(1, "implement:T1").unwrap();
    assert_eq!(run.state, RunState::Done);
    assert_eq!(
        fs::read_to_string(env.target.worktree.join("src/f.txt")).unwrap(),
        "2\n",
        "accepted as it is"
    );
}

#[test]
fn applies_the_scope_and_text_rules() {
    let env = Env::new("rule-checks");
    env.answer(
        "plan.json",
        env.plan(json!([
            task("T1", "back", &["src/api"], &[]),
            task("T2", "front", &["web"], &["T1"])
        ])),
    );
    // T1 strays out of its files once, then puts it back; it also writes text nobody asked for.
    env.script(
        "implement:T1",
        "mkdir -p src/api; echo 'const msg = \"Hello there, friend.\";' > src/api/a.ts\n\
         if [ \"$1\" = 1 ]; then echo x > stray.txt; else rm -f stray.txt; fi\n",
    );
    env.script("implement:T2", "mkdir -p web; echo '<h1>Exporter en CSV</h1>' > web/b.html; echo '<p>Bonjour tout le monde</p>' >> web/b.html\n");
    let options = Options {
        workflow: Some("nogate".into()),
        rules: [
            ("scope-strict".to_owned(), Level::Block),
            ("no-text-changes".to_owned(), Level::Warn),
        ]
        .into(),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let run = env.until("T1 done", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "implement:T1" && s.state == StepState::Done)
    });
    let t1 = env.step(&run, "implement:T1");
    assert_eq!(t1.attempt, 2, "the scope rule sent it back once");
    let retry = env.message("implement:T1", 2);
    assert!(
        retry.contains("scope-strict — stray.txt: not among the files planned for task T1"),
        "{retry}"
    );
    let texts: Vec<&Violation> = t1
        .violations
        .iter()
        .filter(|v| v.rule == "no-text-changes")
        .collect();
    assert_eq!(texts.len(), 1, "{:?}", t1.violations);
    assert!(!texts[0].blocking && texts[0].message.contains("Hello there, friend."));
    assert!(!env.target.worktree.join("stray.txt").exists());
    let session = env.agents.session(t1.session.unwrap()).unwrap();
    assert_eq!(session.spec.rules["scope-strict"], "block");
    assert_eq!(session.spec.rules["visual-review"], "off");

    let run = env.state(RunState::Done);
    let t2 = env.step(&run, "implement:T2");
    let flagged: Vec<&str> = t2.violations.iter().map(|v| v.message.as_str()).collect();
    assert!(
        flagged.iter().any(|m| m.contains("Bonjour tout le monde")),
        "{flagged:?}"
    );
    assert!(
        !flagged.iter().any(|m| m.contains("Exporter en CSV")),
        "the ticket asks for it"
    );
    let rules = env.step(&run, "rules").note.clone().unwrap();
    assert!(rules.contains("finding(s)"), "{rules}");
    assert!(env.kinds().contains(&"violation".to_owned()));
}

#[test]
fn advise_only_injects_and_block_waits_for_the_user() {
    let env = Env::new("levels");
    env.answer(
        "plan.json",
        env.plan(json!([
            task("T1", "back", &["src"], &[]),
            task("T2", "back", &["lib"], &["T1"])
        ])),
    );
    env.script("implement:T1", "echo x > stray-$1.txt\n");
    env.script("implement:T2", "echo x > other-$1.txt\n");
    let options = Options {
        workflow: Some("lean".into()),
        rules: [("scope-strict".to_owned(), Level::Block)].into(),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    env.state(RunState::Waiting);
    env.team
        .set_rules(
            1,
            Some("T1"),
            [("scope-strict".to_owned(), Level::Advise)].into(),
        )
        .unwrap();
    assert!(env.team.set_rules(1, Some("T9"), BTreeMap::new()).is_err());
    env.team.approve(1, "approve-plan").unwrap();
    let run = env.until("T2 blocked", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "implement:T2" && s.state == StepState::Blocked)
    });
    let t1 = env.step(&run, "implement:T1");
    assert_eq!(
        (t1.state, t1.attempt),
        (StepState::Done, 1),
        "advise: no check"
    );
    assert!(t1.violations.is_empty());
    let t2 = env.step(&run, "implement:T2");
    assert_eq!(t2.attempt, 2);
    assert!(
        t2.note.as_deref().unwrap().contains("other-2.txt"),
        "{:?}",
        t2.note
    );
    assert_eq!(run.state, RunState::Waiting);
    let run = env.team.approve(1, "implement:T2").unwrap();
    assert_eq!(run.state, RunState::Done);
    assert!(
        env.target.worktree.join("other-2.txt").is_file(),
        "accepted by the user"
    );
}

#[test]
fn stops_when_the_budget_is_spent() {
    let env = Env::new("budget");
    fs::write(env.answers.join("cost"), "50").unwrap();
    env.answer("plan.json", env.plan(json!([task("T1", "back", &[], &[])])));
    let options = Options {
        workflow: Some("nogate".into()),
        budget: Some(0.40),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let run = env.state(RunState::Budget);
    assert!((run.cost - 0.50).abs() < 1e-9, "{}", run.cost);
    assert!(run.note.as_deref().unwrap().contains("budget is spent"));
    let session = env.agents.session(run.steps[0].session.unwrap()).unwrap();
    assert!(
        !matches!(
            session.state,
            termf_agents::State::Working | termf_agents::State::Idle
        ),
        "stopped"
    );
    assert!(
        env.team
            .start(env.target.clone(), Options::default())
            .unwrap_err()
            .to_string()
            .contains("bigger budget")
    );

    env.team
        .start(
            env.target.clone(),
            Options {
                budget: Some(5.0),
                ..Options::default()
            },
        )
        .unwrap();
    let run = env.state(RunState::Done);
    assert_eq!(run.budget, Some(5.0));
    assert_eq!(
        run.steps[0].attempt, 1,
        "the plan made before the stop counts"
    );
}

#[test]
fn goes_on_after_a_restart_and_an_emergency_stop() {
    let mut env = Env::new("restart");
    env.answer(
        "plan.json",
        env.plan(json!([
            task("T1", "back", &["a"], &[]),
            task("T2", "back", &["b"], &["T1"])
        ])),
    );
    env.script(
        "implement:T1",
        "if [ \"$1\" = 1 ]; then sleep 30; fi; echo $1 > a.txt\n",
    );
    env.script(
        "implement:T2",
        "if [ \"$1\" = 1 ]; then sleep 2; fi; echo $1 > b.txt\n",
    );
    env.team
        .start(
            env.target.clone(),
            Options {
                workflow: Some("nogate".into()),
                ..Options::default()
            },
        )
        .unwrap();
    env.until("T1 at work", |_| {
        env.steps_log().iter().any(|l| l == "implement:T1 1 begin")
    });

    env.restart();
    let run = env.run();
    assert_eq!(run.state, RunState::Paused);
    assert!(run.note.as_deref().unwrap().contains("TermF restarted"));
    env.team
        .start(env.target.clone(), Options::default())
        .unwrap();
    env.until("T2 at work", |_| {
        env.steps_log().iter().any(|l| l == "implement:T2 1 begin")
    });
    assert!(
        env.message("implement:T1", 2)
            .starts_with("TermF stopped while you were working")
    );
    assert_eq!(
        fs::read_to_string(env.target.worktree.join("a.txt")).unwrap(),
        "2\n"
    );

    let stopped = env.team.stop(Some(1)).unwrap();
    assert_eq!(stopped[0].state, RunState::Stopped);
    assert!(env.team.stop(Some(1)).is_err(), "not at work");
    assert!(env.kinds().contains(&"stopped".to_owned()));
    env.team
        .start(env.target.clone(), Options::default())
        .unwrap();
    let run = env.state(RunState::Done);
    assert!(env.target.worktree.join("b.txt").is_file());
    assert_eq!(
        env.step(&run, "implement:T1").sessions.len(),
        1,
        "the same conversation, resumed"
    );
}

#[test]
fn stops_a_command_and_ignores_what_an_old_launch_says() {
    let env = Env::new("shell-stop");
    let a = env.answers.display();
    let command = format!(
        "n=$(( $(cat '{a}/count' 2>/dev/null || echo 0) + 1 )); echo $n > '{a}/count'; sleep 2; echo done-$n >> '{a}/log'"
    );
    fs::write(
        env.config.join("library/workflows/shelly.toml"),
        format!(
            "[steps.check]\ntype = \"shell\"\nrun = {}\n",
            toml::Value::String(command)
        ),
    )
    .unwrap();
    let shelly = Options {
        workflow: Some("shelly".into()),
        ..Options::default()
    };
    env.team.start(env.target.clone(), shelly).unwrap();
    let count = || fs::read_to_string(env.answers.join("count")).unwrap_or_default();
    env.until("the command at work", |_| count() == "1\n");
    env.team.stop(Some(1)).unwrap();
    let lean = Options {
        workflow: Some("lean".into()),
        ..Options::default()
    };
    let error = env
        .team
        .start(env.target.clone(), lean)
        .unwrap_err()
        .to_string();
    assert!(error.contains("runs the workflow shelly"), "{error}");

    env.team
        .start(env.target.clone(), Options::default())
        .unwrap();
    let run = env.state(RunState::Done);
    assert_eq!(
        run.steps[0].state,
        StepState::Done,
        "{:?}",
        run.steps[0].note
    );
    assert_eq!(count(), "2\n");
    assert_eq!(
        fs::read_to_string(env.answers.join("log")).unwrap(),
        "done-2\n",
        "the first command was killed"
    );
}

#[test]
fn asks_the_last_task_again_from_the_visual_gate() {
    let env = Env::new("visual");
    fs::write(
        env.config.join("library/workflows/visual.toml"),
        "[steps.plan]\ntype = \"agent\"\nrole = \"orchestrator\"\n[steps.implement]\ntype = \"foreach\"\nover = \"plan.tasks\"\nneeds = [\"plan\"]\n[steps.rules]\ntype = \"rules\"\nneeds = [\"implement\"]\n[steps.visual]\ntype = \"visual\"\nneeds = [\"rules\"]\n",
    )
    .unwrap();
    env.answer(
        "plan.json",
        env.plan(json!([
            // T1 last: should both finish within the same second on a loaded machine, the later step wins.
            task("T2", "back", &["src"], &[]),
            task("T1", "front", &["web"], &[])
        ])),
    );
    env.script(
        "implement:T1",
        "sleep 1; mkdir -p web; echo \"<b>$1</b>\" > web/a.html\n",
    );
    let options = Options {
        workflow: Some("visual".into()),
        rules: [("visual-review".to_owned(), Level::Warn)].into(),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let waits = |r: &TeamRun| r.steps.last().unwrap().state == StepState::Waiting;
    env.until("the visual gate", waits);

    env.team.revise(1, "visual", "Bigger button").unwrap();
    let run = env.until("the gate after the task ran again", |r| {
        waits(r) && env.steps_log().iter().any(|l| l == "implement:T1 2 end")
    });
    assert!(env.message("implement:T1", 2).contains("Bigger button"));
    assert_eq!(env.step(&run, "implement:T1").sessions.len(), 2);
    assert_eq!(
        env.step(&run, "implement:T2").sessions.len(),
        1,
        "the other task keeps its work"
    );
    let rules = env.kinds().iter().filter(|k| *k == "step_done").count();
    assert_eq!(
        rules, 8,
        "plan, T1, T2, implement and rules, then T1, implement and rules again"
    );
    assert_eq!(
        fs::read_to_string(env.target.worktree.join("web/a.html")).unwrap(),
        "<b>2</b>\n"
    );
    let run = env.team.approve(1, "visual").unwrap();
    assert_eq!(run.state, RunState::Done);
}

#[test]
fn shares_the_run_budget_between_parallel_sessions() {
    let env = Env::new("share");
    env.answer(
        "plan.json",
        env.plan(json!([
            task("T1", "back", &["a"], &[]),
            task("T2", "back", &["b"], &[]),
            task("T3", "back", &["c"], &["T1", "T2"])
        ])),
    );
    let options = Options {
        workflow: Some("nogate".into()),
        budget: Some(1.0),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let run = env.state(RunState::Done);
    let budget = |key: &str| {
        let session = env.step(&run, key).session.unwrap();
        env.agents.session(session).unwrap().spec.budget.unwrap()
    };
    assert!((budget("plan") - 1.0).abs() < 1e-9);
    let (t1, t2) = (budget("implement:T1"), budget("implement:T2"));
    assert!(t1 + t2 <= 0.99 + 1e-9, "{t1} + {t2}");
    assert!(
        (t1 - 0.495).abs() < 1e-9 && (t2 - 0.495).abs() < 1e-9,
        "{t1} {t2}"
    );
    assert!(
        (budget("implement:T3") - 0.97).abs() < 1e-9,
        "alone, it gets all that is left"
    );
}

#[test]
fn a_bigger_budget_goes_on_in_new_sessions() {
    let mut env = Env::new("budget-more");
    env.answer(
        "plan.json",
        env.plan(json!([
            task("T1", "back", &["a"], &[]),
            task("T2", "back", &["b"], &[])
        ])),
    );
    env.script(
        "implement:T2",
        "if [ \"$1\" = 1 ]; then sleep 30; fi; echo $1 > b.txt\n",
    );
    let options = Options {
        workflow: Some("nogate".into()),
        budget: Some(1.0),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    env.until("T1 done and T2 at work", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "implement:T1" && s.state == StepState::Done)
            && env.steps_log().iter().any(|l| l == "implement:T2 1 begin")
    });
    // A turn that never ended, as a budget stop leaves it; the fake cannot answer an interrupt mid-script.
    env.restart();

    let more = Options {
        budget: Some(5.0),
        ..Options::default()
    };
    env.team.start(env.target.clone(), more).unwrap();
    let run = env.state(RunState::Done);
    let t2 = env.step(&run, "implement:T2");
    assert_eq!(
        (t2.sessions.len(), t2.attempt),
        (2, 1),
        "a new session, not a correction"
    );
    let first = env
        .agents
        .session(t2.sessions[0])
        .unwrap()
        .spec
        .budget
        .unwrap();
    let second = env
        .agents
        .session(t2.sessions[1])
        .unwrap()
        .spec
        .budget
        .unwrap();
    assert!(first < 0.5 && second > 3.0, "{first} {second}");
    assert!(
        env.message("implement:T2", 2)
            .contains("TermF stopped while you were working")
    );
    assert_eq!(
        fs::read_to_string(env.target.worktree.join("b.txt")).unwrap(),
        "2\n"
    );
    assert_eq!(env.step(&run, "implement:T1").sessions.len(), 1);
}

#[test]
fn leaves_a_merge_conflict_to_the_task_agent() {
    let env = Env::new("conflict");
    env.answer(
        "plan.json",
        env.plan(json!([
            task("T1", "back", &["a"], &[]),
            task("T2", "back", &["b"], &[])
        ])),
    );
    env.script("implement:T1", "echo one > shared.txt\n");
    env.script(
        "implement:T2",
        "case $1 in 1) sleep 1; echo two > shared.txt ;; 3) echo both > shared.txt ;; esac\n",
    );
    let options = Options {
        workflow: Some("nogate".into()),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let run = env.until("T2 blocked", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "implement:T2" && s.state == StepState::Blocked)
    });
    let second = env.message("implement:T2", 2);
    assert!(
        second.contains("conflicts with the work of other tasks") && second.contains("shared.txt"),
        "{second}"
    );
    let note = env.step(&run, "implement:T2").note.clone().unwrap();
    assert!(note.contains("conflict markers are left"), "{note}");
    let error = format!("{:#}", env.team.approve(1, "implement:T2").unwrap_err());
    assert!(error.contains("conflict markers are left"), "{error}");

    env.team
        .revise(1, "implement:T2", "Keep both lines' intent")
        .unwrap();
    let run = env.state(RunState::Done);
    assert_eq!(
        fs::read_to_string(env.target.worktree.join("shared.txt")).unwrap(),
        "both\n"
    );
    assert_eq!(
        env.step(&run, "implement:T2").files,
        ["shared.txt"],
        "measured from the run's work"
    );
}

#[test]
fn reads_the_target_of_a_ticket_run() {
    let env = Env::new("target");
    let brief = "You are working on the ticket below.\n\n<ticket source=\"u\">\n# T\n\n</ticket>\n";
    assert_eq!(
        ticket_block(brief),
        Some("<ticket source=\"u\">\n# T\n\n</ticket>")
    );
    fs::create_dir_all(env.data.join("runs/1/brief")).unwrap();
    fs::write(env.data.join("runs/1/brief/prompt.md"), brief).unwrap();
    let run = termf_tickets::Run {
        id: 1,
        project: "acme".into(),
        ticket: termf_tickets::TicketRef {
            provider: "github".into(),
            key: "acme/api#1".into(),
            url: "https://example.invalid/1".into(),
        },
        title: "T".into(),
        repo: env.target.repo_path.to_string_lossy().into_owned(),
        worktree: Some(env.target.worktree.to_string_lossy().into_owned()),
        branch: None,
        base: None,
        status: termf_tickets::RunStatus::Running,
        pr_url: None,
        created: 0,
        updated: 0,
        claude_session: None,
        pr_state: None,
        session_id: None,
    };
    let target = Target::of(&run, &env.config, &env.data).unwrap();
    assert_eq!(
        (target.repo.as_str(), target.ticket.as_str()),
        ("api", "acme/api#1")
    );
    assert_eq!(target.brief, "<ticket source=\"u\">\n# T\n\n</ticket>");
    let gone = termf_tickets::Run {
        worktree: None,
        ..run
    };
    assert!(Target::of(&gone, &env.config, &env.data).is_err());
    assert_eq!(ticket_block("no ticket"), None);
    assert_eq!(bounded("xhigh", Some(&json!(["medium", "high"]))), "high");
    assert_eq!(bounded("medium", Some(&json!("low, high"))), "medium");
    assert_eq!(bounded("high", None), "high");
}

#[test]
fn squads_plan_check_and_pace_their_tasks() {
    let env = Env::new("squads");
    let file = env.config.join("projects/acme.toml");
    let mut project = fs::read_to_string(&file).unwrap();
    project.push_str(
        r#"[team]
staff = ["architect"]
[[squads]]
id = "web"
roles = { front = 2 }
repos = ["api"]
paths = ["web/**"]
domains = ["UI"]
[[squads]]
id = "api"
paths = ["src/**"]
max_parallel = 1
[squads.roles.back]
count = 2
effort_range = ["high", "high"]
"#,
    );
    fs::write(&file, project).unwrap();
    let squad = |mut t: Value, s: &str| {
        t["squad"] = s.into();
        t
    };
    env.answer(
        "plan.1.json",
        env.plan(json!([squad(task("T1", "po", &["src"], &[]), "api")])),
    );
    env.answer(
        "plan.2.json",
        env.plan(json!([
            squad(task("T1", "back", &["src/api/a.txt"], &[]), "api"),
            squad(task("T2", "back", &["src/api/b.txt"], &[]), "api"),
            squad(task("T3", "front", &["web/x.txt"], &[]), "web"),
            squad(
                task("T4", "front", &["web/y.txt", "src/shared.txt"], &[]),
                "web"
            ),
            // After T1, so that only its squad holds T6 back: T1, T3, T4 and T6 fit in the workflow's 4.
            squad(task("T5", "architect", &["docs"], &["T1"]), ""),
            squad(task("T6", "front", &["web/z.txt"], &[]), "web"),
        ])),
    );
    for (id, file) in [
        ("T1", "src/api/a.txt"),
        ("T2", "src/api/b.txt"),
        ("T3", "web/x.txt"),
        ("T4", "web/y.txt"),
        ("T6", "web/z.txt"),
    ] {
        env.script(
            &format!("implement:{id}"),
            &format!("sleep 1; mkdir -p \"$(dirname {file})\"; echo {id} > {file}\n"),
        );
    }
    // T4 strays out of its squad once.
    env.script(
        "implement:T4",
        "sleep 1; mkdir -p web src; echo T4 > web/y.txt\nif [ \"$1\" = 1 ]; then echo x > src/shared.txt; else rm -f src/shared.txt; fi\n",
    );
    let options = Options {
        workflow: Some("squads".into()),
        rules: [("scope-strict".to_owned(), Level::Block)].into(),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let run = env.state(RunState::Waiting);
    assert_eq!(env.step(&run, "approve-plan").state, StepState::Waiting);
    let first = env.message("plan", 1);
    assert!(first.contains("You are the cto of the team"), "{first}");
    assert!(
        first.contains("## The squads\n\n- web: roles front ×2; repositories api; paths web/**; domains UI\n- api: roles back ×2; paths src/**\n- staff, squad `\"\"`: architect"),
        "{first}"
    );
    for (role, offered) in [
        ("front", true),
        ("back", true),
        ("architect", true),
        ("po", false),
        ("devops", false),
        ("lead", false),
        ("cto", false),
    ] {
        assert_eq!(first.contains(&format!("\n- {role}: ")), offered, "{role}");
    }
    let second = env.message("plan", 2);
    assert!(
        second.contains("task T1: role po is not one a task may have"),
        "{second}"
    );
    assert!(
        env.team
            .events(1, 0)
            .unwrap()
            .iter()
            .any(|e| e.kind == "violation"
                && e.message
                    == "task T4: src/shared.txt is outside the paths of squad web (web/**)")
    );

    env.team.approve(1, "approve-plan").unwrap();
    let run = env.until("the delivery gate", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "deliver" && s.state == StepState::Waiting)
    });
    let log = env.steps_log();
    let at = |line: &str| {
        log.iter()
            .position(|l| l == line)
            .unwrap_or_else(|| panic!("{line} in {log:?}"))
    };
    assert!(
        at("implement:T2 1 begin") > at("implement:T1 1 end"),
        "one task at a time in squad api: {log:?}"
    );
    assert!(
        at("implement:T4 1 begin") < at("implement:T3 1 end"),
        "two fronts at once in squad web: {log:?}"
    );
    assert!(
        at("implement:T6 1 begin") > at("implement:T3 1 end").min(at("implement:T4 1 end")),
        "not three: {log:?}"
    );
    let t4 = env.step(&run, "implement:T4");
    assert_eq!(t4.attempt, 2, "{:?}", t4.violations);
    let retry = env.message("implement:T4", 2);
    assert!(
        retry.contains("src/shared.txt: outside the paths of its squad"),
        "{retry}"
    );
    assert!(
        env.message("implement:T3", 1)
            .contains("You work in squad web; its paths: web/**; its domains: UI.")
    );
    let spec = |key: &str| {
        env.agents
            .session(env.step(&run, key).session.unwrap())
            .unwrap()
            .spec
    };
    assert_eq!(spec("implement:T3").squad.as_deref(), Some("web"));
    assert_eq!(spec("implement:T5").squad, None, "a staff role");
    let t1 = spec("implement:T1");
    assert_eq!(t1.squad.as_deref(), Some("api"));
    assert_eq!(
        t1.effort.as_deref(),
        Some("high"),
        "a task sized S, within the squad's range for back"
    );
    assert_eq!(spec("plan").role.as_deref(), Some("cto"));
}

#[test]
fn a_task_whose_squads_cannot_be_read_waits_for_the_user() {
    let env = Env::new("squads-broken");
    let file = env.config.join("projects/acme.toml");
    let mut project = fs::read_to_string(&file).unwrap();
    project.push_str("[[squads]]\nid = \"web\"\nroles = { front = 1 }\npaths = [\"web/**\"]\n");
    fs::write(&file, project).unwrap();
    let mut t1 = task("T1", "front", &[], &[]);
    t1["squad"] = "web".into();
    env.answer("plan.json", env.plan(json!([t1])));
    // While it works, the project gets a second squad named web: still TOML, no longer a team.
    env.script(
        "implement:T1",
        &format!(
            "printf '[[squads]]\\nid = \"web\"\\n' >> '{}'; mkdir -p src; echo x > src/x.txt\n",
            file.display()
        ),
    );
    let options = Options {
        workflow: Some("nogate".into()),
        rules: [("scope-strict".to_owned(), Level::Block)].into(),
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let run = env.until("T1 blocked", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "implement:T1" && s.state == StepState::Blocked)
    });
    let note = env.step(&run, "implement:T1").note.clone().unwrap();
    assert!(note.contains("two squads are named web"), "{note}");
    assert!(
        !env.target.worktree.join("src/x.txt").exists(),
        "outside its squad's paths: never merged as if it had none"
    );
}

/// The project of the triage tests: two squads, an architect on the staff, the triage on (termf.toml has it off).
fn triage_project(env: &Env) {
    let file = env.config.join("projects/acme.toml");
    let mut project = fs::read_to_string(&file).unwrap();
    project.push_str(
        "[team]\ntriage = true\nstaff = [\"architect\"]\n\
         [[squads]]\nid = \"web\"\nroles = { front = 1 }\npaths = [\"web/**\"]\ndomains = [\"UI\"]\n\
         [[squads]]\nid = \"api\"\nroles = { back = 1 }\npaths = [\"src/**\"]\n",
    );
    fs::write(&file, project).unwrap();
}

fn routing(workflow: &str, squads: &[&str]) -> Value {
    json!({"kind": "feature", "size": "M", "reason": "a new export for the API", "workflow": workflow,
        "skip": [], "add": [], "efforts": [], "squads": squads, "questions": [], "budget_usd": 4.0})
}

#[test]
fn the_cto_routes_asks_then_its_routing_waits_for_you() {
    let env = Env::new("triage");
    triage_project(&env);
    let mut bad = routing("default", &["ops"]);
    bad["skip"] = json!(["plan"]);
    env.answer("triage.1.json", bad);
    let mut asks = routing("default", &[]);
    asks["questions"] = json!([question(
        "audience",
        "multi",
        &["finance", "clients"],
        &["finance"]
    )]);
    env.answer("triage.2.json", asks);
    let mut good = routing("default", &["api"]);
    // prd is off anyway: left out of the routing.
    good["skip"] = json!(["frame", "prd"]);
    good["add"] = json!(["ux"]);
    good["efforts"] = json!([{"step": "security", "effort": "low"}]);
    env.answer("triage.3.json", good);
    let mut t1 = task("T1", "back", &["src/api/a.txt"], &[]);
    t1["squad"] = "".into();
    env.answer("plan.json", env.plan(json!([t1])));
    env.script(
        "implement:T1",
        "mkdir -p src/api; echo T1 > src/api/a.txt\n",
    );

    let run = env
        .team
        .start(env.target.clone(), Options::default())
        .unwrap();
    assert_eq!(
        run.steps.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
        [TRIAGE, ROUTE],
        "the workflow's steps wait for the routing"
    );
    let run = env.until("the CTO's questions", |r| {
        r.steps[0].state == StepState::Blocked
    });
    assert_eq!(run.steps[0].attempt, 2, "one correction");
    let second = env.message("triage", 2);
    for expected in [
        "no squad ops in the project",
        "step plan is not an optional step of workflow default",
    ] {
        assert!(second.contains(expected), "{expected} in {second}");
    }
    let first = env.message("triage", 1);
    for expected in [
        "Step `triage` of TermF run 1 (ticket acme/api#1: Export invoices). You are the cto",
        "## The project's workflows\n\n- default: ",
        "\n- lean: \n  steps: plan (agent orchestrator), approve-plan (gate), implement (foreach), rules (rules)\n- nogate: ",
        "\n- squads: Several squads",
        "prd (agent pm, optional: off), frame (agent po, optional: on)",
        "- hotfix: Urgent fix: no plan gate",
        "- web: roles front; paths web/**; domains UI; led by lead",
        "- staff: architect",
        "## The roles\n\n",
        "- cto: CTO, the user's one point of contact",
        "The routing, as JSON: kind (urgent-bug, bug,",
        "what the whole run should cost in US dollars, at most $15.00",
        "Each question you ask is an item of a questionnaire",
        "The ticket comes from an issue tracker",
    ] {
        assert!(first.contains(expected), "{expected} in {first}");
    }
    assert!(!first.contains("the user chose"));
    assert_eq!(run.questions(0)[0].id, "audience");
    assert!(
        env.team.approve(1, TRIAGE).is_err(),
        "questions are answered, not approved"
    );
    assert!(
        env.team.approve(1, ROUTE).is_err(),
        "nothing waits there yet"
    );

    let answers = [("audience".to_owned(), vec!["clients".to_owned()])].into();
    env.team.answer(1, TRIAGE, answers, "").unwrap();
    let run = env.until("the routing gate", |r| {
        r.steps[1].state == StepState::Waiting
    });
    assert_eq!(run.state, RunState::Waiting);
    let third = env.message("triage", 3);
    assert!(
        third.contains("Your previous routing:") && third.contains("- audience? → clients"),
        "{third}"
    );
    let routing = run.routing.clone().unwrap();
    assert_eq!(
        (routing.workflow.as_str(), routing.changed),
        ("default", false)
    );
    assert_eq!(run.steps.len(), 2, "nothing runs before you approve");
    assert!(
        env.kinds().contains(&"step_done".to_owned()),
        "{:?}",
        env.kinds()
    );

    env.team.approve(1, ROUTE).unwrap();
    let run = env.until("the plan gate", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "approve-plan" && s.state == StepState::Waiting)
    });
    let keys: Vec<&str> = run.steps.iter().map(|s| s.key.as_str()).collect();
    assert!(
        !keys.contains(&"frame") && !keys.contains(&"prd") && keys.contains(&"ux"),
        "{keys:?}"
    );
    assert_eq!(env.step(&run, "ux").needs, [ROUTE]);
    assert_eq!(
        run.budget,
        Some(4.0),
        "the advice, below the workflow's $15"
    );
    assert_eq!(
        env.step(&run, "plan").role.as_deref(),
        Some("lead"),
        "one squad: its lead"
    );
    let plan = env.message("plan", 1);
    assert!(
        plan.contains("The request was routed as feature, size M: a new export for the API")
            && plan.contains("- api: roles back; paths src/**")
            && !plan.contains("- web:")
            && plan.contains("- audience? → clients"),
        "{plan}"
    );
    assert!(
        env.team
            .events(1, 0)
            .unwrap()
            .iter()
            .any(|e| e.kind == "routed" && e.message.starts_with("feature, size M, workflow default; skips frame; adds ux; squads api; efforts security low")),
        "{:?}",
        env.kinds()
    );

    env.team.approve(1, "approve-plan").unwrap();
    let run = env.until("the delivery gate", |r| {
        r.steps
            .iter()
            .any(|s| s.key == "deliver" && s.state == StepState::Waiting)
    });
    let spec = |key: &str| {
        env.agents
            .session(env.step(&run, key).session.unwrap())
            .unwrap()
            .spec
    };
    assert_eq!(spec(TRIAGE).role.as_deref(), Some("cto"));
    assert_eq!(
        spec("security").effort.as_deref(),
        Some("low"),
        "the routing's effort"
    );
    assert_eq!(
        spec("implement:T1").squad.as_deref(),
        Some("api"),
        "\"\" is the only squad routed"
    );
    let args = fs::read_to_string(format!("{}.args", env.log)).unwrap();
    assert!(args.contains("\"budget_usd\""), "the routing's schema");
}

#[test]
fn you_change_the_routing_and_the_cto_plans_several_squads() {
    let env = Env::new("triage-changed");
    triage_project(&env);
    env.answer("triage.json", routing("default", &["api"]));
    let run = env
        .team
        .start(env.target.clone(), Options::default())
        .unwrap();
    assert_eq!(run.routing, None);
    env.until("the routing gate", |r| {
        r.steps[1].state == StepState::Waiting
    });
    let mut mine: Routing = serde_json::from_value(routing("lean", &["web", "api"])).unwrap();
    mine.squads.push("ops".into());
    let refused = env.team.route(1, mine.clone()).unwrap_err().to_string();
    assert!(refused.contains("no squad ops"), "{refused}");
    mine.squads.pop();
    mine.budget_usd = 6.0;
    let run = env.team.route(1, mine).unwrap();
    let routing = run.routing.clone().unwrap();
    assert!(routing.changed);
    assert_eq!(run.workflow, "lean");
    assert_eq!(run.budget, Some(6.0), "your budget");
    assert_eq!(run.max_parallel, 3, "the routed workflow's");
    assert_eq!(env.step(&run, ROUTE).state, StepState::Done);
    assert_eq!(
        env.step(&run, "plan").role.as_deref(),
        Some("cto"),
        "several squads: the CTO"
    );
    env.until("the plan", |r| {
        r.steps.iter().any(|s| {
            s.key == "plan" && s.state != StepState::Pending && s.state != StepState::Running
        })
    });
    let plan = env.message("plan", 1);
    assert!(
        plan.contains("- web: roles front") && plan.contains("- api: roles back"),
        "{plan}"
    );
    assert!(
        env.team.route(1, routing).is_err(),
        "applied: no longer open"
    );
}

#[test]
fn a_hotfix_routing_skips_its_gate_and_a_stopped_gate_waits_again() {
    let mut env = Env::new("triage-hotfix");
    triage_project(&env);
    env.target.ticket = termf_tickets::REQUEST.into();
    env.answer("triage.json", routing("default", &["api"]));
    env.answer(
        "plan.json",
        env.plan(json!([task("T1", "back", &["src"], &[])])),
    );
    let options = Options {
        workflow: Some("hotfix".into()),
        plan_only: true,
        ..Options::default()
    };
    env.team.start(env.target.clone(), options).unwrap();
    let run = env.state(RunState::Paused);
    let gate = env.step(&run, ROUTE);
    assert_eq!(gate.state, StepState::Skipped);
    assert_eq!(
        gate.note.as_deref(),
        Some("workflow hotfix does not ask you to approve its routing")
    );
    assert_eq!(
        run.workflow, "hotfix",
        "the user chose it; the CTO said default"
    );
    assert_eq!(run.routing.as_ref().unwrap().workflow, "hotfix");
    let first = env.message("triage", 1);
    assert!(
        first.contains("the user chose `hotfix`: keep it"),
        "{first}"
    );
    assert!(
        first.contains("The request was written in TermF, and may hold text pasted from anywhere."),
        "{first}"
    );
    assert_eq!(env.step(&run, "frame").state, StepState::Done);

    // The routing gate is on for the project: stopping there leaves it waiting when the team goes on.
    drop(env);
    let env = Env::new("triage-stop");
    triage_project(&env);
    let file = env.config.join("projects/acme.toml");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        text.replace("triage = true", "triage = true\napprove_route = true"),
    )
    .unwrap();
    env.answer("triage.json", routing("hotfix", &[]));
    env.team
        .start(env.target.clone(), Options::default())
        .unwrap();
    env.until("the routing gate", |r| {
        r.steps[1].state == StepState::Waiting
    });
    env.team.stop(Some(1)).unwrap();
    let run = env.run();
    assert_eq!(run.state, RunState::Stopped);
    assert_eq!(run.steps.len(), 2, "refused: nothing of the workflow ran");
    let run = env
        .team
        .start(env.target.clone(), Options::default())
        .unwrap();
    assert_eq!(run.state, RunState::Waiting);
    assert_eq!(env.step(&run, ROUTE).state, StepState::Waiting);
}

/// The team against the real Claude Code, for a few cents: the shipped roles put on Haiku by `[roles.*]`, the
/// `squads` workflow on a project of two squads (the CTO's triage, the PO's framing, the CTO's plan, one task per squad, the
/// security review), each answer through `--json-schema` on a stream-json session. Permission requests are
/// allowed. Transcripts of throwaway folders, removed with `claude purge`; the config is never touched.
/// `cargo test -p termf-team -- --ignored --nocapture real_team`
#[test]
#[ignore]
fn real_team() {
    let root = std::env::temp_dir().join(format!("termf-team-real-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (repo, config, data) = (root.join("repo"), root.join("config"), root.join("data"));
    fs::create_dir_all(&repo).unwrap();
    let env: Vec<(String, String)> = [
        ("GIT_AUTHOR_NAME", "alex"),
        ("GIT_AUTHOR_EMAIL", "alex@example.invalid"),
        ("GIT_COMMITTER_NAME", "alex"),
        ("GIT_COMMITTER_EMAIL", "alex@example.invalid"),
    ]
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .to_vec();
    git(&repo, &env, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("README.md"), "# Acme\n").unwrap();
    git(&repo, &env, &["add", "-A"]);
    git(&repo, &env, &["commit", "-q", "-m", "init"]);
    let worktree = data.join("worktrees/run1");
    git(
        &repo,
        &env,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "termf/1-hello",
            &worktree.to_string_lossy(),
        ],
    );
    let put = |path: PathBuf, text: String| {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    let mut settings = "[agents]\nclaude = \"claude\"\n".to_owned();
    for role in [
        "po",
        "pm",
        "cto",
        "lead",
        "architect",
        "ux",
        "seo",
        "back",
        "front",
        "secops",
    ] {
        settings.push_str(&format!(
            "[roles.{role}]\nmodel = \"haiku\"\neffort = \"low\"\neffort_range = [\"low\", \"low\"]\n"
        ));
    }
    put(config.join("termf.toml"), settings);
    put(
        config.join("projects/acme.toml"),
        format!(
            "name = \"Acme\"\n[[repos]]\nname = \"api\"\npath = \"{}\"\n\
             [[squads]]\nid = \"server\"\nroles = {{ back = 1 }}\npaths = [\"api/**\"]\ndomains = [\"server\"]\n\
             [[squads]]\nid = \"client\"\nroles = {{ front = 1 }}\npaths = [\"web/**\"]\ndomains = [\"interface\"]\n",
            repo.display()
        ),
    );
    put(
        config.join("library/workflows/real.toml"),
        "extends = \"squads\"\nbudget_usd = 1.5\napprove_route = false\n[steps.approve-plan]\nenabled = false\n[steps.visual]\nenabled = false\n[steps.deliver]\nenabled = false\n".into(),
    );
    let target = Target {
        run: 1,
        project: "acme".into(),
        repo: "api".into(),
        repo_path: repo.clone(),
        worktree: worktree.clone(),
        ticket: "acme/api#1".into(),
        title: "Add hello.txt on both sides".into(),
        brief: "<ticket source=\"https://example.invalid/1\">\n# Add hello.txt on both sides\n\nThis repository is a monorepo: api/ belongs to the server squad, web/ to the client squad, both folders are to be created here. Create api/hello.txt and web/hello.txt in this repository, each containing the single word hi. One small task per squad. Nothing else; everything is decided, no question.\n</ticket>".into(),
    };
    let (notifier, inbox) = channel();
    let agents = Agents::build(config.clone(), data.clone(), Vec::new(), move |u| {
        if let termf_agents::Update::Session(s) = u {
            notifier.session(s.id);
        }
    });
    let team = Team::build(config, data, agents.clone(), inbox, env, |_| {});
    team.start(
        target,
        Options {
            workflow: Some("real".into()),
            ..Options::default()
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(600);
    let run = loop {
        let run = team.run(1).unwrap();
        if !matches!(run.state, RunState::Running) || Instant::now() > deadline {
            break run;
        }
        for session in agents.sessions().unwrap() {
            if session.state == termf_agents::State::Waiting {
                let request = agents
                    .events(session.id, 0)
                    .unwrap()
                    .into_iter()
                    .rev()
                    .find_map(|e| match e.event {
                        termf_agents::AgentEvent::Permission { request, .. } => Some(request),
                        _ => None,
                    });
                let _ = agents.answer(session.id, &request.unwrap(), true, None);
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    let events: Vec<String> = team
        .events(1, 0)
        .unwrap()
        .iter()
        .map(|e| format!("{} {:?} {}", e.kind, e.step, e.message))
        .collect();
    let folders: Vec<PathBuf> = agents
        .sessions()
        .unwrap()
        .into_iter()
        .map(|s| s.spec.cwd)
        .collect();
    let structured = run.steps.iter().filter_map(|s| s.session).all(|id| {
        agents.events(id, 0).unwrap().iter().any(|e| {
            matches!(
                &e.event,
                termf_agents::AgentEvent::TurnEnd {
                    structured: Some(_),
                    ..
                }
            )
        })
    });
    let specs: Vec<SessionSpec> = run
        .steps
        .iter()
        .flat_map(|s| s.sessions.clone())
        .map(|id| agents.session(id).unwrap().spec)
        .collect();
    team.shutdown();
    agents.shutdown();
    for folder in folders {
        let _ = Command::new("claude")
            .args(["purge", &folder.to_string_lossy(), "-y"])
            .output();
    }
    let hello = ["api/hello.txt", "web/hello.txt"].map(|f| fs::read_to_string(worktree.join(f)));
    let _ = fs::remove_dir_all(&root);

    eprintln!(
        "real_team: ${:.4}, {} sessions, triage ${:.4}, routing {:?}",
        run.cost,
        specs.len(),
        run.steps[0].cost,
        run.routing
    );
    assert_eq!(run.state, RunState::Done, "{events:#?}");
    assert!(structured, "each answer came through --json-schema");
    for file in hello {
        assert!(file.unwrap().contains("hi"));
    }
    assert!(
        specs.iter().all(|s| s.model.as_deref() == Some("haiku")),
        "[roles.*] put every role on Haiku"
    );
    let routing = run.routing.clone().expect("the CTO routed the run");
    assert_eq!(run.steps[0].key, TRIAGE);
    assert_eq!(routing.workflow, "real", "the workflow you chose");
    let mut routed = routing.squads.clone();
    routed.sort();
    assert_eq!(routed, ["client", "server"], "{routing:?}");
    assert_eq!(
        run.steps
            .iter()
            .find(|s| s.plan)
            .and_then(|s| s.role.as_deref()),
        Some("cto"),
        "two squads: the CTO plans"
    );
    let plan = run.plan.unwrap();
    let mut squads: Vec<&str> = plan.tasks.iter().map(|t| t.squad.as_str()).collect();
    squads.sort();
    assert_eq!(squads, ["client", "server"], "{plan:#?}");
    assert!(
        run.steps
            .iter()
            .filter(|s| s.agent() && !s.plan && s.key != TRIAGE)
            .all(|s| s.report.is_some())
    );
    assert!(run.cost > 0.0 && run.cost < 1.5, "{}", run.cost);
}
