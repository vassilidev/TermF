//! `termf vault …`: the vault from a terminal. A command that reads or writes secrets asks for the master
//! password on the terminal (never in argv nor on stdin) and opens the vault file itself: it never gets a value
//! from TermF, whose local API gives none. `unlock`, `lock` and `status` act on TermF's own vault session. With
//! security keys enrolled, a key is touched too, after its PIN (Windows asks for it in its own window).

use std::fs;
use std::io::{IsTerminal, Read, Write};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use termf_core::ipc::{self, Call, VaultCall};
use termf_sync::OsKeyStore;
use termf_vault::{
    Access, Actor, ActorKind, Change, Field, ForeignChoice, KEYCHAIN, Scope, Secret, SecretId,
    SystemClock, Vault, Zeroizing,
};

use crate::{Client, take_flag, take_opt};

pub fn run(mut args: Vec<&str>) -> Result<()> {
    termf_vault::harden_process();
    let vault = Vault::new(
        &termf_core::default_data_dir()?,
        Box::new(OsKeyStore(KEYCHAIN)),
        Box::new(SystemClock::default()),
    )
    .with_path(std::env::var_os("PATH"))
    .with_keys(termf_vault::fido::system());
    let actor = Actor {
        session: std::env::var(ipc::ENV_SESSION).ok(),
        ..Actor::human()
    };
    let scope = take_opt(&mut args, "--scope")?;
    let scope = scope.as_deref().map(Scope::parse).transpose()?;
    let project = take_opt(&mut args, "--project")?;
    let description = take_opt(&mut args, "--description")?;
    let out = take_opt(&mut args, "-o")?;
    let csv = take_flag(&mut args, "--csv");
    let id = |name: &str| SecretId::new(name, scope.unwrap_or(Scope::Dev), project.as_deref());
    match args.as_slice() {
        ["init"] => {
            let password = prompt("New master password (8 characters or more): ")?;
            ensure!(
                prompt("Same password again: ")? == password,
                "the two passwords differ"
            );
            vault.init(&actor, password.expose(), false)?;
            println!("The vault is ready. termf vault unlock opens it in TermF.");
        }
        ["unlock"] => {
            let mut client = app()?;
            let password = prompt("Master password: ")?;
            let pin = pin(&vault)?;
            print_status(&client.call(Call::Vault(VaultCall::Unlock {
                password: password.expose().into(),
                pin: pin.map(|p| p.expose().into()),
            }))?);
        }
        ["lock"] => print_status(&app()?.call(Call::Vault(VaultCall::Lock))?),
        ["status"] if vault.initialized() => match Client::connect(None) {
            Ok(mut client) => print_status(&client.call(Call::Vault(VaultCall::Status))?),
            Err(e) => println!(
                "TermF       not reachable ({e:#}); the other vault commands ask for the master password themselves"
            ),
        },
        ["status"] => println!("No vault yet: termf vault init creates one."),
        ["ls"] => {
            open(&vault, &actor)?;
            for s in vault.list(scope, project.as_deref())? {
                let what = s.reference.unwrap_or_default();
                println!(
                    "{:<8}  {:<12}  {:<32}  {what:<32}  {}",
                    s.scope.as_str(),
                    s.project.as_deref().unwrap_or("global"),
                    s.name,
                    s.description
                );
            }
        }
        ["set", name] => {
            let id = id(name)?;
            let value = if std::io::stdin().is_terminal() {
                prompt("Value: ")?
            } else {
                let mut text = String::new();
                std::io::stdin().read_to_string(&mut text)?;
                let text = Secret::new(text);
                let value = text.expose().strip_suffix('\n').unwrap_or(text.expose());
                Secret::new(value.strip_suffix('\r').unwrap_or(value).into())
            };
            open(&vault, &actor)?;
            vault.set(&actor, &id, value, description.as_deref())?;
            println!("Saved {id}.");
        }
        ["ref", name, uri] => {
            let id = id(name)?;
            termf_vault::check_reference(uri)?;
            open(&vault, &actor)?;
            vault.set_reference(&actor, &id, uri, description.as_deref())?;
            println!("Saved {id}: resolved by its manager each time it is used.");
        }
        ["reveal", name] => {
            let id = id(name)?;
            open(&vault, &actor)?;
            println!("{}", vault.value(&actor, &id, Access::Reveal)?.expose());
        }
        ["rm", name] => {
            let id = id(name)?;
            open(&vault, &actor)?;
            vault.delete(&actor, &id)?;
            println!("Deleted {id}.");
        }
        ["items"] => {
            open(&vault, &actor)?;
            for i in vault.items(project.as_deref())? {
                let kind = serde_json::to_value(i.kind)?;
                println!(
                    "{}  {:<5}  {:<12}  {:<32}  {}",
                    i.id,
                    kind.as_str().unwrap_or_default(),
                    i.project.as_deref().unwrap_or("global"),
                    i.title,
                    i.username
                );
            }
        }
        ["item", id, field @ ..] => {
            let field = match field {
                [] => Field::Password,
                [name] => serde_json::from_value(Value::from(*name))
                    .map_err(|_| anyhow::anyhow!("password, totp, seed or notes"))?,
                _ => bail!(crate::USAGE),
            };
            open(&vault, &actor)?;
            println!(
                "{}",
                vault
                    .item_value(&actor, id, field, Access::Reveal)?
                    .expose()
            );
        }
        ["audit", rest @ ..] => audit(&vault, rest.to_vec())?,
        ["export"] => {
            let out = out.context("give the file to write: -o <file>")?;
            open(&vault, &actor)?;
            if csv {
                export_csv(&vault, &actor, &out, project.as_deref())?;
            } else {
                let password =
                    prompt("Export password (8 characters or more, for this export only): ")?;
                ensure!(
                    prompt("Same password again: ")? == password,
                    "the two passwords differ"
                );
                export_to(&out, |write| {
                    vault.export_sealed(&actor, project.as_deref(), password.expose(), write)
                })?;
                println!(
                    "Wrote {out}, sealed by its export password: termf vault import reads it."
                );
            }
        }
        ["import", file] => import(&vault, &actor, file, project.as_deref())?,
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

/// A file only the user reads (Unix), written whole.
fn write_private(path: &str, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .with_context(|| format!("cannot write {path}"))?;
    #[cfg(unix)]
    file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    file.write_all(bytes)?;
    Ok(file.sync_all()?)
}

/// Writes an export, audited once written: a file left by an export that failed after is removed.
fn export_to(
    path: &str,
    export: impl FnOnce(termf_vault::ExportWrite) -> Result<()>,
) -> Result<()> {
    let mut written = false;
    let result = export(&mut |bytes| {
        write_private(path, bytes)?;
        written = true;
        Ok(())
    });
    if result.is_err() && written {
        let _ = fs::remove_file(path);
    }
    result
}

/// A yes typed on the terminal.
fn confirm(question: &str) -> Result<bool> {
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

fn export_csv(vault: &Vault, actor: &Actor, out: &str, project: Option<&str>) -> Result<()> {
    println!(
        "A plain CSV holds every password, TOTP secret and note of the logins in clear: anyone who reads the file has them."
    );
    println!(
        "A value that starts with =, +, -, @ gets an apostrophe first, so a spreadsheet never runs it as a formula."
    );
    print!("Type plain to write it anyway: ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    ensure!(answer.trim() == "plain", "nothing written");
    export_to(out, |write| vault.export_csv(actor, project, write))?;
    println!("Wrote {out} in clear: delete it as soon as it is imported elsewhere.");
    Ok(())
}

/// A TermF export (alone or in a config archive), merged; or another manager's, after its preview.
fn import(vault: &Vault, actor: &Actor, file: &str, project: Option<&str>) -> Result<()> {
    let mut bytes = Zeroizing::new(fs::read(file).with_context(|| format!("cannot read {file}"))?);
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let sealed = termf_library::archive::vault(Path::new(file))?;
        bytes = Zeroizing::new(sealed.context("this configuration export holds no vault")?);
    }
    open(vault, actor)?;
    if termf_vault::is_export(&bytes) {
        let password = prompt("Export password: ")?;
        let rows = vault.preview_import(&bytes, password.expose())?;
        for row in rows.iter().filter(|r| r.change != Change::Unchanged) {
            let change = serde_json::to_value(row.change)?;
            let what = row.scope.map_or("item", Scope::as_str);
            let project = row.project.as_deref().unwrap_or("global");
            println!(
                "{:<9}  {what:<7}  {project:<12}  {}",
                change.as_str().unwrap_or_default(),
                row.title
            );
        }
        println!("{}", termf_vault::summary(&rows));
        if confirm("Import?")? {
            let rows = vault.import_sealed(actor, &bytes, password.expose())?;
            println!("Imported: {}.", termf_vault::summary(&rows));
        }
        return Ok(());
    }
    let preview = vault.preview_foreign(&bytes)?;
    let mut skip = Vec::new();
    for row in &preview.rows {
        let note = match (&row.duplicate, row.problem) {
            (_, Some("trash")) => "in the trash: left out".to_owned(),
            (Some(title), _) if title.is_empty() => "same as a line above: left out".to_owned(),
            (Some(title), _) => format!("already here as {title}: left out"),
            (None, Some("totp")) => "TOTP kept in the notes".to_owned(),
            _ => String::new(),
        };
        if row.duplicate.is_some() || row.problem == Some("trash") {
            skip.push(row.index);
        }
        println!(
            "{:>4}  {:<32}  {:<24}  {:<16}  {note}",
            row.index, row.title, row.username, row.folder
        );
    }
    if preview.unsupported > 0 {
        println!(
            "{} element(s) TermF has no place for (cards, identities, documents) left out.",
            preview.unsupported
        );
    }
    let into = project.unwrap_or("global");
    if !confirm(&format!(
        "Import {} items from {} into {into}?",
        preview.rows.len() - skip.len(),
        preview.source
    ))? {
        return Ok(());
    }
    let projects = preview
        .folders
        .iter()
        .map(|f| (f.clone(), project.map(str::to_owned)))
        .collect();
    let choice = ForeignChoice {
        digest: preview.digest.clone(),
        skip,
        projects,
    };
    let count = vault.import_foreign(actor, &bytes, &choice)?;
    println!("Imported {count} items.");
    println!("{file} holds these passwords in clear.");
    if confirm(&format!("Delete {file} now?"))? {
        fs::remove_file(file)?;
        println!("Deleted.");
    } else {
        println!("Delete it yourself once you no longer need it.");
    }
    Ok(())
}

fn prompt(text: &str) -> Result<Secret> {
    Ok(Secret::new(rpassword::prompt_password(text)?))
}

/// With security keys enrolled: the key's PIN unless Windows asks for it, then a word to touch the key.
fn pin(vault: &Vault) -> Result<Option<Secret>> {
    if vault.status()?.keys == 0 {
        return Ok(None);
    }
    let pin = if vault.asks_pin() {
        Some(prompt("Security key PIN: ")?)
    } else {
        None
    };
    eprintln!("Touch your security key when it blinks.");
    Ok(pin)
}

/// This process's own session, which ends with it: the password typed now also opens prod.
fn open(vault: &Vault, actor: &Actor) -> Result<()> {
    let password = prompt("Master password: ")?;
    let pin = pin(vault)?;
    vault.unlock(
        actor,
        Some(password.expose()),
        None,
        pin.as_ref().map(Secret::expose),
    )?;
    Ok(())
}

/// TermF, before asking anything: its vault session is the one these commands act on.
fn app() -> Result<Client> {
    Client::connect(None).context(
        "TermF is not reachable; the other vault commands ask for the master password themselves",
    )
}

fn print_status(status: &Value) {
    let unlocked = status["unlocked"] == true;
    println!(
        "vault       {}",
        if unlocked {
            "unlocked in TermF"
        } else {
            "locked"
        }
    );
    if status["remembered"] == true {
        println!("keychain    keeps the key: TermF unlocks without the password");
    }
    if let Some(keys) = status["keys"].as_u64().filter(|k| *k > 0) {
        println!("2nd factor  {keys} security key(s): the password alone does not open the vault");
    }
    println!(
        "auto-lock   after {} min without use, and when the computer sleeps",
        status["autoLockMinutes"]
    );
    if unlocked {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let left = status["prodUntil"]
            .as_u64()
            .map(|until| until.saturating_sub(now).div_ceil(60));
        match left {
            Some(min) => println!("prod        no password asked for {min} more min"),
            None => println!("prod        asks for the master password again"),
        }
    }
}

fn audit(vault: &Vault, mut args: Vec<&str>) -> Result<()> {
    let limit = take_opt(&mut args, "--limit")?;
    if take_flag(&mut args, "--jsonl") {
        vault.export(&mut std::io::stdout().lock())?;
        return Ok(());
    }
    if args == ["verify"] {
        let check = vault.verify()?;
        ensure!(
            check.ok,
            "entry {} does not hold: {} (checked {} before it)",
            check.broken.unwrap_or_default(),
            check.error.unwrap_or_default(),
            check.entries
        );
        println!(
            "{} entries, chained: nothing was changed, removed or inserted.",
            check.entries
        );
        return Ok(());
    }
    ensure!(args.is_empty(), crate::USAGE);
    let limit = limit.map(|l| l.parse()).transpose()?.unwrap_or(50);
    for e in vault.audit(limit)?.into_iter().rev() {
        let who = match (e.actor.kind, e.actor.run) {
            (ActorKind::Agent, Some(run)) => format!("agent, run {run}"),
            (kind, _) => format!("{kind:?}").to_lowercase(),
        };
        let target = e
            .target
            .map(|name| {
                let project = e.project.as_deref().unwrap_or("global");
                let scope = e.scope.map_or("", Scope::as_str);
                format!("{name} ({scope}, {project})")
            })
            .or(e.item.map(|id| format!("item {id}")));
        let action = serde_json::to_value(e.action)?;
        println!(
            "{:>5}  {}  {who:<14}  {:<13}  {}  {}",
            e.id,
            termf_tickets::utc(e.at as i64),
            action.as_str().unwrap_or_default(),
            target.unwrap_or_default(),
            e.detail.unwrap_or_default()
        );
    }
    Ok(())
}
