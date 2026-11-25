use std::process::Command;

use super::*;
use crate::config::{diff, upsert};

fn scratch(name: &str) -> Ssh {
    let home = std::env::temp_dir().join(format!("termf-ssh-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(home.join(".ssh")).unwrap();
    Ssh::new(home)
}

fn git_repo(path: &Path, remotes: &[(&str, &str)]) {
    fs::create_dir_all(path).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    };
    git(&["init", "-q"]);
    for (name, url) in remotes {
        git(&["remote", "add", name, url]);
    }
}

fn find<'a>(keys: &'a [Key], name: &str) -> &'a Key {
    keys.iter()
        .find(|k| k.name == name)
        .unwrap_or_else(|| panic!("no {name} in {keys:#?}"))
}

#[test]
fn generates_every_type_and_reads_it_back() {
    let ssh = scratch("types");
    ssh.generate("id_ed25519", KeyType::Ed25519, "me@laptop", None)
        .unwrap();
    ssh.generate("id_ecdsa", KeyType::Ecdsa, "ec", Some("correct horse"))
        .unwrap();
    ssh.generate("id_rsa", KeyType::Rsa, "", None).unwrap();
    let (keys, problems) = ssh.scan();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(keys.len(), 3);

    let ed = find(&keys, "id_ed25519");
    assert_eq!(
        (ed.algorithm.as_str(), ed.bits, ed.encrypted, ed.orphan),
        ("ssh-ed25519", Some(256), Some(false), false)
    );
    assert_eq!(ed.comment, "me@laptop");
    assert!(ed.fingerprint.as_ref().unwrap().starts_with("SHA256:"));
    let public = fs::read_to_string(ssh.dir.join("id_ed25519.pub")).unwrap();
    assert_eq!(ed.public_key.as_deref(), Some(public.trim()));

    let ec = find(&keys, "id_ecdsa");
    assert_eq!(
        (ec.algorithm.as_str(), ec.bits, ec.encrypted),
        ("ecdsa-sha2-nistp256", Some(256), Some(true))
    );
    assert_eq!(
        ec.comment, "ec",
        "an encrypted key's comment comes from its .pub"
    );
    let rsa = find(&keys, "id_rsa");
    assert_eq!((rsa.algorithm.as_str(), rsa.bits), ("ssh-rsa", Some(4096)));

    // OpenSSH's standard encrypted format: only the passphrase opens it.
    let encrypted = PrivateKey::read_openssh_file(&ssh.dir.join("id_ecdsa")).unwrap();
    assert!(encrypted.decrypt("wrong").is_err());
    assert!(encrypted.decrypt("correct horse").is_ok());

    #[cfg(unix)]
    {
        assert_eq!(ed.mode, Some(0o600));
        assert!(!ed.too_open);
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(ssh.dir.join("id_ed25519.pub"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o200, 0o200);
        assert_eq!(mode & 0o022, 0, "umask permitting, 0644");
    }
}

#[test]
fn never_overwrites_and_checks_names() {
    let ssh = scratch("overwrite");
    let key = ssh.generate("id", KeyType::Ed25519, "first", None).unwrap();
    let before = fs::read(&key).unwrap();
    assert!(
        ssh.generate("id", KeyType::Ed25519, "second", None)
            .is_err()
    );
    assert_eq!(fs::read(&key).unwrap(), before);

    fs::write(ssh.dir.join("taken.pub"), "mine").unwrap();
    assert!(ssh.generate("taken", KeyType::Ed25519, "", None).is_err());
    assert!(!ssh.dir.join("taken").exists());
    assert_eq!(
        fs::read_to_string(ssh.dir.join("taken.pub")).unwrap(),
        "mine"
    );

    for bad in [
        "",
        "../escape",
        "a/b",
        r"a\b",
        ".hidden",
        "config",
        "x.pub",
        "a b",
    ] {
        assert!(
            ssh.generate(bad, KeyType::Ed25519, "", None).is_err(),
            "{bad:?} accepted"
        );
    }
    assert!(!ssh.home.join("escape").exists());
    assert!(
        ssh.generate("multi", KeyType::Ed25519, "two\nlines", None)
            .is_err()
    );
}

#[test]
fn lists_orphans_legacy_keys_and_reports_bad_files() {
    let ssh = scratch("files");
    ssh.generate("full", KeyType::Ed25519, "full", None)
        .unwrap();
    let orphan = ssh
        .generate("orphan", KeyType::Ed25519, "lonely", None)
        .unwrap();
    fs::remove_file(orphan).unwrap();
    let write = |name: &str, text: &str| fs::write(ssh.dir.join(name), text).unwrap();
    write(
        "legacy",
        "-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,00\n\nAAAA\n-----END RSA PRIVATE KEY-----\n",
    );
    write(
        "broken",
        "-----BEGIN OPENSSH PRIVATE KEY-----\nnot a key\n-----END OPENSSH PRIVATE KEY-----\n",
    );
    write("broken2.pub", "ssh-ed25519 not-base64 x\n");
    write("notes.txt", "hello");
    write("config", "Host x\n");
    write("known_hosts", "github.com ssh-ed25519 AAAA\n");
    write(
        "id_cert-cert.pub",
        "ssh-ed25519-cert-v01@openssh.com AAAA\n",
    );

    let (keys, problems) = ssh.scan();
    let mut names: Vec<_> = keys.iter().map(|k| k.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["full", "legacy", "orphan.pub"]);
    let mut bad: Vec<_> = problems
        .iter()
        .map(|p| file_name(Path::new(&p.path)))
        .collect();
    bad.sort();
    assert_eq!(bad, ["broken", "broken2.pub"]);

    let orphan = find(&keys, "orphan.pub");
    assert!(orphan.orphan);
    assert_eq!(orphan.encrypted, None);
    assert_eq!(orphan.comment, "lonely");
    assert!(orphan.fingerprint.is_some());
    let legacy = find(&keys, "legacy");
    assert_eq!(
        (
            legacy.algorithm.as_str(),
            legacy.encrypted,
            legacy.fingerprint.as_deref()
        ),
        ("ssh-rsa", Some(true), None)
    );
    assert!(
        keys.iter().all(|k| !k
            .public_key
            .as_deref()
            .unwrap_or_default()
            .contains("PRIVATE")),
        "never private key material"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(ssh.dir.join("full"), fs::Permissions::from_mode(0o644)).unwrap();
        write("locked", "-----BEGIN OPENSSH PRIVATE KEY-----\n");
        fs::set_permissions(ssh.dir.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        let (keys, problems) = ssh.scan();
        assert!(find(&keys, "full").too_open);
        assert!(
            problems.iter().any(|p| p.path.ends_with("locked")),
            "unreadable files are reported, not fatal: {problems:?}"
        );
    }
}

#[test]
fn guards_paths_from_the_ui() {
    let ssh = scratch("guard");
    let key = ssh.generate("id", KeyType::Ed25519, "", None).unwrap();
    fs::write(
        ssh.home.join("secret"),
        "-----BEGIN OPENSSH PRIVATE KEY-----",
    )
    .unwrap();
    for ok in ["id", "./id", "id.pub", &key.to_string_lossy()] {
        assert_eq!(
            ssh.resolve(ok).unwrap().parent(),
            Some(ssh.dir.as_path()),
            "{ok}"
        );
    }
    let outside = ssh.home.join("secret");
    for bad in [
        "../secret",
        &outside.to_string_lossy(),
        "",
        ".",
        "missing",
        "/",
    ] {
        assert!(ssh.resolve(bad).is_err(), "{bad:?} accepted");
    }
    assert!(ssh.trash("../secret").is_err());
    assert!(outside.exists());
    let entry = HostEntry {
        alias: "x".into(),
        identity_file: Some("../secret".into()),
        ..Default::default()
    };
    assert!(ssh.host_preview(&entry).is_err());

    let trashed = ssh.trash("id").unwrap();
    let in_trash = trashed.join("id");
    assert!(
        ssh.resolve(&in_trash.to_string_lossy()).is_err(),
        "trash is off limits"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let real = ssh.generate("real", KeyType::Ed25519, "", None).unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o644)).unwrap();
        symlink(&outside, ssh.dir.join("leak")).unwrap();
        symlink(&real, ssh.dir.join("alias")).unwrap();
        assert!(ssh.resolve("leak").is_err(), "symlink out of ~/.ssh");
        assert_eq!(
            ssh.resolve("alias").unwrap(),
            ssh.dir.join("alias"),
            "a symlink within ~/.ssh is used as the entry it is"
        );
        let (keys, problems) = ssh.scan();
        assert!(keys.iter().all(|k| k.name != "leak"));
        assert!(
            problems
                .iter()
                .any(|p| p.path.ends_with("leak") && p.code == Some("outside")),
            "{problems:?}"
        );
        assert!(find(&keys, "real").too_open);
        assert!(
            !find(&keys, "alias").too_open,
            "fix permissions never touches a symlink, so it is not reported"
        );
        ssh.fix_permissions().unwrap();
        assert!(!find(&ssh.scan().0, "real").too_open);

        let lone = ssh.generate("lone", KeyType::Ed25519, "", None).unwrap();
        fs::remove_file(with_pub(&lone)).unwrap();
        symlink(&outside, with_pub(&lone)).unwrap();
        let trashed = ssh.trash("lone").unwrap();
        assert!(trashed.join("lone").exists() && !trashed.join("lone.pub").exists());
        assert!(
            outside.exists(),
            "a sibling symlinked out of ~/.ssh is left alone"
        );

        // A .pub that is a link to its own key: both entries go, once each.
        let twin = ssh.generate("twin", KeyType::Ed25519, "", None).unwrap();
        fs::remove_file(with_pub(&twin)).unwrap();
        symlink("twin", with_pub(&twin)).unwrap();
        let trashed = ssh.trash("twin.pub").unwrap();
        assert!(fs::symlink_metadata(trashed.join("twin.pub")).is_ok());
        assert!(trashed.join("twin").is_file() && !twin.exists());
    }

    let repo = ssh.home.join("repo");
    git_repo(&repo, &[]);
    fs::create_dir_all(ssh.home.join("other")).unwrap();
    let known = [repo.canonicalize().unwrap()];
    assert_eq!(known_repo(&repo.to_string_lossy(), &known).unwrap(), repo);
    for bad in [
        ssh.home.join("other"),
        repo.join("..").join("other"),
        ssh.home.join("missing"),
    ] {
        assert!(
            known_repo(&bad.to_string_lossy(), &known).is_err(),
            "{bad:?}"
        );
    }
    assert!(ssh.assign(&repo, Some("../secret")).is_err());
}

#[test]
fn trash_moves_and_never_overwrites() {
    let ssh = scratch("trash");
    let first = ssh
        .generate("old", KeyType::Ed25519, "first", None)
        .unwrap();
    let first_text = fs::read(&first).unwrap();
    let dir1 = ssh.trash("old.pub").unwrap();
    assert!(!first.exists() && !with_pub(&first).exists());
    assert_eq!(fs::read(dir1.join("old")).unwrap(), first_text);
    assert!(dir1.join("old.pub").exists());
    assert!(dir1.starts_with(ssh.dir.join(TRASH)));

    ssh.generate("old", KeyType::Ed25519, "second", None)
        .unwrap();
    let dir2 = ssh.trash("old").unwrap();
    assert_ne!(dir1, dir2);
    assert_eq!(fs::read(dir1.join("old")).unwrap(), first_text);
    assert!(dir2.join("old").exists());
    assert!(ssh.scan().0.is_empty(), "trashed keys are not listed");
    assert!(ssh.trash("old").is_err());
}

#[test]
fn parses_hosts_with_includes() {
    let ssh = scratch("hosts");
    fs::create_dir_all(ssh.dir.join("config.d")).unwrap();
    fs::write(
        ssh.dir.join("config"),
        "IdentityFile ~/.ssh/global\nInclude config.d/*\n\nHost gh-work\n  HostName github.com\n  User git\n  Port=2222\n  IdentityFile \"~/.ssh/work key\"\n  IdentitiesOnly yes\n\nMatch host x\n  IdentityFile ~/.ssh/ignored\n\nHost *\n  User me\n",
    )
    .unwrap();
    fs::write(
        ssh.dir.join("config.d/extra"),
        "Host extra other\n  IdentityFile %d/.ssh/extra\n",
    )
    .unwrap();
    let hosts = config::hosts(&ssh.dir, &ssh.home);
    let patterns: Vec<_> = hosts.iter().map(|h| h.patterns.join(" ")).collect();
    assert_eq!(patterns, ["extra other", "*", "gh-work", "*"]);
    let work = &hosts[2];
    assert_eq!(work.host_name.as_deref(), Some("github.com"));
    assert_eq!(work.port.as_deref(), Some("2222"));
    assert_eq!(work.identities_only, Some(true));
    assert_eq!(
        work.identity_files,
        [ssh.dir.join("work key").to_string_lossy()]
    );
    assert_eq!(work.line, 4);
    assert_eq!(
        hosts[0].identity_files,
        [ssh.dir.join("extra").to_string_lossy()]
    );
    assert!(hosts[0].file.ends_with("extra"));
    assert_eq!(
        hosts[1].identity_files,
        [ssh.dir.join("global").to_string_lossy()]
    );
    assert!(
        hosts
            .iter()
            .all(|h| !h.identity_files.iter().any(|f| f.ends_with("ignored"))),
        "Match blocks are skipped"
    );
}

const CONFIG: &str = "\
# my config
Include config.d/*

Host github.com
    HostName github.com
    user git
    # keep me
    IdentityFile ~/.ssh/old
    IdentityFile ~/.ssh/second
    ProxyJump none

# defaults
Host *
    AddKeysToAgent yes
";

#[test]
fn updates_a_host_and_keeps_the_rest() {
    let entry = HostEntry {
        alias: "GitHub.com".into(),
        host_name: Some("github.com".into()),
        user: Some("git".into()),
        port: None,
        identity_file: Some("~/.ssh/new".into()),
        identities_only: Some(true),
    };
    let updated = upsert(CONFIG, &entry).unwrap();
    assert_eq!(
        updated,
        CONFIG
            .replace(
                "    IdentityFile ~/.ssh/old\n    IdentityFile ~/.ssh/second\n",
                "    IdentityFile ~/.ssh/new\n"
            )
            .replace(
                "ProxyJump none\n",
                "ProxyJump none\n    IdentitiesOnly yes\n"
            )
    );
    assert_eq!(upsert(&updated, &entry).unwrap(), updated, "idempotent");
    assert_eq!(diff(&updated, &updated), "");

    let without_user = HostEntry {
        user: None,
        ..entry.clone()
    };
    assert!(
        !upsert(&updated, &without_user)
            .unwrap()
            .contains("user git")
    );

    let crlf = CONFIG.replace('\n', "\r\n");
    let updated = upsert(&crlf, &entry).unwrap();
    assert!(updated.contains("    IdentityFile ~/.ssh/new\r\n") && !updated.contains("\r\r"));
    assert_eq!(
        updated.matches('\n').count(),
        updated.matches("\r\n").count()
    );
}

#[test]
fn adds_a_host_before_the_catch_all() {
    let entry = HostEntry {
        alias: "work".into(),
        host_name: Some("github.com".into()),
        identity_file: Some("~/.ssh/my key".into()),
        ..Default::default()
    };
    let added = upsert(CONFIG, &entry).unwrap();
    assert_eq!(
        added,
        CONFIG.replace(
            "# defaults\n",
            "# Written by TermF\nHost work\n  HostName github.com\n  IdentityFile \"~/.ssh/my key\"\n\n# defaults\n"
        )
    );
    assert_eq!(
        diff(CONFIG, &added),
        "     IdentityFile ~/.ssh/second\n     ProxyJump none\n \n+# Written by TermF\n+Host work\n+  HostName github.com\n+  IdentityFile \"~/.ssh/my key\"\n+\n # defaults\n Host *\n     AddKeysToAgent yes\n"
    );
    assert_eq!(
        upsert("", &entry).unwrap(),
        "# Written by TermF\nHost work\n  HostName github.com\n  IdentityFile \"~/.ssh/my key\"\n"
    );
    assert_eq!(
        upsert("Host a\n  User b", &entry).unwrap(),
        "Host a\n  User b\n\n# Written by TermF\nHost work\n  HostName github.com\n  IdentityFile \"~/.ssh/my key\"\n"
    );
    for alias in ["", "a b", "*", "-oProxyCommand=x", "a\nHost b"] {
        let bad = HostEntry {
            alias: alias.into(),
            ..Default::default()
        };
        assert!(upsert("", &bad).is_err(), "{alias:?}");
    }
    let injected = HostEntry {
        host_name: Some("x\n  ProxyCommand evil".into()),
        ..entry
    };
    assert!(upsert("", &injected).is_err());
}

#[test]
fn quotes_values_so_openssh_reads_them_back() {
    let ssh = scratch("quotes");
    let entry = HostEntry {
        alias: "box".into(),
        user: Some("o'neil".into()),
        identity_file: Some("~/.ssh/my key".into()),
        ..Default::default()
    };
    let text = upsert("", &entry).unwrap();
    assert_eq!(
        text,
        "# Written by TermF\nHost box\n  User \"o'neil\"\n  IdentityFile \"~/.ssh/my key\"\n"
    );
    fs::write(ssh.config(), &text).unwrap();
    let hosts = config::hosts(&ssh.dir, &ssh.home);
    assert_eq!(hosts[0].user.as_deref(), Some("o'neil"));
    assert_eq!(
        hosts[0].identity_files,
        [ssh.dir.join("my key").to_string_lossy()]
    );
    // The system's ssh agrees (-G only prints the resolved options; -F skips every other config).
    let out = Command::new("ssh")
        .arg("-G")
        .arg("-F")
        .arg(ssh.config())
        .arg("box")
        .output()
        .unwrap();
    let resolved = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(resolved.lines().any(|l| l == "user o'neil"), "{resolved}");
    assert!(
        resolved
            .lines()
            .any(|l| l.starts_with("identityfile ") && l.ends_with("my key")),
        "{resolved}"
    );

    for value in ["say \"hi\"", r"DOMAIN\me"] {
        let bad = HostEntry {
            user: Some(value.into()),
            ..entry.clone()
        };
        let error = upsert("", &bad).unwrap_err().to_string();
        assert!(error.contains("edit the file by hand"), "{error}");
    }
}

#[test]
fn never_splits_a_shared_block_and_warns_about_earlier_ones() {
    let ssh = scratch("shared");
    ssh.generate("web", KeyType::Ed25519, "", None).unwrap();
    let entry = HostEntry {
        alias: "web".into(),
        user: Some("deploy".into()),
        identity_file: Some("web".into()),
        ..Default::default()
    };
    let shared = "Host web db\n  User root\n";
    let error = upsert(shared, &entry).unwrap_err().to_string();
    assert!(
        error.contains("line 1") && error.contains("Host web db"),
        "{error}"
    );
    assert!(upsert("Host !web other\n  User x\n", &entry).is_ok());

    fs::create_dir_all(ssh.dir.join("config.d")).unwrap();
    fs::write(
        ssh.dir.join("config.d/team"),
        "Host web\n  HostName web.example.com\n",
    )
    .unwrap();
    let config = "IdentityFile ~/.ssh/everywhere\nInclude config.d/*\n\nHost w*\n  User admin\n\nHost *\n  User fallback\n";
    fs::write(ssh.config(), config).unwrap();
    let preview = ssh.host_preview(&entry).unwrap();
    assert!(preview.diff.contains("+Host web"), "{}", preview.diff);
    let warnings = preview.warnings.join("\n");
    assert_eq!(preview.warnings.len(), 3, "{warnings}");
    assert!(
        warnings.contains("also declared in") && warnings.contains("team:1"),
        "{warnings}"
    );
    assert!(warnings.contains("gets User from `Host w*`"), "{warnings}");
    assert!(
        warnings.contains("before any Host") && warnings.contains("IdentityFile"),
        "{warnings}"
    );
    assert!(
        !warnings.contains("fallback") && !warnings.contains("`Host *`"),
        "Host * comes after the new block"
    );
}

#[test]
fn deletes_a_host_and_only_its_block() {
    assert_eq!(
        config::remove(CONFIG, "GitHub.com").unwrap(),
        "# my config\nInclude config.d/*\n\n# defaults\nHost *\n    AddKeysToAgent yes\n",
        "its options and comments go, the next block's comment stays"
    );
    assert!(
        config::remove(CONFIG, "gitlab.com").is_err(),
        "no such host"
    );
    assert!(
        config::remove("Host a b\n    User me\n", "a").is_err(),
        "a block shared with other hosts stays"
    );
    let marked = format!(
        "{}\nHost work\n    User git\n\nHost other\n    User me\n",
        config::MARK
    );
    assert_eq!(
        config::remove(&marked, "work").unwrap(),
        "Host other\n    User me\n",
        "TermF's mark goes with its block"
    );

    let ssh = scratch("delete");
    fs::write(ssh.config(), CONFIG).unwrap();
    let preview = ssh.host_delete_preview("github.com").unwrap();
    assert!(
        preview
            .diff
            .lines()
            .any(|l| l.starts_with('-') && l.contains("IdentityFile ~/.ssh/old")),
        "{preview:?}"
    );
    assert_eq!(
        fs::read_to_string(ssh.config()).unwrap(),
        CONFIG,
        "a preview writes nothing"
    );
    ssh.host_delete("github.com").unwrap();
    let left = fs::read_to_string(ssh.config()).unwrap();
    assert!(
        !left.contains("Host github.com") && left.contains("Host *"),
        "{left}"
    );
}

#[test]
fn saves_hosts_atomically() {
    let ssh = scratch("save");
    ssh.generate("id_work", KeyType::Ed25519, "", None).unwrap();
    let entry = HostEntry {
        alias: "work".into(),
        host_name: Some("github.com".into()),
        identity_file: Some(ssh.dir.join("id_work").to_string_lossy().into_owned()),
        identities_only: Some(true),
        ..Default::default()
    };
    let preview = ssh.host_preview(&entry).unwrap();
    assert!(
        preview.diff.contains("+  IdentityFile ~/.ssh/id_work"),
        "{preview:?}"
    );
    assert!(preview.warnings.is_empty());
    assert!(!ssh.config().exists(), "a preview writes nothing");
    ssh.host_save(&entry).unwrap();
    assert_eq!(ssh.host_preview(&entry).unwrap().diff, "");
    let hosts = config::hosts(&ssh.dir, &ssh.home);
    assert_eq!(
        hosts[0].identity_files,
        [ssh.dir.join("id_work").to_string_lossy()]
    );
    assert!(!ssh.dir.join(".config.termf-tmp").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let mode = fs::metadata(ssh.config()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        // A config symlinked from a dotfiles folder is written through.
        let dotfiles = ssh.home.join("dotfiles");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::rename(ssh.config(), dotfiles.join("ssh_config")).unwrap();
        symlink(dotfiles.join("ssh_config"), ssh.config()).unwrap();
        let other = HostEntry {
            alias: "other".into(),
            user: Some("me".into()),
            ..Default::default()
        };
        ssh.host_save(&other).unwrap();
        assert!(
            fs::symlink_metadata(ssh.config())
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let text = fs::read_to_string(dotfiles.join("ssh_config")).unwrap();
        assert!(
            text.contains("Host work") && text.contains("Host other"),
            "{text}"
        );
    }
}

#[test]
fn maps_keys_to_hosts_and_repositories() {
    let ssh = scratch("mapping");
    for name in ["work", "perso", "assigned", "unused"] {
        ssh.generate(name, KeyType::Ed25519, name, None).unwrap();
    }
    fs::write(
        ssh.config(),
        "Host gh-work\n  HostName github.com\n  IdentityFile ~/.ssh/work\n\nHost github.com\n  IdentityFile ~/.ssh/perso.pub\n",
    )
    .unwrap();
    let src = ssh.home.join("src");
    let (a, b, c, d) = (src.join("a"), src.join("b"), src.join("c"), src.join("d"));
    git_repo(&a, &[("origin", "git@gh-work:org/a.git")]);
    git_repo(
        &b,
        &[
            ("origin", "https://github.com/x/b.git"),
            ("upstream", "ssh://git@github.com/x/b.git"),
        ],
    );
    git_repo(&c, &[("origin", "git@github.com:x/c.git")]);
    git_repo(&d, &[("origin", "https://example.com/d.git")]);
    ssh.assign(&c, Some("assigned.pub")).unwrap();

    let agent_key = find(&ssh.scan().0, "work").fingerprint.clone().unwrap();
    let agent = Agent {
        error: None,
        keys: vec![AgentKey {
            bits: Some(256),
            fingerprint: agent_key,
            comment: "work".into(),
            kind: "ED25519".into(),
        }],
    };
    let inventory = ssh.collect(&[a.clone(), b.clone(), c.clone(), d.clone()], agent);
    let keys = &inventory.keys;
    let text = |p: &PathBuf| p.to_string_lossy().into_owned();
    let work = find(keys, "work");
    assert_eq!(
        (work.hosts.as_slice(), work.repos.as_slice()),
        (&["gh-work".to_owned()][..], &[text(&a)][..])
    );
    assert!(work.in_agent);
    let perso = find(keys, "perso");
    assert_eq!(
        (perso.hosts.as_slice(), perso.repos.as_slice()),
        (&["github.com".to_owned()][..], &[text(&b)][..])
    );
    assert!(!perso.in_agent);
    let assigned = find(keys, "assigned");
    assert_eq!(assigned.repos, [text(&c)]);
    assert!(assigned.hosts.is_empty());
    let unused = find(keys, "unused");
    assert!(unused.hosts.is_empty() && unused.repos.is_empty());

    let repo_paths: Vec<_> = inventory.repos.iter().map(|r| r.path.clone()).collect();
    assert_eq!(
        repo_paths,
        [text(&a), text(&b), text(&c)],
        "https-only repositories are left out"
    );
    let b_remotes: Vec<_> = inventory.repos[1]
        .remotes
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(b_remotes, ["upstream"]);
    let command = inventory.repos[2].ssh_command.as_deref().unwrap();
    assert!(command.starts_with("ssh -i ") && command.ends_with(" -o IdentitiesOnly=yes"));

    ssh.assign(&c, None).unwrap();
    ssh.assign(&c, None).unwrap();
    assert_eq!(tools::local_ssh_command(&c), None);
}

#[test]
fn lists_a_repository_key_once_in_order() {
    let ssh = scratch("repo-keys-once");
    fs::write(
        ssh.config(),
        "Host gh-a\n  IdentityFile ~/.ssh/work\n  IdentityFile ~/.ssh/perso\n\nHost gh-b\n  IdentityFile ~/.ssh/work\n",
    )
    .unwrap();
    let repo = ssh.home.join("src").join("r");
    git_repo(
        &repo,
        &[
            ("origin", "git@gh-a:x/r.git"),
            ("upstream", "git@gh-b:x/r.git"),
        ],
    );
    let inventory = ssh.collect(&[repo], Agent::default());
    let key = |name: &str| ssh.dir.join(name).to_string_lossy().into_owned();
    assert_eq!(
        inventory.repos[0].identity_files,
        [key("work"), key("perso")]
    );
}

#[test]
fn rotates_a_key_and_keeps_the_old_one() {
    let ssh = scratch("rotate");
    let old = ssh.generate("old", KeyType::Ed25519, "old", None).unwrap();
    fs::write(
        ssh.config(),
        "# hosts\nHost gh\n  IdentityFile ~/.ssh/old\nHost other\n  IdentityFile ~/.ssh/other\nInclude config.d/*\n",
    )
    .unwrap();
    fs::create_dir_all(ssh.dir.join("config.d")).unwrap();
    fs::write(
        ssh.dir.join("config.d/work"),
        "Host gitlab\n  IdentityFile ~/.ssh/old.pub\n",
    )
    .unwrap();
    let (repo, untouched) = (ssh.home.join("repo"), ssh.home.join("untouched"));
    git_repo(&repo, &[("origin", "git@gh:x/y.git")]);
    git_repo(&untouched, &[]);
    ssh.assign(&repo, Some("old")).unwrap();

    let rotation = ssh
        .rotate(
            "old",
            "new",
            KeyType::Ed25519,
            "new",
            None,
            false,
            &[repo.clone(), untouched.clone()],
        )
        .unwrap();
    assert_eq!(rotation.hosts, ["gh", "gitlab"]);
    assert_eq!(rotation.repos, [repo.to_string_lossy()]);
    assert!(rotation.errors.is_empty(), "{:?}", rotation.errors);
    assert!(!rotation.uploaded);
    assert!(
        rotation
            .warning
            .as_deref()
            .unwrap()
            .contains("no server knows the new key"),
        "repointing without an upload says so"
    );
    assert_eq!(
        fs::read_to_string(ssh.config()).unwrap(),
        "# hosts\nHost gh\n  IdentityFile ~/.ssh/new\nHost other\n  IdentityFile ~/.ssh/other\nInclude config.d/*\n"
    );
    assert_eq!(
        fs::read_to_string(ssh.dir.join("config.d/work")).unwrap(),
        "Host gitlab\n  IdentityFile ~/.ssh/new\n",
        "included files are rewritten too"
    );
    let command = tools::local_ssh_command(&repo).unwrap();
    assert!(
        command.contains(&*ssh.dir.join("new").to_string_lossy()),
        "{command}"
    );
    assert_eq!(tools::local_ssh_command(&untouched), None);
    assert!(
        old.exists() && with_pub(&old).exists(),
        "the old key stays until trashed"
    );

    // A step that fails is reported, and what was done is still listed.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let set = |p: &Path, m| fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
        fs::write(
            ssh.dir.join("config.d/work"),
            "Host gitlab\n  IdentityFile ~/.ssh/new\n",
        )
        .unwrap();
        set(&ssh.dir.join("config.d"), 0o500);
        let rotation = ssh
            .rotate(
                "new",
                "newer",
                KeyType::Ed25519,
                "",
                None,
                false,
                std::slice::from_ref(&repo),
            )
            .unwrap();
        set(&ssh.dir.join("config.d"), 0o700);
        assert_eq!(rotation.hosts, ["gh"]);
        assert_eq!(rotation.repos, [repo.to_string_lossy()]);
        assert_eq!(rotation.errors.len(), 1, "{:?}", rotation.errors);
        assert!(rotation.errors[0].contains("work"), "{:?}", rotation.errors);
        assert!(rotation.key.ends_with("newer") && Path::new(&rotation.key).exists());
    }
}

#[cfg(unix)]
#[test]
fn fixes_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let ssh = scratch("perms");
    let key = ssh.generate("id", KeyType::Ed25519, "", None).unwrap();
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let set = |p: &Path, m| fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
    set(&key, 0o644);
    set(&with_pub(&key), 0o666);
    set(&ssh.dir, 0o755);
    assert_eq!(ssh.fix_permissions().unwrap().len(), 3);
    assert_eq!(
        (mode(&key), mode(&with_pub(&key)), mode(&ssh.dir)),
        (0o600, 0o644, 0o700)
    );
    assert!(ssh.fix_permissions().unwrap().is_empty());
}

#[test]
fn pairs_go_to_another_machine_private_and_never_over_a_file() {
    let (from, to) = (scratch("pairs-from"), scratch("pairs-to"));
    from.generate("id_work", KeyType::Ed25519, "work", Some("correct horse"))
        .unwrap();
    from.generate("solo", KeyType::Ed25519, "", None).unwrap();
    fs::remove_file(from.dir.join("solo.pub")).unwrap();
    fs::write(from.dir.join("notes.txt"), "not a key").unwrap();
    let pairs = from.pairs().unwrap();
    let names: Vec<&str> = pairs.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["id_work", "solo"]);
    assert!(pairs[1].public.is_none());

    fs::remove_dir_all(&to.dir).unwrap();
    for pair in &pairs {
        to.restore(pair).unwrap();
    }
    assert_eq!(
        fs::read(to.dir.join("id_work")).unwrap(),
        fs::read(from.dir.join("id_work")).unwrap()
    );
    assert!(!to.dir.join("solo.pub").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&to.dir.join("id_work")), 0o600);
        assert_eq!(mode(&to.dir.join("id_work.pub")) & 0o022, 0);
        assert_eq!(mode(&to.dir) & 0o077, 0);
    }
    let error = to.restore(&pairs[0]).unwrap_err().to_string();
    assert!(error.contains("already exists"), "{error}");
    for bad in ["../x", ".hidden", "config", "a/b", "k.pub", ""] {
        assert!(!valid_name(bad), "{bad}");
    }
}

#[test]
fn only_the_blocks_termf_added_are_synced() {
    let ssh = scratch("marked");
    ssh.host_save(&HostEntry {
        alias: "gh-work".into(),
        host_name: Some("github.com".into()),
        user: Some("git".into()),
        ..Default::default()
    })
    .unwrap();
    let by_hand = "Host mine\n  User me\n\n";
    let text = fs::read_to_string(ssh.config()).unwrap();
    fs::write(ssh.config(), format!("{by_hand}{text}")).unwrap();
    let marked = ssh.marked_hosts().unwrap();
    assert_eq!(marked.len(), 1);
    assert_eq!(
        (marked[0].alias.as_str(), marked[0].user.as_deref()),
        ("gh-work", Some("git"))
    );
    assert!(ssh.hand_written("mine").unwrap() && !ssh.hand_written("gh-work").unwrap());

    // Another machine's blocks: one updated, one added with a key path out of ~/.ssh (dropped), and one
    // for an alias written by hand here (left alone).
    let incoming = [
        HostEntry {
            alias: "gh-work".into(),
            host_name: Some("github.com".into()),
            user: Some("git".into()),
            port: Some(443),
            ..Default::default()
        },
        HostEntry {
            alias: "mine".into(),
            user: Some("other".into()),
            ..Default::default()
        },
        HostEntry {
            alias: "box".into(),
            user: Some("root".into()),
            identity_file: Some("/etc/passwd".into()),
            ..Default::default()
        },
    ];
    ssh.apply_hosts(&incoming).unwrap();
    let text = fs::read_to_string(ssh.config()).unwrap();
    assert!(text.starts_with(by_hand), "{text}");
    assert!(
        text.contains("  Port 443") && !text.contains("/etc/passwd"),
        "{text}"
    );
    assert_eq!(ssh.marked_hosts().unwrap().len(), 2);

    // Gone on the other machine: removed here, mark included.
    ssh.apply_hosts(&incoming[1..2]).unwrap();
    assert_eq!(
        fs::read_to_string(ssh.config()).unwrap(),
        "Host mine\n  User me\n"
    );

    assert!(synced_identity("~/.ssh/work/id_ed25519") && synced_identity("~/.ssh/id"));
    for bad in [
        "/etc/passwd",
        "~/.ssh/../x",
        "~/.ssh/",
        "~/.ssh/a//b",
        "~/.ssh/a\\b",
    ] {
        assert!(!synced_identity(bad), "{bad}");
    }
    // Unreadable is an error, never "no blocks": the sync would delete them elsewhere.
    fs::write(ssh.config(), b"# Written by TermF\nHost caf\xe9\n").unwrap();
    assert!(ssh.marked_hosts().is_err());
    assert!(ssh.hand_written("x").is_err() && ssh.apply_hosts(&[]).is_err());
}

#[test]
fn lists_an_ecdsa_key_whose_scalar_lost_its_leading_zero() {
    use crate::keys::tests::{
        OPENSSH_SHORT_SCALAR, OPENSSH_SHORT_SCALAR_ENCRYPTED, OPENSSH_SHORT_SCALAR_PUBLIC,
    };
    let ssh = scratch("short-scalar");
    for (name, text) in [
        ("id_short", OPENSSH_SHORT_SCALAR),
        ("id_short_enc", OPENSSH_SHORT_SCALAR_ENCRYPTED),
    ] {
        fs::write(ssh.dir.join(name), text).unwrap();
        fs::write(
            ssh.dir.join(format!("{name}.pub")),
            format!("{OPENSSH_SHORT_SCALAR_PUBLIC}\n"),
        )
        .unwrap();
    }
    let (keys, problems) = ssh.scan();
    assert!(problems.is_empty(), "{problems:?}");
    for (name, encrypted) in [("id_short", false), ("id_short_enc", true)] {
        let key = find(&keys, name);
        assert_eq!(
            (key.algorithm.as_str(), key.encrypted),
            ("ecdsa-sha2-nistp521", Some(encrypted))
        );
        assert_eq!(key.public_key.as_deref(), Some(OPENSSH_SHORT_SCALAR_PUBLIC));
    }
    let _ = fs::remove_dir_all(&ssh.home);
}
