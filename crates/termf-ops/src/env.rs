//! A production `.env` (ARCHITECTURE §12.4): read into memory, its values kept apart and masked, each revealed
//! on its own; compared with a local one by the hashes of their values; one value written into a run's worktree,
//! readable by the user only, hidden from git by `info/exclude`, gone with the worktree when the run ends.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use termf_vault::Secret;

/// A key of a `.env`, its value, and the lines (`start..end`) that hold it.
struct Entry {
    key: String,
    value: String,
    lines: std::ops::Range<usize>,
}

/// Every entry of a `.env`, in the file's order. A quoted value goes on over the next lines up to its closing
/// quote; without one, it is the rest of its own line, and the next lines are read as entries.
fn entries(text: &str) -> Vec<Entry> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let start = at;
        at += 1;
        let line = lines[start].trim_start();
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let valid = key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
        if !valid {
            continue;
        }
        let rest = rest.trim_start();
        let plain = || match rest.find(" #") {
            Some(cut) => rest[..cut].trim_end().to_owned(),
            None => rest.trim_end().to_owned(),
        };
        let value = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => {
                let mut body = rest[1..].to_owned();
                let mut next = at;
                loop {
                    if let Some(end) = closing(&body, quote) {
                        at = next;
                        let value = &body[..end];
                        break if quote == '"' {
                            unescape(value)
                        } else {
                            value.to_owned()
                        };
                    }
                    let Some(more) = lines.get(next) else {
                        break plain();
                    };
                    body.push('\n');
                    body.push_str(more);
                    next += 1;
                }
            }
            _ => plain(),
        };
        out.push(Entry {
            key: key.to_owned(),
            value,
            lines: start..at,
        });
    }
    out
}

/// The keys of a `.env` and their values, in the file's order; a key set twice keeps its last value.
pub fn parse(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for e in entries(text) {
        out.retain(|(k, _)| *k != e.key);
        out.push((e.key, e.value));
    }
    out
}

/// Where the quote that ends a value is: one that ends its line (a comment may follow), a `\"` not counted in
/// double quotes. A quote within a line is the value's, or the next key's opening one.
fn closing(body: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        match c {
            '\\' if quote == '"' && !escaped => escaped = true,
            c if c == quote && !escaped => {
                let after = body[i + 1..].split('\n').next().unwrap_or_default().trim();
                if after.is_empty() || after.starts_with('#') {
                    return Some(i);
                }
            }
            _ => escaped = false,
        }
    }
    None
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// A value as a `.env` line holds it: as it is when it can, else quoted.
fn quoted(value: &str) -> String {
    let plain = value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_-./:@+,=%".contains(c));
    if plain {
        value.to_owned()
    } else if !value.contains(['\'', '\n', '\r']) {
        format!("'{value}'")
    } else {
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        format!("\"{escaped}\"")
    }
}

/// How a key of production compares with the local `.env`.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// The same value on both sides.
    Same,
    Different,
    /// Only in production: missing here.
    Prod,
    /// Only here.
    Local,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Diff {
    pub key: String,
    pub state: State,
}

fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

/// Production's keys against the local file's, compared by the SHA-256 of their values: production's order,
/// then the keys only the local file has.
pub fn compare(prod: &[(String, Secret)], local: &[(String, String)]) -> Vec<Diff> {
    let local: Vec<(&str, [u8; 32])> = local.iter().map(|(k, v)| (k.as_str(), digest(v))).collect();
    let mut out: Vec<Diff> = prod
        .iter()
        .map(|(key, value)| {
            let state = match local.iter().find(|(k, _)| k == key) {
                Some((_, hash)) if *hash == digest(value.expose()) => State::Same,
                Some(_) => State::Different,
                None => State::Prod,
            };
            Diff {
                key: key.clone(),
                state,
            }
        })
        .collect();
    for (key, _) in &local {
        if !prod.iter().any(|(k, _)| k == key) {
            out.push(Diff {
                key: (*key).to_owned(),
                state: State::Local,
            });
        }
    }
    out
}

/// A relative path inside a folder, without `..`.
pub fn inside(root: &Path, file: &str) -> Result<std::path::PathBuf> {
    let relative = Path::new(file);
    ensure!(
        !file.is_empty()
            && relative.is_relative()
            && relative
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "{file}: a path inside the repository, without .."
    );
    Ok(root.join(relative))
}

fn git(dir: &Path, args: &[&str]) -> Result<(bool, String)> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .context("cannot run git")?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_owned(),
    ))
}

/// Writes `key` into the `.env` at the root of `worktree` (the line replaced, or added), readable by the user only,
/// and keeps that file out of git through `info/exclude`. Refused when git tracks it or still sees it.
pub fn write(worktree: &Path, key: &str, value: &Secret) -> Result<()> {
    let file = worktree.join(".env");
    let (tracked, _) = git(worktree, &["ls-files", "--error-unmatch", "--", ".env"])?;
    ensure!(
        !tracked,
        "{} is tracked by git: TermF writes no secret into it",
        file.display()
    );
    let (ok, prefix) = git(worktree, &["rev-parse", "--show-prefix"])?;
    ensure!(ok, "{} is not a git worktree", worktree.display());
    let (_, exclude) = git(worktree, &["rev-parse", "--git-path", "info/exclude"])?;
    let exclude = worktree.join(exclude);
    // Taken literally: a folder's name may hold what gitignore reads as a pattern.
    let prefix: String = prefix
        .chars()
        .flat_map(|c| ["\\*?[".contains(c).then_some('\\'), Some(c)])
        .flatten()
        .collect();
    let pattern = format!("/{prefix}.env");
    let text = match fs::read_to_string(&exclude) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", exclude.display())),
    };
    if !text.lines().any(|l| l.trim() == pattern) {
        fs::create_dir_all(exclude.parent().context("no parent folder")?)?;
        let newline = if text.is_empty() || text.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&exclude)
            .and_then(|mut f| writeln!(f, "{newline}{pattern}"))
            .with_context(|| format!("cannot write {}", exclude.display()))?;
    }
    let old = match fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", file.display())),
    };
    let line = format!("{key}={}", quoted(value.expose()));
    // Every line of the key's entries goes, a quoted value over several lines whole.
    let gone: Vec<std::ops::Range<usize>> = entries(&old)
        .into_iter()
        .filter(|e| e.key == key)
        .map(|e| e.lines)
        .collect();
    let mut lines: Vec<&str> = old
        .lines()
        .enumerate()
        .filter(|(i, _)| !gone.iter().any(|r| r.contains(i)))
        .map(|(_, l)| l)
        .collect();
    lines.push(&line);
    let text = termf_vault::Zeroizing::new(lines.join("\n") + "\n");
    let tmp = worktree.join(".env.termf.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options.open(&tmp).and_then(|mut out| {
        #[cfg(unix)]
        out.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        out.write_all(text.as_bytes())?;
        out.sync_all()
    });
    if let Err(e) = written.and_then(|()| fs::rename(&tmp, &file)) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("cannot write {}", file.display()));
    }
    let (ignored, _) = git(worktree, &["check-ignore", "-q", "--", ".env"])?;
    if !ignored {
        let _ = fs::remove_file(&file);
        bail!(
            "git would still see {} (a ! rule in a .gitignore?): TermF removed it",
            file.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dotenv_reads_as_its_keys_and_values() {
        let text = "# Acme, production\nexport DATABASE_URL=postgres://app:Pg-4f8a@db/acme\nAPP_ENV = production # where\n\nSTRIPE_KEY=\"sk_live_51H\\\"x\"\nPEM=\"-----BEGIN KEY-----\nabc\n-----END KEY-----\"\nSINGLE='a $b # c'\nnot a line\n1BAD=x\nAPP_ENV=prod\n";
        let parsed = parse(text);
        let keys: Vec<(&str, &str)> = parsed
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            keys,
            [
                ("DATABASE_URL", "postgres://app:Pg-4f8a@db/acme"),
                ("STRIPE_KEY", "sk_live_51H\"x"),
                ("PEM", "-----BEGIN KEY-----\nabc\n-----END KEY-----"),
                ("SINGLE", "a $b # c"),
                ("APP_ENV", "prod"),
            ]
        );
    }

    #[test]
    fn a_quote_never_closed_holds_its_own_line_only() {
        let parsed = parse("A=\"never closed\nB=2\nC='x y'\nD=\"also open # here\n");
        let keys: Vec<(&str, &str)> = parsed
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            keys,
            [
                ("A", "\"never closed"),
                ("B", "2"),
                ("C", "x y"),
                ("D", "\"also open")
            ]
        );
        let spans: Vec<_> = entries("A=\"x\ny\"\nB=1\n")
            .into_iter()
            .map(|e| (e.key, e.lines))
            .collect();
        assert_eq!(spans, [("A".to_owned(), 0..2), ("B".to_owned(), 2..3)]);
    }

    #[test]
    fn local_and_prod_compare_by_hash() {
        let prod = vec![
            ("A".to_owned(), Secret::new("same".into())),
            ("B".to_owned(), Secret::new("prod".into())),
            ("C".to_owned(), Secret::new("only prod".into())),
        ];
        let local = parse("A=same\nB=dev\nD=only here\n");
        let states: Vec<(String, State)> = compare(&prod, &local)
            .into_iter()
            .map(|d| (d.key, d.state))
            .collect();
        assert_eq!(
            states,
            [
                ("A".into(), State::Same),
                ("B".into(), State::Different),
                ("C".into(), State::Prod),
                ("D".into(), State::Local),
            ]
        );
    }

    #[test]
    fn values_round_trip_through_a_written_line() {
        for value in [
            "plain",
            "with space",
            "it's",
            "multi\nline \"q\" \\",
            "a'b\"c",
        ] {
            let line = format!("K={}\n", quoted(value));
            assert_eq!(parse(&line), [("K".to_owned(), value.to_owned())], "{line}");
        }
    }

    #[test]
    fn paths_stay_inside() {
        let root = Path::new("/repo");
        assert_eq!(
            inside(root, "apps/api/.env").unwrap(),
            root.join("apps/api/.env")
        );
        for bad in ["", "/etc/passwd", "../.env", "a/../../.env"] {
            assert!(inside(root, bad).is_err(), "{bad}");
        }
    }
}
