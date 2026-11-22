//! Keys of any format: generate, convert, reveal and test (architecture §12.7). An SSH key in OpenSSH
//! format goes to `~/.ssh`; any other key goes only to the folder the user picked, or is only returned.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use ssh_key::{HashAlg, PrivateKey};
use zeroize::Zeroizing;

use crate::formats::{self, Output, Secret};
pub use crate::formats::{Algo, Format, Locked, Usage};
use crate::{MAX_KEY_BYTES, NOT_KEYS, Ssh, create_private_dir};

/// What to make of a key. No `Debug`: it holds the passphrase.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub usage: Usage,
    pub format: Format,
    /// The file name, without extension.
    pub name: String,
    #[serde(default)]
    pub comment: String,
    pub passphrase: Option<String>,
    /// The folder the user chose for a key that is not an OpenSSH one for SSH; `None` writes nothing.
    pub dir: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Generated {
    /// `Ed25519`, `ECDSA P-256`, `RSA 4096`, `X25519`.
    pub algorithm: String,
    /// `SHA256:…`, for a key SSH can use.
    pub fingerprint: Option<String>,
    /// The JWK thumbprint (RFC 7638), also the `kid` of a JWK.
    pub kid: String,
    pub files: Vec<KeyFile>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct KeyFile {
    pub name: String,
    /// Where it was written; `None` when the key was only shown.
    pub path: Option<String>,
    pub secret: bool,
    /// Always for a public file; for a private one only when nothing was written.
    pub text: Option<String>,
}

#[derive(Serialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum CheckKind {
    /// The private key could be read.
    Read,
    /// Only for an encrypted key.
    Passphrase,
    /// The public file belongs to the private key.
    Pair,
    /// A message signed by the private key verifies with the public one.
    Signature,
    /// X25519, which cannot sign: a key agreement gives both sides the same secret.
    Agreement,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub kind: CheckKind,
    pub ok: bool,
    /// The algorithm (`read`) or the public file's name (`pair`).
    pub detail: Option<String>,
    /// Why it failed, in English.
    pub error: Option<String>,
}

fn check(kind: CheckKind, ok: bool, detail: Option<String>, error: Option<String>) -> Check {
    Check {
        kind,
        ok,
        detail,
        error,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A key file: small, text. Its content never appears in an error.
fn read_key(path: &Path) -> Result<Zeroizing<String>> {
    let meta = fs::metadata(path).with_context(|| format!("cannot open {}", path.display()))?;
    ensure!(
        meta.is_file() && meta.len() <= MAX_KEY_BYTES,
        "{} is not a key file",
        path.display()
    );
    let bytes =
        Zeroizing::new(fs::read(path).with_context(|| format!("cannot read {}", path.display()))?);
    let text =
        std::str::from_utf8(&bytes).map_err(|_| anyhow!("{} is not a key file", path.display()))?;
    Ok(Zeroizing::new(text.to_owned()))
}

/// The public file next to a private one, as `export` names them: `id.pub`, `api.prod.pub.pem` for
/// `api.prod.pem`, `k.pub.jwk.json` for `k.jwk.json`, `id.pub` for `id.ppk`; else `<file>.pub`.
fn sibling_public(private: &Path) -> Option<PathBuf> {
    const SUFFIXES: [(&str, &str); 4] = [
        (".jwks.json", ".pub.jwks.json"),
        (".jwk.json", ".pub.jwk.json"),
        (".pem", ".pub.pem"),
        (".ppk", ".pub"),
    ];
    let name = file_name(private);
    let mut candidates: Vec<String> = SUFFIXES
        .iter()
        .filter_map(|(private, public)| {
            name.strip_suffix(private)
                .map(|stem| format!("{stem}{public}"))
        })
        .collect();
    candidates.push(format!("{name}.pub"));
    candidates
        .into_iter()
        .map(|c| private.with_file_name(c))
        .find(|p| p != private && p.is_file())
}

impl Ssh {
    /// A key file the UI may read: inside `~/.ssh` (a symlink leading out is refused), or one of
    /// `allowed`, the canonical paths TermF wrote or the user picked in a dialog this session.
    pub fn readable(&self, path: &str, allowed: &HashSet<PathBuf>) -> Result<PathBuf> {
        if let Ok(inside) = self.resolve(path) {
            return Ok(inside);
        }
        let canonical = Path::new(path)
            .canonicalize()
            .with_context(|| format!("cannot open {path}"))?;
        ensure!(
            allowed.contains(&canonical),
            "{path} is outside {} and was not picked in this session: choose it with “Other file…”",
            self.dir.display()
        );
        Ok(canonical)
    }

    /// A new key of any algorithm, in `spec`'s format.
    pub fn create(&self, algorithm: Algo, spec: &Spec) -> Result<Generated> {
        self.export(&Secret::random(algorithm)?, spec)
    }

    /// The key at `path` (see `readable`), in another format; an empty comment keeps the key's own.
    pub fn convert(
        &self,
        path: &str,
        passphrase: Option<&str>,
        spec: &Spec,
        allowed: &HashSet<PathBuf>,
    ) -> Result<Generated> {
        let parsed = formats::parse(&read_key(&self.readable(path, allowed)?)?, passphrase)?;
        let mut spec = spec.clone();
        if spec.comment.trim().is_empty() {
            spec.comment = parsed.comment;
        }
        self.export(&parsed.secret, &spec)
    }

    /// The private key in clear, on an explicit request: the file as it is, or decrypted with
    /// `passphrase` and re-encoded in its own format. A key TermF cannot read (DSA, `-sk`) is shown
    /// as it is when it is not encrypted.
    pub fn reveal(
        &self,
        path: &str,
        passphrase: Option<&str>,
        allowed: &HashSet<PathBuf>,
    ) -> Result<Zeroizing<String>> {
        let text = read_key(&self.readable(path, allowed)?)?;
        let parsed = match formats::parse(&text, passphrase) {
            Ok(parsed) => parsed,
            Err(_) if formats::plain_private(&text) => return Ok(text),
            Err(e) => return Err(e),
        };
        if !parsed.encrypted {
            return Ok(text);
        }
        parsed
            .secret
            .encode(parsed.format, &parsed.comment, None, Usage::Ssh)?
            .into_iter()
            .find(|o| o.secret)
            .map(|o| o.text)
            .context("no private key to show")
    }

    /// Opens the key, then checks its public file (`public`, else the one next to it) and a signature.
    pub fn test(
        &self,
        path: &str,
        passphrase: Option<&str>,
        public: Option<&str>,
        allowed: &HashSet<PathBuf>,
    ) -> Result<Vec<Check>> {
        let path = self.readable(path, allowed)?;
        let public = public.map(|p| self.readable(p, allowed)).transpose()?;
        let text = read_key(&path)?;
        let parsed = match formats::parse(&text, passphrase) {
            Ok(parsed) => parsed,
            Err(e) => {
                let kind = match e.downcast_ref::<Locked>() {
                    Some(_) => CheckKind::Passphrase,
                    None => CheckKind::Read,
                };
                return Ok(vec![check(kind, false, None, Some(format!("{e:#}")))]);
            }
        };
        let secret = &parsed.secret;
        let mut checks = vec![check(CheckKind::Read, true, Some(secret.label()), None)];
        if parsed.encrypted {
            checks.push(check(CheckKind::Passphrase, true, None, None));
        }
        if let Some(public) = public.or_else(|| sibling_public(&path)) {
            let name = Some(file_name(&public));
            let result = read_key(&public).and_then(|t| formats::parse_public(&t));
            checks.push(match result {
                Ok(der) => check(CheckKind::Pair, der == secret.spki()?, name, None),
                Err(e) => check(CheckKind::Pair, false, name, Some(format!("{e:#}"))),
            });
        }
        let kind = match secret {
            Secret::X25519(_) => CheckKind::Agreement,
            _ => CheckKind::Signature,
        };
        checks.push(match secret.self_test() {
            Ok(ok) => check(kind, ok, None, None),
            Err(e) => check(kind, false, None, Some(format!("{e:#}"))),
        });
        Ok(checks)
    }

    fn export(&self, secret: &Secret, spec: &Spec) -> Result<Generated> {
        let name = spec.name.as_str();
        ensure!(
            !name.is_empty()
                && name.len() <= 100
                && !name.starts_with('.')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-@+".contains(c)),
            "invalid key name {name:?}: letters, digits and . _ - @ + only"
        );
        ensure!(
            !spec.comment.contains(|c: char| c.is_control()),
            "the comment must fit on one line"
        );
        let outputs = secret.encode(
            spec.format,
            &spec.comment,
            spec.passphrase.as_deref(),
            spec.usage,
        )?;
        let dir = if spec.usage == Usage::Ssh && spec.format == Format::Openssh {
            ensure!(
                !name.ends_with(".pub") && !NOT_KEYS.contains(&name),
                "invalid key name {name:?}: letters, digits and . _ - @ + only"
            );
            create_private_dir(&self.dir)?;
            Some(self.dir.clone())
        } else {
            spec.dir
                .as_deref()
                .map(|dir| {
                    let dir = PathBuf::from(dir);
                    ensure!(
                        dir.is_absolute() && dir.is_dir(),
                        "{} is not a folder",
                        dir.display()
                    );
                    Ok(dir)
                })
                .transpose()?
        };
        // Everything that can fail comes before the files, so a failure leaves none behind.
        let fingerprint = secret
            .to_ssh()?
            .map(|pair| {
                PrivateKey::new(pair, "")
                    .map(|k| k.public_key().fingerprint(HashAlg::Sha256).to_string())
            })
            .transpose()?;
        let kid = secret.thumbprint()?;
        let written = match &dir {
            Some(dir) => write_all(dir, name, &outputs, fill)?,
            None => Vec::new(),
        };
        Ok(Generated {
            algorithm: secret.label(),
            fingerprint,
            kid,
            files: outputs
                .into_iter()
                .enumerate()
                .map(|(i, o)| {
                    let path = written.get(i).map(|p| p.to_string_lossy().into_owned());
                    KeyFile {
                        name: format!("{name}{}", o.suffix),
                        text: (!o.secret || path.is_none()).then(|| o.text.to_string()),
                        path,
                        secret: o.secret,
                    }
                })
                .collect(),
        })
    }

    /// `ssh -T host`, with only `key` offered when given (it must be in the agent if it has a passphrase).
    pub fn test_host(&self, host: &str, key: Option<&str>) -> Result<crate::HostTest> {
        let key = key
            .map(|k| self.pair(k)?.0.context("the private key is missing"))
            .transpose()?;
        crate::tools::test_host(host, key.as_deref())
    }
}

fn fill(file: &mut fs::File, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()
}

/// Every file or none: nothing is overwritten, and a failure removes every file this call created,
/// the half-written one included. `fill` writes a file's content.
fn write_all(
    dir: &Path,
    name: &str,
    outputs: &[Output],
    fill: impl Fn(&mut fs::File, &[u8]) -> std::io::Result<()>,
) -> Result<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = outputs
        .iter()
        .map(|o| dir.join(format!("{name}{}", o.suffix)))
        .collect();
    for path in &paths {
        ensure!(
            fs::symlink_metadata(path).is_err(),
            "{} already exists",
            path.display()
        );
    }
    let mut created: Vec<&PathBuf> = Vec::new();
    for (path, output) in paths.iter().zip(outputs) {
        let mode = if output.secret { 0o600 } else { 0o644 };
        let result = create_new(path, mode).and_then(|mut file| {
            created.push(path);
            fill(&mut file, output.text.as_bytes())
                .with_context(|| format!("cannot write {}", path.display()))
        });
        if let Err(e) = result {
            for done in &created {
                let _ = fs::remove_file(done);
            }
            return Err(e);
        }
    }
    Ok(paths)
}

#[cfg_attr(not(unix), allow(unused_variables))]
fn create_new(path: &Path, mode: u32) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    options
        .open(path)
        .with_context(|| format!("cannot create {}", path.display()))
}

#[cfg(test)]
pub(crate) mod tests;
