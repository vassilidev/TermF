mod fake;
mod home;
mod props;
mod remotes;

use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

use super::*;

/// Never the system keychain. Clones share it, as the app and the CLI share the machine's keychain.
#[derive(Default, Clone)]
struct MemoryStore(Arc<Mutex<HashMap<String, Vec<u8>>>>);

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
            _ => unreachable!(),
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
            _ => unreachable!(),
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

#[test]
fn a_password_changed_by_the_cli_does_not_lock_the_app() {
    let sandbox = Sandbox::new("stale-key");
    let remote = sandbox.folder();
    let keychain = MemoryStore::default();
    let app = sandbox.machine_with("a", Box::new(keychain.clone()));
    app.setup(&remote);
    let cli = Engine::new(app.dir.clone(), Box::new(keychain));
    cli.change_password(PASSWORD, "another password").unwrap();
    app.put("termf.toml", "scan_depth = 3\n");
    assert_eq!(app.push().pushed, ["termf.toml"]);
    assert!(app.engine.status().unwrap().unlocked);
}

#[test]
fn setup_refuses_a_repository_with_other_branches_only() {
    let sandbox = Sandbox::new("master");
    let remote = sandbox.bare();
    let RemoteSpec::Git { url } = &remote else {
        unreachable!()
    };
    let work = sandbox.root.join("work");
    fs::create_dir_all(&work).unwrap();
    sandbox.git(&work, &["init", "-q", "-b", "master"]);
    fs::write(work.join("README.md"), "a real project\n").unwrap();
    sandbox.git(&work, &["add", "-A"]);
    sandbox.git(&work, &["commit", "-q", "-m", "init"]);
    sandbox.git(&work, &["push", "-q", url, "master"]);

    let a = sandbox.machine("a");
    let error = a
        .engine
        .setup(remote.clone(), PASSWORD, &Scope::AVAILABLE, None)
        .unwrap_err();
    assert!(error.to_string().contains("master"), "{error:#}");
    assert!(!a.engine.enabled());
    let branches = sandbox.git(
        &sandbox.root.join("remote.git"),
        &["branch", "--format=%(refname:short)"],
    );
    assert_eq!(branches.trim(), "master", "no main pushed");
}

#[test]
fn local_paths_are_kept_per_project_and_dropped_once_they_match() {
    let sandbox = Sandbox::new("mapping");
    let remote = sandbox.folder();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    let project = |name: &str, path: &str| {
        format!("name = {name:?}\n[[repos]]\npath = {path:?}\nremote = \"git@h:x/shared.git\"\n")
    };
    let path = |m: &Machine, p: &str| m.toml(p)["repos"][0]["path"].as_str().unwrap().to_owned();
    a.put("projects/one.toml", &project("One", "~/termf-test-a/one"));
    a.put("projects/two.toml", &project("Two", "~/termf-test-a/two"));
    a.setup(&remote);
    b.setup(&remote);

    // One remote, cloned twice on b: once per project.
    b.put("projects/one.toml", &project("One", "~/termf-test-b/one"));
    b.put("projects/two.toml", &project("Two", "~/termf-test-b/two"));
    assert!(b.push().pushed.is_empty());
    a.put(
        "projects/one.toml",
        &project("One Inc", "~/termf-test-a/one"),
    );
    a.put(
        "projects/two.toml",
        &project("Two Inc", "~/termf-test-a/two"),
    );
    a.push();
    b.push();
    assert_eq!(
        b.toml("projects/one.toml")["name"].as_str(),
        Some("One Inc")
    );
    assert_eq!(path(&b, "projects/one.toml"), "~/termf-test-b/one");
    assert_eq!(path(&b, "projects/two.toml"), "~/termf-test-b/two");

    // Back at the synced path: the local entry goes, so the next pull leaves the path alone.
    b.put(
        "projects/one.toml",
        &project("One Inc", "~/termf-test-a/one"),
    );
    assert!(b.push().pushed.is_empty());
    let local = b.read("local.toml");
    assert!(
        !local.contains("termf-test-b/one") && local.contains("termf-test-b/two"),
        "{local}"
    );
    a.put("projects/one.toml", &project("One", "~/termf-test-a/one"));
    a.push();
    b.push();
    assert_eq!(b.toml("projects/one.toml")["name"].as_str(), Some("One"));
    assert_eq!(path(&b, "projects/one.toml"), "~/termf-test-a/one");
    assert_eq!(path(&b, "projects/two.toml"), "~/termf-test-b/two");
}

#[test]
fn an_unreadable_file_is_skipped_and_reported_never_deleted() {
    let sandbox = Sandbox::new("invalid");
    let remote = sandbox.folder();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put("termf.toml", "scan_depth = 3\n");
    a.put("projects/acme.toml", "name = \"Acme\"\n");
    a.setup(&remote);
    b.setup(&remote);

    a.put("termf.toml", "scan_depth =\n");
    fs::write(a.dir.join("projects/acme.toml"), b"name = \"\xff\"\n").unwrap();
    a.put("library/commands/c.md", "still synced\n");
    let report = a.push();
    assert_eq!(report.pushed, ["library/commands/c.md"]);
    let skipped: Vec<_> = report.skipped.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(skipped, ["projects/acme.toml", "termf.toml"]);
    let error = a.engine.status().unwrap().last_error.unwrap();
    assert!(error.contains("termf.toml is not synced"), "{error}");

    b.push();
    assert_eq!(b.read("termf.toml"), "scan_depth = 3\n");
    assert_eq!(b.read("projects/acme.toml"), "name = \"Acme\"\n");
    assert_eq!(b.read("library/commands/c.md"), "still synced\n");

    a.put("termf.toml", "scan_depth = 4\n");
    a.put("projects/acme.toml", "name = \"Acme\"\n");
    let report = a.push();
    assert!(report.skipped.is_empty(), "{report:?}");
    assert_eq!(report.pushed, ["termf.toml"]);
    assert_eq!(a.engine.status().unwrap().last_error, None);
}

#[test]
fn the_scout_travels_and_its_journals_merge() {
    let sandbox = Sandbox::new("scout");
    let remote = sandbox.folder();
    let (a, b) = (sandbox.machine("a"), sandbox.machine("b"));
    a.put(
        "scout/rejected.toml",
        "[[rejected]]\nfingerprint = \"f1\"\n",
    );
    a.put("scout/global.toml", "[[entries]]\nitem = \"review\"\n");
    a.put("scout/claude-version", "2.1.332\n");
    a.put("scout/journal.jsonl", "{\"n\":1}\n");
    a.setup(&remote);
    b.setup(&remote);
    for path in [
        "scout/rejected.toml",
        "scout/global.toml",
        "scout/claude-version",
    ] {
        assert_eq!(b.read(path), a.read(path), "{path}");
    }

    // Both ran the scout: each journal keeps its lines and gets the other's.
    a.put("scout/journal.jsonl", "{\"n\":1}\n{\"n\":2}\n");
    b.put("scout/journal.jsonl", "{\"n\":1}\n{\"n\":3}\n");
    a.push();
    assert!(b.push().conflicts.is_empty());
    a.push();
    for m in [&a, &b] {
        assert_eq!(
            m.read("scout/journal.jsonl"),
            "{\"n\":1}\n{\"n\":3}\n{\"n\":2}\n"
        );
    }
}

/// What TermF keeps in its config folder and never syncs, with why.
const NOT_SYNCED: &[(&str, &str)] = &[
    (
        "local.toml",
        "this machine's own paths for the projects' repositories",
    ),
    (
        ".sync",
        "the sync's own state on this machine: settings, merge bases, the clone",
    ),
    (".import", "an import in progress, removed once it is done"),
];

/// The first path segment of each `config_dir.join(…)` in `file`: a literal, a `format!` literal, or a constant
/// of the same file. Anything else (a variable) is a path the sync or the archive already chose.
fn config_dir_entries(file: &str) -> Vec<String> {
    let constant = |name: &str| {
        let decl = format!("const {name}: &str = \"");
        let start = file.find(&decl)? + decl.len();
        Some(file[start..].split('"').next()?.to_owned())
    };
    let mut found = Vec::new();
    for (i, _) in file.match_indices("config_dir") {
        let rest = &file[i + "config_dir".len()..];
        let rest = rest.strip_prefix("()").unwrap_or(rest);
        let Some(arg) = rest.strip_prefix(".join(") else {
            continue;
        };
        let arg = arg.trim_start();
        let literal = match arg
            .strip_prefix("format!(\"")
            .or_else(|| arg.strip_prefix('"'))
        {
            Some(text) => text.split(['"', '{']).next().map(str::to_owned),
            None => {
                let name: String = arg
                    .chars()
                    .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                    .collect();
                if name.is_empty() {
                    continue;
                }
                Some(constant(&name).unwrap_or_else(|| panic!("const {name} not found")))
            }
        };
        if let Some(first) = literal.as_deref().and_then(|l| l.split('/').next()) {
            found.push(first.to_owned());
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Like the actions' coverage: a new file in the config folder belongs to a scope, or says here why not.
#[test]
fn every_file_termf_keeps_in_its_config_folder_is_synced_or_says_why_not() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for krate in fs::read_dir(root.join("crates")).unwrap().flatten() {
        rust_files(&krate.path().join("src"), &mut files);
    }
    rust_files(&root.join("app/src-tauri/src"), &mut files);
    let mut seen = BTreeSet::new();
    for file in &files {
        seen.extend(config_dir_entries(&fs::read_to_string(file).unwrap()));
    }
    assert!(seen.len() >= 8, "the sources were read: {seen:?}");
    for name in &seen {
        assert!(
            local::valid(name) || NOT_SYNCED.iter().any(|(n, _)| n == name),
            "{name} is in TermF's config folder but in no sync scope: give it one (`archive::SYNCED`, \
             `local::scope_of`), or say why not in NOT_SYNCED"
        );
    }
    for (name, why) in NOT_SYNCED {
        assert!(why.len() > 20, "a real reason for {name}");
        assert!(seen.contains(*name), "{name} is not written anymore");
    }
    // Where the audit found each kind of setting: in a synced file, of the right scope.
    for (path, scope) in [
        ("scout/journal.jsonl", Scope::Config),
        ("scout/rejected.toml", Scope::Config),
        ("schedules.toml", Scope::Config),
        // My environment: its manifest and its named scripts.
        ("env.toml", Scope::Config),
        ("env/laptop.sh", Scope::Config),
        // Ops connections, saved SQL queries, the Forge account, Jira sites: keys of the project's file.
        ("projects/acme.toml", Scope::Projects),
        ("projects/acme/recipes/deploy.toml", Scope::Projects),
        (
            "projects/acme/library/skills/review/SKILL.md",
            Scope::Projects,
        ),
        ("projects/acme/conventions/c/source.md", Scope::Conventions),
    ] {
        assert!(
            local::valid(path) && local::scope_of(path) == scope,
            "{path}"
        );
    }
}
