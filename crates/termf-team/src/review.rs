//! The code review of a run (a `review` step): the reviewer's findings, checked, then each important one sent
//! back to the task that must fix it; the checks and the review run again, a few rounds at most. A finding with
//! no clear task, or still there at the last round, waits for the user, as a questionnaire.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use termf_core::squads::same_role;

use crate::plan::{Question, QuestionKind, Task, object};

/// The answer to a review question that leaves its finding as it is.
pub const LEAVE: &str = "leave";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A bug, a broken criterion or contract, a convention the project sets, a test without value.
    Important,
    Nit,
}

/// What the reviewer answers.
#[derive(Deserialize, Debug)]
pub struct Answer {
    pub summary: String,
    pub findings: Vec<Proposed>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Proposed {
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    pub scenario: String,
    pub fix: String,
    /// The task that must fix it, by its id; or a role; empty when none can.
    pub task: String,
}

/// A round of a review step: what the reviewer found, and what became of each finding.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Review {
    pub step: String,
    /// From 1.
    pub round: u32,
    pub summary: String,
    pub findings: Vec<Finding>,
    /// Seconds since the Unix epoch.
    pub at: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Finding {
    /// F1, F2… within its round.
    pub id: String,
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    pub scenario: String,
    pub fix: String,
    /// The task as the reviewer named it.
    pub task: String,
    pub fate: Fate,
    /// The task it went to.
    pub to: Option<String>,
    /// Why it stays as it is, when someone said.
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Fate {
    /// Back to the task `to`, in a new attempt.
    Sent,
    /// It waits for the user.
    Asked,
    /// A nit, only listed.
    Reported,
    /// The user left it as it is.
    Left,
}

/// What `[team]` says of the review: the project's file first, then `termf.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// `review`, on by default.
    pub on: bool,
    /// `review_rounds`, 2 by default: the most reviews of a step, each but the last followed by its fixes.
    pub rounds: u32,
    /// `review_nits = "fix"`: the nits go back to their task too; `"report"` (the default) only lists them.
    pub fix_nits: bool,
}

impl Settings {
    pub fn read(files: &[&Path]) -> Result<Settings> {
        let mut settings = Settings {
            on: true,
            rounds: 2,
            fix_nits: false,
        };
        if let Some((value, file)) = team_key(files, "review")? {
            settings.on = value
                .as_bool()
                .with_context(|| format!("`[team] review` in {file} must be true or false"))?;
        }
        if let Some((value, file)) = team_key(files, "review_rounds")? {
            settings.rounds = value
                .as_integer()
                .filter(|n| (1..=10).contains(n))
                .with_context(|| format!("`[team] review_rounds` in {file} must be from 1 to 10"))?
                as u32;
        }
        if let Some((value, file)) = team_key(files, "review_nits")? {
            settings.fix_nits = match value.as_str() {
                Some("fix") => true,
                Some("report") => false,
                _ => bail!("`[team] review_nits` in {file} must be \"report\" or \"fix\""),
            };
        }
        Ok(settings)
    }
}

/// `[team] <key>` in the first of `files` that sets it, with that file's name.
fn team_key(files: &[&Path], key: &str) -> Result<Option<(toml::Value, String)>> {
    for file in files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        let table: toml::Table =
            toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
        if let Some(value) = table.get("team").and_then(|t| t.get(key)) {
            return Ok(Some((value.clone(), file.display().to_string())));
        }
    }
    Ok(None)
}

pub fn schema() -> Value {
    let finding = object(json!({
        "severity": { "type": "string", "enum": ["important", "nit"] },
        "file": { "type": "string" },
        "line": { "type": "integer" },
        "scenario": { "type": "string" },
        "fix": { "type": "string" },
        "task": { "type": "string" },
    }));
    object(json!({
        "summary": { "type": "string" },
        "findings": { "type": "array", "items": finding },
    }))
}

/// The findings of `answer`, each with the task it goes to when that is clear: a task of the plan by its id, or
/// the role of exactly one of them. `role`: whether a name is a role of the library. Else what is wrong.
pub fn check(
    answer: &Answer,
    tasks: &[Task],
    role: impl Fn(&str) -> bool,
) -> Result<Vec<(Proposed, Option<String>)>, Vec<String>> {
    let mut problems = Vec::new();
    if answer.summary.trim().is_empty() {
        problems.push("the review has no summary".to_owned());
    }
    let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    let mut found = Vec::new();
    for (n, f) in answer.findings.iter().enumerate() {
        let at = format!("finding {} ({}:{})", n + 1, f.file, f.line);
        let path = Path::new(&f.file);
        let rooted = path.is_absolute() || f.file.starts_with(['/', '\\']);
        if f.file.trim().is_empty() || rooted || path.components().any(|c| c.as_os_str() == "..") {
            problems.push(format!(
                "{at}: the file must be a path inside the repository"
            ));
        }
        if f.line == 0 {
            problems.push(format!("{at}: lines count from 1"));
        }
        if f.scenario.trim().is_empty() || f.fix.trim().is_empty() {
            problems.push(format!("{at} must say how it fails and the fix expected"));
        }
        let name = f.task.trim();
        let by_role: Vec<&Task> = tasks.iter().filter(|t| same_role(&t.role, name)).collect();
        let to = match (ids.contains(&name), by_role.as_slice()) {
            _ if name.is_empty() => None,
            (true, _) => Some(name.to_owned()),
            (false, [one]) => Some(one.id.clone()),
            (false, _) if role(name) => None,
            _ => {
                problems.push(format!(
                    "{at}: {name} is neither a task of the plan ({}) nor a role",
                    ids.join(", ")
                ));
                None
            }
        };
        found.push((f.clone(), to));
    }
    match problems.is_empty() {
        true => Ok(found),
        false => Err(problems),
    }
}

/// The questionnaire of the findings the user decides: which task fixes each, or leave it as it is. None
/// without a task to send them to. `last`: why a finding with a task asks the user all the same.
pub fn questions(findings: &[&Finding], tasks: &[Task], last: &str) -> Vec<Question> {
    if tasks.is_empty() {
        return Vec::new();
    }
    let mut options: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
    options.push(LEAVE.into());
    let mut implications: Vec<String> = tasks
        .iter()
        .map(|t| {
            format!(
                "{} ({}) fixes it in a new attempt, then the checks and the review run again",
                t.id, t.role
            )
        })
        .collect();
    implications
        .push("nobody fixes it: it stays in the run's report, with your reason as the note".into());
    findings
        .iter()
        .map(|f| Question {
            id: f.id.clone(),
            theme: "technical".into(),
            question: format!(
                "Who fixes {} ({}) at {}:{}? {}",
                f.id,
                severity(f.severity),
                f.file,
                f.line,
                f.scenario
            ),
            why: format!(
                "Fix expected: {} {}",
                f.fix,
                match f.to {
                    Some(_) => last,
                    None => "No task of the plan clearly owns it.",
                }
            ),
            kind: QuestionKind::Choice,
            options: options.clone(),
            proposed: f.to.iter().cloned().collect(),
            implications: implications.clone(),
        })
        .collect()
}

/// What a task gets back: the findings it must fix.
pub fn feedback(round: u32, findings: &[&Finding]) -> String {
    let mut text = format!(
        "The code review (round {round}) found what to fix in your work, now merged with the other tasks' in \
         this folder:\n"
    );
    for f in findings {
        text.push_str(&format!(
            "- {}:{} ({}): {} Fix expected: {}\n",
            f.file,
            f.line,
            severity(f.severity),
            f.scenario,
            f.fix
        ));
    }
    text.push_str("Fix these, only these; say in your report's summary what you changed for each.");
    text
}

/// How a round ends, in a few words: the tasks its findings go back to, and how many wait, are only listed or
/// are left as they are.
pub fn tally(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "no finding".into();
    }
    let count = |fate: Fate| findings.iter().filter(|f| f.fate == fate).count();
    let mut to: Vec<&str> = findings
        .iter()
        .filter(|f| f.fate == Fate::Sent)
        .filter_map(|f| f.to.as_deref())
        .collect();
    to.sort();
    to.dedup();
    let mut parts = vec![format!("{} finding(s)", findings.len())];
    if !to.is_empty() {
        parts.push(format!("back to {}", to.join(", ")));
    }
    for (fate, label) in [
        (Fate::Asked, "wait for the user"),
        (Fate::Reported, "listed"),
        (Fate::Left, "left as they are"),
    ] {
        if count(fate) > 0 {
            parts.push(format!("{} {label}", count(fate)));
        }
    }
    parts.join(", ")
}

/// One line per finding, with what became of it.
pub fn lines(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|f| {
            let fate = match (f.fate, &f.to, &f.reason) {
                (Fate::Sent, Some(to), _) => format!("sent to {to}"),
                (Fate::Asked, _, _) => "waits for the user".into(),
                (Fate::Reported, _, _) => "reported".into(),
                (Fate::Left, _, Some(why)) => format!("left as it is: {why}"),
                _ => "left as it is".into(),
            };
            format!(
                "{} {} {}:{} ({fate}): {}",
                f.id,
                severity(f.severity),
                f.file,
                f.line,
                f.scenario
            )
        })
        .collect()
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Important => "important",
        Severity::Nit => "nit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Size;

    fn task(id: &str, role: &str) -> Task {
        Task {
            id: id.into(),
            role: role.into(),
            squad: String::new(),
            repo: "api".into(),
            size: Size::S,
            covers: vec![],
            goal: "g".into(),
            files_hint: vec![],
            depends_on: vec![],
            ui_impact: false,
        }
    }

    fn proposed(file: &str, line: u32, task: &str) -> Proposed {
        Proposed {
            severity: Severity::Important,
            file: file.into(),
            line,
            scenario: "an empty list panics".into(),
            fix: "return early".into(),
            task: task.into(),
        }
    }

    #[test]
    fn a_finding_goes_to_one_task_or_to_nobody() {
        let tasks = [task("T1", "back"), task("T2", "front"), task("T3", "front")];
        let role = |n: &str| ["back", "front", "devops"].contains(&n);
        let answer = |findings| Answer {
            summary: "checked the export".into(),
            findings,
        };
        let found = check(
            &answer(vec![
                proposed("src/a.rs", 3, "T2"),
                proposed("src/a.rs", 3, "back"),
                proposed("src/a.rs", 3, "front"),
                proposed("src/a.rs", 3, "devops"),
                proposed("src/a.rs", 3, ""),
            ]),
            &tasks,
            role,
        )
        .unwrap();
        let to: Vec<Option<&str>> = found.iter().map(|(_, t)| t.as_deref()).collect();
        assert_eq!(
            to,
            [Some("T2"), Some("T1"), None, None, None],
            "by id; by the role of one task only; two front tasks, no devops task and no name are unclear"
        );

        let wrong = check(
            &answer(vec![
                proposed("/etc/passwd", 1, "T1"),
                proposed("../x.rs", 1, "T1"),
                proposed("src/a.rs", 0, "T1"),
                proposed("src/a.rs", 2, "T9"),
            ]),
            &tasks,
            role,
        )
        .unwrap_err();
        assert_eq!(wrong.len(), 4, "{wrong:?}");
        assert!(wrong[3].contains("T9 is neither a task"), "{wrong:?}");
    }
}
