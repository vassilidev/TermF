//! Time-based one-time passwords (RFC 6238) for the accounts kept in the vault.

use anyhow::{Result, bail, ensure};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::Secret;

const NOT_TOTP: &str = "this is neither a TOTP secret (base32) nor an otpauth://totp/ link";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Totp {
    /// Base32, upper case, without spaces nor padding.
    pub(crate) secret: Secret,
    pub algorithm: Algorithm,
    pub digits: u32,
    pub period: u64,
}

impl Totp {
    /// A base32 secret (spaces, dashes and padding allowed), or an `otpauth://totp/…?secret=…` link.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        let mut totp = Self {
            secret: Secret::default(),
            algorithm: Algorithm::Sha1,
            digits: 6,
            period: 30,
        };
        let secret = match text.get(..10) {
            Some(scheme) if scheme.eq_ignore_ascii_case("otpauth://") => {
                let rest = &text[10..];
                ensure!(
                    rest.get(..5)
                        .is_some_and(|t| t.eq_ignore_ascii_case("totp/")),
                    "only time-based codes (otpauth://totp/) are supported"
                );
                let query = rest.split_once('?').map_or("", |(_, q)| q);
                let mut secret = None;
                for pair in query.split('&') {
                    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                    let value = Zeroizing::new(percent_decode(value));
                    match key.to_ascii_lowercase().as_str() {
                        "secret" => secret = Some(Secret::new(value.to_string())),
                        "algorithm" => {
                            totp.algorithm = match value.to_ascii_uppercase().as_str() {
                                "SHA1" => Algorithm::Sha1,
                                "SHA256" => Algorithm::Sha256,
                                "SHA512" => Algorithm::Sha512,
                                _ => bail!("unknown TOTP algorithm (SHA1, SHA256, SHA512)"),
                            }
                        }
                        "digits" => totp.digits = value.parse().unwrap_or(0),
                        "period" => totp.period = value.parse().unwrap_or(0),
                        _ => {}
                    }
                }
                secret.ok_or_else(|| anyhow::anyhow!("the otpauth:// link has no secret"))?
            }
            _ => Secret::new(text.into()),
        };
        ensure!(
            (6..=8).contains(&totp.digits),
            "a TOTP code has 6 to 8 digits"
        );
        ensure!(
            (1..=600).contains(&totp.period),
            "a TOTP period is 1 to 600 seconds"
        );
        let normalized: String = secret
            .expose()
            .chars()
            .filter(|c| !matches!(c, ' ' | '-' | '='))
            .map(|c| c.to_ascii_uppercase())
            .collect();
        totp.secret = Secret::new(normalized);
        // At least 80 bits, as every service gives: shorter is likely a typo or another field pasted here.
        ensure!(totp.key().is_some_and(|k| k.len() >= 10), NOT_TOTP);
        Ok(totp)
    }

    fn key(&self) -> Option<Zeroizing<Vec<u8>>> {
        base32(self.secret.expose())
    }

    /// The code valid at `at` (Unix seconds), zero-padded.
    pub fn code(&self, at: u64) -> Secret {
        let key = self.key().unwrap_or_default();
        let counter = (at / self.period).to_be_bytes();
        let new = "HMAC takes any key size";
        let hash = match self.algorithm {
            Algorithm::Sha1 => sign(
                Hmac::<sha1::Sha1>::new_from_slice(&key).expect(new),
                &counter,
            ),
            Algorithm::Sha256 => sign(
                Hmac::<sha2::Sha256>::new_from_slice(&key).expect(new),
                &counter,
            ),
            Algorithm::Sha512 => sign(
                Hmac::<sha2::Sha512>::new_from_slice(&key).expect(new),
                &counter,
            ),
        };
        let offset = usize::from(hash[hash.len() - 1] & 0x0f);
        let bin =
            u32::from_be_bytes(hash[offset..offset + 4].try_into().expect("4 bytes")) & 0x7fff_ffff;
        let code = bin % 10u32.pow(self.digits);
        Secret::new(format!("{code:0width$}", width = self.digits as usize))
    }
}

fn sign(mut mac: impl Mac, message: &[u8]) -> Zeroizing<Vec<u8>> {
    mac.update(message);
    Zeroizing::new(mac.finalize().into_bytes().to_vec())
}

/// RFC 4648 base32, upper case, no padding.
fn base32(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(Vec::with_capacity(text.len() * 5 / 8));
    let (mut buffer, mut bits) = (0u64, 0);
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'2'..=b'7' => c - b'2' + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | u64::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    buffer.zeroize();
    Some(out)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238, appendix B: the seeds are these ASCII digits, the codes 8 digits long.
    #[test]
    fn gives_the_rfc_6238_codes() {
        let seed = |len: usize| -> String {
            let ascii: Vec<u8> = b"1234567890".iter().cycle().take(len).copied().collect();
            encode(&ascii)
        };
        let vectors: [(u64, [&str; 3]); 6] = [
            (59, ["94287082", "46119246", "90693936"]),
            (1111111109, ["07081804", "68084774", "25091201"]),
            (1111111111, ["14050471", "67062674", "99943326"]),
            (1234567890, ["89005924", "91819424", "93441116"]),
            (2000000000, ["69279037", "90698825", "38618901"]),
            (20000000000, ["65353130", "77737706", "47863826"]),
        ];
        let algorithms = [
            (Algorithm::Sha1, 20, "SHA1"),
            (Algorithm::Sha256, 32, "SHA256"),
            (Algorithm::Sha512, 64, "SHA512"),
        ];
        for (i, (algorithm, len, name)) in algorithms.into_iter().enumerate() {
            let link = format!(
                "otpauth://totp/ACME:alex?secret={}&algorithm={name}&digits=8&period=30",
                seed(len)
            );
            let totp = Totp::parse(&link).unwrap();
            assert_eq!(totp.algorithm, algorithm);
            for (at, codes) in vectors {
                assert_eq!(totp.code(at).expose(), codes[i], "{name} at {at}");
            }
        }
    }

    #[test]
    fn reads_a_pasted_secret_or_link() {
        let totp = Totp::parse(" jbsw y3dp-ehpk 3pxp ").unwrap();
        assert_eq!(totp.secret.expose(), "JBSWY3DPEHPK3PXP");
        assert_eq!(
            (totp.algorithm, totp.digits, totp.period),
            (Algorithm::Sha1, 6, 30)
        );
        assert_eq!(totp.code(59).expose().len(), 6);
        let totp = Totp::parse(
            "OTPAUTH://TOTP/Acme%20Inc:alex%40acme.test?issuer=Acme+Inc&secret=JBSWY3DPEHPK3PXP%3D%3D&period=60&digits=8",
        )
        .unwrap();
        assert_eq!(totp.secret.expose(), "JBSWY3DPEHPK3PXP");
        assert_eq!((totp.digits, totp.period), (8, 60));
        assert_eq!(totp.code(0).expose(), totp.code(59).expose());

        for bad in [
            "",
            "hunter2",
            "JBSWY3DP",
            "JBSWY3DPEHPK3PX1",
            "otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP",
            "otpauth://totp/x?issuer=acme",
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=4",
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=0",
            "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&algorithm=MD5",
        ] {
            let err = Totp::parse(bad).unwrap_err().to_string();
            assert!(
                !err.contains("JBSWY3DP") && !err.contains("hunter2"),
                "{err}"
            );
        }
    }

    fn encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let mut out = String::new();
        for chunk in bytes.chunks(5) {
            let mut block = [0u8; 5];
            block[..chunk.len()].copy_from_slice(chunk);
            let value = block.iter().fold(0u64, |v, b| (v << 8) | u64::from(*b));
            let chars = (chunk.len() * 8).div_ceil(5);
            for i in 0..chars {
                out.push(ALPHABET[((value >> (35 - 5 * i)) & 31) as usize] as char);
            }
        }
        out
    }

    #[test]
    fn decodes_base32() {
        assert_eq!(
            base32(&encode(b"12345678901234567890")).unwrap().as_slice(),
            b"12345678901234567890"
        );
        assert_eq!(base32("MZXW6YQ").unwrap().as_slice(), b"foob");
        assert!(base32("mzxw").is_none());
    }
}
