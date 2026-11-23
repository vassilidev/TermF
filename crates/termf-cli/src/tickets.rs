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
        "inbox" => &["--project", "--refresh"],
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
    let refresh = take_flag(&mut args, "--refresh");
    let tickets = || -> Result<Tickets> {
        Ok(Tickets::new(
            termf_core::default_config_dir()?,
            termf_core::default_data_dir()?,
        ))
    };
    let call = |call: Call| Client::connect(None)?.call(call);
    match (command, args.as_slice()) {
        ("run", ["resume", id]) if project.is_none() && repo.is_none() => {
            let resumed = call(Call::Resume { run: id.parse()? })?;
            if let Some(warning) = resumed["warning"].as_str() {
                eprintln!("termf: warning: {warning}");
            }
            let run: Run = serde_json::from_value(resumed["run"].clone())?;
            println!(
                "run {}: resumed in {}",
                run.id,
                run.worktree.unwrap_or_default()
            );
        }
        ("run", ["resume", ..]) => bail!("usage: termf run resume <run> (no other option)"),
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
            let inbox = tickets()?.inbox(project.as_deref(), refresh)?;
            for ticket in &inbox.tickets {
                let project = ticket.project.as_deref().unwrap_or("-");
                println!(
                    "{:<28}  {project:<12}  {}",
                    ticket.reference.key, ticket.title
                );
            }
            for error in &inbox.errors {
                eprintln!("termf: {}: {}", error.source, error.message);
            }
            if !inbox.errors.is_empty() {
                bail!("some sources could not be read");
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
            let question = match &preview.local {
                Some(into) => format!(
                    "No origin: commit these changes and merge the branch into {into} here? [y/N] "
                ),
                None => "Commit these changes, push and open the pull request? [y/N] ".to_owned(),
            };
            if !yes && !confirmed(&question)? {
                bail!("not delivered");
            }
            let local = preview.local.clone();
            let delivery: Delivery = serde_json::from_value(call(Call::Deliver {
                run: id,
                confirm: preview.fingerprint,
            })?)?;
            match (delivery.run.pr_url, local) {
                (Some(url), _) => println!("{url}"),
                (None, Some(into)) => println!("merged into {into}"),
                (None, None) => {}
            }
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
/// on stderr. Anything it cannot read lets the command run. With `--worktree <dir>` (a team's step), a command
/// that only reads there, or runs one of the `--allow` commands, runs without asking (`termf_tickets::reads`).
pub fn guard(args: &[&str]) -> ExitCode {
    let (mut worktree, mut allowed) = (None, Vec::new());
    for pair in args.chunks(2) {
        match pair {
            ["--worktree", dir] => worktree = Some(std::path::Path::new(*dir)),
            ["--allow", command] => allowed.push(command.to_string()),
            _ => {}
        }
    }
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let payload: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let command = payload["tool_input"]["command"]
        .as_str()
        .unwrap_or_default();
    if let Some(reason) = termf_tickets::guard(command) {
        eprintln!("{reason}");
        return ExitCode::from(2);
    }
    // The trifecta rule (ARCHITECTURE §12.4): once a recipe read prod for this session, the user approves each
    // way out. Claude Code asks them: in the terminal of a tile, through TermF's card for a headless session.
    let tool = payload["tool_name"].as_str().unwrap_or("Bash");
    let session = payload["session_id"].as_str().unwrap_or_default();
    let data = std::env::var_os(termf_core::ipc::ENV_DATA_DIR)
        .map(std::path::PathBuf::from)
        .or_else(|| termf_core::default_data_dir().ok());
    if let Some(reason) = termf_tickets::egress(tool, command)
        && data.is_some_and(|d| termf_core::taint::session(&d, session))
    {
        let ask = serde_json::json!({ "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "ask",
            "permissionDecisionReason": reason,
        } });
        println!("{ask}");
    } else if let Some(worktree) = worktree
        && tool == "Bash"
        && let Some(cwd) = payload["cwd"].as_str()
        && termf_tickets::reads(command, worktree, std::path::Path::new(cwd), &allowed)
    {
        let allow = serde_json::json!({ "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "permissionDecisionReason": "TermF: it reads, or runs the repository's checks, in the step's worktree",
        } });
        println!("{allow}");
    }
    ExitCode::SUCCESS
}
