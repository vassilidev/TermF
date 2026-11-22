use std::process::Command;

use termf_sync::{Engine, RemoteSpec, Report, VAULT_PATH};

use super::*;
use crate::merge::Change;
use crate::synced::Synced;
use crate::transfer::ForeignChoice;

const SYNC_PASSWORD: &str = "sync horse battery";
const NEW_PASSWORD: &str = "a brand new password";
const EXPORT_PASSWORD: &str = "export horse battery";

/// A git that reads none of the user's config, and a bare repository beside it: nothing leaves the machine.
fn bare_remote(root: &Path) -> (RemoteSpec, Vec<(String, String)>) {
    let gitconfig = root.join("gitconfig");
    fs::write(&gitconfig, "").unwrap();
    let env: Vec<(String, String)> = [
        ("GIT_CONFIG_GLOBAL", gitconfig.to_string_lossy().as_ref()),
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_AUTHOR_NAME", "t"),
        ("GIT_AUTHOR_EMAIL", "t@t"),
        ("GIT_COMMITTER_NAME", "t"),
        ("GIT_COMMITTER_EMAIL", "t@t"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let dir = root.join("remote.git");
    fs::create_dir_all(&dir).unwrap();
    let ok = Command::new("git")
        .current_dir(&dir)
        .args(["init", "--bare", "-q"])
        .envs(env.iter().map(|(k, v)| (k, v)))
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let url = dir.to_string_lossy().into();
    (RemoteSpec::Git { url }, env)
}

/// One machine: its own vault (files, keychain, clock) and its own sync engine.
struct Machine {
    env: Env,
    vault: Vault,
    engine: Engine,
}

impl Machine {
    fn new(name: &str, git: &[(String, String)]) -> Self {
        let env = Env::new(name);
        let vault = env.vault();
        let engine = Engine::new(env.root.join("config"), Box::new(MemoryStore::default()))
            .with_vault(env.data().join("vault"))
            .with_git_env(git.to_vec());
        Self { env, vault, engine }
    }

    fn setup(&self, remote: &RemoteSpec) -> Report {
        let scopes = [termf_sync::Scope::Config, termf_sync::Scope::Vault];
        let setup = self
            .engine
            .setup(remote.clone(), SYNC_PASSWORD, &scopes, None)
            .unwrap();
        assert!(setup.error.is_none(), "{:?}", setup.error);
        setup.report
    }

    /// As the app does: sync, merge what came while the vault is open, then push the merge.
    fn sync(&self) {
        self.engine.sync(true).unwrap();
        self.vault.merge_synced(&human()).unwrap();
        self.engine.sync(true).unwrap();
    }

    fn titles(&self) -> Vec<String> {
        let mut titles: Vec<String> = self
            .vault
            .items(None)
            .unwrap()
            .into_iter()
            .map(|i| i.title)
            .collect();
        titles.sort();
        titles
    }

    fn password(&self, id: &str) -> String {
        self.vault
            .item_value(&human(), id, Field::Password, Access::Reveal)
            .unwrap()
            .expose()
            .to_owned()
    }
}

/// The export as written, and the file only once the export is.
fn sealed(vault: &Vault, project: Option<&str>, password: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    vault.export_sealed(&human(), project, password, &mut |bytes| {
        out = bytes.to_vec();
        Ok(())
    })?;
    Ok(out)
}

fn csv(vault: &Vault, project: Option<&str>) -> Result<String> {
    let mut out = String::new();
    vault.export_csv(&human(), project, &mut |bytes| {
        out = String::from_utf8(bytes.to_vec())?;
        Ok(())
    })?;
    Ok(out)
}

fn note(title: &str) -> ItemDraft {
    ItemDraft {
        kind: Kind::Note,
        password: None,
        totp: None,
        notes: Some(Secret::new("SSID acme-guest, hunter2-wifi".into())),
        ..login(None, title)
    }
}

fn with_password(draft: ItemDraft, id: &str, password: &str) -> ItemDraft {
    ItemDraft {
        id: Some(id.into()),
        password: Some(Secret::new(password.into())),
        ..draft
    }
}

#[test]
fn syncs_two_vaults_item_by_item_through_a_bare_remote() {
    let remote_env = Env::new("sync-remote");
    let (remote, git) = bare_remote(&remote_env.root);
    let (a, b) = (Machine::new("sync-a", &git), Machine::new("sync-b", &git));

    a.vault.init(&human(), PASSWORD, false).unwrap();
    let github = a
        .vault
        .save_item(&human(), login(Some("acme"), "GitHub"))
        .unwrap();
    let wifi = a.vault.save_item(&human(), note("Wi-Fi")).unwrap();
    let db = id("DATABASE_URL", Scope::Dev);
    a.vault
        .set(&human(), &db, Secret::new(VALUE.into()), None)
        .unwrap();
    a.vault.set_quick(&human(), Some(7)).unwrap();
    assert!(a.setup(&remote).pushed.contains(&VAULT_PATH.to_owned()));

    // B has no vault yet: it takes A's, which opens with A's master password and nothing kept here.
    assert!(b.setup(&remote).pulled.contains(&VAULT_PATH.to_owned()));
    let status = b.vault.status().unwrap();
    assert!(status.initialized && !status.unlocked && !status.remembered);
    assert_eq!(status.quick_days, None, "Touch ID stays on its own machine");
    assert!(!b.env.data().join("vault/quick.json").exists());
    assert!(!b.env.data().join("vault/incoming.json").exists());
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    assert_eq!(b.titles(), ["GitHub", "Wi-Fi"]);
    assert_eq!(
        b.vault.value(&human(), &db, Access::Use).unwrap().expose(),
        VALUE
    );

    // Added and deleted on B, changed on A, a secret changed on B: each side gets the other's.
    b.vault.save_item(&human(), login(None, "AWS")).unwrap();
    b.vault.delete_item(&human(), &wifi.id).unwrap();
    b.vault
        .set(&human(), &db, Secret::new("pg://changed-on-b".into()), None)
        .unwrap();
    let renamed = ItemDraft {
        id: Some(github.id.clone()),
        title: "GitHub (acme)".into(),
        ..login(Some("acme"), "GitHub")
    };
    a.vault.save_item(&human(), renamed).unwrap();
    b.sync();
    a.sync();
    assert_eq!(a.titles(), ["AWS", "GitHub (acme)"]);
    assert_eq!(
        a.vault.value(&human(), &db, Access::Use).unwrap().expose(),
        "pg://changed-on-b"
    );
    b.sync();
    assert_eq!(b.titles(), ["AWS", "GitHub (acme)"]);

    // Both change the same item: both versions are kept, and the user chooses.
    let stripe = a.vault.save_item(&human(), login(None, "Stripe")).unwrap();
    a.sync();
    b.sync();
    let from_a = with_password(login(None, "Stripe"), &stripe.id, "from-a-0123456789");
    let from_b = with_password(login(None, "Stripe"), &stripe.id, "from-b-0123456789");
    a.vault.save_item(&human(), from_a).unwrap();
    b.vault.save_item(&human(), from_b).unwrap();
    a.sync();
    b.sync();
    let item = b.vault.items(None).unwrap();
    let item = item.iter().find(|i| i.id == stripe.id).unwrap();
    assert_eq!(item.conflicts, 1);
    assert_eq!(b.password(&stripe.id), "from-b-0123456789");
    let others = b.vault.conflicts(&stripe.id).unwrap();
    assert_eq!(others[0].differs, ["password"]);
    let shown = serde_json::to_string(&others).unwrap();
    assert!(
        !shown.contains("from-a") && !shown.contains("hunter2"),
        "{shown}"
    );
    let masked = b.vault.masker().unwrap().mask("leaked from-a-0123456789");
    assert_eq!(masked.count, 1, "the other version's values are masked too");
    b.vault
        .resolve(&human(), &stripe.id, Some(&others[0].id))
        .unwrap();
    assert_eq!(b.password(&stripe.id), "from-a-0123456789");
    b.sync();
    a.sync();
    let items = a.vault.items(None).unwrap();
    assert!(items.iter().all(|i| i.conflicts == 0), "settled on A too");
    assert_eq!(a.password(&stripe.id), "from-a-0123456789");

    // Each machine keeps its own audit; both say they merged.
    let (on_a, on_b) = (a.env.actions(&a.vault), b.env.actions(&b.vault));
    assert!(on_a.contains(&Action::Merge) && on_b.contains(&Action::Merge));
    assert!(on_a.contains(&Action::Security) && !on_b.contains(&Action::Security));
    let detail = b.vault.audit(1000).unwrap();
    assert!(
        detail
            .iter()
            .any(|e| e.detail.as_deref() == Some("synced: 1 conflict")),
        "{detail:?}"
    );

    // A new master password on A: B locks, and only the new one opens it.
    a.env.clock.awake(60);
    a.vault.change_password(&human(), NEW_PASSWORD).unwrap();
    a.sync();
    b.sync();
    let status = b.vault.status().unwrap();
    assert!(!status.unlocked);
    assert_eq!(status.synced, Some(Synced::Secured));
    assert_eq!(
        err(b.vault.unlock(&human(), Some(PASSWORD), None, None)),
        "the master password changed on another machine: use the new one"
    );
    b.vault
        .unlock(&human(), Some(NEW_PASSWORD), None, None)
        .unwrap();
    assert_eq!(b.vault.status().unwrap().synced, None);
    assert_eq!(b.titles(), ["AWS", "GitHub (acme)", "Stripe"]);

    // A security key enrolled on A protects the vault on B too: the password alone opens nothing there.
    let yubikey = key();
    a.env.plug(&[&yubikey]);
    a.env.clock.awake(60);
    a.vault.add_key(&human(), "YubiKey", Some(PIN)).unwrap();
    a.sync();
    b.sync();
    assert!(!b.vault.status().unwrap().unlocked);
    assert!(
        b.vault
            .unlock(&human(), Some(NEW_PASSWORD), None, Some(PIN))
            .is_err()
    );
    assert!(!b.vault.status().unwrap().unlocked);
    b.env.plug(&[&yubikey]);
    b.vault
        .unlock(&human(), Some(NEW_PASSWORD), None, Some(PIN))
        .unwrap();
    assert_eq!(b.vault.status().unwrap().keys, 1);
    let file = fs::read_to_string(b.env.file()).unwrap();
    assert!(!file.contains("hunter2") && !file.contains("Stripe"));
}

#[test]
fn refuses_another_vault_and_says_how_to_join_it() {
    let remote_env = Env::new("join-remote");
    let (remote, git) = bare_remote(&remote_env.root);
    let (a, b) = (Machine::new("join-a", &git), Machine::new("join-b", &git));
    a.vault.init(&human(), PASSWORD, false).unwrap();
    a.vault.save_item(&human(), login(None, "GitHub")).unwrap();
    a.setup(&remote);
    b.vault.init(&human(), "b's own password", false).unwrap();
    b.vault.save_item(&human(), login(None, "Local")).unwrap();
    b.setup(&remote);

    // Another vault: never merged, never pushed over the other machines' one.
    assert_eq!(b.vault.status().unwrap().synced, Some(Synced::Other));
    b.sync();
    assert_eq!(b.titles(), ["Local"]);
    a.sync();
    assert_eq!(a.titles(), ["GitHub"]);

    // To join it: export this one, take the synced one (this one is set aside), then import the export.
    let export = sealed(&b.vault, None, EXPORT_PASSWORD).unwrap();
    let before = fs::read(b.env.file()).unwrap();
    let status = b.vault.join_synced(&human()).unwrap();
    assert!(!status.unlocked && status.synced.is_none());
    let aside = fs::read_dir(b.env.data().join("vault"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("vault.") && n != "vault.json")
        .unwrap();
    assert_eq!(
        fs::read(b.env.data().join("vault").join(aside)).unwrap(),
        before
    );
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    assert_eq!(b.titles(), ["GitHub"]);
    b.vault
        .import_sealed(&human(), &export, EXPORT_PASSWORD, false)
        .unwrap();
    b.sync();
    a.sync();
    assert_eq!(a.titles(), ["GitHub", "Local"]);
}

#[test]
fn exports_sealed_by_its_own_password_and_imports_as_a_merge() {
    let env = Env::new("export");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let github = vault
        .save_item(&human(), login(Some("acme"), "GitHub"))
        .unwrap();
    vault.save_item(&human(), note("Wi-Fi")).unwrap();
    let prod = id("STRIPE_KEY", Scope::Prod);
    vault
        .set(&human(), &prod, Secret::new(VALUE.into()), None)
        .unwrap();

    // The master password again first, as for changing how the vault opens.
    env.clock.awake(6 * 60);
    assert_eq!(
        err(sealed(&vault, None, EXPORT_PASSWORD)),
        "this change needs the master password again"
    );
    vault.reauth(&human(), PASSWORD).unwrap();
    assert!(err(sealed(&vault, None, "short")).contains("8 characters"));
    let all = sealed(&vault, None, EXPORT_PASSWORD).unwrap();
    let acme = sealed(&vault, Some("acme"), EXPORT_PASSWORD).unwrap();
    let text = String::from_utf8_lossy(&all);
    for clear in ["hunter2", "GitHub", "alex", VALUE, "STRIPE_KEY"] {
        assert!(!text.contains(clear), "{clear} in the export");
    }
    assert!(crate::is_export(&all) && !crate::is_export(b"{}"));
    let exports: Vec<Option<String>> = vault
        .audit(10)
        .unwrap()
        .into_iter()
        .filter(|e| e.action == Action::Export)
        .map(|e| e.detail)
        .collect();
    assert_eq!(
        exports,
        [
            Some("encrypted: 1 items, 0 secrets, project acme".into()),
            Some("encrypted: 2 items, 1 secrets".into())
        ]
    );

    let other = Env::new("import");
    let target = other.vault();
    target.init(&human(), "another password", false).unwrap();
    assert_eq!(
        err(target.preview_import(&all, "wrong password!", false)),
        "wrong export password"
    );
    let rows = target
        .preview_import(&acme, EXPORT_PASSWORD, false)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].title.as_str(), rows[0].change),
        ("GitHub", Change::Added)
    );
    let rows = target.preview_import(&all, EXPORT_PASSWORD, false).unwrap();
    assert!(rows.iter().all(|r| r.change == Change::Added) && rows.len() == 3);
    let shown = serde_json::to_string(&rows).unwrap();
    assert!(!shown.contains("hunter2") && !shown.contains(VALUE));

    // Writing a prod secret asks for the master password again, as by hand.
    other.clock.awake(6 * 60);
    assert_eq!(
        err(target.import_sealed(&human(), &all, EXPORT_PASSWORD, false)),
        "the prod scope needs the master password again"
    );
    target.reauth(&human(), "another password").unwrap();
    target
        .import_sealed(&human(), &all, EXPORT_PASSWORD, false)
        .unwrap();
    assert_eq!(target.items(None).unwrap().len(), 2);
    assert_eq!(
        target.value(&human(), &prod, Access::Use).unwrap().expose(),
        VALUE
    );
    let rows = target.preview_import(&all, EXPORT_PASSWORD, false).unwrap();
    assert!(
        rows.iter().all(|r| r.change == Change::Unchanged),
        "{rows:?}"
    );

    // An older export of this same vault does not bring back what was deleted since.
    let own = sealed(&target, None, EXPORT_PASSWORD).unwrap();
    target.delete_item(&human(), &github.id).unwrap();
    let rows = target
        .import_sealed(&human(), &own, EXPORT_PASSWORD, false)
        .unwrap();
    assert!(rows.iter().all(|r| r.title != "GitHub"));
    assert_eq!(target.items(None).unwrap().len(), 1);
    let imported = target.audit(1).unwrap().remove(0);
    assert_eq!(imported.action, Action::Import);
    assert_eq!(
        imported.detail.as_deref(),
        Some("TermF export: nothing changed")
    );
}

#[test]
fn exports_a_plain_csv_only_when_asked_and_reads_it_back() {
    let env = Env::new("csv");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    vault
        .save_item(&human(), login(Some("acme"), "GitHub, Inc."))
        .unwrap();
    vault
        .set(
            &human(),
            &id("DATABASE_URL", Scope::Dev),
            Secret::new(VALUE.into()),
            None,
        )
        .unwrap();
    env.clock.awake(6 * 60);
    assert!(err(csv(&vault, None)).contains("master password again"));
    vault.reauth(&human(), PASSWORD).unwrap();
    let text = csv(&vault, None).unwrap();
    assert!(text.starts_with("folder,favorite,type,name,notes,fields,reprompt,login_uri,"));
    assert!(text.contains("acme,,login,\"GitHub, Inc.\",") && text.contains("hunter2-github-0123"));
    assert!(text.contains(&format!("otpauth://totp/GitHub%2C%20Inc.?secret={SEED}")));
    assert!(!text.contains(VALUE), "secrets stay out of it");
    let entry = vault.audit(1).unwrap().remove(0);
    assert_eq!(
        (entry.action, entry.detail.as_deref()),
        (Action::Export, Some("plain CSV: 1 items, 0 secrets"))
    );

    let preview = vault.preview_foreign(text.as_bytes()).unwrap();
    assert_eq!(preview.source, "bitwarden-csv");
    let row = &preview.rows[0];
    assert_eq!(
        (
            row.title.as_str(),
            row.folder.as_str(),
            row.has_totp,
            row.problem
        ),
        ("GitHub, Inc.", "acme", true, None)
    );
    assert_eq!(row.duplicate.as_deref(), Some("GitHub, Inc."));
}

/// Commits on the bare remote's branch: each one is a sync that sent something.
fn commits(root: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root.join("remote.git"))
        .args(["rev-list", "--count", "--all"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn merges(machine: &Machine) -> usize {
    machine
        .env
        .actions(&machine.vault)
        .iter()
        .filter(|a| **a == Action::Merge)
        .count()
}

#[test]
fn two_machines_never_send_the_vault_to_each_other_for_nothing() {
    let remote_env = Env::new("quiet-remote");
    let (remote, git) = bare_remote(&remote_env.root);
    let (a, b) = (Machine::new("quiet-a", &git), Machine::new("quiet-b", &git));
    // The keychain keeps the key on A only: nothing of it goes into the synced file.
    a.vault.init(&human(), PASSWORD, true).unwrap();
    a.vault.save_item(&human(), login(None, "GitHub")).unwrap();
    a.setup(&remote);
    b.setup(&remote);
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    assert!(a.vault.status().unwrap().remembered && !b.vault.status().unwrap().remembered);
    assert!(
        !fs::read_to_string(a.env.file())
            .unwrap()
            .contains("remembered")
    );

    // A change on each side, both at once: a few rounds settle it, then nothing moves.
    a.vault.save_item(&human(), login(None, "AWS")).unwrap();
    b.vault.save_item(&human(), login(None, "Stripe")).unwrap();
    for _ in 0..3 {
        a.sync();
        b.sync();
    }
    assert_eq!(a.titles(), ["AWS", "GitHub", "Stripe"]);
    assert_eq!(b.titles(), a.titles());
    let (sent, merged) = (commits(&remote_env.root), (merges(&a), merges(&b)));
    for _ in 0..4 {
        a.sync();
        b.sync();
    }
    assert_eq!(
        commits(&remote_env.root),
        sent,
        "no sync sends anything new"
    );
    assert_eq!((merges(&a), merges(&b)), merged, "nor merges anything");
    assert_eq!(
        fs::read(a.env.file()).unwrap(),
        fs::read(b.env.file()).unwrap()
    );

    // Locking and opening again (the keychain on A, the password on B) sends nothing either.
    a.vault.lock(&human()).unwrap();
    a.vault.unlock(&human(), None, None, None).unwrap();
    b.vault.lock(&human()).unwrap();
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    a.sync();
    b.sync();
    assert_eq!(commits(&remote_env.root), sent);

    // Two versions of one item, unresolved, do not keep both machines busy either.
    let item = a.vault.items(None).unwrap();
    let github = item.iter().find(|i| i.title == "GitHub").unwrap();
    let from_a = with_password(login(None, "GitHub"), &github.id, "from-a-0123456789");
    let from_b = with_password(login(None, "GitHub"), &github.id, "from-b-0123456789");
    a.vault.save_item(&human(), from_a).unwrap();
    b.vault.save_item(&human(), from_b).unwrap();
    for _ in 0..3 {
        a.sync();
        b.sync();
    }
    let sent = commits(&remote_env.root);
    for _ in 0..3 {
        a.sync();
        b.sync();
    }
    assert_eq!(commits(&remote_env.root), sent);
    let conflicts = |m: &Machine| {
        m.vault
            .items(None)
            .unwrap()
            .iter()
            .map(|i| i.conflicts)
            .sum::<usize>()
    };
    assert_eq!((conflicts(&a), conflicts(&b)), (1, 1));
}

#[test]
fn deleting_an_item_with_two_versions_deletes_it_everywhere() {
    let remote_env = Env::new("bury-remote");
    let (remote, git) = bare_remote(&remote_env.root);
    let (a, b) = (Machine::new("bury-a", &git), Machine::new("bury-b", &git));
    a.vault.init(&human(), PASSWORD, false).unwrap();
    let github = a.vault.save_item(&human(), login(None, "GitHub")).unwrap();
    a.setup(&remote);
    b.setup(&remote);
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    let from_a = with_password(login(None, "GitHub"), &github.id, "from-a-0123456789");
    let from_b = with_password(login(None, "GitHub"), &github.id, "from-b-0123456789");
    a.vault.save_item(&human(), from_a).unwrap();
    b.vault.save_item(&human(), from_b).unwrap();
    a.sync();
    b.sync();
    a.sync();
    let has = |m: &Machine| {
        m.vault
            .items(None)
            .unwrap()
            .iter()
            .any(|i| i.id == github.id)
    };
    let shown = |m: &Machine| m.password(&github.id);
    // Each side shows its own version, the other beside it.
    assert_eq!(
        shown(&a),
        "from-b-0123456789",
        "A took B's bytes, the same two versions"
    );
    assert_eq!(b.vault.conflicts(&github.id).unwrap().len(), 1);

    a.vault.delete_item(&human(), &github.id).unwrap();
    for _ in 0..2 {
        a.sync();
        b.sync();
    }
    assert!(!has(&a) && !has(&b), "gone on both machines, both versions");
    assert!(b.vault.conflicts(&github.id).unwrap().is_empty());
}

#[test]
fn another_vaults_export_only_adds_and_never_replaces_nor_deletes() {
    let (here, there) = (Env::new("foreign-here"), Env::new("foreign-there"));
    let (mine, theirs) = (here.vault(), there.vault());
    mine.init(&human(), PASSWORD, false).unwrap();
    theirs.init(&human(), "their own password", false).unwrap();
    let db = id("DATABASE_URL", Scope::Dev);
    let token = id("TOKEN", Scope::Dev);
    mine.set(&human(), &db, Secret::new("pg://mine".into()), None)
        .unwrap();
    mine.set(&human(), &token, Secret::new("token-mine".into()), None)
        .unwrap();
    theirs
        .set(&human(), &db, Secret::new("pg://theirs".into()), None)
        .unwrap();
    // A tombstone there for a secret of the same name here.
    theirs
        .set(&human(), &token, Secret::new("token-theirs".into()), None)
        .unwrap();
    theirs.delete(&human(), &token).unwrap();
    theirs
        .save_item(&human(), login(None, "Theirs only"))
        .unwrap();
    let export = sealed(&theirs, None, EXPORT_PASSWORD).unwrap();

    let rows = mine
        .preview_import(&export, EXPORT_PASSWORD, false)
        .unwrap();
    let change = |title: &str| rows.iter().find(|r| r.title == title).map(|r| r.change);
    assert_eq!(change("DATABASE_URL"), Some(Change::Conflict));
    assert_eq!(change("Theirs only"), Some(Change::Added));
    assert_eq!(change("TOKEN"), None, "their deletion deletes nothing here");
    mine.import_sealed(&human(), &export, EXPORT_PASSWORD, false)
        .unwrap();
    let value = |id: &SecretId| {
        mine.value(&human(), id, Access::Use)
            .unwrap()
            .expose()
            .to_owned()
    };
    assert_eq!(
        (value(&db), value(&token)),
        ("pg://mine".into(), "token-mine".into())
    );
    let others = mine.conflicts("secret:dev::DATABASE_URL").unwrap();
    assert_eq!(others.len(), 1, "theirs kept beside ours");
    assert_eq!(others[0].differs, ["value"]);

    // The same export again: nothing new, the other version is not piled up.
    let rows = mine
        .import_sealed(&human(), &export, EXPORT_PASSWORD, false)
        .unwrap();
    assert!(rows.iter().all(|r| r.change != Change::Added));
    assert_eq!(mine.conflicts("secret:dev::DATABASE_URL").unwrap().len(), 1);
}

#[test]
fn a_synced_version_that_does_not_open_is_set_aside_and_the_vault_opens() {
    let remote_env = Env::new("damaged-remote");
    let (remote, git) = bare_remote(&remote_env.root);
    let (a, b) = (
        Machine::new("damaged-a", &git),
        Machine::new("damaged-b", &git),
    );
    a.vault.init(&human(), PASSWORD, false).unwrap();
    a.vault.save_item(&human(), login(None, "GitHub")).unwrap();
    a.setup(&remote);
    b.setup(&remote);
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    b.vault.save_item(&human(), login(None, "Local")).unwrap();
    b.vault.lock(&human()).unwrap();

    let incoming = b.env.data().join("vault/incoming.json");
    let aside = b.env.data().join("vault/incoming.damaged.json");
    // Its content sealed by no key this vault has, and said to be more secure than this machine's: the
    // password still opens this machine's own file.
    let mut file: serde_json::Value =
        serde_json::from_slice(&fs::read(a.env.file()).unwrap()).unwrap();
    file["data"] = "00".repeat(80).into();
    file["secured"] = 4_000_000_000u64.into();
    fs::write(&incoming, file.to_string()).unwrap();
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    assert_eq!(b.titles(), ["GitHub", "Local"]);
    assert!(!incoming.exists() && aside.exists());
    assert_eq!(b.vault.status().unwrap().synced, Some(Synced::Damaged));
    let entry = b.vault.audit(10).unwrap();
    assert!(
        entry.iter().any(|e| e
            .detail
            .as_deref()
            .is_some_and(|d| d.ends_with("set aside as vault/incoming.damaged.json"))),
        "{entry:?}"
    );

    // Not even JSON, while the vault is open: set aside too.
    fs::write(&incoming, "{ not json").unwrap();
    assert_eq!(b.vault.status().unwrap().synced, Some(Synced::Damaged));
    assert_eq!(b.vault.merge_synced(&human()).unwrap(), None);
    assert!(!incoming.exists());
    assert_eq!(fs::read_to_string(&aside).unwrap(), "{ not json");
    b.vault.lock(&human()).unwrap();
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    assert_eq!(b.titles(), ["GitHub", "Local"]);
}

#[test]
fn an_export_is_audited_once_written_only() {
    let env = Env::new("export-audit");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    let failed = vault.export_sealed(&human(), None, EXPORT_PASSWORD, &mut |_| {
        anyhow::bail!("disk full")
    });
    assert_eq!(err(failed), "disk full");
    let failed = vault.export_csv(&human(), None, &mut |_| anyhow::bail!("disk full"));
    assert!(failed.is_err());
    assert!(!env.actions(&vault).contains(&Action::Export));
    sealed(&vault, None, EXPORT_PASSWORD).unwrap();
    assert_eq!(env.actions(&vault).last(), Some(&Action::Export));
}

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/import")
        .join(name);
    fs::read(path).unwrap()
}

/// 1Password's `.1pux`: a ZIP of its `export.data` and `export.attributes`.
fn one_pux() -> Vec<u8> {
    let data = fixture("1password-export.data");
    let attributes = br#"{"version": 3, "description": "1Password Unencrypted Export", "timestamp": 1700000000}"#;
    crate::zip::tests::zip(&[("export.attributes", attributes), ("export.data", &data)])
}

#[test]
fn imports_other_managers_exports_after_a_preview_that_shows_no_value() {
    let env = Env::new("foreign");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    vault.save_item(&human(), login(None, "GitHub")).unwrap();

    let crlf = String::from_utf8(fixture("bitwarden.csv"))
        .unwrap()
        .replace('\n', "\r\n");
    let files: [(&str, Vec<u8>, &str, &[&str]); 10] = [
        (
            "bitwarden.json",
            fixture("bitwarden.json"),
            "bitwarden-json",
            &["GitHub", "AWS console", "Wi-Fi office"],
        ),
        (
            "bitwarden.csv",
            fixture("bitwarden.csv"),
            "bitwarden-csv",
            &["Stripe", "Door code"],
        ),
        (
            "bitwarden.csv (CRLF)",
            crlf.into_bytes(),
            "bitwarden-csv",
            &["Stripe", "Door code"],
        ),
        (
            "export.1pux",
            one_pux(),
            "1password-1pux",
            &["Vercel", "Router admin", "Server notes"],
        ),
        (
            "1password.csv",
            fixture("1password.csv"),
            "1password-csv",
            &["Netlify"],
        ),
        (
            "chrome.csv",
            fixture("chrome.csv"),
            "chrome-csv",
            &["github.com", "com.acme.app"],
        ),
        (
            "firefox.csv",
            fixture("firefox.csv"),
            "firefox-csv",
            &["figma.com"],
        ),
        (
            "safari.csv",
            fixture("safari.csv"),
            "safari-csv",
            &["linear.app (alex)"],
        ),
        (
            "keepassxc.csv",
            fixture("keepassxc.csv"),
            "keepassxc-csv",
            &["Sentry", "Old"],
        ),
        (
            "keepass.xml",
            fixture("keepass.xml"),
            "keepass-xml",
            &["Grafana", "Deleted"],
        ),
    ];
    for (name, bytes, source, titles) in &files {
        let preview = vault.preview_foreign(bytes).unwrap();
        assert_eq!(preview.source, *source, "{name}");
        let shown: Vec<&str> = preview.rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(shown, *titles, "{name}");
        let json = serde_json::to_string(&preview).unwrap();
        assert!(
            !json.contains("Hunter2") && !json.contains("JBSWY3DP"),
            "{name}: {json}"
        );
    }

    let bitwarden = vault.preview_foreign(&files[0].1).unwrap();
    assert_eq!(bitwarden.unsupported, 1, "the card");
    assert_eq!(bitwarden.folders, ["", "Acme"]);
    let rows = &bitwarden.rows;
    assert_eq!(
        rows[0].duplicate.as_deref(),
        Some("GitHub"),
        "same host and user"
    );
    assert_eq!(
        (rows[1].duplicate.as_deref(), rows[1].problem),
        (None, Some("totp"))
    );
    assert_eq!(rows[2].kind, Kind::Note);
    let choice = ForeignChoice {
        digest: bitwarden.digest.clone(),
        skip: vec![0],
        projects: [("Acme".to_owned(), Some("acme".to_owned()))].into(),
    };
    assert!(
        err(vault.import_foreign(&human(), &files[1].1, &choice))
            .contains("changed since its preview")
    );
    assert_eq!(
        vault
            .import_foreign(&human(), &files[0].1, &choice)
            .unwrap(),
        2
    );
    let items = vault.items(None).unwrap();
    let find = |title: &str| items.iter().find(|i| i.title == title).unwrap().clone();
    assert_eq!(items.len(), 3, "the duplicate left out");
    let (aws, wifi) = (find("AWS console"), find("Wi-Fi office"));
    assert_eq!((aws.project, wifi.project.as_deref()), (None, Some("acme")));
    assert!(aws.totp.is_none() && aws.has_notes);
    let notes = vault
        .item_value(&human(), &aws.id, Field::Notes, Access::Reveal)
        .unwrap();
    assert_eq!(notes.expose(), "TOTP: not a totp secret", "kept, not lost");
    let entry = vault.audit(2).unwrap().remove(1);
    assert_eq!(
        (entry.action, entry.detail.as_deref()),
        (Action::Import, Some("bitwarden-json: 2 items, 1 left out"))
    );

    // 1Password: the TOTP, the password of a password item, the other fields in the notes.
    let pux = vault.preview_foreign(&files[3].1).unwrap();
    assert_eq!(pux.unsupported, 1);
    let choice = ForeignChoice {
        digest: pux.digest.clone(),
        ..Default::default()
    };
    vault
        .import_foreign(&human(), &files[3].1, &choice)
        .unwrap();
    let items = vault.items(None).unwrap();
    let vercel = items.iter().find(|i| i.title == "Vercel").unwrap();
    assert!(vercel.totp.is_some());
    let notes = vault
        .item_value(&human(), &vercel.id, Field::Notes, Access::Reveal)
        .unwrap();
    assert_eq!(notes.expose(), "Team acme.\nrecovery email: alex@acme.test");
    let router = items.iter().find(|i| i.title == "Router admin").unwrap();
    assert_eq!(
        vault
            .item_value(&human(), &router.id, Field::Password, Access::Reveal)
            .unwrap()
            .expose(),
        "op-router-Hunter2!"
    );

    // KeePass: groups as folders, the trash flagged, history left out, custom fields in the notes.
    let keepass = vault.preview_foreign(&files[9].1).unwrap();
    let grafana = &keepass.rows[0];
    assert_eq!(
        (
            grafana.folder.as_str(),
            grafana.urls.len(),
            grafana.has_totp
        ),
        ("Acme & Co", 2, true)
    );
    assert_eq!(keepass.rows[1].problem, Some("trash"));
    let choice = ForeignChoice {
        digest: keepass.digest.clone(),
        skip: vec![1],
        ..Default::default()
    };
    vault
        .import_foreign(&human(), &files[9].1, &choice)
        .unwrap();
    let items = vault.items(None).unwrap();
    let grafana = items.iter().find(|i| i.title == "Grafana").unwrap();
    let reveal = |field| {
        vault
            .item_value(&human(), &grafana.id, field, Access::Reveal)
            .unwrap()
            .expose()
            .to_owned()
    };
    assert_eq!(reveal(Field::Password), "kp-grafana-Hunter2!<");
    assert_eq!(
        reveal(Field::Notes),
        "Line one\nLine two\nDatabase: kp-db-Hunter2!"
    );
    assert!(
        items
            .iter()
            .all(|i| i.title != "Deleted" && !i.title.contains("old"))
    );
    let keepassxc = vault.preview_foreign(&files[8].1).unwrap();
    assert_eq!(
        (keepassxc.rows[0].folder.as_str(), keepassxc.rows[1].problem),
        ("Acme", Some("trash"))
    );
}

#[test]
fn a_restore_brings_back_what_was_deleted_since_the_export_on_every_machine() {
    let remote_env = Env::new("restore-remote");
    let (remote, git) = bare_remote(&remote_env.root);
    let (a, b) = (
        Machine::new("restore-a", &git),
        Machine::new("restore-b", &git),
    );
    a.vault.init(&human(), PASSWORD, false).unwrap();
    let github = a.vault.save_item(&human(), login(None, "GitHub")).unwrap();
    a.vault.save_item(&human(), login(None, "Kept")).unwrap();
    let db = id("DATABASE_URL", Scope::Prod);
    a.vault
        .set(&human(), &db, Secret::new("pg://prod".into()), None)
        .unwrap();
    a.setup(&remote);
    b.setup(&remote);
    b.vault
        .unlock(&human(), Some(PASSWORD), None, None)
        .unwrap();
    let export = sealed(&a.vault, None, EXPORT_PASSWORD).unwrap();
    a.vault.delete_item(&human(), &github.id).unwrap();
    a.vault.delete(&human(), &db).unwrap();
    a.sync();
    b.sync();
    assert_eq!(b.titles(), ["Kept"]);

    let rows = a
        .vault
        .preview_import(&export, EXPORT_PASSWORD, false)
        .unwrap();
    assert!(
        rows.iter().all(|r| r.change == Change::Unchanged),
        "{rows:?}"
    );
    let rows = a
        .vault
        .preview_import(&export, EXPORT_PASSWORD, true)
        .unwrap();
    let added: Vec<&str> = rows
        .iter()
        .filter(|r| r.change == Change::Added)
        .map(|r| r.title.as_str())
        .collect();
    assert_eq!(added, ["GitHub", "DATABASE_URL"]);

    // A prod secret comes back only with the master password again, as by hand.
    a.env.clock.0.lock().unwrap().1 += Duration::from_secs(301);
    assert!(
        a.vault
            .import_sealed(&human(), &export, EXPORT_PASSWORD, true)
            .is_err()
    );
    a.vault.reauth(&human(), PASSWORD).unwrap();
    a.vault
        .import_sealed(&human(), &export, EXPORT_PASSWORD, true)
        .unwrap();
    let last = a.vault.audit(1).unwrap().remove(0);
    assert_eq!(last.action, Action::Restore);
    assert!(last.detail.unwrap().ends_with("; 2 restored"));
    assert_eq!(a.titles(), ["GitHub", "Kept"]);
    assert_eq!(a.password(&github.id), "hunter2-github-0123");

    for _ in 0..2 {
        a.sync();
        b.sync();
    }
    assert_eq!(
        b.titles(),
        ["GitHub", "Kept"],
        "the other machine's deletion does not take it again"
    );
    assert_eq!(
        b.vault.value(&human(), &db, Access::Use).unwrap().expose(),
        "pg://prod"
    );
}
