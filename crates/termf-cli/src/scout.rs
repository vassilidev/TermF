//! `termf library scout`: the library scout from a terminal (§11.2). Its results are reviewed in TermF.

use std::path::Path;

use anyhow::{Context, Result, bail};
use termf_library::Kind;
use termf_library::scout::{self, ItemRef, Options, Progress};

pub fn run(config_dir: &Path, project: Option<&str>, mut args: Vec<&str>) -> Result<()> {
    let mut items = Vec::new();
    while let Some(item) = crate::take_opt(&mut args, "--item")? {
        let (kind, name) = item
            .split_once('/')
            .with_context(|| format!("{item}: expected <kind>/<name>"))?;
        let kind = Kind::parse(kind).with_context(|| format!("unknown kind {kind}"))?;
        items.push(ItemRef {
            kind,
            name: name.into(),
        });
    }
    let budget = crate::take_opt(&mut args, "--budget")?;
    let options = Options {
        model: crate::take_opt(&mut args, "--model")?,
        budget: budget
            .map(|b| b.parse().with_context(|| format!("invalid budget {b}")))
            .transpose()?,
        cancel: None,
    };
    if !args.is_empty() {
        bail!(crate::USAGE);
    }
    let claude = vec![termf_core::load_settings(config_dir)?.claude_program()];
    let (model, budget) = scout::settings(config_dir);
    eprintln!(
        "Claude Code ({}, at most ${:.2}) searches the web and reviews the library…",
        options.model.as_deref().unwrap_or(&model),
        options.budget.unwrap_or(budget)
    );
    let draft = scout::run(
        config_dir,
        project,
        &items,
        &options,
        &claude,
        &mut |p| match p {
            Progress::Search(query) => eprintln!("  search  {query}"),
            Progress::Fetch(url) => eprintln!("  read    {url}"),
        },
    )?;
    if draft.entries.is_empty() {
        println!("Nothing to improve.");
    }
    for e in &draft.entries {
        let what = match (e.layer, e.content.is_empty() && e.script.is_empty()) {
            (None, _) => "new item proposed",
            (_, false) => "new version proposed",
            (_, true) => "critique",
        };
        let runs = if e.executable {
            " (runs a script or a hook)"
        } else {
            ""
        };
        println!("{} {}: {what}{runs}", e.kind, e.name);
        for f in &e.critique {
            let flaw = format!("{:?}", f.flaw).to_lowercase();
            println!(
                "  - {flaw}: {} Better: {} ({})",
                f.text, f.practice, f.source
            );
        }
        if !e.summary.is_empty() {
            println!("  + {} ({})", e.summary, e.sources.join(", "));
        }
    }
    for why in &draft.dropped {
        println!("left out: {why}");
    }
    if let Some(cost) = draft.cost {
        println!("cost: ${cost:.2}");
    }
    println!("Review them in TermF: Library, Scout.");
    Ok(())
}
