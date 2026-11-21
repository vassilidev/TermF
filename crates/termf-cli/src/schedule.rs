//! `termf schedule …`: scheduled tasks. Tasks and their history are read and written directly; running one goes
//! through TermF when it is open (it may open a tile), else runs here, headless, and ends with a notification.

use std::io::Read;

use anyhow::{Context, Result, bail};
use termf_core::ipc::Call;
use termf_schedule::{Execution, Host, Mode, Scheduler, Status, Task, Trigger, local};

use crate::{Client, take_flag, take_opt};

fn scheduler() -> Result<Scheduler> {
    Ok(Scheduler::new(
        termf_core::default_config_dir()?,
        termf_core::default_data_dir()?,
    ))
}

/// The options shared by `add` and `edit`, applied to `task`.
fn apply(task: &mut Task, args: &mut Vec<&str>) -> Result<()> {
    if let Some(name) = take_opt(args, "--name")? {
        task.name = name;
    }
    if let Some(cron) = take_opt(args, "--cron")? {
        task.cron = cron;
    }
    if let Some(dir) = take_opt(args, "--dir")? {
        task.dir = std::path::absolute(dir)?.to_string_lossy().into_owned();
    }
    if let Some(prompt) = take_opt(args, "--prompt")? {
        task.prompt = if prompt == "-" {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            text
        } else {
            prompt
        };
    }
    for (flag, value) in [
        ("--model", &mut task.model),
        ("--effort", &mut task.effort),
        ("--permission-mode", &mut task.permissions),
    ] {
        if let Some(v) = take_opt(args, flag)? {
            *value = Some(v);
        }
    }
    if let Some(budget) = take_opt(args, "--budget")? {
        task.budget = match budget.as_str() {
            "" | "none" => None,
            b => Some(b.parse().context("--budget is an amount in US dollars")?),
        };
    }
    for (flag, mode) in [("--tile", Mode::Tile), ("--headless", Mode::Headless)] {
        if take_flag(args, flag) {
            task.mode = mode;
        }
    }
    for (flag, on) in [("--enable", true), ("--disable", false)] {
        if take_flag(args, flag) {
            task.enabled = on;
        }
    }
    for (flag, on) in [("--catchup", true), ("--no-catchup", false)] {
        if take_flag(args, flag) {
            task.catchup = on;
        }
    }
    if let Some(flag) = args.iter().find(|a| a.starts_with('-')) {
        bail!("unknown option {flag}\n{}", crate::USAGE);
    }
    Ok(())
}

fn termf() -> Result<std::path::PathBuf> {
    std::env::current_exe().context("cannot find the termf program")
}

pub fn run(mut args: Vec<&str>) -> Result<()> {
    let s = scheduler()?;
    match args.first().copied() {
        Some("add") => {
            args.remove(0);
            let name = match args.first() {
                Some(name) if !name.starts_with('-') => args.remove(0).to_owned(),
                _ => bail!(crate::USAGE),
            };
            let mut task = Task {
                name,
                prompt: String::new(),
                dir: std::env::current_dir()?.to_string_lossy().into_owned(),
                cron: String::new(),
                mode: Mode::Headless,
                model: None,
                effort: None,
                permissions: None,
                budget: None,
                enabled: true,
                catchup: true,
            };
            apply(&mut task, &mut args)?;
            ensure_no_extra(&args)?;
            let id = s.save(None, task, Some(&termf()?))?;
            println!("{id}: next run {}", next(&s, &id));
        }
        Some("edit") => {
            let ["edit", id, ..] = args[..] else {
                bail!(crate::USAGE)
            };
            let mut task = s.task(id)?;
            args.drain(..2);
            apply(&mut task, &mut args)?;
            ensure_no_extra(&args)?;
            s.save(Some(id), task, Some(&termf()?))?;
            println!("{id}: next run {}", next(&s, id));
        }
        _ => match args.as_slice() {
            ["list"] => {
                for (id, task) in s.tasks()? {
                    let state = if task.enabled { "on" } else { "off" };
                    let mode = match task.mode {
                        Mode::Headless => "headless",
                        Mode::Tile => "tile",
                    };
                    let closed = if s.installed(&id) {
                        "  [also while TermF is closed]"
                    } else {
                        ""
                    };
                    println!(
                        "{id:<24}  {state:<3}  {mode:<8}  {:<16}  next {}  {}{closed}",
                        task.cron,
                        next(&s, &id),
                        task.name
                    );
                }
            }
            ["rm", id] => {
                s.delete(id)?;
                println!("{id} removed");
            }
            ["run", id, rest @ ..] => {
                let scheduled = match rest {
                    [] => false,
                    ["--scheduled"] => true,
                    _ => bail!(crate::USAGE),
                };
                run_task(&s, id, scheduled)?;
            }
            ["history", rest @ ..] => {
                let mut rest = rest.to_vec();
                let limit = take_opt(&mut rest, "--limit")?.map_or(Ok(20), |l| l.parse())?;
                if let Some(run) = take_opt(&mut rest, "--run")? {
                    ensure_no_extra(&rest)?;
                    show(&s.run(run.parse()?)?);
                    return Ok(());
                }
                let task = match rest.as_slice() {
                    [] => None,
                    [id] if !id.starts_with('-') => Some(*id),
                    _ => bail!(crate::USAGE),
                };
                for run in s.history(task, limit)? {
                    println!("{}", line(&run));
                }
            }
            ["install", id] => {
                s.install(id, &termf()?)?;
                println!("{id} also runs while TermF is closed");
            }
            ["uninstall", id] => {
                s.uninstall(id)?;
                println!("{id} only runs while TermF is open");
            }
            _ => bail!(crate::USAGE),
        },
    }
    Ok(())
}

fn ensure_no_extra(args: &[&str]) -> Result<()> {
    if !args.is_empty() {
        bail!("unexpected {}\n{}", args.join(" "), crate::USAGE);
    }
    Ok(())
}

fn next(s: &Scheduler, id: &str) -> String {
    let task = s.task(id).ok();
    match task {
        Some(task) if !task.enabled => "never (off)".into(),
        Some(task) => termf_schedule::upcoming(&task.cron, termf_schedule::now(), 1)
            .ok()
            .and_then(|n| n.first().copied())
            .map_or_else(|| "never".into(), local),
        None => "never".into(),
    }
}

fn status(run: &Execution) -> &'static str {
    match run.status {
        Status::Running => "running",
        Status::Succeeded => "succeeded",
        Status::Failed => "failed",
        Status::Opened => "opened",
    }
}

fn line(run: &Execution) -> String {
    let trigger = match run.trigger {
        Trigger::Schedule => "scheduled",
        Trigger::Catchup => "caught up",
        Trigger::Manual => "by hand",
    };
    let took = run
        .finished
        .map(|f| format!("{} s", f - run.started))
        .unwrap_or_default();
    let cost = run.cost.map(|c| format!("${c:.2}")).unwrap_or_default();
    format!(
        "{:>5}  {}  {:<9}  {trigger:<9}  {took:>7}  {cost:>6}  {}",
        run.id,
        local(run.started),
        status(run),
        run.name
    )
}

fn show(run: &Execution) {
    println!("{}", line(run));
    if let Some(error) = &run.error {
        println!("\nerror: {error}");
    }
    if let Some(output) = &run.output {
        println!("\n{output}");
    }
}

/// Through TermF when it is open; here otherwise, headless only.
fn run_task(s: &Scheduler, id: &str, scheduled: bool) -> Result<()> {
    if let Ok(mut client) = Client::connect(None) {
        let run: Option<Execution> = serde_json::from_value(client.call(Call::Schedule {
            task: id.into(),
            scheduled,
        })?)?;
        match run {
            Some(run) => println!("run {} started in TermF", run.id),
            None => println!("{id}: nothing due"),
        }
        return Ok(());
    }
    let task = s.task(id)?;
    let claim = if scheduled {
        s.due(id, &task, termf_schedule::now())?
            .map(|(slot, trigger)| (trigger, Some(slot)))
    } else {
        Some((Trigger::Manual, None))
    };
    let Some((trigger, slot)) = claim else {
        println!("{id}: nothing due");
        return Ok(());
    };
    let Some(run) = s.claim(id, &task, trigger, slot, Host::Cli)? else {
        println!("{id}: this time already ran");
        return Ok(());
    };
    let done = if task.mode == Mode::Tile {
        s.finish(
            run.id,
            &termf_schedule::Outcome {
                status: Status::Failed,
                cost: None,
                output: None,
                error: Some("this task opens a tile: TermF must be open".into()),
            },
        )?
    } else {
        let settings = termf_core::load_settings(&termf_core::default_config_dir()?)?;
        s.run_headless(&run, &task, &termf_schedule::claude_program(&settings))?
    };
    notify(&done);
    show(&done);
    if done.status == Status::Failed {
        bail!("{id} failed");
    }
    Ok(())
}

fn notify(run: &Execution) {
    let body = match run.status {
        Status::Succeeded => match run.cost {
            Some(cost) => format!("Done (${cost:.2})"),
            None => "Done".into(),
        },
        _ => format!(
            "Failed: {}",
            run.error
                .as_deref()
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or_default()
        ),
    };
    // Shows TermF's name and icon when the app is installed.
    #[cfg(target_os = "macos")]
    let _ = notify_rust::set_application("dev.termf.app");
    let _ = notify_rust::Notification::new()
        .summary(&format!("TermF · {}", run.name))
        .body(&body)
        .show();
}
