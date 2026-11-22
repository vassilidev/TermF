//! Escalation to the CTO: a task that failed twice, or a code review whose last round still holds findings, goes to the
//! CTO before the user. The CTO answers with a verdict TermF checks and applies: the task runs again with its
//! instructions, another role takes it over, the work stays as it is (a ruling kept in the project's decisions), or
//! the user decides. Once per step; then the user.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use termf_core::squads::same_role;

use crate::plan::object;
use crate::review::LEAVE;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// The task runs again with the CTO's instructions; for a review, each open finding goes to its task.
    Retry,
    /// Another role takes the task over.
    Reassign,
    /// The work stays as it is.
    Accept,
    /// The user decides: a product choice, or one the CTO cannot make.
    Ask,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Verdict {
    pub decision: Decision,
    pub role: String,
    pub instructions: String,
    pub ruling: String,
    pub findings: Vec<Pick>,
}

/// An open finding of the review and the task that fixes it, or `leave`.
#[derive(Deserialize, Clone, Debug)]
pub struct Pick {
    pub id: String,
    pub task: String,
}

pub fn schema() -> Value {
    let pick = object(json!({ "id": { "type": "string" }, "task": { "type": "string" } }));
    object(json!({
        "decision": { "type": "string", "enum": ["retry", "reassign", "accept", "ask"] },
        "role": { "type": "string" },
        "instructions": { "type": "string" },
        "ruling": { "type": "string" },
        "findings": { "type": "array", "items": pick },
    }))
}

/// What the CTO answers, after the situation its step's prompt gives.
pub const ANSWER: &str = "\n## What to answer\n\nChange nothing yourself. Your decision, as JSON: decision (retry: the \
     task runs again with your instructions; reassign: a role from the list above takes the task over, with your \
     instructions; accept: the work stays as it is; ask: the user decides), role (reassign only, else empty), \
     instructions (retry and reassign: what the next attempt must do differently; else empty), ruling (`Ruling: what \
     — why — cost if wrong`; when you accept, it joins the project's decisions) and findings (a review only: each \
     open finding's id with the task that fixes it, or `leave` to keep it as it is with your ruling as the reason; \
     else empty). For a review, retry sends the findings to their tasks, accept leaves them all, and reassign does \
     not apply. Answer ask when the choice is a product one (what the users get, what the ticket leaves open), or \
     when the code and the reports do not let you decide.\n";

/// What is wrong with `verdict` on a task (`review: None`), or on a review's open findings and the tasks that may
/// fix them; `roles`: those that may take a task; `current`: the task's role.
pub fn check(
    verdict: &Verdict,
    review: Option<(&[String], &[String])>,
    roles: &[String],
    current: &str,
) -> Vec<String> {
    let mut problems = Vec::new();
    if verdict.ruling.trim().is_empty() {
        problems.push("the decision has no ruling".to_owned());
    }
    let instructed = !verdict.instructions.trim().is_empty();
    if review.is_none() && !verdict.findings.is_empty() {
        problems.push("findings are for a review: a task has none".to_owned());
    }
    match (review, verdict.decision) {
        (None, Decision::Retry) if !instructed => {
            problems.push("retry needs your instructions".to_owned())
        }
        (None, Decision::Reassign) => {
            let role = verdict.role.trim();
            if !instructed {
                problems.push("reassign needs your instructions".to_owned());
            }
            if !roles.iter().any(|r| same_role(r, role)) {
                problems.push(format!(
                    "{role:?} is not a role that may take the task ({})",
                    roles.join(", ")
                ));
            } else if same_role(role, current) {
                problems.push(format!(
                    "the task already has the role {current}: retry it instead"
                ));
            }
        }
        (Some(_), Decision::Reassign) => problems
            .push("reassign is for a task: send each finding to its task with retry".to_owned()),
        (Some((open, tasks)), Decision::Retry) => {
            for id in open {
                let picks: Vec<&Pick> = verdict.findings.iter().filter(|p| &p.id == id).collect();
                match picks.as_slice() {
                    [] => problems.push(format!("say which task fixes {id}, or leave it")),
                    [pick] if pick.task != LEAVE && !tasks.contains(&pick.task) => {
                        problems.push(format!(
                            "{id}: {} is neither a task of the plan ({}) nor {LEAVE}",
                            pick.task,
                            tasks.join(", ")
                        ))
                    }
                    [_] => {}
                    _ => problems.push(format!("{id} is named twice")),
                }
            }
            for pick in verdict.findings.iter().filter(|p| !open.contains(&p.id)) {
                problems.push(format!("{} is not an open finding", pick.id));
            }
        }
        _ => {}
    }
    problems
}

/// The verdict in a line, for the run's events and its step's report.
pub fn describe(verdict: &Verdict) -> String {
    let what = match verdict.decision {
        Decision::Retry if verdict.findings.is_empty() => {
            format!("again, with: {}", verdict.instructions.trim())
        }
        Decision::Retry => {
            let picks: Vec<String> = verdict
                .findings
                .iter()
                .map(|p| format!("{} → {}", p.id, p.task))
                .collect();
            format!("the findings go back: {}", picks.join(", "))
        }
        Decision::Reassign => format!(
            "to the role {}, with: {}",
            verdict.role.trim(),
            verdict.instructions.trim()
        ),
        Decision::Accept => "accepted as it is".to_owned(),
        Decision::Ask => "left to you".to_owned(),
    };
    format!("the CTO decides: {what}\n{}", verdict.ruling.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(decision: Decision, role: &str, findings: &[(&str, &str)]) -> Verdict {
        Verdict {
            decision,
            role: role.into(),
            instructions: "use the existing helper".into(),
            ruling: "Ruling: x — y — z".into(),
            findings: findings
                .iter()
                .map(|(id, task)| Pick {
                    id: (*id).into(),
                    task: (*task).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_verdict_fits_its_step() {
        let roles = ["front".to_owned(), "back".to_owned()];
        assert!(
            check(
                &verdict(Decision::Reassign, "back", &[]),
                None,
                &roles,
                "front"
            )
            .is_empty()
        );
        let same = check(
            &verdict(Decision::Reassign, "front", &[]),
            None,
            &roles,
            "front",
        );
        assert!(same[0].contains("retry it instead"), "{same:?}");
        let unknown = check(
            &verdict(Decision::Reassign, "dba", &[]),
            None,
            &roles,
            "front",
        );
        assert!(unknown[0].contains("not a role"), "{unknown:?}");
        let stray = check(
            &verdict(Decision::Retry, "", &[("F1", "T1")]),
            None,
            &roles,
            "front",
        );
        assert!(stray[0].contains("findings are for a review"), "{stray:?}");
        let mut silent = verdict(Decision::Accept, "", &[]);
        silent.ruling = " ".into();
        assert_eq!(check(&silent, None, &roles, "front").len(), 1);

        let open = ["F1".to_owned(), "F2".to_owned()];
        let tasks = ["T1".to_owned()];
        let review = Some((&open[..], &tasks[..]));
        let fine = verdict(Decision::Retry, "", &[("F1", "T1"), ("F2", LEAVE)]);
        assert!(check(&fine, review, &roles, "").is_empty());
        let wrong = verdict(Decision::Retry, "", &[("F1", "T9"), ("F3", "T1")]);
        let problems = check(&wrong, review, &roles, "");
        assert_eq!(
            problems.len(),
            3,
            "T9, F2 missing, F3 not open: {problems:?}"
        );
        assert_eq!(
            check(
                &verdict(Decision::Reassign, "back", &[]),
                review,
                &roles,
                ""
            )
            .len(),
            1
        );
        assert!(check(&verdict(Decision::Accept, "", &[]), review, &roles, "").is_empty());
    }
}
