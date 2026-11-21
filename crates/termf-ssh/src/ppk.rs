//! PuTTY's `.ppk` key files (PuTTY manual, appendix C): written in version 3, read in versions 2 and 3.

use aes::Aes256;
use anyhow::{Context, Result, anyhow, bail, ensure};
use base64ct::{Base64, Encoding};
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use ssh_key::private::{KeypairData, RsaKeypair, RsaPrivateKey};
use ssh_key::public::KeyData;
use ssh_key::rand_core::{OsRng, RngCore};
use ssh_key::{Mpint, PublicKey};
use zeroize::Zeroizing;

use crate::formats::{Curve, Locked, Secret, ssh_spki};

// PuTTYgen's defaults.
const ARGON2_MEMORY: u32 = 8192;
const ARGON2_PASSES: u32 = 13;

fn put_string(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn get_string<'a>(input: &mut &'a [u8]) -> Result<&'a [u8]> {
    let invalid = || anyhow!("invalid PuTTY key");
    let (len, rest) = input.split_first_chunk::<4>().ok_or_else(invalid)?;
    let len = u32::from_be_bytes(*len) as usize;
    ensure!(rest.len() >= len, "invalid PuTTY key");
    let (value, rest) = rest.split_at(len);
    *input = rest;
    Ok(value)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>> {
    ensure!(text.len().is_multiple_of(2), "invalid PuTTY key");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| anyhow!("invalid PuTTY key")))
        .collect()
}

fn lines(out: &mut String, bytes: &[u8]) {
    let text = Zeroizing::new(Base64::encode_string(bytes));
    let chunks: Vec<&str> = text
        .as_bytes()
        .chunks(64)
        .map(|c| std::str::from_utf8(c).expect("base64 is ASCII"))
        .collect();
    out.push_str(&chunks.len().to_string());
    out.push('\n');
    for chunk in chunks {
        out.push_str(chunk);
        out.push('\n');
    }
}

fn private_blob(key: &KeypairData) -> Result<Zeroizing<Vec<u8>>> {
    let mut blob = Zeroizing::new(Vec::with_capacity(4096));
    match key {
        // A fixed-length little-endian integer: the 32-byte seed as it is.
        KeypairData::Ed25519(pair) => {
            put_string(&mut blob, Zeroizing::new(pair.private.to_bytes()).as_ref())
        }
        KeypairData::Ecdsa(pair) => put_string(
            &mut blob,
            Mpint::from_positive_bytes(pair.private_key_bytes())?.as_bytes(),
        ),
        KeypairData::Rsa(pair) => {
            for part in [
                &pair.private.d,
                &pair.private.p,
                &pair.private.q,
                &pair.private.iqmp,
            ] {
                put_string(&mut blob, part.as_bytes());
            }
        }
        _ => bail!("PuTTY keys here are Ed25519, ECDSA or RSA"),
    }
    Ok(blob)
}

fn mac_input(
    algorithm: &str,
    encryption: &str,
    comment: &str,
    public: &[u8],
    private: &[u8],
) -> Zeroizing<Vec<u8>> {
    let mut data = Zeroizing::new(Vec::with_capacity(private.len() + public.len() + 256));
    for part in [
        algorithm.as_bytes(),
        encryption.as_bytes(),
        comment.as_bytes(),
        public,
        private,
    ] {
        put_string(&mut data, part);
    }
    data
}

/// Cipher key, IV and MAC key, from Argon2 (version 3).
fn argon2(
    kind: argon2::Algorithm,
    passphrase: &str,
    salt: &[u8],
    memory: u32,
    passes: u32,
    lanes: u32,
) -> Result<Zeroizing<[u8; 80]>> {
    // Bounds keep a crafted file from asking for gigabytes.
    ensure!(
        memory <= 1 << 20 && passes <= 1000 && lanes <= 64,
        "unreasonable Argon2 parameters"
    );
    let params =
        argon2::Params::new(memory, passes, lanes, Some(80)).map_err(|e| anyhow!("Argon2: {e}"))?;
    let mut out = Zeroizing::new([0u8; 80]);
    argon2::Argon2::new(kind, argon2::Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, out.as_mut())
        .map_err(|e| anyhow!("Argon2: {e}"))?;
    Ok(out)
}

pub(crate) fn encode(
    key: &KeypairData,
    comment: &str,
    passphrase: Option<&str>,
) -> Result<Zeroizing<String>> {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    encode_with(key, comment, passphrase, &salt)
}

fn encode_with(
    key: &KeypairData,
    comment: &str,
    passphrase: Option<&str>,
    salt: &[u8; 16],
) -> Result<Zeroizing<String>> {
    let algorithm = key.algorithm()?.as_str().to_owned();
    let public = PublicKey::new(KeyData::try_from(key)?, "").to_bytes()?;
    let mut private = private_blob(key)?;
    let encryption = if passphrase.is_some() {
        "aes256-cbc"
    } else {
        "none"
    };
    let keys = match passphrase {
        Some(passphrase) => {
            // PuTTY pads with the blob's own SHA-1, so its output is reproducible.
            let pad = (16 - private.len() % 16) % 16;
            let hash = Sha1::digest(private.as_slice());
            private.extend_from_slice(&hash[..pad]);
            Some(argon2(
                argon2::Algorithm::Argon2id,
                passphrase,
                salt,
                ARGON2_MEMORY,
                ARGON2_PASSES,
                1,
            )?)
        }
        None => None,
    };
    let mac_key = keys.as_ref().map_or(&[][..], |k| &k[48..]);
    let mut mac = Hmac::<Sha256>::new_from_slice(mac_key).expect("HMAC takes any key");
    mac.update(&mac_input(
        &algorithm, encryption, comment, &public, &private,
    ));
    let mac = mac.finalize().into_bytes();
    if let Some(keys) = &keys {
        let len = private.len();
        cbc::Encryptor::<Aes256>::new(keys[..32].into(), keys[32..48].into())
            .encrypt_padded_mut::<NoPadding>(&mut private, len)
            .map_err(|_| anyhow!("encryption failed"))?;
    }
    let mut out = Zeroizing::new(String::with_capacity(
        private.len() * 2 + public.len() * 2 + 512,
    ));
    out.push_str(&format!(
        "PuTTY-User-Key-File-3: {algorithm}\nEncryption: {encryption}\nComment: {comment}\nPublic-Lines: "
    ));
    lines(&mut out, &public);
    if keys.is_some() {
        out.push_str(&format!(
            "Key-Derivation: Argon2id\nArgon2-Memory: {ARGON2_MEMORY}\nArgon2-Passes: {ARGON2_PASSES}\n\
             Argon2-Parallelism: 1\nArgon2-Salt: {}\n",
            hex(salt)
        ));
    }
    out.push_str("Private-Lines: ");
    lines(&mut out, &private);
    out.push_str(&format!("Private-MAC: {}\n", hex(&mac)));
    Ok(out)
}

struct File<'a> {
    version: u8,
    algorithm: &'a str,
    encryption: &'a str,
    comment: &'a str,
    public: Vec<u8>,
    fields: Vec<(&'a str, &'a str)>,
    private: Zeroizing<Vec<u8>>,
    mac: &'a str,
}

fn field<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Result<(&'a str, &'a str)> {
    let line = lines.next().context("truncated PuTTY key")?;
    line.split_once(": ").context("invalid PuTTY key")
}

fn read(text: &str) -> Result<File<'_>> {
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    let (header, algorithm) = field(&mut lines)?;
    let version = match header {
        "PuTTY-User-Key-File-2" => 2,
        "PuTTY-User-Key-File-3" => 3,
        _ => bail!("unsupported PuTTY key version: 2 and 3 only"),
    };
    let mut fields = Vec::new();
    let mut public = None;
    let mut private = None;
    loop {
        let (name, value) = field(&mut lines)?;
        match name {
            "Public-Lines" | "Private-Lines" => {
                let count: usize = value.parse().map_err(|_| anyhow!("invalid PuTTY key"))?;
                ensure!(count <= 1000, "invalid PuTTY key");
                let mut text = Zeroizing::new(String::new());
                for _ in 0..count {
                    text.push_str(lines.next().context("truncated PuTTY key")?.trim());
                }
                let bytes = Zeroizing::new(
                    Base64::decode_vec(&text).map_err(|_| anyhow!("invalid PuTTY key"))?,
                );
                if name == "Public-Lines" {
                    public = Some(bytes.to_vec());
                } else {
                    private = Some(bytes);
                }
            }
            "Private-MAC" => {
                let field = |n| fields.iter().find(|(k, _)| *k == n).map(|(_, v)| *v);
                return Ok(File {
                    version,
                    algorithm,
                    encryption: field("Encryption").context("invalid PuTTY key")?,
                    comment: field("Comment").unwrap_or_default(),
                    public: public.context("invalid PuTTY key")?,
                    private: private.context("invalid PuTTY key")?,
                    mac: value,
                    fields,
                });
            }
            _ => fields.push((name, value)),
        }
    }
}

/// The public key of a `.ppk`, which is stored in clear.
pub(crate) fn public(text: &str) -> Result<KeyData> {
    Ok(PublicKey::from_bytes(&read(text)?.public)
        .map_err(|_| anyhow!("invalid PuTTY key"))?
        .key_data()
        .clone())
}

/// The key, its comment, and whether a passphrase protected it.
pub(crate) fn decode(text: &str, passphrase: Option<&str>) -> Result<(Secret, String, bool)> {
    let mut file = read(text)?;
    let encrypted = match file.encryption {
        "none" => false,
        "aes256-cbc" => true,
        _ => bail!("unsupported PuTTY encryption: aes256-cbc only"),
    };
    let passphrase = match encrypted {
        true => passphrase
            .filter(|p| !p.is_empty())
            .ok_or(Locked::NeedsPassphrase)?,
        false => "",
    };
    let field = |name: &str| {
        file.fields
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| *v)
    };
    // (cipher key, IV, MAC key)
    let (cipher, iv, mac_key): (Zeroizing<Vec<u8>>, [u8; 16], Zeroizing<Vec<u8>>) =
        if file.version == 3 {
            if encrypted {
                let kind = match field("Key-Derivation") {
                    Some("Argon2id") => argon2::Algorithm::Argon2id,
                    Some("Argon2i") => argon2::Algorithm::Argon2i,
                    Some("Argon2d") => argon2::Algorithm::Argon2d,
                    _ => bail!("unsupported PuTTY key derivation"),
                };
                let number = |name| -> Result<u32> {
                    field(name)
                        .and_then(|v| v.parse().ok())
                        .context("invalid PuTTY key")
                };
                let salt = unhex(field("Argon2-Salt").context("invalid PuTTY key")?)?;
                let keys = argon2(
                    kind,
                    passphrase,
                    &salt,
                    number("Argon2-Memory")?,
                    number("Argon2-Passes")?,
                    number("Argon2-Parallelism")?,
                )?;
                (
                    Zeroizing::new(keys[..32].to_vec()),
                    keys[32..48].try_into()?,
                    Zeroizing::new(keys[48..].to_vec()),
                )
            } else {
                (
                    Zeroizing::new(Vec::new()),
                    [0; 16],
                    Zeroizing::new(Vec::new()),
                )
            }
        } else {
            let sha1 = |prefix: &[u8]| {
                let mut hash = Sha1::new();
                hash.update(prefix);
                hash.update(passphrase.as_bytes());
                hash.finalize()
            };
            let mut cipher = Zeroizing::new(sha1(&[0, 0, 0, 0]).to_vec());
            cipher.extend_from_slice(&sha1(&[0, 0, 0, 1]));
            cipher.truncate(32);
            (
                cipher,
                [0; 16],
                Zeroizing::new(sha1(b"putty-private-key-file-mac-key").to_vec()),
            )
        };
    if encrypted {
        ensure!(file.private.len().is_multiple_of(16), "invalid PuTTY key");
        cbc::Decryptor::<Aes256>::new(cipher[..].into(), (&iv).into())
            .decrypt_padded_mut::<NoPadding>(&mut file.private)
            .map_err(|_| anyhow!("invalid PuTTY key"))?;
    }
    let data = mac_input(
        file.algorithm,
        file.encryption,
        file.comment,
        &file.public,
        &file.private,
    );
    let verified = match file.version {
        3 => {
            let mut mac = Hmac::<Sha256>::new_from_slice(&mac_key).expect("HMAC takes any key");
            mac.update(&data);
            mac.verify_slice(&unhex(file.mac)?).is_ok()
        }
        _ => {
            let mut mac = Hmac::<Sha1>::new_from_slice(&mac_key).expect("HMAC takes any key");
            mac.update(&data);
            mac.verify_slice(&unhex(file.mac)?).is_ok()
        }
    };
    if !verified {
        if encrypted {
            return Err(Locked::WrongPassphrase.into());
        }
        bail!("this PuTTY key is corrupted (its MAC does not match)");
    }
    let public = PublicKey::from_bytes(&file.public).map_err(|_| anyhow!("invalid PuTTY key"))?;
    let mut input = &file.private[..];
    let mut next =
        || Mpint::from_bytes(get_string(&mut input)?).map_err(|_| anyhow!("invalid PuTTY key"));
    let secret = match public.key_data() {
        KeyData::Ed25519(_) => {
            let seed: [u8; 32] = get_string(&mut input)?
                .try_into()
                .map_err(|_| anyhow!("invalid PuTTY key"))?;
            Secret::Ed25519(Zeroizing::new(seed))
        }
        KeyData::Ecdsa(key) => {
            let scalar = next()?;
            let scalar = scalar.as_positive_bytes().unwrap_or_default();
            let size = key.as_sec1_bytes().len() / 2;
            ensure!(scalar.len() <= size, "invalid PuTTY key");
            let mut d = Zeroizing::new(vec![0u8; size]);
            d[size - scalar.len()..].copy_from_slice(scalar);
            let curve = match key.curve() {
                ssh_key::EcdsaCurve::NistP256 => Curve::P256,
                ssh_key::EcdsaCurve::NistP384 => Curve::P384,
                ssh_key::EcdsaCurve::NistP521 => Curve::P521,
            };
            Secret::Ec(curve, d)
        }
        KeyData::Rsa(key) => {
            let (d, p, q, iqmp) = (next()?, next()?, next()?, next()?);
            let pair = RsaKeypair {
                public: key.clone(),
                private: RsaPrivateKey { d, iqmp, p, q },
            };
            Secret::from_ssh(&KeypairData::Rsa(pair))?
        }
        _ => bail!("unsupported PuTTY key type: Ed25519, ECDSA or RSA only"),
    };
    ensure!(
        secret.spki()? == ssh_spki(public.key_data())?,
        "this PuTTY key's halves do not match"
    );
    Ok((secret, file.comment.to_owned(), encrypted))
}

#[cfg(test)]
mod tests {
    use super::*;

    // PuTTY's own vectors (test/cryptsuite.py, testPPKLoadSave), passphrase "test-passphrase".
    const CLEAR: &str = "PuTTY-User-Key-File-3: ssh-ed25519
Encryption: none
Comment: ed25519-key-20200105
Public-Lines: 2
AAAAC3NzaC1lZDI1NTE5AAAAIHJCszOHaI9X/yGLtjn22f0hO6VPMQDVtctkym6F
JH1W
Private-Lines: 1
AAAAIGvvIpl8jyqn8Xufkw6v3FnEGtXF3KWw55AP3/AGEBpY
Private-MAC: 816c84093fc4877e8411b8e5139c5ce35d8387a2630ff087214911d67417a54d
";
    const ENCRYPTED: &str = "PuTTY-User-Key-File-3: ssh-ed25519
Encryption: aes256-cbc
Comment: ed25519-key-20200105
Public-Lines: 2
AAAAC3NzaC1lZDI1NTE5AAAAIHJCszOHaI9X/yGLtjn22f0hO6VPMQDVtctkym6F
JH1W
Key-Derivation: Argon2id
Argon2-Memory: 8192
Argon2-Passes: 13
Argon2-Parallelism: 1
Argon2-Salt: 37c3911bfefc8c1d11ec579627d2b3d9
Private-Lines: 1
amviz4sVUBN64jLO3gt4HGXJosUArghc4Soi7aVVLb2Tir5Baj0OQClorycuaPRd
Private-MAC: 6f5e588e475e55434106ec2c3569695b03f423228b44993a9e97d52ffe7be5a8
";
    const V2_CLEAR: &str = "PuTTY-User-Key-File-2: ssh-ed25519
Encryption: none
Comment: ed25519-key-20200105
Public-Lines: 2
AAAAC3NzaC1lZDI1NTE5AAAAIHJCszOHaI9X/yGLtjn22f0hO6VPMQDVtctkym6F
JH1W
Private-Lines: 1
AAAAIGvvIpl8jyqn8Xufkw6v3FnEGtXF3KWw55AP3/AGEBpY
Private-MAC: 2a629acfcfbe28488a1ba9b6948c36406bc28422
";
    const V2_ENCRYPTED: &str = "PuTTY-User-Key-File-2: ssh-ed25519
Encryption: aes256-cbc
Comment: ed25519-key-20200105
Public-Lines: 2
AAAAC3NzaC1lZDI1NTE5AAAAIHJCszOHaI9X/yGLtjn22f0hO6VPMQDVtctkym6F
JH1W
Private-Lines: 1
4/jKlTgC652oa9HLVGrMjHZw7tj0sKRuZaJPOuLhGTvb25Jzpcqpbi+Uf+y+uo+Z
Private-MAC: 5b1f6f4cc43eb0060d2c3e181bc0129343adba2b
";

    #[test]
    fn reads_and_writes_putty_vectors() {
        let (secret, comment, encrypted) = decode(CLEAR, None).unwrap();
        assert_eq!(
            (comment.as_str(), encrypted),
            ("ed25519-key-20200105", false)
        );
        for (text, encrypted) in [(ENCRYPTED, true), (V2_CLEAR, false), (V2_ENCRYPTED, true)] {
            let (other, _, was) = decode(text, Some("test-passphrase")).unwrap();
            assert_eq!(was, encrypted);
            assert_eq!(other.spki().unwrap(), secret.spki().unwrap());
        }
        let pair = secret.to_ssh().unwrap().unwrap();
        assert_eq!(
            decode(ENCRYPTED, Some("nope"))
                .unwrap_err()
                .downcast::<Locked>()
                .unwrap(),
            Locked::WrongPassphrase
        );
        assert_eq!(
            decode(ENCRYPTED, None)
                .unwrap_err()
                .downcast::<Locked>()
                .unwrap(),
            Locked::NeedsPassphrase
        );
        // Byte for byte what PuTTYgen writes, given the same salt.
        let salt: [u8; 16] = unhex("37c3911bfefc8c1d11ec579627d2b3d9")
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(*encode_with(&pair, &comment, None, &salt).unwrap(), CLEAR);
        assert_eq!(
            *encode_with(&pair, &comment, Some("test-passphrase"), &salt).unwrap(),
            ENCRYPTED
        );
    }

    #[test]
    fn a_tampered_file_is_refused() {
        let tampered = CLEAR.replace("AAAAIGvvIpl8", "AAAAIGvvIpl9");
        let error = format!("{:#}", decode(&tampered, None).unwrap_err());
        assert!(error.contains("MAC"), "{error}");
        assert!(!error.contains("AAAAIGvv"), "{error}");
    }
}
