use std::collections::HashMap;
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

/// Wall and monotonic clocks moved by hand: both while awake, only the wall one while asleep.
#[derive(Clone)]
struct TestClock(Arc<Mutex<(u64, Duration)>>);

impl TestClock {
    fn awake(&self, secs: u64) {
        let mut t = self.0.lock().unwrap();
        t.0 += secs;
        t.1 += Duration::from_secs(secs);
    }

    fn asleep(&self, secs: u64) {
        self.0.lock().unwrap().0 += secs;
    }
}

impl Clock for TestClock {
    fn wall(&self) -> u64 {
        self.0.lock().unwrap().0
    }
    fn mono(&self) -> Duration {
        self.0.lock().unwrap().1
    }
}

const PASSWORD: &str = "correct horse battery";
const VALUE: &str = "pg://acme:hunter2-0123456789@db.acme.test/app";

struct Env {
    root: PathBuf,
    store: MemoryStore,
    clock: TestClock,
}

impl Env {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("termf-vault-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self {
            root,
            store: MemoryStore::default(),
            clock: TestClock(Arc::new(Mutex::new((1_800_000_000, Duration::ZERO)))),
        }
    }

    fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    /// Another process on the same machine: same files, same keychain, same time.
    fn vault(&self) -> Vault {
        Vault::new(
            &self.data(),
            Box::new(self.store.clone()),
            Box::new(self.clock.clone()),
        )
    }

    fn file(&self) -> PathBuf {
        self.data().join("vault/vault.json")
    }

    fn actions(&self, vault: &Vault) -> Vec<Action> {
        let mut entries = vault.audit(1000).unwrap();
        entries.reverse();
        entries.into_iter().map(|e| e.action).collect()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn id(name: &str, scope: Scope) -> SecretId {
    SecretId::new(name, scope, None).unwrap()
}

fn human() -> Actor {
    Actor::human()
}

#[test]
fn keeps_secrets_encrypted_and_gives_them_back() {
    let env = Env::new("roundtrip");
    let vault = env.vault();
    assert!(!vault.status().unwrap().initialized);
    assert!(vault.unlock(&human(), Some(PASSWORD), None).is_err());
    assert!(vault.init(&human(), "short", false).is_err());
    let status = vault.init(&human(), PASSWORD, false).unwrap();
    assert!(status.initialized && status.unlocked && !status.remembered);
    assert!(vault.init(&human(), PASSWORD, false).is_err(), "only one");

    let db = id("DATABASE_URL", Scope::Dev);
    vault
        .set(&human(), &db, Secret::new(VALUE.into()), Some(" Main DB "))
        .unwrap();
    let acme = SecretId::new("API_KEY", Scope::Staging, Some("acme")).unwrap();
    vault
        .set(
            &human(),
            &acme,
            Secret::new("sk-staging-0123456789".into()),
            None,
        )
        .unwrap();
    let other = SecretId::new("API_KEY", Scope::Staging, Some("other")).unwrap();
    vault
        .set(
            &human(),
            &other,
            Secret::new("sk-other-0123456789".into()),
            None,
        )
        .unwrap();

    let on_disk = fs::read_to_string(env.file()).unwrap();
    for clear in [VALUE, "DATABASE_URL", "Main DB", "acme", "sk-staging"] {
        assert!(!on_disk.contains(clear), "{clear} in the file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&env.file()), 0o600);
        assert_eq!(mode(&env.data().join("vault")), 0o700);
    }

    let names = |list: Vec<SecretInfo>| -> Vec<String> {
        list.into_iter()
            .map(|s| format!("{}/{}", s.project.unwrap_or_default(), s.name))
            .collect()
    };
    assert_eq!(
        names(vault.list(None, Some("acme")).unwrap()),
        ["/DATABASE_URL", "acme/API_KEY"]
    );
    assert_eq!(
        names(vault.list(Some(Scope::Staging), None).unwrap()).len(),
        2
    );
    let info = &vault.list(Some(Scope::Dev), None).unwrap()[0];
    assert_eq!(info.description, "Main DB");
    assert_eq!(info.reference, None);

    vault.lock(&human()).unwrap();
    let err = vault.list(None, None).unwrap_err();
    assert!(err.downcast_ref::<Locked>().is_some());

    // The CLI: another process, which unlocks with the password.
    let cli = env.vault();
    cli.unlock(&human(), Some(PASSWORD), None).unwrap();
    assert_eq!(
        cli.value(&human(), &db, Access::Reveal).unwrap().expose(),
        VALUE
    );
    cli.set(&human(), &db, Secret::new("changed-value".into()), None)
        .unwrap();
    let info = &cli.list(Some(Scope::Dev), None).unwrap()[0];
    assert_eq!(info.description, "Main DB", "kept without a new one");
    cli.delete(&human(), &acme).unwrap();
    assert!(cli.delete(&human(), &acme).is_err());

    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
    assert_eq!(
        vault.value(&human(), &db, Access::Use).unwrap().expose(),
        "changed-value"
    );
    assert_eq!(vault.list(None, None).unwrap().len(), 2);
    assert_eq!(
        env.actions(&vault),
        [
            Action::Init,
            Action::Write,
            Action::Write,
            Action::Write,
            Action::Lock,
            Action::Unlock,
            Action::Reveal,
            Action::Write,
            Action::Delete,
            Action::Unlock,
            Action::Read,
        ]
    );
    let reveal = &vault.audit(100).unwrap()[4];
    assert_eq!(reveal.action, Action::Reveal);
    assert_eq!(reveal.target.as_deref(), Some("DATABASE_URL"));
    assert_eq!(reveal.size, Some(VALUE.len() as u64));
}

#[test]
fn refuses_a_wrong_password_and_a_tampered_file() {
    let env = Env::new("tamper");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    vault
        .set(
            &human(),
            &id("TOKEN", Scope::Dev),
            Secret::new(VALUE.into()),
            None,
        )
        .unwrap();
    vault.lock(&human()).unwrap();

    let err = vault
        .unlock(&human(), Some("not the password"), None)
        .unwrap_err()
        .to_string();
    assert_eq!(err, "wrong master password");
    assert_eq!(*env.actions(&vault).last().unwrap(), Action::UnlockFailed);

    let original = fs::read_to_string(env.file()).unwrap();
    let tamper = |field: &str| {
        let mut file: serde_json::Value = serde_json::from_str(&original).unwrap();
        let text = file[field].as_str().unwrap().to_owned();
        let flipped = if text.ends_with('0') { '1' } else { '0' };
        file[field] = format!("{}{flipped}", &text[..text.len() - 1]).into();
        fs::write(env.file(), file.to_string()).unwrap();
    };
    tamper("data");
    let err = vault.unlock(&human(), Some(PASSWORD), None).unwrap_err();
    assert_eq!(err.to_string(), DAMAGED);
    assert!(!vault.status().unwrap().unlocked);
    tamper("key");
    assert!(vault.unlock(&human(), Some(PASSWORD), None).is_err());
    fs::write(env.file(), "{ not json").unwrap();
    assert_eq!(
        vault
            .unlock(&human(), Some(PASSWORD), None)
            .unwrap_err()
            .to_string(),
        DAMAGED
    );
    fs::write(env.file(), &original).unwrap();
    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
}

#[test]
fn locks_after_the_idle_delay_and_after_a_sleep() {
    let env = Env::new("autolock");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let dev = id("TOKEN", Scope::Dev);
    vault
        .set(&human(), &dev, Secret::new(VALUE.into()), None)
        .unwrap();

    env.clock.awake(9 * 60);
    assert!(!vault.check());
    vault.list(None, None).unwrap();
    env.clock.awake(9 * 60);
    assert!(!vault.check(), "using it put the delay back");
    env.clock.awake(60);
    assert!(vault.check());
    assert!(!vault.status().unwrap().unlocked);
    let lock = &vault.audit(1).unwrap()[0];
    assert_eq!(
        (lock.action, lock.actor.kind, lock.detail.as_deref()),
        (Action::Lock, ActorKind::Termf, Some("idle"))
    );

    vault.set_auto_lock(1).unwrap();
    assert!(vault.set_auto_lock(0).is_err());
    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
    assert_eq!(vault.status().unwrap().auto_lock_minutes, 1);
    env.clock.awake(61);
    assert!(
        vault
            .list(None, None)
            .unwrap_err()
            .downcast_ref::<Locked>()
            .is_some(),
        "locked on use too, without waiting for a check"
    );

    vault.set_auto_lock(10).unwrap();
    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
    env.clock.awake(20);
    env.clock.asleep(20);
    assert!(!vault.check(), "a short gap is not a sleep");
    env.clock.asleep(40);
    assert!(vault.check());
    assert_eq!(vault.audit(1).unwrap()[0].detail.as_deref(), Some("sleep"));
}

#[test]
fn prod_asks_for_the_password_again_after_five_minutes() {
    let env = Env::new("prod");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, true).unwrap();
    let prod = id("DATABASE_URL", Scope::Prod);
    vault
        .set(&human(), &prod, Secret::new(VALUE.into()), None)
        .unwrap();
    let until = vault.status().unwrap().prod_until.unwrap();
    assert_eq!(until, env.clock.wall() + 5 * 60);

    env.clock.awake(4 * 60);
    vault.value(&human(), &prod, Access::Reveal).unwrap();
    env.clock.awake(61);
    assert_eq!(vault.status().unwrap().prod_until, None);
    let err = vault.value(&human(), &prod, Access::Use).unwrap_err();
    assert!(err.downcast_ref::<NeedsPassword>().is_some());
    assert!(vault.delete(&human(), &prod).is_err());
    assert!(
        vault
            .set(&human(), &prod, Secret::new("x-value".into()), None)
            .is_err()
    );
    assert!(vault.reauth(&human(), "wrong password").is_err());
    vault.reauth(&human(), PASSWORD).unwrap();
    assert_eq!(
        vault
            .value(&human(), &prod, Access::Reveal)
            .unwrap()
            .expose(),
        VALUE
    );
    let dev = id("TOKEN", Scope::Dev);
    env.clock.awake(6 * 60);
    vault
        .set(&human(), &dev, Secret::new(VALUE.into()), None)
        .unwrap();

    // The keychain unlocks, but only a typed password opens prod.
    vault.lock(&human()).unwrap();
    vault.unlock(&human(), None, None).unwrap();
    assert!(vault.value(&human(), &prod, Access::Reveal).is_err());
    vault.reauth(&human(), PASSWORD).unwrap();
    env.clock.awake(60);
    env.clock.asleep(60);
    assert!(
        vault.value(&human(), &prod, Access::Reveal).is_err(),
        "slept"
    );

    let refused = vault
        .audit(100)
        .unwrap()
        .into_iter()
        .filter(|e| e.action == Action::Refused)
        .count();
    assert_eq!(refused, 5);
    assert!(
        env.actions(&vault)
            .windows(2)
            .any(|w| w == [Action::ReauthFailed, Action::Reauth])
    );
}

#[test]
fn keeps_the_key_in_the_keychain_only_when_asked() {
    let env = Env::new("keychain");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    assert!(env.store.0.lock().unwrap().is_empty());
    vault.lock(&human()).unwrap();
    assert_eq!(
        vault.unlock(&human(), None, None).unwrap_err().to_string(),
        "the master password is needed"
    );

    vault.unlock(&human(), Some(PASSWORD), Some(true)).unwrap();
    assert!(vault.status().unwrap().remembered);
    assert_eq!(env.store.0.lock().unwrap().len(), 1);
    vault.lock(&human()).unwrap();
    let app = env.vault();
    let status = app.unlock(&human(), None, None).unwrap();
    assert!(status.unlocked && status.prod_until.is_none());
    assert_eq!(app.audit(1).unwrap()[0].detail.as_deref(), Some("keychain"));

    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
    assert!(vault.status().unwrap().remembered, "None leaves it");
    vault.unlock(&human(), Some(PASSWORD), Some(false)).unwrap();
    assert!(!vault.status().unwrap().remembered);
    assert!(env.store.0.lock().unwrap().is_empty());
    vault.lock(&human()).unwrap();
    assert!(vault.unlock(&human(), None, None).is_err());
}

#[test]
fn never_shows_a_value_to_an_agent_nor_in_debug_or_errors() {
    let env = Env::new("leaks");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let dev = id("TOKEN", Scope::Dev);
    vault
        .set(&human(), &dev, Secret::new(VALUE.into()), None)
        .unwrap();

    let agent = Actor::agent(Some(7), Some("3".into()));
    assert!(vault.value(&agent, &dev, Access::Reveal).is_err());
    assert_eq!(
        vault.value(&agent, &dev, Access::Use).unwrap().expose(),
        VALUE
    );
    let entries = vault.audit(2).unwrap();
    assert_eq!(entries[1].action, Action::Refused);
    assert_eq!(
        (entries[0].action, &entries[0].actor),
        (Action::Read, &agent)
    );

    let secret = Secret::new(VALUE.into());
    let mut texts = vec![
        format!("{secret:?}"),
        format!("{:?}", vault.list(None, None).unwrap()),
        format!("{:?}", vault.masker().unwrap()),
        format!("{:?}", vault.audit(100).unwrap()),
        serde_json::to_string(&vault.list(None, None).unwrap()).unwrap(),
        serde_json::to_string(&vault.status().unwrap()).unwrap(),
    ];
    let mut export = Vec::new();
    vault.export(&mut export).unwrap();
    texts.push(String::from_utf8(export).unwrap());
    for err in [
        vault
            .set(&human(), &dev, Secret::new(String::new()), None)
            .unwrap_err(),
        SecretId::new("1bad", Scope::Dev, None).unwrap_err(),
        vault
            .set(&human(), &dev, Secret::new(VALUE.repeat(2000)), None)
            .unwrap_err(),
        vault.unlock(&human(), Some(VALUE), None).unwrap_err(),
        vault.reauth(&human(), VALUE).unwrap_err(),
    ] {
        texts.push(format!("{err:#} {err:?}"));
    }
    for text in texts {
        assert!(!text.contains("hunter2"), "{text}");
    }
}

#[test]
fn masks_the_values_it_knows_then_what_looks_like_a_secret() {
    let env = Env::new("mask");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let pem = "-----BEGIN KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASC\n-----END KEY-----";
    let account = "{\n  \"type\": \"service_account\",\n  \"project_id\": \"acme\",\n  \"private_key_id\": \"0a1b2c3d4e5f60718293a4b5c6d7e8f9\"\n}";
    for (name, value) in [
        ("DB", VALUE),
        ("PIN", "123"),
        ("KEY", pem),
        ("WORD", "acme-prod"),
        ("ACCOUNT", account),
    ] {
        vault
            .set(
                &human(),
                &id(name, Scope::Dev),
                Secret::new(value.into()),
                None,
            )
            .unwrap();
    }
    let mut masker = vault.masker().unwrap();
    let text = format!(
        "connect {VALUE} as acme-prod, pin 123\nline MIIEvQIBADANBgkqhkiG9w0BAQEFAASC\nAuthorization: Bearer abc"
    );
    let masked = masker.mask(&text);
    assert_eq!(
        masked.text,
        format!(
            "connect {MASK} as {MASK}, pin 123\nline {MASK}\nAuthorization: Bearer {MASK}",
            MASK = termf_claude::mask::MASK
        )
    );
    assert_eq!(masked.count, 4);
    let ordinary = "-----BEGIN KEY-----\n  \"type\": \"service_account\",\n}";
    assert_eq!(
        masker.mask(ordinary).text,
        ordinary,
        "the armor and a JSON file's ordinary lines stay visible"
    );
    masker.add(&Secret::new("resolved-from-op".into()));
    assert_eq!(masker.mask("x resolved-from-op").count, 1);
}

#[test]
fn chains_the_audit_and_finds_any_change() {
    let env = Env::new("audit");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    for i in 0..4 {
        vault
            .set(
                &human(),
                &id(&format!("S{i}"), Scope::Dev),
                Secret::new(VALUE.into()),
                None,
            )
            .unwrap();
    }
    let ok = vault.verify().unwrap();
    assert!(ok.ok && ok.entries == 5, "{ok:?}");
    let entries = vault.audit(10).unwrap();
    assert_eq!(entries.last().unwrap().prev, audit::GENESIS);
    assert_eq!(entries[0].prev, entries[1].hash);
    let mut export = Vec::new();
    assert_eq!(vault.export(&mut export).unwrap(), 5);
    assert_eq!(String::from_utf8(export).unwrap().lines().count(), 5);

    let db = rusqlite::Connection::open(env.data().join("termf.db")).unwrap();
    assert!(
        db.execute("DELETE FROM vault_audit WHERE id = 2", [])
            .is_err(),
        "append-only"
    );
    assert!(
        db.execute("UPDATE vault_audit SET entry = '' WHERE id = 2", [])
            .is_err()
    );
    // Someone who drops the triggers first.
    let tamper = |sql: &str, entry: &str| {
        db.execute_batch("DROP TRIGGER vault_audit_no_update; DROP TRIGGER vault_audit_no_delete;")
            .unwrap();
        db.execute(sql, [entry]).unwrap();
    };
    let entry: String = db
        .query_row("SELECT entry FROM vault_audit WHERE id = 3", [], |r| {
            r.get(0)
        })
        .unwrap();
    let edited = entry.replace("\"target\":\"S1\"", "\"target\":\"S9\"");
    assert_ne!(edited, entry);
    tamper("UPDATE vault_audit SET entry = ?1 WHERE id = 3", &edited);
    let broken = vault.verify().unwrap();
    assert_eq!((broken.ok, broken.broken), (false, Some(3)), "{broken:?}");
    tamper("UPDATE vault_audit SET entry = ?1 WHERE id = 3", &entry);
    assert!(vault.verify().unwrap().ok);
    tamper("DELETE FROM vault_audit WHERE id = 2 AND ?1 != ''", "x");
    assert_eq!(vault.verify().unwrap().broken, Some(3));
}

#[cfg(unix)]
#[test]
fn resolves_references_with_the_managers_cli_and_never_keeps_the_value() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new("refs");
    let bin = env.root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let log = env.root.join("op.log");
    let op = bin.join("op");
    fs::write(
        &op,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\n\
             if [ \"$2\" = op://acme/db/password ]; then printf 'from-op-0123456789\\n'; exit 0; fi\n\
             if [ \"$2\" = op://acme/agent/password ]; then /bin/sleep 30 & printf 'kept-open-0123456789\\n'; exit 0; fi\n\
             if [ \"$2\" = op://acme/hang/password ]; then exec /bin/sleep 30; fi\n\
             echo \"[ERROR] not found, token=ghp_0123456789abcdefghij0123456789abcd\" >&2; exit 1\n",
            log.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&op, fs::Permissions::from_mode(0o755)).unwrap();
    let vault = env.vault().with_path(Some(bin.clone().into()));
    vault.init(&human(), PASSWORD, false).unwrap();

    let db = id("DATABASE_PASSWORD", Scope::Dev);
    assert!(
        vault
            .set_reference(&human(), &db, "op://acme/db", None)
            .is_err()
    );
    let info = vault
        .set_reference(&human(), &db, "op://acme/db/password", Some("1Password"))
        .unwrap();
    assert_eq!(info.reference.as_deref(), Some("op://acme/db/password"));
    assert!(!fs::exists(&log).unwrap(), "checked, not resolved");
    assert_eq!(
        vault.value(&human(), &db, Access::Reveal).unwrap().expose(),
        "from-op-0123456789"
    );
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        "read op://acme/db/password\n"
    );
    let resolve = &vault.audit(1).unwrap()[0];
    assert_eq!(resolve.action, Action::Resolve);
    assert_eq!(
        resolve.detail.as_deref(),
        Some("reveal op://acme/db/password")
    );
    assert_eq!(resolve.size, Some(18));
    vault.lock(&human()).unwrap();
    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
    assert!(
        !vault
            .masker()
            .unwrap()
            .mask("from-op-0123456789")
            .text
            .contains('•'),
        "never stored, so not known"
    );

    let missing = id("OTHER", Scope::Dev);
    vault
        .set_reference(&human(), &missing, "op://acme/other/password", None)
        .unwrap();
    let err = vault
        .value(&human(), &missing, Access::Use)
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("op could not read op://acme/other/password"),
        "{err}"
    );
    assert!(err.contains("not found") && !err.contains("ghp_"), "{err}");

    let bw = id("BW", Scope::Dev);
    vault
        .set_reference(&human(), &bw, "bw://acme-db", None)
        .unwrap();
    let err = vault.value(&human(), &bw, Access::Use).unwrap_err();
    assert_eq!(err.to_string(), "bw is not installed or not on the PATH");

    // A helper left running with the output pipe open, then a CLI that never ends: one deadline for both.
    let kept = id("KEPT", Scope::Dev);
    vault
        .set_reference(&human(), &kept, "op://acme/agent/password", None)
        .unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        vault.value(&human(), &kept, Access::Use).unwrap().expose(),
        "kept-open-0123456789"
    );
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
    let hang = id("HANG", Scope::Dev);
    vault
        .set_reference(&human(), &hang, "op://acme/hang/password", None)
        .unwrap();
    let start = std::time::Instant::now();
    let err = vault.value(&human(), &hang, Access::Use).unwrap_err();
    assert_eq!(err.to_string(), "op did not answer within 3 s");
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn records_before_it_changes_anything_and_always_locks() {
    let env = Env::new("audit-first");
    // The audit cannot be written: a folder where the database goes.
    fs::create_dir_all(env.data().join("termf.db")).unwrap();
    let vault = env.vault();
    assert!(vault.init(&human(), PASSWORD, true).is_err());
    assert!(!vault.initialized(), "no entry, no vault");
    assert!(env.store.0.lock().unwrap().is_empty());
    fs::remove_dir(env.data().join("termf.db")).unwrap();

    vault.init(&human(), PASSWORD, false).unwrap();
    let dev = id("TOKEN", Scope::Dev);
    vault
        .set(&human(), &dev, Secret::new(VALUE.into()), None)
        .unwrap();
    let saved = env.root.join("saved");
    fs::create_dir_all(&saved).unwrap();
    let break_audit = || {
        for suffix in ["", "-wal", "-shm"] {
            let file = env.data().join(format!("termf.db{suffix}"));
            if file.exists() {
                fs::rename(&file, saved.join(format!("termf.db{suffix}"))).unwrap();
            }
        }
        fs::create_dir_all(env.data().join("termf.db")).unwrap();
    };
    let mend_audit = || {
        fs::remove_dir(env.data().join("termf.db")).unwrap();
        for entry in fs::read_dir(&saved).unwrap() {
            let entry = entry.unwrap();
            fs::rename(entry.path(), env.data().join(entry.file_name())).unwrap();
        }
    };
    break_audit();
    assert!(
        vault
            .set(&human(), &dev, Secret::new("other-value".into()), None)
            .is_err()
    );
    assert!(vault.delete(&human(), &dev).is_err());
    assert!(vault.value(&human(), &dev, Access::Reveal).is_err());
    assert!(
        !vault.lock(&human()).unwrap().unlocked,
        "locked all the same"
    );
    assert!(vault.unlock(&human(), Some(PASSWORD), None).is_err());
    assert!(!vault.status().unwrap().unlocked, "no entry, no unlock");
    mend_audit();

    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
    assert_eq!(
        vault
            .value(&human(), &dev, Access::Reveal)
            .unwrap()
            .expose(),
        VALUE,
        "unchanged, not deleted"
    );
    assert!(vault.verify().unwrap().ok);
}

#[test]
fn tells_when_it_locks_itself() {
    let env = Env::new("on-lock");
    let told = Arc::new(Mutex::new(Vec::new()));
    let seen = told.clone();
    let vault = env
        .vault()
        .on_lock(move |status| seen.lock().unwrap().push(status.unlocked));
    vault.init(&human(), PASSWORD, false).unwrap();
    vault.lock(&human()).unwrap();
    assert!(told.lock().unwrap().is_empty(), "not when asked to");
    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
    env.clock.awake(11 * 60);
    assert!(vault.list(None, None).is_err());
    assert_eq!(*told.lock().unwrap(), [false], "a command noticed it first");
    assert!(!vault.check());
    assert_eq!(told.lock().unwrap().len(), 1);
}

#[cfg(unix)]
#[test]
fn turns_core_dumps_off() {
    harden_process();
    // SAFETY: reads this process's own limit into a valid `rlimit`.
    let limit = unsafe {
        let mut limit = libc::rlimit {
            rlim_cur: 1,
            rlim_max: 1,
        };
        libc::getrlimit(libc::RLIMIT_CORE, &mut limit);
        limit
    };
    assert_eq!(limit.rlim_cur, 0);
}

const SEED: &str = "JBSWY3DPEHPK3PXP";

fn login(project: Option<&str>, title: &str) -> ItemDraft {
    ItemDraft {
        id: None,
        kind: Kind::Login,
        project: project.map(Into::into),
        title: title.into(),
        urls: vec!["https://github.com ".into(), " ".into()],
        username: " alex ".into(),
        password: Some(Secret::new("hunter2-github-0123".into())),
        totp: Some(Secret::new(format!(
            "otpauth://totp/GitHub:alex?secret={SEED}&issuer=GitHub"
        ))),
        notes: Some(Secret::new("recovery codes: hunter2-a hunter2-b".into())),
        agents: false,
    }
}

#[test]
fn keeps_logins_and_notes_by_project_and_never_shows_their_values() {
    let env = Env::new("items");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let github = vault
        .save_item(&human(), login(Some("acme"), "GitHub"))
        .unwrap();
    assert_eq!(github.id.len(), 32);
    assert_eq!(
        (
            github.urls.clone(),
            github.username.as_str(),
            github.has_password,
            github.has_notes
        ),
        (vec!["https://github.com".to_owned()], "alex", true, true)
    );
    assert_eq!(
        github.totp,
        Some(TotpInfo {
            algorithm: Algorithm::Sha1,
            digits: 6,
            period: 30
        })
    );
    let note = vault
        .save_item(
            &human(),
            ItemDraft {
                kind: Kind::Note,
                password: None,
                totp: None,
                notes: Some(Secret::new("# Wi-Fi\nhunter2-wifi".into())),
                ..login(None, "office Wi-Fi")
            },
        )
        .unwrap();
    assert_eq!(
        (note.username.as_str(), note.has_password, note.totp),
        ("", false, None)
    );
    vault
        .save_item(&human(), login(Some("other"), "AWS"))
        .unwrap();

    let titles = |project| -> Vec<String> {
        vault
            .items(project)
            .unwrap()
            .into_iter()
            .map(|i| i.title)
            .collect()
    };
    assert_eq!(titles(Some("acme")), ["GitHub", "office Wi-Fi"]);
    assert_eq!(titles(None), ["AWS", "GitHub", "office Wi-Fi"]);

    // Left as they are when the draft has none, removed when empty.
    vault
        .save_item(
            &human(),
            ItemDraft {
                id: Some(github.id.clone()),
                password: None,
                totp: Some(Secret::default()),
                notes: None,
                ..login(Some("acme"), "GitHub (work)")
            },
        )
        .unwrap();
    let pass = vault
        .item_value(&human(), &github.id, Field::Password, Access::Reveal)
        .unwrap();
    assert_eq!(pass.expose(), "hunter2-github-0123");
    let changed = &vault.items(Some("acme")).unwrap()[0];
    assert_eq!(
        (changed.title.as_str(), changed.totp, changed.has_notes),
        ("GitHub (work)", None, true)
    );
    let err = vault
        .item_value(&human(), &github.id, Field::Totp, Access::Reveal)
        .unwrap_err();
    assert_eq!(err.to_string(), "this item has no totp");

    let on_disk = fs::read_to_string(env.file()).unwrap();
    for clear in ["hunter2", "GitHub", "alex", SEED, "Wi-Fi", "acme"] {
        assert!(!on_disk.contains(clear), "{clear} in the file");
    }
    let mut texts = vec![
        serde_json::to_string(&vault.items(None).unwrap()).unwrap(),
        format!("{:?}", vault.items(None).unwrap()),
        format!("{:?}", login(None, "x")),
        format!("{:?}", vault.audit(100).unwrap()),
    ];
    let bad = ItemDraft {
        totp: Some(Secret::new("hunter2".into())),
        ..login(None, "x")
    };
    texts.push(vault.save_item(&human(), bad).unwrap_err().to_string());
    let long = ItemDraft {
        password: Some(Secret::new("hunter2".repeat(20_000))),
        ..login(None, "x")
    };
    texts.push(vault.save_item(&human(), long).unwrap_err().to_string());
    for text in texts {
        assert!(!text.contains("hunter2") && !text.contains(SEED), "{text}");
    }
    assert!(vault.save_item(&human(), login(Some("../x"), "x")).is_err());
    assert!(vault.save_item(&human(), login(None, "  ")).is_err());
    let unknown = ItemDraft {
        id: Some("0".repeat(32)),
        ..login(None, "x")
    };
    assert!(vault.save_item(&human(), unknown).is_err());

    vault.delete_item(&human(), &note.id).unwrap();
    assert!(vault.delete_item(&human(), &note.id).is_err());
    assert_eq!(titles(None), ["AWS", "GitHub (work)"]);

    let github_entries: Vec<_> = vault
        .audit(100)
        .unwrap()
        .into_iter()
        .filter(|e| e.item.as_deref() == Some(github.id.as_str()))
        .map(|e| (e.action, e.detail))
        .collect();
    assert_eq!(
        github_entries,
        [
            (Action::Reveal, Some("password".into())),
            (Action::Write, None),
            (Action::Write, None),
        ]
    );
    assert!(vault.verify().unwrap().ok);
    vault.lock(&human()).unwrap();
    let err = vault.items(None).unwrap_err();
    assert!(err.downcast_ref::<Locked>().is_some());
}

#[test]
fn gives_an_item_to_an_agent_only_when_opened_to_agents() {
    let env = Env::new("item-agents");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let closed = vault.save_item(&human(), login(None, "GitHub")).unwrap();
    let open = vault
        .save_item(
            &human(),
            ItemDraft {
                agents: true,
                ..login(Some("acme"), "Stripe")
            },
        )
        .unwrap();
    let agent = Actor::agent(Some(7), Some("3".into()));

    let err = vault
        .item_value(&agent, &closed.id, Field::Password, Access::Use)
        .unwrap_err();
    assert_eq!(err.to_string(), "this item is not open to agents");
    let err = vault
        .item_value(&agent, &open.id, Field::Password, Access::Reveal)
        .unwrap_err();
    assert_eq!(err.to_string(), "only the user reveals a secret");
    let used = vault
        .item_value(&agent, &open.id, Field::Password, Access::Use)
        .unwrap();
    assert_eq!(used.expose(), "hunter2-github-0123");
    let code = vault
        .item_value(&agent, &open.id, Field::Totp, Access::Use)
        .unwrap();
    assert_eq!(
        code.expose(),
        Totp::parse(SEED).unwrap().code(env.clock.wall()).expose()
    );

    let entries: Vec<_> = vault
        .audit(4)
        .unwrap()
        .into_iter()
        .rev()
        .map(|e| (e.action, e.item, e.actor.kind, e.detail))
        .collect();
    let agent_kind = ActorKind::Agent;
    assert_eq!(
        entries,
        [
            (
                Action::Refused,
                Some(closed.id.clone()),
                agent_kind,
                Some("this item is not open to agents".into())
            ),
            (
                Action::Refused,
                Some(open.id.clone()),
                agent_kind,
                Some("only the user reveals a secret".into())
            ),
            (
                Action::Read,
                Some(open.id.clone()),
                agent_kind,
                Some("password".into())
            ),
            (
                Action::Read,
                Some(open.id.clone()),
                agent_kind,
                Some("totp".into())
            ),
        ]
    );

    let masked = vault
        .masker()
        .unwrap()
        .mask(&format!("pass hunter2-github-0123 seed {SEED} done"));
    assert!(masked.count >= 2, "{}", masked.text);
    assert!(!masked.text.contains("hunter2") && !masked.text.contains(SEED));
}

#[test]
fn migrates_a_format_1_vault_and_refuses_a_newer_one() {
    let env = Env::new("migrate");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let db = id("DATABASE_URL", Scope::Dev);
    vault
        .set(&human(), &db, Secret::new(VALUE.into()), None)
        .unwrap();
    // A vault as the first TermF wrote it: format 1, a content without items.
    let (_, key) = vault.key().unwrap();
    let mut file = vault.read().unwrap();
    let mut content = serde_json::to_value(file.open(&key).unwrap()).unwrap();
    content.as_object_mut().unwrap().remove("items");
    let plain = serde_json::to_vec(&content).unwrap();
    file.data = crypto::hex(&crypto::seal(&key, &aad("data", &file.id), &plain));
    file.format = 1;
    vault.write(&file).unwrap();
    let v1 = fs::read(env.file()).unwrap();
    vault.lock(&human()).unwrap();

    let vault = env.vault();
    vault.unlock(&human(), Some(PASSWORD), None).unwrap();
    assert!(vault.items(None).unwrap().is_empty());
    let format = || -> u64 {
        let file: serde_json::Value =
            serde_json::from_slice(&fs::read(env.file()).unwrap()).unwrap();
        file["format"].as_u64().unwrap()
    };
    assert_eq!(format(), 1, "reading changes nothing");
    vault.save_item(&human(), login(None, "GitHub")).unwrap();
    assert_eq!(format(), 2);
    let backup = env.data().join("vault/vault.v1.json");
    assert_eq!(fs::read(&backup).unwrap(), v1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&backup).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    assert_eq!(
        vault.value(&human(), &db, Access::Use).unwrap().expose(),
        VALUE
    );
    assert_eq!(vault.items(None).unwrap().len(), 1);

    let mut newer: serde_json::Value =
        serde_json::from_slice(&fs::read(env.file()).unwrap()).unwrap();
    newer["format"] = 3.into();
    fs::write(env.file(), newer.to_string()).unwrap();
    let err = env
        .vault()
        .unlock(&human(), Some(PASSWORD), None)
        .unwrap_err();
    assert_eq!(err.to_string(), "this vault was written by a newer TermF");
    assert!(vault.items(None).is_err());
}

#[test]
fn a_totp_code_on_screen_does_not_keep_the_vault_open() {
    let env = Env::new("totp-idle");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let github = vault.save_item(&human(), login(None, "GitHub")).unwrap();
    for _ in 0..19 {
        env.clock.awake(30);
        vault
            .item_value(&human(), &github.id, Field::Totp, Access::Reveal)
            .unwrap();
    }
    env.clock.awake(30);
    let err = vault
        .item_value(&human(), &github.id, Field::Totp, Access::Reveal)
        .unwrap_err();
    assert!(err.downcast_ref::<Locked>().is_some());
}

#[test]
fn hashes_an_entry_without_an_item_as_before() {
    let env = Env::new("audit-compat");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let json = serde_json::to_string(&vault.audit(1).unwrap()[0]).unwrap();
    assert!(!json.contains("\"item\""), "{json}");
}
