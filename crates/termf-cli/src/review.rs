//! `termf review …`: visual reviews before / after. Starting one, deciding and posting on the pull request go
//! through TermF, which holds the team's gate; showing a review reads the database directly.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use termf_core::ipc::{Call, ReviewCall};
use termf_visual::{Decision, Pin, PrPreview, Request, Review, VersionState};

use crate::{Client, take_flag, take_opt};

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let data_dir = termf_core::default_data_dir()?;
    let call = |call: ReviewCall| Client::connect(None)?.call(Call::Review(call));
    let number = |text: &str| -> Result<i64> {
        text.parse()
            .with_context(|| format!("{text} is not a number"))
    };
    let run = take_opt(&mut args, "--run")?
        .map(|r| number(&r))
        .transpose()?;
    let repo = take_opt(&mut args, "--repo")?
        .map(std::path::absolute)
        .transpose()?;
    let base = take_opt(&mut args, "--base")?;
    let head = take_opt(&mut args, "--head")?;
    let comment = take_opt(&mut args, "--comment")?;
    let yes = take_flag(&mut args, "--yes");
    let mut routes = Vec::new();
    while let Some(route) = take_opt(&mut args, "--route")? {
        routes.push(route);
    }
    let mut pins = Vec::new();
    while let Some(pin) = take_opt(&mut args, "--pin")? {
        pins.push(parse_pin(&pin)?);
    }
    match args.as_slice() {
        ["start"] => {
            let request = Request {
                run,
                repo,
                base,
                head,
                routes,
            };
            let review: Review = serde_json::from_value(call(ReviewCall::Start {
                request: serde_json::to_value(request)?,
            })?)?;
            println!("review {} v{}: capturing", review.id, last(&review)?.n);
            follow(&data_dir, review.id)?;
        }
        ["show"] => {
            let run = run.context("termf review show <id> | --run <run>")?;
            let review = termf_visual::reviews(&data_dir)?
                .into_iter()
                .find(|r| r.run == Some(run))
                .with_context(|| format!("run {run} has no visual review"))?;
            show(&data_dir, &review);
        }
        ["show", id] => show(&data_dir, &termf_visual::review(&data_dir, number(id)?)?),
        ["decide", id, screen, decision] => {
            let decision = Decision::parse(decision)?;
            let review: Review = serde_json::from_value(call(ReviewCall::Decide {
                review: number(id)?,
                screen: screen.to_string(),
                decision: serde_json::to_value(decision)?
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                comment,
                pins: (!pins.is_empty()).then(|| json!(pins)),
            })?)?;
            show(&data_dir, &review);
        }
        ["pr", id] => {
            let id = number(id)?;
            let preview: PrPreview = serde_json::from_value(call(ReviewCall::PrComment {
                review: id,
                confirm: None,
            })?)?;
            println!(
                "pull request: {}\npushes {} image(s) to the branch {}:",
                preview.pr,
                preview.files.len(),
                preview.branch
            );
            for file in &preview.files {
                println!("  {file}");
            }
            println!("comment:\n{}", preview.body);
            if !yes {
                println!("nothing posted: run again with --yes to post it");
                return Ok(());
            }
            let _: Value = call(ReviewCall::PrComment {
                review: id,
                confirm: Some(preview.fingerprint),
            })?;
            println!("posted on {}", preview.pr);
        }
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

/// `x,y,comment`, in pixels of the after image.
fn parse_pin(text: &str) -> Result<Pin> {
    let mut parts = text.splitn(3, ',');
    let mut coord = |name: &str| -> Result<u32> {
        parts
            .next()
            .and_then(|p| p.trim().parse().ok())
            .with_context(|| format!("--pin {text}: {name} is not a number (write x,y,comment)"))
    };
    let (x, y) = (coord("x")?, coord("y")?);
    let comment = parts.next().unwrap_or_default().trim().to_owned();
    anyhow::ensure!(
        !comment.is_empty(),
        "--pin {text}: say what to change after x,y,"
    );
    Ok(Pin { x, y, comment })
}

fn last(review: &Review) -> Result<&termf_visual::Version> {
    review.versions.last().context("nothing captured yet")
}

/// Prints the capture's progress until it ends, then the review. The capture goes on in TermF if this stops.
fn follow(data_dir: &std::path::Path, id: i64) -> Result<()> {
    let mut said = String::new();
    loop {
        let review = termf_visual::review(data_dir, id)?;
        let v = last(&review)?;
        let line = match v.progress.total {
            0 => v.progress.message.clone(),
            total => format!(
                "{}/{total}  {}  {}",
                v.progress.done, v.progress.phase, v.progress.message
            ),
        };
        if line != said {
            println!("  {line}");
            said = line;
        }
        if v.state != VersionState::Capturing {
            show(data_dir, &review);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn word(value: Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn show(data_dir: &std::path::Path, review: &Review) {
    let run = review.run.map(|r| format!(", run {r}")).unwrap_or_default();
    let ticket = review
        .ticket
        .as_deref()
        .map(|t| format!(" {t}"))
        .unwrap_or_default();
    println!(
        "review {}{run}:{ticket} {} ({})",
        review.id, review.title, review.repo
    );
    let Some(v) = review.versions.last() else {
        return;
    };
    let short = |c: &Option<String>| {
        c.as_deref()
            .map(|c| c[..c.len().min(10)].to_owned())
            .unwrap_or_default()
    };
    println!(
        "v{}: {}, {} → {}",
        v.n,
        word(json!(v.state)),
        short(&v.before),
        short(&v.after)
    );
    if let Some(error) = &v.error {
        println!("  error: {error}");
    }
    for note in &v.notes {
        println!("  note: {note}");
    }
    let dir = termf_visual::folder(data_dir, review.id, v.n);
    for s in &v.screens {
        let change = match (&s.error, s.changed) {
            (Some(e), _) => format!("not captured: {e}"),
            (None, true) => format!(
                "{} zone(s), {:.1}% of pixels",
                s.rects.len(),
                s.ratio * 100.0
            ),
            (None, false) => "unchanged".into(),
        };
        let state = s
            .state
            .as_deref()
            .map(|st| format!(" ({st})"))
            .unwrap_or_default();
        println!(
            "  {:<9} {}  {}{state}  {change}",
            word(json!(s.decision)),
            s.id,
            s.path
        );
        for t in &s.texts {
            let why = match (&t.justified_by, t.flagged) {
                (_, true) => "  ! not justified in the code".to_owned(),
                (Some(by), _) => format!("  ({by})"),
                (None, _) => String::new(),
            };
            let sign = if t.change == "added" { '+' } else { '-' };
            println!("            {sign} {:?}{why}", t.text);
        }
        if !s.comment.is_empty() {
            println!("            “{}”", s.comment);
        }
        for p in &s.pins {
            println!("            at {},{}: {}", p.x, p.y, p.comment);
        }
        if let Some(after) = &s.after {
            println!("            {}", dir.join(after).display());
        }
    }
    let count = |d: Decision| v.screens.iter().filter(|s| s.decision == d).count();
    println!(
        "{} approved, {} rejected, {} changes, {} pending{}",
        count(Decision::Approved),
        count(Decision::Rejected),
        count(Decision::Changes),
        count(Decision::Pending),
        v.outcome
            .map(|o| format!(": {}", word(json!(o))))
            .unwrap_or_default()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_pin() {
        let pin = parse_pin("120, 48,8px more, please").unwrap();
        assert_eq!(
            (pin.x, pin.y, pin.comment.as_str()),
            (120, 48, "8px more, please")
        );
        assert!(parse_pin("a,1,x").is_err());
        assert!(parse_pin("1,2,").is_err());
    }
}
