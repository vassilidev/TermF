//! Running a task with Claude Code: headless (`claude -p`, its answer saved), or the arguments of an
//! interactive session.

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::Task;
use crate::store::{Outcome, Status};

/// shortcut: one limit for every task; make it a task setting if a task ever needs longer.
pub const TIMEOUT: Duration = Duration::from_secs(2 * 3600);

/// `--model`, `--effort` and `--permission-mode`, as the task sets them.
fn options(task: &Task) -> Vec<String> {
    let mut args = Vec::new();
    for (flag, value) in [
        ("--model", &task.model),
        ("--effort", &task.effort),
        ("--permission-mode", &task.permissions),
    ] {
        if let Some(value) = value {
            args.extend([flag.to_owned(), value.clone()]);
        }
    }
    args
}

/// The prompt goes on stdin.
pub fn headless_args(task: &Task) -> Vec<String> {
    let mut args: Vec<String> = ["-p", "--output-format", "json"].map(Into::into).to_vec();
    args.extend(options(task));
    if let Some(budget) = task.budget {
        args.extend(["--max-budget-usd".into(), budget.to_string()]);
    }
    args
}

/// The prompt is the session's first message.
pub fn tile_args(task: &Task) -> Vec<String> {
    let mut args = options(task);
    args.push(task.prompt.clone());
    args
}

/// A command that opens no console window on Windows.
fn command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs the task headless with `claude` (the program and its first arguments) and says how it ended.
pub fn headless(claude: &[String], task: &Task, timeout: Duration) -> Outcome {
    match execute(claude, task, timeout) {
        Ok((success, stdout, stderr)) => outcome(success, &stdout, &stderr),
        Err(e) => failed(format!("{e:#}")),
    }
}

fn failed(error: String) -> Outcome {
    Outcome {
        status: Status::Failed,
        cost: None,
        output: None,
        error: Some(error),
    }
}

fn execute(claude: &[String], task: &Task, timeout: Duration) -> Result<(bool, String, String)> {
    let (program, lead) = claude.split_first().context("no Claude Code command")?;
    let dir = termf_core::expand_tilde(&task.dir);
    if !dir.is_dir() {
        bail!("the folder {} does not exist", dir.display());
    }
    let scratch = scratch_dir()?;
    let result = (|| {
        let (stdin, stdout, stderr) =
            (scratch.join("in"), scratch.join("out"), scratch.join("err"));
        fs::write(&stdin, &task.prompt)?;
        let mut cmd = command(program);
        cmd.args(lead)
            .args(headless_args(task))
            .current_dir(&dir)
            .stdin(fs::File::open(&stdin)?)
            .stdout(fs::File::create(&stdout)?)
            .stderr(fs::File::create(&stderr)?);
        // An app started from the Finder lacks the PATH that finds `claude` (and the `node` it may need).
        if let Some(path) = termf_core::shell::login_path() {
            cmd.env("PATH", path);
        }
        // Its own process group, so a timeout also stops what it started.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("cannot run {program}"))?;
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() > deadline {
                #[cfg(unix)]
                let _ = Command::new("kill")
                    .args(["-KILL", &format!("-{}", child.id())])
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                bail!("stopped after {} min", timeout.as_secs() / 60);
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        let read = |file| String::from_utf8_lossy(&fs::read(file).unwrap_or_default()).into_owned();
        Ok((status.success(), read(&stdout), read(&stderr)))
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn scratch_dir() -> Result<PathBuf> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "termf-schedule-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn tail(text: &str) -> String {
    let text = text.trim();
    let start = text.char_indices().rev().nth(2000).map_or(0, |(i, _)| i);
    text[start..].into()
}

/// Reads Claude Code's JSON result: the whole output, else its last JSON line (a wrapper may print first).
pub(crate) fn outcome(success: bool, stdout: &str, stderr: &str) -> Outcome {
    let json = |text: &str| {
        serde_json::from_str::<Value>(text.trim())
            .ok()
            .filter(Value::is_object)
    };
    let result = json(stdout).or_else(|| stdout.lines().rev().find_map(json));
    let Some(result) = result else {
        let said = [stderr, stdout]
            .map(tail)
            .into_iter()
            .find(|t| !t.is_empty());
        return failed(said.unwrap_or_else(|| "Claude Code gave no answer".into()));
    };
    let answer = result["result"].as_str().map(str::to_owned);
    let cost = result["total_cost_usd"].as_f64();
    if success && result["is_error"] != true {
        return Outcome {
            status: Status::Succeeded,
            cost,
            output: answer,
            error: None,
        };
    }
    let error = answer
        .clone()
        .filter(|a| !a.trim().is_empty())
        .or_else(|| result["subtype"].as_str().map(str::to_owned))
        .or_else(|| Some(tail(stderr)).filter(|t| !t.is_empty()))
        .unwrap_or_else(|| "Claude Code stopped with an error".into());
    Outcome {
        status: Status::Failed,
        cost,
        output: answer,
        error: Some(error),
    }
}
