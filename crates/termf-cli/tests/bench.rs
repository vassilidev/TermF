//! The team arm of the benchmark (`bench/run.mjs`): one whole team run with the real Claude Code on a request, in
//! a repository the bench prepared, played as a hurried user would (every gate approved, every questionnaire
//! answered with the CTO's proposal, every permission granted), stopped before the pull request. Throwaway
//! config and data; your Claude Code account. Not run by `cargo test`: `bench/run.mjs` runs it with
//! `TERMF_BENCH_REPO` (a git repository whose `origin` is a local bare copy), `TERMF_BENCH_TICKET` (the request,
//! a file), `TERMF_BENCH_ROOT` (an empty folder for config and data), `TERMF_BENCH_OUT` (the JSON written at the
//! end), `TERMF_BENCH_MODEL` (every role's model, `sonnet` by default), `TERMF_BENCH_BUDGET` (US dollars, 3 by
//! default) and `TERMF_BENCH_MINUTES` (40 by default).
#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::json;
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

/// Claude Code without the user's own settings, CLAUDE.md, skills, plugins and MCP servers: the bench compares
/// TermF's library with nothing, not with the tester's setup. `--settings` and `--mcp-config` from TermF still
/// apply.
const CLAUDE: &str = "claude --setting-sources project,local --strict-mcp-config";

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// A hurried user: the CTO's proposal, else the first option, else "yes", else a short text.
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
        _ => vec!["Keep it as small as the ticket allows; your call.".into()],
    }
}

#[test]
#[ignore]
fn team_arm() {
    let (Some(repo), Some(ticket), Some(root), Some(out)) = (
        var("TERMF_BENCH_REPO"),
        var("TERMF_BENCH_TICKET"),
        var("TERMF_BENCH_ROOT"),
        var("TERMF_BENCH_OUT"),
    ) else {
        eprintln!("run by bench/run.mjs: TERMF_BENCH_REPO, _TICKET, _ROOT and _OUT are not set");
        return;
    };
    let model = var("TERMF_BENCH_MODEL").unwrap_or_else(|| "sonnet".into());
    let budget: f64 = var("TERMF_BENCH_BUDGET").map_or(3.0, |b| b.parse().unwrap());
    let minutes: u64 = var("TERMF_BENCH_MINUTES").map_or(40, |m| m.parse().unwrap());
    let root = PathBuf::from(root);
    let (config, data) = (root.join("config"), root.join("data"));
    std::fs::create_dir_all(config.join("projects")).unwrap();
    std::fs::create_dir_all(root.join("home")).unwrap();
    let gitconfig = root.join("gitconfig");
    std::fs::write(
        &gitconfig,
        "[user]\n\tname = TermF bench\n\temail = bench@termf.invalid\n",
    )
    .unwrap();
    let env: Vec<(String, String)> = vec![
        (
            "GIT_CONFIG_GLOBAL".into(),
            gitconfig.to_string_lossy().into(),
        ),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
    ];
    let mut termf = format!("[agents]\nclaude = {CLAUDE:?}\n");
    for role in ROLES {
        termf.push_str(&format!("[roles.{role}]\nmodel = {model:?}\n"));
    }
    std::fs::write(config.join("termf.toml"), termf).unwrap();
    std::fs::write(
        config.join("projects/bench.toml"),
        format!(
            "name = \"Bench\"\n[[repos]]\nname = \"app\"\npath = {repo:?}\nchecks = \"npm test\"\n"
        ),
    )
    .unwrap();

    let started = Instant::now();
    let mut tickets = Tickets::new(config.clone(), data.clone());
    tickets.env = env.clone();
    tickets.home = root.join("home");
    let request = std::fs::read_to_string(&ticket).unwrap();
    let run = tickets.start_request(&request, "bench", None).unwrap();
    let target = || termf_team::Target::of(&run, &config, &data).unwrap();
    let (notifier, inbox) = termf_team::channel();
    let agents = Agents::build(config.clone(), data.clone(), env.clone(), move |u| {
        if let termf_agents::Update::Session(s) = u {
            notifier.session(s.id);
        }
    });
    agents.serve_guard(env!("CARGO_BIN_EXE_termf").into());
    let team = Team::build(
        config.clone(),
        data.clone(),
        agents.clone(),
        inbox,
        env,
        |_| {},
    );
    let options = || Options {
        budget: Some(budget),
        ..Options::default()
    };
    team.start(target(), options()).unwrap();

    let deadline = started + Duration::from_secs(minutes * 60);
    let (mut seen, mut approved, mut answered, mut permissions) = (0, vec![], vec![], vec![]);
    let (mut timed_out, mut gave_up, mut revised) = (false, false, BTreeMap::new());
    let mut raised = Vec::new();
    let done = loop {
        for e in team.events(run.id, seen).unwrap() {
            seen = e.id;
            let message: String = e.message.chars().take(160).collect();
            let step = e.step.unwrap_or_default();
            println!("  [{}] {step} {}", e.kind, message.replace('\n', " / "));
        }
        let now = team.run(run.id).unwrap();
        // The CTO's advised budget is spent: the user lets it go on, up to the bench's cap.
        if now.state == RunState::Budget && now.cost < budget {
            raised.push(now.budget.unwrap_or_default());
            team.start(target(), options()).unwrap();
            continue;
        }
        if !matches!(now.state, RunState::Running | RunState::Waiting) {
            break now;
        }
        if Instant::now() > deadline {
            timed_out = true;
            break now;
        }
        if !drive(&team, &now, &mut approved, &mut answered, &mut revised) {
            gave_up = true;
            break now;
        }
        for session in agents.sessions().unwrap() {
            if session.state != termf_agents::State::Waiting {
                continue;
            }
            let asked = agents
                .events(session.id, 0)
                .unwrap()
                .into_iter()
                .rev()
                .find_map(|e| match e.event {
                    AgentEvent::Permission { request, tool, .. } => Some((request, tool)),
                    _ => None,
                });
            if let Some((request, tool)) = asked {
                permissions.push(tool);
                let _ = agents.answer(session.id, &request, true, None);
            }
        }
        std::thread::sleep(Duration::from_millis(700));
    };
    let wall = started.elapsed().as_secs_f64();
    if timed_out || gave_up {
        let _ = team.stop(Some(run.id));
    }

    let (mut turns, mut tokens) = (0, BTreeMap::<&str, u64>::new());
    let ids: Vec<i64> = done.steps.iter().flat_map(|s| s.sessions.clone()).collect();
    for id in &ids {
        for e in agents.events(*id, 0).unwrap_or_default() {
            if let AgentEvent::TurnEnd { usage, .. } = e.event {
                turns += 1;
                *tokens.entry("input").or_default() += usage.input;
                *tokens.entry("output").or_default() += usage.output;
                *tokens.entry("cache_read").or_default() += usage.cache_read;
                *tokens.entry("cache_creation").or_default() += usage.cache_creation;
            }
        }
    }
    team.shutdown();
    agents.shutdown();
    let result = json!({
        "state": done.state.as_str(),
        "timed_out": timed_out,
        "gave_up": gave_up,
        "budget_raised_from": raised,
        "wall_s": wall,
        "cost_usd": done.cost,
        "sessions": ids.len(),
        "turns": turns,
        "tokens": tokens,
        "worktree": run.worktree,
        "base": run.base,
        "approved": approved,
        "answered": answered,
        "permissions": permissions,
        "run": done,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&result).unwrap()).unwrap();
    println!(
        "team arm: {} in {wall:.0} s, ${:.3}, {} sessions",
        done.state.as_str(),
        done.cost,
        ids.len()
    );
}

/// Acts on the first step that waits for the user, as the QA harness does; `false` once a step was sent back
/// twice and is blocked again, where a user would give up.
fn drive(
    team: &Team,
    run: &TeamRun,
    approved: &mut Vec<String>,
    answered: &mut Vec<String>,
    revised: &mut BTreeMap<String, u32>,
) -> bool {
    let id = run.run;
    for (i, step) in run.steps.iter().enumerate() {
        let key = step.key.clone();
        match step.state {
            StepState::Waiting => {
                let passed = match team.approve(id, &key) {
                    Err(e) if step.kind == StepKind::Connector => {
                        println!("  (delivery gate refused: {e:#})");
                        team.approve_anyway(id, &key, "Bench: delivered as it is")
                    }
                    other => other,
                };
                if let Err(e) = passed {
                    println!("  (approving {key} failed: {e:#})");
                }
                approved.push(key);
            }
            StepState::Blocked if !run.questions(i).is_empty() => {
                let answers: BTreeMap<String, Vec<String>> = run
                    .questions(i)
                    .iter()
                    .map(|q| (q.id.clone(), answer(q)))
                    .collect();
                println!("  (answering {key}: {answers:?})");
                if let Err(e) = team.answer(id, &key, answers, "") {
                    println!("  (answering {key} failed: {e:#})");
                }
                answered.push(key);
            }
            StepState::Blocked => {
                println!("  (blocked step {key}: {:?})", step.note);
                if team.approve(id, &key).is_err() {
                    let count = revised.entry(key.clone()).or_insert(0);
                    *count += 1;
                    if *count > 2 {
                        return false;
                    }
                    let again = "Answer again, fixing what TermF reported.";
                    if let Err(e) = team.revise(id, &key, again) {
                        println!("  (revising {key} failed: {e:#})");
                    }
                    answered.push(format!("{key} (revised)"));
                } else {
                    approved.push(key);
                }
            }
            _ => continue,
        }
        return true;
    }
    true
}
