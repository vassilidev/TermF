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
        use std::os::unix::fs::symlink;
        ssh.generate("real", KeyType::Ed25519, "", None).unwrap();
        symlink(&outside, ssh.dir.join("leak")).unwrap();
        symlink(ssh.dir.join("real"), ssh.dir.join("alias")).unwrap();
        assert!(ssh.resolve("leak").is_err(), "symlink out of ~/.ssh");
        assert!(ssh.resolve("alias").is_ok(), "symlink within ~/.ssh");
        let lone = ssh.generate("lone", KeyType::Ed25519, "", None).unwrap();
        fs::remove_file(with_pub(&lone)).unwrap();
        symlink(&outside, with_pub(&lone)).unwrap();
        let trashed = ssh.trash("lone").unwrap();
        assert!(trashed.join("lone").exists() && !trashed.join("lone.pub").exists());
        assert!(
            outside.exists(),
            "a sibling symlinked out of ~/.ssh is left alone"
        );
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
            "Host work\n  HostName github.com\n  IdentityFile \"~/.ssh/my key\"\n\n# defaults\n"
        )
    );
    assert_eq!(
        diff(CONFIG, &added),
        "     IdentityFile ~/.ssh/second\n     ProxyJump none\n \n+Host work\n+  HostName github.com\n+  IdentityFile \"~/.ssh/my key\"\n+\n # defaults\n Host *\n     AddKeysToAgent yes\n"
    );
    assert_eq!(
        upsert("", &entry).unwrap(),
        "Host work\n  HostName github.com\n  IdentityFile \"~/.ssh/my key\"\n"
    );
    assert_eq!(
        upsert("Host a\n  User b", &entry).unwrap(),
        "Host a\n  User b\n\nHost work\n  HostName github.com\n  IdentityFile \"~/.ssh/my key\"\n"
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
        preview.contains("+  IdentityFile ~/.ssh/id_work"),
        "{preview}"
    );
    assert!(!ssh.config().exists(), "a preview writes nothing");
    ssh.host_save(&entry).unwrap();
    assert_eq!(ssh.host_preview(&entry).unwrap(), "");
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
fn rotates_a_key_and_keeps_the_old_one() {
    let ssh = scratch("rotate");
    let old = ssh.generate("old", KeyType::Ed25519, "old", None).unwrap();
    fs::write(
        ssh.config(),
        "# hosts\nHost gh\n  IdentityFile ~/.ssh/old\nHost other\n  IdentityFile ~/.ssh/other\n",
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
    assert_eq!(rotation.hosts, ["gh"]);
    assert_eq!(rotation.repos, [repo.to_string_lossy()]);
    assert_eq!(rotation.upload_error, None);
    assert_eq!(
        fs::read_to_string(ssh.config()).unwrap(),
        "# hosts\nHost gh\n  IdentityFile ~/.ssh/new\nHost other\n  IdentityFile ~/.ssh/other\n"
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
