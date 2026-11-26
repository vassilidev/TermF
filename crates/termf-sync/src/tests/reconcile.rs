//! Two machines that each had their own files before syncing: everything comes together, nothing is replaced or
//! lost in silence, a real disagreement waits for the user, and the last sync can be undone.

use serde_json::Value;

use super::*;

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn home_machine(sandbox: &Sandbox, name: &str) -> (Machine, PathBuf) {
    let mut m = sandbox.machine(name);
    let home = sandbox.root.join(format!("{name}-home"));
    fs::create_dir_all(home.join(".ssh")).unwrap();
    m.engine.home = Some(home.clone());
    (m, home)
}

#[test]
fn settings_of_two_machines_combine_and_the_preview_says_how_first() {
    let sandbox = Sandbox::new("combine");
    let remote = sandbox.folder();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put(
        "termf.toml",
        "scan_depth = 3\neditor = \"/Applications/Zed.app/Contents/MacOS/cli\"\n\n[keys.macos]\npalette = \"Cmd+K\"\n",
    );
    a.put("projects/acme.toml", "name = \"Acme\"\n");
    a.put("library/commands/a.md", "from a\n");
    b.put(
        "termf.toml",
        "# b's own\nkeep_awake = \"always\"\n\n[keys.macos]\n\"terminal.new\" = \"Cmd+T\"\n",
    );
    b.put("projects/acme.toml", "name = \"Acme Inc\"\n");
    b.put("projects/web.toml", "name = \"Web\"\n");
    a.setup(&remote);

    let shared = sandbox.root.join("shared");
    let (remote_before, b_before) = (all_files(&shared), all_files(&b.dir));
    let preview = b
        .engine
        .preview(remote.clone(), PASSWORD, &Scope::DEFAULT)
        .unwrap();
    assert!(
        preview.pulled.contains(&"library/commands/a.md".into()),
        "{preview:?}"
    );
    assert_eq!(preview.merged, ["termf.toml"]);
    assert_eq!(preview.conflicts.len(), 1, "{preview:?}");
    assert_eq!(
        (
            preview.conflicts[0].path.as_str(),
            &preview.conflicts[0].keys
        ),
        ("projects/acme.toml", &vec!["name".to_owned()])
    );
    assert!(preview.pushed.contains(&"projects/web.toml".into()));
    // Nothing applied before the user agrees: neither here nor on the remote, and sync is still off here.
    assert_eq!(all_files(&shared), remote_before);
    let b_after: Vec<_> = all_files(&b.dir)
        .into_iter()
        .filter(|(p, _)| !p.contains("/.sync/"))
        .collect();
    assert_eq!(b_after, b_before);
    assert!(!b.engine.enabled());
    assert!(
        b.engine
            .preview(remote.clone(), "wrong password!", &Scope::DEFAULT)
            .is_err()
    );

    let joined = b.setup(&remote);
    assert_eq!(joined.report.merged, ["termf.toml"]);
    a.push();
    for m in [&a, &b] {
        let settings = m.toml("termf.toml");
        assert_eq!(settings["scan_depth"].as_integer(), Some(3));
        assert_eq!(settings["keep_awake"].as_str(), Some("always"));
        assert_eq!(settings["keys"]["macos"]["palette"].as_str(), Some("Cmd+K"));
        assert_eq!(
            settings["keys"]["macos"]["terminal.new"].as_str(),
            Some("Cmd+T")
        );
        assert_eq!(m.read("library/commands/a.md"), "from a\n");
        assert_eq!(m.read("projects/web.toml"), "name = \"Web\"\n");
    }
    assert!(b.read("termf.toml").starts_with("# b's own\n"));
    // An application's path stays on its machine.
    assert!(a.read("termf.toml").contains("Zed.app"));
    assert!(!b.read("termf.toml").contains("editor"));
    // The real disagreement: each keeps its own until the user chooses.
    assert_eq!(a.read("projects/acme.toml"), "name = \"Acme\"\n");
    assert_eq!(b.read("projects/acme.toml"), "name = \"Acme Inc\"\n");
    b.engine
        .resolve("projects/acme.toml", Resolution::Local)
        .unwrap();
    a.push();
    assert_eq!(a.read("projects/acme.toml"), "name = \"Acme Inc\"\n");
}

#[test]
fn known_hosts_add_up_and_never_lose_a_line() {
    let sandbox = Sandbox::new("known-hosts");
    let remote = sandbox.folder();
    let (a, a_home) = home_machine(&sandbox, "a");
    let (b, b_home) = home_machine(&sandbox, "b");
    let (ka, kb) = (
        a_home.join(".ssh/known_hosts"),
        b_home.join(".ssh/known_hosts"),
    );
    let hashed = "|1|c2FsdA==|aGFzaA== ssh-ed25519 AAAAhashed";
    put(&ka, &format!("github.com ssh-ed25519 AAAAgh\n{hashed}\n"));
    put(&kb, &format!("{hashed}\ngitlab.com ssh-ed25519 AAAAgl"));
    a.setup(&remote);
    b.setup(&remote);
    a.push();
    fn both(text: &str) -> Vec<&str> {
        let mut lines: Vec<&str> = text.lines().collect();
        lines.sort();
        lines
    }
    for file in [&ka, &kb] {
        assert_eq!(
            both(&read(file)),
            [
                "github.com ssh-ed25519 AAAAgh",
                "gitlab.com ssh-ed25519 AAAAgl",
                hashed
            ],
            "once each"
        );
    }
    assert!(read(&kb).starts_with(hashed), "this machine's lines first");

    // `ssh-keygen -R` on b, and a new host there: the new one goes everywhere, the removed one comes back.
    put(
        &kb,
        &(read(&kb).replace("github.com ssh-ed25519 AAAAgh\n", "")
            + "box.acme.test ssh-ed25519 AAAAbox\n"),
    );
    b.push();
    a.push();
    b.push();
    for file in [&ka, &kb] {
        let text = read(file);
        assert!(
            text.contains("box.acme.test") && text.contains("github.com"),
            "{text}"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&ka).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "readable by the user only");
    }
}

#[test]
fn the_dev_config_merges_entry_by_entry_and_each_machine_keeps_its_own() {
    let sandbox = Sandbox::new("dev-merge");
    let remote = sandbox.folder();
    let (a, a_home) = home_machine(&sandbox, "a");
    let (b, b_home) = home_machine(&sandbox, "b");
    let scopes = [Scope::Config, Scope::DevConfig];
    put(
        &a_home.join(".gitconfig"),
        "# a\n[user]\n\tname = Alex\n\temail = alex@acme.test\n[gpg]\n\tprogram = /opt/homebrew/bin/gpg\n[credential]\n\thelper = osxkeychain\n",
    );
    put(
        &b_home.join(".gitconfig"),
        "[user]\n\tname = Alex\n[pull]\n\trebase = true\n[gpg]\n\tprogram = /usr/bin/gpg\n",
    );
    put(
        &a_home.join(".claude.json"),
        r#"{"mcpServers": {"db": {"command": "npx", "args": ["db"]}, "local": {"command": "/opt/homebrew/bin/local-mcp"}}}"#,
    );
    put(
        &b_home.join(".claude.json"),
        r#"{"numStartups": 3, "mcpServers": {"web": {"command": "uvx", "args": ["web"]}}}"#,
    );
    put(
        &a_home.join(".claude/settings.json"),
        r#"{"model": "opus", "env": {"A": "1"}}"#,
    );
    put(
        &b_home.join(".claude/settings.json"),
        r#"{"theme": "dark", "env": {"B": "2"}, "statusLine": {"type": "command", "command": "/usr/local/bin/status"}}"#,
    );
    for m in [&a, &b] {
        m.engine
            .setup(remote.clone(), PASSWORD, &scopes, Some(name(&m.dir)))
            .unwrap();
    }
    a.engine.sync(true).unwrap();

    let (ga, gb) = (
        read(&a_home.join(".gitconfig")),
        read(&b_home.join(".gitconfig")),
    );
    for git in [&ga, &gb] {
        assert!(
            git.contains("email = alex@acme.test") && git.contains("rebase = true"),
            "{git}"
        );
    }
    assert!(
        ga.starts_with("# a\n")
            && ga.contains("/opt/homebrew/bin/gpg")
            && ga.contains("osxkeychain")
    );
    assert!(
        gb.contains("program = /usr/bin/gpg")
            && !gb.contains("homebrew")
            && !gb.contains("osxkeychain")
    );

    let servers = |home: &Path| -> Value {
        serde_json::from_str::<Value>(&read(&home.join(".claude.json"))).unwrap()["mcpServers"]
            .clone()
    };
    let (sa, sb) = (servers(&a_home), servers(&b_home));
    for s in [&sa, &sb] {
        assert_eq!(s["db"]["command"], "npx");
        assert_eq!(s["web"]["command"], "uvx");
    }
    assert_eq!(sa["local"]["command"], "/opt/homebrew/bin/local-mcp");
    assert!(sb.get("local").is_none(), "a tool of a only: {sb}");

    let settings = |home: &Path| -> Value {
        serde_json::from_str(&read(&home.join(".claude/settings.json"))).unwrap()
    };
    let (ca, cb) = (settings(&a_home), settings(&b_home));
    for s in [&ca, &cb] {
        assert_eq!(s["model"], "opus");
        assert_eq!(s["theme"], "dark");
        assert_eq!(s["env"], serde_json::json!({"A": "1", "B": "2"}));
    }
    assert!(ca.get("statusLine").is_none());
    assert_eq!(cb["statusLine"]["command"], "/usr/local/bin/status");

    // Steady: nothing more to send either way.
    assert!(b.push().pushed.is_empty() && a.push().pushed.is_empty());
}

#[test]
fn the_last_sync_can_be_undone_and_then_waits_for_a_choice() {
    let sandbox = Sandbox::new("undo");
    let remote = sandbox.folder();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put("termf.toml", "scan_depth = 3\n");
    a.put("library/commands/a.md", "from a\n");
    b.put("termf.toml", "keep_awake = \"always\"\n");
    a.setup(&remote);
    assert!(b.engine.undo().is_err(), "not set up yet");
    b.setup(&remote);
    assert!(b.engine.status().unwrap().last_backup.is_some());
    assert!(b.read("termf.toml").contains("scan_depth"));

    let undone = b.engine.undo().unwrap();
    assert_eq!(
        undone,
        Undone {
            restored: vec!["library/commands/a.md".into(), "termf.toml".into()],
            changed: Vec::new(),
        }
    );
    assert_eq!(b.read("termf.toml"), "keep_awake = \"always\"\n");
    assert!(!b.dir.join("library/commands/a.md").exists());
    // Put back, they are neither merged nor sent again: they wait, side by side.
    let status = b.engine.status().unwrap();
    assert_eq!(status.conflicts.len(), 2, "{:?}", status.conflicts);
    assert!(status.last_backup.is_none(), "one undo per sync");
    let report = b.push();
    assert!(
        report.pushed.is_empty() && report.pulled.is_empty(),
        "{report:?}"
    );
    a.push();
    assert_eq!(
        a.toml("termf.toml"),
        toml::from_str("scan_depth = 3\nkeep_awake = \"always\"").unwrap()
    );

    // Keeping this machine's version is then the user's choice; taking the other one too.
    b.engine.resolve("termf.toml", Resolution::Local).unwrap();
    b.engine
        .resolve("library/commands/a.md", Resolution::Remote)
        .unwrap();
    assert_eq!(b.read("library/commands/a.md"), "from a\n");
    a.push();
    assert_eq!(a.read("termf.toml"), "keep_awake = \"always\"\n");
    assert!(b.engine.status().unwrap().conflicts.is_empty());

    // A file changed here since the sync is left as it is.
    a.put("termf.toml", "keep_awake = \"never\"\n");
    a.push();
    b.push();
    b.put("termf.toml", "keep_awake = \"mine\"\n");
    let undone = b.engine.undo().unwrap();
    assert_eq!(undone.changed, ["termf.toml"]);
    assert_eq!(b.read("termf.toml"), "keep_awake = \"mine\"\n");

    // Only the last ten syncs are kept.
    for n in 0..12 {
        a.put(
            "termf.toml",
            &format!("keep_awake = \"mine\"\nscan_depth = {n}\n"),
        );
        a.push();
        b.push();
    }
    let backups = fs::read_dir(b.dir.join(".sync/backups")).unwrap().count();
    assert_eq!(backups, KEEP_BACKUPS);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(b.dir.join(".sync"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "the backups are the user's only");
    }
}
