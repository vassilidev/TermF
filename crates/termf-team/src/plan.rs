//! What the agents answer: the orchestrator's plan (ARCHITECTURE appendix A) and the report every step ends
//! with (appendix B). Claude Code holds them to a JSON schema (`--json-schema`); TermF checks them again, since
//! a schema says nothing of a task that depends on itself or covers a criterion nobody wrote.

use std::collections::{BTreeMap, HashSet};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use termf_core::squads::{Membership, Squad, Team, same_role};

use crate::rules;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Plan {
    pub summary: String,
    pub acceptance: Vec<Criterion>,
    pub out_of_scope: Vec<String>,
    /// While there are any, nothing runs: the user answers first.
    pub questions: Vec<Question>,
    pub tasks: Vec<Task>,
}

/// What the user must answer before a step goes on: the same questionnaire for the triage, the PRD, the
/// framing, the plan and any step that reports `blocked`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(from = "QuestionIn")]
pub struct Question {
    pub id: String,
    /// One of [`THEMES`]: how the form groups the questions.
    pub theme: String,
    pub question: String,
    /// Why it matters: what the answer changes, or the side effect found.
    pub why: String,
    pub kind: QuestionKind,
    pub options: Vec<String>,
    /// The answer proposed, in the shape of an answer.
    pub proposed: Vec<String>,
    /// What each option implies, in the order of `options`; empty when there is nothing to say.
    pub implications: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QuestionKind {
    /// One of `options`.
    Choice,
    /// Any of `options`.
    Multi,
    /// `yes` or `no`.
    Bool,
    Text,
}

/// How the questions are grouped.
pub const THEMES: [&str; 7] = [
    "product",
    "scope",
    "interface",
    "technical",
    "security",
    "delivery",
    "other",
];

/// A question as stored before the questionnaire, a sentence, still reads.
#[derive(Deserialize)]
#[serde(untagged)]
enum QuestionIn {
    Full {
        id: String,
        theme: String,
        question: String,
        why: String,
        kind: QuestionKind,
        options: Vec<String>,
        proposed: Vec<String>,
        #[serde(default)]
        implications: Vec<String>,
    },
    Sentence(String),
}

impl From<QuestionIn> for Question {
    fn from(q: QuestionIn) -> Question {
        match q {
            QuestionIn::Full {
                id,
                theme,
                question,
                why,
                kind,
                options,
                proposed,
                implications,
            } => Question {
                id,
                theme,
                question,
                why,
                kind,
                options,
                proposed,
                implications,
            },
            QuestionIn::Sentence(question) => Question {
                id: String::new(),
                theme: "other".into(),
                question,
                why: String::new(),
                kind: QuestionKind::Text,
                options: Vec::new(),
                proposed: Vec::new(),
                implications: Vec::new(),
            },
        }
    }
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
    /// The squad of the project it belongs to; `""` when there is only one, or for a staff role.
    #[serde(default)]
    pub squad: String,
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

    /// The most turns (model calls) a step of this size takes by default, `[team] max_turns` aside: room for
    /// the work, a stop for a session going round in circles that the budget would only catch later.
    pub fn turns(self) -> u32 {
        match self {
            Size::S => 60,
            Size::M => 120,
            Size::L => 240,
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
    pub questions: Vec<Question>,
    /// A task's: its work changes what users or other programs rely on (the definition of done then asks for the
    /// documentation to follow).
    #[serde(default)]
    pub public_change: bool,
    /// A security review's: each vulnerability it confirmed.
    #[serde(default)]
    pub vulnerabilities: Vec<String>,
}

impl Report {
    /// A report TermF writes itself: a status and a summary.
    pub(crate) fn said(status: ReportStatus, summary: String) -> Report {
        Report {
            status,
            summary,
            covers: Vec::new(),
            text_changes: Vec::new(),
            affected_views: Vec::new(),
            out_of_scope_findings: Vec::new(),
            questions: Vec::new(),
            public_change: false,
            vulnerabilities: Vec::new(),
        }
    }
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

pub(crate) fn strings() -> Value {
    json!({ "type": "array", "items": { "type": "string" } })
}

pub(crate) fn object(properties: Value) -> Value {
    let required: Vec<&String> = properties
        .as_object()
        .map_or(Vec::new(), |p| p.keys().collect());
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

/// A question of the questionnaire, as the `questions` of any step's JSON schema take it.
pub fn question_schema() -> Value {
    object(json!({
        "id": { "type": "string" },
        "theme": { "type": "string", "enum": THEMES },
        "question": { "type": "string" },
        "why": { "type": "string" },
        "kind": { "type": "string", "enum": ["choice", "multi", "bool", "text"] },
        "options": strings(),
        "proposed": {
            "type": "array",
            "items": { "type": "string" },
            "description": "Options word for word; for bool, yes or no (in English, whatever the language of the rest)",
        },
        "implications": {
            "type": "array",
            "items": { "type": "string" },
            "description": "What each option implies, in the order of options (for bool: yes then no), or none",
        },
    }))
}

pub fn questions_schema() -> Value {
    json!({ "type": "array", "items": question_schema() })
}

pub fn plan_schema() -> Value {
    let task = object(json!({
        "id": { "type": "string" },
        "role": { "type": "string" },
        "squad": { "type": "string" },
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
        "questions": questions_schema(),
        "tasks": { "type": "array", "items": task },
    }))
}

pub fn report_schema() -> Value {
    object(report_properties())
}

/// A task's report: whether its work changes the public behaviour, too.
pub fn task_report_schema() -> Value {
    let mut properties = report_properties();
    properties["public_change"] = json!({ "type": "boolean" });
    object(properties)
}

/// A security review's report: the vulnerabilities it confirmed, too.
pub fn security_report_schema() -> Value {
    let mut properties = report_properties();
    properties["vulnerabilities"] = strings();
    object(properties)
}

fn report_properties() -> Value {
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
    json!({
        "status": { "type": "string", "enum": ["done", "blocked", "failed"] },
        "summary": { "type": "string" },
        "covers": strings(),
        "text_changes": { "type": "array", "items": change },
        "affected_views": { "type": "array", "items": view },
        "out_of_scope_findings": strings(),
        "questions": questions_schema(),
    })
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

/// What is wrong with a plan beyond its schema. `roles`: those a task may have; `planners`: the roles that plan
/// a batch of their squad, which only its lead takes; `repos`: those a task may name, the run's own first (the one
/// of a task that names none); `team`: the project's squads, each task's in turn.
pub fn check_plan(
    plan: &Plan,
    roles: &[String],
    planners: &[String],
    repos: &[String],
    team: &Team,
) -> Vec<String> {
    let own = repos.first().map(String::as_str).unwrap_or_default();
    let mut problems = Vec::new();
    if plan.tasks.is_empty() && plan.questions.is_empty() {
        problems.push("the plan has no task and no question".to_owned());
    }
    let criteria: HashSet<&str> = plan.acceptance.iter().map(|c| c.id.as_str()).collect();
    let mut ids = HashSet::new();
    for task in &plan.tasks {
        let id = &task.id;
        if !task_id(id) {
            problems.push(format!(
                "task id {id:?} is not made of letters, digits, - or _, with a dot only between two of them"
            ));
        }
        if !ids.insert(id.as_str()) {
            problems.push(format!("two tasks are named {id}"));
        }
        let batch = planners.iter().any(|r| same_role(r, &task.role));
        if !batch && !roles.iter().any(|r| same_role(r, &task.role)) {
            problems.push(format!(
                "task {id}: role {} is not one a task may have ({})",
                task.role,
                roles.join(", ")
            ));
        }
        let repo = if task.repo.is_empty() {
            own
        } else {
            &task.repo
        };
        if !repos.iter().any(|r| r.eq_ignore_ascii_case(repo)) {
            problems.push(format!(
                "task {id}: repository {repo} is not one of the project's ({})",
                repos.join(", ")
            ));
        }
        match team.place(&task.squad, &task.role) {
            Err(why) => problems.push(format!("task {id}: {why}")),
            Ok(Membership::Staff) if batch => {
                problems.push(format!(
                    "task {id}: a batch of {} names its squad",
                    task.role
                ));
            }
            Ok(Membership::Squad(squad)) if batch && !same_role(&squad.lead, &task.role) => {
                problems.push(format!(
                    "task {id}: a batch goes to the lead of its squad, {} for squad {}",
                    squad.lead, squad.id
                ));
            }
            Ok(Membership::Staff) if !team.staff.iter().any(|r| same_role(r, &task.role)) => {
                problems.push(format!(
                    "task {id}: name its squad ({}), unless its role is on the staff ({})",
                    team.squads
                        .iter()
                        .map(|s| s.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    team.staff.join(", ")
                ));
            }
            Ok(Membership::Staff) => {}
            Ok(Membership::Squad(squad)) => {
                if !batch && !squad.has(&task.role) {
                    problems.push(format!(
                        "task {id}: role {} is not in squad {} ({})",
                        task.role,
                        squad.id,
                        squad.roles.keys().cloned().collect::<Vec<_>>().join(", ")
                    ));
                }
                if !squad.repos.is_empty()
                    && !squad.repos.iter().any(|r| r.eq_ignore_ascii_case(repo))
                {
                    problems.push(format!(
                        "task {id}: squad {} works on {}, not on {repo}",
                        squad.id,
                        squad.repos.join(", ")
                    ));
                }
            }
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
            // `/x` is not absolute on Windows, yet never inside the repository.
            let rooted = path.is_absolute() || file.starts_with(['/', '\\']);
            if rooted || path.components().any(|c| c.as_os_str() == "..") {
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
    problems.extend(check_questions(&plan.questions));
    problems
}

/// Whether `id` names a task: letters, digits, `-` and `_`, and a dot only between two such parts, as a grafted
/// task's `T2.1` (never `.` nor `..`: an id names the task's folder).
pub fn task_id(id: &str) -> bool {
    id.split('.').all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
    })
}

/// What is wrong with a questionnaire beyond its schema.
pub fn check_questions(questions: &[Question]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ids = HashSet::new();
    for q in questions {
        let id = &q.id;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
        {
            problems.push(format!(
                "question id {id:?} is not made of letters, digits, - or _ ({})",
                q.question
            ));
        }
        if !ids.insert(id.as_str()) {
            problems.push(format!("two questions are named {id}"));
        }
        if !THEMES.contains(&q.theme.as_str()) {
            problems.push(format!(
                "question {id}: theme {} is not one of {}",
                q.theme,
                THEMES.join(", ")
            ));
        }
        if q.question.trim().is_empty() || q.why.trim().is_empty() {
            problems.push(format!("question {id} must say what it asks and why"));
        }
        let listed = matches!(q.kind, QuestionKind::Choice | QuestionKind::Multi);
        if listed && q.options.len() < 2 {
            problems.push(format!("question {id} offers fewer than two options"));
        }
        if let Some(wrong) = q.proposed.iter().find(|p| !answerable(q, p)) {
            problems.push(format!(
                "question {id}: {wrong:?} is not an answer it takes"
            ));
        }
        if q.kind != QuestionKind::Multi && q.proposed.len() > 1 {
            problems.push(format!("question {id} proposes more than one answer"));
        }
        // A yes/no question lists no options: its implications are yes's, then no's. A free text's are only read.
        let choices = match q.kind {
            QuestionKind::Bool => Some(2),
            QuestionKind::Text => None,
            _ => Some(q.options.len()),
        };
        if !q.implications.is_empty() && choices.is_some_and(|n| q.implications.len() != n) {
            problems.push(match q.kind {
                QuestionKind::Bool => {
                    format!("question {id}: give two implications (yes, then no), or none")
                }
                _ => format!("question {id}: give one implication per option, or none"),
            });
        }
    }
    problems
}

fn answerable(q: &Question, answer: &str) -> bool {
    match q.kind {
        QuestionKind::Choice | QuestionKind::Multi => q.options.iter().any(|o| o == answer),
        QuestionKind::Bool => matches!(answer, "yes" | "no"),
        QuestionKind::Text => !answer.trim().is_empty(),
    }
}

/// What is wrong with the user's answers to `questions`: each one answered, in the shape its kind takes (a
/// multiple choice may tick nothing). An answer of the user's own may stand in for the options of a choice, so
/// that no question leaves them stuck.
pub fn check_answers(
    questions: &[Question],
    answers: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for id in answers.keys() {
        if !questions.iter().any(|q| &q.id == id) {
            problems.push(format!("there is no question {id}"));
        }
    }
    for q in questions {
        let answer = answers.get(&q.id).map(Vec::as_slice).unwrap_or_default();
        let one = q.kind != QuestionKind::Multi;
        let valid = |a: &String| match q.kind {
            QuestionKind::Bool => answerable(q, a),
            _ => !a.trim().is_empty(),
        };
        if (one && answer.len() != 1) || !answer.iter().all(valid) {
            problems.push(format!("answer question {}: {}", q.id, q.question));
        }
    }
    problems
}

/// The files a task plans outside its squad's paths: not wrong in itself, the `scope` rule decides once the work
/// is done.
pub fn stray_hints(plan: &Plan, team: &Team) -> Vec<String> {
    let mut found = Vec::new();
    for task in &plan.tasks {
        let Ok(Membership::Squad(squad)) = team.place(&task.squad, &task.role) else {
            continue;
        };
        if squad.paths.is_empty() {
            continue;
        }
        for hint in &task.files_hint {
            if !squad.paths.iter().any(|p| inside(hint, p)) {
                found.push(format!(
                    "task {}: {hint} is outside the paths of squad {} ({})",
                    task.id,
                    squad.id,
                    squad.paths.join(", ")
                ));
            }
        }
    }
    found
}

/// What is wrong with the sub-plan `sub` that the lead of `batch`, a task of `plan`, made: a plan of its squad
/// alone (`roles`: those of the squad a task may have; its repositories among `repos`; within its paths), on the
/// batch's criteria and the contracts between squads (`IC…`), each criterion of the batch covered, its ids new to
/// `plan` once grafted.
pub fn check_subplan(
    plan: &Plan,
    batch: &Task,
    sub: &Plan,
    roles: &[String],
    repos: &[String],
    squad: &Squad,
) -> Vec<String> {
    let mut problems = Vec::new();
    let mut sub = sub.clone();
    sub.acceptance = batch_criteria(plan, batch);
    let own = match batch.repo.as_str() {
        "" => repos.first().cloned().unwrap_or_default(),
        repo => repo.to_owned(),
    };
    for task in &mut sub.tasks {
        if task.squad.is_empty() {
            task.squad = squad.id.clone();
        } else if task.squad != squad.id {
            problems.push(format!(
                "task {}: a sub-plan stays in squad {}",
                task.id, squad.id
            ));
            task.squad = squad.id.clone();
        }
        if task.repo.is_empty() {
            task.repo = own.clone();
        }
    }
    let team = Team {
        declared: true,
        squads: vec![squad.clone()],
        staff: Vec::new(),
    };
    let repos: Vec<String> = repos
        .iter()
        .filter(|r| squad.repos.is_empty() || squad.repos.iter().any(|s| s.eq_ignore_ascii_case(r)))
        .cloned()
        .collect();
    problems.extend(check_plan(&sub, roles, &[], &repos, &team));
    problems.extend(stray_hints(&sub, &team));
    for covered in &batch.covers {
        if !sub.questions.is_empty() {
            break;
        }
        if !sub.tasks.iter().any(|t| t.covers.contains(covered)) {
            problems.push(format!(
                "no task covers {covered}, a criterion of batch {}",
                batch.id
            ));
        }
    }
    let mut ids = std::collections::HashSet::new();
    for task in graft(batch, &sub) {
        if plan.tasks.iter().any(|t| t.id == task.id) || !ids.insert(task.id.clone()) {
            problems.push(format!(
                "task {} would be {} once grafted, which another task is",
                task.id.split_once('.').map_or("", |(_, id)| id),
                task.id
            ));
        }
    }
    problems
}

/// The criteria a batch's sub-plan works on: the batch's own, and the contracts between squads (`IC…`).
pub fn batch_criteria(plan: &Plan, batch: &Task) -> Vec<Criterion> {
    plan.acceptance
        .iter()
        .filter(|c| batch.covers.contains(&c.id) || c.id.starts_with("IC"))
        .cloned()
        .collect()
}

/// The tasks of the sub-plan of `batch` as tasks of the run's plan: their ids prefixed with the batch's (`T1` of
/// batch `T2` is `T2.1`), in its squad and repository unless they name theirs, after what the batch waits for.
pub fn graft(batch: &Task, sub: &Plan) -> Vec<Task> {
    let id = |own: &str| {
        let short = own
            .strip_prefix('T')
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(own);
        format!("{}.{short}", batch.id)
    };
    sub.tasks
        .iter()
        .map(|t| Task {
            id: id(&t.id),
            squad: batch.squad.clone(),
            repo: match t.repo.is_empty() {
                true => batch.repo.clone(),
                false => t.repo.clone(),
            },
            depends_on: batch
                .depends_on
                .iter()
                .cloned()
                .chain(t.depends_on.iter().map(|d| id(d)))
                .collect(),
            ..t.clone()
        })
        .collect()
}

/// Whether a file, folder or glob of a plan falls within the squad path `path`.
fn inside(hint: &str, path: &str) -> bool {
    let hint = hint.trim().trim_start_matches("./").trim_end_matches('/');
    let fixed = &hint[..hint.find(['*', '?']).unwrap_or(hint.len())];
    rules::covers(path, hint) || rules::covers(path, &format!("{}/x", fixed.trim_end_matches('/')))
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
    problems.extend(check_questions(&report.questions));
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
            squad: String::new(),
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

    fn small() -> Team {
        termf_core::squads::parse("name = \"Acme\"\n").unwrap()
    }

    fn repos(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn squads() -> Team {
        termf_core::squads::parse(
            r#"[[squads]]
id = "web"
roles = { front = 2, qa = 1 }
repos = ["acme-web"]
paths = ["src/**"]
[[squads]]
id = "api"
lead = "orchestrator"
roles = { back = 1 }
repos = ["acme-api"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn a_task_id_never_names_a_folder_outside_its_own() {
        for good in ["T1", "T2.1", "T2.QA", "back-1_b"] {
            assert!(task_id(good), "{good}");
        }
        for bad in ["", ".", "..", ".T1", "T1.", "T1..2", "a/b", "../x", "T 1"] {
            assert!(!task_id(bad), "{bad}");
        }
        let all = check_plan(
            &plan(vec![task("..", &[]), task(".", &[])]),
            &repos(&["back"]),
            &[],
            &repos(&["api"]),
            &small(),
        )
        .join("\n");
        assert!(
            all.contains("task id \"..\" is not made of letters, digits, - or _, with a dot only between two of them")
                && all.contains("task id \".\""),
            "{all}"
        );
    }

    #[test]
    fn a_batch_goes_to_the_lead_of_its_squad() {
        let team = squads();
        let roles = repos(&["front", "back", "qa"]);
        let planners = repos(&["lead"]);
        let all = repos(&["acme-web", "acme-api"]);
        let mut web = task("T1", &[]);
        (web.role, web.squad, web.repo) = ("lead".into(), "web".into(), "acme-web".into());
        let mut api = task("T2", &["T1"]);
        (api.role, api.squad, api.repo) = ("lead".into(), "api".into(), "acme-api".into());
        let good = plan(vec![web.clone(), api]);
        assert_eq!(
            check_plan(&good, &roles, &planners, &all, &team),
            Vec::<String>::new()
        );
        assert!(
            check_plan(&good, &roles, &[], &all, &team)
                .join("\n")
                .contains("role lead is not one a task may have"),
            "no batch where no role plans one"
        );
        let mut staff = web.clone();
        staff.squad = String::new();
        let mut other = web;
        other.id = "T3".into();
        other.repo = "acme-api".into();
        let all = check_plan(&plan(vec![staff, other]), &roles, &planners, &all, &team).join("\n");
        for expected in [
            "task T1: a batch of lead names its squad",
            "task T3: squad web works on acme-web, not on acme-api",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }
    }

    #[test]
    fn a_sub_plan_stays_in_its_squad_and_is_grafted_with_its_batch_prefix() {
        let team = squads();
        let web = team.squads[0].clone();
        let mut top = plan(vec![]);
        top.acceptance.push(Criterion {
            id: "AC2".into(),
            text: "a button".into(),
        });
        top.acceptance.push(Criterion {
            id: "IC1".into(),
            text: "the API answers CSV".into(),
        });
        let mut batch = task("T2", &["T1"]);
        (batch.role, batch.squad, batch.repo) = ("lead".into(), "web".into(), "acme-web".into());
        batch.covers = vec!["AC2".into(), "IC1".into()];
        let mut t1 = task("T1", &[]);
        t1.id = "T0".into();
        top.tasks = vec![t1, batch.clone()];
        let roles = repos(&["front", "qa"]);
        let all = repos(&["acme-web", "acme-api"]);

        let mut front = task("T1", &[]);
        (front.role, front.repo) = ("front".into(), String::new());
        front.covers = vec!["AC2".into(), "IC1".into()];
        front.files_hint = vec!["src/List.svelte".into()];
        let mut qa = task("T2", &["T1"]);
        (qa.role, qa.repo) = ("qa".into(), String::new());
        qa.covers = vec!["AC2".into()];
        qa.files_hint = vec!["src/List.test.ts".into()];
        let sub = Plan {
            acceptance: vec![],
            tasks: vec![front.clone(), qa],
            ..plan(vec![])
        };
        assert_eq!(
            check_subplan(&top, &batch, &sub, &roles, &all, &web),
            Vec::<String>::new()
        );
        let grafted = graft(&batch, &sub);
        let ids: Vec<(&str, &[String], &str)> = grafted
            .iter()
            .map(|t| (t.id.as_str(), t.depends_on.as_slice(), t.repo.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                ("T2.1", &["T1".to_owned()][..], "acme-web"),
                (
                    "T2.2",
                    &["T1".to_owned(), "T2.1".to_owned()][..],
                    "acme-web"
                ),
            ]
        );
        assert!(grafted.iter().all(|t| t.squad == "web"));

        let mut stray = front.clone();
        stray.files_hint = vec!["infra/main.tf".into()];
        stray.covers = vec!["AC1".into()];
        let mut back = front;
        (back.id, back.role, back.repo) = ("T3".into(), "back".into(), "acme-api".into());
        back.covers = vec!["IC1".into()];
        back.squad = "api".into();
        let wrong = Plan {
            tasks: vec![stray, back],
            ..sub.clone()
        };
        let all = check_subplan(&top, &batch, &wrong, &roles, &all, &web).join("\n");
        for expected in [
            "task T1 covers AC1, which is not a criterion",
            "task T1: infra/main.tf is outside the paths of squad web",
            "task T3: a sub-plan stays in squad web",
            "task T3: role back is not in squad web",
            "task T3: repository acme-api is not one of the project's (acme-web)",
            "no task covers AC2, a criterion of batch T2",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }
    }

    #[test]
    fn checks_what_the_schema_cannot() {
        let roles = ["back".to_owned(), "front".to_owned()];
        let small = small();
        assert!(
            check_plan(
                &plan(vec![task("T1", &[]), task("T2", &["T1"])]),
                &roles,
                &[],
                &repos(&["API"]),
                &small
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
            &[],
            &repos(&["api"]),
            &small,
        );
        let all = problems.join("\n");
        for expected in [
            "two tasks are named T1",
            "role wizard is not one a task may have (back, front)",
            "repository front is not one of the project's (api)",
            "covers AC7",
            "../etc/passwd is not a path",
            "/abs is not a path",
            "depends on T9",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }

        let looped = plan(vec![task("T1", &["T2"]), task("T2", &["T1"])]);
        assert_eq!(
            check_plan(&looped, &roles, &[], &repos(&["api"]), &small),
            ["the tasks depend on each other in a cycle"]
        );
        assert_eq!(
            check_plan(&plan(vec![]), &roles, &[], &repos(&["api"]), &small).len(),
            1
        );
        let mut main = task("T1", &[]);
        main.squad = "main".into();
        main.role = "orchestrator".into();
        let lead = ["lead".to_owned()];
        assert!(
            check_plan(&plan(vec![main]), &lead, &[], &repos(&["api"]), &small).is_empty(),
            "the implicit squad, and the former name of lead"
        );
    }

    #[test]
    fn checks_each_task_against_its_squad() {
        let team = termf_core::squads::parse(
            r#"name = "Acme"
[team]
staff = ["architect"]
[[squads]]
id = "web"
roles = { front = 2, qa = 1 }
repos = ["front"]
paths = ["apps/web/**", "packages/ui"]
[[squads]]
id = "api"
roles = { back = 1 }
repos = ["api"]
paths = ["services/**"]
"#,
        )
        .unwrap();
        let roles: Vec<String> = ["front", "back", "qa", "architect"]
            .map(String::from)
            .to_vec();
        let mut t1 = task("T1", &[]);
        t1.squad = "api".into();
        t1.files_hint = vec!["services/billing".into(), "README.md".into()];
        let mut t2 = task("T2", &["T1"]);
        t2.role = "architect".into();
        let good = plan(vec![t1.clone(), t2]);
        assert!(check_plan(&good, &roles, &[], &repos(&["api", "front"]), &team).is_empty());
        assert_eq!(
            stray_hints(&good, &team),
            ["task T1: README.md is outside the paths of squad api (services/**)"]
        );

        let mut outside = task("T3", &[]);
        outside.squad = "web".into();
        let mut nameless = task("T4", &[]);
        nameless.role = "back".into();
        let mut unknown = task("T5", &[]);
        unknown.squad = "ops".into();
        let problems = check_plan(
            &plan(vec![outside, nameless, unknown]),
            &roles,
            &[],
            &repos(&["api"]),
            &team,
        );
        let all = problems.join("\n");
        for expected in [
            "task T3: role back is not in squad web (front, qa)",
            "task T3: squad web works on front, not on api",
            "task T4: name its squad (web, api), unless its role is on the staff (architect)",
            "task T5: no squad ops in the project (its squads: web, api)",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }

        let mut web = task("T6", &[]);
        web.squad = "web".into();
        web.role = "front".into();
        web.repo = String::new();
        web.files_hint = vec!["packages/ui/button.ts".into(), "apps/web".into()];
        let web = plan(vec![web]);
        assert!(stray_hints(&web, &team).is_empty());
        assert_eq!(
            check_plan(&web, &roles, &[], &repos(&["front", "api"]), &team),
            Vec::<String>::new(),
            "no repository: the run's"
        );
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

    fn question(id: &str, kind: QuestionKind, options: &[&str], proposed: &[&str]) -> Question {
        Question {
            id: id.into(),
            theme: "product".into(),
            question: format!("{id}?"),
            why: "it changes the export".into(),
            kind,
            options: options.iter().map(|o| o.to_string()).collect(),
            proposed: proposed.iter().map(|o| o.to_string()).collect(),
            implications: Vec::new(),
        }
    }

    #[test]
    fn checks_a_questionnaire_and_its_answers() {
        let format = question("format", QuestionKind::Choice, &["CSV", "Excel"], &["CSV"]);
        let columns = question(
            "columns",
            QuestionKind::Multi,
            &["date", "amount", "client"],
            &["date", "amount"],
        );
        let public = question("public", QuestionKind::Bool, &[], &["no"]);
        let name = question("name", QuestionKind::Text, &[], &[]);
        // What the real Claude Code writes for a yes/no question: what yes implies, then no.
        let mut weighed = question("weighed", QuestionKind::Bool, &[], &["yes"]);
        weighed.implications = vec!["a public link".into(), "signed-in users only".into()];
        // And for a free text: what its proposal and another answer would imply.
        let mut titled = question("titled", QuestionKind::Text, &[], &["Multiply"]);
        titled.implications = vec!["matches the page".into(), "the user writes it".into()];
        assert_eq!(check_questions(&[weighed, titled]), Vec::<String>::new());
        let good = [
            format.clone(),
            columns.clone(),
            public.clone(),
            name.clone(),
        ];
        assert_eq!(check_questions(&good), Vec::<String>::new());
        let schema = question_schema().to_string();
        assert!(
            schema.contains("yes or no") && schema.contains("yes then no"),
            "{schema}"
        );

        let mut odd = question("format", QuestionKind::Choice, &["CSV"], &["PDF"]);
        odd.theme = "mood".into();
        odd.why = " ".into();
        odd.implications = vec!["a".into(), "b".into()];
        let mut two = question("x y", QuestionKind::Bool, &[], &["maybe", "no"]);
        two.question = String::new();
        two.implications = vec!["only one".into()];
        let all = check_questions(&[format.clone(), odd, two]).join("\n");
        for expected in [
            "two questions are named format",
            "theme mood is not one of",
            "question format must say what it asks and why",
            "question format offers fewer than two options",
            "question format: \"PDF\" is not an answer it takes",
            "one implication per option",
            "question x y: give two implications (yes, then no), or none",
            "question id \"x y\" is not made of",
            "question x y: \"maybe\" is not an answer it takes",
            "question x y proposes more than one answer",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }

        let answers = |pairs: &[(&str, &[&str])]| -> BTreeMap<String, Vec<String>> {
            pairs
                .iter()
                .map(|(id, a)| (id.to_string(), a.iter().map(|s| s.to_string()).collect()))
                .collect()
        };
        let full = answers(&[
            ("format", &["Excel"]),
            ("columns", &[]),
            ("public", &["yes"]),
            ("name", &["invoices.csv"]),
        ]);
        assert_eq!(check_answers(&good, &full), Vec::<String>::new());
        let own = answers(&[
            ("format", &["PDF, as the bank asks"]),
            ("columns", &["date", "tax code"]),
            ("public", &["no"]),
            ("name", &["invoices.csv"]),
        ]);
        assert_eq!(
            check_answers(&good, &own),
            Vec::<String>::new(),
            "an answer of one's own for a choice"
        );
        let wrong = answers(&[
            ("format", &[]),
            ("columns", &["date", " "]),
            ("public", &["maybe"]),
            ("ghost", &["x"]),
        ]);
        let all = check_answers(&good, &wrong).join("\n");
        for expected in [
            "there is no question ghost",
            "answer question format",
            "answer question columns",
            "answer question public",
            "answer question name",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }

        let old: Plan =
            serde_json::from_value(json!({"summary": "s", "acceptance": [], "out_of_scope": [],
            "questions": ["CSV or Excel?"], "tasks": []}))
            .unwrap();
        assert_eq!(
            old.questions[0].question, "CSV or Excel?",
            "a plan stored before the questionnaire"
        );
        assert!(
            !check_questions(&old.questions).is_empty(),
            "never valid as a new answer"
        );
        let back: Question =
            serde_json::from_value(serde_json::to_value(&columns).unwrap()).unwrap();
        assert_eq!(back, columns);
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
            10
        );
        assert_eq!(
            report_schema()["properties"]["status"]["enum"][1],
            "blocked"
        );
    }
}
