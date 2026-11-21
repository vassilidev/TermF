//! What the agents answer: the orchestrator's plan (ARCHITECTURE appendix A) and the report every step ends
//! with (appendix B). Claude Code holds them to a JSON schema (`--json-schema`); TermF checks them again, since
//! a schema says nothing of a task that depends on itself or covers a criterion nobody wrote.

use std::collections::{BTreeMap, HashSet};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Plan {
    pub summary: String,
    pub acceptance: Vec<Criterion>,
    pub out_of_scope: Vec<String>,
    /// While there are any, nothing runs: the user answers first.
    pub questions: Vec<String>,
    pub tasks: Vec<Task>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Criterion {
    pub id: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Task {
    pub id: String,
    pub role: String,
    /// The repository's name in the project.
    pub repo: String,
    pub size: Size,
    pub covers: Vec<String>,
    pub goal: String,
    /// The files it should touch: paths, folders or globs, relative to the repository.
    pub files_hint: Vec<String>,
    pub depends_on: Vec<String>,
    pub ui_impact: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    S,
    M,
    L,
}

impl Size {
    /// The effort a task of this size gets (ARCHITECTURE §7.3).
    pub fn effort(self) -> &'static str {
        match self {
            Size::S => "medium",
            Size::M => "high",
            Size::L => "xhigh",
        }
    }

    /// Lines changed beyond which the diff is out of proportion with the estimate.
    pub fn lines(self) -> usize {
        match self {
            Size::S => 200,
            Size::M => 800,
            Size::L => 3000,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Report {
    pub status: ReportStatus,
    pub summary: String,
    pub covers: Vec<String>,
    pub text_changes: Vec<TextChange>,
    pub affected_views: Vec<View>,
    pub out_of_scope_findings: Vec<String>,
    /// With `blocked`: what the agent needs to know to go on.
    #[serde(default)]
    pub questions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Done,
    /// It cannot go on without an answer.
    Blocked,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TextChange {
    pub file: String,
    pub text: String,
    pub justified_by: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct View {
    pub url: String,
    pub state: String,
    pub viewports: Vec<String>,
}

fn strings() -> Value {
    json!({ "type": "array", "items": { "type": "string" } })
}

fn object(properties: Value) -> Value {
    let required: Vec<&String> = properties
        .as_object()
        .map_or(Vec::new(), |p| p.keys().collect());
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

pub fn plan_schema() -> Value {
    let task = object(json!({
        "id": { "type": "string" },
        "role": { "type": "string" },
        "repo": { "type": "string" },
        "size": { "type": "string", "enum": ["S", "M", "L"] },
        "covers": strings(),
        "goal": { "type": "string" },
        "files_hint": strings(),
        "depends_on": strings(),
        "ui_impact": { "type": "boolean" },
    }));
    let criterion = object(json!({ "id": { "type": "string" }, "text": { "type": "string" } }));
    object(json!({
        "summary": { "type": "string" },
        "acceptance": { "type": "array", "items": criterion },
        "out_of_scope": strings(),
        "questions": strings(),
        "tasks": { "type": "array", "items": task },
    }))
}

pub fn report_schema() -> Value {
    let change = object(json!({
        "file": { "type": "string" },
        "text": { "type": "string" },
        "justified_by": { "type": "string" },
    }));
    let view = object(json!({
        "url": { "type": "string" },
        "state": { "type": "string" },
        "viewports": strings(),
    }));
    object(json!({
        "status": { "type": "string", "enum": ["done", "blocked", "failed"] },
        "summary": { "type": "string" },
        "covers": strings(),
        "text_changes": { "type": "array", "items": change },
        "affected_views": { "type": "array", "items": view },
        "out_of_scope_findings": strings(),
        "questions": strings(),
    }))
}

/// The answer of a turn: what the schema gave, else JSON in its text, alone or in a fenced block.
pub fn answer<T: serde::de::DeserializeOwned>(
    structured: Option<&Value>,
    text: Option<&str>,
) -> Result<T> {
    if let Some(value) = structured {
        return serde_json::from_value(value.clone()).map_err(|e| anyhow::anyhow!("{e}"));
    }
    let text = text.unwrap_or_default().trim();
    if text.is_empty() {
        bail!("the agent gave no answer");
    }
    let fenced = text
        .rfind("```json")
        .and_then(|start| {
            let body = &text[start + 7..];
            body.find("```").map(|end| &body[..end])
        })
        .unwrap_or(text);
    let braces = match (fenced.find('{'), fenced.rfind('}')) {
        (Some(start), Some(end)) if start < end => &fenced[start..=end],
        _ => bail!("the agent's answer holds no JSON object"),
    };
    serde_json::from_str(braces).map_err(|e| anyhow::anyhow!("{e}"))
}

/// What is wrong with a plan beyond its schema. `roles`: those the library has; `repo`: the run's repository,
/// the only one a task may name until runs span several.
pub fn check_plan(plan: &Plan, roles: &[String], repo: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if plan.tasks.is_empty() && plan.questions.is_empty() {
        problems.push("the plan has no task and no question".to_owned());
    }
    let criteria: HashSet<&str> = plan.acceptance.iter().map(|c| c.id.as_str()).collect();
    let mut ids = HashSet::new();
    for task in &plan.tasks {
        let id = &task.id;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
        {
            problems.push(format!(
                "task id {id:?} is not made of letters, digits, - or _"
            ));
        }
        if !ids.insert(id.as_str()) {
            problems.push(format!("two tasks are named {id}"));
        }
        if !roles.contains(&task.role) {
            problems.push(format!("task {id}: no role {} in the library", task.role));
        }
        if !task.repo.is_empty() && !task.repo.eq_ignore_ascii_case(repo) {
            problems.push(format!(
                "task {id}: repository {} is not the run's ({repo}); a run works on one repository for now",
                task.repo
            ));
        }
        if task.goal.trim().is_empty() {
            problems.push(format!("task {id} has no goal"));
        }
        for covered in &task.covers {
            if !criteria.contains(covered.as_str()) {
                problems.push(format!(
                    "task {id} covers {covered}, which is not a criterion"
                ));
            }
        }
        for file in &task.files_hint {
            let path = std::path::Path::new(file);
            if path.is_absolute() || path.components().any(|c| c.as_os_str() == "..") {
                problems.push(format!(
                    "task {id}: {file} is not a path inside the repository"
                ));
            }
        }
    }
    for task in &plan.tasks {
        for dep in &task.depends_on {
            if !ids.contains(dep.as_str()) {
                problems.push(format!(
                    "task {} depends on {dep}, which is not a task",
                    task.id
                ));
            }
        }
    }
    if problems.is_empty() && cycle(&plan.tasks) {
        problems.push("the tasks depend on each other in a cycle".to_owned());
    }
    problems
}

fn cycle(tasks: &[Task]) -> bool {
    let deps: BTreeMap<&str, &[String]> = tasks
        .iter()
        .map(|t| (t.id.as_str(), t.depends_on.as_slice()))
        .collect();
    let mut done: HashSet<&str> = HashSet::new();
    // Kahn: what never becomes free is in a cycle.
    loop {
        let free: Vec<&str> = deps
            .iter()
            .filter(|(id, d)| !done.contains(*id) && d.iter().all(|x| done.contains(x.as_str())))
            .map(|(id, _)| *id)
            .collect();
        if free.is_empty() {
            return done.len() < deps.len();
        }
        done.extend(free);
    }
}

/// What is wrong with a report. `criteria`: the plan's, when there is one.
pub fn check_report(report: &Report, criteria: Option<&[Criterion]>) -> Vec<String> {
    let mut problems = Vec::new();
    if report.summary.trim().is_empty() {
        problems.push("the report has no summary".to_owned());
    }
    if let Some(criteria) = criteria {
        for covered in &report.covers {
            if !criteria.iter().any(|c| &c.id == covered) {
                problems.push(format!(
                    "the report covers {covered}, which is not a criterion"
                ));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, deps: &[&str]) -> Task {
        Task {
            id: id.into(),
            role: "back".into(),
            repo: "api".into(),
            size: Size::M,
            covers: vec!["AC1".into()],
            goal: "do it".into(),
            files_hint: vec!["src/a.rs".into()],
            depends_on: deps.iter().map(|d| d.to_string()).collect(),
            ui_impact: false,
        }
    }

    fn plan(tasks: Vec<Task>) -> Plan {
        Plan {
            summary: "s".into(),
            acceptance: vec![Criterion {
                id: "AC1".into(),
                text: "it works".into(),
            }],
            out_of_scope: vec![],
            questions: vec![],
            tasks,
        }
    }

    #[test]
    fn checks_what_the_schema_cannot() {
        let roles = ["back".to_owned(), "front".to_owned()];
        assert!(
            check_plan(
                &plan(vec![task("T1", &[]), task("T2", &["T1"])]),
                &roles,
                "API"
            )
            .is_empty()
        );

        let mut bad = task("T2", &["T9"]);
        bad.role = "wizard".into();
        bad.repo = "front".into();
        bad.covers = vec!["AC7".into()];
        bad.files_hint = vec!["../etc/passwd".into(), "/abs".into()];
        let problems = check_plan(
            &plan(vec![task("T1", &[]), task("T1", &[]), bad]),
            &roles,
            "api",
        );
        let all = problems.join("\n");
        for expected in [
            "two tasks are named T1",
            "no role wizard",
            "repository front is not the run's",
            "covers AC7",
            "../etc/passwd is not a path",
            "/abs is not a path",
            "depends on T9",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }

        let looped = plan(vec![task("T1", &["T2"]), task("T2", &["T1"])]);
        assert_eq!(
            check_plan(&looped, &roles, "api"),
            ["the tasks depend on each other in a cycle"]
        );
        assert_eq!(check_plan(&plan(vec![]), &roles, "api").len(), 1);
    }

    #[test]
    fn reads_an_answer_from_the_schema_or_the_text() {
        let report = json!({"status": "done", "summary": "ok", "covers": [], "text_changes": [],
            "affected_views": [], "out_of_scope_findings": []});
        let from_schema: Report = answer(Some(&report), None).unwrap();
        assert_eq!(from_schema.status, ReportStatus::Done);
        let text = format!("Here it is:\n```json\n{report}\n```\nDone.");
        let from_text: Report = answer(None, Some(&text)).unwrap();
        assert_eq!(from_text, from_schema);
        assert!(answer::<Report>(None, Some("I did it.")).is_err());
        assert!(answer::<Report>(None, None).is_err());
        assert!(answer::<Report>(Some(&json!({"status": "maybe"})), None).is_err());
        assert_eq!(check_report(&from_schema, None), Vec::<String>::new());
        let empty = Report {
            summary: " ".into(),
            covers: vec!["AC2".into()],
            ..from_schema
        };
        assert_eq!(
            check_report(&empty, Some(&plan(vec![]).acceptance)).len(),
            2
        );
    }

    #[test]
    fn schemas_require_every_field() {
        let schema = plan_schema();
        assert_eq!(schema["required"].as_array().unwrap().len(), 5);
        assert_eq!(
            schema["properties"]["tasks"]["items"]["required"]
                .as_array()
                .unwrap()
                .len(),
            9
        );
        assert_eq!(
            report_schema()["properties"]["status"]["enum"][1],
            "blocked"
        );
    }
}
