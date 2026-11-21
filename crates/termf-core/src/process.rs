//! Running a helper program (Claude Code, a check script) with a deadline.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus};
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
    mut cmd: Command,
    input: &[u8],
    timeout: Duration,
    line: &mut dyn FnMut(&str),
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
                kill_group(&mut child);
                bail!("timed out after {} s", timeout.as_secs());
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        out.append(&mut partial);
        Ok((status, out, fs::read(&stderr)?))
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn kill_group(child: &mut Child) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{}", child.id())])
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

/// A fresh private folder in the system's temporary folder.
pub fn scratch_dir() -> Result<PathBuf> {
    use std::sync::atomic::{AtomicUsize, Ordering};
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
