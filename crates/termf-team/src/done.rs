//! The definition of done (`[team] done`): what must hold before a run is delivered, read at its delivery gate from
//! what the run did. An unmet criterion holds the delivery until the user passes over it, with a reason that stays in
//! the project's decisions.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use termf_core::squads::canonical_role;

use crate::{Step, StepKind, StepState, TeamRun, review};

/// Every criterion, the default list.
pub const CRITERIA: [&str; 5] = ["checks", "review", "security", "visual", "docs"];

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Done {
    pub criteria: Vec<Criterion>,
    /// The user delivered over the unmet criteria: why.
    pub overridden: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Criterion {
    /// One of [`CRITERIA`].
    pub id: String,
    pub met: bool,
    /// What the run shows of it.
    pub note: String,
}

impl Done {
    /// Whether the delivery may go: every criterion met, or the user passed over them.
    pub fn passes(&self) -> bool {
        self.overridden.is_some() || self.criteria.iter().all(|c| c.met)
    }

    /// The unmet criteria, each with what holds it, on one line.
    pub fn unmet(&self) -> String {
        self.criteria
            .iter()
            .filter(|c| !c.met)
            .map(|c| format!("{} ({})", c.id, c.note))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// `[team] done` of the first of `files` that sets it (the project's, then `termf.toml`), else every criterion.
pub fn criteria(files: &[&Path]) -> Result<Vec<String>> {
    let Some((value, file)) = review::team_key(files, "done")? else {
        return Ok(CRITERIA.map(str::to_owned).to_vec());
    };
    let wrong = || {
        anyhow::anyhow!(
            "`[team] done` in {file} must list criteria among {}, each once",
            CRITERIA.join(", ")
        )
    };
    let mut ids: Vec<String> = Vec::new();
    for item in value.as_array().ok_or_else(wrong)? {
        match item.as_str() {
            Some(id) if CRITERIA.contains(&id) && !ids.iter().any(|i| i == id) => {
                ids.push(id.to_owned())
            }
            _ => return Err(wrong()),
        }
    }
    Ok(ids)
}

/// What the steps after the plan read of it.
pub fn describe(ids: &[String]) -> String {
    ids.iter()
        .map(|id| match id.as_str() {
            "checks" => "the project's checks pass",
            "review" => "the code review leaves no important finding open",
            "security" => "the security review confirms no vulnerability",
            "visual" => "the visual review approves the interface when it changes",
            _ => "the documentation follows when the public behaviour changes",
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Each criterion of `ids`, as the run stands.
pub fn evaluate(run: &TeamRun, ids: &[String]) -> Done {
    let criteria = ids
        .iter()
        .map(|id| {
            let (met, note) = match check(run, id) {
                Ok(note) => (true, note),
                Err(note) => (false, note),
            };
            Criterion {
                id: id.clone(),
                met,
                note,
            }
        })
        .collect();
    Done {
        criteria,
        overridden: None,
    }
}

fn check(run: &TeamRun, id: &str) -> Result<String, String> {
    let of = |kind: StepKind| run.steps.iter().filter(move |s| s.kind == kind);
    match id {
        "checks" => {
            over(of(StepKind::Shell), true, "the workflow runs no checks")?;
            Ok("passed".into())
        }
        "review" => {
            let steps = over(
                of(StepKind::Review),
                false,
                "the workflow has no code review",
            )?;
            let mut open = 0;
            let mut left = 0;
            for step in steps {
                let last = run.reviews.iter().rev().find(|r| r.step == step.key);
                for f in last.iter().flat_map(|r| &r.findings) {
                    if f.severity != review::Severity::Important {
                        continue;
                    }
                    match f.fate {
                        review::Fate::Asked | review::Fate::Sent => open += 1,
                        review::Fate::Left => left += 1,
                        _ => {}
                    }
                }
            }
            match (open, left) {
                (0, 0) => Ok("no important finding open".into()),
                (0, n) => Ok(format!("{n} important finding(s) left as they are")),
                (n, _) => Err(format!("{n} important finding(s) open")),
            }
        }
        "security" => {
            let secops = run
                .steps
                .iter()
                .filter(|s| s.role.as_deref().map(canonical_role) == Some("secops"));
            let steps = over(secops, true, "the workflow has no security review")?;
            let found: Vec<&String> = steps
                .iter()
                .filter_map(|s| s.report.as_ref())
                .flat_map(|r| &r.vulnerabilities)
                .collect();
            match found.as_slice() {
                [] => Ok("no vulnerability confirmed".into()),
                _ => Err(format!(
                    "{} vulnerability(ies): {}",
                    found.len(),
                    found
                        .iter()
                        .map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                )),
            }
        }
        "visual" => {
            if !run.ui_changed {
                return Ok("the interface did not change".into());
            }
            over(
                of(StepKind::Visual),
                false,
                "the interface changed and the workflow has no visual review",
            )?;
            Ok("approved".into())
        }
        _ => {
            let tasks: Vec<&Step> = of(StepKind::Task).collect();
            let public: Vec<&str> = tasks
                .iter()
                .filter(|s| s.report.as_ref().is_some_and(|r| r.public_change))
                .filter_map(|s| s.task.as_deref())
                .collect();
            if public.is_empty() {
                return Ok("no task changed the public behaviour".into());
            }
            let docs: Vec<&str> = tasks
                .iter()
                .flat_map(|s| &s.files)
                .map(String::as_str)
                .filter(|f| doc_file(f))
                .collect();
            match docs.as_slice() {
                [] => Err(format!(
                    "{} changed the public behaviour; no documentation changed",
                    public.join(", ")
                )),
                _ => Ok(format!("documentation changed: {}", docs.join(", "))),
            }
        }
    }
}

/// `steps`, when there are some and each is done (and not accepted as it is after a failure, when `accepted` fails
/// it); else what holds the criterion.
fn over<'a>(
    steps: impl Iterator<Item = &'a Step>,
    accepted: bool,
    none: &str,
) -> Result<Vec<&'a Step>, String> {
    let steps: Vec<&Step> = steps.collect();
    if steps.is_empty() {
        return Err(none.to_owned());
    }
    for step in &steps {
        let why = match step.state {
            StepState::Done if accepted && step.accepted => "accepted as it is by you",
            StepState::Done => continue,
            StepState::Skipped => step.note.as_deref().unwrap_or("skipped"),
            _ => "not done yet",
        };
        return Err(format!("{}: {why}", step.key));
    }
    Ok(steps)
}

/// A file of the documentation: Markdown and the like, or anything in a `docs` folder.
fn doc_file(file: &str) -> bool {
    let lower = file.to_lowercase();
    let ext = Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    ["md", "mdx", "markdown", "rst", "adoc"].contains(&ext)
        || lower
            .split('/')
            .rev()
            .skip(1)
            .any(|dir| dir == "docs" || dir == "doc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_the_documentation() {
        for doc in [
            "README.md",
            "docs/api.yaml",
            "web/doc/x.html",
            "CHANGELOG.rst",
        ] {
            assert!(doc_file(doc), "{doc}");
        }
        for code in ["src/docs.rs", "requirements.txt", "docs"] {
            assert!(!doc_file(code), "{code}");
        }
    }
}
