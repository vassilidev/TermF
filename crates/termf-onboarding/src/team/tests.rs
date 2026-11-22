use std::path::PathBuf;

use super::*;

type Files<'a> = &'a [(&'a str, &'a str)];

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Project `acme` in a throwaway config, with one throwaway repository per `(name, files)`.
fn setup(name: &str, repos: &[(&str, Files)]) -> (PathBuf, PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "termf-onboarding-team-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let config = dir.join("config");
    let mut file = "# Acme, by hand\nname = \"Acme\"\n".to_owned();
    for (repo, files) in repos {
        let root = dir.join(repo);
        fs::create_dir_all(&root).unwrap();
        for (path, text) in *files {
            put(&root.join(path), text);
        }
        file += &format!("\n[[repos]]\npath = {:?}\n", root.to_string_lossy());
    }
    file += "\n[trackers.github]\nissues = true\n";
    put(&config.join("projects/acme.toml"), &file);
    (dir.clone(), config, dir.join("data"))
}

const SIMPLE: Files = &[
    (
        "package.json",
        r#"{ "name": "shop", "dependencies": { "next": "15" } }"#,
    ),
    ("src/cart.test.ts", ""),
    ("public/robots.txt", ""),
    ("Dockerfile", "FROM node\n"),
    ("node_modules/left-pad/test/a.js", ""),
];
const MONOREPO: Files = &[
    (
        "pnpm-workspace.yaml",
        "packages:\n  - 'apps/*'\n  - \"packages/*\" # shared\n  - '!**/test/**'\n",
    ),
    (
        "package.json",
        r#"{ "name": "acme", "devDependencies": { "turbo": "2" } }"#,
    ),
    (
        "apps/web/package.json",
        r#"{ "name": "@acme/web", "dependencies": { "react": "19" }, "devDependencies": { "vite": "6" } }"#,
    ),
    (
        "apps/api/package.json",
        r#"{ "name": "@acme/api", "dependencies": { "express": "5" } }"#,
    ),
    ("apps/api/tests/orders.test.ts", ""),
    (
        "packages/ui/package.json",
        r#"{ "name": "@acme/ui", "peerDependencies": { "react": "19" } }"#,
    ),
    (
        "packages/utils/package.json",
        r#"{ "name": "@acme/utils" }"#,
    ),
    (".github/workflows/ci.yml", "on: push\n"),
    ("infra/main.tf", ""),
    ("docs/README.md", ""),
];
const CARGO: Files = &[
    ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
    (
        "crates/core/Cargo.toml",
        "[package]\nname = \"engine-core\"\n",
    ),
    ("crates/core/tests/it.rs", ""),
    (
        "crates/cli/Cargo.toml",
        "[package]\nname = \"engine-cli\"\n[dependencies]\nclap = \"4\"\n",
    ),
    ("target/debug/tests/x.rs", ""),
];
const API: Files = &[
    ("go.mod", "module billing\n"),
    ("main.go", ""),
    ("main_test.go", ""),
];

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn kept(team: &Team) -> Vec<(&str, bool, &str)> {
    team.roles
        .iter()
        .map(|r| (r.role.as_str(), r.keep, r.why.as_str()))
        .collect()
}

fn composition(team: &Team) -> &Composition {
    team.proposal.team.as_ref().unwrap()
}

fn writes(team: &Team, role: &str) -> Option<Vec<String>> {
    team.proposal.roles.get(role)?.writes.clone()
}

#[test]
fn surveys_the_code_of_each_kind_of_repository() {
    let (dir, config, _) = setup(
        "survey",
        &[
            ("shop", SIMPLE),
            ("acme", MONOREPO),
            ("engine", CARGO),
            ("billing", API),
        ],
    );
    let items = library(&config, "acme").unwrap();
    let repos = crate::repos(&config, "acme", &test_globs(&items)).unwrap();
    let part = |path: &str, name: &str, role: &str, web: bool| Part {
        path: path.into(),
        name: name.into(),
        role: role.into(),
        web,
    };

    let shop = &repos[0];
    assert_eq!(shop.workspace, None);
    assert_eq!(shop.parts, [part("", "shop", "front", true)]);
    assert_eq!(shop.tests, ["**/*.test.*"], "node_modules is left out");
    assert_eq!(shop.infra, ["Dockerfile"]);

    let mono = &repos[1];
    assert_eq!(mono.workspace.as_deref(), Some("pnpm"));
    assert_eq!(
        mono.parts,
        [
            part("apps/api", "@acme/api", "back", false),
            part("apps/web", "@acme/web", "front", false),
            part("packages/ui", "@acme/ui", "front", false),
            part("packages/utils", "@acme/utils", "back", false),
        ],
        "the workspace's root only holds tooling"
    );
    assert_eq!(mono.tests, ["**/tests/**", "**/*.test.*"]);
    assert_eq!(mono.infra, [".github/workflows/", "infra/"]);

    let engine = &repos[2];
    assert_eq!(engine.workspace.as_deref(), Some("cargo"));
    assert_eq!(
        engine.parts,
        [
            part("crates/cli", "engine-cli", "back", false),
            part("crates/core", "engine-core", "back", false),
        ]
    );
    assert_eq!(engine.tests, ["**/tests/**"], "target is left out");
    assert!(engine.infra.is_empty());

    let billing = &repos[3];
    assert_eq!(billing.parts, [part("", "billing", "back", false)]);
    assert_eq!(billing.tests, ["**/*_test.*"]);

    // Laravel keeps its interface in `resources`, and a public site has its robots.txt.
    let laravel = dir.join("laravel");
    for file in [
        "composer.json",
        "artisan",
        "resources/views/home.blade.php",
        "public/robots.txt",
    ] {
        put(&laravel.join(file), "");
    }
    let (_, parts) = parts(&laravel);
    assert_eq!(
        parts,
        [
            part("", "laravel", "back", false),
            part("resources", "laravel", "front", true)
        ]
    );
    assert_eq!(
        yaml_list("packages: ['a/*', \"b\"]\n", "packages"),
        ["a/*", "b"]
    );
    assert_eq!(
        go_uses("go 1.22\nuse ./api\nuse (\n  ./web // site\n)\n"),
        ["./api", "./web"]
    );
}

#[test]
fn proposes_a_team_for_each_kind_of_project() {
    let (_, config, data) = setup("simple", &[("shop", SIMPLE)]);
    let simple = team(&config, &data, "acme", Choices::default()).unwrap();
    assert_eq!(
        kept(&simple),
        [
            ("architect", true, "design"),
            ("back", false, "no-back"),
            ("devops", true, "infra"),
            ("front", true, "front"),
            ("qa", true, "tests"),
            ("ux", true, "ui"),
        ]
    );
    assert_eq!(simple.roles[3].detail, ["shop"]);
    assert_eq!(simple.steps, ["cto", "lead", "pm", "po", "secops"]);
    let squads = &composition(&simple).squads;
    assert_eq!(squads.len(), 1);
    assert_eq!(
        (squads[0].id.as_str(), squads[0].lead.as_str()),
        ("main", "lead")
    );
    assert_eq!(
        squads[0].roles,
        BTreeMap::from([("devops".into(), 1), ("front".into(), 1), ("qa".into(), 1)])
    );
    assert_eq!(composition(&simple).staff, ["architect", "ux"]);
    assert_eq!(simple.proposal.workflow.as_deref(), Some("default"));
    assert_eq!(writes(&simple, "qa"), Some(strings(&["**/*.test.*"])));
    assert_eq!(writes(&simple, "devops"), Some(strings(&["Dockerfile"])));
    assert_eq!(
        writes(&simple, "front"),
        None,
        "the front is the whole repository"
    );
    assert!(!simple.splittable);
    assert!(simple.status.values().all(|s| *s == Status::New));

    let (_, config, data) = setup("mono", &[("acme", MONOREPO)]);
    let mono = team(&config, &data, "acme", Choices::default()).unwrap();
    assert!(mono.splittable);
    assert_eq!(
        (mono.choices.split, mono.choices.size),
        (Some(true), Some(2))
    );
    let squads = &composition(&mono).squads;
    let (web, api) = (&squads[0], &squads[1]);
    assert_eq!(web.id, "web");
    assert_eq!(
        web.roles,
        BTreeMap::from([("front".into(), 2), ("qa".into(), 1)])
    );
    assert_eq!(web.paths, ["apps/web/**", "packages/ui/**"]);
    assert_eq!(web.domains, ["@acme/web", "@acme/ui"]);
    assert!(web.repos.is_empty());
    assert_eq!(api.id, "api");
    assert_eq!(
        api.roles,
        BTreeMap::from([("back".into(), 2), ("devops".into(), 1), ("qa".into(), 1)])
    );
    assert_eq!(
        api.paths,
        [
            "apps/api/**",
            "packages/utils/**",
            ".github/workflows/**",
            "infra/**"
        ]
    );
    assert_eq!(composition(&mono).staff, ["architect", "ux"]);
    assert_eq!(mono.proposal.workflow.as_deref(), Some("squads"));
    assert_eq!(
        writes(&mono, "front"),
        Some(strings(&["apps/web/**", "packages/ui/**"]))
    );
    assert_eq!(
        writes(&mono, "qa"),
        Some(strings(&["**/tests/**", "**/*.test.*"]))
    );

    let (_, config, data) = setup("cargo", &[("engine", CARGO)]);
    let cargo = team(&config, &data, "acme", Choices::default()).unwrap();
    assert_eq!(
        composition(&cargo).squads[0].roles,
        BTreeMap::from([("back".into(), 2), ("qa".into(), 1)])
    );
    assert_eq!(writes(&cargo, "back"), Some(strings(&["crates/**"])));
    assert!(
        kept(&cargo).contains(&("front", false, "no-front"))
            && kept(&cargo).contains(&("ux", false, "no-ui"))
            && kept(&cargo).contains(&("devops", false, "no-infra"))
    );

    let (_, config, data) = setup("api", &[("billing", API)]);
    let api = team(&config, &data, "acme", Choices::default()).unwrap();
    assert_eq!(
        composition(&api).squads[0].roles,
        BTreeMap::from([("back".into(), 1), ("qa".into(), 1)])
    );
    assert_eq!(composition(&api).staff, ["architect"]);
    assert_eq!(writes(&api, "qa"), Some(strings(&["**/*_test.*"])));
    assert_eq!(api.proposal.roles.len(), 1);
}

#[test]
fn the_answers_and_the_card_shape_the_proposal() {
    let (_, config, data) = setup("choices", &[("acme", MONOREPO)]);
    let items = library(&config, "acme").unwrap();
    let repos = crate::repos(&config, "acme", &test_globs(&items)).unwrap();
    let asked = questions(&repos, "en");
    let ids: Vec<&str> = asked.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, [SIZE, SPLIT, MODEL]);
    assert_eq!(asked[0].default, ["2"]);
    assert_eq!(asked[1].default, ["A squad per part: web and api"]);
    assert!(asked.iter().all(|q| crate::accepts(q, &q.default)));

    let answers = BTreeMap::from([
        (SPLIT.to_owned(), strings(&["One squad"])),
        (MODEL.to_owned(), strings(&["Quality: opus everywhere"])),
    ]);
    let (choices, _, edit) = propose(&repos, &roles(&items), &answered(&asked, &answers));
    assert_eq!(choices.split, Some(false));
    let squads = &edit.team.as_ref().unwrap().squads;
    assert_eq!(squads.len(), 1);
    assert_eq!(
        squads[0].roles,
        BTreeMap::from([
            ("back".into(), 2),
            ("devops".into(), 1),
            ("front".into(), 2),
            ("qa".into(), 1)
        ])
    );
    assert_eq!(edit.workflow.as_deref(), Some("default"));
    let models: Vec<&str> = edit
        .roles
        .iter()
        .filter(|(_, s)| s.model.as_deref() == Some("opus"))
        .map(|(r, _)| r.as_str())
        .collect();
    assert_eq!(
        models,
        ["back", "devops", "front", "qa"],
        "only the roles not on opus yet"
    );

    let card = Choices {
        keep: BTreeMap::from([("devops".into(), false), ("back".into(), false)]),
        size: Some(9),
        model: Some("sonnet".into()),
        ..Choices::default()
    };
    let shaped = team(&config, &data, "acme", card).unwrap();
    assert_eq!(shaped.choices.size, Some(3), "at most 3 at once");
    let devops = shaped.roles.iter().find(|r| r.role == "devops").unwrap();
    assert!(!devops.keep && devops.chosen && devops.why == "infra");
    let squads = &composition(&shaped).squads;
    assert_eq!(squads.len(), 1, "without back, the api squad is gone");
    assert_eq!(squads[0].id, "web");
    assert_eq!(
        squads[0].roles,
        BTreeMap::from([("front".into(), 3), ("qa".into(), 1)])
    );
    assert_eq!(shaped.proposal.workflow.as_deref(), Some("default"));
    assert!(!shaped.proposal.roles.contains_key("devops"));
    assert_eq!(
        shaped.proposal.roles["lead"].model.as_deref(),
        Some("sonnet")
    );
    assert_eq!(
        shaped.proposal.roles["front"].model, None,
        "front is on sonnet already"
    );
}

#[test]
fn writes_the_team_into_the_project_file_and_keeps_the_rest() {
    let (_, config, data) = setup("write", &[("acme", MONOREPO)]);
    let proposed = team(&config, &data, "acme", Choices::default()).unwrap();
    let shown = preview(&config, "acme", &proposed.proposal).unwrap();
    assert!(shown.path.ends_with("acme.toml"));
    assert!(
        shown.diff.contains("+[workflows]\n+default = \"squads\"\n"),
        "{}",
        shown.diff
    );
    assert!(
        shown.diff.contains(
            "+[[squads]]\n+id = \"web\"\n+lead = \"lead\"\n+roles = { front = 2, qa = 1 }\n"
        ),
        "{}",
        shown.diff
    );
    assert!(!shown.diff.contains("-"), "nothing removed: {}", shown.diff);

    let state = save(&config, &data, "acme", &proposed.proposal, false).unwrap();
    let text = fs::read_to_string(config.join("projects/acme.toml")).unwrap();
    assert!(
        text.starts_with("# Acme, by hand\nname = \"Acme\"\n"),
        "{text}"
    );
    assert!(
        text.contains("[trackers.github]\nissues = true\n"),
        "{text}"
    );
    let read = termf_core::squads::load_team(&config, "acme").unwrap();
    assert!(read.declared);
    assert_eq!(read.staff, ["architect", "ux"]);
    assert_eq!(read.squads[1].paths, composition(&proposed).squads[1].paths);
    assert_eq!(read.squads[0].count("front"), Some(2));
    let scope = Scope {
        project: Some("acme".into()),
        ..Scope::default()
    };
    let (_, settings) = termf_library::role_settings(&config, &scope, "qa").unwrap();
    let qa = settings.iter().find(|s| s.key == "writes").unwrap();
    assert_eq!(qa.value, json!(["**/tests/**", "**/*.test.*"]));
    assert_eq!(qa.from, ["project"]);

    assert_eq!(state.decisions["team"], Decision::Accepted);
    let logged = state.log.last().unwrap();
    assert_eq!(logged.what, "team");
    assert_eq!(
        logged.detail,
        "team, workflow, roles.back, roles.devops, roles.front, roles.qa"
    );
    let again = team(&config, &data, "acme", Choices::default()).unwrap();
    assert!(
        again.status.values().all(|s| *s == Status::Same),
        "{:?}",
        again.status
    );
    assert_eq!(preview(&config, "acme", &again.proposal).unwrap().diff, "");

    // Only what is accepted is written.
    let only = Edit {
        roles: BTreeMap::from([(
            "qa".into(),
            Settings {
                effort: Some("high".into()),
                ..Settings::default()
            },
        )]),
        ..Edit::default()
    };
    save(&config, &data, "acme", &only, false).unwrap();
    let text = fs::read_to_string(config.join("projects/acme.toml")).unwrap();
    assert!(text.contains("[roles.qa]\neffort = \"high\"\n"), "{text}");
    assert!(text.contains("[[squads]]"));
}

#[test]
fn refuses_what_would_not_read_back() {
    let (_, config, data) = setup("refuse", &[("billing", API)]);
    let base = team(&config, &data, "acme", Choices::default())
        .unwrap()
        .proposal;
    let refused = |edit: Edit, why: &str| {
        let e = format!("{:#}", preview(&config, "acme", &edit).unwrap_err());
        assert!(e.contains(why), "{why}: {e}");
        assert!(save(&config, &data, "acme", &edit, true).is_err());
    };
    let mut wizard = base.clone();
    wizard.team.as_mut().unwrap().squads[0]
        .roles
        .insert("wizard".into(), 1);
    refused(wizard, "no role named wizard");
    let mut spaced = base.clone();
    spaced.team.as_mut().unwrap().squads[0].id = "a b".into();
    refused(spaced, "is not made of");
    let mut flow = base.clone();
    flow.workflow = Some("nope".into());
    refused(flow, "no workflow named nope");
    for (settings, why) in [
        (
            Settings {
                effort: Some("huge".into()),
                ..Settings::default()
            },
            "no effort",
        ),
        (
            Settings {
                model: Some("opus 4".into()),
                ..Settings::default()
            },
            "is not a model",
        ),
        (
            Settings {
                writes: Some(strings(&["/etc/**"])),
                ..Settings::default()
            },
            "not a path inside",
        ),
        (
            Settings {
                writes: Some(strings(&["src/../../x"])),
                ..Settings::default()
            },
            "not a path inside",
        ),
    ] {
        refused(
            Edit {
                roles: BTreeMap::from([("qa".into(), settings)]),
                ..Edit::default()
            },
            why,
        );
    }
    let e = save(&config, &data, "acme", &Edit::default(), false).unwrap_err();
    assert!(e.to_string().contains("nothing to write"));
    assert!(
        !fs::read_to_string(config.join("projects/acme.toml"))
            .unwrap()
            .contains("squads")
    );
}

#[test]
fn a_new_proposal_respects_what_was_changed_by_hand() {
    let (dir, config, data) = setup("again", &[("acme", MONOREPO)]);
    let first = team(&config, &data, "acme", Choices::default()).unwrap();
    save(&config, &data, "acme", &first.proposal, false).unwrap();
    let file = config.join("projects/acme.toml");

    // By hand: three front agents, and a setting the assistant does not manage.
    let text = fs::read_to_string(&file).unwrap();
    let text = text.replace("front = 2, qa = 1", "front = 3, qa = 1")
        + "\n[roles.ux]\neffort_range = [\"medium\", \"high\"]\n";
    fs::write(&file, &text).unwrap();
    let now = team(&config, &data, "acme", Choices::default()).unwrap();
    assert_eq!(now.status["team"], Status::Edited);
    assert_eq!(now.status["workflow"], Status::Same);
    let e = save(&config, &data, "acme", &now.proposal, false).unwrap_err();
    assert!(
        e.to_string()
            .contains("team in the project's file was changed by hand since"),
        "{e}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), text, "nothing written");

    // A role delivered since, and a new package: proposed again, over the assistant's own writing only.
    let roles = config.join("library/roles/seo.md");
    put(
        &roles,
        "---\nname: seo\ndescription: SEO. Use for public pages\n---\nYou are the SEO.\n",
    );
    let acme = dir.join("acme");
    put(
        &acme.join("apps/admin/package.json"),
        r#"{ "name": "@acme/admin", "dependencies": { "react": "19" } }"#,
    );
    put(&acme.join("apps/web/public/robots.txt"), "");
    let later = team(&config, &data, "acme", Choices::default()).unwrap();
    assert!(kept(&later).contains(&("seo", true, "web")));
    assert_eq!(composition(&later).staff, ["architect", "seo", "ux"]);
    assert_eq!(
        composition(&later).squads[0].paths,
        ["apps/admin/**", "apps/web/**", "packages/ui/**"]
    );
    assert_eq!(
        later.status["team"],
        Status::Edited,
        "still the hand-made one"
    );
    assert_eq!(later.status["roles.front"], Status::Changed);
    let saved = save(&config, &data, "acme", &later.proposal, true).unwrap();
    assert!(saved.log.iter().filter(|l| l.what == "team").count() == 2);
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("front = 2, qa = 1"),
        "replaced, once asked to"
    );
    assert!(
        text.contains("effort_range = [\"medium\", \"high\"]"),
        "kept: not the assistant's"
    );
    let settled = team(&config, &data, "acme", Choices::default()).unwrap();
    assert!(
        settled.status.values().all(|s| *s == Status::Same),
        "{:?}",
        settled.status
    );
}
