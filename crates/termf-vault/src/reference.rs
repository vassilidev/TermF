//! References to an external secret manager, resolved by the manager's own CLI each time they are used: the
//! vault never keeps what they resolve to.

use std::ffi::OsStr;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use termf_claude::mask::mask_text;
use termf_core::process::tail;
use zeroize::Zeroizing;

use crate::{MAX_VALUE, Secret};

/// Long enough to approve a 1Password or Bitwarden unlock prompt.
const TIMEOUT: Duration = Duration::from_secs(if cfg!(test) { 3 } else { 120 });
/// How long the output may take to end once the CLI has.
const GRACE: Duration = Duration::from_secs(1);
const FORMS: &str = "a reference is op://vault/item[/section]/field, bw://item[/password|username|notes|totp|uri], \
     doppler://project/config/NAME, aws-sm://secret-id or vault://path#field";

/// The program and the arguments that print the value of `uri`. No part may read as one of its options.
fn command(uri: &str) -> Result<(&'static str, Vec<String>)> {
    ensure!(
        uri.len() <= 1024 && !uri.chars().any(char::is_control),
        "{FORMS}"
    );
    let (scheme, rest) = uri.split_once("://").context(FORMS)?;
    let parts: Vec<&str> = rest.split('/').collect();
    ensure!(
        parts.iter().all(|p| !p.is_empty() && !p.starts_with('-')),
        "{FORMS}"
    );
    let args = |args: &[&str]| args.iter().map(|a| a.to_string()).collect();
    Ok(match (scheme, parts.as_slice()) {
        ("op", [_, _, _] | [_, _, _, _]) => ("op", args(&["read", uri])),
        ("bw", [item]) => ("bw", args(&["get", "password", *item])),
        (
            "bw",
            [
                item,
                field @ ("password" | "username" | "notes" | "totp" | "uri"),
            ],
        ) => ("bw", args(&["get", *field, *item])),
        ("doppler", [project, config, name]) => (
            "doppler",
            args(&[
                "secrets",
                "get",
                *name,
                "--plain",
                &format!("--project={project}"),
                &format!("--config={config}"),
            ]),
        ),
        ("aws-sm", _) => (
            "aws",
            args(&[
                "secretsmanager",
                "get-secret-value",
                &format!("--secret-id={rest}"),
                "--query",
                "SecretString",
                "--output",
                "text",
            ]),
        ),
        ("vault", _) => match rest.rsplit_once('#') {
            Some((path, field)) if !field.is_empty() && !field.starts_with('-') => (
                "vault",
                args(&["kv", "get", &format!("-field={field}"), path]),
            ),
            _ => bail!("{FORMS}"),
        },
        _ => bail!("{FORMS}"),
    })
}

/// Whether `uri` is a reference the vault knows how to resolve.
pub fn check(uri: &str) -> Result<()> {
    command(uri).map(drop)
}

/// What each manager's CLI reads from its environment, which an app opened from the Dock or a desktop menu
/// lacks: only these come from the login shell, never the whole environment.
fn variables(program: &str) -> &'static [&'static str] {
    match program {
        "op" => &[
            "OP_ACCOUNT",
            "OP_SERVICE_ACCOUNT_TOKEN",
            "OP_CONNECT_HOST",
            "OP_CONNECT_TOKEN",
            "OP_CONFIG_DIR",
        ],
        "bw" => &["BW_SESSION", "BITWARDENCLI_APPDATA_DIR"],
        "doppler" => &["DOPPLER_TOKEN", "DOPPLER_CONFIG_DIR"],
        "aws" => &[
            "AWS_PROFILE",
            "AWS_REGION",
            "AWS_DEFAULT_REGION",
            "AWS_CONFIG_FILE",
            "AWS_SHARED_CREDENTIALS_FILE",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
        ],
        "vault" => &[
            "VAULT_ADDR",
            "VAULT_TOKEN",
            "VAULT_NAMESPACE",
            "VAULT_CACERT",
        ],
        _ => &[],
    }
}

type Shared = Arc<Mutex<Zeroizing<Vec<u8>>>>;

/// Reads `from` in the background, `limit` bytes and one more at most, into a buffer sized up front (no copy
/// left behind in a freed one) and wiped once dropped. The receiver hears when it reached the end.
fn drain(mut from: impl Read + Send + 'static, limit: usize) -> (Shared, mpsc::Receiver<()>) {
    let buf: Shared = Arc::new(Mutex::new(Zeroizing::new(Vec::with_capacity(limit + 1))));
    let (done, ended) = mpsc::channel();
    let shared = buf.clone();
    thread::spawn(move || {
        let mut chunk = Zeroizing::new([0u8; 4096]);
        while let Ok(n @ 1..) = from.read(chunk.as_mut()) {
            let mut buf = shared.lock().unwrap();
            let room = (limit + 1).saturating_sub(buf.len());
            buf.extend_from_slice(&chunk[..n.min(room)]);
            if n > room {
                break;
            }
        }
        let _ = done.send(());
    });
    (buf, ended)
}

/// Runs the manager's CLI, within one deadline for it and its output. The output stays in memory (never in a
/// file) and is wiped once read. `path`: `None` takes the login shell's `PATH` and the manager's variables.
pub(crate) fn resolve(uri: &str, path: Option<&OsStr>) -> Result<Secret> {
    let (program, args) = command(uri)?;
    let mut cmd = Command::new(program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match path {
        Some(path) => {
            cmd.env("PATH", path);
        }
        None => {
            if let Some(path) = termf_core::shell::login_path() {
                cmd.env("PATH", path);
            }
            let missing: Vec<&str> = variables(program)
                .iter()
                .copied()
                .filter(|name| std::env::var_os(name).is_none())
                .collect();
            cmd.envs(termf_core::shell::login_vars(&missing));
        }
    }
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => anyhow!("{program} is not installed or not on the PATH"),
        _ => anyhow!("cannot run {program}: {e}"),
    })?;
    let (out, out_done) = drain(child.stdout.take().expect("piped"), MAX_VALUE);
    let (err, err_done) = drain(child.stderr.take().expect("piped"), 64 << 10);
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{program} did not answer within {} s", TIMEOUT.as_secs());
        }
        thread::sleep(Duration::from_millis(50));
    };
    // A helper it left running (an agent caching the session) may keep the pipes open: what the CLI printed
    // before it ended is read by then.
    for done in [&out_done, &err_done] {
        let _ = done.recv_timeout(GRACE.min(deadline.saturating_duration_since(Instant::now())));
    }
    let out = std::mem::take(&mut *out.lock().unwrap());
    let err = std::mem::take(&mut *err.lock().unwrap());
    if !status.success() {
        let why = mask_text(&tail(&String::from_utf8_lossy(&err)));
        bail!("{program} could not read {uri} ({status}): {why}");
    }
    let text = std::str::from_utf8(&out)
        .map_err(|_| anyhow!("{program} gave a value that is not text"))?;
    let text = text.strip_suffix('\n').unwrap_or(text);
    let text = text.strip_suffix('\r').unwrap_or(text);
    ensure!(!text.is_empty(), "{program} gave an empty value for {uri}");
    ensure!(
        text.len() <= MAX_VALUE,
        "{program} gave a value longer than 64 KiB"
    );
    Ok(Secret::new(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_references_into_commands() {
        let run = |uri| command(uri).map(|(p, a)| format!("{p} {}", a.join(" ")));
        assert_eq!(
            run("op://Private/db/password").unwrap(),
            "op read op://Private/db/password"
        );
        assert_eq!(
            run("op://Private/My item/creds/password").unwrap(),
            "op read op://Private/My item/creds/password"
        );
        assert_eq!(run("bw://acme-db").unwrap(), "bw get password acme-db");
        assert_eq!(
            run("bw://acme-db/username").unwrap(),
            "bw get username acme-db"
        );
        assert_eq!(
            run("doppler://acme/prd/DATABASE_URL").unwrap(),
            "doppler secrets get DATABASE_URL --plain --project=acme --config=prd"
        );
        assert_eq!(
            run("aws-sm://prod/acme/db").unwrap(),
            "aws secretsmanager get-secret-value --secret-id=prod/acme/db --query SecretString --output text"
        );
        assert_eq!(
            run("vault://secret/acme/db#password").unwrap(),
            "vault kv get -field=password secret/acme/db"
        );
        for bad in [
            "op://Private/db",
            "op://Private/--account=x/db",
            "bw://-x",
            "bw://item/secret",
            "doppler://acme/prd",
            "aws-sm://--endpoint-url=http://evil",
            "vault://secret/acme/db",
            "vault://secret/acme/db#-x",
            "vault://-address=x#f",
            "https://example.com/secret",
            "op://a//b",
            "op://a/b/c\nd",
            "plain text",
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn names_the_variables_each_manager_needs() {
        for (uri, needed) in [
            ("op://a/b/c", "OP_SERVICE_ACCOUNT_TOKEN"),
            ("bw://item", "BW_SESSION"),
            ("doppler://p/c/N", "DOPPLER_TOKEN"),
            ("aws-sm://id", "AWS_PROFILE"),
            ("aws-sm://id", "AWS_REGION"),
            ("vault://p#f", "VAULT_ADDR"),
            ("vault://p#f", "VAULT_TOKEN"),
        ] {
            let (program, _) = command(uri).unwrap();
            assert!(variables(program).contains(&needed), "{uri}: {needed}");
        }
        assert!(variables("sh").is_empty());
    }
}
