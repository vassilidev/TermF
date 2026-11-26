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

/// Review of #214, 1: what only makes sense on a machine is never lost, neither to a deletion from elsewhere nor
/// to an undo.
#[test]
fn a_deletion_elsewhere_and_an_undo_keep_this_machines_own_values() {
    let sandbox = Sandbox::new("own-values");
    let remote = sandbox.folder();
    let (a, a_home) = home_machine(&sandbox, "a");
    let (b, b_home) = home_machine(&sandbox, "b");
    let git = "[user]\n\tname = Alex\n[credential]\n\thelper = osxkeychain\n[gpg]\n\tprogram = /opt/homebrew/bin/gpg\n";
    let ssh = "Host *\n  UseKeychain yes\n  ProxyCommand /usr/bin/nc %h %p\n  User alex\n";
    let mcp =
        r#"{"mcpServers": {"local": {"command": "/opt/acme/bin/mcp"}, "web": {"command": "npx"}}}"#;
    put(&a_home.join(".gitconfig"), git);
    put(&a_home.join(".ssh/config"), ssh);
    put(&a_home.join(".claude.json"), mcp);
    a.setup(&remote);
    b.setup(&remote);
    assert!(read(&b_home.join(".gitconfig")).contains("name = Alex"));

    // b deletes the three: a keeps them, each a conflict that shows what a would lose.
    fs::remove_file(b_home.join(".gitconfig")).unwrap();
    fs::remove_file(b_home.join(".ssh/config")).unwrap();
    put(&b_home.join(".claude.json"), "{}");
    b.push();
    let report = a.push();
    for path in ["dev/gitconfig", "ssh/config", "dev/claude.json"] {
        let conflict = report
            .conflicts
            .iter()
            .find(|c| c.path == path)
            .unwrap_or_else(|| panic!("{path}: {report:?}"));
        assert_eq!(conflict.remote, None);
    }
    let shown = report
        .conflicts
        .iter()
        .find(|c| c.path == "dev/gitconfig")
        .unwrap();
    assert!(shown.local.as_deref().unwrap().contains("osxkeychain"));
    assert_eq!(read(&a_home.join(".gitconfig")), git);
    assert_eq!(read(&a_home.join(".ssh/config")), ssh);

    // Taking the deletions, then undoing them: each file back as it was, byte for byte.
    for path in ["dev/gitconfig", "ssh/config", "dev/claude.json"] {
        a.engine.resolve(path, Resolution::Remote).unwrap();
    }
    assert!(!a_home.join(".gitconfig").exists() && !a_home.join(".ssh/config").exists());
    for _ in 0..3 {
        assert_eq!(a.engine.undo().unwrap().changed, Vec::<String>::new());
    }
    assert_eq!(read(&a_home.join(".gitconfig")), git);
    assert_eq!(read(&a_home.join(".ssh/config")), ssh);
    let servers: Value = serde_json::from_str(&read(&a_home.join(".claude.json"))).unwrap();
    assert_eq!(
        servers["mcpServers"]["local"]["command"],
        "/opt/acme/bin/mcp"
    );
    assert_eq!(servers["mcpServers"]["web"]["command"], "npx");
}

/// Review of #214, 2: an option given more than once is known by its value, never by its rank.
#[test]
fn an_option_given_twice_is_never_lost_nor_doubled() {
    let sandbox = Sandbox::new("twice");
    let remote = sandbox.folder();
    let (a, a_home) = home_machine(&sandbox, "a");
    let (b, b_home) = home_machine(&sandbox, "b");
    let git = "[safe]\n\tdirectory = /opt/src\n\tdirectory = ~/b\n\tdirectory = ~/c\n[tool]\n\tplugin = /opt/acme/a.so\n\tplugin = ~/b.so\n";
    put(&a_home.join(".gitconfig"), git);
    a.setup(&remote);
    b.setup(&remote);
    assert_eq!(
        read(&b_home.join(".gitconfig")),
        "[safe]\n\tdirectory = ~/b\n\tdirectory = ~/c\n[tool]\n\tplugin = ~/b.so\n"
    );
    assert!(a.push().pushed.is_empty() && b.push().pushed.is_empty());
    assert_eq!(read(&a_home.join(".gitconfig")), git);

    put(
        &b_home.join(".gitconfig"),
        "[safe]\n\tdirectory = ~/b\n\tdirectory = ~/c\n\tdirectory = ~/d\n[tool]\n\tplugin = ~/b.so\n\tplugin = ~/e.so\n",
    );
    b.push();
    a.push();
    let on_a = read(&a_home.join(".gitconfig"));
    for line in [
        "directory = /opt/src",
        "directory = ~/b",
        "directory = ~/c",
        "directory = ~/d",
        "plugin = /opt/acme/a.so",
        "plugin = ~/b.so",
        "plugin = ~/e.so",
    ] {
        assert_eq!(
            on_a.matches(&format!("\t{line}\n")).count(),
            1,
            "{line}: {on_a}"
        );
    }
    assert!(a.push().pushed.is_empty() && b.push().pushed.is_empty());
}

/// Review of #214, 3: ssh takes the first value it finds, so another machine's hosts never come before this
/// machine's; and an order changed on one machine alone is kept.
#[test]
fn hosts_from_elsewhere_never_take_over_and_an_order_change_travels() {
    let sandbox = Sandbox::new("host-order");
    let remote = sandbox.folder();
    let (a, a_home) = home_machine(&sandbox, "a");
    let (b, b_home) = home_machine(&sandbox, "b");
    let (ca, cb) = (a_home.join(".ssh/config"), b_home.join(".ssh/config"));
    put(
        &ca,
        "Host *.acme.test\n  User deploy\n\nHost *\n  ServerAliveInterval 60\n",
    );
    put(
        &cb,
        "Host box.acme.test\n  User alex\n\nHost *\n  ServerAliveInterval 60\n",
    );
    a.setup(&remote);
    b.setup(&remote);
    let on_b = read(&cb);
    let at = |text: &str, host: &str| text.find(&format!("Host {host}\n")).unwrap();
    assert!(
        at(&on_b, "box.acme.test") < at(&on_b, "*.acme.test")
            && at(&on_b, "*.acme.test") < at(&on_b, "*"),
        "{on_b}"
    );

    // a puts its wildcard first again, b adds a port meanwhile: a's order, b's port.
    a.push();
    let on_a = read(&ca);
    let wildcard = "Host *.acme.test\n  User deploy\n\n";
    put(&ca, &format!("{wildcard}{}", on_a.replace(wildcard, "")));
    a.push();
    put(
        &cb,
        &on_b.replace("  User alex\n", "  User alex\n  Port 2222\n"),
    );
    b.push();
    let on_b = read(&cb);
    assert!(
        at(&on_b, "*.acme.test") < at(&on_b, "box.acme.test") && on_b.contains("Port 2222"),
        "{on_b}"
    );
}

/// Review of #214, 4: the same host keys in another order are the same host keys: nothing to send, ever.
#[test]
fn known_hosts_in_another_order_are_not_sent_again_and_again() {
    let sandbox = Sandbox::new("known-hosts-order");
    let remote = sandbox.folder();
    let (a, a_home) = home_machine(&sandbox, "a");
    let (b, b_home) = home_machine(&sandbox, "b");
    let (gh, gl) = (
        "github.com ssh-ed25519 AAAAgh\n",
        "gitlab.com ssh-ed25519 AAAAgl\n",
    );
    put(&a_home.join(".ssh/known_hosts"), &format!("{gh}{gl}"));
    put(&b_home.join(".ssh/known_hosts"), &format!("{gl}{gh}"));
    a.setup(&remote);
    b.setup(&remote);
    for _ in 0..3 {
        for m in [&a, &b] {
            let report = m.push();
            assert!(
                !report.pushed.contains(&"ssh/known_hosts".into())
                    && !report.pulled.contains(&"ssh/known_hosts".into()),
                "{report:?}"
            );
        }
    }
    assert_eq!(read(&b_home.join(".ssh/known_hosts")), format!("{gl}{gh}"));
}

/// Review of #214, 5: a comment written after a sync is a change: undoing that sync leaves the file alone.
#[test]
fn undo_leaves_a_file_commented_since() {
    let sandbox = Sandbox::new("undo-comment");
    let remote = sandbox.folder();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put("termf.toml", "scan_depth = 3\n");
    b.put("termf.toml", "keep_awake = \"always\"\n");
    a.setup(&remote);
    b.setup(&remote);
    let commented = format!("# mine\n{}", b.read("termf.toml"));
    b.put("termf.toml", &commented);
    let undone = b.engine.undo().unwrap();
    assert_eq!(undone.changed, ["termf.toml"]);
    assert_eq!(b.read("termf.toml"), commented);
}

/// Review of #214, 6: a comment written here stays when the other machine changes the setting below it.
#[test]
fn a_comment_stays_when_the_other_machine_changes_the_host() {
    let sandbox = Sandbox::new("host-comment");
    let remote = sandbox.folder();
    let (a, a_home) = home_machine(&sandbox, "a");
    let (b, b_home) = home_machine(&sandbox, "b");
    let (ca, cb) = (a_home.join(".ssh/config"), b_home.join(".ssh/config"));
    put(&ca, "Host prod\n  HostName a.acme.test\n  User deploy\n");
    a.setup(&remote);
    b.setup(&remote);
    put(&cb, &read(&cb).replace("a.acme.test", "b.acme.test"));
    b.push();
    put(&ca, &format!("# careful: production\n{}", read(&ca)));
    a.push();
    b.push();
    let expected = "# careful: production\nHost prod\n  HostName b.acme.test\n  User deploy\n";
    assert_eq!(read(&ca), expected);
    assert_eq!(read(&cb), expected);
}
