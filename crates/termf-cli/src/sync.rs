//! `termf sync …`: the encrypted sync from a terminal. Passwords are read from the terminal, never from argv.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use termf_sync::{Engine, Locked, OsKeyStore, RemoteSpec, Report, Resolution, Scope};

pub fn run(args: &[&str]) -> Result<()> {
    let engine = Engine::new(termf_core::default_config_dir()?, Box::new(OsKeyStore));
    match args {
        ["setup", options @ ..] => setup(&engine, options)?,
        ["status"] => status(&engine)?,
        ["push"] => print(&unlocked(&engine, |e| e.sync(true))?),
        ["pull"] => print(&unlocked(&engine, |e| e.sync(false))?),
        ["resolve", path, keep @ ("--local" | "--remote")] => {
            let resolution = if *keep == "--local" {
                Resolution::Local
            } else {
                Resolution::Remote
            };
            print(&unlocked(&engine, |e| e.resolve(path, resolution.clone()))?)
        }
        ["devices"] => {
            for d in unlocked(&engine, |e| e.devices())? {
                let current = if d.current { "  (this machine)" } else { "" };
                println!("{:<24}  {:<8}  {}{current}", d.name, d.os, ago(d.last_sync));
            }
        }
        ["change-password"] => {
            let current = rpassword::prompt_password("Current sync password: ")?;
            let password = new_password()?;
            show_recovery_key(&engine.change_password(&current, &password)?);
            println!("Other machines will ask for the new password.");
        }
        ["recover"] => {
            let key = rpassword::prompt_password("Recovery key: ")?;
            let password = new_password()?;
            show_recovery_key(&engine.recover(&key, &password)?);
        }
        ["scopes", list] => {
            engine.set_scopes(&scopes(list)?)?;
            status(&engine)?;
        }
        ["disable"] => {
            engine.disable()?;
            println!("Sync is off on this machine. Your config and the remote are unchanged.");
        }
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

fn setup(engine: &Engine, options: &[&str]) -> Result<()> {
    let (mut remote, mut name, mut chosen) = (None, None, Scope::AVAILABLE.to_vec());
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match (*option, options.next()) {
            ("--git", Some(url)) => {
                remote = Some(RemoteSpec::Git {
                    url: url.to_string(),
                })
            }
            ("--folder", Some(path)) => {
                let path = std::path::absolute(path)?.to_string_lossy().into_owned();
                remote = Some(RemoteSpec::Folder { path })
            }
            ("--name", Some(text)) => name = Some(text.to_string()),
            ("--scopes", Some(list)) => chosen = scopes(list)?,
            _ => bail!(crate::USAGE),
        }
    }
    let remote = remote.context("give --git <url> or --folder <path>")?;
    let joining = engine.probe(&remote)?;
    let password = if joining {
        println!("This remote already holds a TermF sync: joining it.");
        rpassword::prompt_password("Sync password: ")?
    } else {
        new_password()?
    };
    let setup = engine.setup(remote, &password, &chosen, name.as_deref())?;
    if let Some(key) = &setup.recovery_key {
        show_recovery_key(key);
    }
    print(&setup.report);
    if let Some(error) = setup.error {
        bail!("sync is set up, but the first sync failed: {error}");
    }
    Ok(())
}

fn new_password() -> Result<String> {
    let password = rpassword::prompt_password("New sync password (8 characters or more): ")?;
    ensure!(
        rpassword::prompt_password("Same password again: ")? == password,
        "the passwords differ"
    );
    Ok(password)
}

fn show_recovery_key(key: &str) {
    println!(
        "\nRecovery key, shown only now: keep it somewhere safe (printed, in a password manager)."
    );
    println!("Without it, a forgotten password makes the synced data unreadable.\n\n    {key}\n");
}

/// Asks the password when this machine needs it (changed elsewhere, no keychain), then tries again.
fn unlocked<T>(engine: &Engine, f: impl Fn(&Engine) -> Result<T>) -> Result<T> {
    match f(engine) {
        Err(e) if e.downcast_ref::<Locked>().is_some() => {
            engine.unlock(&rpassword::prompt_password("Sync password: ")?)?;
            f(engine)
        }
        other => other,
    }
}

fn scopes(list: &str) -> Result<Vec<Scope>> {
    list.split(',')
        .map(|s| match s.trim() {
            "config" => Ok(Scope::Config),
            "projects" => Ok(Scope::Projects),
            "conventions" => Ok(Scope::Conventions),
            other => bail!("unknown scope {other} (config, projects, conventions)"),
        })
        .collect()
}

fn status(engine: &Engine) -> Result<()> {
    let status = engine.status()?;
    let Some(remote) = status.remote else {
        println!("Sync is not set up on this machine (termf sync setup).");
        return Ok(());
    };
    match remote {
        RemoteSpec::Git { url } => println!("remote      git {url}"),
        RemoteSpec::Folder { path } => println!("remote      folder {path}"),
    }
    let scopes: Vec<String> = status
        .scopes
        .iter()
        .map(|s| format!("{s:?}").to_lowercase())
        .collect();
    println!("syncs       {}", scopes.join(", "));
    println!("device      {}", status.device_name.unwrap_or_default());
    let key = match (status.unlocked, status.remembered) {
        (false, _) => "password needed (termf sync push asks for it)",
        (true, true) => "in the system keychain",
        (true, false) => "in memory only: no system keychain",
    };
    println!("key         {key}");
    println!(
        "last sync   {}",
        status.last_sync.map_or("never".into(), ago)
    );
    if let Some(error) = status.last_error {
        println!("last error  {error}");
    }
    print(&Report {
        conflicts: status.conflicts,
        missing_repos: status.missing_repos,
        ..Default::default()
    });
    Ok(())
}

fn print(report: &Report) {
    for path in &report.pulled {
        println!("pulled      {path}");
    }
    for path in &report.pushed {
        println!("pushed      {path}");
    }
    for c in &report.conflicts {
        let keys = if c.keys.is_empty() {
            String::new()
        } else {
            format!(" ({})", c.keys.join(", "))
        };
        println!(
            "conflict    {}{keys}: termf sync resolve {} --local|--remote",
            c.path, c.path
        );
    }
    for repo in &report.missing_repos {
        let clone = repo
            .remote
            .as_deref()
            .map(|r| format!(": git clone {r} {}", repo.path));
        println!(
            "missing     {} in {}{}",
            repo.path,
            repo.project,
            clone.unwrap_or_default()
        );
    }
    for s in &report.skipped {
        println!("not synced  {}: {}", s.path, s.error);
    }
}

pub(crate) fn ago(secs: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match now.saturating_sub(secs) {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86400 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86400),
    }
}
