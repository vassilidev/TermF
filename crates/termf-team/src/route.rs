//! The CTO's triage: every run starts with it, unless `[team] triage = false` in the project's file or in
//! `termf.toml`. The CTO reads the request and the project and answers with a routing, which Claude Code holds
//! to a JSON schema and TermF checks again: the workflow, the optional steps it runs or skips, the efforts, the
//! squads, the questions, the advised budget.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use termf_core::squads::Team;

use crate::plan::{Question, Size, object, questions_schema, strings};
use crate::workflow::{StepKind, Workflow};

/// What a request may be.
pub const KINDS: [&str; 20] = [
    "urgent-bug",
    "bug",
    "feature",
    "epic",
    "refactor",
    "infra",
    "seo",
    "docs",
    "brand",
    "copy",
    "motion",
    "database",
    "integration",
    "reliability",
    "accessibility",
    "ai",
    "analytics",
    "legal",
    "research",
    "other",
];
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Routing {
    /// One of [`KINDS`].
    pub kind: String,
    pub size: Size,
    /// Why this path, in one sentence.
    pub reason: String,
    pub workflow: String,
    /// The workflow's optional steps left out, and those turned on.
    pub skip: Vec<String>,
    pub add: Vec<String>,
    pub efforts: Vec<Effort>,
    /// The squads the work falls in: one, its lead plans; several, the CTO plans their batches.
    pub squads: Vec<String>,
    /// While there are any, nothing is routed: the user answers first.
    pub questions: Vec<Question>,
    /// The most the run should cost, in US dollars; 0: no advice.
    pub budget_usd: f64,
    /// The CTO's decision journal: `Ruling: what — why — cost if wrong`, one a decision.
    #[serde(default)]
    pub rulings: Vec<String>,
    /// Decisions of the project the CTO goes back on, each with the ruling that takes its place.
    #[serde(default)]
    pub replaces: Vec<Replace>,
    /// The user changed the CTO's routing, or set it.
    #[serde(default)]
    pub changed: bool,
    /// The direct workflow's one task, which the CTO gives instead of a lead's plan.
    #[serde(default)]
    pub task: Option<crate::plan::Task>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Replace {
    /// `D<id>`.
    pub decision: String,
    pub ruling: String,
}

impl Replace {
    /// The id of the decision it names.
    pub fn id(&self) -> Option<i64> {
        self.decision.trim().strip_prefix('D')?.parse().ok()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Effort {
    pub step: String,
    pub effort: String,
}

/// A workflow as the triage, and the user changing a routing, see it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct WorkflowView {
    pub name: String,
    pub description: String,
    /// In the order they run, the optional ones included.
    pub steps: Vec<StepView>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct StepView {
    pub id: String,
    pub kind: StepKind,
    pub role: Option<String>,
    pub optional: bool,
    /// Whether it runs unless the routing says otherwise.
    pub on: bool,
}

pub fn schema() -> Value {
    let effort = object(json!({
        "step": { "type": "string" },
        "effort": { "type": "string", "enum": EFFORTS },
    }));
    object(json!({
        "kind": { "type": "string", "enum": KINDS },
        "size": { "type": "string", "enum": ["S", "M", "L"] },
        "reason": { "type": "string" },
        "workflow": { "type": "string" },
        "skip": strings(),
        "add": strings(),
        "efforts": { "type": "array", "items": effort },
        "squads": strings(),
        "questions": questions_schema(),
        "budget_usd": { "type": "number" },
        "rulings": strings(),
        "task": { "anyOf": [crate::plan::task_schema(), { "type": "null" }] },
        "replaces": {
            "type": "array",
            "items": object(json!({ "decision": { "type": "string" }, "ruling": { "type": "string" } })),
        },
    }))
}

/// What is wrong with the decisions a routing replaces: each must be one of `in_force`, once, with a ruling.
pub fn check_replaces(routing: &Routing, in_force: &[i64]) -> Vec<String> {
    let mut problems = Vec::new();
    for (n, r) in routing.replaces.iter().enumerate() {
        match r.id() {
            Some(id) if in_force.contains(&id) => {
                if routing.replaces[..n].iter().any(|o| o.id() == Some(id)) {
                    problems.push(format!("decision {} is replaced twice", r.decision));
                }
            }
            _ => problems.push(format!(
                "{} is not a decision of the project in force",
                r.decision
            )),
        }
        if r.ruling.trim().is_empty() {
            problems.push(format!("say what replaces {}", r.decision));
        }
    }
    problems
}

impl WorkflowView {
    pub fn of(workflow: &Workflow) -> WorkflowView {
        let mut every = workflow.clone();
        for step in every.steps.values_mut().filter(|s| s.optional) {
            step.enabled = true;
        }
        // A workflow whose optional steps cannot all run at once shows what it runs by default.
        let order = every
            .steps()
            .or_else(|_| workflow.steps())
            .unwrap_or_default();
        WorkflowView {
            name: workflow.name.clone(),
            description: workflow.description.clone(),
            steps: order
                .into_iter()
                .map(|(id, def)| StepView {
                    on: workflow.steps[&id].enabled,
                    id,
                    kind: def.kind,
                    role: def.role,
                    optional: def.optional,
                })
                .collect(),
        }
    }

    /// One line for the CTO: each step, its role, and whether it is optional.
    pub fn line(&self) -> String {
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|s| {
                let kind = serde_json::to_value(s.kind)
                    .ok()
                    .and_then(|k| k.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let role = s
                    .role
                    .as_deref()
                    .map(|r| format!(" {r}"))
                    .unwrap_or_default();
                let optional = match (s.optional, s.on) {
                    (false, _) => "",
                    (true, true) => ", optional: on",
                    (true, false) => ", optional: off",
                };
                format!("{} ({kind}{role}{optional})", s.id)
            })
            .collect();
        format!(
            "- {}: {}\n  steps: {}",
            self.name,
            self.description,
            steps.join(", ")
        )
    }
}

/// What is wrong with a routing beyond its schema. `workflows`: those the run may take, as the layers make them;
/// `team`: the project's squads; `role`: whether the library has a role a session may play.
pub fn check(
    routing: &Routing,
    workflows: &BTreeMap<String, Workflow>,
    team: &Team,
    role: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut problems = Vec::new();
    if !KINDS.contains(&routing.kind.as_str()) {
        problems.push(format!(
            "kind {} is not one of {}",
            routing.kind,
            KINDS.join(", ")
        ));
    }
    if routing.reason.trim().is_empty() {
        problems.push("the routing gives no reason".to_owned());
    }
    if !routing.budget_usd.is_finite() || routing.budget_usd < 0.0 {
        problems.push("budget_usd must be 0 or more".to_owned());
    }
    match workflows.get(&routing.workflow) {
        None => problems.push(format!(
            "workflow {} is not one of the project's ({})",
            routing.workflow,
            workflows.keys().cloned().collect::<Vec<_>>().join(", ")
        )),
        Some(workflow) => check_steps(routing, workflow, &role, &mut problems),
    }
    let ids: Vec<&str> = team.squads.iter().map(|s| s.id.as_str()).collect();
    for (n, id) in routing.squads.iter().enumerate() {
        if !ids.contains(&id.as_str()) {
            problems.push(format!(
                "no squad {id} in the project (its squads: {})",
                ids.join(", ")
            ));
        } else if routing.squads[..n].contains(id) {
            problems.push(format!("squad {id} is named twice"));
        }
    }
    problems
}

fn check_steps(
    routing: &Routing,
    workflow: &Workflow,
    role: &impl Fn(&str) -> bool,
    problems: &mut Vec<String>,
) {
    let name = &workflow.name;
    for id in routing.skip.iter().chain(&routing.add) {
        match workflow.steps.get(id) {
            Some(step) if step.optional => {}
            _ => problems.push(format!(
                "step {id} is not an optional step of workflow {name}"
            )),
        }
    }
    for id in routing.skip.iter().filter(|id| routing.add.contains(id)) {
        problems.push(format!("step {id} is both skipped and added"));
    }
    // TermF runs what `skip` says: a ruling that skips a step `skip` keeps would run a step the journal says it left out.
    for ruling in &routing.rulings {
        let lower = ruling.trim().to_lowercase();
        let what = lower.trim_start_matches("ruling:").trim();
        // Whatever dash or colon ends the decision: the why after it names steps too.
        let what = what.split(['—', '–', ':', ';']).next().unwrap_or(what);
        let what = what.split(" - ").next().unwrap_or(what);
        let Some(named) = what.strip_prefix("skip ") else {
            continue;
        };
        let words: Vec<&str> = named
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .collect();
        for (id, step) in &workflow.steps {
            if step.optional
                && step.enabled
                && !routing.skip.contains(id)
                && words.contains(&id.as_str())
            {
                problems.push(format!(
                    "a ruling skips step {id}, but skip does not list it: put it in skip, or rule to keep it"
                ));
            }
        }
    }
    for id in &routing.add {
        if let Some(needed) = workflow.steps.get(id).and_then(|s| s.role.as_deref())
            && !role(needed)
        {
            problems.push(format!(
                "step {id} needs the role {needed}, which the library does not have"
            ));
        }
    }
    if crate::DIRECT.contains(&name.as_str()) && routing.task.is_none() {
        problems.push(format!(
            "workflow {name} needs its one task in task: the role, the goal, the files, the criteria it covers"
        ));
    }
    let mut routed = workflow.clone();
    if routed.route(&routing.skip, &routing.add).is_err() {
        return;
    }
    let steps = match routed.steps() {
        Ok(steps) => steps,
        Err(e) => return problems.push(format!("{e:#}")),
    };
    for effort in &routing.efforts {
        if !EFFORTS.contains(&effort.effort.as_str()) {
            problems.push(format!(
                "effort {} of step {} is not one of {}",
                effort.effort,
                effort.step,
                EFFORTS.join(", ")
            ));
        }
        if !steps.iter().any(|(id, s)| {
            *id == effort.step
                && matches!(
                    s.kind,
                    StepKind::Agent | StepKind::Review | StepKind::Update
                )
        }) {
            problems.push(format!(
                "efforts: {} is not an agent step that runs in workflow {name}",
                effort.step
            ));
        }
    }
}

/// Leaves out what changes nothing: a step skipped that is off anyway, one added that runs anyway.
pub fn normalize(routing: &mut Routing, workflow: &Workflow) {
    let enabled = |id: &String| workflow.steps.get(id).is_some_and(|s| s.enabled);
    routing.skip.retain(enabled);
    routing.add.retain(|id| !enabled(id));
}

/// `[team] <key>` of a TOML file, when it says.
pub fn flag(file: &Path, key: &str) -> Result<Option<bool>> {
    let Ok(text) = fs::read_to_string(file) else {
        return Ok(None);
    };
    let table: toml::Table =
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
    let Some(value) = table.get("team").and_then(|t| t.get(key)) else {
        return Ok(None);
    };
    value
        .as_bool()
        .map(Some)
        .with_context(|| format!("`[team] {key}` in {} must be true or false", file.display()))
}

/// Words of a ticket that touch what a single developer never takes without the security review: authentication,
/// permissions, secrets, cryptography, data migrations, payments, concurrency. A trailing `*` matches the start of a
/// word, a space a phrase.
const SENSITIVE: [&str; 61] = [
    "auth",
    "authent*",
    "authori*",
    "authn",
    "authz",
    "oauth*",
    "login*",
    "logout",
    "signin",
    "sign in",
    "signup",
    "sign up",
    "password*",
    "passwd",
    "passphrase*",
    "mot de passe",
    "mots de passe",
    "2fa",
    "mfa",
    "otp",
    "totp",
    "sso",
    "saml",
    "jwt*",
    "csrf",
    "permission*",
    "privilege*",
    "rbac",
    "acl*",
    "role",
    "roles",
    "rôle*",
    "droit",
    "droits",
    "access control",
    "secret*",
    "credential*",
    "api key*",
    "apikey*",
    "token*",
    "private key*",
    "crypt*",
    "encrypt*",
    "decrypt*",
    "chiffr*",
    "hmac",
    "migration*",
    "migrate*",
    "backfill*",
    "payment*",
    "paiement*",
    "billing",
    "factur*",
    "checkout",
    "stripe",
    "refund*",
    "rembours*",
    "concurren*",
    "race condition*",
    "deadlock*",
    "same time",
];

/// The first word of [`SENSITIVE`] that `text` says, if any.
pub fn sensitive(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let spaced = format!(" {} ", words.join(" "));
    SENSITIVE.into_iter().find(|term| {
        let (stem, prefix) = match term.strip_suffix('*') {
            Some(stem) => (stem, true),
            None => (*term, false),
        };
        match (stem.contains(' '), prefix) {
            (true, true) => spaced.contains(&format!(" {stem}")),
            (true, false) => spaced.contains(&format!(" {stem} ")),
            (false, true) => words.iter().any(|w| w.starts_with(stem)),
            (false, false) => words.contains(&stem),
        }
    })
}

/// A short account of a routing, for the events and the CLI.
pub fn describe(routing: &Routing) -> String {
    let mut parts = vec![format!(
        "{}, size {:?}, workflow {}",
        routing.kind, routing.size, routing.workflow
    )];
    for (label, items) in [
        ("skips", &routing.skip),
        ("adds", &routing.add),
        ("squads", &routing.squads),
    ] {
        if !items.is_empty() {
            parts.push(format!("{label} {}", items.join(", ")));
        }
    }
    if !routing.efforts.is_empty() {
        let efforts: Vec<String> = routing
            .efforts
            .iter()
            .map(|e| format!("{} {}", e.step, e.effort))
            .collect();
        parts.push(format!("efforts {}", efforts.join(", ")));
    }
    if routing.budget_usd > 0.0 {
        parts.push(format!("budget ${:.2}", routing.budget_usd));
    }
    format!("{}: {}", parts.join("; "), routing.reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routing() -> Routing {
        Routing {
            kind: "feature".into(),
            size: Size::M,
            reason: "a new export".into(),
            workflow: "default".into(),
            skip: vec![],
            add: vec!["ux".into()],
            efforts: vec![Effort {
                step: "security".into(),
                effort: "high".into(),
            }],
            squads: vec!["web".into()],
            questions: vec![],
            budget_usd: 4.0,
            rulings: vec![],
            replaces: vec![],
            changed: false,
            task: None,
        }
    }

    #[test]
    fn checks_the_workflow_its_steps_and_the_squads() {
        let dir = std::env::temp_dir().join(format!("termf-team-route-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut workflows = BTreeMap::new();
        for name in ["default", "hotfix"] {
            workflows.insert(
                name.to_owned(),
                crate::workflow::load(&dir, None, None, name).unwrap(),
            );
        }
        let team = termf_core::squads::parse(
            "name = \"Acme\"\n[[squads]]\nid = \"web\"\n[[squads]]\nid = \"api\"\n",
        )
        .unwrap();
        let any = |_: &str| true;
        assert_eq!(
            check(&routing(), &workflows, &team, any),
            Vec::<String>::new()
        );

        let bad = Routing {
            kind: "wish".into(),
            reason: " ".into(),
            workflow: "spike".into(),
            squads: vec!["ops".into()],
            budget_usd: -1.0,
            ..routing()
        };
        let all = check(&bad, &workflows, &team, any).join("\n");
        for expected in [
            "kind wish is not one of",
            "no reason",
            "budget_usd must be 0 or more",
            "workflow spike is not one of the project's (default, hotfix)",
            "no squad ops in the project (its squads: web, api)",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }

        let steps = Routing {
            skip: vec!["plan".into(), "ux".into()],
            add: vec!["seo".into(), "ux".into()],
            efforts: vec![
                Effort {
                    step: "frame".into(),
                    effort: "huge".into(),
                },
                Effort {
                    step: "checks".into(),
                    effort: "low".into(),
                },
            ],
            squads: vec!["web".into(), "web".into()],
            ..routing()
        };
        let no_seo = |r: &str| r != "seo";
        let all = check(&steps, &workflows, &team, no_seo).join("\n");
        for expected in [
            "step plan is not an optional step of workflow default",
            "step ux is both skipped and added",
            "step seo needs the role seo, which the library does not have",
            "squad web is named twice",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }
        let ruled = |skip: &[&str], ruling: &str| Routing {
            skip: skip.iter().map(|s| s.to_string()).collect(),
            rulings: vec![ruling.into()],
            ..routing()
        };
        let skips_frame = "Ruling: skip frame (po) — the ticket lists its criteria — cost if wrong: a missed edge case";
        for ruling in [
            skips_frame,
            "Ruling: skip frame - the ticket lists its criteria - cost if wrong: a missed edge case",
            "ruling: Skip frame – the ticket lists its criteria",
            "Ruling: skip frame: the ticket lists its criteria",
        ] {
            assert_eq!(
                check(&ruled(&[], ruling), &workflows, &team, any),
                [
                    "a ruling skips step frame, but skip does not list it: put it in skip, or rule to keep it"
                ],
                "{ruling}"
            );
        }
        for (skip, ruling) in [
            (&["frame"][..], skips_frame),
            (
                &[],
                "Ruling: keep frame (po) at low effort — confirms the criteria — cost if wrong: cents",
            ),
            (
                &[],
                "Ruling: skip prd, architecture, seo — off anyway — cost if wrong: none",
            ),
            (
                &[],
                "Ruling: budget $1 — skip nothing — cost if wrong: a top-up",
            ),
            (
                &[],
                "Ruling: keep everything - skip frame would lose the criteria",
            ),
        ] {
            assert!(
                check(&ruled(skip, ruling), &workflows, &team, any).is_empty(),
                "{ruling}"
            );
        }

        let efforts = Routing {
            skip: vec!["frame".into()],
            efforts: vec![
                Effort {
                    step: "frame".into(),
                    effort: "high".into(),
                },
                Effort {
                    step: "checks".into(),
                    effort: "huge".into(),
                },
                Effort {
                    step: "review".into(),
                    effort: "xhigh".into(),
                },
                Effort {
                    step: "update-base".into(),
                    effort: "xhigh".into(),
                },
            ],
            ..routing()
        };
        let all = check(&efforts, &workflows, &team, any).join("\n");
        assert!(
            !all.contains("review") && !all.contains("update-base"),
            "the CTO sets the review's and the integrator's efforts too: {all}"
        );
        for expected in [
            "efforts: frame is not an agent step that runs in workflow default",
            "effort huge of step checks is not one of",
            "efforts: checks is not an agent step",
        ] {
            assert!(all.contains(expected), "{expected} in {all}");
        }
        assert!(schema()["required"].as_array().unwrap().len() == 13);
    }

    #[test]
    fn shows_every_step_and_reads_the_team_flags() {
        let dir = std::env::temp_dir().join(format!("termf-team-flags-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let hotfix = WorkflowView::of(&crate::workflow::load(&dir, None, None, "hotfix").unwrap());
        let line = hotfix.line();
        assert!(
            line.starts_with("- hotfix: Urgent fix: no plan gate\n  steps: prd (agent pm, optional: off), frame (agent po, optional: on),"),
            "{line}"
        );
        assert!(!line.contains("approve-plan"), "disabled, not optional");
        assert!(line.contains("seo (agent seo, optional: off)"), "{line}");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("termf.toml");
        assert_eq!(flag(&file, "triage").unwrap(), None);
        fs::write(&file, "[team]\ntriage = false\n").unwrap();
        assert_eq!(flag(&file, "triage").unwrap(), Some(false));
        assert_eq!(flag(&file, "approve_route").unwrap(), None);
        fs::write(&file, "[team]\ntriage = \"no\"\n").unwrap();
        assert!(flag(&file, "triage").is_err());
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            describe(&routing()),
            "feature, size M, workflow default; adds ux; squads web; efforts security high; budget $4.00: a new export"
        );
    }

    #[test]
    fn a_sensitive_ticket_is_told_by_its_words() {
        for (text, word) in [
            ("Reset the PASSWORD of a user", "password*"),
            (
                "Only admins with the right permissions can share",
                "permission*",
            ),
            ("Partage d'une liste avec des rôles", "rôle*"),
            ("Store the API key in the vault", "api key*"),
            (
                "Edits are lost when two devices save at the same time",
                "same time",
            ),
            ("Add a migration for the version column", "migration*"),
            ("Le paiement échoue", "paiement*"),
            ("Fix the OAuth callback", "oauth*"),
        ] {
            assert_eq!(sensitive(text), Some(word), "{text}");
        }
        for text in [
            "Rename the Save button",
            "The author's name is cut on the card",
            "Fix a typo in the README, keep the same timeline",
            "Export invoices to CSV",
        ] {
            assert_eq!(sensitive(text), None, "{text}");
        }
    }
}
