//! The system tools TermF drives: ssh-add, ssh, git and the GitHub CLI.

use std::ffi::OsStr;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::config::{self, Host};

/// Set by TermF on ssh-add so the `termf` binary acts as its askpass helper (see `askpass`).
pub const ASKPASS_ENV: &str = "TERMF_ASKPASS";

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// Why the agent could not be listed: none running, no ssh-add…
    pub error: Option<String>,
    pub keys: Vec<AgentKey>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentKey {
    pub bits: Option<u32>,
    pub fingerprint: String,
    pub comment: String,
    /// As ssh-add names it: `ED25519`, `RSA`, `ECDSA-SK`…
    pub kind: String,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostTest {
    pub ok: bool,
    /// The server's greeting, or the error ssh gave.
    pub message: String,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepoKeys {
    pub path: String,
    /// SSH remotes only.
    pub remotes: Vec<Remote>,
    pub ssh_command: Option<String>,
    /// The keys of `core.sshCommand` when it names some, else those the config gives the remotes' hosts.
    pub identity_files: Vec<String>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: String,
    /// What ssh connects to: a host name or an alias of the config.
    pub host: String,
}

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

fn output_text(out: &Output) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    text.trim().to_owned()
}

/// Apple's ssh-add, which can keep passphrases in the keychain; Homebrew's, first on some PATHs, cannot.
fn ssh_add() -> (Command, bool) {
    let apple = Path::new("/usr/bin/ssh-add");
    if cfg!(target_os = "macos") && apple.exists() {
        (command(apple), true)
    } else {
        (command("ssh-add"), false)
    }
}

/// The keys of the agent `SSH_AUTH_SOCK` points at (1Password, Secretive and others included).
pub fn agent() -> Agent {
    let (mut cmd, _) = ssh_add();
    match cmd
        .args(["-l", "-E", "sha256"])
        .stdin(Stdio::null())
        .output()
    {
        Err(e) => Agent {
            error: Some(format!("cannot run ssh-add: {e}")),
            keys: Vec::new(),
        },
        Ok(out) if out.status.success() => Agent {
            error: None,
            keys: parse_agent(&String::from_utf8_lossy(&out.stdout)),
        },
        Ok(out) if String::from_utf8_lossy(&out.stdout).contains("no identities") => {
            Agent::default()
        }
        Ok(out) => Agent {
            error: Some(agent_error(&out)),
            keys: Vec::new(),
        },
    }
}

/// `ssh-add -l` lines: `256 SHA256:… comment (ED25519)`.
fn parse_agent(text: &str) -> Vec<AgentKey> {
    text.lines()
        .filter_map(|line| {
            let (bits, rest) = line.split_once(' ')?;
            let (fingerprint, rest) = rest.split_once(' ').unwrap_or((rest, ""));
            let (comment, kind) = rest.rsplit_once('(').unwrap_or((rest, ""));
            Some(AgentKey {
                bits: bits.parse().ok(),
                fingerprint: fingerprint.into(),
                comment: comment.trim().into(),
                kind: kind.trim_end_matches(')').into(),
            })
        })
        .collect()
}

fn agent_error(out: &Output) -> String {
    let text = output_text(out);
    if text.contains("Could not open a connection") || text.contains("Error connecting to agent") {
        "no SSH agent is reachable (SSH_AUTH_SOCK)".into()
    } else if text.is_empty() {
        format!("ssh-add failed ({})", out.status)
    } else {
        text
    }
}

/// Adds a key to the agent, keeping its passphrase in the macOS keychain when this ssh-add can.
/// A passphrase reaches ssh-add through `askpass` (the `termf` binary), which reads it from a pipe:
/// never from arguments, environment or files. Without `askpass`, ssh-add asks on the terminal if any.
pub fn agent_add(key: &Path, passphrase: Option<&str>, askpass: Option<&Path>) -> Result<()> {
    ensure!(
        askpass.is_some() || passphrase.is_none(),
        "the termf helper that hands the passphrase to ssh-add is missing: run `ssh-add {}` in a terminal",
        key.display()
    );
    let (_, keychain) = ssh_add();
    let mut out = add_once(
        key,
        keychain.then_some("--apple-use-keychain"),
        passphrase,
        askpass,
    )?;
    let text = output_text(&out);
    // macOS before 12 only knows the older spelling.
    if keychain && (text.contains("illegal option") || text.contains("unrecognized option")) {
        out = add_once(key, Some("-K"), passphrase, askpass)?;
    }
    if out.status.success() {
        return Ok(());
    }
    let text = output_text(&out);
    // ssh-add shows "Bad passphrase" in its next prompt, not on stderr: a silent failure is a refused passphrase.
    if text.is_empty() || text.contains("Bad passphrase") {
        match passphrase {
            Some(_) => bail!("wrong passphrase"),
            None => bail!("this key needs its passphrase"),
        }
    }
    bail!(agent_error(&out))
}

fn add_once(
    key: &Path,
    keychain_flag: Option<&str>,
    passphrase: Option<&str>,
    askpass: Option<&Path>,
) -> Result<Output> {
    let (mut cmd, _) = ssh_add();
    cmd.args(keychain_flag)
        .arg(key)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match askpass {
        Some(askpass) => {
            cmd.env("SSH_ASKPASS", askpass)
                .env("SSH_ASKPASS_REQUIRE", "force")
                .env(ASKPASS_ENV, "1")
                .stdin(Stdio::piped());
            // OpenSSH before 8.4 ignores SSH_ASKPASS_REQUIRE and only asks askpass when DISPLAY is set.
            if std::env::var_os("DISPLAY").is_none() {
                cmd.env("DISPLAY", ":0");
            }
        }
        None => _ = cmd.env("SSH_ASKPASS_REQUIRE", "never").stdin(Stdio::null()),
    }
    let mut child = cmd.spawn().context("cannot run ssh-add")?;
    // ssh-add never reads this pipe itself: its askpass child inherits it as stdin.
    if let Some(mut stdin) = child.stdin.take() {
        let line = Zeroizing::new(format!("{}\n", passphrase.unwrap_or("")));
        // An unencrypted key, or one whose passphrase is in the keychain, is added without asking.
        let _ = stdin.write_all(line.as_bytes());
    }
    Ok(child.wait_with_output()?)
}

/// The askpass side, run by the `termf` binary when `ASKPASS_ENV` is set: prints the passphrase TermF wrote
/// on the pipe it inherited from ssh-add. A second call (after a wrong passphrase) finds the pipe empty,
/// which makes ssh-add give up instead of asking forever.
pub fn askpass() -> Result<()> {
    let mut line = Zeroizing::new(String::new());
    std::io::stdin().lock().read_line(&mut line)?;
    let mut out = std::io::stdout().lock();
    out.write_all(line.trim_end_matches(['\n', '\r']).as_bytes())?;
    out.write_all(b"\n")?;
    Ok(out.flush()?)
}

pub fn agent_remove(key: &Path) -> Result<()> {
    let (mut cmd, _) = ssh_add();
    let out = cmd
        .arg("-d")
        .arg(key)
        .stdin(Stdio::null())
        .output()
        .context("cannot run ssh-add")?;
    ensure!(out.status.success(), "{}", agent_error(&out));
    Ok(())
}

/// `host` comes from the UI: it can never be read as an option. With `identity`, only that key is offered.
pub fn test_host(host: &str, identity: Option<&Path>) -> Result<HostTest> {
    ensure!(
        !host.is_empty()
            && !host.starts_with('-')
            && !host.contains(|c: char| c.is_whitespace() || c.is_control()),
        "invalid host {host:?}"
    );
    let mut cmd = command("ssh");
    cmd.args(["-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"]);
    if let Some(identity) = identity {
        cmd.arg("-i")
            .arg(identity)
            .args(["-o", "IdentitiesOnly=yes"]);
    }
    let out = cmd
        .args(["--", host])
        .stdin(Stdio::null())
        .output()
        .context("cannot run ssh")?;
    Ok(interpret_test(out.status.code(), &output_text(&out)))
}

/// GitHub, Gitea and Bitbucket greet a working key then exit with 1, since they give no shell.
fn interpret_test(code: Option<i32>, output: &str) -> HostTest {
    const GREETINGS: &[&str] = &[
        "successfully authenticated",
        "Welcome to GitLab",
        "logged in as",
        "authenticated via",
    ];
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if let Some(line) = lines
        .iter()
        .find(|l| GREETINGS.iter().any(|g| l.contains(g)))
    {
        return HostTest {
            ok: true,
            message: line.to_string(),
        };
    }
    let ok = code == Some(0);
    let message = match lines.last() {
        Some(line) => line.to_string(),
        None if ok => "connected".into(),
        None => format!("ssh failed (exit code {code:?})"),
    };
    HostTest { ok, message }
}

/// Adds a public key to the GitHub account `gh` is signed in to. Only the key text goes to gh, on stdin.
pub fn github_upload(public_key: &str, title: &str) -> Result<()> {
    let mut cmd = gh();
    cmd.args(["ssh-key", "add", "-"]);
    if !title.trim().is_empty() {
        cmd.args(["--title", title]);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("the GitHub CLI (gh) is not installed")?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{public_key}\n").as_bytes());
    }
    let out = child.wait_with_output()?;
    if out.status.success() {
        return Ok(());
    }
    let text = output_text(&out);
    if text.contains("admin:public_key") {
        bail!(
            "gh lacks the admin:public_key scope: run `gh auth refresh -h github.com -s admin:public_key`"
        );
    }
    if text.contains("gh auth login") {
        bail!("gh is not signed in: run `gh auth login`");
    }
    bail!("gh ssh-key add failed: {text}")
}

fn gh() -> Command {
    let mut cmd = command("gh");
    // Launched from the Finder, TermF lacks the PATH the shell profile sets (Homebrew…); it is also gh's lookup PATH.
    if let Some(path) = termf_core::shell::login_path() {
        cmd.env("PATH", path);
    }
    cmd
}

fn git(repo: &Path) -> Command {
    let mut cmd = command("git");
    cmd.arg("-C").arg(repo).stdin(Stdio::null());
    cmd
}

pub fn assign(repo: &Path, key: Option<&Path>) -> Result<()> {
    let out = match key {
        Some(key) => git(repo)
            .args(["config", "core.sshCommand"])
            .arg(format!(
                "ssh -i {} -o IdentitiesOnly=yes",
                sh_quote(&key.to_string_lossy())
            ))
            .output(),
        None => git(repo)
            .args(["config", "--unset", "core.sshCommand"])
            .output(),
    }
    .context("cannot run git")?;
    // 5: there was no core.sshCommand to remove.
    ensure!(
        out.status.success() || (key.is_none() && out.status.code() == Some(5)),
        "git config failed: {}",
        output_text(&out)
    );
    Ok(())
}

/// git runs core.sshCommand through a shell.
fn sh_quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+@:,=".contains(c))
    {
        word.into()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

pub(crate) fn identity_args(command: &str) -> Vec<String> {
    let words = config::words(command);
    let mut files = Vec::new();
    let mut words = words.into_iter();
    while let Some(word) = words.next() {
        if word == "-i" {
            files.extend(words.next());
        } else if let Some(file) = word.strip_prefix("-i").filter(|f| !f.is_empty()) {
            files.push(file.into());
        }
    }
    files
}

/// The repository's own core.sshCommand, ignoring the global one.
pub(crate) fn local_ssh_command(repo: &Path) -> Option<String> {
    let out = git(repo)
        .args(["config", "--local", "--get", "core.sshCommand"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

pub(crate) fn repos_keys(repos: &[PathBuf], hosts: &[Host], home: &Path) -> Vec<RepoKeys> {
    let chunk = repos.len().div_ceil(8).max(1);
    std::thread::scope(|s| {
        let workers: Vec<_> = repos
            .chunks(chunk)
            .map(|repos| {
                s.spawn(move || {
                    repos
                        .iter()
                        .filter_map(|r| repo_keys(r, hosts, home))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect()
    })
}

fn repo_keys(repo: &Path, hosts: &[Host], home: &Path) -> Option<RepoKeys> {
    let out = git(repo)
        .args([
            "config",
            "--get-regexp",
            r"^(remote\..*\.url|core\.sshcommand)$",
        ])
        .output()
        .ok()?;
    let mut remotes = Vec::new();
    let mut ssh_command = None;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        if key == "core.sshcommand" {
            ssh_command = Some(value.to_owned());
        } else if let (Some(name), Some(host)) = (
            key.strip_prefix("remote.")
                .and_then(|k| k.strip_suffix(".url")),
            ssh_host(value),
        ) {
            remotes.push(Remote {
                name: name.into(),
                url: value.into(),
                host,
            });
        }
    }
    if remotes.is_empty() && ssh_command.is_none() {
        return None;
    }
    let from_command = ssh_command
        .as_deref()
        .map(identity_args)
        .unwrap_or_default();
    let mut identity_files: Vec<String> = if from_command.is_empty() {
        remotes
            .iter()
            .flat_map(|r| hosts.iter().filter(|h| h.matches(&r.host)))
            .flat_map(|h| h.identity_files.clone())
            .collect()
    } else {
        from_command
            .iter()
            .map(|f| config::expand(f, home).to_string_lossy().into_owned())
            .collect()
    };
    // Two remotes, or two blocks matching one host, can name the same key apart from each other.
    let mut seen = std::collections::HashSet::new();
    identity_files.retain(|f| seen.insert(f.clone()));
    Some(RepoKeys {
        path: repo.to_string_lossy().into_owned(),
        remotes,
        ssh_command,
        identity_files,
    })
}

fn ssh_host(url: &str) -> Option<String> {
    let host = match ["ssh://", "git+ssh://", "ssh+git://"]
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
    {
        Some(rest) => {
            let authority = rest.split('/').next()?;
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            match host.strip_prefix('[') {
                Some(v6) => v6.split(']').next()?,
                None => host.split(':').next()?,
            }
        }
        // scp-like `[user@]host:path`, as git reads it: a colon before any slash. One letter is a Windows drive.
        None if !url.contains("://") => {
            let (authority, _) = url.split_once(':')?;
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            if authority.contains('/') || host.len() < 2 {
                return None;
            }
            host.trim_start_matches('[').trim_end_matches(']')
        }
        None => return None,
    };
    (!host.is_empty()).then(|| host.to_owned())
}

/// What a copied secret is compared on: line endings and trailing blanks change on the way through
/// some clipboards (Windows turns LF into CRLF).
pub fn clipboard_digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let normalized = Zeroizing::new(text.replace('\r', ""));
    Sha256::digest(normalized.trim_end())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Empties the clipboard if it still holds the text whose `clipboard_digest` is `digest`, so a copied
/// private key does not linger, while anything copied since is left alone. Returns whether it was emptied.
pub fn clipboard_clear(digest: &str) -> Result<bool> {
    // (read, clear) commands; the clear ones get an empty stdin.
    let tools: &[(&[&str], &[&str])] = if cfg!(target_os = "macos") {
        &[(&["pbpaste"], &["pbcopy"])]
    } else if cfg!(windows) {
        &[(
            &[
                "powershell",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-Clipboard -Raw",
            ],
            &["clip"],
        )]
    } else {
        &[
            (&["wl-paste", "--no-newline"], &["wl-copy", "--clear"]),
            (
                &["xclip", "-selection", "clipboard", "-o"],
                &["xclip", "-selection", "clipboard", "-i"],
            ),
            (
                &["xsel", "--clipboard", "--output"],
                &["xsel", "--clipboard", "--clear"],
            ),
        ]
    };
    let mut found = false;
    for (read, clear) in tools {
        let out = match command(read[0])
            .args(&read[1..])
            .stdin(Stdio::null())
            .output()
        {
            Ok(out) => out,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("cannot run {}", read[0])),
        };
        found = true;
        // wl-paste outside Wayland, or an empty clipboard: the next tool may know better.
        if !out.status.success() {
            continue;
        }
        let content = Zeroizing::new(String::from_utf8_lossy(&out.stdout).into_owned());
        if clipboard_digest(&content) != digest {
            return Ok(false);
        }
        let status = command(clear[0])
            .args(&clear[1..])
            .stdin(Stdio::null())
            .status()
            .with_context(|| format!("cannot run {}", clear[0]))?;
        ensure!(status.success(), "{} failed", clear[0]);
        return Ok(true);
    }
    ensure!(
        found,
        "no clipboard tool found (wl-clipboard, xclip or xsel)"
    );
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_listing() {
        let keys = parse_agent(
            "256 SHA256:abc me@laptop (ED25519)\n4096 SHA256:def My (work) key (RSA)\n256 SHA256:ghi (ECDSA-SK)\n",
        );
        assert_eq!(
            keys[0],
            AgentKey {
                bits: Some(256),
                fingerprint: "SHA256:abc".into(),
                comment: "me@laptop".into(),
                kind: "ED25519".into(),
            }
        );
        assert_eq!(keys[1].comment, "My (work) key");
        assert_eq!(keys[1].kind, "RSA");
        assert_eq!(keys[2].comment, "");
        assert_eq!(keys[2].kind, "ECDSA-SK");
    }

    #[test]
    fn interprets_test_output() {
        let github = interpret_test(
            Some(1),
            "Hi octocat! You've successfully authenticated, but GitHub does not provide shell access.",
        );
        assert!(github.ok);
        assert!(github.message.starts_with("Hi octocat!"));
        assert!(interpret_test(Some(0), "Welcome to GitLab, @octocat!").ok);
        assert!(
            interpret_test(
                Some(1),
                "Hi there, octocat! You've successfully authenticated with the key named k, but Gitea does not provide shell access."
            )
            .ok
        );
        let denied = interpret_test(
            Some(255),
            "Warning: Permanently added 'github.com' to the list of known hosts.\ngit@github.com: Permission denied (publickey).",
        );
        assert_eq!(
            denied,
            HostTest {
                ok: false,
                message: "git@github.com: Permission denied (publickey).".into()
            }
        );
        assert!(!interpret_test(Some(255), "Host key verification failed.").ok);
        assert!(!interpret_test(None, "").ok);
        assert!(interpret_test(Some(0), "").ok);
    }

    #[test]
    fn reads_ssh_remotes() {
        for (url, host) in [
            ("git@github.com:org/repo.git", Some("github.com")),
            ("gh-work:org/repo.git", Some("gh-work")),
            ("ssh://git@gitlab.com:2222/org/repo.git", Some("gitlab.com")),
            ("ssh://[::1]:22/repo", Some("::1")),
            ("git+ssh://me@host/repo", Some("host")),
            ("https://github.com/org/repo.git", None),
            ("/srv/git/repo.git", None),
            ("./a:b", None),
            ("C:/src/repo", None),
            ("file:///srv/repo", None),
        ] {
            assert_eq!(ssh_host(url).as_deref(), host, "{url}");
        }
    }

    #[test]
    fn reads_identity_args() {
        assert_eq!(
            identity_args("ssh -i /k/id -o IdentitiesOnly=yes"),
            ["/k/id"]
        );
        assert_eq!(
            identity_args("ssh -i '/my keys/id' -i~/x"),
            ["/my keys/id", "~/x"]
        );
        assert_eq!(identity_args("ssh -v"), Vec::<String>::new());
        let quoted = sh_quote("/Users/o'neil/.ssh/id");
        assert_eq!(
            identity_args(&format!("ssh -i {quoted}")),
            ["/Users/o'neil/.ssh/id"]
        );
        assert_eq!(
            sh_quote("/home/me/.ssh/id_ed25519"),
            "/home/me/.ssh/id_ed25519"
        );
        assert_eq!(sh_quote(r"C:\Users\me\.ssh\id"), r"'C:\Users\me\.ssh\id'");
        assert_eq!(
            identity_args(&format!("ssh -i {}", sh_quote(r"C:\Users\me\.ssh\id"))),
            [r"C:\Users\me\.ssh\id"]
        );
    }
}
