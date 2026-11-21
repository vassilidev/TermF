use std::process::Command;

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("termf-onboarding-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Project `acme` with two throwaway git repositories: `api` (Node with pnpm, a CI workflow, a CLAUDE.md, an
/// `.mcp.json`) and `front`, on GitHub.
fn setup(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = temp(name);
    let (config, data) = (dir.join("config"), dir.join("data"));
    let (api, front) = (dir.join("api"), dir.join("front"));
    put(&api.join("package.json"), "{ \"name\": \"api\" }\n");
    put(&api.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    put(&api.join(".github/workflows/ci.yml"), "on: push\n");
    put(&api.join(".github/workflows/notes.txt"), "not a workflow\n");
    put(&api.join("CLAUDE.md"), "# api\n");
    put(&api.join(".mcp.json"), "{}\n");
    put(&front.join("README.md"), "front\n");
    for (repo, remote) in [
        (&api, None),
        (&front, Some("git@github.com:acme/front.git")),
    ] {
        let git = |args: &[&str]| {
            let out = Command::new("git").arg("-C").arg(repo).args(args).output();
            assert!(out.unwrap().status.success(), "{args:?}");
        };
        git(&["init", "-q"]);
        if let Some(remote) = remote {
            git(&["remote", "add", "origin", remote]);
        }
    }
    put(
        &config.join("projects/acme.toml"),
        &format!(
            "name = \"Acme\"\n[[repos]]\npath = {:?}\n[[repos]]\npath = {:?}\n",
            api.to_string_lossy(),
            front.to_string_lossy()
        ),
    );
    (dir, config, data)
}

/// A stand-in for Claude Code that records its arguments and prompt, then answers `answer` as Claude Code would.
#[cfg(unix)]
fn fake_claude(dir: &Path, answer: &Value) -> Vec<String> {
    let out = json!({ "type": "result", "is_error": false, "total_cost_usd": 0.4, "structured_output": answer });
    let script = dir.join("fake-claude.sh");
    put(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{0}/args.txt\"\ncat > \"{0}/prompt.txt\"\ncat <<'EOF'\n{1}\nEOF\n",
            dir.display(),
            out
        ),
    );
    vec!["/bin/sh".into(), script.to_string_lossy().into_owned()]
}

fn question(id: &str, kind: &str, options: &[&str], default: &[&str]) -> Value {
    json!({ "id": id, "theme": "tests", "text": format!("{id} ?"), "kind": kind, "options": options, "default": default })
}

#[test]
fn surveys_the_repositories_and_the_project_by_itself() {
    let (_, config, data) = setup("survey");
    let analysis = survey(&config, &data, "acme").unwrap();
    let api = &analysis.repos[0];
    assert_eq!(api.name, "api");
    assert!(
        api.stack.contains(&"node:pnpm".to_string()),
        "{:?}",
        api.stack
    );
    assert_eq!(api.workflows, ["ci.yml"]);
    assert_eq!(api.instructions, ["CLAUDE.md"]);
    assert_eq!(api.claude, [".mcp.json"]);
    let front = &analysis.repos[1];
    assert!(front.stack.is_empty() && front.workflows.is_empty() && front.instructions.is_empty());
    assert_eq!(analysis.github, ["acme/front"]);
    assert_eq!(analysis.tickets, ["github"]);
    assert!(!analysis.recipe && analysis.library.is_empty() && analysis.conventions.is_empty());
    assert!(!analysis.asked);
}

#[cfg(unix)]
#[test]
fn keeps_only_the_well_formed_part_of_claude_s_answer() {
    let (dir, config, data) = setup("analyze");
    let long = "x".repeat(301);
    let mut findings: Vec<Value> = (0..9)
        .map(|i| json!({ "repo": "api", "label": format!("fact {i}"), "value": "pnpm  test\n" }))
        .collect();
    findings.push(json!({ "repo": "nope", "label": "Tests", "value": "make test" }));
    findings.push(json!({ "repo": "front", "label": "Tests", "value": long }));
    findings.push(json!({ "repo": "front" }));
    let answer = json!({
        "findings": findings,
        "questions": [
            question("tests-required", "bool", &[], &["yes"]),
            question("checks", "multi", &["lint", "tests", "types"], &["lint", "tests"]),
            question("branch", "choice", &["feature/*", "fix/*"], &["feature/*"]),
            question("notes", "text", &[], &[""]),
            question("branch", "choice", &["a", "b"], &["a"]),
            question("Bad Id", "bool", &[], &["no"]),
            question("off-list", "choice", &["a", "b"], &["c"]),
            question("one-option", "choice", &["a"], &["a"]),
            question("maybe", "bool", &[], &["maybe"]),
            json!({ "id": "theme", "theme": "tickets", "text": "?", "kind": "bool", "options": [], "default": ["no"] }),
        ],
    });
    let claude = fake_claude(&dir, &answer);
    let state = analyze(&config, &data, "acme", "fr", &claude).unwrap();

    let args = fs::read_to_string(dir.join("args.txt")).unwrap();
    for arg in [
        "--max-budget-usd\n2\n",
        "-p\n",
        "--json-schema\n",
        "--tools\nRead,Grep,Glob\n",
        "--model\nopus\n",
    ] {
        assert!(args.contains(arg), "{arg:?} in {args}");
    }
    let prompt = fs::read_to_string(dir.join("prompt.txt")).unwrap();
    assert!(
        prompt.contains("stack: node:pnpm") && prompt.contains("in French"),
        "{prompt}"
    );

    let analysis = state.analysis.unwrap();
    assert!(analysis.asked);
    assert_eq!(analysis.cost, Some(0.4));
    assert_eq!(analysis.findings.len(), 8);
    assert_eq!(analysis.findings[0].value, "pnpm test");
    let ids: Vec<&str> = analysis.questions.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["tests-required", "checks", "branch", "notes"]);
    assert_eq!(analysis.questions[3].default, Vec::<String>::new());
    assert_eq!(analysis.dropped.len(), 4 + 6, "{:?}", analysis.dropped);
    assert_eq!(state.step, Step::Questions);
    assert_eq!(state.answers["checks"], ["lint", "tests"]);
    assert_eq!(load(&config, &data, "acme").unwrap().answers, state.answers);
}

#[cfg(unix)]
#[test]
fn keeps_the_survey_when_claude_code_fails() {
    let (dir, config, data) = setup("fails");
    let script = dir.join("broken.sh");
    put(
        &script,
        "#!/bin/sh\ncat > /dev/null\necho '{\"type\":\"result\",\"is_error\":true,\"subtype\":\"error_max_budget_usd\"}'\n",
    );
    let claude = vec!["/bin/sh".into(), script.to_string_lossy().into_owned()];
    let error = analyze(&config, &data, "acme", "en", &claude).unwrap_err();
    assert!(
        format!("{error:#}").contains("error_max_budget_usd"),
        "{error:#}"
    );
    let state = load(&config, &data, "acme").unwrap();
    let analysis = state.analysis.unwrap();
    assert!(!analysis.asked && analysis.repos.len() == 2);
    assert_eq!(state.step, Step::Analysis);
}

#[cfg(unix)]
#[test]
fn answers_become_the_onboarding_convention() {
    let (dir, config, data) = setup("answer");
    let answer = json!({
        "findings": [{ "repo": "api", "label": "Tests", "value": "pnpm test" }],
        "questions": [
            question("tests-required", "bool", &[], &["yes"]),
            question("checks", "multi", &["lint", "tests", "types"], &["lint"]),
            question("notes", "text", &[], &[]),
        ],
    });
    analyze(&config, &data, "acme", "en", &fake_claude(&dir, &answer)).unwrap();

    let refused = |id: &str, value: &[&str]| {
        let answers = BTreeMap::from([(
            id.to_string(),
            value.iter().map(|v| v.to_string()).collect(),
        )]);
        assert!(answer_(&config, &data, answers).is_err(), "{id} {value:?}");
    };
    refused("checks", &["deploy"]);
    refused("checks", &["lint", "lint"]);
    refused("tests-required", &["maybe"]);
    refused("unknown", &["yes"]);
    refused("notes", &[&"x".repeat(501)]);

    let answers = BTreeMap::from([
        (
            "checks".to_string(),
            vec!["lint".to_string(), "types".to_string()],
        ),
        ("notes".to_string(), vec!["  Small PRs only.  ".to_string()]),
    ]);
    let state = answer_(&config, &data, answers).unwrap();
    assert_eq!(state.step, Step::Proposals);
    assert_eq!(state.answers["tests-required"], ["yes"]);
    assert_eq!(state.answers["notes"], ["Small PRs only."]);
    let convention = conventions::load(&config, "acme", SLUG).unwrap();
    assert_eq!(
        convention.source,
        "# Project setup (TermF's onboarding assistant)\n\nWhat the repositories hold:\n\
         - api: node:pnpm\n- api: Tests: pnpm test\n\nWhat the owner decided:\n\
         - tests-required ? yes\n- checks ? lint, types\n- notes ? Small PRs only.\n"
    );
}

fn answer_(config: &Path, data: &Path, answers: BTreeMap<String, Vec<String>>) -> Result<State> {
    answer(config, data, "acme", answers)
}

#[test]
fn opens_by_itself_once_for_a_new_project_and_records_decisions() {
    let (_, config, data) = setup("state");
    assert!(!load(&config, &data, "acme").unwrap().auto_open);
    created(&config, &data, "acme").unwrap();
    assert!(load(&config, &data, "acme").unwrap().auto_open);
    seen(&config, &data, "acme").unwrap();
    created(&config, &data, "acme").unwrap();
    assert!(!load(&config, &data, "acme").unwrap().auto_open);

    assert_eq!(
        step(&config, &data, "acme", Step::Trust).unwrap().step,
        Step::Trust
    );
    let state = decide(
        &config,
        &data,
        "acme",
        "ci",
        Decision::Accepted,
        "notify: mine",
    )
    .unwrap();
    assert_eq!(state.decisions["ci"], Decision::Accepted);
    assert_eq!(state.log.last().unwrap().what, "ci accepted");
    assert!(decide(&config, &data, "acme", "deploy", Decision::Accepted, "").is_err());
    assert!(load(&config, &data, "../acme").is_err());
}
