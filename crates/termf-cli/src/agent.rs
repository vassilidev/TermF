//! `termf agent …`: headless agent sessions. What drives a process goes through TermF, which holds it; listing
//! and reading the events read the database directly.

use std::io::Read;

use anyhow::{Context, Result, bail};
use termf_agents::{AgentSession, SessionSpec};
use termf_core::ipc::{AgentCall, Call, SessionSummary};
use termf_tickets::utc;

use crate::{Client, take_opt};

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let data_dir = termf_core::default_data_dir()?;
    let call = |call: AgentCall| Client::connect(None)?.call(Call::Agent(call));
    let id = |text: &str| -> Result<i64> {
        text.parse()
            .with_context(|| format!("{text} is not a session number"))
    };
    match args.first().copied() {
        Some("start") => {
            args.remove(0);
            let (spec, prompt) = spec(&mut args)?;
            if let Some(extra) = args.first() {
                bail!("termf agent start does not take {extra}\n{}", crate::USAGE);
            }
            let session: AgentSession = serde_json::from_value(call(AgentCall::Start {
                spec: serde_json::to_value(spec)?,
                prompt,
            })?)?;
            println!(
                "{:>4}  {}  {}",
                session.id,
                session.state.as_str(),
                session.spec.title
            );
            return Ok(());
        }
        Some("events") => {
            args.remove(0);
            let after = take_opt(&mut args, "--after")?
                .map(|a| id(&a))
                .transpose()?;
            let [session] = args[..] else {
                bail!(crate::USAGE);
            };
            for e in termf_agents::events(&data_dir, id(session)?, after.unwrap_or(0))? {
                println!("{:>6}  {}  {}", e.id, utc(e.at), e.event.describe());
            }
            return Ok(());
        }
        _ => {}
    }
    match args.as_slice() {
        ["ls"] => {
            for s in termf_agents::sessions(&data_dir)? {
                let context = s
                    .context
                    .and_then(|c| c.percent)
                    .map(|p| format!("  context {p}%"));
                println!(
                    "{:>4}  {:<11}  ${:<8.4}  {}{}",
                    s.id,
                    s.state.as_str(),
                    s.cost,
                    s.spec.title,
                    context.unwrap_or_default()
                );
            }
        }
        ["send", session, text @ ..] if !text.is_empty() => {
            call(AgentCall::Send {
                session: id(session)?,
                text: text.join(" "),
            })?;
        }
        ["interrupt", session] => {
            call(AgentCall::Interrupt {
                session: id(session)?,
            })?;
        }
        [action @ ("allow" | "deny"), session, request, message @ ..] => {
            call(AgentCall::Answer {
                session: id(session)?,
                request: request.to_string(),
                allow: *action == "allow",
                message: (!message.is_empty()).then(|| message.join(" ")),
            })?;
        }
        ["stop", session] => {
            let s: AgentSession = serde_json::from_value(call(AgentCall::Stop {
                session: id(session)?,
            })?)?;
            println!("{:>4}  {}", s.id, s.state.as_str());
        }
        ["takeover", session] => {
            let tile: SessionSummary = serde_json::from_value(call(AgentCall::Takeover {
                session: id(session)?,
            })?)?;
            println!("terminal {}  {}", tile.id, tile.title);
        }
        ["handback", session] => {
            let s: AgentSession = serde_json::from_value(call(AgentCall::Handback {
                session: id(session)?,
            })?)?;
            println!("{:>4}  {}", s.id, s.state.as_str());
        }
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

/// The session's options, and its first message (`--prompt -` reads it on stdin).
fn spec(args: &mut Vec<&str>) -> Result<(SessionSpec, Option<String>)> {
    let dir = take_opt(args, "--dir")?.unwrap_or_else(|| ".".into());
    let mut spec = SessionSpec {
        cwd: std::path::absolute(dir)?,
        ..SessionSpec::default()
    };
    if let Some(title) = take_opt(args, "--title")? {
        spec.title = title;
    }
    for (flag, value) in [
        ("--project", &mut spec.project),
        ("--agent", &mut spec.agent),
        ("--model", &mut spec.model),
        ("--effort", &mut spec.effort),
        ("--permission-mode", &mut spec.permission_mode),
    ] {
        *value = take_opt(args, flag)?;
    }
    if let Some(budget) = take_opt(args, "--budget")? {
        spec.budget = Some(
            budget
                .parse()
                .context("--budget is an amount in US dollars")?,
        );
    }
    let prompt = match take_opt(args, "--prompt")?.as_deref() {
        Some("-") => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            Some(text)
        }
        other => other.map(str::to_owned),
    };
    Ok((spec, prompt))
}
