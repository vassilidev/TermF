//! `termf vault …`: the vault from a terminal. A command that reads or writes secrets asks for the master
//! password on the terminal (never in argv nor on stdin) and opens the vault file itself: it never gets a value
//! from TermF, whose local API gives none. `unlock`, `lock` and `status` act on TermF's own vault session.

use std::io::{IsTerminal, Read};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use termf_core::ipc::{self, Call, VaultCall};
use termf_sync::OsKeyStore;
use termf_vault::{
    Access, Actor, ActorKind, KEYCHAIN, Scope, Secret, SecretId, SystemClock, Vault,
};

use crate::{Client, take_flag, take_opt};

pub fn run(mut args: Vec<&str>) -> Result<()> {
    termf_vault::harden_process();
    let vault = Vault::new(
        &termf_core::default_data_dir()?,
        Box::new(OsKeyStore(KEYCHAIN)),
        Box::new(SystemClock::default()),
    )
    .with_path(std::env::var_os("PATH"));
    let actor = Actor {
        session: std::env::var(ipc::ENV_SESSION).ok(),
        ..Actor::human()
    };
    let scope = take_opt(&mut args, "--scope")?;
    let scope = scope.as_deref().map(Scope::parse).transpose()?;
    let project = take_opt(&mut args, "--project")?;
    let description = take_opt(&mut args, "--description")?;
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
            print_status(&client.call(Call::Vault(VaultCall::Unlock {
                password: password.expose().into(),
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
        ["audit", rest @ ..] => audit(&vault, rest.to_vec())?,
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

fn prompt(text: &str) -> Result<Secret> {
    Ok(Secret::new(rpassword::prompt_password(text)?))
}

/// This process's own session, which ends with it: the password typed now also opens prod.
fn open(vault: &Vault, actor: &Actor) -> Result<()> {
    let password = prompt("Master password: ")?;
    vault.unlock(actor, Some(password.expose()), None)?;
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
        let target = e.target.map(|name| {
            let project = e.project.as_deref().unwrap_or("global");
            let scope = e.scope.map_or("", Scope::as_str);
            format!("{name} ({scope}, {project})")
        });
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
