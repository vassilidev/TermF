use std::fs;
use std::process::Command;

use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("termf-setup-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn run(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}");
}

#[test]
fn reads_remotes_without_their_credentials() {
    let r = |url: &str| remote(url).map(|r| format!("{} {}", r.host, r.slug));
    assert_eq!(
        r("git@github.com:acme/api.git").unwrap(),
        "github.com acme/api"
    );
    assert_eq!(
        r("https://alex:s3cret@GitLab.com/acme/infra/").unwrap(),
        "gitlab.com acme/infra"
    );
    assert_eq!(
        r("ssh://git@bitbucket.org:22/acme/mobile.git").unwrap(),
        "bitbucket.org acme/mobile"
    );
    assert_eq!(r("/srv/git/api.git"), None);
    assert_eq!(r("C:\\code\\api"), None);
    assert_eq!(r("file:///srv/git/api"), None);
}

#[test]
fn reads_the_branch_and_the_changes() {
    let text = "# branch.oid abc\n# branch.head main\n1 .M N... 100644 100644 100644 a b src/x.rs\n? notes.md\n";
    assert_eq!(status(text), (Some("main".into()), 2));
    assert_eq!(status("# branch.head (detached)\n"), (None, 0));
}

#[test]
fn finds_ticket_keys_cited_in_commits() {
    let subjects = "ACME-12 fix login\n[ACME-14] tests\nUTF-8 everywhere\nUTF-8 again\nOPS-3 deploy\nAcme-1 no\nACME-15";
    assert_eq!(ticket_keys(subjects), ["ACME"]);
    assert_eq!(
        ticket_keys("Merge ACME-142-fix-login\nOPS-3 deploy\nOPS-4-retry"),
        ["OPS"]
    );
    assert_eq!(ticket_keys("ACME-142-fix-login\nACME-143-docs"), ["ACME"]);
    assert!(ticket_keys("bump to ES-2022, ES-2023").is_empty());
}

#[test]
fn finds_the_commands_a_repository_declares() {
    let dir = temp("scripts");
    put(
        &dir.join("package.json"),
        r#"{ "scripts": { "dev": "vite", "build": "vite build", "test": "vitest", "check": "svelte-check" } }"#,
    );
    put(
        &dir.join("Makefile"),
        "test:\n\tpnpm test\nlint :\n\techo\n",
    );
    let recipe = termf_worktree::detect(&dir, &[]);
    let found = |recipe: &RepoRecipe| -> Vec<String> {
        scripts(&dir, recipe)
            .into_iter()
            .map(|s| format!("{} {}", s.kind, s.run))
            .collect()
    };
    assert_eq!(
        found(&recipe),
        [
            "dev npm run dev",
            "build npm run build",
            "test npm test",
            "lint npm run check"
        ]
    );
    // What `npm init` writes is no test: it always fails, and would block every delivery as checks.
    put(
        &dir.join("package.json"),
        r#"{ "scripts": { "test": "echo \"Error: no test specified\" && exit 1", "build": "tsc" } }"#,
    );
    let init = found(&termf_worktree::detect(&dir, &[]));
    assert!(!init.iter().any(|s| s.starts_with("test npm")), "{init:?}");
    assert_eq!(init[0], "build npm run build");
    put(
        &dir.join("package.json"),
        r#"{ "scripts": { "dev": "vite", "build": "vite build", "test": "vitest", "check": "svelte-check" } }"#,
    );
    put(&dir.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'\n");
    let pnpm = termf_worktree::detect(&dir, &[]);
    assert_eq!(found(&pnpm)[0], "dev pnpm dev");

    fs::remove_file(dir.join("package.json")).unwrap();
    put(&dir.join("Cargo.toml"), "[package]\nname = \"x\"\n");
    assert_eq!(
        found(&termf_worktree::detect(&dir, &[])),
        ["build cargo build", "test make test", "lint make lint"]
    );
    fs::remove_dir_all(&dir).unwrap();

    // PHP: Composer's scripts, else Laravel's own test command.
    let php = temp("scripts-php");
    put(
        &php.join("composer.json"),
        r#"{ "require": { "laravel/framework": "^11.0" } }"#,
    );
    put(&php.join("artisan"), "#!/usr/bin/env php\n");
    let found = |dir: &Path| -> Vec<String> {
        scripts(dir, &termf_worktree::detect(dir, &[]))
            .into_iter()
            .map(|s| format!("{} {}", s.kind, s.run))
            .collect()
    };
    assert_eq!(found(&php), ["test php artisan test"]);
    put(
        &php.join("composer.json"),
        r#"{ "scripts": { "test": "pest", "lint": "pint --test" } }"#,
    );
    assert_eq!(found(&php), ["test composer test", "lint composer lint"]);
    fs::remove_dir_all(&php).unwrap();
}

#[test]
fn cards_tell_the_branch_remote_changes_and_stack() {
    let dir = temp("cards");
    let api = dir.join("api");
    put(
        &api.join("package.json"),
        r#"{ "dependencies": { "react": "^19.1.0" } }"#,
    );
    run(&api, &["init", "-q", "-b", "main"]);
    run(
        &api,
        &["remote", "add", "origin", "git@github.com:acme/api.git"],
    );
    run(&api, &["add", "."]);
    run(&api, &["commit", "-q", "-m", "first"]);
    put(&api.join("notes.md"), "x\n");
    let plain = dir.join("scratch");
    fs::create_dir_all(&plain).unwrap();
    run(&plain, &["init", "-q"]);

    let found = cards(&[
        api.to_string_lossy().into_owned(),
        plain.to_string_lossy().into_owned(),
    ]);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].name, "api");
    assert_eq!(found[0].branch.as_deref(), Some("main"));
    assert_eq!(found[0].remote.as_ref().unwrap().slug, "acme/api");
    assert_eq!(found[0].changes, 1);
    assert!(
        found[0].stack.iter().any(|t| t == "react"),
        "{:?}",
        found[0].stack
    );
    assert_eq!(found[1].name, "scratch");
    assert_eq!(found[1].remote, None);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn keeps_facts_per_test_globs() {
    let dir = temp("facts");
    put(&dir.join("src/a.spec.ts"), "test\n");
    put(&dir.join("src/b.test.ts"), "test\n");
    let spec = facts_of(&dir, &["**/*.spec.ts".into()]);
    let test = facts_of(&dir, &["**/*.test.ts".into()]);
    assert_ne!(spec.tests, test.tests, "{:?} {:?}", spec.tests, test.tests);
}

#[test]
fn detects_and_proposes_before_the_project_exists() {
    let dir = temp("draft");
    let config = dir.join("config");
    fs::create_dir_all(&config).unwrap();
    let web = dir.join("web");
    put(
        &web.join("package.json"),
        r#"{ "dependencies": { "react": "^19.1.0" }, "scripts": { "test": "vitest" } }"#,
    );
    put(&web.join(".github/workflows/ci.yml"), "on: push\n");
    put(&web.join("src/App.test.tsx"), "test\n");
    run(&web, &["init", "-q"]);
    run(&web, &["add", "."]);
    run(&web, &["commit", "-q", "-m", "WEB-1 first"]);
    run(&web, &["commit", "-q", "--allow-empty", "-m", "WEB-2 next"]);
    let path = web.to_string_lossy().into_owned();

    let found = detect(&config, &path).unwrap();
    assert_eq!(found.facts.workflows, ["ci.yml"]);
    assert_eq!(found.scripts[0].run, "npm test");
    assert_eq!(found.keys, ["WEB"]);
    assert!(found.tags.iter().any(|t| t.name == "react"));

    let draft = draft(&config, std::slice::from_ref(&path)).unwrap();
    let kept: Vec<&str> = draft
        .roles
        .iter()
        .filter(|r| r.keep)
        .map(|r| r.role.as_str())
        .collect();
    for role in ["front", "qa", "devops"] {
        assert!(kept.contains(&role), "{role} in {kept:?}");
    }
    assert!(!kept.contains(&"back"), "{kept:?}");
    assert!(draft.team.team.is_some());
    assert!(
        draft
            .skills
            .iter()
            .any(|s| s.by.iter().any(|t| t == "react"))
    );

    let trackers: termf_tickets::Trackers =
        serde_json::from_str(r#"{ "github": { "issues": true }, "mcp": {} }"#).unwrap();
    // The test command the detection found becomes the repository's checks, which the team runs before delivering.
    let checks = BTreeMap::from([(path.clone(), "npm test".to_owned())]);
    let text = preview(
        " Acme ",
        std::slice::from_ref(&path),
        Some(&draft.team),
        Some(&trackers),
        &checks,
    )
    .unwrap();
    assert!(text.starts_with("name = \"Acme\"\n"), "{text}");
    assert!(text.contains("[[repos]]"), "{text}");
    assert!(text.contains("checks = \"npm test\""), "{text}");
    assert!(text.contains("[[squads]]"), "{text}");
    assert!(text.contains("[trackers.github]"), "{text}");
    assert!(preview("  ", &[], None, None, &BTreeMap::new()).is_err());
    fs::remove_dir_all(&dir).unwrap();
}
