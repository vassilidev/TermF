//! `termf ssh …`: the SSH key manager from a terminal.

use std::path::PathBuf;

use anyhow::{Result, bail, ensure};
use termf_ssh::{KeyType, Ssh};

pub fn run(args: &[&str]) -> Result<()> {
    let ssh = Ssh::current()?;
    match args {
        ["ls"] => ls(&ssh)?,
        ["gen", name, options @ ..] => generate(&ssh, name, options)?,
        ["test", host] => {
            let test = termf_ssh::test_host(host)?;
            println!("{}", test.message);
            ensure!(test.ok, "{host} did not accept the connection");
        }
        // Without askpass, ssh-add asks the passphrase on this terminal.
        ["agent-add", key] => ssh.agent_add(key, None, None)?,
        ["agent-rm", key] => ssh.agent_remove(key)?,
        ["assign", key, repo @ ..] => {
            let repo = match repo {
                [] => std::env::current_dir()?,
                [repo] => PathBuf::from(repo),
                _ => bail!(crate::USAGE),
            };
            ssh.assign(&repo, (*key != "--none").then_some(*key))?;
        }
        ["trash", key] => println!("moved to {}", ssh.trash(key)?.display()),
        _ => bail!(crate::USAGE),
    }
    Ok(())
}

fn generate(ssh: &Ssh, name: &str, options: &[&str]) -> Result<()> {
    let (mut kind, mut comment) = (KeyType::Ed25519, String::new());
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match (*option, options.next()) {
            ("--type", Some(&"ed25519")) => kind = KeyType::Ed25519,
            ("--type", Some(&"ecdsa")) => kind = KeyType::Ecdsa,
            ("--type", Some(&"rsa")) => kind = KeyType::Rsa,
            ("--comment", Some(text)) => comment = text.to_string(),
            _ => bail!(crate::USAGE),
        }
    }
    let passphrase = rpassword::prompt_password("Passphrase (empty for none): ")?;
    if !passphrase.is_empty() {
        ensure!(
            rpassword::prompt_password("Same passphrase again: ")? == passphrase,
            "the passphrases differ"
        );
    }
    let key = ssh.generate(name, kind, &comment, Some(&passphrase))?;
    println!("created {} and {0}.pub", key.display());
    Ok(())
}

fn ls(ssh: &Ssh) -> Result<()> {
    let config_dir = termf_core::default_config_dir()?;
    let settings = termf_core::load_settings(&config_dir)?;
    let projects = termf_core::load_projects(&config_dir)?;
    let repos: Vec<PathBuf> = termf_core::discover_repos(&settings.roots, settings.scan_depth)
        .repos
        .iter()
        .chain(projects.iter().flat_map(|p| &p.repos))
        .map(|r| PathBuf::from(&r.path))
        .collect();
    let inventory = ssh.inventory(&repos);

    println!("keys in {}", inventory.dir);
    for key in &inventory.keys {
        let mut flags = Vec::new();
        if key.orphan {
            flags.push("no private key");
        }
        if key.encrypted == Some(true) {
            flags.push("passphrase");
        }
        if key.in_agent {
            flags.push("in agent");
        }
        if key.too_open {
            flags.push("TOO OPEN");
        }
        let bits = key.bits.map(|b| b.to_string()).unwrap_or_default();
        println!(
            "  {:<20} {:<20} {:>4}  {}  {}  [{}]",
            key.name,
            key.algorithm,
            bits,
            key.fingerprint.as_deref().unwrap_or("-"),
            key.comment,
            flags.join(", ")
        );
        for (label, list) in [("hosts", &key.hosts), ("repos", &key.repos)] {
            if !list.is_empty() {
                println!("      {label}: {}", list.join(", "));
            }
        }
    }
    for problem in &inventory.problems {
        println!("  ! {}: {}", problem.path, problem.error);
    }

    println!("agent");
    if let Some(error) = &inventory.agent.error {
        println!("  {error}");
    }
    for key in &inventory.agent.keys {
        println!("  {}  {}  {}", key.kind, key.fingerprint, key.comment);
    }

    println!("hosts");
    for host in &inventory.hosts {
        let target = [host.user.as_deref(), host.host_name.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("@");
        println!(
            "  {:<24} {:<30} {}",
            host.patterns.join(" "),
            target,
            host.identity_files.join(", ")
        );
    }
    Ok(())
}
