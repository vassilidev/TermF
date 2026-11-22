//! `termf memory …` and `termf ask`: the project memory and its expert. A search reads the index directly;
//! indexing and asking go through TermF, which masks with the vault's values and runs the expert.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use termf_core::ipc::{Call, MemoryCall};
use termf_memory::{Hit, Memory, Source};
use termf_tickets::utc;

use crate::{Client, take_opt};

/// `project` (from `--project`), else the project that lists the current folder.
pub(crate) fn or_here(project: Option<String>) -> Result<String> {
    match project {
        Some(id) => Ok(id),
        None => termf_memory::project_of(
            &termf_core::default_config_dir()?,
            &std::env::current_dir()?,
        )?
        .context("no project lists this folder: add --project <id>"),
    }
}

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let project = take_opt(&mut args, "--project")?;
    match args.first().copied() {
        Some("search") => {
            args.remove(0);
            let mut sources = Vec::new();
            while let Some(source) = take_opt(&mut args, "--source")? {
                sources.push(Source::parse(&source)?);
            }
            let limit = take_opt(&mut args, "--limit")?
                .map(|l| l.parse::<usize>().context("--limit takes a number"))
                .transpose()?
                .unwrap_or(10);
            if args.is_empty() {
                bail!(crate::USAGE);
            }
            let settings = termf_core::load_settings(&termf_core::default_config_dir()?)?.memory;
            let memory = Memory::open(&termf_core::default_data_dir()?)?;
            let hits = memory.search(
                &settings,
                &or_here(project)?,
                &args.join(" "),
                &sources,
                limit,
            )?;
            if hits.is_empty() {
                println!("nothing found");
            }
            for Hit {
                id,
                source,
                title,
                locator,
                at,
                snippet,
            } in hits
            {
                println!("{id:>6}  {:<9}  {}  {title}", source.id(), utc(at));
                println!("        {locator}");
                println!("        {}", snippet.replace('\n', " "));
            }
        }
        // Every project unless one is named.
        Some("reindex") if args.len() == 1 => {
            let statuses =
                Client::connect(None)?.call(Call::Memory(MemoryCall::Reindex { project }))?;
            for status in statuses.as_array().into_iter().flatten() {
                let docs: u64 = status["sources"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s["docs"].as_u64())
                    .sum();
                let state = if status["enabled"] == true {
                    "on"
                } else {
                    "off"
                };
                println!(
                    "{:<20}  {state:<3}  {docs} documents",
                    status["project"].as_str().unwrap_or_default()
                );
                for source in status["sources"].as_array().into_iter().flatten() {
                    if let Some(error) = source["error"].as_str() {
                        println!(
                            "    {}: {error}",
                            source["source"].as_str().unwrap_or_default()
                        );
                    }
                }
            }
        }
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

pub fn ask(mut args: Vec<&str>) -> Result<()> {
    let project = or_here(take_opt(&mut args, "--project")?)?;
    let budget = take_opt(&mut args, "--budget")?
        .map(|b| {
            b.parse::<f64>()
                .context("--budget takes an amount in US dollars")
        })
        .transpose()?;
    if args.is_empty() {
        bail!(crate::USAGE);
    }
    let question = args.join(" ");
    let answer: Value = Client::connect(None)?.call(Call::Memory(MemoryCall::Ask {
        project,
        question,
        budget,
    }))?;
    println!("{}", answer["text"].as_str().unwrap_or_default().trim_end());
    println!(
        "\n(expert conversation {}, ${:.2})",
        answer["session"],
        answer["cost"].as_f64().unwrap_or_default()
    );
    Ok(())
}
