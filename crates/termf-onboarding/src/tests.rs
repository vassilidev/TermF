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

#[cfg(unix)]
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
            question("team-size", "bool", &[], &["yes"]),
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
    assert_eq!(
        ids,
        [
            "tests-required",
            "checks",
            "branch",
            "notes",
            "team-size",
            "team-model"
        ],
        "TermF's own team questions come last, and the team cannot be split here"
    );
    assert_eq!(analysis.questions[3].default, Vec::<String>::new());
    assert_eq!(analysis.questions[4].text, "obTeamQSize");
    assert_eq!(analysis.dropped.len(), 4 + 7, "{:?}", analysis.dropped);
    assert!(
        analysis
            .dropped
            .contains(&"question team-size: an id kept for TermF's own questions".into())
    );
    assert!(prompt.contains("Never ask about the agents' team either"));
    assert_eq!(state.step, Step::Questions);
    assert_eq!(state.answers["checks"], ["lint", "tests"]);
    assert_eq!(load(&config, &data, "acme").unwrap().answers, state.answers);
}

#[cfg(unix)]
#[test]
fn keeps_the_survey_when_claude_code_fails_and_a_failed_rerun_keeps_the_last_analysis() {
    let (dir, config, data) = setup("fails");
    let script = dir.join("broken.sh");
    put(
        &script,
        "#!/bin/sh\ncat > /dev/null\necho '{\"type\":\"result\",\"is_error\":true,\"subtype\":\"error_max_budget_usd\"}'\n",
    );
    let broken = vec!["/bin/sh".into(), script.to_string_lossy().into_owned()];
    let error = analyze(&config, &data, "acme", "en", &broken).unwrap_err();
    assert!(
        format!("{error:#}").contains("error_max_budget_usd"),
        "{error:#}"
    );
    let state = load(&config, &data, "acme").unwrap();
    let analysis = state.analysis.unwrap();
    assert!(!analysis.asked && analysis.repos.len() == 2);
    assert_eq!(state.step, Step::Analysis);
    assert!(!state.analyzing);

    let answer =
        json!({ "findings": [], "questions": [question("tests-required", "bool", &[], &["yes"])] });
    let asked = analyze(&config, &data, "acme", "en", &fake_claude(&dir, &answer)).unwrap();
    assert!(analyze(&config, &data, "acme", "en", &broken).is_err());
    let state = load(&config, &data, "acme").unwrap();
    assert_eq!(state.analysis, asked.analysis);
    assert_eq!(state.answers, asked.answers);
    assert!(answer_(&config, &data, BTreeMap::new()).is_ok());
}

#[cfg(unix)]
#[test]
fn analyzes_repositories_before_they_are_a_project_and_the_project_adopts_it() {
    let (dir, config, data) = setup("draft");
    let paths: Vec<String> = ["api", "front"]
        .map(|r| dir.join(r).to_string_lossy().into_owned())
        .to_vec();
    let mut tests_required = question("tests-required", "bool", &[], &["yes"]);
    tests_required["why"] = json!("A missing test sends the change back to the agent.");
    tests_required["proof"] = json!("x".repeat(201));
    let answer = json!({
        "findings": [{ "repo": "api", "label": "Tests", "value": "pnpm test" }],
        "questions": [tests_required],
    });
    let mut read = 0;
    let (analysis, error) = analyze_draft(
        &config,
        &data,
        &paths,
        None,
        "en",
        &fake_claude(&dir, &answer),
        &mut |_| read += 1,
    )
    .unwrap();
    assert_eq!(error, None);
    assert!(analysis.asked && analysis.repos.len() == 2);
    assert_eq!(analysis.github, ["acme/front"]);
    assert!(analysis.tickets.is_empty() && analysis.library.is_empty());
    assert_eq!(
        analysis.questions[0].why,
        "A missing test sends the change back to the agent."
    );
    assert_eq!(analysis.questions[0].proof, "", "too long to read");
    assert_eq!(analysis.questions.last().unwrap().id, "team-model");
    assert!(
        !data.join("onboarding/acme.json").exists(),
        "nothing written before the project is created"
    );

    let state = adopt(&config, &data, "acme", analysis.clone()).unwrap();
    assert_eq!(state.analysis.as_ref(), Some(&analysis));
    assert_eq!(state.answers["tests-required"], ["yes"]);
    assert_eq!(state.step, Step::Questions);
    let state = answer_(&config, &data, BTreeMap::new()).unwrap();
    assert!(state.source.contains("pnpm test"), "{}", state.source);

    let mut bad = analysis.clone();
    bad.questions[0].default = vec!["maybe".into()];
    assert!(adopt(&config, &data, "acme", bad).is_err());

    let script = dir.join("broken.sh");
    put(&script, "#!/bin/sh\ncat > /dev/null\nexit 3\n");
    let broken = vec!["/bin/sh".into(), script.to_string_lossy().into_owned()];
    let (alone, error) = analyze_draft(
        &config,
        &data,
        &paths,
        Some("acme"),
        "fr",
        &broken,
        &mut |_| {},
    )
    .unwrap();
    assert!(error.is_some(), "the reason Claude Code failed comes back");
    assert!(!alone.asked && alone.lang == "fr");
    assert_eq!(alone.tickets, ["github"], "the project's own survey");
    assert!(
        alone.questions.iter().all(|q| q.id.starts_with("team-")),
        "TermF's own questions still"
    );
}

#[cfg(unix)]
#[test]
fn the_cto_advises_an_answer_the_question_accepts() {
    let (dir, config, data) = setup("advise");
    let mut analysis = survey(&config, &data, "acme").unwrap();
    analysis.lang = "fr".into();
    analysis.questions = vec![
        serde_json::from_value(question(
            "commits",
            "choice",
            &["conventional", "free"],
            &["free"],
        ))
        .unwrap(),
    ];
    let advice = json!({ "answer": ["conventional"], "why": "Les 40 derniers commits  suivent feat:/fix:." });
    let got = advise(
        &config,
        &analysis,
        "commits",
        &fake_claude(&dir, &advice),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(got.answer, ["conventional"]);
    assert_eq!(got.why, "Les 40 derniers commits suivent feat:/fix:.");
    let prompt = fs::read_to_string(dir.join("prompt.txt")).unwrap();
    assert!(
        prompt.contains("commits ?") && prompt.contains("in French"),
        "{prompt}"
    );
    let args = fs::read_to_string(dir.join("args.txt")).unwrap();
    assert!(args.contains("--max-budget-usd\n0.5\n"), "{args}");

    let off = json!({ "answer": ["squash"], "why": "?" });
    assert!(
        advise(
            &config,
            &analysis,
            "commits",
            &fake_claude(&dir, &off),
            &mut |_| {}
        )
        .is_err()
    );
    assert!(
        advise(
            &config,
            &analysis,
            "nope",
            &fake_claude(&dir, &advice),
            &mut |_| {}
        )
        .is_err()
    );
}

#[test]
fn refuses_a_second_analysis_of_a_project_while_one_runs() {
    let (_, config, data) = setup("running");
    let key = || file(&config, &data, "acme").unwrap();
    let first = Running::start(key()).unwrap();
    assert!(load(&config, &data, "acme").unwrap().analyzing);
    assert!(Running::start(key()).is_err());
    let other = Running::start(data.join("onboarding/docs.json")).unwrap();
    drop(first);
    assert!(!load(&config, &data, "acme").unwrap().analyzing);
    assert!(Running::start(key()).is_ok());
    drop(other);
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
         - tests-required ? yes\n- checks ? lint, types\n- notes ? Small PRs only.\n\n\
         The agents' team (proposed by the assistant):\n- devops: .github/workflows/**\n\
         Each role gets the project's instructions that concern it.\n",
        "TermF's team questions stay out of what the owner decided"
    );
    assert_eq!(state.answers["team-size"], ["1"]);
    assert!(!state.edited);

    // Edited by hand in the library: new answers replace it only once asked to.
    conventions::save_source(&config, "acme", SLUG, "Small PRs, always.\n").unwrap();
    assert!(load(&config, &data, "acme").unwrap().edited);
    assert!(answer_(&config, &data, BTreeMap::new()).is_err());
    assert_eq!(
        conventions::load(&config, "acme", SLUG).unwrap().source,
        "Small PRs, always.\n"
    );
    let state = super::answer(&config, &data, "acme", BTreeMap::new(), true).unwrap();
    assert!(!state.edited);
    assert_eq!(
        conventions::load(&config, "acme", SLUG).unwrap().source,
        state.source
    );
}

#[cfg(unix)]
fn answer_(config: &Path, data: &Path, answers: BTreeMap<String, Vec<String>>) -> Result<State> {
    answer(config, data, "acme", answers, false)
}

#[test]
fn opens_by_itself_once_for_a_new_project_and_records_decisions() {
    let (_, config, data) = setup("state");
    assert!(!load(&config, &data, "acme").unwrap().auto_open);
    created(&config, &data, "acme").unwrap();
    assert!(load(&config, &data, "acme").unwrap().auto_open);
    assert!(!seen(&config, &data, "acme").unwrap().auto_open);

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
    let logged = state.log.last().unwrap();
    assert_eq!(
        (logged.what.as_str(), logged.decision),
        ("ci", Some(Decision::Accepted))
    );
    assert!(decide(&config, &data, "acme", "deploy", Decision::Accepted, "").is_err());
    assert!(load(&config, &data, "../acme").is_err());

    // A project deleted then created again with the same id starts over.
    created(&config, &data, "acme").unwrap();
    assert_eq!(
        load(&config, &data, "acme").unwrap(),
        State {
            auto_open: true,
            ..State::default()
        }
    );
}

/// `cargo test -p termf-onboarding -- --ignored real_onboarding --nocapture`: the installed Claude Code (Haiku,
/// its own login, TermF's config and data throwaway) on a throwaway Node repository: the project setup's survey,
/// the assistant's analysis, a convention written in one sentence proposed then applied, and a short scout pass.
#[test]
#[ignore]
fn real_onboarding() {
    use termf_library::conventions::Review;
    use termf_library::scout;

    let dir = temp("real");
    let (config, data, repo) = (dir.join("config"), dir.join("data"), dir.join("shop"));
    put(
        &repo.join("package.json"),
        r#"{ "name": "shop", "scripts": { "test": "node --test", "lint": "eslint ." }, "devDependencies": { "eslint": "^9.0.0" } }"#,
    );
    put(
        &repo.join("package-lock.json"),
        r#"{ "lockfileVersion": 3 }"#,
    );
    put(
        &repo.join("src/cart.js"),
        "export function total(items) {\n  return items.reduce((s, i) => s + i.price * i.qty, 0);\n}\n",
    );
    put(
        &repo.join("test/cart.test.js"),
        "import { test } from 'node:test';\nimport assert from 'node:assert';\nimport { total } from '../src/cart.js';\ntest('total', () => assert.equal(total([{ price: 2, qty: 3 }]), 6));\n",
    );
    put(&repo.join("README.md"), "# Shop\n\nA tiny cart.\n");
    let git = |args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(&repo).args(args).output();
        assert!(out.unwrap().status.success(), "{args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.name=a",
        "-c",
        "user.email=a@b",
        "commit",
        "-q",
        "-m",
        "init",
    ]);
    put(
        &config.join("projects/acme.toml"),
        &format!(
            "name = \"Acme\"\n[[repos]]\npath = {:?}\n",
            repo.to_string_lossy()
        ),
    );
    put(
        &config.join("termf.toml"),
        "[conventions]\nmodel = \"haiku\"\neffort = \"low\"\n[scout]\nmodel = \"haiku\"\nbudget = 0.2\n",
    );
    let claude: Vec<String> = ["claude", "--max-budget-usd", "0.2"]
        .map(Into::into)
        .to_vec();
    let mut spent = 0.0;

    // The project setup's survey: files and git only.
    let path = repo.to_string_lossy().into_owned();
    let detected = setup::detect(&config, &path).unwrap();
    assert_eq!(detected.facts.stack, ["node:npm"], "{detected:?}");
    assert!(
        detected.scripts.iter().any(|s| s.run == "npm test"),
        "{detected:?}"
    );
    setup::draft(&config, std::slice::from_ref(&path), &Choices::default()).unwrap();

    let state = analyze(&config, &data, "acme", "fr", &claude).unwrap();
    let analysis = state.analysis.clone().unwrap();
    eprintln!("analysis: {analysis:#?}");
    assert!(analysis.asked, "{analysis:?}");
    spent += analysis.cost.unwrap_or_default();
    assert!(!analysis.findings.is_empty() || !analysis.questions.is_empty());
    let state = answer(&config, &data, "acme", BTreeMap::new(), false).unwrap();
    assert_eq!(state.step, Step::Proposals);

    // One sentence, proposed then applied.
    let slug = "commits";
    conventions::save_source(
        &config,
        "acme",
        slug,
        "Les messages de commit sont en anglais, à l'impératif, en une ligne de 72 caractères au plus.\n",
    )
    .unwrap();
    let mut convention = conventions::propose(&config, "acme", slug, Vec::new(), &claude).unwrap();
    let draft = convention.draft.clone().unwrap();
    eprintln!("convention: {draft:#?}");
    spent += draft.cost.unwrap_or_default();
    if !draft.questions.is_empty() {
        let answers = draft
            .questions
            .iter()
            .map(|q| conventions::Answer {
                question: q.clone(),
                answer: "Choose the simplest option.".into(),
            })
            .collect();
        convention = conventions::propose(&config, "acme", slug, answers, &claude).unwrap();
        spent += convention
            .draft
            .as_ref()
            .and_then(|d| d.cost)
            .unwrap_or_default();
    }
    let items = convention.draft.clone().unwrap().items;
    assert!(!items.is_empty(), "{convention:?}");
    for item in &items {
        conventions::review(
            &config,
            "acme",
            slug,
            &item.name,
            Some(Review::Accepted),
            None,
        )
        .unwrap();
    }
    let applied = conventions::apply(&config, "acme", slug).unwrap();
    assert!(applied.draft.is_none());
    assert_eq!(applied.pack.unwrap().items.len(), items.len());

    // A short scout pass on one shipped skill.
    let only = [scout::ItemRef {
        kind: termf_library::Kind::Skill,
        name: "curl".into(),
    }];
    let found = scout::run(
        &config,
        None,
        &only,
        &scout::Options::default(),
        &claude,
        &mut |p| eprintln!("scout: {p:?}"),
    )
    .unwrap();
    eprintln!("scout: {found:#?}");
    spent += found.cost.unwrap_or_default();
    assert!(found.entries.iter().all(|e| e.name == "curl"), "{found:?}");
    eprintln!("spent: ${spent:.3}");
    assert!(spent < 1.0);
    let _ = fs::remove_dir_all(&dir);
}
