//! `termf env scan | list | add | remove | script | install`: the machine's development environment, `env.toml`.
//! Works on the config files directly: TermF does not need to run, so a new machine installs it with one command.

use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use termf_core::devenv::{self, Entry, Event, Group, Key, Machine, Manager};

fn parse_manager(text: &str) -> Result<Manager> {
    serde_json::from_value(Value::String(text.into()))
        .with_context(|| format!("unknown manager {text}"))
}

fn word(value: serde_json::Result<Value>) -> String {
    value
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn line(e: &Entry) -> String {
    let mut text = format!("{:<9} {}", word(serde_json::to_value(e.manager)), e.name);
    if let Some(v) = &e.version {
        text.push_str(&format!(" {v}"));
    }
    if !e.active {
        text.push_str("  [off]");
    }
    if let Some(d) = &e.description {
        text.push_str(&format!("  ({d})"));
    }
    text
}

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let config_dir = termf_core::default_config_dir()?;
    match args.as_slice() {
        ["scan", ..] => {
            let save = crate::take_flag(&mut args, "--save");
            if args.len() != 1 {
                bail!(crate::USAGE);
            }
            let manifest = devenv::load(&config_dir)?;
            let found = devenv::scan(&Machine::current(), &manifest, &|e| {
                if let Event::Scanned(s) = e
                    && let Some(why) = &s.error
                {
                    eprintln!("{}: {why}", s.source);
                }
            });
            let listed = |k: &Key| manifest.iter().any(|e| &e.key() == k);
            let mut new = Vec::new();
            for f in &found {
                let mark = if listed(&f.entry.key()) { "=" } else { "+" };
                let installed = f
                    .installed
                    .as_deref()
                    .map(|v| format!("  [{v}]"))
                    .unwrap_or_default();
                println!("{mark} {}{installed}", line(&f.entry));
                if !listed(&f.entry.key()) && !new.iter().any(|n: &Entry| n.key() == f.entry.key())
                {
                    new.push(f.entry.clone());
                }
            }
            let missing: Vec<&Entry> = manifest
                .iter()
                .filter(|e| e.active && !found.iter().any(|f| f.entry.key() == e.key()))
                .collect();
            for e in &missing {
                println!("- {}  [missing here]", line(e));
            }
            if save && !new.is_empty() {
                devenv::add(&config_dir, new.clone())?;
                println!(
                    "{} added to {}",
                    new.len(),
                    config_dir.join(devenv::FILE).display()
                );
            } else {
                println!(
                    "{} new (+), {} missing here (-): `termf env scan --save` adds the new ones",
                    new.len(),
                    missing.len()
                );
            }
        }
        ["list"] => {
            let entries = devenv::load(&config_dir)?;
            if entries.is_empty() {
                println!(
                    "{} lists nothing yet: `termf env scan --save` starts it",
                    devenv::FILE
                );
            }
            for group in Group::ALL {
                let list: Vec<&Entry> = entries.iter().filter(|e| e.group == group).collect();
                if !list.is_empty() {
                    println!("{}:", word(serde_json::to_value(group)));
                    for e in list {
                        println!("  {}", line(e));
                    }
                }
            }
            for name in devenv::scripts(&config_dir) {
                println!("script {name}");
            }
        }
        ["add", manager, name, ..] => {
            let (manager, name) = (parse_manager(manager)?, name.to_string());
            args.drain(..3);
            let mut entry = Entry::new(manager, &name);
            if let Some(g) = crate::take_opt(&mut args, "--group")? {
                entry.group = serde_json::from_value(Value::String(g.clone()))
                    .with_context(|| format!("unknown group {g}"))?;
            }
            entry.version = crate::take_opt(&mut args, "--version")?;
            entry.id = crate::take_opt(&mut args, "--id")?;
            entry.description = crate::take_opt(&mut args, "--description")?;
            entry.install = crate::take_opt(&mut args, "--install")?;
            entry.path = crate::take_opt(&mut args, "--path")?;
            entry.active = !crate::take_flag(&mut args, "--off");
            if !args.is_empty() {
                bail!(crate::USAGE);
            }
            devenv::add(&config_dir, vec![entry])?;
            println!("{name} is in {}", config_dir.join(devenv::FILE).display());
        }
        ["remove", manager, name] => {
            let key = Key {
                manager: parse_manager(manager)?,
                name: name.to_string(),
            };
            devenv::remove(&config_dir, &[key])?;
            println!("{name} removed");
        }
        ["script"] => print!("{}", devenv::script(&devenv::load(&config_dir)?)),
        ["script", name] => print!("{}", devenv::read_script(&config_dir, name)?),
        ["install", ..] => {
            let yes = crate::take_flag(&mut args, "--yes");
            let text = match args.as_slice() {
                ["install"] => devenv::script(&devenv::load(&config_dir)?),
                ["install", name] => devenv::read_script(&config_dir, name)?,
                _ => bail!(crate::USAGE),
            };
            if cfg!(windows) {
                bail!("the install script is for macOS and Linux");
            }
            print!("{text}");
            if !yes && !crate::tickets::confirmed("\nRun this script now? [y/N] ")? {
                bail!("nothing was run");
            }
            let dir = termf_core::process::scratch_dir()?;
            let file = dir.join("install.sh");
            std::fs::write(&file, &text)?;
            let status = Command::new("sh").arg(&file).status();
            let _ = std::fs::remove_dir_all(&dir);
            if !status?.success() {
                bail!("the install script stopped on an error (above)");
            }
        }
        _ => bail!(crate::USAGE),
    }
    Ok(())
}
