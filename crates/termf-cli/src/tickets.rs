//! `termf run | runs | inbox | deliver | close`. Starting, delivering and closing a run go through TermF, which
//! holds its session; listing reads the run database and GitHub directly.

use anyhow::{Result, bail};
use termf_core::ipc::Call;
use termf_tickets::{Run, Tickets, utc};

use crate::{Client, take_flag, take_opt};

pub fn run(command: &str, mut args: Vec<&str>) -> Result<()> {
    let project = take_opt(&mut args, "--project")?;
    let repo = take_opt(&mut args, "--repo")?;
    let force = take_flag(&mut args, "--force");
    let tickets = || -> Result<Tickets> {
        Ok(Tickets::new(
            termf_core::default_config_dir()?,
            termf_core::default_data_dir()?,
        ))
    };
    let call = |call: Call| -> Result<Run> {
        Ok(serde_json::from_value(Client::connect(None)?.call(call)?)?)
    };
    match (command, args.as_slice()) {
        ("run", [ticket]) => {
            let run = call(Call::Run {
                ticket: ticket.to_string(),
                project,
                repo,
            })?;
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
            let run = call(Call::Deliver { run: id.parse()? })?;
            println!("{}", run.pr_url.unwrap_or_default());
        }
        ("close", [id]) => {
            let run = call(Call::Close {
                run: id.parse()?,
                force,
            })?;
            println!("run {} closed", run.id);
        }
        _ => bail!(crate::USAGE),
    }
    Ok(())
}
