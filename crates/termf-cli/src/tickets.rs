//! `termf run | runs | inbox | deliver | close`, and `termf guard`, the hook of a run's session. Starting,
//! delivering and closing a run go through TermF, which holds its session; listing and the delivery preview
//! read the run database, git and GitHub directly.

use std::io::{BufRead, Read, Write};
use std::process::ExitCode;

use anyhow::{Result, bail};
use serde_json::Value;
use termf_core::ipc::Call;
use termf_tickets::{Change, Delivery, Run, Tickets, utc};

use crate::{Client, take_flag, take_opt};

pub fn run(command: &str, mut args: Vec<&str>) -> Result<()> {
    let allowed: &[&str] = match command {
        "run" => &["--project", "--repo"],
        "inbox" => &["--project"],
        "deliver" => &["--yes"],
        "close" => &["--force"],
        _ => &[],
    };
    if let Some(flag) = args
        .iter()
        .find(|a| a.starts_with('-') && !allowed.contains(a))
    {
        bail!("termf {command} does not take {flag}\n{}", crate::USAGE);
    }
    let project = take_opt(&mut args, "--project")?;
    let repo = take_opt(&mut args, "--repo")?;
    let force = take_flag(&mut args, "--force");
    let yes = take_flag(&mut args, "--yes");
    let tickets = || -> Result<Tickets> {
        Ok(Tickets::new(
            termf_core::default_config_dir()?,
            termf_core::default_data_dir()?,
        ))
    };
    let call = |call: Call| Client::connect(None)?.call(call);
    match (command, args.as_slice()) {
        ("run", [ticket]) => {
            let run: Run = serde_json::from_value(call(Call::Run {
                ticket: ticket.to_string(),
                project,
                repo,
            })?)?;
            let (worktree, branch) = (
                run.worktree.unwrap_or_default(),
                run.branch.unwrap_or_default(),
            );
            println!("run {}: {worktree} on {branch}", run.id);
        }
        ("runs", []) => {
            for run in tickets()?.runs()? {
                let pr = run.pr_url.as_deref().unwrap_or_default();
                let (id, status, key) = (run.id, run.status.as_str(), &run.ticket.key);
                println!("{id:>4}  {status:<9}  {key:<24}  {}  {pr}", run.title);
            }
        }
        ("runs", [id]) => {
            let tickets = tickets()?;
            let id = id.parse()?;
            let run = tickets.run(id)?;
            println!(
                "{} {}: {} ({})",
                run.ticket.key,
                run.title,
                run.status.as_str(),
                run.ticket.url
            );
            for line in [&run.worktree, &run.branch, &run.pr_url]
                .into_iter()
                .flatten()
            {
                println!("  {line}");
            }
            for event in tickets.events(id)? {
                println!("{}  {}", utc(event.at), event.event.describe());
            }
        }
        ("inbox", []) => {
            for ticket in tickets()?.inbox(project.as_deref())? {
                let project = ticket.project.as_deref().unwrap_or("-");
                println!(
                    "{:<28}  {project:<12}  {}",
                    ticket.reference.key, ticket.title
                );
            }
        }
        ("deliver", [id]) => {
            let id = id.parse()?;
            let preview = tickets()?.deliver_preview(id)?;
            for file in &preview.files {
                let change = match file.change {
                    Change::Added => "added",
                    Change::Modified => "modified",
                    Change::Deleted => "deleted",
                    Change::Renamed => "renamed",
                };
                let from = file.from.as_deref().map(|f| format!(" (from {f})"));
                println!("{change:<9}  {}{}", file.path, from.unwrap_or_default());
            }
            for commit in &preview.commits {
                println!("commit     {} {}", &commit.commit[..7], commit.subject);
            }
            if !yes && !confirmed("Commit these changes, push and open the pull request? [y/N] ")? {
                bail!("not delivered");
            }
            let delivery: Delivery = serde_json::from_value(call(Call::Deliver {
                run: id,
                confirm: preview.fingerprint,
            })?)?;
            println!("{}", delivery.run.pr_url.unwrap_or_default());
            if let Some(warning) = delivery.warning {
                eprintln!("termf: warning: {warning}");
            }
        }
        ("close", [id]) => {
            let run: Run = serde_json::from_value(call(Call::Close {
                run: id.parse()?,
                force,
            })?)?;
            println!("run {} closed", run.id);
        }
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

fn confirmed(question: &str) -> Result<bool> {
    print!("{question}");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// A Claude Code `PreToolUse` hook (JSON on stdin): exit code 2 refuses the Bash command, and Claude reads why
/// on stderr. Anything it cannot read lets the command run.
pub fn guard() -> ExitCode {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let payload: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let command = payload["tool_input"]["command"]
        .as_str()
        .unwrap_or_default();
    match termf_tickets::guard(command) {
        Some(reason) => {
            eprintln!("{reason}");
            ExitCode::from(2)
        }
        None => ExitCode::SUCCESS,
    }
}
