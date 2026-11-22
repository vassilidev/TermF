//! References to an external secret manager, resolved by the manager's own CLI each time they are used: the
//! vault never keeps what they resolve to.

use std::ffi::OsStr;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use termf_claude::mask::mask_text;
use termf_core::process::tail;
use zeroize::Zeroizing;

use crate::{MAX_VALUE, Secret};

/// Long enough to approve a 1Password or Bitwarden unlock prompt.
const TIMEOUT: Duration = Duration::from_secs(120);
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

/// Runs the manager's CLI. Its output stays in memory (never in a file), and is wiped once read.
pub(crate) fn resolve(uri: &str, path: Option<&OsStr>) -> Result<Secret> {
    let (program, args) = command(uri)?;
    let mut cmd = Command::new(program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => anyhow!("{program} is not installed or not on the PATH"),
        _ => anyhow!("cannot run {program}: {e}"),
    })?;
    let stdout = child.stdout.take().expect("piped");
    let stderr = child.stderr.take().expect("piped");
    let out = thread::spawn(move || {
        // Sized up front, so reading never leaves a copy behind in a freed buffer.
        let mut buf = Zeroizing::new(Vec::with_capacity(MAX_VALUE + 64));
        let _ = stdout.take(MAX_VALUE as u64 + 1).read_to_end(&mut buf);
        buf
    });
    let err = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.take(64 << 10).read_to_end(&mut buf);
        buf
    });
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
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
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
}
