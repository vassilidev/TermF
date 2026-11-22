//! Running a task with Claude Code: headless (`claude -p`, its answer saved), or the arguments of an
//! interactive session.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;
use termf_term::{shell_command, shell_quote};

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

/// Go after the agent's command, so they win over its own options. The prompt is the session's first message,
/// after `--` so that one starting with `-` is not read as an option.
pub fn tile_args(task: &Task) -> Vec<String> {
    let mut args = options(task);
    args.extend(["--".into(), task.prompt.clone()]);
    args
}

/// The agent's command with the task's arguments, through the login shell as a tile's: `~`, quotes and the
/// shell's PATH work the same.
pub fn headless_command(agent: &str, task: &Task) -> Command {
    let args: Vec<String> = headless_args(task).iter().map(|a| shell_quote(a)).collect();
    let (program, args) = shell_command(Some(&format!("{agent} {}", args.join(" "))));
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.args(args)
        .current_dir(termf_core::expand_tilde(&task.dir));
    cmd
}

/// Runs the task headless with `cmd` (see [`headless_command`]) and says how it ended. `started` gets the
/// process id; what the process said when it failed starts with `who`.
pub(crate) fn headless(
    cmd: Command,
    task: &Task,
    timeout: Duration,
    who: &str,
    started: impl FnOnce(u32),
) -> Outcome {
    let dir = termf_core::expand_tilde(&task.dir);
    if !dir.is_dir() {
        return failed(format!("the folder {} does not exist", dir.display()));
    }
    match execute(cmd, &task.prompt, timeout, started) {
        Ok((success, stdout, stderr)) => {
            let mut outcome = outcome(success, &stdout, &stderr);
            if let Some(error) = &mut outcome.error {
                *error = format!("{who}: {error}");
            }
            outcome
        }
        Err(e) => failed(format!("{e:#}")),
    }
}

pub(crate) fn failed(error: String) -> Outcome {
    Outcome {
        status: Status::Failed,
        cost: None,
        output: None,
        error: Some(error),
    }
}

/// Kills the run's process group once its runner (the app or the CLI) is gone or its time is up, so it never
/// outlives them. Its own process group: Ctrl+C on the CLI does not reach it.
#[cfg(unix)]
const WATCHDOG: &str = r#"while kill -s 0 -- -"$1" 2>/dev/null; do
  if ! kill -0 "$2" 2>/dev/null || [ "$(date +%s)" -ge "$3" ]; then kill -s KILL -- -"$1" 2>/dev/null; exit 0; fi
  sleep 1
done"#;

#[cfg(unix)]
pub(crate) fn watchdog(group: u32, runner: u32, deadline: i64) -> Option<std::process::Child> {
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    Command::new("/bin/sh")
        .args(["-c", WATCHDOG, "sh"])
        .args([group.to_string(), runner.to_string(), deadline.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()
}

/// Whether a process still runs (a zombie counts until its parent reaps it).
pub(crate) fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 only checks that the process exists and may be signalled.
        let found = unsafe { libc::kill(pid, 0) } == 0;
        found || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        // shortcut: asks tasklist rather than the Win32 API; only used when a run looks left behind.
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
    }
}

fn execute(
    mut cmd: Command,
    prompt: &str,
    timeout: Duration,
    started: impl FnOnce(u32),
) -> Result<(bool, String, String)> {
    let scratch = scratch_dir()?;
    let result = (|| {
        let (stdin, stdout, stderr) =
            (scratch.join("in"), scratch.join("out"), scratch.join("err"));
        fs::write(&stdin, prompt)?;
        cmd.stdin(fs::File::open(&stdin)?)
            .stdout(fs::File::create(&stdout)?)
            .stderr(fs::File::create(&stderr)?);
        // Its own process group, so stopping it also stops what it started.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd.spawn().context("cannot run Claude Code")?;
        started(child.id());
        // A backup of the deadline below, a minute later, for when this process stops before it.
        #[cfg(unix)]
        let mut watchdog = watchdog(
            child.id(),
            std::process::id(),
            crate::now() + timeout.as_secs() as i64 + 60,
        );
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if Instant::now() > deadline {
                #[cfg(unix)]
                termf_core::process::kill_group(child.id());
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        #[cfg(unix)]
        if let Some(w) = watchdog.as_mut() {
            let _ = w.kill();
            let _ = w.wait();
        }
        let Some(status) = status else {
            bail!("stopped after {} min", timeout.as_secs() / 60);
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

/// Reads Claude Code's JSON result: the whole output, else its last JSON line (a shell profile may print first).
pub(crate) fn outcome(success: bool, stdout: &str, stderr: &str) -> Outcome {
    let json = |text: &str| {
        serde_json::from_str::<Value>(text.trim())
            .ok()
            .filter(Value::is_object)
    };
    let result = json(stdout).or_else(|| stdout.lines().rev().find_map(json));
    let Some(result) = result else {
        let said = [stderr, stdout]
            .map(termf_core::process::excerpt)
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
        .or_else(|| Some(termf_core::process::excerpt(stderr)).filter(|t| !t.is_empty()))
        .unwrap_or_else(|| "Claude Code stopped with an error".into());
    Outcome {
        status: Status::Failed,
        cost,
        output: answer,
        error: Some(error),
    }
}
