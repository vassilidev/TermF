//! S3 and WebDAV through a local fake server, SSH keys, forgetting a machine and cloning a missing repository.

use super::fake::{ACCESS, Fake, Kind, REGION, SECRET};
use super::*;
use crate::http;
use crate::objects::{Objects, Store};

fn credentials() -> Credentials {
    Credentials {
        user: ACCESS.into(),
        secret: SECRET.into(),
    }
}

/// Whether this system's curl signs S3 requests; the S3 tests say so and stop otherwise.
fn s3_curl() -> bool {
    let version = Command::new("curl")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| http::parse_version(&String::from_utf8_lossy(&o.stdout)));
    let ok = version.is_some_and(|v| http::check_s3_curl(v).is_ok());
    if !ok {
        eprintln!(
            "skipped: no curl {:?} or newer here ({version:?})",
            http::S3_CURL
        );
    }
    ok
}

fn any_curl() -> bool {
    let ok = Command::new("curl").arg("--version").output().is_ok();
    if !ok {
        eprintln!("skipped: no curl here");
    }
    ok
}

fn s3(fake: &Fake) -> RemoteSpec {
    RemoteSpec::S3 {
        endpoint: fake.url.clone(),
        bucket: "termf".into(),
        region: REGION.into(),
        prefix: "sync".into(),
        credentials: Some(credentials()),
    }
}

fn webdav(fake: &Fake) -> RemoteSpec {
    RemoteSpec::Webdav {
        url: format!("{}/dav/termf", fake.url),
        credentials: Some(Credentials {
            user: "alex".into(),
            secret: "dav password".into(),
        }),
    }
}

/// A home whose curl config says `--fail`, which would turn a lost race (412) into an error if TermF read it.
fn hostile_curlrc(sandbox: &Sandbox) -> Vec<(String, String)> {
    let home = sandbox.root.join("curl-home");
    fs::create_dir_all(&home).unwrap();
    for name in [".curlrc", "_curlrc"] {
        fs::write(home.join(name), "--fail\n").unwrap();
    }
    let home = home.to_string_lossy().into_owned();
    [
        "HOME",
        "CURL_HOME",
        "XDG_CONFIG_HOME",
        "USERPROFILE",
        "APPDATA",
    ]
    .iter()
    .map(|k| (k.to_string(), home.clone()))
    .collect()
}

fn converge(sandbox: &Sandbox, remote: &RemoteSpec, secret: &str) {
    let (mut a, mut b) = (sandbox.machine("a"), sandbox.machine("b"));
    for m in [&mut a, &mut b] {
        m.engine.git_env.extend(hostile_curlrc(sandbox));
    }
    a.put("termf.toml", "scan_depth = 3\n");
    a.put("library/skills/s/SKILL.md", "from a\n");
    a.setup(remote);
    b.put("layouts.toml", "# from b\n");
    let joined = b.setup(remote);
    assert!(joined.recovery_key.is_none(), "b joins the sync a created");
    a.push();
    assert_eq!(b.read("termf.toml"), "scan_depth = 3\n");
    assert_eq!(a.read("layouts.toml"), "# from b\n");

    // Both change the same file: merged key by key, through conditional writes.
    a.put("termf.toml", "scan_depth = 4\n");
    b.put("termf.toml", "scan_depth = 3\neditor = \"vim\"\n");
    a.push();
    b.push();
    a.push();
    assert_eq!(a.toml("termf.toml"), b.toml("termf.toml"));
    assert_eq!(a.toml("termf.toml")["scan_depth"].as_integer(), Some(4));

    // The credentials are in the keychain, never with the settings.
    for m in [&a, &b] {
        let state = fs::read(m.dir.join(".sync/state.json")).unwrap();
        assert!(!contains(&state, secret.as_bytes()));
        let fresh = Engine::new(m.dir.clone(), Box::new(NoKeychain));
        assert!(fresh.sync(true).is_err(), "no keychain, no credentials");
    }
    assert!(!a.engine.devices().unwrap().is_empty());
}

#[test]
fn s3_with_signed_requests() {
    if !s3_curl() {
        return;
    }
    let fake = Fake::start(Kind::S3);
    let sandbox = Sandbox::new("s3");
    converge(&sandbox, &s3(&fake), SECRET);
    let store = fake.store.lock().unwrap();
    assert_eq!(store.bad_signatures, 0);
    assert!(store.files.keys().all(|k| k.starts_with("termf/sync/")));
    assert!(store.files.len() > 4, "listed page after page");
}

#[test]
fn s3_refuses_a_wrong_secret() {
    if !s3_curl() {
        return;
    }
    let fake = Fake::start(Kind::S3);
    let sandbox = Sandbox::new("s3-wrong");
    let a = sandbox.machine("a");
    let RemoteSpec::S3 {
        endpoint,
        bucket,
        region,
        prefix,
        ..
    } = s3(&fake)
    else {
        unreachable!()
    };
    let wrong = RemoteSpec::S3 {
        endpoint,
        bucket,
        region,
        prefix,
        credentials: Some(Credentials {
            user: ACCESS.into(),
            secret: "not the secret".into(),
        }),
    };
    let error = format!("{:#}", a.engine.probe(&wrong).unwrap_err());
    assert!(
        error.contains("403") && error.contains("SignatureDoesNotMatch"),
        "{error}"
    );
    assert!(fake.store.lock().unwrap().files.is_empty());
}

#[test]
fn webdav_folder() {
    if !any_curl() {
        return;
    }
    let fake = Fake::start(Kind::Webdav);
    let sandbox = Sandbox::new("webdav");
    converge(&sandbox, &webdav(&fake), "dav password");
    let store = fake.store.lock().unwrap();
    assert!(store.dirs.contains("dav/termf/files/"), "{:?}", store.dirs);
}

#[test]
fn a_server_ignoring_conditions_is_refused() {
    if !any_curl() {
        return;
    }
    let fake = Fake::start(Kind::Webdav);
    fake.store.lock().unwrap().ignore_conditions = true;
    let sandbox = Sandbox::new("unconditional");
    let a = sandbox.machine("a");
    let error = a
        .engine
        .setup(webdav(&fake), PASSWORD, &Scope::DEFAULT, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("ignores conditional writes"), "{error}");
    assert!(!a.engine.enabled());
}

#[test]
fn a_server_ignoring_if_match_is_refused() {
    if !any_curl() {
        return;
    }
    let fake = Fake::start(Kind::Webdav);
    fake.store.lock().unwrap().ignore_if_match = true;
    let sandbox = Sandbox::new("no-if-match");
    let a = sandbox.machine("a");
    let error = a
        .engine
        .setup(webdav(&fake), PASSWORD, &Scope::DEFAULT, None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("(If-Match)"), "{error}");
    assert!(!a.engine.enabled());
}

/// The kinds of object store this system's curl can test.
fn kinds() -> Vec<Kind> {
    let mut kinds = Vec::new();
    if any_curl() {
        kinds.push(Kind::Webdav);
        if s3_curl() {
            kinds.push(Kind::S3);
        }
    }
    kinds
}

/// One machine's view of the fake server, as the engine has it, with a `.curlrc` that must not be read.
fn objects(kind: Kind, fake: &Fake, sandbox: &Sandbox, machine: &str) -> Objects {
    let work = sandbox.root.join(machine).join("http");
    let env = hostile_curlrc(sandbox);
    let store: Box<dyn Store> = match kind {
        Kind::S3 => Box::new(
            http::S3::new(&fake.url, "termf", REGION, "", &credentials(), work, env).unwrap(),
        ),
        Kind::Webdav => Box::new(
            http::Webdav::new(
                &format!("{}/dav", fake.url),
                Some(&credentials()),
                work,
                env,
            )
            .unwrap(),
        ),
    };
    Objects::new(
        store,
        sandbox.root.join(machine).join("objects"),
        "fake".into(),
    )
}

/// What the fake server holds under `name`.
fn on_server(fake: &Fake, name: &str) -> Option<String> {
    let store = fake.store.lock().unwrap();
    store
        .files
        .iter()
        .find(|(k, _)| k.ends_with(&format!("/{name}")))
        .map(|(_, (bytes, _))| String::from_utf8(bytes.clone()).unwrap())
}

/// Two machines that began from the same state: the second to publish is told to begin again, and nothing
/// the first wrote is overwritten, whether the object is new or changed.
#[test]
fn the_second_writer_never_overwrites_the_first() {
    for kind in kinds() {
        let fake = Fake::start(kind);
        let sandbox = Sandbox::new(if kind == Kind::S3 {
            "race-s3"
        } else {
            "race-dav"
        });
        let (mut one, mut two) = (
            objects(kind, &fake, &sandbox, "one"),
            objects(kind, &fake, &sandbox, "two"),
        );
        let blob = format!("files/{}", "a".repeat(32));
        let server = || on_server(&fake, &blob).unwrap();
        for round in ["created", "changed"] {
            one.begin().unwrap();
            two.begin().unwrap();
            fs::write(two.root().join(&blob), format!("two {round}")).unwrap();
            assert!(two.commit().unwrap());
            fs::write(one.root().join(&blob), format!("one {round}")).unwrap();
            assert!(!one.commit().unwrap(), "{round}: lost the race");
            assert_eq!(server(), format!("two {round}"));
            one.begin().unwrap();
            assert_eq!(
                fs::read_to_string(one.root().join(&blob)).unwrap(),
                format!("two {round}")
            );
        }
    }
}

/// A rotates the data key (a password change, a forgotten device) while B, which began before it, writes a
/// file: A never deletes B's version, and B, finding a new key file, begins again instead of counting it.
#[test]
fn a_key_rotation_never_loses_what_another_machine_wrote() {
    for kind in kinds() {
        let fake = Fake::start(kind);
        let sandbox = Sandbox::new(if kind == Kind::S3 {
            "rotate-s3"
        } else {
            "rotate-dav"
        });
        let (mut a, mut b) = (
            objects(kind, &fake, &sandbox, "a"),
            objects(kind, &fake, &sandbox, "b"),
        );
        let blob = |c: char| format!("files/{}", c.to_string().repeat(32));
        let (x, y, z) = (blob('a'), blob('b'), blob('c'));
        a.begin().unwrap();
        fs::write(a.root().join(KEYS), "key 1").unwrap();
        fs::write(a.root().join(&x), "x by a").unwrap();
        assert!(a.commit().unwrap());

        // B writes after A began its rotation but before A deletes the old generation.
        a.begin().unwrap();
        b.begin().unwrap();
        fs::write(b.root().join(&x), "x by b").unwrap();
        assert!(b.commit().unwrap());
        fs::remove_file(a.root().join(&x)).unwrap();
        fs::write(a.root().join(&y), "y, new key").unwrap();
        fs::write(a.root().join(KEYS), "key 2").unwrap();
        assert!(a.commit().unwrap());
        assert_eq!(on_server(&fake, &x).as_deref(), Some("x by b"), "{kind:?}");
        assert_eq!(on_server(&fake, KEYS).as_deref(), Some("key 2"));

        // B began before A's next rotation, and writes after it: told to begin again.
        b.begin().unwrap();
        a.begin().unwrap();
        fs::remove_file(a.root().join(&y)).unwrap();
        fs::write(a.root().join(&z), "z, newer key").unwrap();
        fs::write(a.root().join(KEYS), "key 3").unwrap();
        assert!(a.commit().unwrap());
        assert_eq!(
            on_server(&fake, &y),
            None,
            "unchanged since A's begin: deleted"
        );
        fs::write(b.root().join(&x), "x by b, again").unwrap();
        assert!(!b.commit().unwrap(), "{kind:?}: the key file changed");
        b.begin().unwrap();
        assert_eq!(fs::read_to_string(b.root().join(KEYS)).unwrap(), "key 3");
    }
}

fn ssh_machine(sandbox: &Sandbox, name: &str) -> (Machine, termf_ssh::Ssh) {
    let mut m = sandbox.machine(name);
    let home = sandbox.root.join(format!("{name}-home"));
    fs::create_dir_all(home.join(".ssh")).unwrap();
    let ssh = termf_ssh::Ssh::new(home.clone());
    m.engine.home = Some(home);
    (m, ssh)
}

#[test]
fn ssh_keys_travel_sealed_and_never_replace_a_key() {
    use termf_ssh::{HostEntry, KeyType};
    let sandbox = Sandbox::new("ssh");
    let remote = sandbox.folder();
    let (a, a_ssh) = ssh_machine(&sandbox, "a");
    let (b, b_ssh) = ssh_machine(&sandbox, "b");
    a_ssh
        .generate(
            "id_work",
            KeyType::Ed25519,
            "alex@work",
            Some("correct horse"),
        )
        .unwrap();
    a_ssh
        .generate("id_ed25519", KeyType::Ed25519, "alex@a", None)
        .unwrap();
    b_ssh
        .generate("id_ed25519", KeyType::Ed25519, "alex@b", None)
        .unwrap();
    a_ssh
        .host_save(&HostEntry {
            alias: "acme-prod".into(),
            host_name: Some("prod.acme.test".into()),
            user: Some("deploy".into()),
            identity_file: Some("id_work".into()),
            ..Default::default()
        })
        .unwrap();
    let b_own = fs::read(b_ssh.dir().join("id_ed25519")).unwrap();

    a.setup(&remote);
    let joined = b.setup(&remote);
    assert!(
        joined
            .report
            .pulled
            .contains(&"ssh/keys/id_work".to_owned())
    );
    assert_eq!(
        fs::read(b_ssh.dir().join("id_work")).unwrap(),
        fs::read(a_ssh.dir().join("id_work")).unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(b_ssh.dir().join("id_work"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let config = fs::read_to_string(b_ssh.dir().join("config")).unwrap();
    assert!(
        config.contains("Host acme-prod") && config.contains("IdentityFile ~/.ssh/id_work"),
        "{config}"
    );

    // Same name, different key: shown as a conflict with public keys only, and b keeps its own.
    let status = b.engine.status().unwrap();
    let conflict = status
        .conflicts
        .iter()
        .find(|c| c.path == "ssh/keys/id_ed25519")
        .expect("a conflict");
    assert!(conflict.local.as_deref().unwrap().contains("alex@b"));
    assert!(conflict.remote.as_deref().unwrap().contains("alex@a"));
    assert_eq!(fs::read(b_ssh.dir().join("id_ed25519")).unwrap(), b_own);

    // No private key in clear outside ~/.ssh: neither the merge base, the state nor the remote.
    for m in [&a, &b] {
        for private in [
            fs::read(a_ssh.dir().join("id_work")).unwrap(),
            fs::read(a_ssh.dir().join("id_ed25519")).unwrap(),
            b_own.clone(),
        ] {
            for file in all_files(&m.dir) {
                assert!(!contains(&file.1, &private), "a private key in {}", file.0);
            }
            if let RemoteSpec::Folder { path } = &remote {
                for file in all_files(Path::new(path)) {
                    assert!(!contains(&file.1, &private), "a private key in {}", file.0);
                }
            }
        }
    }

    // Taking the other machine's key puts this one in the trash first.
    b.engine
        .resolve("ssh/keys/id_ed25519", Resolution::Remote)
        .unwrap();
    assert_eq!(
        fs::read(b_ssh.dir().join("id_ed25519")).unwrap(),
        fs::read(a_ssh.dir().join("id_ed25519")).unwrap()
    );
    let trash = b_ssh.dir().join(termf_ssh::TRASH);
    let kept = fs::read_dir(&trash)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(fs::read(kept.join("id_ed25519")).unwrap(), b_own);
    assert!(b.engine.status().unwrap().conflicts.is_empty());

    // Keeping one's own: it stays here, and is not pushed over the others'.
    let (c, c_ssh) = ssh_machine(&sandbox, "c");
    c_ssh
        .generate("id_ed25519", KeyType::Ed25519, "alex@c", None)
        .unwrap();
    let c_own = fs::read(c_ssh.dir().join("id_ed25519")).unwrap();
    c.setup(&remote);
    c.engine
        .resolve("ssh/keys/id_ed25519", Resolution::Local)
        .unwrap();
    assert!(c.engine.status().unwrap().conflicts.is_empty());
    assert!(c.push().conflicts.is_empty());
    a.push();
    assert!(a.engine.status().unwrap().conflicts.is_empty());
    assert_eq!(fs::read(c_ssh.dir().join("id_ed25519")).unwrap(), c_own);
    assert_ne!(fs::read(a_ssh.dir().join("id_ed25519")).unwrap(), c_own);
    assert!(
        c.engine
            .resolve("ssh/keys/id_work", Resolution::Merged { text: "x".into() })
            .is_err()
    );

    // A host block TermF removed on a goes on b too.
    a_ssh.host_delete("acme-prod").unwrap();
    a.push();
    b.push();
    let config = fs::read_to_string(b_ssh.dir().join("config")).unwrap_or_default();
    assert!(!config.contains("acme-prod"), "{config}");
}

/// What cannot be read here is left out of the sync, never taken as deleted.
#[test]
fn unreadable_ssh_files_are_skipped_not_deleted_elsewhere() {
    use termf_ssh::KeyType;
    let sandbox = Sandbox::new("ssh-unreadable");
    let remote = sandbox.folder();
    let (a, a_ssh) = ssh_machine(&sandbox, "a");
    let (b, b_ssh) = ssh_machine(&sandbox, "b");
    a_ssh
        .generate("id_work", KeyType::Ed25519, "alex@work", None)
        .unwrap();
    let block = format!(
        "{}\nHost acme-prod\n  HostName prod.acme.test\n",
        termf_ssh::MARK
    );
    fs::write(a_ssh.dir().join("config"), &block).unwrap();
    a.setup(&remote);
    b.setup(&remote);
    let b_config = b_ssh.dir().join("config");
    assert!(fs::read_to_string(&b_config).unwrap().contains("acme-prod"));

    // Latin-1 in ~/.ssh/config: not UTF-8, it travels as it is.
    let latin1 = [block.as_bytes(), b"# caf\xe9\n"].concat();
    fs::write(a_ssh.dir().join("config"), &latin1).unwrap();
    a.push();
    b.push();
    assert_eq!(fs::read(&b_config).unwrap(), latin1);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let config = a_ssh.dir().join("config");
        fs::set_permissions(&config, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&config).is_err() {
            let report = a.push();
            assert!(
                report.skipped.iter().any(|s| s.path == "ssh/config"),
                "{:?}",
                report.skipped
            );
            b.push();
            assert_eq!(
                fs::read(&b_config).unwrap(),
                latin1,
                "never taken as deleted"
            );
        }
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();

        let key = a_ssh.dir().join("id_work");
        fs::set_permissions(&key, fs::Permissions::from_mode(0o000)).unwrap();
        let unreadable = fs::read(&key).is_err();
        let report = a.push();
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
        // Root reads anything: nothing to check then.
        if unreadable {
            assert!(
                report.skipped.iter().any(|s| s.path == "ssh/keys"),
                "{:?}",
                report.skipped
            );
            let (c, c_ssh) = ssh_machine(&sandbox, "c");
            c.setup(&remote);
            assert!(
                c_ssh.dir().join("id_work").exists(),
                "never offered as deleted"
            );
        }
    }
}

/// `~/.ssh/config` travels whole, blocks written by hand too, its paths into the home folder made this machine's.
#[test]
fn the_whole_ssh_config_travels() {
    let sandbox = Sandbox::new("ssh-config");
    let remote = sandbox.folder();
    let (a, a_ssh) = ssh_machine(&sandbox, "a");
    let (b, b_ssh) = ssh_machine(&sandbox, "b");
    let home = |m: &Machine| m.engine.home.clone().unwrap().display().to_string();
    let config = format!(
        "Include ~/.ssh/config.d/*\n\nHost *.acme.test\n  User deploy\n  IdentityFile {}/.ssh/work/id_ed25519\n\n\
         Host legacy\n  HostName old.acme.test\n  IdentityFile /opt/keys/legacy\n",
        home(&a)
    );
    fs::write(a_ssh.dir().join("config"), &config).unwrap();
    a.setup(&remote);
    b.setup(&remote);
    let b_config = b_ssh.dir().join("config");
    let on_b = fs::read_to_string(&b_config).unwrap();
    assert_eq!(on_b, config.replace(&home(&a), &home(&b)));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&b_config).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert!(b.push().pushed.is_empty(), "nothing sent back");

    fs::write(
        &b_config,
        format!("{on_b}\nHost box\n  HostName box.acme.test\n"),
    )
    .unwrap();
    b.push();
    a.push();
    let on_a = fs::read_to_string(a_ssh.dir().join("config")).unwrap();
    assert!(
        on_a.contains("Host box") && on_a.contains(&home(&a)),
        "{on_a}"
    );

    // Changed on both sides: side by side, and each keeps its own until the user chooses.
    fs::write(a_ssh.dir().join("config"), "Host a\n").unwrap();
    fs::write(&b_config, "Host b\n").unwrap();
    a.push();
    let report = b.push();
    assert!(
        report.conflicts.iter().any(|c| c.path == "ssh/config"),
        "{report:?}"
    );
    assert_eq!(fs::read_to_string(&b_config).unwrap(), "Host b\n");
    b.engine.resolve("ssh/config", Resolution::Remote).unwrap();
    assert_eq!(fs::read_to_string(&b_config).unwrap(), "Host a\n");
}

#[test]
fn a_forgotten_machine_without_a_keychain_says_so_at_unlock() {
    let sandbox = Sandbox::new("forget-no-keychain");
    let remote = sandbox.folder();
    let a = sandbox.machine("a");
    let d = sandbox.machine_with("d", Box::new(NoKeychain));
    a.setup(&remote);
    d.setup(&remote);
    let id = d.engine.status().unwrap().device_id.unwrap();
    a.engine.forget(&id, PASSWORD).unwrap();
    // TermF started again on d: no key in memory, the password is asked.
    let restarted = Engine::new(d.dir.clone(), Box::new(NoKeychain));
    assert!(!restarted.status().unwrap().unlocked);
    let error = restarted.unlock(PASSWORD).unwrap_err();
    assert!(error.downcast_ref::<Forgotten>().is_some(), "{error:#}");
    assert!(restarted.status().unwrap().forgotten);
}

#[test]
fn a_forgotten_machine_reads_nothing_new_and_says_why() {
    let sandbox = Sandbox::new("forget");
    let remote = sandbox.folder();
    let (a, b, c) = (
        sandbox.machine("a"),
        sandbox.machine("b"),
        sandbox.machine("c"),
    );
    a.put("termf.toml", "scan_depth = 3\n");
    let old_recovery = a.setup(&remote).recovery_key.unwrap();
    b.setup(&remote);
    c.setup(&remote);
    let id = |m: &Machine| m.engine.status().unwrap().device_id.unwrap();
    assert_eq!(a.engine.devices().unwrap().len(), 3);

    let itself = a.engine.forget(&id(&a), PASSWORD).unwrap_err().to_string();
    assert!(itself.contains("cannot forget itself"), "{itself}");
    assert!(a.engine.forget(&id(&b), "not the password").is_err());
    let recovery = a.engine.forget(&id(&b), PASSWORD).unwrap();
    assert_ne!(recovery, old_recovery);
    a.put("termf.toml", "scan_depth = 4\n");
    a.push();

    let error = b.engine.sync(true).unwrap_err();
    assert!(error.downcast_ref::<Forgotten>().is_some(), "{error:#}");
    let status = b.engine.status().unwrap();
    assert!(status.forgotten && status.last_error.is_some());
    assert!(
        b.engine
            .unlock(PASSWORD)
            .unwrap_err()
            .downcast_ref::<Forgotten>()
            .is_some(),
        "the password does not let it back in"
    );
    assert_eq!(b.read("termf.toml"), "scan_depth = 3\n");

    // The others ask for the password once.
    let locked = c.engine.sync(true).unwrap_err();
    assert!(locked.downcast_ref::<Locked>().is_some(), "{locked:#}");
    c.engine.unlock(PASSWORD).unwrap();
    c.push();
    assert_eq!(c.read("termf.toml"), "scan_depth = 4\n");
    let devices: Vec<String> = a
        .engine
        .devices()
        .unwrap()
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert!(
        !devices.contains(&id(&b)) && devices.len() == 2,
        "{devices:?}"
    );

    // Set up again, it is a new machine of the sync.
    b.engine.disable().unwrap();
    b.setup(&remote);
    let status = b.engine.status().unwrap();
    assert!(!status.forgotten && status.unlocked);
    // Its own termf.toml changed meanwhile, with no common version: the user chooses.
    assert_eq!(status.conflicts[0].path, "termf.toml");
    assert_eq!(a.engine.devices().unwrap().len(), 3);
}

#[test]
fn a_missing_repository_is_cloned_where_the_user_says() {
    let sandbox = Sandbox::new("clone");
    let remote = sandbox.folder();
    let code = sandbox.root.join("code");
    let origin = sandbox.root.join("origin");
    fs::create_dir_all(&origin).unwrap();
    sandbox.git(&origin, &["init", "-q", "-b", "main"]);
    fs::write(origin.join("README.md"), "acme api\n").unwrap();
    sandbox.git(&origin, &["add", "-A"]);
    sandbox.git(&origin, &["commit", "-q", "-m", "first"]);
    let web = code.join("web");
    fs::create_dir_all(&web).unwrap();
    sandbox.git(&web, &["init", "-q"]);
    let ssh_command = "ssh -i ~/.ssh/acme -o IdentitiesOnly=yes";
    sandbox.git(&web, &["config", "core.sshCommand", ssh_command]);
    let api = code.join("api");
    let path = |p: &Path| p.to_string_lossy().replace('\\', "/");
    let project = format!(
        "name = \"Acme\"\n\n[[repos]]\npath = \"{}\"\nremote = \"{}\"\n\n[[repos]]\npath = \"{}\"\nremote = \"git@example.test:acme/web.git\"\n\n[[repos]]\npath = \"{}\"\nremote = \"{}\"\n",
        path(&api),
        path(&origin),
        path(&web),
        path(&code.join("gone")),
        path(&sandbox.root.join("no-such-remote")),
    );

    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put("projects/acme.toml", &project);
    a.setup(&remote);
    b.setup(&remote);
    let missing = b.engine.status().unwrap().missing_repos;
    assert_eq!(missing.len(), 2, "{missing:?}");

    let target = sandbox.root.join("elsewhere").join("api");
    let mut lines = Vec::new();
    let status = b
        .engine
        .clone_repo("acme", &path(&api), &target, &mut |l| {
            lines.push(l.to_owned())
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(target.join("README.md")).unwrap(),
        "acme api\n"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("Cloning into")),
        "{lines:?}"
    );
    assert_eq!(status.missing_repos.len(), 1);
    assert_eq!(
        sandbox
            .git(&target, &["config", "--local", "--get", "core.sshCommand"])
            .trim(),
        ssh_command,
        "the key another repository of the project uses"
    );
    let paths: Vec<PathBuf> = b.toml("projects/acme.toml")["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| termf_core::expand_tilde(r["path"].as_str().unwrap()))
        .collect();
    assert!(paths.contains(&target), "{paths:?}");

    // The other machine keeps the synced path.
    b.push();
    a.push();
    assert!(a.read("projects/acme.toml").contains(&path(&api)));
    assert!(!a.read("projects/acme.toml").contains("elsewhere"));

    let error = b
        .engine
        .clone_repo(
            "acme",
            &path(&code.join("gone")),
            &sandbox.root.join("gone"),
            &mut |_| {},
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("git clone failed"), "{error}");
    assert!(!sandbox.root.join("gone").exists());
    assert!(
        b.engine
            .clone_repo("acme", &path(&api), &target, &mut |_| {})
            .is_err(),
        "no longer missing"
    );
}
