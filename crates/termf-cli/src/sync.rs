//! `termf sync …`: the encrypted sync from a terminal. Passwords are read from the terminal, never from argv.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use termf_sync::{
    Credentials, Engine, Locked, OsKeyStore, RemoteSpec, Report, Resolution, Scope, VAULT_PATH,
};

pub fn run(args: &[&str]) -> Result<()> {
    let mut engine = Engine::new(
        termf_core::default_config_dir()?,
        Box::new(OsKeyStore::SYNC),
    )
    .with_vault(termf_core::default_data_dir()?.join("vault"));
    let home = termf_core::expand_tilde("~");
    if home.is_absolute() {
        engine = engine.with_home(home);
    }
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
        ["forget", device] => {
            let devices = unlocked(&engine, |e| e.devices())?;
            let found: Vec<_> = devices
                .iter()
                .filter(|d| d.id == *device || d.name == *device)
                .collect();
            let [target] = found.as_slice() else {
                bail!("no single machine named {device}: see termf sync devices");
            };
            println!(
                "{} will no longer read what syncs next. Your other machines will ask for the password once.",
                target.name
            );
            let password = rpassword::prompt_password("Sync password: ")?;
            show_recovery_key(&engine.forget(&target.id, &password)?);
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
    let (mut remote, mut name, mut chosen) = (None, None, Scope::DEFAULT.to_vec());
    let (mut bucket, mut region, mut prefix) = (None, "us-east-1".to_owned(), String::new());
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
            ("--s3", Some(endpoint)) => {
                remote = Some(RemoteSpec::S3 {
                    endpoint: endpoint.to_string(),
                    bucket: String::new(),
                    region: String::new(),
                    prefix: String::new(),
                    credentials: None,
                })
            }
            ("--bucket", Some(text)) => bucket = Some(text.to_string()),
            ("--region", Some(text)) => region = text.to_string(),
            ("--prefix", Some(text)) => prefix = text.to_string(),
            ("--webdav", Some(url)) => {
                remote = Some(RemoteSpec::Webdav {
                    url: url.to_string(),
                    credentials: None,
                })
            }
            ("--name", Some(text)) => name = Some(text.to_string()),
            ("--scopes", Some(list)) => chosen = scopes(list)?,
            _ => bail!(crate::USAGE),
        }
    }
    let remote = match remote
        .context("give --git <url>, --folder <path>, --s3 <endpoint> or --webdav <url>")?
    {
        RemoteSpec::S3 { endpoint, .. } => {
            let user = prompt("Access key id: ")?;
            let secret = rpassword::prompt_password("Secret access key: ")?;
            RemoteSpec::S3 {
                endpoint,
                bucket: bucket.context("give the bucket with --bucket <name>")?,
                region,
                prefix,
                credentials: Some(Credentials { user, secret }),
            }
        }
        RemoteSpec::Webdav { url, .. } => {
            let user = prompt("User name (empty: none): ")?;
            let secret = if user.is_empty() {
                String::new()
            } else {
                rpassword::prompt_password("Password: ")?
            };
            RemoteSpec::Webdav {
                url,
                credentials: Some(Credentials { user, secret }),
            }
        }
        other => other,
    };
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

fn prompt(question: &str) -> Result<String> {
    print!("{question}");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().to_owned())
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

/// Each scope as the command line names it.
const SCOPES: [(Scope, &str); 6] = [
    (Scope::Config, "config"),
    (Scope::Projects, "projects"),
    (Scope::Conventions, "conventions"),
    (Scope::Vault, "vault"),
    (Scope::SshKeys, "ssh-keys"),
    (Scope::DevConfig, "dev-config"),
];

fn scopes(list: &str) -> Result<Vec<Scope>> {
    list.split(',')
        .map(|s| match s.trim() {
            "sshKeys" => Ok(Scope::SshKeys),
            "devConfig" => Ok(Scope::DevConfig),
            name => SCOPES
                .iter()
                .find(|(_, n)| *n == name)
                .map(|(scope, _)| *scope)
                .with_context(|| {
                    let names: Vec<&str> = SCOPES.iter().map(|(_, n)| *n).collect();
                    format!("unknown scope {name} ({})", names.join(", "))
                }),
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
        other @ RemoteSpec::S3 { .. } => println!("remote      s3 {}", other.label()),
        RemoteSpec::Webdav { url, .. } => println!("remote      webdav {url}"),
    }
    let scopes: Vec<&str> = SCOPES
        .iter()
        .filter(|(scope, _)| status.scopes.contains(scope))
        .map(|(_, name)| *name)
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
        if path == VAULT_PATH {
            println!(
                "            another machine's vault: merged item by item the next time the vault opens"
            );
        }
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
