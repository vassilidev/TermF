//! Keys of any format: generate, convert, reveal and test (architecture §12.7). An SSH key in OpenSSH
//! format goes to `~/.ssh`; any other key goes only to the folder the user picked, or is only returned.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use ssh_key::{HashAlg, PrivateKey};
use zeroize::Zeroizing;

use crate::formats::{self, Output, Secret};
pub use crate::formats::{Algo, Format, Locked, Usage};
use crate::{MAX_KEY_BYTES, NOT_KEYS, Ssh, create_private_dir, write_new};

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

/// A key file the UI names: absolute, small, text. Its content never appears in an error.
fn read_key(path: &str) -> Result<Zeroizing<String>> {
    let path = Path::new(path);
    ensure!(
        path.is_absolute(),
        "{} is not an absolute path",
        path.display()
    );
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

/// The public file next to a private one: `id.pub`, `id.pub.pem` for `id.pem`, `id.pub` for `id.ppk`.
fn sibling_public(private: &Path) -> Option<PathBuf> {
    let name = file_name(private);
    let mut candidates = vec![format!("{name}.pub")];
    if let Some((stem, ext)) = name.split_once('.') {
        candidates.push(format!("{stem}.pub.{ext}"));
    }
    if let Some((stem, _)) = name.rsplit_once('.') {
        candidates.push(format!("{stem}.pub"));
    }
    candidates
        .into_iter()
        .map(|c| private.with_file_name(c))
        .find(|p| p != private && p.is_file())
}

impl Ssh {
    /// A new key of any algorithm, in `spec`'s format.
    pub fn create(&self, algorithm: Algo, spec: &Spec) -> Result<Generated> {
        self.export(&Secret::random(algorithm)?, spec)
    }

    /// The key at `path`, in another format; an empty comment keeps the key's own.
    pub fn convert(&self, path: &str, passphrase: Option<&str>, spec: &Spec) -> Result<Generated> {
        let parsed = formats::parse(&read_key(path)?, passphrase)?;
        let mut spec = spec.clone();
        if spec.comment.trim().is_empty() {
            spec.comment = parsed.comment;
        }
        self.export(&parsed.secret, &spec)
    }

    /// The private key in clear, on an explicit request: the file as it is, or decrypted with
    /// `passphrase` and re-encoded in its own format.
    pub fn reveal(&self, path: &str, passphrase: Option<&str>) -> Result<Zeroizing<String>> {
        let text = read_key(path)?;
        let parsed = formats::parse(&text, passphrase)?;
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
    ) -> Result<Vec<Check>> {
        let text = read_key(path)?;
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
        let public = match public {
            Some(public) => Some(PathBuf::from(public)),
            None if parsed.format == Format::Ppk => None,
            None => sibling_public(Path::new(path)),
        };
        if let Some(public) = public {
            let name = Some(file_name(&public));
            let result =
                read_key(&public.to_string_lossy()).and_then(|t| formats::parse_public(&t));
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
        let written = match &dir {
            Some(dir) => write_all(dir, name, &outputs)?,
            None => Vec::new(),
        };
        let fingerprint = secret
            .to_ssh()?
            .map(|pair| {
                PrivateKey::new(pair, "")
                    .map(|k| k.public_key().fingerprint(HashAlg::Sha256).to_string())
            })
            .transpose()?;
        Ok(Generated {
            algorithm: secret.label(),
            fingerprint,
            kid: secret.thumbprint()?,
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

/// Every file or none: nothing is overwritten, and a failure removes what this call created.
fn write_all(dir: &Path, name: &str, outputs: &[Output]) -> Result<Vec<PathBuf>> {
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
    for (i, (path, output)) in paths.iter().zip(outputs).enumerate() {
        let mode = if output.secret { 0o600 } else { 0o644 };
        if let Err(e) = write_new(path, output.text.as_bytes(), mode) {
            for done in &paths[..i] {
                let _ = fs::remove_file(done);
            }
            return Err(e);
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests;
