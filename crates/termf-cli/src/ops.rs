//! `termf ops …`: Ops without an agent (ARCHITECTURE §12.4, manual mode). It works on the project's files and
//! the vault directly, as `termf vault` does: the master password is asked on the terminal when there is a vault
//! (its values are masked in every output, and a recipe that writes in prod needs it), never through TermF.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use anyhow::{Context, Result, bail};
use termf_core::ipc;
use termf_ops::{Mode, Ops, Runner};
use termf_sync::OsKeyStore;
use termf_vault::{Access, Actor, KEYCHAIN, Secret, SecretId, SystemClock, Vault};

use crate::{USAGE, take_opt};

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let project = crate::memory::or_here(take_opt(&mut args, "--project")?)?;
    let connection = take_opt(&mut args, "--connection")?;
    let config_dir = termf_core::default_config_dir()?;
    let data_dir = termf_core::default_data_dir()?;
    let vault = Vault::new(
        &data_dir,
        Box::new(OsKeyStore(KEYCHAIN)),
        Box::new(SystemClock::default()),
    )
    .with_path(std::env::var_os("PATH"))
    .with_keys(termf_vault::fido::system());
    let actor = Actor {
        session: std::env::var(ipc::ENV_SESSION).ok(),
        ..Actor::human()
    };
    let runner = Runner::new(std::env::var_os("PATH"));
    let open = |vault: &Vault| -> Result<()> {
        if vault.initialized() {
            crate::vault::open(vault, &actor)?;
        }
        Ok(())
    };
    match args.as_slice() {
        ["connections"] => {
            let all = termf_ops::connections(&config_dir, &project)?;
            if all.is_empty() {
                println!(
                    "No connection in {project} yet: add one in TermF's Ops view, or as [[ops.connections]] in its file."
                );
            }
            for c in &all {
                println!(
                    "{:<20}  {:<10}  {:<7}  ai: {:<8}  {}",
                    c.name,
                    format!("{:?}", c.kind).to_lowercase(),
                    c.env.as_str(),
                    format!("{:?}", c.ai_access).to_lowercase(),
                    c.target(&all)
                );
            }
        }
        ["recipes"] => {
            let (recipes, errors) = termf_ops::recipes(&config_dir, Some(&project));
            for r in &recipes {
                let mode = if r.mode == Mode::Write {
                    "writes"
                } else {
                    "reads"
                };
                println!("{:<20}  {mode:<6}  {}", r.name, r.description);
                for p in &r.params {
                    let default = p
                        .default
                        .as_deref()
                        .map(|d| format!(" (default {d})"))
                        .unwrap_or_default();
                    println!("    --{:<14} {}{default}", p.name, p.description);
                }
            }
            for e in errors {
                eprintln!("termf: {e}");
            }
        }
        ["test", name] => {
            let all = termf_ops::connections(&config_dir, &project)?;
            let conn = all
                .iter()
                .find(|c| c.name == *name)
                .with_context(|| format!("no connection {name} in {project}"))?;
            if !conn.secrets.is_empty() {
                open(&vault)?;
            }
            let secret = |n: &str| -> Result<Secret> {
                let mine = SecretId::new(n, conn.env, Some(&project))?;
                vault.value(&actor, &mine, Access::Use).or_else(|_| {
                    vault.value(&actor, &SecretId::new(n, conn.env, None)?, Access::Use)
                })
            };
            for c in runner.test(conn, &all, &secret)? {
                let mark = match (c.ok, c.warn) {
                    (_, true) => "warn",
                    (true, _) => "ok",
                    (false, _) => "FAIL",
                };
                println!("{mark:<4}  {:<8}  {}  {}", c.code, c.subject, c.detail);
            }
        }
        ["run", recipe, rest @ ..] => {
            let connection = connection.context("say on which connection: --connection <name>")?;
            let mut params = BTreeMap::new();
            let mut rest = rest.iter();
            while let Some(flag) = rest.next() {
                let Some(name) = flag.strip_prefix("--") else {
                    bail!("{flag}: parameters go as --<name> <value>\n{USAGE}")
                };
                let value = rest
                    .next()
                    .with_context(|| format!("{flag} needs a value"))?;
                params.insert(name.to_owned(), value.to_string());
            }
            let ops = Ops {
                config_dir: &config_dir,
                data_dir: &data_dir,
                vault: &vault,
                runner: &runner,
            };
            let plan = ops.plan(&project, recipe, &connection, &params)?;
            eprintln!(
                "{} on {} ({}): {}",
                plan.recipe,
                plan.target,
                plan.env.as_str(),
                plan.command
            );
            let mut confirm = None;
            if plan.mode == Mode::Write {
                eprint!(
                    "{} writes. Type the environment's name ({}) to run it: ",
                    plan.recipe,
                    plan.env.as_str()
                );
                std::io::stderr().flush()?;
                let mut typed = String::new();
                std::io::stdin().lock().read_line(&mut typed)?;
                confirm = Some(typed.trim().to_owned());
            }
            open(&vault)?;
            let (_, out) = ops.run(
                &project,
                &actor,
                recipe,
                &connection,
                &params,
                confirm.as_deref(),
            )?;
            print!("{}", out.text);
            if !out.errors.trim().is_empty() {
                eprintln!("{}", out.errors.trim_end());
            }
            let mut notes = vec![format!("{} masked", out.masks)];
            if out.truncated {
                notes.push(format!(
                    "cut at {} lines or {} bytes",
                    plan.limits.lines, plan.limits.bytes
                ));
            }
            if let Some(stopped) = &out.stopped {
                notes.push(stopped.clone());
            }
            eprintln!(
                "termf: exit {}, {}",
                out.exit.map_or("-".into(), |c| c.to_string()),
                notes.join(", ")
            );
            if out.exit != Some(0) {
                bail!("{} did not succeed", plan.recipe);
            }
        }
        _ => bail!(USAGE),
    }
    Ok(())
}
