use std::collections::HashMap;
use std::process::Command;

use super::*;

/// Never the system keychain.
#[derive(Default)]
struct MemoryStore(Mutex<HashMap<String, Vec<u8>>>);

impl KeyStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }
    fn set(&self, account: &str, secret: &[u8]) -> Result<()> {
        self.0.lock().unwrap().insert(account.into(), secret.into());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

/// A machine without a keychain (headless Linux).
struct NoKeychain;

impl KeyStore for NoKeychain {
    fn get(&self, _: &str) -> Result<Option<Vec<u8>>> {
        bail!("no keychain")
    }
    fn set(&self, _: &str, _: &[u8]) -> Result<()> {
        bail!("no keychain")
    }
    fn delete(&self, _: &str) -> Result<()> {
        bail!("no keychain")
    }
}

const PASSWORD: &str = "correct horse battery";

struct Sandbox {
    root: PathBuf,
    env: Vec<(String, String)>,
}

impl Sandbox {
    /// A throwaway folder, and a git that reads none of the user's config.
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("termf-sync-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let gitconfig = root.join("gitconfig");
        fs::write(&gitconfig, "").unwrap();
        let env = vec![
            (
                "GIT_CONFIG_GLOBAL".into(),
                gitconfig.to_string_lossy().into(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_AUTHOR_NAME".into(), "t".into()),
            ("GIT_AUTHOR_EMAIL".into(), "t@t".into()),
            ("GIT_COMMITTER_NAME".into(), "t".into()),
            ("GIT_COMMITTER_EMAIL".into(), "t@t".into()),
        ];
        Self { root, env }
    }

    fn git(&self, cwd: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A local bare repository: nothing ever leaves the machine.
    fn bare(&self) -> RemoteSpec {
        let dir = self.root.join("remote.git");
        fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "--bare", "-q"]);
        RemoteSpec::Git {
            url: dir.to_string_lossy().into(),
        }
    }

    fn folder(&self) -> RemoteSpec {
        let dir = self.root.join("shared");
        fs::create_dir_all(&dir).unwrap();
        RemoteSpec::Folder {
            path: dir.to_string_lossy().into(),
        }
    }

    fn machine(&self, name: &str) -> Machine {
        self.machine_with(name, Box::<MemoryStore>::default())
    }

    fn machine_with(&self, name: &str, store: Box<dyn KeyStore>) -> Machine {
        let dir = self.root.join(name);
        fs::create_dir_all(&dir).unwrap();
        let mut engine = Engine::new(dir.clone(), store);
        engine.git_env = self.env.clone();
        Machine { dir, engine }
    }
}

struct Machine {
    dir: PathBuf,
    engine: Engine,
}

impl Machine {
    fn put(&self, path: &str, text: &str) {
        let file = self.dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.dir.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn toml(&self, path: &str) -> toml::Table {
        toml::from_str(&self.read(path)).unwrap()
    }

    fn setup(&self, remote: &RemoteSpec) -> Setup {
        let setup = self
            .engine
            .setup(
                remote.clone(),
                PASSWORD,
                &Scope::AVAILABLE,
                Some(name(&self.dir)),
            )
            .unwrap();
        assert_eq!(setup.error, None);
        setup
    }

    fn push(&self) -> Report {
        self.engine.sync(true).unwrap()
    }
}

fn name(dir: &Path) -> &str {
    dir.file_name().unwrap().to_str().unwrap()
}

/// Every file under `dir`, with its content.
fn all_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push((
                    path.to_string_lossy().into_owned(),
                    fs::read(&path).unwrap(),
                ));
            }
        }
    }
    found
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn two_machines_converge_through_git() {
    let sandbox = Sandbox::new("converge");
    let remote = sandbox.bare();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put("termf.toml", "scan_depth = 3\n");
    a.put("layouts.toml", "[[layouts]]\nname = \"m\"\n");
    a.put("library/skills/s/SKILL.md", "skill\n");
    a.put("library/skills/s/scripts/run.sh", "#!/bin/sh\n");
    a.put("library/commands/old.md", "old\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script = a.dir.join("library/skills/s/scripts/run.sh");
        fs::set_permissions(script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    b.put("library/commands/c.md", "from b\n");

    let created = a.setup(&remote);
    assert!(created.recovery_key.is_some());
    assert_eq!(created.report.pushed.len(), 5, "{:?}", created.report);
    let joined = b.setup(&remote);
    assert!(
        joined.recovery_key.is_none(),
        "joining shows no recovery key"
    );
    assert_eq!(joined.report.pushed, ["library/commands/c.md"]);
    a.push();

    for m in [&a, &b] {
        assert_eq!(m.read("termf.toml"), "scan_depth = 3\n");
        assert_eq!(m.read("library/skills/s/SKILL.md"), "skill\n");
        assert_eq!(m.read("library/commands/c.md"), "from b\n");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(b.dir.join("library/skills/s/scripts/run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "the execute bit travels");
    }

    fs::remove_file(a.dir.join("library/commands/old.md")).unwrap();
    fs::remove_dir_all(a.dir.join("library/skills/s")).unwrap();
    assert_eq!(a.push().pushed.len(), 3);
    let report = b.push();
    assert_eq!(report.pulled.len(), 3, "{report:?}");
    assert!(!b.dir.join("library/commands/old.md").exists());
    assert!(
        !b.dir.join("library/skills/s").exists(),
        "a deleted skill leaves no empty folder"
    );
    assert!(b.dir.join("library/commands/c.md").exists());

    let again = a.push();
    assert!(
        again.pulled.is_empty() && again.pushed.is_empty(),
        "{again:?}"
    );
    let names: Vec<_> = a
        .engine
        .devices()
        .unwrap()
        .into_iter()
        .map(|d| (d.name, d.current))
        .collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&("a".into(), true)) && names.contains(&("b".into(), false)));

    b.engine.disable().unwrap();
    assert!(!b.dir.join(".sync").exists(), "the clone goes too");
    assert!(b.dir.join("library/commands/c.md").exists());
}

#[test]
fn merges_different_keys_and_reports_the_same_key() {
    let sandbox = Sandbox::new("merge");
    let remote = sandbox.bare();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put(
        "termf.toml",
        "# mine\nscan_depth = 5\neditor = \"vim\"\n[agents]\nclaude = \"claude\"\n",
    );
    a.setup(&remote);
    b.setup(&remote);
    assert_eq!(a.read("termf.toml"), b.read("termf.toml"));

    a.put(
        "termf.toml",
        "# mine\nscan_depth = 3\neditor = \"vim\"\n[agents]\nclaude = \"claude\"\n",
    );
    b.put("termf.toml", "# mine\nscan_depth = 5\neditor = \"vim\"\n[agents]\nclaude = \"claude\"\ncodex = \"codex\"\n");
    a.push();
    let report = b.push();
    assert!(report.conflicts.is_empty(), "{report:?}");
    a.push();
    for m in [&a, &b] {
        let settings = m.toml("termf.toml");
        assert_eq!(settings["scan_depth"].as_integer(), Some(3));
        assert_eq!(settings["agents"]["codex"].as_str(), Some("codex"));
        assert!(m.read("termf.toml").starts_with("# mine\n"));
    }

    a.put("termf.toml", "scan_depth = 7\neditor = \"vim\"\n");
    b.put("termf.toml", "scan_depth = 9\neditor = \"vim\"\n");
    a.push();
    let report = b.push();
    assert_eq!(report.conflicts.len(), 1, "{report:?}");
    let conflict = &report.conflicts[0];
    assert_eq!(conflict.path, "termf.toml");
    assert_eq!(conflict.keys, ["scan_depth"]);
    assert!(
        conflict
            .local
            .as_deref()
            .unwrap()
            .contains("scan_depth = 9")
    );
    assert!(
        conflict
            .remote
            .as_deref()
            .unwrap()
            .contains("scan_depth = 7")
    );
    assert_eq!(b.engine.status().unwrap().conflicts, report.conflicts);
    assert!(
        b.read("termf.toml").contains("scan_depth = 9"),
        "nothing lost here"
    );
    a.push();
    assert!(a.read("termf.toml").contains("scan_depth = 7"), "nor there");
    assert!(b.push().conflicts.len() == 1, "reported until resolved");

    let resolved = b.engine.resolve("termf.toml", Resolution::Local).unwrap();
    assert!(resolved.conflicts.is_empty() && resolved.pushed == ["termf.toml"]);
    a.push();
    assert!(a.read("termf.toml").contains("scan_depth = 9"));

    b.put("library/commands/c.md", "b\n");
    b.push();
    a.push();
    a.put("library/commands/c.md", "a\n");
    b.put("library/commands/c.md", "b2\n");
    a.push();
    let report = b.push();
    assert_eq!(
        report.conflicts[0].keys,
        Vec::<String>::new(),
        "a text file conflicts whole"
    );
    let merged = Resolution::Merged {
        text: "a\nb2\n".into(),
    };
    b.engine.resolve("library/commands/c.md", merged).unwrap();
    a.push();
    assert_eq!(a.read("library/commands/c.md"), "a\nb2\n");
}

#[test]
fn a_version_written_without_ours_is_merged_not_taken() {
    // A sync service can let a stale machine overwrite a newer blob: the clocks tell.
    let sandbox = Sandbox::new("overwrite");
    let remote = sandbox.folder();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put("termf.toml", "scan_depth = 5\neditor = \"vim\"\n");
    a.setup(&remote);
    b.setup(&remote);
    let shared = sandbox.root.join("shared");
    let before: Vec<_> = all_files(&shared);

    a.put("termf.toml", "scan_depth = 3\neditor = \"vim\"\n");
    a.push();
    for (path, bytes) in &before {
        fs::write(path, bytes).unwrap();
    }
    b.put("termf.toml", "scan_depth = 5\neditor = \"zed\"\n");
    b.push();

    let report = a.push();
    assert_eq!(report.conflicts.len(), 1, "{report:?}");
    assert!(
        a.read("termf.toml").contains("scan_depth = 3"),
        "a's change is kept"
    );
}

#[test]
fn passwords_recovery_and_revocation() {
    let sandbox = Sandbox::new("passwords");
    let remote = sandbox.bare();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put("termf.toml", "scan_depth = 3\n");
    let recovery = a.setup(&remote).recovery_key.unwrap();

    let wrong = b
        .engine
        .setup(remote.clone(), "not the password", &Scope::AVAILABLE, None)
        .unwrap_err();
    assert!(wrong.to_string().contains("wrong password"), "{wrong:#}");
    assert!(!b.engine.enabled() && !b.dir.join("termf.toml").exists());
    b.setup(&remote);
    assert!(b.engine.unlock("not the password").is_err());
    assert!(b.engine.status().unwrap().unlocked);
    assert!(
        a.engine
            .change_password("not the password", "a new password")
            .is_err()
    );
    assert!(a.engine.change_password(PASSWORD, "short").is_err());

    // Changing the password re-encrypts everything: b's key no longer opens anything.
    let new_recovery = a
        .engine
        .change_password(PASSWORD, "a new password")
        .unwrap();
    assert_ne!(new_recovery, recovery);
    a.put("termf.toml", "scan_depth = 4\n");
    a.push();
    let locked = b.engine.sync(true).unwrap_err();
    assert!(locked.downcast_ref::<Locked>().is_some(), "{locked:#}");
    let status = b.engine.status().unwrap();
    assert!(!status.unlocked && status.last_error.is_some());
    assert!(
        b.engine.unlock(PASSWORD).is_err(),
        "the old password is useless"
    );
    assert_eq!(b.read("termf.toml"), "scan_depth = 3\n");
    b.engine.unlock("a new password").unwrap();
    b.push();
    assert_eq!(b.read("termf.toml"), "scan_depth = 4\n");

    // The old recovery key died with the old data key; the new one sets a new password.
    assert!(b.engine.recover(&recovery, "third password").is_err());
    let typed = new_recovery.to_lowercase().replace('-', " ");
    let third = b.engine.recover(&typed, "third password").unwrap();
    assert_ne!(third, new_recovery);
    assert!(
        a.engine
            .sync(true)
            .unwrap_err()
            .downcast_ref::<Locked>()
            .is_some()
    );
    a.engine.unlock("third password").unwrap();
    a.push();

    let c = sandbox.machine("c");
    assert!(
        c.engine
            .setup(remote.clone(), "a new password", &Scope::AVAILABLE, None)
            .is_err()
    );
    c.engine
        .setup(remote.clone(), "third password", &Scope::AVAILABLE, None)
        .unwrap();
    assert_eq!(c.read("termf.toml"), "scan_depth = 4\n");
}

#[test]
fn the_remote_sees_no_content_and_no_name() {
    let sandbox = Sandbox::new("opaque");
    let markers = [
        "termf.toml",
        "layouts",
        "library",
        "SKILL.md",
        "acme",
        "conventions",
        "topsecret-content",
        "laptop-of-alice",
        PASSWORD,
    ];
    for remote in [sandbox.folder(), sandbox.bare()] {
        let m = sandbox.machine(match remote {
            RemoteSpec::Git { .. } => "git-machine",
            RemoteSpec::Folder { .. } => "folder-machine",
        });
        m.put("termf.toml", "editor = \"topsecret-content\"\n");
        m.put("layouts.toml", "# topsecret-content\n");
        m.put("library/skills/acme-skill/SKILL.md", "topsecret-content\n");
        m.put("projects/acme.toml", "name = \"topsecret-content\"\n");
        m.put(
            "projects/acme/conventions/style/source.md",
            "topsecret-content\n",
        );
        let recovery = m
            .engine
            .setup(
                remote.clone(),
                PASSWORD,
                &Scope::AVAILABLE,
                Some("laptop-of-alice"),
            )
            .unwrap()
            .recovery_key
            .unwrap();
        assert_eq!(
            m.engine.status().unwrap().device_name.as_deref(),
            Some("laptop-of-alice")
        );

        let (files, log) = match &remote {
            RemoteSpec::Folder { path } => (all_files(Path::new(path)), String::new()),
            RemoteSpec::Git { url } => {
                let clone = sandbox.root.join("inspect");
                sandbox.git(
                    &sandbox.root,
                    &["clone", "-q", "-b", "main", url, &clone.to_string_lossy()],
                );
                let log = sandbox.git(&clone, &["log", "--all", "--format=%an %ae %s %b"]);
                let mut files = all_files(&clone);
                files.retain(|(p, _)| !p.contains(".git"));
                (files, log)
            }
        };
        assert!(files.len() >= 6, "{files:?}");
        for (path, bytes) in &files {
            for marker in markers.iter().chain([&recovery.as_str()]) {
                let shown = path.strip_prefix(&*sandbox.root.to_string_lossy()).unwrap();
                assert!(!shown.contains(marker), "{marker} in the name {shown}");
                assert!(!contains(bytes, marker.as_bytes()), "{marker} in {shown}");
                assert!(!log.contains(marker), "{marker} in the history");
            }
        }
    }
}

#[test]
fn repository_paths_stay_on_their_machine() {
    let sandbox = Sandbox::new("paths");
    let remote = sandbox.bare();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    let (api_a, api_b) = (
        sandbox.root.join("a-src/api"),
        sandbox.root.join("b-src/api"),
    );
    fs::create_dir_all(&api_a).unwrap();
    fs::create_dir_all(&api_b).unwrap();
    let path = |p: &Path| termf_core::portable(&p.to_string_lossy());
    a.put(
        "projects/acme.toml",
        &format!(
            "name = \"Acme\"\n[[repos]]\npath = {:?}\nremote = \"git@h:acme/api.git\"\n[[repos]]\npath = \"~/termf-test-missing/web\"\nremote = \"git@h:acme/web.git\"\n",
            path(&api_a)
        ),
    );
    a.setup(&remote);
    let report = b.setup(&remote).report;
    assert_eq!(
        report.missing_repos,
        [MissingRepo {
            project: "acme".into(),
            path: "~/termf-test-missing/web".into(),
            remote: Some("git@h:acme/web.git".into()),
        }]
    );

    // b keeps its clone elsewhere: its own path stays here, the synced one does not change.
    let mine = b.read("projects/acme.toml").replace(
        &format!("{:?}", path(&api_a)),
        &format!("{:?}", path(&api_b)),
    );
    b.put("projects/acme.toml", &mine);
    assert!(
        b.push().pushed.is_empty(),
        "a path of this machine is not a change"
    );
    let local = b.read("local.toml");
    assert!(
        local.contains("git@h:acme/api.git") && local.contains(&path(&api_b)),
        "{local}"
    );

    a.put(
        "projects/acme.toml",
        &a.read("projects/acme.toml").replace("Acme", "Acme Inc"),
    );
    a.push();
    b.push();
    let project = b.toml("projects/acme.toml");
    assert_eq!(project["name"].as_str(), Some("Acme Inc"));
    assert_eq!(
        project["repos"][0]["path"].as_str(),
        Some(path(&api_b).as_str())
    );
    assert!(
        a.read("projects/acme.toml")
            .contains(&format!("{:?}", path(&api_a)))
    );
    assert!(!a.dir.join("local.toml").exists());

    let clone = sandbox.root.join("inspect");
    let RemoteSpec::Git { url } = &remote else {
        unreachable!()
    };
    sandbox.git(
        &sandbox.root,
        &["clone", "-q", "-b", "main", url, &clone.to_string_lossy()],
    );
    let keys = read_keys(&clone).unwrap().unwrap();
    let kek = keys.derive(&keys.password, PASSWORD).unwrap();
    let data = keys
        .data_keys(&keys.unwrap(&keys.password, &kek).unwrap())
        .unwrap();
    let synced = read_files(&clone, &data).unwrap();
    let synced =
        String::from_utf8(synced["projects/acme.toml"].entry.clone().unwrap().bytes).unwrap();
    assert!(synced.contains(&format!("{:?}", path(&api_a))), "{synced}");
    assert!(!synced.contains("b-src"), "{synced}");
    assert!(
        read_files(&clone, &data)
            .unwrap()
            .keys()
            .all(|p| p != "local.toml")
    );
}

#[test]
fn a_disabled_scope_is_not_synced() {
    let sandbox = Sandbox::new("scopes");
    let remote = sandbox.folder();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put("termf.toml", "scan_depth = 3\n");
    a.put("projects/a-only.toml", "name = \"A\"\n");
    a.put("projects/a-only/conventions/c/source.md", "A\n");
    a.engine
        .setup(remote.clone(), PASSWORD, &[Scope::Config], None)
        .unwrap();
    b.put("projects/b-only.toml", "name = \"B\"\n");
    b.setup(&remote);
    a.push();
    assert_eq!(b.read("termf.toml"), "scan_depth = 3\n");
    assert!(!b.dir.join("projects/a-only.toml").exists());
    assert!(!b.dir.join("projects/a-only").exists());
    assert!(!a.dir.join("projects/b-only.toml").exists());

    b.engine
        .set_scopes(&[Scope::Config, Scope::Conventions])
        .unwrap();
    a.engine
        .set_scopes(&[Scope::Config, Scope::Conventions])
        .unwrap();
    a.push();
    b.push();
    assert_eq!(b.read("projects/a-only/conventions/c/source.md"), "A\n");
    assert!(!b.dir.join("projects/a-only.toml").exists());
    assert!(
        a.engine.set_scopes(&[Scope::Vault]).is_err(),
        "not available yet"
    );
    assert!(a.engine.set_scopes(&[]).is_err());
}

#[test]
fn without_a_keychain_the_key_stays_in_memory() {
    let sandbox = Sandbox::new("no-keychain");
    let remote = sandbox.folder();
    let a = sandbox.machine_with("a", Box::new(NoKeychain));
    a.put("termf.toml", "scan_depth = 3\n");
    a.setup(&remote);
    let status = a.engine.status().unwrap();
    assert!(status.unlocked && !status.remembered);
    let kek = a.engine.memory.lock().unwrap().clone().unwrap().1;
    for (path, bytes) in all_files(&sandbox.root) {
        assert!(!contains(&bytes, kek.as_ref()), "the key is in {path}");
        assert!(
            !contains(&bytes, crypto::hex(kek.as_ref()).as_bytes()),
            "{path}"
        );
    }

    // TermF restarted: the password is asked again.
    let restarted = sandbox.machine_with("a", Box::new(NoKeychain));
    assert!(!restarted.engine.status().unwrap().unlocked);
    let error = restarted.engine.sync(true).unwrap_err();
    assert!(error.downcast_ref::<Locked>().is_some(), "{error:#}");
    restarted.engine.unlock(PASSWORD).unwrap();
    restarted.push();

    restarted.engine.disable().unwrap();
    assert!(!restarted.engine.enabled() && !restarted.dir.join(".sync").exists());
    assert_eq!(restarted.read("termf.toml"), "scan_depth = 3\n");
}

#[test]
fn setup_refuses_a_folder_holding_other_files() {
    let sandbox = Sandbox::new("foreign");
    let remote = sandbox.folder();
    fs::write(sandbox.root.join("shared/notes.txt"), "mine").unwrap();
    fs::write(sandbox.root.join("shared/README.md"), "fine").unwrap();
    let a = sandbox.machine("a");
    let error = a.engine.probe(&remote).and_then(|_| {
        a.engine
            .setup(remote.clone(), PASSWORD, &Scope::AVAILABLE, None)
            .map(|_| ())
    });
    assert!(error.unwrap_err().to_string().contains("notes.txt"));
    assert!(!a.engine.enabled());
    let empty = sandbox.root.join("empty");
    fs::create_dir_all(&empty).unwrap();
    let empty = RemoteSpec::Folder {
        path: empty.to_string_lossy().into(),
    };
    let short = a.engine.setup(empty, "short", &Scope::AVAILABLE, None);
    assert!(short.unwrap_err().to_string().contains("8 characters"));
}

#[test]
fn a_git_push_that_lost_the_race_asks_to_begin_again() {
    let sandbox = Sandbox::new("race");
    let RemoteSpec::Git { url } = sandbox.bare() else {
        unreachable!()
    };
    let clone = |name: &str| Git::new(url.clone(), sandbox.root.join(name), sandbox.env.clone());
    let (mut first, mut second) = (clone("first"), clone("second"));
    first.begin().unwrap();
    put(first.root(), "x", b"1").unwrap();
    assert!(first.commit().unwrap());
    first.begin().unwrap();
    second.begin().unwrap();
    put(first.root(), "y", b"1").unwrap();
    put(second.root(), "z", b"1").unwrap();
    assert!(first.commit().unwrap());
    assert!(!second.commit().unwrap(), "rejected: someone pushed first");
    second.begin().unwrap();
    assert!(second.root().join("y").exists() && !second.root().join("z").exists());
    put(second.root(), "z", b"1").unwrap();
    assert!(second.commit().unwrap());
}
