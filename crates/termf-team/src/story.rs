//! What a run's pull request tells of the team's work: the plan, the criteria covered, what each step did, the checks
//! TermF ran, the visual review. Agents wrote most of it, from an untrusted ticket: it is made inert first.

use crate::{StepKind, StepState, TRIAGE, TeamRun};

/// What GitHub and GitLab read in a pull request's text: each one closes an issue it names once merged.
const CLOSING: [&str; 16] = [
    "close",
    "closes",
    "closed",
    "closing",
    "fix",
    "fixes",
    "fixed",
    "fixing",
    "resolve",
    "resolves",
    "resolved",
    "resolving",
    "implement",
    "implements",
    "implemented",
    "implementing",
];
const ZWSP: char = '\u{200b}';
/// A step's summary in the list, at most.
const SUMMARY_CHARS: usize = 600;

/// The story of `run` for its pull request, in Markdown; `commands(i)`: the commands of its `shell` step `i`;
/// `visual`: a line on its visual review. `None` when the team has nothing to tell.
pub fn story(
    run: &TeamRun,
    commands: &dyn Fn(usize) -> Vec<String>,
    visual: Option<&str>,
) -> Option<String> {
    let mut parts = Vec::new();
    let reported = |s: &&crate::Step| s.state == StepState::Done && s.report.is_some();
    if let Some(plan) = &run.plan {
        if !plan.summary.trim().is_empty() {
            parts.push(format!("## Summary\n\n{}", defuse(plan.summary.trim())));
        }
        if !plan.acceptance.is_empty() {
            let covered: Vec<&String> = run
                .steps
                .iter()
                .filter(reported)
                .flat_map(|s| &s.report.as_ref().unwrap().covers)
                .collect();
            let lines: Vec<String> = plan
                .acceptance
                .iter()
                .map(|c| {
                    let mark = if covered.contains(&&c.id) { "x" } else { " " };
                    format!(
                        "- [{mark}] {}: {}",
                        defuse(&c.id),
                        defuse(&one_line(&c.text))
                    )
                })
                .collect();
            parts.push(format!("## Acceptance criteria\n\n{}", lines.join("\n")));
        }
    }
    // The tasks; a workflow without any: its agents' steps.
    let tasks = run.steps.iter().any(|s| s.kind == StepKind::Task);
    let work: Vec<String> = run
        .steps
        .iter()
        .filter(reported)
        .filter(|s| match tasks {
            true => s.kind == StepKind::Task,
            false => s.kind == StepKind::Agent && s.key != TRIAGE && s.escalates.is_none(),
        })
        .map(|s| {
            let summary = one_line(&s.report.as_ref().unwrap().summary);
            let summary = match summary.char_indices().nth(SUMMARY_CHARS) {
                Some((end, _)) => format!("{}…", &summary[..end]),
                None => summary,
            };
            let who = s.task.as_deref().unwrap_or(&s.key);
            format!(
                "- {} ({}): {}",
                defuse(who),
                defuse(s.role.as_deref().unwrap_or_default()),
                defuse(&summary)
            )
        })
        .collect();
    if !work.is_empty() {
        parts.push(format!("## What the team did\n\n{}", work.join("\n")));
    }
    let checks: Vec<String> = run
        .steps
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == StepKind::Shell)
        .flat_map(|(i, s)| {
            let result = match s.state {
                StepState::Done if s.accepted => "failed, accepted as it is by the user".to_owned(),
                StepState::Done => "passed".to_owned(),
                StepState::Skipped => format!(
                    "skipped: {}",
                    s.note.as_deref().map(one_line).unwrap_or_default()
                ),
                state => format!("{state:?}").to_lowercase(),
            };
            match commands(i) {
                list if list.is_empty() => vec![format!("- {}: {result}", s.key)],
                list => list
                    .into_iter()
                    .map(|c| format!("- `{}`: {result}", one_line(&c)))
                    .collect(),
            }
        })
        .collect();
    if !checks.is_empty() {
        parts.push(format!("## Checks\n\n{}", checks.join("\n")));
    }
    if let Some(visual) = visual {
        parts.push(format!("## Visual review\n\n{visual}"));
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(format!(
        "TermF run {0} · `termf team status {0}`",
        run.target.run
    ));
    Some(parts.join("\n\n"))
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` as it reads, but inert in a pull request: no mention notifies anyone, no reference links or closes an
/// issue (a zero-width space after `@` and `#`, after the dash of `GH-12`, inside each closing keyword, and after
/// the scheme of a URL, which could name an issue whole).
fn defuse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let unlinked = text.replace("://", &format!(":{ZWSP}//"));
    for word in unlinked.split_inclusive(char::is_whitespace) {
        let bare = word.trim_matches(|c: char| !c.is_alphanumeric());
        let mut split = CLOSING.iter().any(|k| bare.eq_ignore_ascii_case(k));
        let issue = bare.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("gh-"))
            && bare.len() > 3
            && bare[3..].bytes().all(|b| b.is_ascii_digit());
        for c in word.chars() {
            out.push(c);
            if c == '@' || c == '#' || (issue && c == '-') || (split && c.is_alphabetic()) {
                out.push(ZWSP);
                split = split && !c.is_alphabetic();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_text_cannot_mention_link_or_close() {
        let text = defuse(
            "Fixes #12 and (closes acme/api#3), cc @alex; see https://x/issues/4. Resolved: yes",
        );
        for live in ["Fixes", "closes", "#12", "#3", "@alex", "Resolved"] {
            assert!(!text.contains(live), "{live} in {text}");
        }
        assert_eq!(
            text.replace(ZWSP, ""),
            "Fixes #12 and (closes acme/api#3), cc @alex; see https://x/issues/4. Resolved: yes"
        );
        assert_eq!(defuse("Adds the export"), "Adds the export");
        // GitLab's keywords too, and an issue's whole URL (the review of PR #135).
        let gitlab = "Closing #5, fixing #6; Resolving #7. Implements #8 and implemented #9, implementing #10, \
                      implement #11; see https://gitlab.com/acme/api/-/issues/4 and https://github.com/acme/api/issues/5";
        let text = defuse(gitlab);
        for live in [
            "Closing",
            "fixing",
            "Resolving",
            "Implements",
            "implemented",
            "implementing",
            "implement #",
            "https://gitlab.com/acme/api/-/issues/4",
            "https://github.com/acme/api/issues/5",
        ] {
            assert!(!text.contains(live), "{live} in {text}");
        }
        assert_eq!(text.replace(ZWSP, ""), gitlab);
        // GitHub links `GH-12` to issue 12 (the second review of PR #135).
        let short = "See GH-12, (gh-7) and GH-3. Not high-level nor GH-x.";
        let text = defuse(short);
        for live in ["GH-12", "gh-7", "GH-3"] {
            assert!(!text.contains(live), "{live} in {text}");
        }
        assert!(
            text.contains("high-level") && text.contains("GH-x"),
            "{text}"
        );
        assert_eq!(text.replace(ZWSP, ""), short);
    }
}
