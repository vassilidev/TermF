//! Running a helper program (Claude Code, a check script) with a deadline.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

/// Runs `cmd` with `input` on stdin. Output goes to files so a chatty child never blocks; after `timeout` the
/// child and what it started are killed.
pub fn run(
    cmd: Command,
    input: &[u8],
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    stream(cmd, input, timeout, &mut |_| {})
}

/// [`run`], handing each line the child prints to `line` as it comes.
pub fn stream(
    cmd: Command,
    input: &[u8],
    timeout: Duration,
    line: &mut dyn FnMut(&str),
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    exec(cmd, input, timeout, line, &AtomicBool::new(false))
}

/// [`run`], killed like at the deadline as soon as `cancel` is set.
pub fn run_until(
    cmd: Command,
    input: &[u8],
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    exec(cmd, input, timeout, &mut |_| {}, cancel)
}

fn exec(
    mut cmd: Command,
    input: &[u8],
    timeout: Duration,
    line: &mut dyn FnMut(&str),
    cancel: &AtomicBool,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let scratch = scratch_dir()?;
    let result = (|| {
        let (stdin, stdout, stderr) =
            (scratch.join("in"), scratch.join("out"), scratch.join("err"));
        fs::write(&stdin, input)?;
        cmd.stdin(fs::File::open(&stdin)?)
            .stdout(fs::File::create(&stdout)?)
            .stderr(fs::File::create(&stderr)?);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd.spawn()?;
        let mut reader = BufReader::new(fs::File::open(&stdout)?);
        let (mut out, mut partial) = (Vec::new(), Vec::new());
        let deadline = Instant::now() + timeout;
        let status = loop {
            let exited = child.try_wait()?;
            // Bytes, not text: the file may end in the middle of a character.
            while reader.read_until(b'\n', &mut partial)? > 0 && partial.ends_with(b"\n") {
                line(String::from_utf8_lossy(&partial).trim_end());
                out.append(&mut partial);
            }
            if let Some(status) = exited {
                break status;
            }
            if Instant::now() > deadline {
                end(&mut child);
                bail!("timed out after {} s", timeout.as_secs());
            }
            if cancel.load(Ordering::SeqCst) {
                end(&mut child);
                bail!("stopped");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        out.append(&mut partial);
        Ok((status, out, fs::read(&stderr)?))
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn end(child: &mut Child) {
    #[cfg(unix)]
    kill_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

/// Kills the process group `leader` started (spawned with `process_group(0)`). A signal, not the `kill`
/// program: Linux's reads `-<pid>` as a signal.
#[cfg(unix)]
pub fn kill_group(leader: u32) {
    if let Ok(group) = i32::try_from(leader) {
        // SAFETY: only sends a signal; a group already gone gives an error, ignored.
        unsafe { libc::kill(-group, libc::SIGKILL) };
    }
}

/// A fresh private folder in the system's temporary folder.
pub fn scratch_dir() -> Result<PathBuf> {
    use std::sync::atomic::AtomicUsize;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "termf-run-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The last 500 characters of a program's output, for an error message.
pub fn tail(text: &str) -> String {
    let text = text.trim();
    let start = text.char_indices().rev().nth(500).map_or(0, |(i, _)| i);
    text[start..].into()
}

/// A program's error output for a message: its start, where the error line is, then its end when cut in between,
/// without stack frames. Each line is cut at 200 characters (a minified source line in a Node trace).
pub fn excerpt(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| match l.char_indices().nth(200) {
            Some((i, _)) => format!("{}…", &l[..i]),
            None => l.to_owned(),
        })
        .collect();
    let fits = |budget: usize| {
        let mut size = 0;
        move |l: &&String| {
            size += l.len();
            size <= budget
        }
    };
    let head = lines
        .iter()
        .take_while(fits(600))
        .count()
        .max(1)
        .min(lines.len());
    let mut end: Vec<&String> = lines[head..]
        .iter()
        .rev()
        .filter(|l| !l.starts_with("at "))
        .take_while(fits(200))
        .collect();
    end.reverse();
    let mut out: Vec<&str> = lines[..head].iter().map(String::as_str).collect();
    if lines.len() > head + end.len() {
        out.push("…");
    }
    out.extend(end.into_iter().map(String::as_str));
    out.join("\n")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn an_excerpt_keeps_the_error_line_and_what_the_end_adds() {
        let code = "var a=1;".repeat(400);
        let frames: String = (0..40)
            .map(|i| format!("    at f{i} (file:///opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js:10:{i})\n"))
            .collect();
        let trace = format!(
            "file:///opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js:388\n{code}\n        ^\n\n\
             TypeError: Cannot read properties of undefined (reading 'prototype')\n{frames}\nNode.js v26.0.0\n"
        );
        let said = excerpt(&trace);
        let lines: Vec<&str> = said.lines().collect();
        assert_eq!(
            lines[0],
            "file:///opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js:388"
        );
        assert_eq!(lines[1].chars().count(), 201, "a long line is cut");
        assert!(
            said.contains(
                "\nTypeError: Cannot read properties of undefined (reading 'prototype')\n"
            ),
            "{said}"
        );
        assert_eq!(lines[lines.len() - 2..], ["…", "Node.js v26.0.0"]);
        assert!(said.len() < 1000, "{said}");

        assert_eq!(excerpt("  Error: not logged in \n"), "Error: not logged in");
        assert_eq!(excerpt(""), "");
        let short = "a\nb\nc";
        assert_eq!(excerpt(short), short, "nothing cut: no ellipsis");
    }

    /// Within 5 s: dead, or a zombie its new parent has not reaped yet.
    fn gone(pid: &str) -> bool {
        (0..50).any(|_| {
            let out = Command::new("ps").args(["-o", "stat=", "-p", pid]).output();
            let stat = String::from_utf8_lossy(&out.unwrap().stdout)
                .trim()
                .to_owned();
            let gone = stat.is_empty() || stat.starts_with('Z');
            if !gone {
                std::thread::sleep(Duration::from_millis(100));
            }
            gone
        })
    }

    #[test]
    fn a_timeout_kills_what_the_program_started() {
        let dir = scratch_dir().unwrap();
        let file = dir.join("pid");
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30 & echo $! > \"$1\"; wait", "sh"])
            .arg(&file);
        let error = run(cmd, b"", Duration::from_millis(500)).unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error}");
        let pid = fs::read_to_string(&file).unwrap();
        assert!(gone(pid.trim()), "its background sleep outlived it");
        let _ = fs::remove_dir_all(&dir);
    }
}
