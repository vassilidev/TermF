use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::time::{Duration, Instant};

use super::*;
use crate::database::{parse_env, read, set_env, with_database};
use crate::files::included;

/// Throwaway config, data, repositories and fake programs, and a git that reads none of the user's config.
struct Sandbox {
    root: PathBuf,
    env: Vec<(String, String)>,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let root =
            std::env::temp_dir().join(format!("termf-worktree-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for dir in ["config/projects", "data", "bin", "repos", "worktrees"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("gitconfig"), "").unwrap();
        let gitconfig = root.join("gitconfig").to_string_lossy().into_owned();
        let mut env: Vec<(String, String)> = [
            ("GIT_CONFIG_GLOBAL", gitconfig.as_str()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_AUTHOR_NAME", "t"),
            ("GIT_AUTHOR_EMAIL", "t@t"),
            ("GIT_COMMITTER_NAME", "t"),
            ("GIT_COMMITTER_EMAIL", "t@t"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        // The fakes first, then the usual tools.
        let usual = std::env::var_os("PATH").unwrap_or_default();
        let paths = std::iter::once(root.join("bin")).chain(std::env::split_paths(&usual));
        let path = std::env::join_paths(paths).unwrap();
        env.push(("PATH".into(), path.to_string_lossy().into_owned()));
        Sandbox { root, env }
    }

    fn config(&self) -> PathBuf {
        self.root.join("config")
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A plain folder with these files.
    fn dir(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = self.root.join("repos").join(name);
        for (file, text) in files {
            let path = dir.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A repository with these files, committed but for those its `.gitignore` names.
    fn repo(&self, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let dir = self.dir(name, files);
        self.git(&dir, &["init", "-q", "-b", "main"]);
        self.git(&dir, &["add", "-A"]);
        self.git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        dir
    }

    fn project(&self, id: &str, repos: &[&Path]) {
        let mut text = format!("name = \"{id}\"\n");
        for repo in repos {
            text.push_str(&format!("[[repos]]\npath = {:?}\n", repo.to_string_lossy()));
        }
        fs::write(self.root.join(format!("config/projects/{id}.toml")), text).unwrap();
    }

    fn worktree(&self, repo: &Path, name: &str) -> PathBuf {
        let path = self.root.join("worktrees").join(name);
        self.git(
            repo,
            &["worktree", "add", "-q", "-b", name, &path.to_string_lossy()],
        );
        path
    }

    fn setup(&self) -> Setup {
        Setup {
            config_dir: self.config(),
            data_dir: self.root.join("data"),
            env: self.env.clone(),
            claude: vec![self.root.join("bin/claude").to_string_lossy().into_owned()],
        }
    }

    #[cfg(unix)]
    fn fake(&self, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let file = self.root.join("bin").join(name);
        fs::write(&file, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    fn log(&self) -> String {
        fs::read_to_string(self.root.join("calls.log")).unwrap_or_default()
    }
}

#[cfg(unix)]
fn find<'a>(report: &'a Report, kind: StepKind, target: &str) -> &'a Step {
    report
        .steps
        .iter()
        .find(|s| s.kind == kind && s.target == target)
        .unwrap_or_else(|| panic!("no {kind:?} step on {target} in {:#?}", report.steps))
}

fn one(report: &Report, kind: StepKind) -> &Step {
    report
        .steps
        .iter()
        .find(|s| s.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} step in {:#?}", report.steps))
}

#[cfg(unix)]
/// Whether this file system clones copy-on-write.
fn cow_here(sb: &Sandbox) -> bool {
    let probe = sb.root.join("probe");
    fs::write(&probe, "x").unwrap();
    cow::clone(&probe, &sb.root.join("probe-clone")).is_ok()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn detects_each_stack() {
    let sb = Sandbox::new("detect");
    let shop = sb.repo(
        "shop",
        &[
            ("composer.json", "{}"),
            ("artisan", ""),
            ("package.json", "{}"),
            ("pnpm-lock.yaml", ""),
            ("vite.config.ts", ""),
            (".gitignore", "/vendor\n/node_modules\n.env\nauth.json\n"),
            (
                ".env",
                "APP_NAME=Shop\nDB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_DATABASE=shop\n",
            ),
            (".env.example", "DB_CONNECTION=mysql\n"),
            ("auth.json", "{}"),
            (".worktreeinclude", "# local keys\nstorage/*.key\n.env\n"),
        ],
    );
    let r = detect(&shop, &sb.env);
    assert_eq!(
        r.stack,
        ["php:composer", "laravel", "node:pnpm", "vite", "mysql"]
    );
    assert_eq!(
        r.include,
        ["storage/*.key", ".env", "auth.json"],
        ".worktreeinclude first, then the ignored local files; a tracked example is not one"
    );
    assert_eq!(r.database.as_deref(), Some(".env"));
    let dirs: Vec<(&str, bool)> = r.deps.iter().map(|d| (d.dir.as_str(), d.clone)).collect();
    assert_eq!(dirs, [("vendor", true), ("node_modules", true)]);
    assert_eq!(
        r.deps[1].install,
        [
            "pnpm install --frozen-lockfile --offline",
            "pnpm install --frozen-lockfile"
        ]
    );
    assert_eq!(r.deps[0].install.len(), 2, "composer offline, then online");

    type Case<'a> = (
        &'a str,
        &'a [(&'a str, &'a str)],
        &'a [&'a str],
        &'a [(&'a str, bool, &'a str)],
    );
    let cases: &[Case] = &[
        (
            "npm",
            &[("package.json", "{}"), ("package-lock.json", "{}")],
            &["node:npm"],
            &[(
                "node_modules",
                true,
                "npm ci --offline --no-audit --no-fund",
            )],
        ),
        (
            "berry",
            &[
                ("package.json", "{}"),
                ("yarn.lock", ""),
                (".yarnrc.yml", ""),
            ],
            &["node:yarn"],
            &[("node_modules", true, "yarn install --immutable")],
        ),
        (
            "bun",
            &[("package.json", "{}"), ("bun.lockb", "")],
            &["node:bun"],
            &[("node_modules", true, "bun install --frozen-lockfile")],
        ),
        (
            "declared",
            &[("package.json", r#"{"packageManager":"pnpm@9.1.0"}"#)],
            &["node:pnpm"],
            &[(
                "node_modules",
                true,
                "pnpm install --prefer-offline --no-lockfile",
            )],
        ),
        (
            "uv",
            &[("pyproject.toml", ""), ("uv.lock", "")],
            &["python:uv"],
            &[(".venv", false, "uv sync --frozen --offline")],
        ),
        (
            "poetry",
            &[("pyproject.toml", ""), ("poetry.lock", "")],
            &["python:poetry"],
            &[("", false, "poetry install --no-interaction")],
        ),
        (
            "pip",
            &[("requirements.txt", "")],
            &["python:pip"],
            &[(
                ".venv",
                false,
                "python3 -m venv .venv && .venv/bin/pip install -r requirements.txt",
            )],
        ),
        (
            "rails",
            &[
                ("Gemfile", ""),
                ("config/application.rb", ""),
                (
                    "config/database.yml",
                    "development:\n  adapter: postgresql\n  database: blog_dev\n",
                ),
            ],
            &["ruby:bundler", "rails", "postgres"],
            &[("", false, "bundle install --local")],
        ),
        (
            "go",
            &[("go.mod", "")],
            &["go"],
            &[("", false, "go mod download")],
        ),
        (
            "rust",
            &[("Cargo.toml", "")],
            &["rust"],
            &[("target", true, "")],
        ),
    ];
    for (name, files, stack, deps) in cases {
        let r = detect(&sb.dir(name, files), &sb.env);
        assert_eq!(r.stack, *stack, "{name}");
        let found: Vec<(&str, bool, &str)> = r
            .deps
            .iter()
            .map(|d| {
                let first = d.install.first().map_or("", String::as_str);
                (d.dir.as_str(), d.clone, first)
            })
            .collect();
        assert_eq!(found, *deps, "{name}");
    }
    let rails = detect(&sb.root.join("repos/rails"), &sb.env);
    assert_eq!(rails.database.as_deref(), Some("config/database.yml"));

    let compose = "services:\n  web:\n    build: .\n    ports: ['8080:80']\n  vite-dev:\n    ports:\n      - '5173:5173'\n  db:\n    image: postgres:16\n";
    let r = detect(
        &sb.dir("compose", &[("docker-compose.yml", compose)]),
        &sb.env,
    );
    assert_eq!(r.stack, ["compose"]);
    assert!(r.compose);
    assert_eq!(
        r.ports,
        ["web", "vite_dev"],
        "the services that publish a port"
    );

    let unknown = detect(&sb.dir("unknown", &[("README.md", "# hi")]), &sb.env);
    assert_eq!(
        unknown,
        RepoRecipe::default(),
        "nothing detected: Claude proposes"
    );
}

#[test]
fn reads_and_rewrites_env_files() {
    let text = "# shop\nexport APP_NAME=\"My Shop\"\nDB_HOST=127.0.0.1 # local\nDB_DATABASE='shop'\nDB_PASSWORD=\n";
    let vars = parse_env(text);
    let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(
        pairs,
        [
            ("APP_NAME", "My Shop"),
            ("DB_HOST", "127.0.0.1"),
            ("DB_DATABASE", "shop"),
            ("DB_PASSWORD", "")
        ]
    );
    assert_eq!(
        set_env(text, "DB_DATABASE", "shop_termf_7"),
        "# shop\nexport APP_NAME=\"My Shop\"\nDB_HOST=127.0.0.1 # local\nDB_DATABASE='shop_termf_7'\nDB_PASSWORD=\n",
        "only the value changes, quotes and comments stay"
    );
    assert_eq!(set_env("A=1", "B", "x y"), "A=1\nB=\"x y\"\n");
    assert_eq!(
        with_database(
            "postgres://u:p%40ss@localhost:5432/app?sslmode=disable",
            "app_termf_7"
        ),
        "postgres://u:p%40ss@localhost:5432/app_termf_7?sslmode=disable"
    );

    let sb = Sandbox::new("env");
    let conn = |text: &str| {
        let dir = sb.dir("conn", &[(".env", text), ("config/database.yml", text)]);
        let from = if text.contains("development:") {
            "config/database.yml"
        } else {
            ".env"
        };
        read(&dir, from)
    };
    let pg = conn("DATABASE_URL=\"postgresql://me:p%40ss@[::1]:5433/app\"\n").unwrap();
    assert_eq!(
        (
            pg.engine,
            pg.host.as_str(),
            pg.port,
            pg.user.as_str(),
            pg.password.as_str(),
            pg.name.as_str()
        ),
        (Engine::Postgres, "::1", Some(5433), "me", "p@ss", "app")
    );
    let socket = conn("DATABASE_URL=postgres://me@/app?host=/var/run/postgresql\n").unwrap();
    assert_eq!(
        (socket.host.as_str(), socket.name.as_str()),
        ("/var/run/postgresql", "app")
    );
    assert_eq!(
        conn("DATABASE_URL=sqlite:////srv/db.sqlite3\n")
            .unwrap()
            .name,
        "/srv/db.sqlite3"
    );
    let prisma = conn("DATABASE_URL=\"file:./dev.db\"\n").unwrap();
    assert_eq!(
        (prisma.engine, prisma.name.as_str()),
        (Engine::Sqlite, "./dev.db")
    );
    let my = conn(
        "DB_CONNECTION=mariadb\nDB_SOCKET=/tmp/mysql.sock\nDB_DATABASE=shop\nDB_USERNAME=root\n",
    )
    .unwrap();
    assert_eq!(
        (my.engine, my.host.as_str(), my.name.as_str()),
        (Engine::Mysql, "/tmp/mysql.sock", "shop")
    );
    let lite = conn("DB_CONNECTION=sqlite\n").unwrap();
    assert_eq!(
        lite.name, "database/database.sqlite",
        "Laravel's default file"
    );
    let yml = "default: &default\n  adapter: postgresql\n  pool: <%= ENV.fetch(\"RAILS_MAX_THREADS\") { 5 } %>\n  host: localhost\ndevelopment:\n  <<: *default\n  database: blog_dev\n";
    let rails = conn(yml).unwrap();
    assert_eq!(
        (rails.engine, rails.host.as_str(), rails.name.as_str()),
        (Engine::Postgres, "localhost", "blog_dev"),
        "merged defaults; an ERB key TermF does not need is fine"
    );
    let erb =
        conn("development:\n  adapter: postgresql\n  database: <%= ENV['DB'] %>\n").unwrap_err();
    assert!(format!("{erb:#}").contains("ERB"), "{erb:#}");
    let laravel11 =
        conn("DB_CONNECTION=mysql\nDB_URL=mysql://u@127.0.0.1/app\nDB_DATABASE=other\n").unwrap();
    assert_eq!(
        (laravel11.engine, laravel11.name.as_str()),
        (Engine::Mysql, "app"),
        "DB_URL overrides DB_DATABASE"
    );
    let other = conn("DB_CONNECTION=sqlsrv\n").unwrap_err();
    assert!(format!("{other:#}").contains("not supported"), "{other:#}");
}

#[test]
fn refuses_remote_databases() {
    let sb = Sandbox::new("remote");
    let refused = |host: &str| {
        // `[::1]` is no file name on Windows.
        let dir = sb.dir(
            &host.replace(['[', ']', ':'], "_"),
            &[(
                ".env",
                &format!("DATABASE_URL=postgres://u:p@{host}:5432/app\n"),
            )],
        );
        database::view(&dir, ".env").refused
    };
    for local in ["localhost", "127.0.0.1", "127.0.0.2", "[::1]"] {
        assert_eq!(refused(local), None, "{local}");
    }
    for remote in [
        "db.example.com",
        "10.0.0.5",
        "localhost.example.com",
        "host.docker.internal",
    ] {
        let why = refused(remote).unwrap_or_else(|| panic!("{remote} is not refused"));
        assert!(why.contains("TermF only copies a local database"), "{why}");
    }
    let compose = "services:\n  mysql:\n    image: mariadb:11\n";
    let dir = sb.dir(
        "sail",
        &[
            ("docker-compose.yml", compose),
            (
                ".env",
                "DB_CONNECTION=mysql\nDB_HOST=mysql\nDB_DATABASE=shop\n",
            ),
        ],
    );
    let view = database::view(&dir, ".env");
    assert_eq!(
        (view.service.as_deref(), view.refused),
        (Some("mysql"), None),
        "a service of the project"
    );
}

#[test]
fn picks_files_like_worktreeinclude() {
    let p = strings(&[
        ".env",
        "/config/master.key",
        "storage/",
        "*.local",
        "!.env.test.local",
        "secrets/**/*.pem",
    ]);
    let take = |path: &str, dir: bool| included(&p, path, dir);
    assert!(
        take(".env", false) && take("api/.env", false),
        "a bare name matches at any depth"
    );
    assert!(
        take("config/master.key", false) && !take("api/config/master.key", false),
        "anchored"
    );
    assert!(
        take("storage", true) && take("storage/app/x.txt", false),
        "a folder and what it holds"
    );
    assert!(
        !take("storage", false),
        "a pattern ending with / takes folders only"
    );
    assert!(
        take(".env.development.local", false) && !take(".env.test.local", false),
        "the last match decides"
    );
    assert!(take("secrets/a/b/key.pem", false) && take("secrets/key.pem", false));
    assert!(!take("README.md", false));
}

#[test]
fn copies_ignored_files_and_never_dependency_folders() {
    let sb = Sandbox::new("include");
    let app = sb.repo(
        "app",
        &[
            (".gitignore", ".env\nnode_modules/\nstorage/*.key\n"),
            (".env", "APP_KEY=x\n"),
            ("node_modules/left-pad/index.js", "module.exports = 1\n"),
            ("storage/oauth.key", "key\n"),
            ("README.md", "tracked\n"),
        ],
    );
    let wt = sb.worktree(&app, "one");
    let patterns = strings(&[".env", "node_modules", "storage/*.key", "README.md"]);
    let (copied, present) = files::include(&sb.env, &app, &wt, &patterns, &[]).unwrap();
    assert_eq!(
        (copied, present),
        (strings(&[".env", "storage/oauth.key"]), 0)
    );
    assert_eq!(fs::read_to_string(wt.join(".env")).unwrap(), "APP_KEY=x\n");
    assert!(
        !wt.join("node_modules").exists(),
        "a dependency folder is never copied raw"
    );
    fs::write(wt.join(".env"), "APP_KEY=mine\n").unwrap();
    let (copied, present) = files::include(&sb.env, &app, &wt, &patterns, &[]).unwrap();
    assert_eq!((copied.len(), present), (0, 2));
    assert_eq!(
        fs::read_to_string(wt.join(".env")).unwrap(),
        "APP_KEY=mine\n",
        "what the worktree already has stays"
    );
}

#[test]
fn locates_worktrees_and_refuses_the_repository_itself() {
    let sb = Sandbox::new("locate");
    let app = sb.repo("app", &[("README.md", "x")]);
    let alone = sb.repo("alone", &[("README.md", "x")]);
    sb.project("acme", &[&app]);
    let setup = sb.setup();
    let wt = sb.worktree(&app, "one");
    fs::create_dir_all(wt.join("src")).unwrap();
    let found = setup.locate(&wt.join("src"), Some("9".into())).unwrap();
    assert_eq!(
        (
            found.project.as_str(),
            found.repo.as_str(),
            found.id.as_str()
        ),
        ("acme", "app", "9")
    );
    assert!(same(&found.path, &wt) && same(&found.source, &app));
    assert!(
        setup.locate(&wt, None).unwrap().id.starts_with('w'),
        "named after its path outside a run"
    );
    let own = format!("{:#}", setup.locate(&app, None).unwrap_err());
    assert!(own.contains("TermF prepares worktrees only"), "{own}");
    let other = sb.worktree(&alone, "two");
    assert!(format!("{:#}", setup.locate(&other, None).unwrap_err()).contains("in no project"));
}

#[test]
fn gives_ports_and_a_compose_project_per_worktree() {
    let sb = Sandbox::new("ports");
    let compose = "services:\n  web:\n    ports: ['8080:80']\n  db:\n    image: postgres\n";
    let app = sb.repo("app", &[("docker-compose.yml", compose)]);
    sb.project("acme", &[&app]);
    let setup = sb.setup();
    let (w1, w2) = (sb.worktree(&app, "one"), sb.worktree(&app, "two"));
    let first = setup.run(&setup.locate(&w1, Some("41".into())).unwrap());
    assert!(!first.saved && !first.failed(), "{first:#?}");
    assert_eq!(first.env["COMPOSE_PROJECT_NAME"], "termf_41");
    let port = |r: &Report, key: &str| -> u16 { r.env[key].parse().unwrap() };
    let mine = [port(&first, "PORT"), port(&first, "TERMF_PORT_WEB")];
    assert!(mine.iter().all(|p| (41000..=41999).contains(p)) && mine[0] != mine[1]);
    assert_eq!(one(&first, StepKind::Env).outcome, Outcome::Set);

    let again = setup.run(&setup.locate(&w1, Some("41".into())).unwrap());
    assert_eq!(again.env, first.env, "a worktree keeps its ports");
    let second = setup.run(&setup.locate(&w2, Some("42".into())).unwrap());
    assert_eq!(second.env["COMPOSE_PROJECT_NAME"], "termf_42");
    for key in ["PORT", "TERMF_PORT_WEB"] {
        assert!(
            !mine.contains(&port(&second, key)),
            "two worktrees never share a port"
        );
    }

    let down = setup.teardown(&w1, "41");
    assert_eq!(one(&down, StepKind::Env).outcome, Outcome::Released);
    let given = fs::read_to_string(sb.root.join("data/ports.json")).unwrap();
    assert!(!given.contains(&mine[0].to_string()) || port(&second, "PORT") == mine[0]);
    assert!(
        setup.teardown(&w1, "41").steps.is_empty(),
        "nothing left to give back"
    );
    assert_eq!(compose_name("Run 7"), "termf_run_7");
}

#[cfg(unix)]
#[test]
fn clones_or_installs_dependencies_never_copies_them() {
    let sb = Sandbox::new("deps");
    let app = sb.repo(
        "app",
        &[
            (".gitignore", "node_modules/\ntarget/\nvendor/\n.venv/\n"),
            ("package.json", "{}"),
            ("node_modules/left-pad/index.js", "source\n"),
            ("vendor/autoload.php", "source\n"),
            ("target/debug/app", "binary\n"),
        ],
    );
    sb.project("acme", &[&app]);
    let dep = |dir: &str, clone: bool, install: &[&str]| Dep {
        dir: dir.into(),
        clone,
        install: strings(install),
    };
    let recipe = Recipe {
        repos: BTreeMap::from([(
            "app".to_owned(),
            RepoRecipe {
                deps: vec![
                    dep(
                        "node_modules",
                        true,
                        &[
                            "false",
                            "mkdir -p node_modules && touch node_modules/.synced",
                        ],
                    ),
                    dep(
                        "vendor",
                        false,
                        &["mkdir -p vendor && touch vendor/installed"],
                    ),
                    dep(".venv", true, &["mkdir -p .venv"]),
                    dep("target", true, &[]),
                    dep("missing", true, &[]),
                    dep("", false, &["exit 3"]),
                    dep("build", false, &["mkdir -p build"]),
                    dep("", false, &["touch package-lock.json"]),
                ],
                ..RepoRecipe::default()
            },
        )]),
        ..Recipe::default()
    };
    save(&sb.config(), "acme", recipe).unwrap();
    let setup = sb.setup();
    let path = sb.worktree(&app, "one");
    let wt = setup.locate(&path, Some("1".into())).unwrap();
    let cow = cow_here(&sb);

    let report = setup.run(&wt);
    assert!(report.saved);
    let node = find(&report, StepKind::Deps, "node_modules");
    if cow {
        assert_eq!(node.outcome, Outcome::Cloned, "{node:?}");
        assert!(
            node.detail.contains("then mkdir -p node_modules"),
            "then synced with the lockfile"
        );
        assert!(path.join("node_modules/left-pad/index.js").exists());
    } else {
        assert_eq!(node.outcome, Outcome::Installed, "{node:?}");
        assert!(
            !path.join("node_modules/left-pad").exists(),
            "never a raw copy"
        );
    }
    assert!(path.join("node_modules/.synced").exists());
    let vendor = find(&report, StepKind::Deps, "vendor");
    assert_eq!(
        (vendor.outcome, vendor.detail.as_str()),
        (
            Outcome::Installed,
            "mkdir -p vendor && touch vendor/installed"
        )
    );
    assert!(
        !path.join("vendor/autoload.php").exists(),
        "clone = false installs"
    );
    assert_eq!(
        find(&report, StepKind::Deps, ".venv").outcome,
        Outcome::Installed
    );
    let target = find(&report, StepKind::Deps, "target").outcome;
    assert_eq!(
        target,
        if cow {
            Outcome::Cloned
        } else {
            Outcome::Skipped
        }
    );
    assert_eq!(
        find(&report, StepKind::Deps, "missing").outcome,
        Outcome::Skipped
    );
    let failed = find(&report, StepKind::Deps, "exit");
    assert_eq!(failed.outcome, Outcome::Failed);
    assert!(failed.detail.contains("exit 3"), "{failed:?}");
    let build = find(&report, StepKind::Deps, "build");
    assert_eq!(build.outcome, Outcome::Skipped, "{build:?}");
    assert!(build.detail.contains("git does not ignore build"));
    assert!(!path.join("build").exists(), "not installed at all");
    let lock = find(&report, StepKind::Deps, "touch");
    assert_eq!(lock.outcome, Outcome::Failed, "{lock:?}");
    assert!(
        lock.detail.contains("files git does not ignore")
            && lock.detail.contains("package-lock.json"),
        "{lock:?}"
    );
    assert!(report.failed());
    assert_eq!(
        setup.last(&path),
        report.steps,
        "the last setup is kept with the worktree"
    );

    let again = setup.run(&wt);
    let node = find(&again, StepKind::Deps, "node_modules");
    assert_eq!(
        node.outcome,
        Outcome::Installed,
        "already there: only synced"
    );
    let target = find(&again, StepKind::Deps, "target").outcome;
    assert_eq!(
        target,
        if cow {
            Outcome::Present
        } else {
            Outcome::Skipped
        }
    );
}

#[test]
fn copies_a_sqlite_database_per_worktree() {
    let sb = Sandbox::new("sqlite");
    let app = sb.repo(
        "app",
        &[
            (".gitignore", ".env\n*.sqlite\n"),
            (".env", "APP_NAME=App\nDB_CONNECTION=sqlite\n"),
        ],
    );
    let shared = sb.root.join("shared/api.sqlite");
    fs::create_dir_all(shared.parent().unwrap()).unwrap();
    let api_env = format!(
        "DB_CONNECTION=sqlite\nDB_DATABASE=\"{}\"\n",
        shared.display()
    );
    let api = sb.repo("api", &[(".gitignore", ".env\n"), (".env", &api_env)]);
    sb.project("acme", &[&app, &api]);
    let original = app.join("database/database.sqlite");
    fs::create_dir_all(original.parent().unwrap()).unwrap();
    for file in [&original, &shared] {
        let db = rusqlite::Connection::open(file).unwrap();
        db.execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (42);")
            .unwrap();
    }
    let rows = |file: &Path| -> Vec<i64> {
        let db = rusqlite::Connection::open(file).unwrap();
        let mut q = db.prepare("SELECT x FROM t ORDER BY x").unwrap();
        q.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let setup = sb.setup();

    let path = sb.worktree(&app, "one");
    let report = setup.run(&setup.locate(&path, Some("5".into())).unwrap());
    let db = one(&report, StepKind::Database);
    assert_eq!(db.outcome, Outcome::Copied, "{db:?}");
    let copy = path.join("database/database.sqlite");
    assert_eq!(rows(&copy), [42]);
    rusqlite::Connection::open(&copy)
        .unwrap()
        .execute("INSERT INTO t VALUES (43)", [])
        .unwrap();
    assert_eq!(rows(&original), [42], "the worktree has its own copy");
    assert_eq!(
        fs::read_to_string(path.join(".env")).unwrap(),
        "APP_NAME=App\nDB_CONNECTION=sqlite\n",
        "a path inside the repository needs no rewrite"
    );
    let again = setup.run(&setup.locate(&path, Some("5".into())).unwrap());
    assert_eq!(one(&again, StepKind::Database).outcome, Outcome::Present);
    assert_eq!(rows(&copy), [42, 43], "a second setup keeps the copy");

    let path = sb.worktree(&api, "two");
    let report = setup.run(&setup.locate(&path, Some("6".into())).unwrap());
    let db = one(&report, StepKind::Database);
    let beside = shared.with_file_name("api_termf_6.sqlite");
    assert_eq!(db.outcome, Outcome::Copied, "{db:?}");
    assert_eq!(rows(&beside), [42]);
    let env = fs::read_to_string(path.join(".env")).unwrap();
    assert!(
        env.contains(&format!("DB_DATABASE=\"{}\"", beside.display())),
        "{env}"
    );
    let down = setup.teardown(&path, "6");
    assert_eq!(one(&down, StepKind::Database).outcome, Outcome::Dropped);
    assert!(
        !beside.exists() && shared.exists(),
        "the copy goes, the original stays"
    );

    let path = sb.worktree(&api, "three");
    setup.run(&setup.locate(&path, Some("9".into())).unwrap());
    let copy = shared.with_file_name("api_termf_9.sqlite");
    assert!(copy.exists());
    fs::remove_dir_all(&path).unwrap();
    sb.git(&api, &["worktree", "prune"]);
    let down = setup.teardown(&path, "9");
    assert_eq!(one(&down, StepKind::Database).outcome, Outcome::Dropped);
    assert!(!copy.exists(), "dropped although the worktree was gone");
}

#[test]
fn never_rewrites_an_env_file_git_does_not_ignore() {
    let sb = Sandbox::new("tracked");
    let app = sb.repo(
        "app",
        &[(".env", "DB_CONNECTION=mysql\nDB_DATABASE=shop\n")],
    );
    let api = sb.repo("api", &[("README.md", "x")]);
    // Untracked, but not ignored either: it would end up in the run's pull request.
    fs::write(
        api.join(".env"),
        "DATABASE_URL=mysql://root@127.0.0.1/shop\n",
    )
    .unwrap();
    sb.project("acme", &[&app, &api]);
    let setup = sb.setup();
    for (repo, name) in [(&app, "one"), (&api, "two")] {
        let path = sb.worktree(repo, name);
        let report = setup.run(&setup.locate(&path, None).unwrap());
        let db = one(&report, StepKind::Database);
        assert_eq!(db.outcome, Outcome::Refused, "{db:?}");
        assert!(db.detail.contains("git does not ignore it"), "{db:?}");
    }
    assert_eq!(
        fs::read_to_string(sb.root.join("worktrees/one/.env")).unwrap(),
        "DB_CONNECTION=mysql\nDB_DATABASE=shop\n"
    );
    assert!(
        !sb.root.join("worktrees/two/.env").exists(),
        "nothing copied"
    );
}

#[cfg(unix)]
#[test]
fn copies_a_postgres_database_through_its_template() {
    let sb = Sandbox::new("postgres");
    let log = sb.root.join("calls.log");
    let busy = sb.root.join("busy");
    sb.fake(
        "psql",
        &format!(
            "echo \"psql $* | $PGHOST $PGPORT $PGUSER $PGPASSWORD\" >> {log:?}\n\
             case \"$*\" in *TEMPLATE*) if [ -e {busy:?} ]; then echo 'ERROR:  source database \"app\" is being accessed by other users' >&2; exit 1; fi;; esac\n\
             cat > /dev/null\n"
        ),
    );
    sb.fake(
        "pg_dump",
        &format!("echo \"pg_dump $* | $PGUSER\" >> {log:?}\necho 'CREATE TABLE t ();'\n"),
    );
    let env = "APP=x\nDATABASE_URL=\"postgres://me:s%3Dcret@localhost:5432/app?sslmode=disable\"\n";
    let app = sb.repo("app", &[(".gitignore", ".env\n"), (".env", env)]);
    sb.project("acme", &[&app]);
    let setup = sb.setup();

    let path = sb.worktree(&app, "one");
    let wt = setup.locate(&path, Some("7".into())).unwrap();
    let report = setup.run(&wt);
    let db = one(&report, StepKind::Database);
    assert_eq!(
        (db.outcome, db.target.as_str()),
        (Outcome::Created, "app_termf_7"),
        "{db:?}"
    );
    assert_eq!(
        sb.log(),
        "psql -w -X -q -v ON_ERROR_STOP=1 -d postgres -c CREATE DATABASE \"app_termf_7\" TEMPLATE \"app\" | localhost 5432 me s=cret\n",
        "the password goes through the environment only"
    );
    assert_eq!(
        fs::read_to_string(path.join(".env")).unwrap(),
        "APP=x\nDATABASE_URL=\"postgres://me:s%3Dcret@localhost:5432/app_termf_7?sslmode=disable\"\n"
    );
    assert_eq!(
        fs::read_to_string(app.join(".env")).unwrap(),
        env,
        "the repository's own stays"
    );
    let again = setup.run(&wt);
    assert_eq!(one(&again, StepKind::Database).outcome, Outcome::Present);
    assert_eq!(sb.log().lines().count(), 1, "made once");
    let down = setup.teardown(&path, "7");
    assert_eq!(one(&down, StepKind::Database).outcome, Outcome::Dropped);
    assert!(
        sb.log()
            .ends_with("-c DROP DATABASE IF EXISTS \"app_termf_7\" | localhost 5432 me s=cret\n")
    );

    // An app connected to the template: dump and restore instead.
    fs::write(&busy, "").unwrap();
    fs::write(&log, "").unwrap();
    let path = sb.worktree(&app, "two");
    let report = setup.run(&setup.locate(&path, Some("8".into())).unwrap());
    let db = one(&report, StepKind::Database);
    assert_eq!(db.outcome, Outcome::Created, "{db:?}");
    assert!(db.detail.contains("open connections"), "{db:?}");
    let logged = sb.log();
    let mut calls: Vec<&str> = logged
        .lines()
        .map(|l| l.split(" | ").next().unwrap())
        .collect();
    // The dump and the restore run side by side.
    calls[2..].sort();
    assert_eq!(
        calls,
        [
            "psql -w -X -q -v ON_ERROR_STOP=1 -d postgres -c CREATE DATABASE \"app_termf_8\" TEMPLATE \"app\"",
            "psql -w -X -q -v ON_ERROR_STOP=1 -d postgres -c CREATE DATABASE \"app_termf_8\"",
            "pg_dump -w --no-owner --no-privileges -d app",
            "psql -w -X -q -v ON_ERROR_STOP=1 -d app_termf_8",
        ]
    );

    // A remote server is refused before anything runs.
    fs::write(&log, "").unwrap();
    fs::write(
        app.join(".env"),
        "DATABASE_URL=postgres://me:x@db.example.com:5432/app\n",
    )
    .unwrap();
    let path = sb.worktree(&app, "three");
    let report = setup.run(&setup.locate(&path, Some("9".into())).unwrap());
    let db = one(&report, StepKind::Database);
    assert_eq!(db.outcome, Outcome::Refused);
    assert!(db.detail.contains("db.example.com"), "{db:?}");
    assert_eq!(sb.log(), "", "nothing ran");
}

#[cfg(unix)]
#[test]
fn copies_a_mysql_database_with_a_dump() {
    let sb = Sandbox::new("mysql");
    let log = sb.root.join("calls.log");
    let restored = sb.root.join("restored.sql");
    sb.fake(
        "mysql",
        &format!("echo \"mysql $* | $MYSQL_PWD\" >> {log:?}\ncat >> {restored:?}\n"),
    );
    sb.fake(
        "mysqldump",
        &format!(
            "case \"$*\" in *--version*) echo 'mysqldump  Ver 8.4.3 for macos15'; exit;; esac\n\
             echo \"mysqldump $* | $MYSQL_PWD\" >> {log:?}\necho '-- the dump'\n"
        ),
    );
    let env = "DB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_PORT=3306\nDB_DATABASE=shop\nDB_USERNAME=root\nDB_PASSWORD=secret\n";
    let shop = sb.repo("shop", &[(".gitignore", ".env\n"), (".env", env)]);
    sb.project("acme", &[&shop]);
    let setup = sb.setup();
    let path = sb.worktree(&shop, "one");
    let report = setup.run(&setup.locate(&path, Some("3".into())).unwrap());
    let db = one(&report, StepKind::Database);
    assert_eq!(
        (db.outcome, db.target.as_str()),
        (Outcome::Created, "shop_termf_3"),
        "{db:?}"
    );
    let conn = "--no-defaults --host=127.0.0.1 --port=3306 --user=root";
    let logged = sb.log();
    let mut calls: Vec<&str> = logged.lines().collect();
    // The dump and the restore run side by side.
    calls[1..].sort();
    assert_eq!(
        calls,
        [
            format!("mysql {conn} -e CREATE DATABASE `shop_termf_3` | secret"),
            format!("mysql {conn} shop_termf_3 | secret"),
            format!(
                "mysqldump {conn} --single-transaction --routines --triggers --set-gtid-purged=OFF shop | secret"
            ),
        ]
    );
    assert_eq!(fs::read_to_string(&restored).unwrap(), "-- the dump\n");
    let written = fs::read_to_string(path.join(".env")).unwrap();
    assert!(
        written.contains("DB_DATABASE=shop_termf_3\n") && written.contains("DB_PASSWORD=secret\n")
    );
    setup.teardown(&path, "3");
    assert!(sb.log().ends_with(&format!(
        "mysql {conn} -e DROP DATABASE IF EXISTS `shop_termf_3` | secret\n"
    )));
}

#[cfg(unix)]
#[test]
fn copies_a_database_inside_its_compose_service() {
    let sb = Sandbox::new("compose-db");
    let log = sb.root.join("calls.log");
    sb.fake(
        "docker",
        &format!(
            "echo \"docker $* | $MYSQL_PWD | $(pwd -P)\" >> {log:?}\n\
             case \"$*\" in *--version*) echo 'mysqldump from 11.4.3-MariaDB';; *mysqldump*) echo '-- dump';; *) cat > /dev/null;; esac\n"
        ),
    );
    let compose = "services:\n  app:\n    ports: ['80:80']\n  mysql:\n    image: mysql:8\n";
    let env = "DB_CONNECTION=mysql\nDB_HOST=mysql\nDB_DATABASE=sail\nDB_USERNAME=sail\nDB_PASSWORD=password\n";
    let app = sb.repo(
        "app",
        &[
            (".gitignore", ".env\n"),
            (".env", env),
            ("docker-compose.yml", compose),
        ],
    );
    sb.project("acme", &[&app]);
    let setup = sb.setup();
    let path = sb.worktree(&app, "one");
    let report = setup.run(&setup.locate(&path, Some("4".into())).unwrap());
    assert_eq!(
        one(&report, StepKind::Database).outcome,
        Outcome::Created,
        "{report:#?}"
    );
    let source = fs::canonicalize(&app).unwrap();
    let first = sb.log().lines().next().unwrap().to_owned();
    assert_eq!(
        first,
        format!(
            "docker compose exec -T -e MYSQL_PWD mysql mysql --user=sail -e CREATE DATABASE `sail_termf_4` | password | {}",
            source.display()
        ),
        "run in the repository's own service, the password from the environment"
    );
    assert!(
        sb.log()
            .contains("mysqldump --user=sail --single-transaction --routines --triggers sail |"),
        "MariaDB's dump has no --set-gtid-purged: {}",
        sb.log()
    );
    assert_eq!(report.env["COMPOSE_PROJECT_NAME"], "termf_4");
    assert!(report.env.contains_key("TERMF_PORT_APP"));
}

#[test]
fn keeps_the_recipe_and_its_skill_agent_and_command_in_the_project_library() {
    let sb = Sandbox::new("recipe");
    let shop = sb.repo(
        "shop",
        &[
            ("composer.json", "{}"),
            ("artisan", ""),
            (".gitignore", ".env\n"),
            (".env", "DB_CONNECTION=mysql\nDB_DATABASE=shop\n"),
        ],
    );
    let tool = sb.repo("tool", &[("README.md", "x")]);
    sb.project("acme", &[&shop, &tool]);
    let config = sb.config();

    let first = view(&config, "acme").unwrap();
    assert!(!first.saved && first.draft.is_none());
    assert_eq!(first.recipe, first.detected);
    assert_eq!(
        first.recipe.repos["shop"].stack,
        ["php:composer", "laravel", "mysql"]
    );
    assert_eq!(first.recipe.repos["tool"], RepoRecipe::default());
    let shop_view = first.repos.iter().find(|r| r.name == "shop").unwrap();
    assert_eq!(shop_view.databases[".env"].engine, Some(Engine::Mysql));

    let mut recipe = first.recipe.clone();
    recipe.agent = true;
    recipe.notes = "Seed with `php artisan db:seed`.".into();
    recipe.repos.get_mut("tool").unwrap().commands = strings(&["make setup", " "]);
    let saved = save(&config, "acme", recipe.clone()).unwrap();
    assert!(saved.saved);
    recipe.repos.get_mut("tool").unwrap().commands = strings(&["make setup"]);
    assert_eq!(saved.recipe, recipe, "trimmed, empty lines dropped");
    let file = config.join("projects/acme/library/recipes/worktree-setup.toml");
    let text = fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("[repos.shop]") && text.contains("agent = true"),
        "{text}"
    );
    assert_eq!(
        recipe::load(&config, "acme").unwrap(),
        Some(recipe.clone()),
        "round trip"
    );

    let scope = termf_library::Scope {
        project: Some("acme".into()),
        ..termf_library::Scope::default()
    };
    let items = termf_library::resolve(&config, &scope).unwrap();
    for kind in [
        termf_library::Kind::Skill,
        termf_library::Kind::Role,
        termf_library::Kind::Command,
    ] {
        let item = items
            .iter()
            .find(|i| i.kind == kind && i.name == "worktree-setup")
            .unwrap_or_else(|| panic!("no {kind} worktree-setup"));
        assert_eq!(
            (item.layer, item.active),
            (termf_library::Layer::Project, true)
        );
        assert!(
            item.error.is_none() && item.description.is_some(),
            "{kind}: {item:?}"
        );
    }
    let skill = &items
        .iter()
        .find(|i| i.kind == termf_library::Kind::Skill && i.name == "worktree-setup")
        .unwrap()
        .body;
    for expected in [
        "## shop (php:composer, laravel, mysql)",
        "`vendor`: cloned copy-on-write",
        "Then: `make setup`",
        "php artisan db:seed",
        "own copy of the local database",
    ] {
        assert!(skill.contains(expected), "{expected} missing in:\n{skill}");
    }
    let command = items
        .iter()
        .find(|i| i.kind == termf_library::Kind::Command && i.name == "worktree-setup")
        .unwrap();
    assert!(command.body.contains("worktree setup"));

    let refused = |edit: &dyn Fn(&mut RepoRecipe)| {
        let mut bad = recipe.clone();
        edit(bad.repos.get_mut("tool").unwrap());
        format!("{:#}", save(&config, "acme", bad).unwrap_err())
    };
    assert!(
        refused(&|r| r.deps = vec![Dep {
            dir: "../x".into(),
            clone: true,
            install: vec![]
        }])
        .contains("not a folder")
    );
    assert!(refused(&|r| r.include = strings(&["node_modules"])).contains("never copied raw"));
    assert!(refused(&|r| r.include = strings(&["../secrets"])).contains("leaves the repository"));
    assert!(
        refused(&|r| r.database = Some("/etc/passwd".into()))
            .contains("not a file of the repository")
    );
    let mut ghost = recipe.clone();
    ghost.repos.insert("ghost".into(), RepoRecipe::default());
    assert!(save(&config, "acme", ghost).is_err());
    assert_eq!(
        recipe::load(&config, "acme").unwrap(),
        Some(recipe),
        "a refused save changes nothing"
    );
}

#[cfg(unix)]
#[test]
fn proposes_a_recipe_with_claude_read_only() {
    let sb = Sandbox::new("propose");
    let shop = sb.repo("shop", &[("composer.json", "{}")]);
    let tool = sb.repo("tool", &[("mix.exs", "")]);
    sb.project("acme", &[&shop, &tool]);
    let answer = serde_json::json!({
        "type": "result", "is_error": false, "total_cost_usd": 0.12,
        "structured_output": {
            "agent": true, "notes": "Run `mix ecto.setup` once.",
            "repos": [
                { "name": "tool", "stack": ["elixir:mix"], "include": [".env"],
                  "deps": [{ "dir": "deps", "clone": true, "install": ["mix deps.get"] }],
                  "database": "", "compose": false, "ports": [], "commands": ["mix compile"] },
                { "name": "ghost", "stack": [], "include": [], "deps": [], "database": "", "compose": false,
                  "ports": [], "commands": [] },
                { "name": "shop", "stack": ["php"], "include": [],
                  "deps": [{ "dir": "../outside", "clone": true, "install": [] }],
                  "database": "", "compose": false, "ports": [], "commands": [] },
            ],
        },
    });
    fs::write(sb.root.join("answer.json"), answer.to_string()).unwrap();
    let (args, prompt) = (sb.root.join("args.log"), sb.root.join("prompt.log"));
    sb.fake(
        "claude",
        &format!(
            "echo \"$*\" > {args:?}\ncat > {prompt:?}\ncat {:?}\n",
            sb.root.join("answer.json")
        ),
    );
    let config = sb.config();
    let claude = [sb.root.join("bin/claude").to_string_lossy().into_owned()];
    let proposed = propose(&config, "acme", &claude).unwrap();
    let draft = proposed.draft.clone().unwrap();
    assert!(!proposed.saved, "a proposal waits for review");
    assert_eq!(draft.cost, Some(0.12));
    assert_eq!(draft.recipe.repos.keys().collect::<Vec<_>>(), ["tool"]);
    assert_eq!(draft.recipe.repos["tool"].deps[0].install, ["mix deps.get"]);
    assert_eq!(draft.recipe.repos["tool"].database, None);
    assert_eq!(draft.dropped.len(), 2, "{:?}", draft.dropped);
    let args = fs::read_to_string(&args).unwrap();
    assert!(
        args.contains("--tools Read,Grep,Glob") && args.contains("--json-schema"),
        "{args}"
    );
    let prompt = fs::read_to_string(&prompt).unwrap();
    assert!(
        prompt.contains(&format!("- tool: {}", tool.display())),
        "{prompt}"
    );
    assert!(prompt.contains("never copied raw"));

    let mut reviewed = proposed.recipe.clone();
    reviewed
        .repos
        .insert("tool".into(), draft.recipe.repos["tool"].clone());
    let saved = save(&config, "acme", reviewed).unwrap();
    assert!(
        saved.saved && saved.draft.is_none(),
        "saving takes the place of the draft"
    );
    propose(&config, "acme", &claude).unwrap();
    assert!(discard(&config, "acme").unwrap().draft.is_none());
}

#[cfg(unix)]
#[test]
fn runs_the_setup_agent_in_the_worktree() {
    let sb = Sandbox::new("agent");
    let app = sb.repo("app", &[("README.md", "x")]);
    sb.project("acme", &[&app]);
    let recipe = Recipe {
        agent: true,
        ..Recipe::default()
    };
    save(&sb.config(), "acme", recipe).unwrap();
    let seen = sb.root.join("seen");
    sb.fake(
        "claude",
        &format!(
            "pwd -P > {seen:?}/pwd\necho \"$*\" > {seen:?}/args\ncat > {seen:?}/prompt\n\
             while [ $# -gt 0 ]; do\n\
               case \"$1\" in --plugin-dir) cp \"$2/skills/worktree-setup/SKILL.md\" {seen:?}/skill;;\n\
               --append-system-prompt-file) cp \"$2\" {seen:?}/role;; esac; shift\n\
             done\n\
             echo '{{\"type\":\"result\",\"is_error\":false,\"result\":\"Ran the migrations.\",\"total_cost_usd\":0.05}}'\n"
        ),
    );
    fs::create_dir_all(&seen).unwrap();
    let setup = sb.setup();
    let path = sb.worktree(&app, "one");
    let report = setup.run(&setup.locate(&path, Some("2".into())).unwrap());
    let agent = one(&report, StepKind::Agent);
    assert_eq!(
        (agent.outcome, agent.detail.as_str()),
        (Outcome::Ran, "Ran the migrations. ($0.05)")
    );
    let read = |name: &str| fs::read_to_string(seen.join(name)).unwrap();
    assert_eq!(
        read("pwd").trim(),
        fs::canonicalize(&path).unwrap().to_string_lossy()
    );
    let args = read("args");
    for expected in [
        "-p",
        "--restricted",
        "--strict-mcp-config",
        "--disallowedTools Bash(git push:*)",
        "\"sandbox\"",
        "\"allowWrite\"",
    ] {
        assert!(args.contains(expected), "{expected} missing in {args}");
    }
    assert!(read("prompt").contains("TermF already ran the recipe's steps"));
    assert!(read("skill").contains("# Preparing a worktree of acme"));
    assert!(
        read("role").starts_with("You prepare a git worktree"),
        "the role without its frontmatter"
    );
}

/// Against a throwaway MySQL server only, reached through its socket: `TERMF_TEST_MYSQL_SOCKET=<socket>
/// cargo test -p termf-worktree -- --ignored`.
#[test]
#[ignore]
fn copies_a_real_mysql_database() {
    let socket = std::env::var("TERMF_TEST_MYSQL_SOCKET").expect("TERMF_TEST_MYSQL_SOCKET");
    let mut sb = Sandbox::new("real-mysql");
    // The client reads no option file of the user's.
    sb.env
        .push(("HOME".into(), sb.root.to_string_lossy().into_owned()));
    let mysql = |sql: &str| {
        let out = Command::new("mysql")
            .args([
                &format!("--socket={socket}"),
                "--user=root",
                "-N",
                "-e",
                sql,
            ])
            .env("HOME", &sb.root)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    mysql(
        "DROP DATABASE IF EXISTS shop; CREATE DATABASE shop; CREATE TABLE shop.t (x INT); INSERT INTO shop.t VALUES (42);",
    );
    let env =
        format!("DB_CONNECTION=mysql\nDB_SOCKET={socket}\nDB_DATABASE=shop\nDB_USERNAME=root\n");
    let app = sb.repo("app", &[(".gitignore", ".env\n"), (".env", &env)]);
    sb.project("acme", &[&app]);
    let setup = sb.setup();
    let path = sb.worktree(&app, "one");
    let report = setup.run(&setup.locate(&path, Some("1".into())).unwrap());
    let db = one(&report, StepKind::Database);
    assert_eq!(db.outcome, Outcome::Created, "{db:?}");
    assert_eq!(mysql("SELECT x FROM shop_termf_1.t"), "42");
    mysql("INSERT INTO shop_termf_1.t VALUES (43)");
    assert_eq!(
        mysql("SELECT COUNT(*) FROM shop.t"),
        "1",
        "the original is untouched"
    );
    setup.teardown(&path, "1");
    assert_eq!(mysql("SHOW DATABASES LIKE 'shop_termf_1'"), "");
    mysql("DROP DATABASE shop");
}

#[test]
fn never_copies_the_repository_worktrees_into_a_new_one() {
    let sb = Sandbox::new("nested");
    // `.claude/` ignored whole, or only its worktrees, by the local exclude file as TermF does.
    for (name, ignore) in [("whole", ".claude/\n"), ("excluded", "")] {
        let app = sb.repo(
            name,
            &[(".gitignore", ignore), (".claude/notes.md", "{}\n")],
        );
        fs::write(app.join(".git/info/exclude"), "/.claude/worktrees/\n").unwrap();
        let nested = |branch: &str| {
            let path = app.join(".claude/worktrees").join(branch);
            let at = path.to_string_lossy();
            sb.git(&app, &["worktree", "add", "-q", "-b", branch, &at]);
            path
        };
        let older = nested("older");
        fs::write(older.join(".env"), "A=1\n").unwrap();
        let wt = nested("new");
        for pattern in [".claude/", ".claude/notes.md"] {
            let (copied, _) =
                files::include(&sb.env, &app, &wt, &strings(&[pattern]), &[]).unwrap();
            let wanted = if ignore.is_empty() {
                vec![]
            } else {
                strings(&[".claude/notes.md"])
            };
            assert_eq!(copied, wanted, "{name} {pattern}");
            assert!(!wt.join(".claude/worktrees").exists(), "{name} {pattern}");
            let _ = fs::remove_file(wt.join(".claude/notes.md"));
        }
    }
}

#[test]
fn includes_files_inside_a_folder_git_ignores_whole() {
    let sb = Sandbox::new("inside");
    let app = sb.repo(
        "app",
        &[
            (".gitignore", ".bundle/\nstorage/\n"),
            ("Gemfile", ""),
            (".bundle/config", "BUNDLE_PATH: \"vendor/bundle\"\n"),
            ("storage/oauth-private.key", "key\n"),
            ("storage/logs/app.log", "log\n"),
        ],
    );
    let detected = detect(&app, &sb.env);
    assert!(
        detected.include.contains(&".bundle/config".to_owned()),
        "{detected:?}"
    );
    let wt = sb.worktree(&app, "one");
    let patterns = strings(&[".bundle/config", "storage/*.key"]);
    let (copied, _) = files::include(&sb.env, &app, &wt, &patterns, &[]).unwrap();
    assert_eq!(
        copied,
        strings(&[".bundle/config", "storage/oauth-private.key"])
    );
    assert!(
        !wt.join("storage/logs").exists(),
        "only what a pattern names"
    );
}

#[test]
fn opens_the_agent_sandbox_to_the_local_database_only() {
    let sb = Sandbox::new("access");
    let access = |env: &str| {
        let dir = sb.dir(&format!("a{}", env.len()), &[(".env", env)]);
        database::access(&dir, ".env")
    };
    let tcp = access("DATABASE_URL=postgres://u@localhost:5433/app\n").unwrap();
    assert!(tcp.local_tcp && tcp.sockets.is_empty());
    let pg_socket = access("DATABASE_URL=postgres://u@/app?host=/var/run/postgresql\n").unwrap();
    assert_eq!(pg_socket.sockets, ["/var/run/postgresql/.s.PGSQL.5432"]);
    let my_default = access("DB_CONNECTION=mysql\nDB_HOST=\nDB_DATABASE=shop\n").unwrap();
    assert!(!my_default.local_tcp && my_default.sockets.contains(&"/tmp/mysql.sock".to_owned()));
    assert_eq!(
        access("DB_CONNECTION=sqlite\n"),
        None,
        "a file needs nothing"
    );
    assert_eq!(
        access("DATABASE_URL=postgres://u@db.example.com/app\n"),
        None,
        "refused, so never opened"
    );
    let settings = sandbox(Some(&tcp));
    assert_eq!(settings["sandbox"]["network"]["allowLocalBinding"], true);
    assert!(settings["sandbox"]["filesystem"]["allowWrite"].is_array());
    let settings = sandbox(Some(&pg_socket));
    assert_eq!(
        settings["sandbox"]["network"]["allowUnixSockets"][0],
        "/var/run/postgresql/.s.PGSQL.5432"
    );
    assert!(sandbox(None)["sandbox"]["network"]["allowLocalBinding"].is_null());
}

#[cfg(unix)]
#[test]
fn points_every_env_file_at_the_copy_and_ignores_inherited_servers() {
    let sb = Sandbox::new("layers");
    let log = sb.root.join("calls.log");
    sb.fake(
        "mysql",
        &format!("echo \"mysql $* | $MYSQL_HOST\" >> {log:?}\ncat > /dev/null\n"),
    );
    sb.fake(
        "mysqldump",
        &format!(
            "case \"$*\" in *--version*) echo 'mysqldump  Ver 8.4.3'; exit;; esac\n\
             echo \"mysqldump $*\" >> {log:?}\necho '-- dump'\n"
        ),
    );
    let app = sb.repo(
        "app",
        &[
            (".gitignore", ".env*\n"),
            (
                ".env",
                "DB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_DATABASE=shop\nDB_USERNAME=root\nANALYTICS_URL=mysql://root@127.0.0.1/stats\n",
            ),
            (".env.development", "DB_URL=mysql://root@127.0.0.1/shop\n"),
            (".env.local", "DATABASE_URL=\"mysql://root@127.0.0.1:3306/shop\"\n"),
            (".env.development.local", "DB_DATABASE=shop\n"),
        ],
    );
    sb.project("acme", &[&app]);
    let mut setup = sb.setup();
    // Settings of the app's own environment that would pick another server.
    setup
        .env
        .push(("MYSQL_HOST".into(), "db.example.com".into()));
    let detected = detect(&app, &sb.env);
    assert_eq!(
        detected.database.as_deref(),
        Some(".env.local"),
        "the file that wins first"
    );
    let path = sb.worktree(&app, "one");
    let report = setup.run(&setup.locate(&path, Some("5".into())).unwrap());
    let db = one(&report, StepKind::Database);
    assert_eq!(
        (db.outcome, db.target.as_str()),
        (Outcome::Created, "shop_termf_5"),
        "{db:?}"
    );
    assert!(sb.log().starts_with(
        "mysql --no-defaults --host=127.0.0.1 --port=3306 --user=root -e CREATE DATABASE `shop_termf_5` | \n"
    ));
    for file in [
        ".env",
        ".env.development",
        ".env.local",
        ".env.development.local",
    ] {
        let text = fs::read_to_string(path.join(file)).unwrap();
        assert!(text.contains("shop_termf_5"), "{file}: {text}");
        assert!(
            !text.contains("/shop\n") && !text.contains("/shop\"") && !text.contains("=shop\n"),
            "{file} still names the original: {text}"
        );
    }
    let base = fs::read_to_string(path.join(".env")).unwrap();
    assert!(
        base.contains("ANALYTICS_URL=mysql://root@127.0.0.1/stats"),
        "another database stays"
    );
}

#[cfg(unix)]
#[test]
fn ignores_inherited_postgres_settings() {
    let sb = Sandbox::new("pgenv");
    let log = sb.root.join("calls.log");
    sb.fake(
        "psql",
        &format!("echo \"$PGHOST|$PGPORT|$PGSERVICE|$PGSERVICEFILE|$PGHOSTADDR\" >> {log:?}\n"),
    );
    let app = sb.repo(
        "app",
        &[
            (".gitignore", ".env\n"),
            (".env", "DATABASE_URL=postgres://me@/app\n"),
        ],
    );
    sb.project("acme", &[&app]);
    let mut setup = sb.setup();
    for (key, value) in [
        ("PGHOST", "db.example.com"),
        ("PGPORT", "6543"),
        ("PGSERVICE", "prod"),
        ("PGSERVICEFILE", "/etc/pg_service.conf"),
        ("PGHOSTADDR", "10.0.0.5"),
    ] {
        setup.env.push((key.into(), value.into()));
    }
    let path = sb.worktree(&app, "one");
    let report = setup.run(&setup.locate(&path, Some("2".into())).unwrap());
    assert_eq!(
        one(&report, StepKind::Database).outcome,
        Outcome::Created,
        "{report:#?}"
    );
    assert_eq!(
        sb.log(),
        "||||\n",
        "no host: the local socket, whatever the environment says"
    );
}

#[cfg(unix)]
#[test]
fn a_restore_that_stops_early_never_hangs_the_dump() {
    let sb = Sandbox::new("pipe");
    let log = sb.root.join("calls.log");
    sb.fake(
        "mysql",
        &format!(
            "echo \"mysql $*\" >> {log:?}\n\
             case \"$*\" in *' -e '*) exit 0;; esac\n\
             echo 'ERROR 1064 (42000) at line 1: syntax error' >&2; exit 1\n"
        ),
    );
    // Far more than a pipe holds: with a reader left open, it would block forever.
    sb.fake(
        "mysqldump",
        "case \"$*\" in *--version*) echo 'mysqldump  Ver 8.4.3'; exit;; esac\nhead -c 50000000 /dev/zero\n",
    );
    let shop = sb.repo(
        "shop",
        &[
            (".gitignore", ".env\n"),
            (
                ".env",
                "DB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_DATABASE=shop\n",
            ),
        ],
    );
    sb.project("acme", &[&shop]);
    let setup = sb.setup();
    let path = sb.worktree(&shop, "one");
    let started = Instant::now();
    let report = setup.run(&setup.locate(&path, Some("3".into())).unwrap());
    assert!(started.elapsed() < Duration::from_secs(60), "it hung");
    let db = one(&report, StepKind::Database);
    assert_eq!(db.outcome, Outcome::Failed, "{db:?}");
    assert!(db.detail.contains("restore: ERROR 1064"), "{db:?}");
    assert!(
        sb.log()
            .contains("-e DROP DATABASE IF EXISTS `shop_termf_3`"),
        "the half-made copy is dropped"
    );
    assert!(
        fs::read_to_string(path.join(".env"))
            .unwrap()
            .contains("DB_DATABASE=shop\n"),
        "not pointed at a copy that does not exist"
    );
}

/// With the real Claude Code (cheap: haiku), on a throwaway repository and its SQLite database:
/// `TERMF_TEST_REAL_CLAUDE=1 cargo test -p termf-worktree real_setup_agent -- --ignored --nocapture`.
#[cfg(unix)]
#[test]
#[ignore]
fn runs_the_real_setup_agent_in_its_sandbox() {
    assert!(std::env::var_os("TERMF_TEST_REAL_CLAUDE").is_some());
    let sb = Sandbox::new("real-agent");
    let app = sb.repo(
        "app",
        &[
            (".gitignore", ".env\n*.sqlite\n"),
            (".env", "DB_CONNECTION=sqlite\n"),
        ],
    );
    let original = app.join("database/database.sqlite");
    fs::create_dir_all(original.parent().unwrap()).unwrap();
    rusqlite::Connection::open(&original)
        .unwrap()
        .execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (42);")
        .unwrap();
    sb.project("acme", &[&app]);
    let outside =
        termf_core::expand_tilde(&format!("~/termf-sandbox-probe-{}", std::process::id()));
    let notes = format!(
        "Migration: with the sqlite3 command line tool, run `CREATE TABLE IF NOT EXISTS notes (body TEXT); \
         INSERT INTO notes VALUES ('migrated');` on database/database.sqlite, the worktree's own copy. Then run \
         `touch {}` once and report whether it worked; do not retry it another way.",
        outside.display()
    );
    let mut recipe = view(&sb.config(), "acme").unwrap().recipe;
    recipe.agent = true;
    recipe.notes = notes;
    save(&sb.config(), "acme", recipe).unwrap();
    let mut setup = sb.setup();
    setup.claude = strings(&["claude", "--model", "haiku"]);
    let path = sb.worktree(&app, "one");
    let report = setup.run(&setup.locate(&path, Some("1".into())).unwrap());
    let agent = one(&report, StepKind::Agent);
    eprintln!("{report:#?}");
    let escaped = outside.exists();
    let _ = fs::remove_file(&outside);
    assert_eq!(agent.outcome, Outcome::Ran, "{agent:?}");
    assert!(!escaped, "the sandbox let Bash write outside the worktree");
    let copy = rusqlite::Connection::open(path.join("database/database.sqlite")).unwrap();
    let body: String = copy
        .query_row("SELECT body FROM notes", [], |r| r.get(0))
        .expect("the agent migrated the worktree's copy");
    assert_eq!(body, "migrated");
    let source = rusqlite::Connection::open(&original).unwrap();
    assert!(
        source
            .query_row("SELECT count(*) FROM notes", [], |r| r.get::<_, i64>(0))
            .is_err(),
        "the original is untouched"
    );
}
