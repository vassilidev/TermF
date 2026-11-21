//! `termf worktree setup | teardown [<path>]`: runs a worktree's setup again, or drops its database copy and
//! gives its ports back. Works on the files and the run database directly: TermF does not need to run.

use std::path::PathBuf;

use anyhow::{Result, bail};
use termf_tickets::Tickets;
use termf_worktree::{Outcome, Report};

pub fn run(args: &[&str]) -> Result<()> {
    let (action, path) = match args {
        [action @ ("setup" | "teardown")] => (*action, PathBuf::from(".")),
        [action @ ("setup" | "teardown"), path] => (*action, PathBuf::from(path)),
        _ => bail!(crate::USAGE),
    };
    let path = std::path::absolute(path)?;
    let tickets = Tickets::new(
        termf_core::default_config_dir()?,
        termf_core::default_data_dir()?,
    );
    let (run, report) = match action {
        "setup" => tickets.prepare(&path)?,
        _ => tickets.unprepare(&path)?,
    };
    print(&report);
    if let Some(run) = run {
        println!("recorded in the journal of run {}", run.id);
    }
    if action == "setup" && !report.saved {
        println!("detected recipe (not saved): the project has no saved recipe yet");
    }
    if report.failed() {
        bail!("some steps failed");
    }
    Ok(())
}

fn print(report: &Report) {
    for step in &report.steps {
        let mark = match step.outcome {
            o if o.failed() => "✗",
            Outcome::Skipped => "-",
            _ => "✓",
        };
        let kind = serde_json::to_value(step.kind).unwrap_or_default();
        let secs = step.ms as f64 / 1000.0;
        println!(
            "{mark} {:<8}  {}  ({secs:.1} s)",
            kind.as_str().unwrap_or_default(),
            step.describe()
        );
    }
    for (key, value) in &report.env {
        println!("  {key}={value}");
    }
}
