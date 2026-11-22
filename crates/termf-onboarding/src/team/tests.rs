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

fn detail<'a>(team: &'a Team, role: &str) -> &'a [String] {
    &team.roles.iter().find(|r| r.role == role).unwrap().detail
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
    let simple = team(&config, &data, "acme", Choices::default(), true).unwrap();
    assert_eq!(
        kept(&simple),
        [
            ("a11y", true, "ui"),
            ("ai", false, "no-llm"),
            ("analytics", false, "no-analytics"),
            ("architect", true, "design"),
            ("back", false, "no-back"),
            ("brand", true, "ui"),
            ("copy", true, "ui"),
            ("dba", false, "no-data"),
            ("devops", true, "infra"),
            ("docs", false, "no-docs"),
            ("front", true, "front"),
            ("integrations", false, "no-integrations"),
            ("legal", true, "web"),
            ("motion", true, "web"),
            ("qa", true, "tests"),
            ("seo", true, "web"),
            ("sre", false, "no-service"),
            ("ux", true, "ui"),
        ]
    );
    for (role, why) in [
        ("front", ""),
        ("brand", "the interface"),
        ("a11y", "the interface"),
        ("motion", "the public site"),
        ("legal", "the public site"),
    ] {
        assert_eq!(detail(&simple, role), ["shop"], "{role}: {why}");
    }
    assert_eq!(
        simple.steps,
        [
            "cto",
            "integrator",
            "lead",
            "pm",
            "po",
            "reviewer",
            "secops"
        ]
    );
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
    assert_eq!(
        composition(&simple).staff,
        [
            "a11y",
            "architect",
            "brand",
            "copy",
            "legal",
            "motion",
            "seo",
            "ux"
        ]
    );
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
    let mono = team(&config, &data, "acme", Choices::default(), true).unwrap();
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
    assert_eq!(
        composition(&mono).staff,
        ["a11y", "architect", "brand", "copy", "docs", "sre", "ux"],
        "an interface but no public site: no motion"
    );
    assert!(kept(&mono).contains(&("motion", false, "no-web")));
    assert!(kept(&mono).contains(&("legal", false, "no-public")));
    assert_eq!(detail(&mono, "docs"), ["docs/"]);
    assert_eq!(
        detail(&mono, "sre"),
        [".github/workflows/", "infra/"],
        "a server and where it is deployed"
    );
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
    let cargo = team(&config, &data, "acme", Choices::default(), true).unwrap();
    assert_eq!(
        composition(&cargo).squads[0].roles,
        BTreeMap::from([("back".into(), 2), ("qa".into(), 1)])
    );
    assert_eq!(writes(&cargo, "back"), Some(strings(&["crates/**"])));
    assert!(
        kept(&cargo).contains(&("front", false, "no-front"))
            && kept(&cargo).contains(&("ux", false, "no-ui"))
            && kept(&cargo).contains(&("devops", false, "no-infra"))
            && kept(&cargo).contains(&("brand", false, "no-ui"))
            && kept(&cargo).contains(&("copy", false, "no-ui"))
            && kept(&cargo).contains(&("motion", false, "no-web"))
            && kept(&cargo).contains(&("a11y", false, "no-ui"))
            && kept(&cargo).contains(&("sre", false, "no-service"))
    );

    let (_, config, data) = setup("api", &[("billing", API)]);
    let api = team(&config, &data, "acme", Choices::default(), true).unwrap();
    assert_eq!(
        composition(&api).squads[0].roles,
        BTreeMap::from([("back".into(), 1), ("qa".into(), 1)])
    );
    assert_eq!(composition(&api).staff, ["architect"]);
    assert_eq!(writes(&api, "qa"), Some(strings(&["**/*_test.*"])));
    assert_eq!(api.proposal.roles.len(), 1);
}

/// A service with a database, a language model, analytics, payments, docs and a container, but no interface.
const SAAS: Files = &[
    (
        "package.json",
        r#"{ "name": "billing", "description": "an openai wrapper", "dependencies": { "express": "5", "@prisma/client": "6", "stripe": "17", "posthog-node": "4", "stripe-mock": "1" } }"#,
    ),
    ("prisma/schema.prisma", ""),
    ("prisma/migrations/20261101_init/migration.sql", ""),
    (
        "worker/requirements.txt",
        "Django==5.2\nanthropic[vertex]>=0.40\n# mixpanel, later\n",
    ),
    ("worker/requirements/ml.txt", "llama_index>=0.12\n"),
    (
        "worker/pyproject.toml",
        "[project]\nname = \"worker\"\ndescription = \"Segment and Mixpanel helpers\"\ndependencies = [\"tortoise_orm>=0.21\"]\n\
         [tool.poetry.group.dev.dependencies]\nsvix = \"^1\"\n",
    ),
    (
        "engine/Cargo.toml",
        "[package]\nname = \"engine\"\ndescription = \"OpenAI helpers\"\n\
         [dependencies]\npayments = { package = \"async-stripe\", version = \"0.39\" }\n",
    ),
    (
        "legacy/Gemfile",
        "source \"https://rubygems.org\"\n# gem \"openai\"\ngem \"pg\"\n",
    ),
    ("legacy/db/migrate/001_init.rb", ""),
    (
        "tools/go.mod",
        "module tools\n\nrequire github.com/stripe/stripe-go/v82 v82.0.0\n",
    ),
    ("docs/guide.md", ""),
    ("api/openapi.yaml", ""),
    ("CHANGELOG.md", ""),
    ("Dockerfile", "FROM node\n"),
];

#[test]
fn finds_what_calls_for_each_specialist() {
    let (_, config, data) = setup("saas", &[("billing", SAAS)]);
    let items = library(&config, "acme").unwrap();
    let repos = crate::repos(&config, "acme", &test_globs(&items)).unwrap();
    let signals = &repos[0].signals;
    assert_eq!(
        signals["data"],
        [
            "@prisma/client",
            "django",
            "legacy/db/migrate/",
            "prisma/migrations/",
            "prisma/schema.prisma",
            "tortoise-orm"
        ],
        "ORMs, migrations of three tools and a schema; PyPI names ignore case and take _ for -"
    );
    assert_eq!(
        signals["llm"],
        ["anthropic", "llama-index"],
        "a requirements folder too; descriptions and comments name no dependency"
    );
    assert_eq!(signals["analytics"], ["posthog-node"]);
    assert_eq!(
        signals["integrations"],
        [
            "async-stripe",
            "github.com/stripe/stripe-go",
            "stripe",
            "svix"
        ],
        "a renamed crate, a Go module before its major version, a Poetry group; stripe-mock is not stripe"
    );
    assert_eq!(
        signals["docs"],
        ["CHANGELOG.md", "api/openapi.yaml", "docs/"]
    );

    let saas = team(&config, &data, "acme", Choices::default(), true).unwrap();
    for (role, why) in [
        ("dba", "data"),
        ("ai", "llm"),
        ("analytics", "analytics"),
        ("integrations", "integrations"),
        ("docs", "docs"),
        ("sre", "service"),
        ("legal", "personal"),
    ] {
        assert!(kept(&saas).contains(&(role, true, why)), "{role}");
    }
    assert!(kept(&saas).contains(&("a11y", false, "no-ui")));
    assert_eq!(detail(&saas, "sre"), ["Dockerfile"]);
    assert_eq!(
        detail(&saas, "legal"),
        [
            "async-stripe",
            "github.com/stripe/stripe-go",
            "stripe",
            "svix",
            "posthog-node"
        ],
        "payments and analytics handle personal data"
    );
    assert_eq!(
        composition(&saas).staff,
        [
            "ai",
            "analytics",
            "architect",
            "dba",
            "docs",
            "integrations",
            "legal",
            "sre"
        ],
        "on the staff, never in a squad"
    );
    assert!(names("gem \"rails\", \"~> 8.0\"\n", "rails"));
    assert!(!names("gem \"rails-html-sanitizer\"\n", "rails"));
}

#[test]
fn names_each_marker_s_repository_when_there_are_several() {
    const DOTNET: Files = &[
        ("Ledger.csproj", "<Project Sdk=\"Microsoft.NET.Sdk\" />\n"),
        ("src/Data/Migrations/20261101120000_Init.cs", ""),
        ("Docs/index.md", ""),
    ];
    let (_, config, data) = setup("several", &[("billing", SAAS), ("ledger", DOTNET)]);
    let items = library(&config, "acme").unwrap();
    let repos = crate::repos(&config, "acme", &test_globs(&items)).unwrap();
    assert_eq!(
        repos[1].signals["data"],
        ["src/Data/Migrations/"],
        "Entity Framework's folder: names in any case"
    );
    assert_eq!(repos[1].signals["docs"], ["Docs/"]);

    let both = team(&config, &data, "acme", Choices::default(), true).unwrap();
    let dba = detail(&both, "dba");
    for marker in [
        "billing/prisma/migrations/",
        "ledger/src/Data/Migrations/",
        "django",
    ] {
        assert!(dba.contains(&marker.to_owned()), "{marker} in {dba:?}");
    }
    assert!(detail(&both, "docs").contains(&"ledger/Docs/".to_owned()));
}

#[test]
fn the_answers_and_the_card_shape_the_proposal() {
    let (_, config, data) = setup("choices", &[("acme", MONOREPO)]);
    let items = library(&config, "acme").unwrap();
    let repos = crate::repos(&config, "acme", &test_globs(&items)).unwrap();
    let asked = questions(&repos);
    let ids: Vec<&str> = asked.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, [SIZE, SPLIT, MODEL]);
    assert_eq!(asked[0].default, ["2"]);
    assert_eq!(asked[1].default, ["split"]);
    assert_eq!(
        (asked[2].text.as_str(), &asked[2].options),
        ("obTeamQModel", &strings(&["own", "sonnet", "opus"])),
        "the interface words them"
    );
    assert!(asked.iter().all(|q| crate::accepts(q, &q.default)));

    let answers = BTreeMap::from([
        (SPLIT.to_owned(), strings(&["one"])),
        (MODEL.to_owned(), strings(&["opus"])),
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
        [
            "a11y",
            "architect",
            "back",
            "brand",
            "copy",
            "cto",
            "devops",
            "docs",
            "expert",
            "front",
            "integrator",
            "lead",
            "pm",
            "po",
            "qa",
            "reviewer",
            "secops",
            "sre",
            "ux"
        ],
        "every kept role and every step, even those already on opus: a global setting could say otherwise"
    );

    let card = Choices {
        keep: BTreeMap::from([("devops".into(), false), ("back".into(), false)]),
        size: Some(9),
        model: Some("sonnet".into()),
        ..Choices::default()
    };
    let shaped = team(&config, &data, "acme", card, true).unwrap();
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
        shaped.proposal.roles["front"].model.as_deref(),
        Some("sonnet")
    );
}

#[test]
fn a_chosen_model_wins_over_a_global_setting() {
    let (_, config, data) = setup("global-model", &[("billing", API)]);
    put(
        &config.join("termf.toml"),
        "[roles.back]\nmodel = \"opus\"\n",
    );
    let card = Choices {
        model: Some("sonnet".into()),
        ..Choices::default()
    };
    let proposed = team(&config, &data, "acme", card, true).unwrap();
    save(&config, &data, "acme", &proposed.proposal, false).unwrap();
    let scope = Scope {
        project: Some("acme".into()),
        ..Scope::default()
    };
    let back = termf_library::role(&config, &scope, "back").unwrap();
    assert_eq!(
        back.meta["model"],
        json!("sonnet"),
        "the frontmatter's sonnet alone would lose to opus"
    );
}

#[test]
fn back_to_each_role_s_own_model_removes_what_the_assistant_wrote() {
    let (_, config, data) = setup("model-back", &[("acme", MONOREPO)]);
    let quality = Choices {
        model: Some("opus".into()),
        ..Choices::default()
    };
    let first = team(&config, &data, "acme", quality, true).unwrap();
    save(&config, &data, "acme", &first.proposal, false).unwrap();
    let file = config.join("projects/acme.toml");
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .contains("[roles.lead]\nmodel = \"opus\"")
    );

    let own = Choices {
        model: Some(String::new()),
        keep: BTreeMap::from([("devops".into(), false)]),
        ..Choices::default()
    };
    let back = team(&config, &data, "acme", own, false).unwrap();
    assert_eq!(back.proposal.roles["lead"], Settings::default());
    assert_eq!(
        back.proposal.roles["devops"],
        Settings::default(),
        "a role left out since"
    );
    assert_eq!(back.proposal.roles["qa"].model, None);
    assert_eq!(back.status["roles.lead"], Status::Changed);
    save(&config, &data, "acme", &back.proposal, false).unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        !text.contains("[roles.lead]") && !text.contains("[roles.devops]"),
        "{text}"
    );
    assert!(!text.contains("model = "), "{text}");
    assert!(text.contains("[roles.qa]\nwrites = "), "{text}");
}

#[test]
fn a_hand_written_part_already_in_place_takes_replace_to_change() {
    let (_, config, data) = setup("hand-same", &[("acme", MONOREPO)]);
    let file = config.join("projects/acme.toml");
    let text = fs::read_to_string(&file).unwrap() + "\n[workflows]\ndefault = \"squads\"\n";
    fs::write(&file, &text).unwrap();
    let proposed = team(&config, &data, "acme", Choices::default(), true).unwrap();
    assert_eq!(proposed.status["workflow"], Status::Same);
    assert_eq!(proposed.hand, ["workflow"]);
    let changed = Edit {
        workflow: Some("default".into()),
        ..Edit::default()
    };
    let e = save(&config, &data, "acme", &changed, false).unwrap_err();
    assert!(e.to_string().contains("changed by hand"), "{e}");
    save(&config, &data, "acme", &changed, true).unwrap();
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .contains("default = \"default\"")
    );
    let after = team(&config, &data, "acme", Choices::default(), false).unwrap();
    assert!(after.hand.is_empty(), "now the assistant's own");
}

#[test]
fn keeps_what_inline_tables_hold() {
    let (_, config, data) = setup("inline", &[("billing", API)]);
    let file = config.join("projects/acme.toml");
    let text = fs::read_to_string(&file).unwrap().replace(
        "name = \"Acme\"\n",
        "name = \"Acme\"\nworkflows = { default = \"hotfix\", note = \"by hand\" }\n\
         roles = { qa = { skills = [\"pytest\"], deny = [\"Bash(rm:*)\"] } }\n",
    );
    fs::write(&file, &text).unwrap();
    let proposed = team(&config, &data, "acme", Choices::default(), true).unwrap();
    assert_eq!(proposed.status["workflow"], Status::Edited);
    save(&config, &data, "acme", &proposed.proposal, true).unwrap();
    let back: toml::Table = toml::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(back["workflows"]["default"].as_str(), Some("default"));
    assert_eq!(back["workflows"]["note"].as_str(), Some("by hand"));
    let qa = &back["roles"]["qa"];
    assert_eq!(qa["skills"], toml::Value::Array(vec!["pytest".into()]));
    assert_eq!(qa["deny"], toml::Value::Array(vec!["Bash(rm:*)".into()]));
    assert_eq!(qa["writes"], toml::Value::Array(vec!["**/*_test.*".into()]));
}

#[test]
fn writes_the_team_into_the_project_file_and_keeps_the_rest() {
    let (_, config, data) = setup("write", &[("acme", MONOREPO)]);
    let proposed = team(&config, &data, "acme", Choices::default(), true).unwrap();
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
    assert_eq!(
        read.staff,
        ["a11y", "architect", "brand", "copy", "docs", "sre", "ux"]
    );
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
    let again = team(&config, &data, "acme", Choices::default(), true).unwrap();
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
    let base = team(&config, &data, "acme", Choices::default(), true)
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
    let mut elsewhere = base.clone();
    elsewhere.team.as_mut().unwrap().squads[0].repos = strings(&["billing", "front"]);
    refused(elsewhere, "squad main: no repository front in the project");
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
    let first = team(&config, &data, "acme", Choices::default(), true).unwrap();
    save(&config, &data, "acme", &first.proposal, false).unwrap();
    let file = config.join("projects/acme.toml");

    // By hand: three front agents, and a setting the assistant does not manage.
    let text = fs::read_to_string(&file).unwrap();
    let text = text.replace("front = 2, qa = 1", "front = 3, qa = 1")
        + "\n[roles.ux]\neffort_range = [\"medium\", \"high\"]\n";
    fs::write(&file, &text).unwrap();
    let now = team(&config, &data, "acme", Choices::default(), true).unwrap();
    assert_eq!(now.status["team"], Status::Edited);
    assert_eq!(now.status["workflow"], Status::Same);
    let e = save(&config, &data, "acme", &now.proposal, false).unwrap_err();
    assert!(
        e.to_string()
            .contains("team in the project's file was changed by hand since"),
        "{e}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), text, "nothing written");

    // A role that appeared since, a public site and a new package: proposed again, over the assistant's own
    // writing only.
    put(
        &config.join("library/roles/translator.md"),
        "---\nname: translator\ndescription: Translator. Use for the translations\n---\nYou translate.\n",
    );
    let acme = dir.join("acme");
    put(
        &acme.join("apps/admin/package.json"),
        r#"{ "name": "@acme/admin", "dependencies": { "react": "19" } }"#,
    );
    put(&acme.join("apps/web/public/robots.txt"), "");
    let kept_survey = team(&config, &data, "acme", Choices::default(), false).unwrap();
    assert!(
        !composition(&kept_survey).squads[0]
            .paths
            .contains(&"apps/admin/**".into()),
        "a choice in the card reuses the last survey"
    );
    let later = team(&config, &data, "acme", Choices::default(), true).unwrap();
    assert!(kept(&later).contains(&("translator", true, "own")));
    assert!(kept(&later).contains(&("seo", true, "web")));
    assert_eq!(
        composition(&later).staff,
        [
            "a11y",
            "architect",
            "brand",
            "copy",
            "docs",
            "legal",
            "motion",
            "seo",
            "sre",
            "translator",
            "ux"
        ]
    );
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
    let settled = team(&config, &data, "acme", Choices::default(), true).unwrap();
    assert!(
        settled.status.values().all(|s| *s == Status::Same),
        "{:?}",
        settled.status
    );
}
