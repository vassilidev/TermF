//! `termf team …`: the agent team of a ticket run. What starts or changes it goes through TermF, which holds
//! its agents; listing, the status and the events read the database directly.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use termf_core::ipc::{Call, TeamCall};
use termf_team::squads::Member;
use termf_team::{Level, Options, Request, StepState, TeamRun};
use termf_tickets::utc;

use crate::{Client, take_opt};

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let data_dir = termf_core::default_data_dir()?;
    let call = |call: TeamCall| Client::connect(None)?.call(Call::Team(call));
    let number = |text: &str| -> Result<i64> {
        text.parse()
            .with_context(|| format!("{text} is not a run number"))
    };
    let workflow = take_opt(&mut args, "--workflow")?;
    let budget = take_opt(&mut args, "--budget")?
        .map(|b| {
            b.parse::<f64>()
                .context("--budget is an amount in US dollars")
        })
        .transpose()?;
    let task = take_opt(&mut args, "--task")?;
    let after = take_opt(&mut args, "--after")?;
    let request = take_opt(&mut args, "--request")?;
    let project = take_opt(&mut args, "--project")?;
    let repo = take_opt(&mut args, "--repo")?;
    let note = take_opt(&mut args, "--note")?;
    let mut rules = Vec::new();
    while let Some(rule) = take_opt(&mut args, "--rule")? {
        rules.push(rule);
    }
    let levels = |pairs: &[String]| -> Result<Vec<(String, Level)>> {
        pairs
            .iter()
            .map(|pair| {
                let (name, level) = pair
                    .split_once('=')
                    .with_context(|| format!("{pair}: write <rule>=<level>"))?;
                Ok((name.trim().to_owned(), Level::parse(level)?))
            })
            .collect()
    };
    match args.as_slice() {
        ["ls"] => {
            for run in termf_team::runs(&data_dir)? {
                let waits: Vec<&str> = run
                    .steps
                    .iter()
                    .filter(|s| matches!(s.state, StepState::Waiting | StepState::Blocked))
                    .map(|s| s.key.as_str())
                    .collect();
                let waits = match waits.is_empty() {
                    true => String::new(),
                    false => format!("  waits: {}", waits.join(", ")),
                };
                println!(
                    "{:>4}  {:<8}  {:<10}  ${:<7.2}  {}{waits}",
                    run.run,
                    run.state.as_str(),
                    run.workflow,
                    run.cost,
                    run.target.ticket
                );
            }
        }
        [action @ ("plan" | "run"), id @ ..] if id.len() == usize::from(request.is_none()) => {
            let options = Options {
                workflow,
                rules: levels(&rules)?.into_iter().collect(),
                budget,
                plan_only: *action == "plan",
            };
            let request = match request {
                Some(text) => Some(serde_json::to_value(Request {
                    text,
                    project: project.map_or_else(here, Ok)?,
                    repo,
                })?),
                None => None,
            };
            let run: TeamRun = serde_json::from_value(call(TeamCall::Start {
                run: id.first().map(|i| number(i)).transpose()?,
                request,
                options: serde_json::to_value(options)?,
            })?)?;
            status(&run);
        }
        ["route", id] => route(&termf_team::run(&data_dir, number(id)?)?),
        ["answer", id, step, pairs @ ..] if !pairs.is_empty() => {
            let mut answers: HashMap<String, Vec<String>> = HashMap::new();
            for pair in pairs {
                let (question, answer) = pair
                    .split_once('=')
                    .with_context(|| format!("{pair}: write <question>=<answer>"))?;
                let given = answers.entry(question.trim().to_owned()).or_default();
                if !answer.is_empty() {
                    given.push(answer.to_owned());
                }
            }
            let run: TeamRun = serde_json::from_value(call(TeamCall::Answer {
                run: number(id)?,
                step: step.to_string(),
                answers,
                note: note.unwrap_or_default(),
            })?)?;
            status(&run);
        }
        ["status", id] => status(&termf_team::run(&data_dir, number(id)?)?),
        ["events", id] => {
            let after = after.map(|a| number(&a)).transpose()?.unwrap_or(0);
            for e in termf_team::events(&data_dir, number(id)?, after)? {
                let step = e.step.map(|s| format!("{s}: ")).unwrap_or_default();
                println!(
                    "{:>6}  {}  {:<12}  {step}{}",
                    e.id,
                    utc(e.at),
                    e.kind,
                    e.message
                );
            }
        }
        ["approve", id, step] => {
            let run: TeamRun = serde_json::from_value(call(TeamCall::Approve {
                run: number(id)?,
                step: step.to_string(),
            })?)?;
            status(&run);
        }
        ["revise", id, step, feedback @ ..] => {
            let run: TeamRun = serde_json::from_value(call(TeamCall::Revise {
                run: number(id)?,
                step: step.to_string(),
                feedback: feedback.join(" "),
            })?)?;
            status(&run);
        }
        ["rules", id, pairs @ ..] if !pairs.is_empty() => {
            let pairs: Vec<String> = pairs.iter().map(|p| p.to_string()).collect();
            let run: TeamRun = serde_json::from_value(call(TeamCall::Rules {
                run: number(id)?,
                task,
                rules: levels(&pairs)?
                    .into_iter()
                    .map(|(name, level)| (name, level.as_str().to_owned()))
                    .collect(),
            })?)?;
            status(&run);
        }
        ["stop", id @ ..] if id.len() < 2 => {
            let run = id.first().map(|i| number(i)).transpose()?;
            let stopped: Vec<TeamRun> = serde_json::from_value(call(TeamCall::Stop { run })?)?;
            if stopped.is_empty() {
                println!("no team at work");
            }
            for run in stopped {
                println!("{:>4}  {}", run.run, run.state.as_str());
            }
        }
        ["squads", project @ ..] if project.len() < 2 => squads(project.first().copied())?,
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

/// The project whose repositories hold the current folder.
fn here() -> Result<String> {
    let config = termf_core::default_config_dir()?;
    let cwd = std::env::current_dir()?;
    termf_core::load_projects(&config)?
        .into_iter()
        .find(|p| {
            p.repos
                .iter()
                .any(|r| cwd.starts_with(termf_core::expand_tilde(&r.path)))
        })
        .map(|p| p.id)
        .context("this folder is in no project: name one with --project")
}

/// The CTO's routing of a run: what it decided and why, what it asks, or that there is none.
fn route(run: &TeamRun) {
    let Some(r) = &run.routing else {
        let triage = run.steps.iter().any(|s| s.key == termf_team::TRIAGE);
        println!(
            "run {}: {}",
            run.run,
            match triage {
                true => "the CTO is routing it",
                false => "no triage: the workflow was chosen at launch",
            }
        );
        return;
    };
    let by = if r.changed { " (set by you)" } else { "" };
    println!(
        "run {} ({}): {}, size {:?}{by}",
        run.run, run.target.ticket, r.kind, r.size
    );
    println!("  why: {}", r.reason);
    println!("  workflow: {}", r.workflow);
    for (label, items) in [("skips", &r.skip), ("adds", &r.add), ("squads", &r.squads)] {
        if !items.is_empty() {
            println!("  {label}: {}", items.join(", "));
        }
    }
    for e in &r.efforts {
        println!("  effort: {} {}", e.step, e.effort);
    }
    if r.budget_usd > 0.0 {
        println!("  advised budget: ${:.2}", r.budget_usd);
    }
    questions(&r.questions);
    if let Some(gate) = run.steps.iter().find(|s| s.key == termf_team::ROUTE) {
        let state = serde_json::to_value(gate.state).unwrap_or(Value::Null);
        println!("  gate: {}", state.as_str().unwrap_or_default());
    }
}

/// A questionnaire: each question with its id, why it matters, its options and the answer proposed.
fn questions(list: &[termf_team::Question]) {
    for q in list {
        println!("  ? {} [{}] {}", q.id, q.theme, q.question);
        if !q.why.is_empty() {
            println!("      why: {}", q.why);
        }
        for (n, option) in q.options.iter().enumerate() {
            let implies = q
                .implications
                .get(n)
                .map(|i| format!(": {i}"))
                .unwrap_or_default();
            println!("      - {option}{implies}");
        }
        if !q.proposed.is_empty() {
            println!("      proposed: {}", q.proposed.join(", "));
        }
    }
}

/// The squads and staff of a project, or of every one, each role with its settings and where each comes from.
fn squads(project: Option<&str>) -> Result<()> {
    let config = termf_core::default_config_dir()?;
    let projects = match project {
        Some(id) => vec![id.to_owned()],
        None => termf_core::load_projects(&config)?
            .into_iter()
            .map(|p| p.id)
            .collect(),
    };
    for id in projects {
        let view = match termf_team::squads::view(&config, &id) {
            Ok(view) => view,
            // Every project: one that cannot be read does not hide the others.
            Err(e) if project.is_none() => {
                eprintln!("{id}: {e:#}");
                continue;
            }
            Err(e) => return Err(e),
        };
        let implicit = match view.declared {
            true => "",
            false => " (no squad declared: one squad for every role)",
        };
        println!("{id}{implicit}");
        for squad in &view.squads {
            let s = &squad.squad;
            let mut about = Vec::new();
            for (label, items) in [
                ("repos", &s.repos),
                ("paths", &s.paths),
                ("domains", &s.domains),
            ] {
                if !items.is_empty() {
                    about.push(format!("{label} {}", items.join(", ")));
                }
            }
            if let Some(n) = s.max_parallel {
                about.push(format!("at most {n} at once"));
            }
            let about = match about.is_empty() {
                true => String::new(),
                false => format!(": {}", about.join("; ")),
            };
            println!("  squad {}{about}", s.id);
            for m in &squad.members {
                member(m);
            }
        }
        if !view.staff.is_empty() {
            println!("  staff");
            for m in &view.staff {
                member(m);
            }
        }
    }
    Ok(())
}

fn member(m: &Member) {
    let mut name = m.role.clone();
    if m.lead {
        name.push_str(" (lead)");
    }
    if let Some(n) = m.count {
        name.push_str(&format!(" ×{n}"));
    }
    if let Some(e) = &m.error {
        println!("    {name}: {e}");
        return;
    }
    let settings: Vec<String> = m
        .settings
        .iter()
        .map(|s| {
            let value = match &s.value {
                Value::String(v) => v.clone(),
                Value::Array(items) => items
                    .iter()
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
                    .collect::<Vec<_>>()
                    .join(","),
                v => v.to_string(),
            };
            format!("{}={value} [{}]", s.key, s.from.join("+"))
        })
        .collect();
    println!("    {name}  {}", settings.join("  "));
}

fn status(run: &TeamRun) {
    let budget = run
        .budget
        .map(|b| format!(" / ${b:.2}"))
        .unwrap_or_default();
    println!(
        "run {} ({}): {}, workflow {}, ${:.2}{budget}",
        run.run,
        run.target.ticket,
        run.state.as_str(),
        run.workflow,
        run.cost
    );
    if let Some(note) = &run.note {
        println!("  {note}");
    }
    if let Some(plan) = &run.plan {
        println!("plan: {}", plan.summary);
        for c in &plan.acceptance {
            println!("  {}  {}", c.id, c.text);
        }
        for t in &plan.tasks {
            let deps = match t.depends_on.is_empty() {
                true => String::new(),
                false => format!(", after {}", t.depends_on.join(", ")),
            };
            println!(
                "  {:<4} {:<8} {:?}  {}  [{}]{deps}",
                t.id,
                t.role,
                t.size,
                t.goal.lines().next().unwrap_or_default(),
                t.files_hint.join(", ")
            );
        }
        questions(&plan.questions);
    }
    println!("steps:");
    for step in &run.steps {
        let state = serde_json::to_value(step.state).unwrap_or(Value::Null);
        let role = step
            .role
            .as_deref()
            .map(|r| format!(" ({r})"))
            .unwrap_or_default();
        let session = step
            .session
            .map(|s| format!("  session {s}"))
            .unwrap_or_default();
        let cost = match step.cost > 0.0 {
            true => format!("  ${:.2}", step.cost),
            false => String::new(),
        };
        println!(
            "  {:<8}  {}{role}{session}{cost}",
            state.as_str().unwrap_or_default(),
            step.key
        );
        let said = step
            .note
            .as_deref()
            .or(step.report.as_ref().map(|r| r.summary.as_str()));
        for line in said.unwrap_or_default().lines().take(8) {
            println!("            {line}");
        }
        for v in &step.violations {
            let level = if v.blocking { "block" } else { "warn" };
            let file = v
                .file
                .as_deref()
                .map(|f| format!("{f}: "))
                .unwrap_or_default();
            println!("            ! {} ({level}): {file}{}", v.rule, v.message);
        }
    }
}
