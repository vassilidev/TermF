//! Password → Argon2id → key-encryption key, which wraps a random data key; the data key encrypts every file
//! with XChaCha20-Poly1305 and names it by a keyed hash of its path.

use anyhow::{Context, Result, anyhow, ensure};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

pub type Key = Zeroizing<[u8; 32]>;

const MAGIC: &[u8; 4] = b"TFS1";
const NONCE: usize = 24;
pub const KEY_ID: usize = 8;

pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).expect("the system has no source of randomness");
    bytes
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kdf {
    pub m: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for Kdf {
    /// RFC 9106's second recommended option (64 MiB, 3 passes), on one lane.
    fn default() -> Self {
        Self {
            m: 64 << 10,
            t: 3,
            p: 1,
        }
    }
}

pub fn derive(secret: &[u8], salt: &[u8], kdf: Kdf) -> Result<Key> {
    // The parameters come from the remote: bounded so a tampered file cannot exhaust memory or time.
    ensure!(
        kdf.m <= 256 << 10 && kdf.t <= 8 && kdf.p <= 4,
        "unexpected key derivation parameters"
    );
    let params = Params::new(kdf.m, kdf.t, kdf.p, Some(32)).map_err(|e| anyhow!("{e}"))?;
    let mut key = Zeroizing::new([0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(secret, salt, key.as_mut())
        .map_err(|e| anyhow!("{e}"))?;
    Ok(key)
}

pub fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Vec<u8> {
    let nonce: [u8; NONCE] = random();
    let cipher = XChaCha20Poly1305::new(key.into());
    let sealed = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .expect("encryption cannot fail with a valid key");
    [&nonce[..], &sealed].concat()
}

pub fn open(key: &[u8; 32], aad: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let (nonce, sealed) = data.split_at_checked(NONCE)?;
    XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(nonce), Payload { msg: sealed, aad })
        .ok()
}

fn mac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC takes any key size");
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

pub struct DataKeys {
    pub id: [u8; KEY_ID],
    content: Key,
    names: Key,
}

impl DataKeys {
    pub fn new(data_key: &[u8; 32], id: [u8; KEY_ID]) -> Self {
        let sub = |label: &[u8]| Zeroizing::new(mac(data_key, &[label]));
        Self {
            id,
            content: sub(b"termf-sync content"),
            names: sub(b"termf-sync names"),
        }
    }

    /// Hides a path (or a device id) from the remote; `domain` keeps files and devices apart.
    pub fn name(&self, domain: &str, id: &str) -> String {
        hex(&mac(
            self.names.as_ref(),
            &[domain.as_bytes(), &[0], id.as_bytes()],
        )[..16])
    }

    /// `name` is bound to the content, so a blob cannot be passed off as another one.
    pub fn seal_blob(&self, name: &str, plain: &[u8]) -> Vec<u8> {
        let header = [&MAGIC[..], &self.id].concat();
        let aad = [&header[..], name.as_bytes()].concat();
        [header, seal(&self.content, &aad, plain)].concat()
    }

    /// `Ok(None)`: a blob of another key generation (left over from a password change).
    pub fn open_blob(&self, name: &str, data: &[u8]) -> Result<Option<Vec<u8>>> {
        let header = data
            .get(..MAGIC.len() + KEY_ID)
            .filter(|h| h.starts_with(MAGIC))
            .with_context(|| format!("{name} is not TermF sync data"))?;
        if header[MAGIC.len()..] != self.id {
            return Ok(None);
        }
        let aad = [header, name.as_bytes()].concat();
        open(&self.content, &aad, &data[header.len()..])
            .map(Some)
            .with_context(|| format!("{name} is damaged or was not written by TermF"))
    }
}

pub fn is_blob(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(text: &str) -> Option<Vec<u8>> {
    text.len()
        .is_multiple_of(2)
        .then(|| {
            (0..text.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
                .collect()
        })
        .flatten()
}

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 160 random bits in Crockford base32, `XXXX-XXXX-…`: easy to copy by hand and to read back.
pub fn recovery_key() -> String {
    let bytes: [u8; 20] = random();
    let mut chars = Vec::new();
    for chunk in bytes.chunks(5) {
        let n = chunk.iter().fold(0u64, |n, b| (n << 8) | u64::from(*b));
        chars.extend(
            (0..8)
                .rev()
                .map(|i| CROCKFORD[((n >> (i * 5)) & 31) as usize] as char),
        );
    }
    chars
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// Forgives case, separators and the letters Crockford base32 reads as digits.
pub fn normalize_recovery_key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_garbage() {
        assert_eq!(hex(&[0, 1, 0xab]), "0001ab");
        assert_eq!(unhex("0001ab"), Some(vec![0, 1, 0xab]));
        assert_eq!(unhex("0g"), None);
        assert_eq!(unhex("abc"), None);
        assert_eq!(unhex("é1"), None);
    }

    #[test]
    fn kdf_parameters_from_the_remote_are_bounded() {
        let refused = |kdf: Kdf| derive(b"x", &[0; 16], kdf).is_err();
        let d = Kdf::default();
        assert!(refused(Kdf {
            m: (256 << 10) + 1,
            ..d
        }));
        assert!(refused(Kdf { t: 9, ..d }));
        assert!(refused(Kdf { p: 5, ..d }));
    }

    #[test]
    fn recovery_keys_are_readable() {
        let key = recovery_key();
        assert_eq!(key.len(), 39, "{key}");
        assert_eq!(key.split('-').count(), 8);
        assert_ne!(key, recovery_key());
        let typed = key.to_lowercase().replace('-', " ").replace('0', "o");
        assert_eq!(normalize_recovery_key(&typed), key.replace('-', ""));
    }

    #[test]
    fn blobs_are_bound_to_their_name_and_generation() {
        let keys = DataKeys::new(&random(), random());
        let blob = keys.seal_blob("files/a", b"secret");
        assert_eq!(
            keys.open_blob("files/a", &blob).unwrap().unwrap(),
            b"secret"
        );
        assert!(keys.open_blob("files/b", &blob).is_err(), "renamed");
        let mut tampered = blob.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(keys.open_blob("files/a", &tampered).is_err());
        let other = DataKeys::new(&random(), random());
        assert_eq!(other.open_blob("files/a", &blob).unwrap(), None);
        assert!(keys.open_blob("files/a", b"plain text").is_err());
        assert_ne!(keys.name("f", "a"), keys.name("d", "a"));
        assert_ne!(keys.name("f", "a"), other.name("f", "a"));
        assert_eq!(keys.name("f", "a").len(), 32);
    }
}
