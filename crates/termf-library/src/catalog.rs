//! Checks of the skills shipped in `library/skills/<tech>/`, run by `cargo test` on every change: the format a
//! TermF skill follows (agentskills.io and Claude Code's frontmatter, TermF's `metadata`), its card, its evals and
//! its credits, and descriptions that do not compete for the same words.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::stack::{TAGS, covers};
use crate::{ANY, parse_markdown};

/// agentskills.io's keys, and `paths`, which Claude Code reads.
const KEYS: [&str; 7] = [
    "name",
    "description",
    "license",
    "compatibility",
    "metadata",
    "allowed-tools",
    "paths",
];
const REQUIRED: [&str; 5] = [
    "termf-stacks",
    "termf-roles",
    "termf-versions",
    "termf-verified",
    "termf-sources",
];
const MAX_LINES: usize = 500;
const MAX_CARD: usize = 25;
const MIN_EVALS: usize = 3;
/// Skills shipped without evals, each with the reason.
const NO_EVALS: [(&str, &str); 0] = [];
/// The roles whose behaviour `pnpm evals` checks, in `library/roles/evals/<role>/`.
const EVALUATED_ROLES: [&str; 8] = [
    "cto",
    "reviewer",
    "qa",
    "front",
    "back",
    "researcher",
    "competitive-analyst",
    "reverse-engineer",
];
const MIN_ROLE_EVALS: usize = 2;
/// The frontmatter keys of a `prompt.md` (an unknown one fails the case), and the grader types, of `claude plugin
/// eval` (code.claude.com/docs/en/plugin-evals, 2026-11-22).
const PROMPT_KEYS: [&str; 13] = [
    "schema_version",
    "name",
    "description",
    "tags",
    "plugins",
    "runs",
    "expected_outcome",
    "model",
    "max_turns",
    "timeout_seconds",
    "allowed_tools",
    "append_system_prompt",
    "env",
];
const GRADERS: [(&str, &[&str]); 6] = [
    ("regex", &["pattern", "flags", "match", "target"]),
    ("tool_used", &["tool", "input_match", "min", "max"]),
    ("tool_order", &["before", "after"]),
    ("file_exists", &["path", "exists"]),
    ("llm", &["criteria", "focus"]),
    ("baseline", &["baseline_file", "criteria"]),
];
/// Two descriptions this close (cosine of their TF-IDF vectors) compete for the same requests.
const MAX_SIMILARITY: f64 = 0.6;
/// Globs that match a folder or a file of any kind of project.
const GENERIC_PATHS: [&str; 6] = [
    "**",
    "**/*",
    "**/app/**",
    "**/src/**",
    "**/server/**",
    "**/vite.config.*",
];

/// What is wrong with the skill in `dir`, each as `<folder>: <problem>`. `template`: the authors' template, whose
/// folder is not its name.
pub(crate) fn check(dir: &Path, notices: &str, roles: &[String], template: bool) -> Vec<String> {
    let folder = dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut errors = Vec::new();
    let mut fail = |text: String| errors.push(format!("{folder}: {text}"));
    let Ok(text) = fs::read_to_string(dir.join("SKILL.md")) else {
        fail("no SKILL.md".into());
        return errors;
    };
    let meta = match parse_markdown(&text) {
        Ok((meta, _)) if !meta.is_empty() => meta,
        Ok(_) => {
            fail("SKILL.md has no frontmatter".into());
            return errors;
        }
        Err(e) => {
            fail(format!("invalid frontmatter: {e:#}"));
            return errors;
        }
    };
    for key in meta.keys().filter(|k| !KEYS.contains(&k.as_str())) {
        fail(format!(
            "unknown frontmatter key {key:?}: TermF's own go under metadata"
        ));
    }
    let text_of = |key: &str| meta.get(key).and_then(Value::as_str).unwrap_or_default();
    let name = text_of("name");
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if !valid {
        fail(format!(
            "name {name:?}: 1 to 64 of a-z, 0-9 and single hyphens"
        ));
    }
    if !template && name != folder {
        fail(format!("name {name:?} is not the folder's"));
    }
    let description = text_of("description");
    if description.trim().is_empty() || description.chars().count() > 1024 {
        fail("description: 1 to 1,024 characters".into());
    }
    if text_of("compatibility").chars().count() > 500 {
        fail("compatibility: 500 characters at most".into());
    }
    if text_of("license").trim().is_empty() {
        fail("no license".into());
    }
    let metadata = meta.get("metadata").and_then(Value::as_object);
    let value = |key: &str| {
        metadata
            .and_then(|m| m.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    match metadata {
        None => fail("no metadata".into()),
        Some(map) => {
            for (key, v) in map {
                if !v.is_string() {
                    fail(format!("metadata.{key} is not text"));
                }
                if !REQUIRED.contains(&key.as_str()) && key != "credits" {
                    fail(format!("unknown metadata key {key:?}"));
                }
            }
            for key in REQUIRED.iter().filter(|k| value(k).trim().is_empty()) {
                fail(format!("no metadata.{key}"));
            }
        }
    }
    let words = |key: &str| {
        value(key)
            .split([' ', ','])
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
    };
    let stacks = words("termf-stacks");
    for tag in &stacks {
        if *tag == ANY && stacks.len() > 1 {
            fail("termf-stacks: \"*\" stands alone".into());
        } else if *tag != ANY && !TAGS.contains(tag) {
            fail(format!(
                "termf-stacks: {tag:?} is no tag the stack detection gives"
            ));
        }
    }
    for role in words("termf-roles") {
        if role != "all" && !roles.iter().any(|r| r == role) {
            fail(format!("termf-roles: no shipped role {role:?}"));
        }
    }
    let versions = value("termf-versions").trim();
    if !versions.is_empty() && versions != ANY && covers(versions, "1").is_none() {
        fail(format!(
            "termf-versions {versions:?}: comparisons such as \">=15 <17\", or \"*\""
        ));
    }
    let verified = value("termf-verified");
    let date = verified.split(' ').next().unwrap_or_default();
    let dated = date.len() == 10
        && date.char_indices().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    if !verified.is_empty() && (!dated || verified.trim().len() <= 11) {
        fail(format!(
            "termf-verified {verified:?}: a date (YYYY-MM-DD), then what was verified"
        ));
    }
    for source in words("termf-sources") {
        if !source.starts_with("https://") {
            fail(format!("termf-sources: {source:?} is not an https URL"));
        }
    }
    // The stack switches a skill on; `paths` only narrows it, so a folder every kind of project has is no filter.
    if stacks != [ANY] {
        let paths = crate::list_of(meta.get("paths"));
        for glob in paths.iter().filter(|g| GENERIC_PATHS.contains(&g.as_str())) {
            fail(format!(
                "paths: {glob:?} is no framework's own folder or file"
            ));
        }
    }
    for credit in crate::credits(&meta) {
        let credit = credit.as_str();
        // `owner/repo@commit (license): path`, as the roles write theirs.
        let source = credit.split_whitespace().next().unwrap_or_default();
        let (repo, commit) = source.split_once('@').unwrap_or((source, ""));
        let listed = notices.contains(&format!("https://github.com/{repo}"))
            && !commit.is_empty()
            && notices.contains(commit)
            && notices.contains(&format!("skills/{folder}"));
        if !listed {
            fail(format!(
                "credits {credit:?}: not in THIRD_PARTY_NOTICES.md with its commit and skills/{folder}"
            ));
        }
    }
    let lines = text.lines().count();
    if lines > MAX_LINES {
        fail(format!("SKILL.md has {lines} lines, {MAX_LINES} at most"));
    }
    if let Ok(card) = fs::read_to_string(dir.join("card.md")) {
        let lines = card.trim().lines().count();
        if lines > MAX_CARD {
            fail(format!("card.md has {lines} lines, {MAX_CARD} at most"));
        }
    }
    for link in references(&text) {
        if !dir.join(&link).is_file() {
            fail(format!("{link} is linked but missing"));
        }
    }
    errors.extend(
        evals(dir, name)
            .into_iter()
            .map(|e| format!("{folder}: {e}")),
    );
    errors
}

/// The `references/…` files a text links to.
fn references(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("references/") {
        let link: String = rest[at..]
            .chars()
            .take_while(|c| {
                !c.is_whitespace() && !matches!(c, '`' | ')' | '"' | '\'' | ']' | '>' | ',')
            })
            .collect();
        let link = link.trim_end_matches(['.', ':', ';']).to_owned();
        if link.len() > "references/".len() {
            found.insert(link);
        }
        rest = &rest[at + "references/".len()..];
    }
    found
}

/// The `claude plugin eval` cases in `evals`, a folder of case folders.
fn cases(evals: &Path) -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = fs::read_dir(evals)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("prompt.md").is_file() || p.join("case.yaml").is_file())
        .collect();
    cases.sort();
    cases
}

/// What `claude plugin eval` would refuse in a case: a frontmatter key it does not know, no grader, a grader of no
/// known type or with an option of another type. Each as `evals/<case>: <problem>`.
fn case_format(case: &Path) -> Vec<String> {
    let label = format!(
        "evals/{}",
        case.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut errors = Vec::new();
    if let Ok(text) = fs::read_to_string(case.join("prompt.md")) {
        match parse_markdown(&text) {
            Ok((meta, _)) => {
                for key in meta.keys().filter(|k| !PROMPT_KEYS.contains(&k.as_str())) {
                    errors.push(format!("{label}: unknown prompt.md key {key:?}"));
                }
            }
            Err(e) => errors.push(format!("{label}: invalid prompt.md: {e:#}")),
        }
    }
    let files: Vec<PathBuf> = fs::read_dir(case.join("graders"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    let in_yaml = fs::read_to_string(case.join("case.yaml")).is_ok_and(|t| t.contains("graders:"));
    if files.is_empty() && !in_yaml {
        errors.push(format!("{label}: no grader"));
    }
    for file in files {
        let grader = file.file_stem().unwrap_or_default().to_string_lossy();
        let text = fs::read_to_string(&file).unwrap_or_default();
        let Ok((meta, body)) = parse_markdown(&text) else {
            errors.push(format!("{label}: grader {grader} has no frontmatter"));
            continue;
        };
        let kind = meta.get("type").and_then(Value::as_str).unwrap_or_default();
        let Some((_, options)) = GRADERS.iter().find(|(t, _)| *t == kind) else {
            errors.push(format!("{label}: grader {grader} has no known type"));
            continue;
        };
        for key in meta.keys().filter(|k| {
            !["type", "weight", "arm"].contains(&k.as_str()) && !options.contains(&k.as_str())
        }) {
            errors.push(format!(
                "{label}: grader {grader} ({kind}) takes no {key:?}"
            ));
        }
        let pattern = meta
            .get("pattern")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let missing = match kind {
            "regex" => pattern.is_empty() || pattern.contains("(?i"),
            "tool_used" => !meta.contains_key("tool"),
            "llm" => body.trim().is_empty() && !meta.contains_key("criteria"),
            _ => false,
        };
        if missing {
            errors.push(format!(
                "{label}: grader {grader} ({kind}) lacks what it checks (a pattern without inline flags, a tool, criteria)"
            ));
        }
    }
    errors
}

/// The `claude plugin eval` cases in `evals/`: three at least, each with a grader, one checking that the skill
/// fires and one dormant case checking that it does not; none for a skill `NO_EVALS` lists.
fn evals(dir: &Path, name: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let cases = cases(&dir.join("evals"));
    if NO_EVALS.iter().any(|(n, _)| *n == name) {
        if !cases.is_empty() {
            errors.push(format!("has evals: take {name} out of NO_EVALS"));
        }
        return errors;
    }
    if cases.len() < MIN_EVALS {
        errors.push(format!(
            "{} eval cases in evals/, {MIN_EVALS} at least",
            cases.len()
        ));
    }
    let (mut fires, mut dormant) = (false, false);
    for case in &cases {
        errors.extend(case_format(case));
        let graders: Vec<_> = fs::read_dir(case.join("graders"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                parse_markdown(&fs::read_to_string(e.path()).ok()?)
                    .ok()
                    .map(|(m, _)| m)
            })
            .collect();
        for g in graders {
            let skill = g.get("type").and_then(Value::as_str) == Some("tool_used")
                && g.get("tool").and_then(Value::as_str) == Some("Skill")
                && g.get("input_match")
                    .and_then(Value::as_str)
                    .is_some_and(|m| m.contains(name));
            if skill && g.get("max").and_then(Value::as_i64) == Some(0) {
                dormant = true;
            } else if skill {
                fires = true;
            }
        }
    }
    if !fires {
        errors.push(format!(
            "no eval checks that the skill {name} fires (tool_used: Skill)"
        ));
    }
    if !dormant {
        errors.push(format!(
            "no dormant eval (tool_used: Skill, min: 0, max: 0) for {name}"
        ));
    }
    errors
}

fn tokens(text: &str) -> Vec<String> {
    const STOP: [&str; 24] = [
        "the", "and", "for", "with", "when", "use", "not", "its", "that", "this", "from", "any",
        "are", "into", "what", "how", "you", "your", "uses", "used", "than", "but", "such", "only",
    ];
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOP.contains(w))
        .map(str::to_owned)
        .collect()
}

/// Pairs of descriptions that compete for the same words, with their similarity.
pub(crate) fn collisions(descriptions: &BTreeMap<String, String>) -> Vec<(String, String, f64)> {
    let docs: Vec<(&String, Vec<String>)> =
        descriptions.iter().map(|(n, d)| (n, tokens(d))).collect();
    let count = docs.len() as f64;
    let mut df: BTreeMap<&str, f64> = BTreeMap::new();
    for (_, words) in &docs {
        for w in words.iter().collect::<BTreeSet<_>>() {
            *df.entry(w).or_default() += 1.0;
        }
    }
    let vectors: Vec<BTreeMap<&str, f64>> = docs
        .iter()
        .map(|(_, words)| {
            let mut v: BTreeMap<&str, f64> = BTreeMap::new();
            for w in words {
                *v.entry(w).or_default() += 1.0;
            }
            for (w, x) in v.iter_mut() {
                *x *= ((1.0 + count) / (1.0 + df[w])).ln() + 1.0;
            }
            v
        })
        .collect();
    let norm = |v: &BTreeMap<&str, f64>| v.values().map(|x| x * x).sum::<f64>().sqrt();
    let mut found = Vec::new();
    for i in 0..docs.len() {
        for j in i + 1..docs.len() {
            let dot: f64 = vectors[i]
                .iter()
                .filter_map(|(w, x)| vectors[j].get(w).map(|y| x * y))
                .sum();
            let similarity = dot / (norm(&vectors[i]) * norm(&vectors[j])).max(f64::EPSILON);
            if similarity > MAX_SIMILARITY {
                found.push((docs[i].0.clone(), docs[j].0.clone(), similarity));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{put, temp_dir};

    fn library() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library")
    }

    fn roles() -> Vec<String> {
        fs::read_dir(library().join("roles"))
            .unwrap()
            .flatten()
            .filter_map(|e| Some(e.file_name().to_str()?.strip_suffix(".md")?.to_owned()))
            .collect()
    }

    fn notices() -> String {
        fs::read_to_string(library().join("THIRD_PARTY_NOTICES.md")).unwrap()
    }

    #[test]
    fn the_shipped_skills_pass_the_catalog_checks() {
        let mut errors = Vec::new();
        let mut descriptions = BTreeMap::new();
        let mut dirs: Vec<PathBuf> = fs::read_dir(library().join("skills"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter(|p| {
                !p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(['_', '.'])
            })
            .collect();
        dirs.sort();
        for dir in &dirs {
            errors.extend(check(dir, &notices(), &roles(), false));
            let text = fs::read_to_string(dir.join("SKILL.md")).unwrap_or_default();
            if let Ok((meta, _)) = parse_markdown(&text)
                && let Some(d) = meta.get("description").and_then(Value::as_str)
            {
                descriptions.insert(
                    dir.file_name().unwrap().to_string_lossy().into_owned(),
                    d.to_owned(),
                );
            }
        }
        for (a, b, similarity) in collisions(&descriptions) {
            errors.push(format!(
                "{a} and {b}: their descriptions compete for the same words ({similarity:.2})"
            ));
        }
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }

    #[test]
    fn the_key_roles_have_evals_claude_code_can_run() {
        let dir = library().join("roles/evals");
        let mut errors = Vec::new();
        for role in fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()) {
            let name = role.file_name().unwrap().to_string_lossy().into_owned();
            if !roles().contains(&name) {
                errors.push(format!("roles/evals/{name}: no shipped role {name}"));
            }
            let cases = cases(&role);
            if cases.len() < MIN_ROLE_EVALS {
                errors.push(format!(
                    "roles/evals/{name}: {} cases, {MIN_ROLE_EVALS} at least",
                    cases.len()
                ));
            }
            for case in &cases {
                errors.extend(
                    case_format(case)
                        .into_iter()
                        .map(|e| format!("roles/{name}/{e}")),
                );
            }
        }
        for role in EVALUATED_ROLES {
            if !dir.join(role).is_dir() {
                errors.push(format!("no evals for the role {role}"));
            }
        }
        for (skill, why) in NO_EVALS {
            if !library().join("skills").join(skill).is_dir() || why.trim().is_empty() {
                errors.push(format!(
                    "NO_EVALS: {skill} is no shipped skill, or has no reason"
                ));
            }
        }
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }

    #[test]
    fn the_case_format_is_the_one_claude_code_reads() {
        let case = temp_dir("catalog-case").join("evals/bad");
        put(
            &case.join("prompt.md"),
            "---\nmax_turns: 5\nallowed-tools: [Read]\n---\nDo it.\n",
        );
        put(
            &case.join("graders/a.md"),
            "---\ntype: regex\npattern: '(?i)done'\nfocus: last_message\n---\n",
        );
        put(&case.join("graders/b.md"), "---\ntype: script\n---\n");
        put(&case.join("graders/c.md"), "---\ntype: llm\n---\n");
        let errors = case_format(&case).join("\n");
        for expected in [
            "evals/bad: unknown prompt.md key \"allowed-tools\"",
            "grader a (regex) takes no \"focus\"",
            "grader a (regex) lacks what it checks",
            "grader b has no known type",
            "grader c (llm) lacks what it checks",
        ] {
            assert!(
                errors.contains(expected),
                "missing {expected:?} in:\n{errors}"
            );
        }
    }

    #[test]
    fn the_template_passes_the_checks_but_for_its_folder() {
        let template = library().join("skills/_template");
        assert_eq!(
            check(&template, &notices(), &roles(), true),
            Vec::<String>::new()
        );
    }

    #[test]
    fn the_checks_catch_what_would_break_a_skill() {
        let dir = temp_dir("catalog").join("react");
        put(
            &dir.join("SKILL.md"),
            &format!(
                "---\nname: React_\ndescription: \"\"\nlicense: MIT\npaths: \"**/src/**, next.config.*\"\nversion: 2\nmetadata:\n  termf-stacks: \"reactjs *\"\n  termf-roles: frontend\n  termf-versions: \"~19\"\n  termf-verified: \"yesterday\"\n  termf-sources: \"http://react.dev\"\n  credits: \"someone/skills@abc123 (MIT): react.md\"\n  owner: me\n---\nRead references/hooks.md when the task touches effects.\n{}",
                "line\n".repeat(500)
            ),
        );
        put(&dir.join("card.md"), &"- pitfall\n".repeat(26));
        put(&dir.join("evals/one/prompt.md"), "Build a form.\n");
        let errors = check(&dir, "", &["front".into()], false).join("\n");
        for expected in [
            "react: unknown frontmatter key \"version\"",
            "react: name \"React_\": 1 to 64",
            "react: name \"React_\" is not the folder's",
            "react: description: 1 to 1,024 characters",
            "react: unknown metadata key \"owner\"",
            "termf-stacks: \"*\" stands alone",
            "termf-stacks: \"reactjs\" is no tag",
            "termf-roles: no shipped role \"frontend\"",
            "termf-versions \"~19\"",
            "termf-verified \"yesterday\"",
            "termf-sources: \"http://react.dev\"",
            "credits \"someone/skills@abc123 (MIT): react.md\": not in THIRD_PARTY_NOTICES.md",
            "SKILL.md has 516 lines, 500 at most",
            "card.md has 26 lines",
            "references/hooks.md is linked but missing",
            "1 eval cases in evals/",
            "evals/one: no grader",
            "no eval checks that the skill React_ fires",
            "no dormant eval",
            "paths: \"**/src/**\" is no framework",
        ] {
            assert!(
                errors.contains(expected),
                "missing {expected:?} in:\n{errors}"
            );
        }

        let mut descriptions = BTreeMap::new();
        descriptions.insert(
            "react".to_owned(),
            "Builds React components, hooks and Server Components. Use when a task touches React code.".to_owned(),
        );
        descriptions.insert(
            "react-ui".to_owned(),
            "Builds React components and hooks. Use when a task touches React components."
                .to_owned(),
        );
        descriptions.insert(
            "django".to_owned(),
            "Writes Django models, migrations and views. Use when a task touches a Django project."
                .to_owned(),
        );
        let found: Vec<(String, String)> = collisions(&descriptions)
            .into_iter()
            .map(|(a, b, _)| (a, b))
            .collect();
        assert_eq!(found, [("react".to_owned(), "react-ui".to_owned())]);
    }
}
