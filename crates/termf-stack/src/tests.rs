use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::bail;
use serde_json::{Value, json};

use crate::deps::{self, Dep, Ecosystem};
use crate::map::{self, LinkKind};
use crate::remote::{self, Request, Severity};
use crate::version::{Bump, best, bump, compare};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("termf-stack-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

#[test]
fn each_registry_lists_its_versions_without_the_yanked_ones() {
    let packagist = br#"{"packages": {"laravel/framework": [{"version": "v12.10.0"}, {"version": "v10.48.29"}]}}"#;
    assert_eq!(
        remote::versions(Ecosystem::Packagist, "laravel/framework", packagist),
        ["12.10.0", "10.48.29"]
    );
    let crates = b"{\"name\":\"serde\",\"vers\":\"1.0.1\",\"yanked\":false}\n{\"name\":\"serde\",\"vers\":\"1.0.2\",\"yanked\":true}\n";
    assert_eq!(
        remote::versions(Ecosystem::Crates, "serde", crates),
        ["1.0.1"]
    );
    assert_eq!(
        remote::versions(Ecosystem::Go, "x", b"v1.9.0\nv1.10.0\n"),
        ["v1.9.0", "v1.10.0"]
    );
    let gems = br#"[{"number": "7.1.3"}, {"number": "7.2.0.rc1"}]"#;
    assert_eq!(
        remote::versions(Ecosystem::RubyGems, "rails", gems),
        ["7.1.3", "7.2.0.rc1"]
    );
}

#[test]
fn versions_compare_across_ecosystems() {
    use std::cmp::Ordering::*;
    for (a, b, want) in [
        ("1.10.0", "1.9.3", Greater),
        ("v1.2.3", "1.2.3", Equal),
        ("1.2", "1.2.0", Equal),
        ("2.0.0-rc.1", "2.0.0", Less),
        ("2.0.0-rc.10", "2.0.0-rc.2", Greater),
        ("2.0.0-alpha", "2.0.0-beta", Less),
        // PEP 440.
        ("2.0rc1", "2.0", Less),
        ("1.2.3.post1", "1.2.3", Greater),
        ("2.0b1", "2.0a2", Greater),
        // RubyGems and Composer.
        ("7.1.0.rc1", "7.1.0", Less),
        ("v6.4.1", "6.4.10", Less),
        // Go: `+incompatible` is build metadata, a pseudo-version is a pre-release.
        ("v2.1.0+incompatible", "2.1.0", Equal),
        ("v0.0.0-20230101120000-abcdef123456", "0.0.1", Less),
    ] {
        assert_eq!(compare(a, b), Some(want), "{a} vs {b}");
    }
    assert_eq!(compare("dev-main", "1.0.0"), None);
}

#[test]
fn an_update_is_a_patch_a_minor_or_a_major() {
    assert_eq!(bump("1.2.3", "1.2.4"), Some(Bump::Patch));
    assert_eq!(bump("1.2.3", "1.4.0"), Some(Bump::Minor));
    assert_eq!(bump("1.2.3", "2.0.0"), Some(Bump::Major));
    assert_eq!(bump("1.2.3", "1.2.3"), None);
    assert_eq!(bump("1.2.3", "1.2.2"), None);
    // Under 0.x a minor breaks, under 0.0.x a patch does.
    assert_eq!(bump("0.3.1", "0.3.4"), Some(Bump::Patch));
    assert_eq!(bump("0.3.1", "0.4.0"), Some(Bump::Major));
    assert_eq!(bump("0.0.3", "0.0.4"), Some(Bump::Major));
    assert_eq!(bump("v1.9.0", "v1.10.0"), Some(Bump::Minor));
    assert_eq!(bump("2.0.0-rc.1", "2.0.0"), Some(Bump::Patch));

    let versions = [
        "17.0.2",
        "17.0.1",
        "18.2.0",
        "18.3.1",
        "19.0.0-rc.1",
        "16.14.0",
    ];
    assert_eq!(
        best(versions, Some("17.0.1")),
        (Some("18.3.1".into()), Some("17.0.2".into()))
    );
    assert_eq!(
        best(versions, Some("18.3.1")),
        (Some("18.3.1".into()), None)
    );
    assert_eq!(
        best(["0.3.1", "0.3.9", "0.4.2"], Some("0.3.1"))
            .1
            .as_deref(),
        Some("0.3.9")
    );
}

fn dep(name: &str, eco: Ecosystem, version: &str) -> Dep {
    Dep {
        name: name.into(),
        ecosystem: eco,
        manager: "npm".into(),
        range: format!("^{version}"),
        version: Some(version.into()),
        locked: true,
        dev: false,
        manifest: "package.json".into(),
    }
}

/// OSV's documented shapes: a batch with ids only, details by id.
fn osv_answers() -> (Value, HashMap<String, Value>) {
    let batch = json!({"results": [
        {"vulns": [{"id": "GHSA-35jh-r3h4-6jhm"}, {"id": "CVE-2021-23337"}]},
        {},
        {"vulns": [{"id": "PYSEC-2023-74"}]},
    ]});
    let details = HashMap::from([
        (
            "GHSA-35jh-r3h4-6jhm".to_owned(),
            json!({
                "id": "GHSA-35jh-r3h4-6jhm",
                "summary": "Command Injection in lodash",
                "aliases": ["CVE-2021-23337"],
                "database_specific": {"severity": "HIGH"},
                "affected": [{"package": {"ecosystem": "npm", "name": "lodash"},
                    "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "4.17.21"}]}]}],
            }),
        ),
        (
            "CVE-2021-23337".to_owned(),
            json!({"id": "CVE-2021-23337", "details": "lodash versions prior to 4.17.21 are vulnerable", "aliases": ["GHSA-35jh-r3h4-6jhm"]}),
        ),
        (
            "PYSEC-2023-74".to_owned(),
            json!({
                "id": "PYSEC-2023-74",
                "details": "Requests leaks Proxy-Authorization headers",
                "aliases": ["CVE-2023-32681", "GHSA-j8r2-6x86-q33q"],
                "affected": [{"package": {"ecosystem": "PyPI", "name": "requests"},
                    "ranges": [
                        {"type": "ECOSYSTEM", "events": [{"introduced": "2.3.0"}, {"fixed": "2.31.0"}]},
                        {"type": "GIT", "events": [{"introduced": "0"}, {"fixed": "74ea7cf"}]}
                    ]}],
            }),
        ),
    ]);
    (batch, details)
}

#[test]
fn osv_answers_are_matched_to_their_dependencies() {
    let deps = [
        dep("lodash", Ecosystem::Npm, "4.17.20"),
        dep("react", Ecosystem::Npm, "18.2.0"),
        dep("requests", Ecosystem::PyPI, "2.28.1"),
    ];
    let queried: Vec<&Dep> = deps.iter().collect();
    let (batch, details) = osv_answers();
    let matched = remote::match_osv(&queried, &batch, &details);
    assert_eq!(matched.len(), 3);
    // The GHSA and its CVE are one vulnerability, named by GitHub's advisory, with its level.
    assert_eq!(matched[0].len(), 1);
    let lodash = &matched[0][0];
    assert_eq!(lodash.id, "GHSA-35jh-r3h4-6jhm");
    assert!(lodash.aliases.contains(&"CVE-2021-23337".to_owned()));
    assert_eq!(lodash.severity, Severity::High);
    assert_eq!(lodash.fixed.as_deref(), Some("4.17.21"));
    assert_eq!(lodash.summary, "Command Injection in lodash");
    assert!(matched[1].is_empty());
    // Without a level: unknown; its fix is the end of the range the version is in, git commits aside.
    let requests = &matched[2][0];
    assert_eq!(requests.severity, Severity::Unknown);
    assert_eq!(requests.fixed.as_deref(), Some("2.31.0"));
    assert_eq!(
        requests.summary,
        "Requests leaks Proxy-Authorization headers"
    );
}

#[test]
fn manifests_and_lockfiles_give_each_dependency_its_version() {
    let repo = scratch("deps");
    write(
        &repo,
        &[
            (
                "package.json",
                r#"{"name": "@acme/web", "dependencies": {"react": "^18.2.0", "@acme/sdk": "workspace:*"},
                    "devDependencies": {"vite": "^5.0.0"}}"#,
            ),
            (
                "pnpm-lock.yaml",
                "lockfileVersion: '9.0'\nimporters:\n  .:\n    dependencies:\n      react:\n        specifier: ^18.2.0\n        version: 18.3.1\n    devDependencies:\n      vite:\n        specifier: ^5.0.0\n        version: 5.4.2(@types/node@20.1.0)\n",
            ),
            (
                "api/composer.json",
                r#"{"name": "Acme/Api", "require": {"php": "^8.2", "ext-json": "*", "laravel/framework": "^10.0"}}"#,
            ),
            (
                "api/composer.lock",
                r#"{"packages": [{"name": "laravel/framework", "version": "v10.48.4"}], "packages-dev": []}"#,
            ),
            (
                "worker/Cargo.toml",
                "[package]\nname = \"worker\"\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nlocal = { path = \"../local\" }\n[dev-dependencies]\ntokio = \"1.40\"\n",
            ),
            (
                "worker/Cargo.lock",
                "[[package]]\nname = \"serde\"\nversion = \"1.0.210\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n[[package]]\nname = \"tokio\"\nversion = \"1.41.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n",
            ),
            (
                "ml/requirements.txt",
                "Requests==2.28.1\nnumpy>=1.26  # math\n-r other.txt\n",
            ),
            (
                "svc/go.mod",
                "module github.com/acme/svc\n\ngo 1.22\n\nrequire (\n\tgithub.com/gin-gonic/gin v1.9.1\n\tgolang.org/x/text v0.14.0 // indirect\n)\n",
            ),
            (
                "rb/Gemfile",
                "gem \"rails\", \"~> 7.1.0\"\ngroup :development, :test do\n  gem 'rspec-rails'\nend\n",
            ),
            (
                "rb/Gemfile.lock",
                "GEM\n  remote: https://rubygems.org/\n  specs:\n    rails (7.1.3)\n      actionpack (= 7.1.3)\n    rspec-rails (6.1.0)\n\nPLATFORMS\n  ruby\n",
            ),
        ],
    );
    let found = deps::read(&repo);
    let get = |name: &str| {
        found
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    assert!(!found.iter().any(|d| d.name == "@acme/sdk"
        || d.name == "local"
        || d.name == "php"
        || d.name == "ext-json"));
    assert_eq!(
        (
            get("react").version.as_deref(),
            get("react").manager.as_str(),
            get("react").dev
        ),
        (Some("18.3.1"), "pnpm", false)
    );
    assert_eq!(
        (get("vite").version.as_deref(), get("vite").dev),
        (Some("5.4.2"), true)
    );
    assert_eq!(get("laravel/framework").version.as_deref(), Some("10.48.4"));
    assert_eq!(get("laravel/framework").manifest, "api/composer.json");
    assert_eq!(get("serde").version.as_deref(), Some("1.0.210"));
    assert_eq!(
        (get("tokio").version.as_deref(), get("tokio").dev),
        (Some("1.41.0"), true)
    );
    assert_eq!(
        (
            get("requests").version.as_deref(),
            get("requests").manager.as_str()
        ),
        (Some("2.28.1"), "pip")
    );
    assert_eq!(
        (get("numpy").version.as_deref(), get("numpy").locked),
        (Some("1.26"), false)
    );
    assert_eq!(
        get("github.com/gin-gonic/gin").version.as_deref(),
        Some("v1.9.1")
    );
    assert!(!found.iter().any(|d| d.name == "golang.org/x/text"));
    assert_eq!(get("rails").version.as_deref(), Some("7.1.3"));
    assert!(get("rspec-rails").dev);
    let _ = fs::remove_dir_all(&repo);
}

#[test]
fn repositories_are_linked_by_calls_packages_and_databases() {
    let root = scratch("links");
    let (users, orders, sdk) = (root.join("users"), root.join("orders"), root.join("sdk"));
    write(
        &users,
        &[
            (
                "package.json",
                r#"{"name": "users", "dependencies": {"@acme/sdk": "^1.0.0"}}"#,
            ),
            (
                "compose.yaml",
                "services:\n  users:\n    build: .\n    ports:\n      - \"${USERS_PORT:-3001}:3000\"\n  db:\n    image: postgres:16\n    ports: [\"5432:5432\"]\n",
            ),
            (
                ".env.example",
                "PORT=3001\nDATABASE_URL=postgres://app:secret@localhost:5432/acme\n",
            ),
        ],
    );
    write(
        &orders,
        &[
            (
                "package.json",
                r#"{"name": "orders", "dependencies": {"@acme/sdk": "^1.2.0"}}"#,
            ),
            (
                ".env.example",
                "USERS_API_URL=http://localhost:3001/v1\nBILLING_URL=https://billing.example.com\nDATABASE_URL=postgres://app:other@127.0.0.1:5432/acme\n",
            ),
        ],
    );
    write(
        &sdk,
        &[(
            "package.json",
            r#"{"name": "@acme/sdk", "version": "1.2.0"}"#,
        )],
    );
    let found = map::map(&[
        ("users".into(), users.clone()),
        ("orders".into(), orders.clone()),
        ("sdk".into(), sdk.clone()),
    ]);
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let users_map = &found.repos[0];
    assert_eq!(users_map.ports, vec![3001, 5432]);
    assert_eq!(users_map.env, vec!["PORT", "DATABASE_URL"]);
    assert!(
        users_map
            .databases
            .iter()
            .any(|d| d.name == "acme" && d.source.contains("DATABASE_URL")),
        "{:?}",
        users_map.databases
    );
    let has = |from: &Path, to: &Path, kind: LinkKind, via: &str| {
        found.links.iter().any(|l| {
            l.from == path(from)
                && l.to == path(to)
                && l.kind == kind
                && l.via.iter().any(|v| v == via)
        })
    };
    assert!(
        has(&orders, &users, LinkKind::Calls, "USERS_API_URL"),
        "{:?}",
        found.links
    );
    assert!(has(&users, &sdk, LinkKind::Package, "@acme/sdk"));
    assert!(has(&orders, &sdk, LinkKind::Package, "@acme/sdk"));
    let (a, b) = if path(&users) < path(&orders) {
        (&users, &orders)
    } else {
        (&orders, &users)
    };
    assert!(has(a, b, LinkKind::Database, "acme"));
    // Nothing says the outside service is one of them, and nobody calls the SDK.
    assert_eq!(found.links.len(), 4, "{:?}", found.links);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_check_asks_once_then_reads_the_cache_and_says_when_offline() {
    let root = scratch("check");
    let cache = root.join("cache.json");
    let deps = [
        Dep {
            manager: "npm".into(),
            ..dep("lodash", Ecosystem::Npm, "4.17.20")
        },
        Dep {
            manager: "pip".into(),
            ..dep("requests", Ecosystem::PyPI, "2.28.1")
        },
    ];
    let tags = [termf_library::stack::Tag {
        name: "node".into(),
        version: Some("18.19.0".into()),
        source: ".nvmrc".into(),
    }];
    let asked = Mutex::new(Vec::<Request>::new());
    let (batch, details) = osv_answers();
    let fake = |r: &Request| -> anyhow::Result<Option<Vec<u8>>> {
        asked.lock().unwrap().push(r.clone());
        let url = r.url.as_str();
        let body = match url {
            "https://registry.npmjs.org/lodash" => {
                json!({"versions": {"4.17.20": {}, "4.17.21": {}, "5.0.0-rc.1": {}}})
            }
            "https://pypi.org/pypi/requests/json" => {
                json!({"releases": {"2.28.1": [{}], "2.32.3": [{}], "3.0.0": [{"yanked": true}]}})
            }
            "https://api.osv.dev/v1/querybatch" => {
                let sent: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
                // Names and versions only.
                assert_eq!(
                    sent["queries"][0],
                    json!({"package": {"ecosystem": "npm", "name": "lodash"}, "version": "4.17.20"})
                );
                json!({"results": [batch["results"][0], batch["results"][2]]})
            }
            "https://endoflife.date/api/nodejs.json" => json!([
                {"cycle": "22", "lts": "2024-10-29", "eol": "2027-04-30", "latest": "22.11.0"},
                {"cycle": "20", "lts": "2023-10-24", "eol": "2026-04-30", "latest": "20.18.0"},
                {"cycle": "18", "lts": "2022-10-25", "eol": "2025-04-30", "latest": "18.20.4"},
            ]),
            _ => match url.strip_prefix("https://api.osv.dev/v1/vulns/") {
                Some(id) => details[id].clone(),
                None => return Ok(None),
            },
        };
        Ok(Some(body.to_string().into_bytes()))
    };
    let checked = remote::check(&deps, &tags, &cache, false, &fake, "2026-11-27");
    assert!(
        !checked.offline && checked.errors.is_empty(),
        "{:?}",
        checked.errors
    );
    let lodash = &checked.deps[0];
    assert_eq!(
        (
            lodash.latest.as_deref(),
            lodash.safe.as_deref(),
            lodash.bump
        ),
        (Some("4.17.21"), Some("4.17.21"), Some(Bump::Patch))
    );
    assert_eq!(
        lodash.command.as_deref(),
        Some("npm install lodash@4.17.21")
    );
    assert_eq!(lodash.vulns[0].fixed.as_deref(), Some("4.17.21"));
    assert_eq!(
        (lodash.safe_bump, lodash.safe_fixes.clone()),
        (Some(Bump::Patch), vec!["GHSA-35jh-r3h4-6jhm".to_owned()])
    );
    let requests = &checked.deps[1];
    assert_eq!(
        (
            requests.latest.as_deref(),
            requests.bump,
            requests.command.as_deref()
        ),
        (Some("2.32.3"), Some(Bump::Minor), None)
    );
    assert_eq!(requests.vulns.len(), 1);
    let node = &checked.support[0];
    assert_eq!(
        (
            node.cycle.as_deref(),
            node.eol.as_deref(),
            node.ended,
            node.lts
        ),
        (Some("18"), Some("2025-04-30"), true, true)
    );
    assert_eq!(
        (node.current_lts.as_deref(), node.latest.as_deref()),
        (Some("22"), Some("22.11.0"))
    );
    let first = asked.lock().unwrap().len();
    assert!(first >= 5);

    // Within a day, nothing is asked again; offline and refreshing, the older answers still show.
    let again = remote::check(&deps, &tags, &cache, false, &fake, "2026-11-27");
    assert_eq!(asked.lock().unwrap().len(), first);
    assert_eq!(again.deps, checked.deps);
    let offline = |_: &Request| -> anyhow::Result<Option<Vec<u8>>> {
        bail!("cannot reach registry.npmjs.org: Could not resolve host")
    };
    let stale = remote::check(&deps, &tags, &cache, true, &offline, "2026-11-27");
    assert!(stale.offline);
    assert_eq!(stale.errors.len(), 1);
    assert_eq!(stale.deps, checked.deps);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_update_command_is_typed_only_for_plain_names() {
    let mut d = dep("@types/node", Ecosystem::Npm, "20.1.0");
    d.dev = true;
    assert_eq!(
        remote::update_command(&d, "20.17.6").as_deref(),
        Some("npm install @types/node@20.17.6 --save-dev")
    );
    d.name = "evil; rm -rf ~".into();
    assert_eq!(remote::update_command(&d, "1.0.0"), None);
    let mut c = dep("serde", Ecosystem::Crates, "1.0.210");
    c.manager = "cargo".into();
    assert_eq!(
        remote::update_command(&c, "1.0.215").as_deref(),
        Some("cargo update -p serde@1.0.210 --precise 1.0.215")
    );
    c.manager = "composer".into();
    c.name = "laravel/framework".into();
    assert_eq!(remote::update_command(&c, "$(id)"), None);
}
