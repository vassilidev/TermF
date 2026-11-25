//! The dev config scope, between two throwaway home folders: never the user's own.

use serde_json::Value;

use super::*;

fn dev_machine(sandbox: &Sandbox, name: &str) -> (Machine, PathBuf) {
    let mut m = sandbox.machine(name);
    let home = sandbox.root.join(format!("{name}-home"));
    fs::create_dir_all(&home).unwrap();
    m.engine.home = Some(home.clone());
    (m, home)
}

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Claude Code's folder for a project under `home`.
fn folder(home: &Path, project: &str) -> String {
    home.join(project)
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[test]
fn the_dev_config_travels_and_nothing_private_does() {
    let sandbox = Sandbox::new("dev");
    let remote = sandbox.folder();
    let (a, a_home) = dev_machine(&sandbox, "a");
    let (b, b_home) = dev_machine(&sandbox, "b");
    let (ca, cb) = (a_home.join(".claude"), b_home.join(".claude"));
    let (ah, bh) = (a_home.display().to_string(), b_home.display().to_string());

    put(&ca.join("CLAUDE.md"), "Short answers.\n");
    put(
        &ca.join("settings.json"),
        &format!(
            r#"{{"hooks": {{"Stop": [{{"hooks": [{{"type": "command", "command": "{ah}/.claude/hooks/fmt.sh"}}]}}]}},
"enabledPlugins": {{"hello@acme-tools": true}},
"extraKnownMarketplaces": {{"acme-tools": {{"source": {{"source": "github", "repo": "acme/tools"}}}}}}}}"#
        ),
    );
    put(&ca.join("keybindings.json"), "[]\n");
    put(&ca.join("hooks/fmt.sh"), "#!/bin/sh\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(ca.join("hooks/fmt.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    put(&ca.join("skills/review/SKILL.md"), "review\n");
    put(&ca.join("agents/qa.md"), "qa\n");
    put(&ca.join("commands/ship.md"), "ship\n");
    put(&ca.join("output-styles/terse.md"), "terse\n");
    let acme = ca.join("projects").join(folder(&a_home, "code/acme"));
    put(
        &acme.join("memory/MEMORY.md"),
        &format!("- acme lives in {ah}/code/acme\n"),
    );
    put(&acme.join("memory/build.md"), "make\n");
    put(
        &ca.join("projects")
            .join(folder(&a_home, "gone"))
            .join("memory/MEMORY.md"),
        "gone\n",
    );
    // Never synced: credentials, transcripts, caches, plugins' copies.
    put(&ca.join(".credentials.json"), "{\"token\": \"secret-a\"}");
    put(&acme.join("1234.jsonl"), "{\"said\": \"secret-b\"}\n");
    for never in [
        "todos/t.json",
        "shell-snapshots/s.sh",
        "statsig/s",
        "plugins/installed_plugins.json",
        "plugins/cache/acme-tools/hello/1.0.0/x.md",
        "history.jsonl",
    ] {
        put(&ca.join(never), "secret-c");
    }
    put(
        &a_home.join(".claude.json"),
        &format!(
            r#"{{"oauthAccount": {{"emailAddress": "secret-d@acme.test"}}, "mcpServers": {{"db": {{"command": "{ah}/bin/db"}}}}, "projects": {{}}}}"#
        ),
    );
    put(
        &a_home.join(".gitconfig"),
        &format!("[user]\n\tname = Alex\n[core]\n\texcludesfile = {ah}/.gitignore_global\n"),
    );
    put(&a_home.join(".gitignore_global"), ".DS_Store\n");
    // b has the project at the same place, and its own Claude Code account.
    fs::create_dir_all(b_home.join("code/acme")).unwrap();
    put(
        &b_home.join(".claude.json"),
        r#"{"oauthAccount": {"emailAddress": "alex@b.test"}, "numStartups": 3}"#,
    );

    a.setup(&remote);
    b.setup(&remote);
    // The excludes file once ~/.gitconfig names it here.
    b.push();

    assert_eq!(read(&cb.join("CLAUDE.md")), "Short answers.\n");
    let settings = read(&cb.join("settings.json"));
    assert!(
        settings.contains(&format!("\"{bh}/.claude/hooks/fmt.sh\""))
            && settings.contains("hello@acme-tools")
            && settings.contains("acme/tools")
            && !settings.contains(&ah),
        "{settings}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(cb.join("hooks/fmt.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "a hook stays executable");
    }
    for same in [
        "keybindings.json",
        "skills/review/SKILL.md",
        "agents/qa.md",
        "commands/ship.md",
        "output-styles/terse.md",
    ] {
        assert_eq!(read(&cb.join(same)), read(&ca.join(same)), "{same}");
    }
    let b_acme = cb.join("projects").join(folder(&b_home, "code/acme"));
    assert_eq!(
        read(&b_acme.join("memory/MEMORY.md")),
        format!("- acme lives in {bh}/code/acme\n")
    );
    assert_eq!(read(&b_acme.join("memory/build.md")), "make\n");
    assert!(
        !cb.join("projects").join(folder(&b_home, "gone")).exists(),
        "no memory for a project that is not here"
    );
    for never in [
        ".credentials.json",
        "todos",
        "shell-snapshots",
        "statsig",
        "plugins",
        "history.jsonl",
    ] {
        assert!(!cb.join(never).exists(), "{never}");
    }
    assert!(!b_acme.join("1234.jsonl").exists());

    let claude: Value = serde_json::from_str(&read(&b_home.join(".claude.json"))).unwrap();
    assert_eq!(claude["oauthAccount"]["emailAddress"], "alex@b.test");
    assert_eq!(claude["numStartups"], 3);
    assert_eq!(
        claude["mcpServers"]["db"]["command"],
        format!("{bh}/bin/db")
    );
    let keys: Vec<&String> = claude.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        ["oauthAccount", "numStartups", "mcpServers"],
        "its order stays"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(b_home.join(".claude.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "readable by the user only");
    }
    assert!(
        read(&b_home.join(".gitconfig"))
            .contains(&format!("excludesfile = {bh}/.gitignore_global"))
    );
    assert_eq!(read(&b_home.join(".gitignore_global")), ".DS_Store\n");

    // Nothing private in what the sync keeps, on either machine.
    for m in [&a, &b] {
        for (file, bytes) in all_files(&m.dir) {
            for secret in ["secret-a", "secret-b", "secret-c", "secret-d"] {
                assert!(!contains(&bytes, secret.as_bytes()), "{secret} in {file}");
            }
        }
    }

    // Claude Code rewrites the rest of ~/.claude.json: nothing to send.
    let mut doc = claude;
    doc["numStartups"] = 4.into();
    fs::write(b_home.join(".claude.json"), doc.to_string()).unwrap();
    assert!(b.push().pushed.is_empty());

    // Changed on both sides: side by side, and each keeps its own.
    put(&ca.join("CLAUDE.md"), "From a.\n");
    put(&cb.join("CLAUDE.md"), "From b.\n");
    a.push();
    let report = b.push();
    let conflict = report
        .conflicts
        .iter()
        .find(|c| c.path == "dev/claude/CLAUDE.md")
        .expect("a conflict");
    assert_eq!(
        (conflict.local.as_deref(), conflict.remote.as_deref()),
        (Some("From b.\n"), Some("From a.\n"))
    );
    assert_eq!(read(&cb.join("CLAUDE.md")), "From b.\n");
    b.engine
        .resolve(
            "dev/claude/CLAUDE.md",
            Resolution::Merged {
                text: "From both.\n".into(),
            },
        )
        .unwrap();
    a.push();
    assert_eq!(read(&ca.join("CLAUDE.md")), "From both.\n");

    // A skill deleted on a goes on b, its folder with it.
    fs::remove_dir_all(ca.join("skills/review")).unwrap();
    a.push();
    b.push();
    assert!(!cb.join("skills/review").exists());

    // Not chosen: nothing of the home folder moves.
    assert!(!Scope::DEFAULT.contains(&Scope::DevConfig));
    let (c, c_home) = dev_machine(&sandbox, "c");
    c.engine
        .setup(remote.clone(), PASSWORD, &Scope::DEFAULT, None)
        .unwrap();
    assert!(fs::read_dir(&c_home).unwrap().next().is_none());
}

/// A link in the home folder is neither followed nor written through.
#[cfg(unix)]
#[test]
fn links_stay_out_of_the_dev_config() {
    let sandbox = Sandbox::new("dev-links");
    let remote = sandbox.folder();
    let (a, a_home) = dev_machine(&sandbox, "a");
    let (b, b_home) = dev_machine(&sandbox, "b");
    let outside = sandbox.root.join("dotfiles");
    put(&outside.join("skills/linked/SKILL.md"), "linked\n");
    put(&outside.join("CLAUDE.md"), "linked\n");
    put(&a_home.join(".claude/skills/own/SKILL.md"), "own\n");
    put(&a_home.join(".claude/CLAUDE.md"), "from a\n");
    std::os::unix::fs::symlink(
        outside.join("skills/linked"),
        a_home.join(".claude/skills/linked"),
    )
    .unwrap();
    // b's CLAUDE.md is its dotfiles': never replaced by a sync.
    fs::create_dir_all(b_home.join(".claude")).unwrap();
    std::os::unix::fs::symlink(outside.join("CLAUDE.md"), b_home.join(".claude/CLAUDE.md"))
        .unwrap();
    a.setup(&remote);
    b.setup(&remote);
    assert_eq!(read(&b_home.join(".claude/skills/own/SKILL.md")), "own\n");
    assert!(!b_home.join(".claude/skills/linked").exists());
    assert!(b_home.join(".claude/CLAUDE.md").is_symlink());
    assert_eq!(read(&outside.join("CLAUDE.md")), "linked\n");
    assert!(b.engine.status().unwrap().last_error.is_none());
}
