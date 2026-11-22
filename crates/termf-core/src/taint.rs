//! The "trifecta" rule (ARCHITECTURE §12.4): once a recipe read prod data for an agent session, that session
//! loses its ways out (the web, network commands, the tracker) unless the user approves each, and the sessions
//! its team run starts next lose them outright. TermF marks the Claude Code session and its run under
//! `<data>/ops/taint/`; `termf guard` and the team read the marks. A mark is never removed.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};

/// The tools that reach the web, which `termf guard` asks about in a marked session.
pub const EGRESS_TOOLS: [&str; 2] = ["WebFetch", "WebSearch"];

/// Programs that send data out, which `termf guard` asks about in a marked session. Not every way out: a script,
/// an interpreter or a program by another name still gets through.
pub const EGRESS_PROGRAMS: &[&str] = &[
    "curl", "wget", "nc", "ncat", "netcat", "socat", "telnet", "ssh", "scp", "sftp", "rsync",
    "ftp", "http", "https", "xh", "gh", "glab", "aws", "gcloud", "az", "kubectl", "dig",
    "nslookup", "host",
];

/// Denied to the sessions a marked run starts next.
pub const EGRESS_DENY: &[&str] = &[
    "WebFetch",
    "WebSearch",
    "Bash(curl *)",
    "Bash(wget *)",
    "Bash(nc *)",
    "Bash(ncat *)",
    "Bash(socat *)",
    "Bash(telnet *)",
    "Bash(ssh *)",
    "Bash(scp *)",
    "Bash(sftp *)",
    "Bash(rsync *)",
    "Bash(ftp *)",
    "Bash(gh *)",
    "Bash(glab *)",
];

fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("ops").join("taint")
}

/// A Claude Code session id: a UUID; nothing that could leave the folder.
fn file_name(session: &str) -> Option<String> {
    let safe = !session.is_empty()
        && session.len() <= 128
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    safe.then(|| format!("session-{session}"))
}

fn mark(data_dir: &Path, name: &str) -> Result<()> {
    let dir = dir(data_dir);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(name), b"")?;
    Ok(())
}

/// Marks a Claude Code session (its `session_id`, the one its hooks report).
pub fn mark_session(data_dir: &Path, session: &str) -> Result<()> {
    let name = file_name(session);
    ensure!(name.is_some(), "invalid session id {session:?}");
    mark(data_dir, &name.unwrap_or_default())
}

pub fn mark_run(data_dir: &Path, run: i64) -> Result<()> {
    mark(data_dir, &format!("run-{run}"))
}

pub fn session(data_dir: &Path, session: &str) -> bool {
    file_name(session).is_some_and(|name| dir(data_dir).join(name).exists())
}

pub fn run(data_dir: &Path, run: i64) -> bool {
    dir(data_dir).join(format!("run-{run}")).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_stay_and_never_leave_their_folder() {
        let data = std::env::temp_dir().join(format!("termf-taint-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        assert!(!session(&data, "4f1c-aa") && !run(&data, 7));
        mark_session(&data, "4f1c-aa").unwrap();
        mark_run(&data, 7).unwrap();
        assert!(session(&data, "4f1c-aa") && run(&data, 7));
        assert!(!session(&data, "other") && !run(&data, 8));
        assert!(mark_session(&data, "../../etc").is_err());
        assert!(!session(&data, "../taint/session-4f1c-aa"));
        let _ = fs::remove_dir_all(&data);
    }
}
