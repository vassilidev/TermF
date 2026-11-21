//! `termf ci runs | view | logs | rerun | cancel | dispatch`: GitHub Actions of a repository (default: here),
//! read and driven through gh directly; TermF does not need to run.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use termf_ci::{Ci, Filter, WorkflowRun};
use termf_tickets::Tickets;

use crate::{take_flag, take_opt};

fn state(run: &WorkflowRun) -> &str {
    run.conclusion.as_deref().unwrap_or(&run.status)
}

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let allowed: &[&str] = match args.first() {
        Some(&"runs") => &["--repo", "--branch", "--status", "--pr", "--page"],
        Some(&"logs") => &["--repo", "--errors"],
        Some(&"rerun") => &["--repo", "--failed"],
        Some(&"dispatch") => &["--repo", "--ref", "--input"],
        _ => &["--repo"],
    };
    if let Some(flag) = args
        .iter()
        .find(|a| a.starts_with('-') && !allowed.contains(a))
    {
        bail!("termf ci does not take {flag} there\n{}", crate::USAGE);
    }
    let repo = match take_opt(&mut args, "--repo")? {
        Some(repo) => std::path::absolute(repo)?,
        None => std::env::current_dir()?,
    };
    let repo = repo.to_string_lossy().into_owned();
    let number = |text: &str| -> Result<u64> {
        text.parse()
            .with_context(|| format!("{text} is not a number"))
    };
    let filter = Filter {
        branch: take_opt(&mut args, "--branch")?,
        status: take_opt(&mut args, "--status")?,
        pr: take_opt(&mut args, "--pr")?
            .map(|n| number(&n))
            .transpose()?,
        page: take_opt(&mut args, "--page")?.map_or(Ok(1), |n| number(&n).map(|n| n as u32))?,
    };
    let errors_only = take_flag(&mut args, "--errors");
    let failed_only = take_flag(&mut args, "--failed");
    let git_ref = take_opt(&mut args, "--ref")?;
    let mut inputs = BTreeMap::new();
    while let Some(input) = take_opt(&mut args, "--input")? {
        let (key, value) = input
            .split_once('=')
            .with_context(|| format!("--input {input}: write it key=value"))?;
        inputs.insert(key.to_owned(), value.to_owned());
    }
    let ci = Ci::new(Tickets::new(
        termf_core::default_config_dir()?,
        termf_core::default_data_dir()?,
    ));
    match args.as_slice() {
        ["runs"] => {
            let page = ci.runs(&repo, &filter)?;
            for run in &page.runs {
                let branch = run.branch.as_deref().unwrap_or("-");
                println!(
                    "{:>11}  {:<11}  {:<20}  {branch:<24}  {:<17}  {}",
                    run.id,
                    state(run),
                    run.workflow,
                    run.event,
                    run.title
                );
            }
            let at = filter.page.max(1);
            println!("page {at}: {} runs of {} in all", page.total, page.repo);
        }
        ["view", id] => {
            let detail = ci.run(&repo, number(id)?)?;
            let run = &detail.run;
            println!(
                "{} #{} (attempt {}): {} — {}",
                run.workflow,
                run.number,
                run.attempt,
                run.title,
                state(run)
            );
            let by = run.triggered_by.as_deref().or(run.actor.as_deref());
            println!(
                "  {} on {} at {}, by {}, {}",
                run.event,
                run.branch.as_deref().unwrap_or("-"),
                &run.sha[..run.sha.len().min(7)],
                by.unwrap_or("-"),
                run.created
            );
            println!("  {}", run.url);
            for job in &detail.jobs {
                let job_state = job.conclusion.as_deref().unwrap_or(&job.status);
                println!("job {:>11}  {job_state:<11}  {}", job.id, job.name);
                for step in &job.steps {
                    let step_state = step.conclusion.as_deref().unwrap_or(&step.status);
                    println!("    {:>3}  {step_state:<11}  {}", step.number, step.name);
                }
            }
        }
        ["logs", job] => {
            let log = ci.log(&repo, number(job)?)?;
            if log.skipped > 0 {
                eprintln!("termf: the first {} lines are left out", log.skipped);
            }
            if errors_only {
                for &i in &log.errors {
                    println!("{:>7}  {}", log.skipped + i + 1, log.lines[i]);
                }
            } else {
                for line in &log.lines {
                    println!("{line}");
                }
            }
        }
        ["rerun", id] => {
            ci.rerun(&repo, number(id)?, failed_only)?;
            println!("run {id} started again");
        }
        ["cancel", id] => {
            ci.cancel(&repo, number(id)?)?;
            println!("run {id} cancelled");
        }
        ["dispatch", workflow] => {
            ci.dispatch(&repo, workflow, git_ref.as_deref(), &inputs)?;
            println!("{workflow} started: `termf ci runs` lists it in a few seconds");
        }
        _ => bail!(crate::USAGE),
    }
    Ok(())
}
