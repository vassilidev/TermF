use std::path::Path;

use super::*;
use crate::{put, temp_dir};

/// A throwaway repository with these files.
fn repo(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = temp_dir(&format!("stack-{name}"));
    for (file, text) in files {
        put(&dir.join(file), text);
    }
    dir
}

fn tag<'a>(tags: &'a [Tag], name: &str) -> &'a Tag {
    tags.iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no {name} in {tags:?}"))
}

fn version(tags: &[Tag], name: &str) -> Option<String> {
    tag(tags, name).version.clone()
}

fn names(tags: &[Tag]) -> Vec<&str> {
    tags.iter().map(|t| t.name.as_str()).collect()
}

#[test]
fn reads_a_javascript_monorepo_with_installed_locked_and_declared_versions() {
    let dir = repo(
        "js",
        &[
            (
                "package.json",
                r#"{"private": true, "workspaces": ["apps/*"], "devDependencies": {"typescript": "^6.0.0"}}"#,
            ),
            (
                "apps/web/package.json",
                r#"{"dependencies": {"next": "^16.0.0", "react": "^19.0.0", "tailwindcss": "^4.1.0"}, "devDependencies": {"@playwright/test": "1.65.1", "vitest": "workspace:*"}}"#,
            ),
            ("apps/web/app/page.tsx", "export default function Page() {}"),
            (
                "apps/api/package.json",
                r#"{"engines": {"node": ">=24"}, "dependencies": {"express": "^5.0.0", "pg": "^8.13.0", "ioredis": "5"}}"#,
            ),
            (
                "node_modules/next/package.json",
                r#"{"name": "next", "version": "16.4.6"}"#,
            ),
            // What a dependency itself uses is not the project's stack.
            (
                "node_modules/some-lib/package.json",
                r#"{"dependencies": {"vue": "3.5.0"}}"#,
            ),
            (
                "pnpm-lock.yaml",
                "lockfileVersion: '9.0'\nimporters:\n  apps/web:\n    dependencies:\n      react:\n        specifier: ^19.0.0\n        version: 19.3.1(react-dom@19.3.1)\n",
            ),
        ],
    );
    let tags = detect(&dir).tags;
    assert_eq!(
        version(&tags, "nextjs").as_deref(),
        Some("16.4.6"),
        "installed"
    );
    assert_eq!(
        tag(&tags, "nextjs").source,
        "node_modules/next/package.json"
    );
    assert_eq!(
        version(&tags, "react").as_deref(),
        Some("19.3.1"),
        "locked by pnpm"
    );
    assert_eq!(tag(&tags, "react").source, "pnpm-lock.yaml");
    assert_eq!(
        version(&tags, "tailwind").as_deref(),
        Some("4.1.0"),
        "declared"
    );
    assert_eq!(version(&tags, "typescript").as_deref(), Some("6.0.0"));
    assert_eq!(version(&tags, "playwright").as_deref(), Some("1.65.1"));
    assert_eq!(version(&tags, "node").as_deref(), Some("24"));
    assert_eq!(version(&tags, "redis").as_deref(), Some("5"));
    assert!(
        names(&tags).contains(&"vitest"),
        "a workspace version is no version"
    );
    assert_eq!(version(&tags, "vitest"), None);
    for name in ["postgres", "sql", "web"] {
        assert!(names(&tags).contains(&name), "{name} in {tags:?}");
    }
    assert!(
        !names(&tags).contains(&"vue"),
        "node_modules is not read: {tags:?}"
    );
}

#[test]
fn reads_npm_and_yarn_lockfiles() {
    let npm = repo(
        "npm",
        &[
            (
                "package.json",
                r#"{"dependencies": {"vue": "^3.4.0", "nuxt": "^4.0.0"}}"#,
            ),
            (
                "package-lock.json",
                r#"{"packages": {"": {}, "node_modules/vue": {"version": "3.5.46"}, "node_modules/nuxt": {"version": "4.6.4"}}}"#,
            ),
        ],
    );
    let tags = detect(&npm).tags;
    assert_eq!(version(&tags, "vue").as_deref(), Some("3.5.46"));
    assert_eq!(tag(&tags, "nuxt").source, "package-lock.json");

    let yarn = repo(
        "yarn",
        &[
            (
                "package.json",
                r#"{"dependencies": {"@angular/core": "^22.0.0", "jest": "~30.0.0"}}"#,
            ),
            (
                "yarn.lock",
                "\"@angular/core@^22.0.0\":\n  version \"22.3.0\"\n  resolved \"x\"\n\njest@~30.0.0, jest@^30:\n  version \"30.5.2\"\n",
            ),
        ],
    );
    let tags = detect(&yarn).tags;
    assert_eq!(version(&tags, "angular").as_deref(), Some("22.3.0"));
    assert_eq!(version(&tags, "jest").as_deref(), Some("30.5.2"));
}

#[test]
fn reads_python_projects() {
    let dir = repo(
        "python",
        &[
            (
                "pyproject.toml",
                "[project]\nname = \"api\"\nrequires-python = \">=3.13\"\ndependencies = [\"Django>=6.0,<7\", \"psycopg[binary]>=3.2\", \"boto3\"]\n\n[dependency-groups]\ndev = [\"pytest>=9\"]\n",
            ),
            (
                "uv.lock",
                "version = 1\n\n[[package]]\nname = \"django\"\nversion = \"6.1.3\"\n\n[[package]]\nname = \"pytest\"\nversion = \"9.2.0\"\n",
            ),
            (
                "worker/requirements.txt",
                "# jobs\nfastapi==0.165.0  # api\n-r base.txt\nredis\n",
            ),
        ],
    );
    let tags = detect(&dir).tags;
    assert_eq!(version(&tags, "python").as_deref(), Some("3.13"));
    assert_eq!(version(&tags, "django").as_deref(), Some("6.1.3"));
    assert_eq!(tag(&tags, "django").source, "uv.lock");
    assert_eq!(version(&tags, "pytest").as_deref(), Some("9.2.0"));
    assert_eq!(version(&tags, "postgres").as_deref(), Some("3.2"));
    assert_eq!(version(&tags, "fastapi").as_deref(), Some("0.165.0"));
    assert_eq!(tag(&tags, "fastapi").source, "worker/requirements.txt");
    for name in ["aws", "redis", "sql"] {
        assert!(names(&tags).contains(&name), "{name}");
    }

    let poetry = repo(
        "poetry",
        &[
            (
                "pyproject.toml",
                "[tool.poetry.dependencies]\npython = \"^3.12\"\nflask = \"^3.1\"\n",
            ),
            (".python-version", "3.14.1\n"),
        ],
    );
    let tags = detect(&poetry).tags;
    assert_eq!(
        version(&tags, "python").as_deref(),
        Some("3.14.1"),
        "the pinned version wins"
    );
    assert_eq!(version(&tags, "flask").as_deref(), Some("3.1"));
}

#[test]
fn reads_go_rust_java_kotlin_swift_and_dotnet() {
    let dir = repo(
        "compiled",
        &[
            (
                "services/api/go.mod",
                "module example.com/api\n\ngo 1.27\n\nrequire (\n\tgithub.com/gin-gonic/gin v1.11.0\n\tgithub.com/jackc/pgx/v5 v5.7.1 // indirect\n)\n",
            ),
            (
                "tools/Cargo.toml",
                "[package]\nname = \"tools\"\nedition = \"2024\"\nrust-version = \"1.90\"\n",
            ),
            (
                "backend/pom.xml",
                "<project><parent><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-parent</artifactId>\n<version>4.1.2</version></parent><properties><java.version>25</java.version></properties></project>",
            ),
            (
                "mobile/build.gradle.kts",
                "plugins {\n    id(\"com.android.application\") version \"9.5.1\"\n    id(\"org.jetbrains.kotlin.android\") version \"2.4.21\"\n}\n",
            ),
            (
                "ios/Package.swift",
                "// swift-tools-version:6.2\nimport PackageDescription\n",
            ),
            ("ios/App.xcodeproj/project.pbxproj", "SDKROOT = iphoneos;\n"),
            (
                "web/Web.csproj",
                "<Project Sdk=\"Microsoft.NET.Sdk.Web\"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup></Project>",
            ),
        ],
    );
    let tags = detect(&dir).tags;
    assert_eq!(version(&tags, "go").as_deref(), Some("1.27"));
    assert_eq!(version(&tags, "gin").as_deref(), Some("1.11.0"));
    assert_eq!(version(&tags, "postgres").as_deref(), Some("5.7.1"));
    assert_eq!(version(&tags, "rust").as_deref(), Some("1.90"));
    assert_eq!(version(&tags, "spring-boot").as_deref(), Some("4.1.2"));
    assert_eq!(version(&tags, "java").as_deref(), Some("25"));
    assert_eq!(version(&tags, "android").as_deref(), Some("9.5.1"));
    assert_eq!(version(&tags, "kotlin").as_deref(), Some("2.4.21"));
    assert_eq!(version(&tags, "swift").as_deref(), Some("6.2"));
    assert!(names(&tags).contains(&"ios"));
    assert_eq!(version(&tags, "dotnet").as_deref(), Some("10.0"));
    assert!(names(&tags).contains(&"aspnetcore"));
}

#[test]
fn reads_react_native_expo_and_flutter_apps() {
    let dir = repo(
        "mobile",
        &[
            (
                "app/package.json",
                r#"{ "dependencies": { "expo": "~54.0.0", "react-native": "0.81.4", "react": "19.1.0" } }"#,
            ),
            (
                "flutter_app/pubspec.yaml",
                "name: shop\nenvironment:\n  sdk: ^3.9.0\n  flutter: \">=3.35.0\"\ndependencies:\n  flutter:\n    sdk: flutter\n",
            ),
            (
                "dart_cli/pubspec.yaml",
                "name: cli\ndependencies:\n  args: ^2.7.0\n",
            ),
        ],
    );
    let tags = detect(&dir).tags;
    assert_eq!(version(&tags, "expo").as_deref(), Some("54.0.0"));
    assert_eq!(version(&tags, "react-native").as_deref(), Some("0.81.4"));
    assert_eq!(version(&tags, "flutter").as_deref(), Some("3.35.0"));
    assert_eq!(tag(&tags, "flutter").source, "flutter_app/pubspec.yaml");

    let dart = repo(
        "dart-only",
        &[("pubspec.yaml", "name: cli\ndependencies:\n  args: ^2.7.0\n")],
    );
    assert!(
        !names(&detect(&dart).tags).contains(&"flutter"),
        "a Dart package without the Flutter SDK"
    );
}

#[test]
fn reads_php_and_ruby_with_their_lockfiles() {
    let dir = repo(
        "php-ruby",
        &[
            (
                "composer.json",
                r#"{"require": {"php": "^8.3", "laravel/framework": "^13.0", "predis/predis": "^3.0"}}"#,
            ),
            (
                "composer.lock",
                r#"{"packages": [{"name": "laravel/framework", "version": "v13.43.0"}], "packages-dev": []}"#,
            ),
            (
                "admin/Gemfile",
                "source \"https://rubygems.org\"\nruby \"4.0.10\"\ngem \"rails\", \"~> 8.1\"\ngem 'pg'\n",
            ),
            (
                "admin/Gemfile.lock",
                "GEM\n  specs:\n    rails (8.1.4.1)\n      actionpack (= 8.1.4.1)\n    pg (1.6.2)\n",
            ),
        ],
    );
    let tags = detect(&dir).tags;
    assert_eq!(version(&tags, "php").as_deref(), Some("8.3"));
    assert_eq!(version(&tags, "laravel").as_deref(), Some("13.43.0"));
    assert_eq!(tag(&tags, "laravel").source, "composer.lock");
    assert!(names(&tags).contains(&"redis"));
    assert_eq!(version(&tags, "ruby").as_deref(), Some("4.0.10"));
    assert_eq!(version(&tags, "rails").as_deref(), Some("8.1.4.1"));
    assert_eq!(version(&tags, "postgres").as_deref(), Some("1.6.2"));
}

#[test]
fn reads_the_test_frameworks_of_php_ruby_and_java() {
    let php = repo(
        "pest",
        &[
            (
                "composer.json",
                r#"{"require-dev": {"pestphp/pest": "^5.3", "phpunit/phpunit": "^12.0"}}"#,
            ),
            (
                "composer.lock",
                r#"{"packages": [], "packages-dev": [{"name": "pestphp/pest", "version": "v5.3.1"}]}"#,
            ),
        ],
    );
    let tags = detect(&php).tags;
    assert_eq!(version(&tags, "pest").as_deref(), Some("5.3.1"));
    assert_eq!(tag(&tags, "pest").source, "composer.lock");
    assert_eq!(version(&tags, "phpunit").as_deref(), Some("12.0"));

    let ruby = repo(
        "rspec",
        &[
            (
                "Gemfile",
                "group :test do\n  gem \"rspec-rails\", \"~> 8.0\"\n  gem \"minitest\"\nend\n",
            ),
            (
                "Gemfile.lock",
                "GEM\n  specs:\n    minitest (5.26.0)\n    rspec-rails (8.0.2)\n",
            ),
        ],
    );
    let tags = detect(&ruby).tags;
    assert_eq!(version(&tags, "rspec").as_deref(), Some("8.0.2"));
    assert_eq!(version(&tags, "minitest").as_deref(), Some("5.26.0"));

    let maven = repo(
        "junit-maven",
        &[(
            "pom.xml",
            "<project><dependencies>\n<dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-test</artifactId></dependency>\n<dependency><groupId>org.junit.jupiter</groupId><artifactId>junit-jupiter</artifactId></dependency>\n<dependency><groupId>x</groupId><artifactId>y</artifactId><version>9.9</version></dependency>\n</dependencies></project>",
        )],
    );
    let tags = detect(&maven).tags;
    assert!(names(&tags).contains(&"junit"));
    assert_eq!(
        version(&tags, "junit"),
        None,
        "managed by Spring Boot: another dependency's version is not junit's"
    );

    let gradle = repo(
        "junit-gradle",
        &[(
            "build.gradle",
            "dependencies {\n    testImplementation 'org.junit.jupiter:junit-jupiter:5.13.4'\n}\n",
        )],
    );
    assert_eq!(
        version(&detect(&gradle).tags, "junit").as_deref(),
        Some("5.13.4")
    );
    let spring = repo(
        "junit-spring",
        &[(
            "build.gradle.kts",
            "plugins { id(\"org.springframework.boot\") version \"4.1.2\" }\ndependencies { testImplementation(\"org.springframework.boot:spring-boot-starter-test\") }\n",
        )],
    );
    let tags = detect(&spring).tags;
    assert!(
        names(&tags).contains(&"junit"),
        "by default with Spring Boot's test starter"
    );

    let android = repo(
        "junit4",
        &[
            (
                "app/build.gradle.kts",
                "plugins { id(\"com.android.application\") }\ndependencies { testImplementation(\"junit:junit:4.13.2\") }\n",
            ),
            (
                "lib/pom.xml",
                "<project><dependencies><dependency><groupId>junit</groupId><artifactId>junit</artifactId><version>4.13.2</version></dependency></dependencies></project>",
            ),
        ],
    );
    let tags = detect(&android).tags;
    assert!(names(&tags).contains(&"android"));
    assert!(
        !names(&tags).contains(&"junit"),
        "JUnit 4 is not what the junit skill covers: {tags:?}"
    );
}

#[test]
fn reads_containers_infrastructure_and_ci() {
    let dir = repo(
        "infra",
        &[
            ("Dockerfile", "FROM node:24\n"),
            (
                "compose.yaml",
                "services:\n  db:\n    image: registry.local:5000/postgres:17-alpine\n  cache:\n    image: valkey/valkey:9.2\n  app:\n    build: .\n",
            ),
            (
                "infra/main.tf",
                "terraform {\n  required_version = \">= 1.17\"\n  required_providers {\n    aws = { source = \"hashicorp/aws\" }\n  }\n}\n",
            ),
            ("charts/app/Chart.yaml", "apiVersion: v2\nname: app\n"),
            (
                "deploy/service.yaml",
                "apiVersion: v1\nkind: Service\nmetadata:\n  name: app\n",
            ),
            ("docs/mkdocs.yml", "site_name: Docs\n"),
            ("Makefile", "test:\n\tgo test ./...\n"),
            (".github/workflows/ci.yml", "on: push\n"),
            (".github/ISSUE_TEMPLATE/bug.yml", "apiVersion: x\nkind: y\n"),
        ],
    );
    let tags = detect(&dir).tags;
    assert_eq!(
        names(&tags),
        [
            "aws",
            "compose",
            "docker",
            "github-actions",
            "helm",
            "kubernetes",
            "make",
            "postgres",
            "sql",
            "terraform",
            "valkey"
        ]
    );
    assert!(
        !names(&tags).contains(&"github"),
        "a workflow folder is no GitHub remote"
    );
    assert_eq!(version(&tags, "postgres").as_deref(), Some("17"));
    assert_eq!(version(&tags, "valkey").as_deref(), Some("9.2"));
    assert_eq!(version(&tags, "terraform").as_deref(), Some("1.17"));
    assert_eq!(
        tag(&tags, "kubernetes").source,
        "deploy/service.yaml",
        "the shallowest"
    );
    assert_eq!(tag(&tags, "helm").source, "charts/app/Chart.yaml");
    assert!(
        TAGS.iter()
            .all(|t| TAGS.iter().filter(|u| u == &t).count() == 1)
    );
    for t in &tags {
        assert!(
            TAGS.contains(&t.name.as_str()),
            "{} is listed in TAGS",
            t.name
        );
    }
}

#[test]
fn keeps_to_the_monorepo_cap_and_skips_dependencies_and_build_output() {
    let mut files: Vec<(String, String)> = (0..60)
        .map(|i| (format!("packages/p{i:02}/package.json"), "{}".to_owned()))
        .collect();
    files.push((
        "packages/p99/package.json".into(),
        r#"{"dependencies": {"svelte": "5"}}"#.into(),
    ));
    files.push((
        "vendor/x/composer.json".into(),
        r#"{"require": {"laravel/framework": "13"}}"#.into(),
    ));
    files.push(("target/debug/build/Cargo.toml".into(), "[package]\n".into()));
    files.push((".venv/lib/pyproject.toml".into(), "[project]\n".into()));
    files.push((
        "a/b/c/d/package.json".into(),
        r#"{"dependencies": {"react": "19"}}"#.into(),
    ));
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let survey = detect(&repo("cap", &files));
    assert_eq!(survey.manifests.len(), MAX_MANIFESTS);
    assert!(
        survey.tags.is_empty(),
        "past the cap, too deep or skipped: {:?}",
        survey.tags
    );
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

#[test]
fn reads_a_repository_again_only_when_its_head_moves() {
    let dir = repo(
        "cache",
        &[("package.json", r#"{"dependencies": {"react": "19.3.1"}}"#)],
    );
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "one"]);
    assert_eq!(names(&stack_tags(&dir)), ["react"]);

    put(
        &dir.join("package.json"),
        r#"{"dependencies": {"vue": "3"}}"#,
    );
    assert_eq!(names(&stack_tags(&dir)), ["react"], "same HEAD, same tags");
    git(&dir, &["commit", "-q", "-am", "two"]);
    assert_eq!(names(&stack_tags(&dir)), ["vue"]);

    let plain = repo("no-git", &[("go.mod", "module x\n")]);
    assert_eq!(names(&stack_tags(&plain)), ["go"]);
    put(&plain.join("Cargo.toml"), "[package]\n");
    assert_eq!(
        names(&stack_tags(&plain)),
        ["go"],
        "outside git, kept a few seconds"
    );
}

#[test]
fn a_remote_on_github_brings_the_github_tag_to_the_repository_and_its_worktrees() {
    let dir = repo("github", &[("go.mod", "module x\n")]);
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "one"]);
    // Hosts that only contain "github.com" somewhere are not GitHub.
    for (name, url) in [
        ("mirror", "https://gitlab.com/acme/github.com-mirror.git"),
        ("lookalike", "git@notgithub.com:acme/api.git"),
        ("enterprise", "https://github.acme.com/acme/api.git"),
    ] {
        git(&dir, &["remote", "add", name, url]);
    }
    assert_eq!(names(&stack_tags(&dir)), ["go"]);

    git(
        &dir,
        &[
            "remote",
            "add",
            "origin",
            "ssh://git@ssh.github.com:443/acme/api.git",
        ],
    );
    let tags = stack_tags(&dir);
    assert_eq!(
        names(&tags),
        ["github", "go"],
        "a remote added since the last read counts, though HEAD did not move"
    );
    assert_eq!(tag(&tags, "github").source, ".git/config");
    for url in ["git@github.com:acme/api.git", "https://GitHub.com/acme/api"] {
        git(&dir, &["remote", "set-url", "origin", url]);
        assert_eq!(names(&detect(&dir).tags), ["github", "go"], "{url}");
    }

    let worktree = dir.with_file_name(format!("{}-wt", dir.file_name().unwrap().to_string_lossy()));
    let _ = fs::remove_dir_all(&worktree);
    git(
        &dir,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "task",
            worktree.to_str().unwrap(),
        ],
    );
    assert!(worktree.join(".git").is_file());
    assert_eq!(names(&detect(&worktree).tags), ["github", "go"]);
}

#[test]
fn finds_the_skills_and_docs_that_installed_packages_ship() {
    let skill = |name: &str| format!("---\nname: {name}\ndescription: {name}\n---\nBody.\n");
    let dir = repo(
        "packages",
        &[
            (
                "package.json",
                r#"{"dependencies": {"ai": "^7", "next": "16", "@angular/core": "22", "@tanstack/react-router": "1"}, "devDependencies": {"@playwright/test": "1.65.1"}}"#,
            ),
            ("node_modules/ai/package.json", r#"{"version": "7.0.177"}"#),
            ("node_modules/ai/skills/ai-sdk/SKILL.md", &skill("ai-sdk")),
            ("node_modules/ai/skills/ai-sdk/references/streaming.md", "x"),
            (
                "node_modules/@tanstack/react-router/package.json",
                r#"{"version": "1.170.46"}"#,
            ),
            (
                "node_modules/@tanstack/react-router/skills/compositions/router-query/SKILL.md",
                &skill("router-query"),
            ),
            (
                "node_modules/playwright-core/package.json",
                r#"{"version": "1.65.1"}"#,
            ),
            (
                "node_modules/playwright-core/lib/tools/skills/playwright-cli/SKILL.md",
                &skill("playwright-cli"),
            ),
            ("node_modules/next/package.json", r#"{"version": "16.4.6"}"#),
            ("node_modules/next/dist/docs/index.md", "# Docs"),
            (
                "node_modules/@angular/core/package.json",
                r#"{"version": "22.3.0", "angular": {"bestPractices": {"path": "./resources/best-practices.md", "format": "markdown"}}}"#,
            ),
            (
                "node_modules/@angular/core/resources/best-practices.md",
                "# Best",
            ),
            // Not a direct dependency: what it ships is not offered.
            ("node_modules/left-pad/skills/pad/SKILL.md", &skill("pad")),
            (
                "api/pyproject.toml",
                "[project]\ndependencies = [\"fastapi[standard]>=0.133\"]\n",
            ),
            (
                "api/.venv/lib/python3.15/site-packages/fastapi-0.165.0.dist-info/METADATA",
                "x",
            ),
            (
                "api/.venv/lib/python3.15/site-packages/fastapi/.agents/skills/fastapi/SKILL.md",
                &skill("fastapi"),
            ),
            (
                "admin/composer.json",
                r#"{"require": {"filament/filament": "^5.0", "laravel/boost": "^2.10"}}"#,
            ),
            (
                "admin/composer.lock",
                r#"{"packages": [{"name": "filament/filament", "version": "v5.2.0"}], "packages-dev": [{"name": "laravel/boost", "version": "v2.10.4"}]}"#,
            ),
            (
                "admin/vendor/filament/filament/resources/boost/skills/filament-development/SKILL.md",
                &skill("filament-development"),
            ),
            (
                "admin/vendor/filament/filament/resources/boost/skills/blade-only/SKILL.blade.php",
                "@php",
            ),
            ("admin/vendor/laravel/boost/.ai/laravel/core.blade.php", "x"),
        ],
    );
    let stack = RepoStack::read("acme", &dir);
    let skills: Vec<(&str, &str, Option<&str>)> = stack
        .skills
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.package.name.as_str(),
                s.package.version.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        skills,
        [
            ("ai-sdk", "ai", Some("7.0.177")),
            ("router-query", "@tanstack/react-router", Some("1.170.46")),
            ("playwright-cli", "playwright-core", Some("1.65.1")),
            ("filament-development", "filament/filament", Some("5.2.0")),
            ("fastapi", "fastapi", Some("0.165.0")),
        ]
    );
    assert_eq!(
        stack.skills[0].path,
        dir.join("node_modules/ai/skills/ai-sdk")
    );
    let guides: Vec<(&str, &str)> = stack
        .guides
        .iter()
        .map(|g| (g.package.name.as_str(), g.path.as_str()))
        .collect();
    assert_eq!(
        guides,
        [
            ("next", "node_modules/next/dist/docs"),
            (
                "@angular/core",
                "node_modules/@angular/core/resources/best-practices.md"
            ),
        ]
    );
    assert!(!stack.trusted, "trust is the caller's to read");

    let (mcp, sources) = offers(&RepoStack {
        path: dir.join("admin"),
        ..stack.clone()
    });
    let servers: Vec<&str> = mcp.iter().map(|m| m.name).collect();
    assert_eq!(
        servers,
        [
            "next-devtools",
            "angular-cli",
            "playwright",
            "laravel-boost"
        ]
    );
    let boost = mcp.iter().find(|m| m.name == "laravel-boost").unwrap();
    assert_eq!(
        boost.server["args"],
        serde_json::json!(["artisan", "boost:mcp"])
    );
    assert!(boost.runs_code);
    assert_eq!(mcp[0].server["command"], "npx");
    let repos: Vec<(&str, &str)> = sources
        .iter()
        .map(|s| (s.repo, s.command.as_str()))
        .collect();
    assert_eq!(
        repos,
        [
            ("laravel/boost", "php artisan boost:install"),
            (
                "angular/skills",
                "npx skills add angular/skills -a claude-code"
            ),
            (
                "vercel/next.js",
                "claude plugin marketplace add vercel/next.js"
            ),
        ]
    );
    put(&dir.join("admin/boost.json"), "{}");
    let (_, sources) = offers(&RepoStack {
        path: dir.join("admin"),
        ..stack
    });
    assert!(
        sources.iter().all(|s| s.repo != "laravel/boost"),
        "installed already"
    );
}

#[test]
fn looks_at_installed_packages_again_only_when_they_change() {
    let dir = repo(
        "installed-cache",
        &[
            ("package.json", r#"{"dependencies": {"ai": "^7"}}"#),
            ("package-lock.json", "{}"),
        ],
    );
    assert!(RepoStack::read("acme", &dir).skills.is_empty());

    put(
        &dir.join("node_modules/ai/skills/ai-sdk/SKILL.md"),
        "---\nname: ai-sdk\n---\n",
    );
    // An install rewrites the lockfile: set its time so that the change shows whatever the clock's resolution.
    let lock = fs::File::options()
        .write(true)
        .open(dir.join("package-lock.json"))
        .unwrap();
    lock.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    let names: Vec<String> = RepoStack::read("acme", &dir)
        .skills
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, ["ai-sdk"]);

    fs::remove_file(dir.join("node_modules/ai/skills/ai-sdk/SKILL.md")).unwrap();
    assert_eq!(
        RepoStack::read("other", &dir).skills.len(),
        1,
        "nothing the package manager touches changed: kept"
    );
    assert_eq!(RepoStack::read("other", &dir).name, "other");
}

#[test]
fn cleans_versions_and_checks_ranges() {
    for (spec, want) in [
        ("^16.1.0", Some("16.1.0")),
        ("~> 8.1", Some("8.1")),
        (">=5.0,<6", Some("5.0")),
        ("v1.11.0", Some("1.11.0")),
        ("16.x", Some("16")),
        ("10.0-windows", Some("10.0-windows")),
        ("workspace:*", None),
        ("file:../x", None),
        ("latest", None),
        ("*", None),
    ] {
        assert_eq!(clean(spec).as_deref(), want, "{spec}");
    }
    for (range, version, want) in [
        (">=16", "16.4.6", Some(true)),
        (">=16", "14.2.0", Some(false)),
        (">=15 <17", "16.0.0", Some(true)),
        (">=15 <17", "17.0.0", Some(false)),
        ("16", "16.4.6", Some(true)),
        ("16.x", "15.9", Some(false)),
        ("<5 || >=6", "6.1", Some(true)),
        (">=3.13", "3.13", Some(true)),
        ("~16", "16.0", None),
        (">=16", "next", None),
    ] {
        assert_eq!(covers(range, version), want, "{range} {version}");
    }
}

#[test]
fn offers_one_language_server_a_language_and_declares_only_the_installed_ones() {
    let bin = temp_dir("stack-lsp-bin");
    put(&bin.join("rust-analyzer"), "");
    put(&bin.join("basedpyright-langserver"), "");
    let tags: Vec<Tag> = ["docker", "python", "rust", "node", "typescript"]
        .iter()
        .map(|t| Tag {
            name: (*t).into(),
            version: None,
            source: "x".into(),
        })
        .collect();
    let servers = language_servers(&tags, bin.as_os_str());
    let found: Vec<(&str, &str, bool)> = servers
        .iter()
        .map(|s| (s.name, s.r#for.as_str(), s.installed))
        .collect();
    assert_eq!(
        found,
        [
            ("rust-analyzer", "rust", true),
            ("typescript", "node", false),
            ("basedpyright", "python", true),
        ],
        "basedpyright takes pyright's place once installed; docker has none"
    );
    assert!(servers[1].docs.ends_with("/plugins/typescript-lsp"));

    let declared = Value::Object(lsp_json(&servers));
    assert_eq!(
        declared,
        serde_json::json!({
            "rust-analyzer": { "command": "rust-analyzer", "extensionToLanguage": { ".rs": "rust" } },
            "basedpyright": {
                "command": "basedpyright-langserver",
                "args": ["--stdio"],
                "extensionToLanguage": { ".py": "python", ".pyi": "python" },
            },
        })
    );

    let none = temp_dir("stack-lsp-none");
    let missing = language_servers(&tags, none.as_os_str());
    assert!(missing.iter().all(|s| !s.installed));
    assert_eq!(
        missing[2].name, "pyright",
        "the official one is the one to install"
    );
    assert!(lsp_json(&missing).is_empty());
}
