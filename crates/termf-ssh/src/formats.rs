//! Keys beyond SSH: generated, read and written in process in every common format (OpenSSH, PEM PKCS#1 /
//! SEC1 / PKCS#8 / SPKI, JWK and JWK sets, PuTTY). Errors never quote the key they failed to read.

use std::fmt;

use anyhow::{Context, Result, anyhow, bail, ensure};
use base64ct::{Base64UrlUnpadded, Encoding};
use pkcs8::der::asn1::{AnyRef, BitStringRef, ContextSpecific, OctetStringRef};
use pkcs8::der::{Decode, Document, Encode, Reader, SliceReader, TagNumber};
use pkcs8::spki::{AlgorithmIdentifierRef, EncodePublicKey, SubjectPublicKeyInfoRef};
use pkcs8::{
    DecodePrivateKey, EncodePrivateKey, EncryptedPrivateKeyInfo, LineEnding, ObjectIdentifier,
    PrivateKeyInfo, SecretDocument,
};
use rsa::pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey};
use rsa::traits::{PrivateKeyParts, PublicKeyParts};
use rsa::{BigUint, RsaPrivateKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use ssh_key::private::{EcdsaKeypair, Ed25519Keypair, KeypairData, RsaKeypair};
use ssh_key::public::KeyData;
use ssh_key::rand_core::{OsRng, RngCore};
use ssh_key::{HashAlg, PrivateKey, PublicKey};
use zeroize::Zeroizing;

use crate::ppk;

const ED25519: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.101.112");
const X25519: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.101.110");
// OWASP's 2023 figure for PBKDF2-HMAC-SHA256.
const PBKDF2_ROUNDS: u32 = 600_000;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Algo {
    Ed25519,
    P256,
    P384,
    P521,
    Rsa2048,
    Rsa3072,
    Rsa4096,
    X25519,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Private key and `.pub` line.
    Openssh,
    /// `RSA PRIVATE KEY`, RSA only.
    Pkcs1,
    /// `EC PRIVATE KEY`, ECDSA only.
    Sec1,
    /// `PRIVATE KEY`, or `ENCRYPTED PRIVATE KEY` with a passphrase.
    Pkcs8,
    /// `PUBLIC KEY` alone.
    Spki,
    Jwk,
    Jwks,
    /// PuTTY's `.ppk`, version 3.
    Ppk,
}

impl Format {
    fn encryptable(self) -> bool {
        matches!(self, Format::Openssh | Format::Pkcs8 | Format::Ppk)
    }
}

/// What the key is for: sets `use` and `alg` in a JWK, and where an OpenSSH key is saved.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Usage {
    #[default]
    Ssh,
    Sign,
    Encrypt,
    Tls,
}

/// A missing or wrong passphrase, which the UI asks again for instead of reporting a broken key.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Locked {
    NeedsPassphrase,
    WrongPassphrase,
}

impl fmt::Display for Locked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Locked::NeedsPassphrase => "this key is protected by a passphrase",
            Locked::WrongPassphrase => "wrong passphrase",
        })
    }
}

impl std::error::Error for Locked {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Curve {
    P256,
    P384,
    P521,
}

const CURVES: [Curve; 3] = [Curve::P256, Curve::P384, Curve::P521];

/// Runs `$body` with `$c` naming the curve's crate.
macro_rules! on_curve {
    ($curve:expr, $c:ident => $body:expr) => {
        match $curve {
            Curve::P256 => {
                use p256 as $c;
                $body
            }
            Curve::P384 => {
                use p384 as $c;
                $body
            }
            Curve::P521 => {
                use p521 as $c;
                $body
            }
        }
    };
}

impl Curve {
    fn oid(self) -> ObjectIdentifier {
        ObjectIdentifier::new_unwrap(match self {
            Curve::P256 => "1.2.840.10045.3.1.7",
            Curve::P384 => "1.3.132.0.34",
            Curve::P521 => "1.3.132.0.35",
        })
    }

    /// Bytes of a scalar or coordinate.
    fn size(self) -> usize {
        match self {
            Curve::P256 => 32,
            Curve::P384 => 48,
            Curve::P521 => 66,
        }
    }

    fn jwk(self) -> &'static str {
        match self {
            Curve::P256 => "P-256",
            Curve::P384 => "P-384",
            Curve::P521 => "P-521",
        }
    }
}

/// A private key, whatever file it came from. Every variant is wiped from memory when dropped.
pub(crate) enum Secret {
    /// The 32-byte seed.
    Ed25519(Zeroizing<[u8; 32]>),
    X25519(Zeroizing<[u8; 32]>),
    /// The private scalar, big-endian, full length.
    Ec(Curve, Zeroizing<Vec<u8>>),
    Rsa(Box<RsaPrivateKey>),
}

// Never the key itself, even in a failed test's output.
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret({})", self.label())
    }
}

/// One file of an export: `suffix` follows the key's name.
pub(crate) struct Output {
    pub suffix: &'static str,
    pub text: Zeroizing<String>,
    pub secret: bool,
}

pub(crate) struct Parsed {
    pub secret: Secret,
    pub comment: String,
    pub format: Format,
    pub encrypted: bool,
}

impl Secret {
    pub fn random(algo: Algo) -> Result<Self> {
        let bytes32 = || {
            let mut seed = Zeroizing::new([0u8; 32]);
            OsRng.fill_bytes(seed.as_mut());
            seed
        };
        let rsa = |bits| -> Result<Self> {
            Ok(Secret::Rsa(Box::new(RsaPrivateKey::new(&mut OsRng, bits)?)))
        };
        let ec = |curve: Curve| {
            let d = on_curve!(curve, c => Zeroizing::new(c::SecretKey::random(&mut OsRng).to_bytes().to_vec()));
            Secret::Ec(curve, d)
        };
        Ok(match algo {
            Algo::Ed25519 => Secret::Ed25519(bytes32()),
            Algo::X25519 => Secret::X25519(bytes32()),
            Algo::P256 => ec(Curve::P256),
            Algo::P384 => ec(Curve::P384),
            Algo::P521 => ec(Curve::P521),
            Algo::Rsa2048 => rsa(2048)?,
            Algo::Rsa3072 => rsa(3072)?,
            Algo::Rsa4096 => rsa(4096)?,
        })
    }

    /// `Ed25519`, `ECDSA P-256`, `RSA 4096`, `X25519`.
    pub fn label(&self) -> String {
        match self {
            Secret::Ed25519(_) => "Ed25519".into(),
            Secret::X25519(_) => "X25519".into(),
            Secret::Ec(curve, _) => format!("ECDSA {}", curve.jwk()),
            Secret::Rsa(key) => format!("RSA {}", key.n().bits()),
        }
    }

    fn x25519_public(seed: &[u8; 32]) -> [u8; 32] {
        curve25519_dalek::MontgomeryPoint::mul_base_clamped(*seed).to_bytes()
    }

    fn ec_point(curve: Curve, d: &[u8]) -> Result<Vec<u8>> {
        on_curve!(curve, c => {
            use c::elliptic_curve::sec1::ToEncodedPoint;
            let key = c::SecretKey::from_slice(d).map_err(|_| anyhow!("invalid private key"))?;
            Ok(key.public_key().to_encoded_point(false).as_bytes().to_vec())
        })
    }

    /// The SSH form, for every key but X25519, which SSH does not use.
    pub fn to_ssh(&self) -> Result<Option<KeypairData>> {
        Ok(Some(match self {
            Secret::Ed25519(seed) => KeypairData::Ed25519(Ed25519Keypair::from_seed(seed)),
            Secret::X25519(_) => return Ok(None),
            Secret::Ec(curve, d) => {
                let invalid = |_| anyhow!("invalid private key");
                KeypairData::Ecdsa(match curve {
                    Curve::P256 => {
                        let key = p256::SecretKey::from_slice(d).map_err(invalid)?;
                        EcdsaKeypair::NistP256 {
                            public: key.public_key().into(),
                            private: key.into(),
                        }
                    }
                    Curve::P384 => {
                        let key = p384::SecretKey::from_slice(d).map_err(invalid)?;
                        EcdsaKeypair::NistP384 {
                            public: key.public_key().into(),
                            private: key.into(),
                        }
                    }
                    Curve::P521 => {
                        let key = p521::SecretKey::from_slice(d).map_err(invalid)?;
                        EcdsaKeypair::NistP521 {
                            public: key.public_key().into(),
                            private: key.into(),
                        }
                    }
                })
            }
            Secret::Rsa(key) => KeypairData::Rsa(RsaKeypair::try_from(key.as_ref())?),
        }))
    }

    pub fn from_ssh(key: &KeypairData) -> Result<Self> {
        Ok(match key {
            KeypairData::Ed25519(pair) => Secret::Ed25519(Zeroizing::new(pair.private.to_bytes())),
            KeypairData::Ecdsa(pair) => {
                let curve = match pair.curve() {
                    ssh_key::EcdsaCurve::NistP256 => Curve::P256,
                    ssh_key::EcdsaCurve::NistP384 => Curve::P384,
                    ssh_key::EcdsaCurve::NistP521 => Curve::P521,
                };
                Secret::Ec(curve, Zeroizing::new(pair.private_key_bytes().to_vec()))
            }
            KeypairData::Rsa(pair) => {
                // Not ssh-key's own conversion: 0.6.7 passes `p` as both primes.
                let int = |m: &ssh_key::Mpint| {
                    m.as_positive_bytes()
                        .map(BigUint::from_bytes_be)
                        .context("invalid RSA key")
                };
                let primes = vec![int(&pair.private.p)?, int(&pair.private.q)?];
                let key = RsaPrivateKey::from_components(
                    int(&pair.public.n)?,
                    int(&pair.public.e)?,
                    int(&pair.private.d)?,
                    primes,
                )
                .map_err(|_| anyhow!("invalid RSA key"))?;
                Secret::Rsa(Box::new(key))
            }
            _ => bail!("unsupported key type: Ed25519, ECDSA or RSA only"),
        })
    }

    /// The public half as DER `SubjectPublicKeyInfo`: what two halves are compared on.
    pub fn spki(&self) -> Result<Vec<u8>> {
        match self {
            Secret::Ed25519(seed) => okp_spki(ED25519, &Ed25519Keypair::from_seed(seed).public.0),
            Secret::X25519(seed) => okp_spki(X25519, &Self::x25519_public(seed)),
            Secret::Ec(curve, d) => ec_spki(*curve, &Self::ec_point(*curve, d)?),
            Secret::Rsa(key) => Ok(key.to_public_key().to_public_key_der()?.into_vec()),
        }
    }

    fn pkcs8(&self) -> Result<SecretDocument> {
        Ok(match self {
            Secret::Ed25519(seed) => okp_pkcs8(ED25519, seed)?,
            Secret::X25519(seed) => okp_pkcs8(X25519, seed)?,
            Secret::Ec(curve, d) => on_curve!(*curve, c => c::SecretKey::from_slice(d)
                .map_err(|_| anyhow!("invalid private key"))?
                .to_pkcs8_der()?),
            Secret::Rsa(key) => key.to_pkcs8_der()?,
        })
    }

    /// The public JWK members, in the order of RFC 7638's thumbprint input where it has one.
    fn jwk_public(&self) -> Result<Vec<(&'static str, String)>> {
        let b64 = |bytes: &[u8]| Base64UrlUnpadded::encode_string(bytes);
        Ok(match self {
            Secret::Ed25519(seed) => vec![
                ("crv", "Ed25519".into()),
                ("kty", "OKP".into()),
                ("x", b64(&Ed25519Keypair::from_seed(seed).public.0)),
            ],
            Secret::X25519(seed) => vec![
                ("crv", "X25519".into()),
                ("kty", "OKP".into()),
                ("x", b64(&Self::x25519_public(seed))),
            ],
            Secret::Ec(curve, d) => {
                let point = Self::ec_point(*curve, d)?;
                let size = (point.len() - 1) / 2;
                vec![
                    ("crv", curve.jwk().into()),
                    ("kty", "EC".into()),
                    ("x", b64(&point[1..=size])),
                    ("y", b64(&point[1 + size..])),
                ]
            }
            Secret::Rsa(key) => vec![
                ("e", b64(&key.e().to_bytes_be())),
                ("kty", "RSA".into()),
                ("n", b64(&key.n().to_bytes_be())),
            ],
        })
    }

    pub fn thumbprint(&self) -> Result<String> {
        Ok(thumbprint(&self.jwk_public()?))
    }

    fn jwk_private(&self) -> Result<Vec<(&'static str, Zeroizing<String>)>> {
        let b64 = |bytes: &[u8]| Zeroizing::new(Base64UrlUnpadded::encode_string(bytes));
        Ok(match self {
            Secret::Ed25519(seed) | Secret::X25519(seed) => vec![("d", b64(seed.as_ref()))],
            Secret::Ec(_, d) => vec![("d", b64(d))],
            Secret::Rsa(key) => {
                let [p, q] = key.primes() else {
                    bail!("an RSA key with more than two primes has no JWK form here");
                };
                let crt = |v: Option<&BigUint>| {
                    v.map(|v| b64(&v.to_bytes_be()))
                        .context("RSA key without CRT values")
                };
                let qi = key
                    .crt_coefficient()
                    .context("RSA key without CRT values")?;
                vec![
                    ("d", b64(&key.d().to_bytes_be())),
                    ("p", b64(&p.to_bytes_be())),
                    ("q", b64(&q.to_bytes_be())),
                    ("dp", crt(key.dp())?),
                    ("dq", crt(key.dq())?),
                    ("qi", b64(&qi.to_bytes_be())),
                ]
            }
        })
    }

    /// `use` and `alg` (RFC 7518, RFC 8037) when the usage names one for this key.
    fn jwk_usage(&self, usage: Usage) -> Vec<(&'static str, String)> {
        let alg = match (usage, self) {
            (Usage::Sign, Secret::Ed25519(_)) => "EdDSA",
            (Usage::Sign, Secret::Ec(Curve::P256, _)) => "ES256",
            (Usage::Sign, Secret::Ec(Curve::P384, _)) => "ES384",
            (Usage::Sign, Secret::Ec(Curve::P521, _)) => "ES512",
            (Usage::Sign, Secret::Rsa(_)) => "RS256",
            (Usage::Encrypt, Secret::Ec(..) | Secret::X25519(_)) => "ECDH-ES",
            (Usage::Encrypt, Secret::Rsa(_)) => "RSA-OAEP-256",
            _ => return Vec::new(),
        };
        let use_ = if usage == Usage::Sign { "sig" } else { "enc" };
        vec![("use", use_.into()), ("alg", alg.into())]
    }

    /// Pretty JSON, built by hand: every value is base64url or a fixed name, and the buffer holding
    /// the private members is sized up front so no copy of them is left behind by a reallocation.
    fn jwk(&self, private: bool, usage: Usage, set: bool) -> Result<Zeroizing<String>> {
        let public = self.jwk_public()?;
        let kty = public
            .iter()
            .find(|(k, _)| *k == "kty")
            .map(|(_, v)| v.clone());
        let mut members: Vec<(&str, Zeroizing<String>)> =
            vec![("kty", Zeroizing::new(kty.unwrap_or_default()))];
        members.extend(
            self.jwk_usage(usage)
                .into_iter()
                .map(|(k, v)| (k, Zeroizing::new(v))),
        );
        members.push(("kid", Zeroizing::new(self.thumbprint()?)));
        members.extend(
            public
                .into_iter()
                .filter(|(k, _)| *k != "kty")
                .map(|(k, v)| (k, Zeroizing::new(v))),
        );
        if private {
            members.extend(self.jwk_private()?);
        }
        let indent = if set { "      " } else { "  " };
        let size: usize = members
            .iter()
            .map(|(k, v)| k.len() + v.len() + indent.len() + 8)
            .sum::<usize>()
            + 64;
        let mut out = Zeroizing::new(String::with_capacity(size));
        out.push_str(if set {
            "{\n  \"keys\": [\n    {\n"
        } else {
            "{\n"
        });
        for (i, (key, value)) in members.iter().enumerate() {
            for part in [indent, "\"", key, "\": \"", value.as_str(), "\""] {
                out.push_str(part);
            }
            out.push_str(if i + 1 < members.len() { ",\n" } else { "\n" });
        }
        out.push_str(if set { "    }\n  ]\n}\n" } else { "}\n" });
        Ok(out)
    }

    fn spki_pem(&self) -> Result<Zeroizing<String>> {
        Ok(Zeroizing::new(
            Document::try_from(self.spki()?)?.to_pem("PUBLIC KEY", LineEnding::LF)?,
        ))
    }

    /// The files of `format`: the private key first, then its public half.
    pub fn encode(
        &self,
        format: Format,
        comment: &str,
        passphrase: Option<&str>,
        usage: Usage,
    ) -> Result<Vec<Output>> {
        let passphrase = passphrase.filter(|p| !p.is_empty());
        ensure!(
            passphrase.is_none() || format.encryptable(),
            "this format cannot hold a passphrase: choose PKCS#8, OpenSSH or PuTTY to protect the key"
        );
        let private = |suffix, text| Output {
            suffix,
            text,
            secret: true,
        };
        let public = |suffix, text| Output {
            suffix,
            text,
            secret: false,
        };
        Ok(match format {
            Format::Openssh | Format::Ppk => {
                let pair = self
                    .to_ssh()?
                    .context("X25519 has no SSH form: choose PKCS#8 or JWK")?;
                let mut key = PrivateKey::new(pair, comment)?;
                let line = Zeroizing::new(format!("{}\n", key.public_key().to_openssh()?));
                if format == Format::Ppk {
                    let text = ppk::encode(key.key_data(), comment, passphrase)?;
                    vec![private(".ppk", text), public(".pub", line)]
                } else {
                    if let Some(passphrase) = passphrase {
                        key = key.encrypt(&mut OsRng, passphrase)?;
                    }
                    vec![
                        private("", key.to_openssh(ssh_key::LineEnding::LF)?),
                        public(".pub", line),
                    ]
                }
            }
            Format::Pkcs1 => {
                let Secret::Rsa(key) = self else {
                    bail!("PKCS#1 holds RSA keys only: choose PKCS#8");
                };
                vec![
                    private(".pem", key.to_pkcs1_pem(LineEnding::LF)?),
                    public(".pub.pem", self.spki_pem()?),
                ]
            }
            Format::Sec1 => {
                let Secret::Ec(curve, d) = self else {
                    bail!("SEC1 holds ECDSA keys only: choose PKCS#8");
                };
                // Built here: the curve crates leave out the curve's name, without which OpenSSL
                // cannot read the key.
                let point = Self::ec_point(*curve, d)?;
                let key = sec1::EcPrivateKey {
                    private_key: d,
                    parameters: Some(sec1::EcParameters::NamedCurve(curve.oid())),
                    public_key: Some(&point),
                };
                let der = Zeroizing::new(key.to_der()?);
                let text = SecretDocument::try_from(der.as_slice())?
                    .to_pem("EC PRIVATE KEY", LineEnding::LF)?;
                vec![private(".pem", text), public(".pub.pem", self.spki_pem()?)]
            }
            Format::Pkcs8 => {
                let der = self.pkcs8()?;
                let text = match passphrase {
                    Some(passphrase) => {
                        let mut salt = [0u8; 16];
                        let mut iv = [0u8; 16];
                        OsRng.fill_bytes(&mut salt);
                        OsRng.fill_bytes(&mut iv);
                        let params = pkcs8::pkcs5::pbes2::Parameters::pbkdf2_sha256_aes256cbc(
                            PBKDF2_ROUNDS,
                            &salt,
                            &iv,
                        )
                        .map_err(|e| anyhow!("{e}"))?;
                        PrivateKeyInfo::try_from(der.as_bytes())?
                            .encrypt_with_params(params, passphrase)?
                            .to_pem("ENCRYPTED PRIVATE KEY", LineEnding::LF)?
                    }
                    None => der.to_pem("PRIVATE KEY", LineEnding::LF)?,
                };
                vec![private(".pem", text), public(".pub.pem", self.spki_pem()?)]
            }
            Format::Spki => vec![public(".pub.pem", self.spki_pem()?)],
            Format::Jwk => vec![
                private(".jwk.json", self.jwk(true, usage, false)?),
                public(".pub.jwk.json", self.jwk(false, usage, false)?),
            ],
            Format::Jwks => vec![
                private(".jwks.json", self.jwk(true, usage, true)?),
                public(".pub.jwks.json", self.jwk(false, usage, true)?),
            ],
        })
    }

    /// Signs a message and checks the signature with the public half (SSH signature format), or for
    /// X25519 checks that a key agreement gives both sides the same secret.
    pub fn self_test(&self) -> Result<bool> {
        if let Secret::X25519(seed) = self {
            let mut other = Zeroizing::new([0u8; 32]);
            OsRng.fill_bytes(other.as_mut());
            let ours =
                curve25519_dalek::MontgomeryPoint(Self::x25519_public(seed)).mul_clamped(*other);
            let theirs =
                curve25519_dalek::MontgomeryPoint(Self::x25519_public(&other)).mul_clamped(**seed);
            return Ok(ours == theirs);
        }
        let message = b"TermF key test";
        if let Secret::Rsa(key) = self {
            // ssh-key 0.6.7 cannot sign with RSA (see `from_ssh`): the rsa crate signs directly.
            use rsa::signature::{Signer, Verifier};
            let signer = rsa::pkcs1v15::SigningKey::<Sha256>::new(key.as_ref().clone());
            let signature = signer.try_sign(message)?;
            let verifier = rsa::pkcs1v15::VerifyingKey::<Sha256>::new(key.to_public_key());
            return Ok(verifier.verify(message, &signature).is_ok());
        }
        let key = PrivateKey::new(self.to_ssh()?.context("no SSH form")?, "")?;
        const NAMESPACE: &str = "termf-key-test";
        let signature = key.sign(NAMESPACE, HashAlg::Sha512, message)?;
        Ok(key
            .public_key()
            .verify(NAMESPACE, message, &signature)
            .is_ok())
    }
}

/// RFC 7638: SHA-256 of the required public members, sorted, without spaces.
pub(crate) fn thumbprint(members: &[(&str, String)]) -> String {
    let canonical: Vec<String> = members
        .iter()
        .map(|(k, v)| format!("\"{k}\":\"{v}\""))
        .collect();
    Base64UrlUnpadded::encode_string(&Sha256::digest(format!("{{{}}}", canonical.join(","))))
}

fn okp_spki(oid: ObjectIdentifier, public: &[u8]) -> Result<Vec<u8>> {
    let info = SubjectPublicKeyInfoRef {
        algorithm: AlgorithmIdentifierRef {
            oid,
            parameters: None,
        },
        subject_public_key: BitStringRef::from_bytes(public)?,
    };
    Ok(info.to_der()?)
}

fn okp_pkcs8(oid: ObjectIdentifier, seed: &[u8; 32]) -> Result<SecretDocument> {
    // RFC 8410: the private key is itself an OCTET STRING inside the PKCS#8 one.
    let inner = Zeroizing::new(OctetStringRef::new(seed)?.to_der()?);
    let info = PrivateKeyInfo::new(
        AlgorithmIdentifierRef {
            oid,
            parameters: None,
        },
        &inner,
    );
    Ok(SecretDocument::try_from(info)?)
}

fn ec_spki(curve: Curve, point: &[u8]) -> Result<Vec<u8>> {
    on_curve!(curve, c => Ok(c::PublicKey::from_sec1_bytes(point)
        .map_err(|_| anyhow!("invalid public key"))?
        .to_public_key_der()?
        .into_vec()))
}

/// The SPKI form of an SSH public key.
pub(crate) fn ssh_spki(key: &KeyData) -> Result<Vec<u8>> {
    if let Some(key) = key.ed25519() {
        return okp_spki(ED25519, &key.0);
    }
    if let Some(key) = key.ecdsa() {
        let curve = match key.curve() {
            ssh_key::EcdsaCurve::NistP256 => Curve::P256,
            ssh_key::EcdsaCurve::NistP384 => Curve::P384,
            ssh_key::EcdsaCurve::NistP521 => Curve::P521,
        };
        return ec_spki(curve, key.as_sec1_bytes());
    }
    if let Some(key) = key.rsa() {
        return Ok(rsa::RsaPublicKey::try_from(key)?
            .to_public_key_der()?
            .into_vec());
    }
    bail!("unsupported key type: Ed25519, ECDSA or RSA only")
}

pub(crate) fn from_pkcs8(der: &[u8]) -> Result<Secret> {
    let info = PrivateKeyInfo::try_from(der).map_err(|_| anyhow!("invalid PKCS#8 key"))?;
    if [ED25519, X25519].contains(&info.algorithm.oid) {
        let inner = OctetStringRef::from_der(info.private_key)
            .map_err(|_| anyhow!("invalid PKCS#8 key"))?;
        let mut seed = Zeroizing::new([0u8; 32]);
        ensure!(inner.as_bytes().len() == 32, "invalid PKCS#8 key");
        seed.copy_from_slice(inner.as_bytes());
        return Ok(match info.algorithm.oid == ED25519 {
            true => Secret::Ed25519(seed),
            false => Secret::X25519(seed),
        });
    }
    if let Ok(key) = RsaPrivateKey::from_pkcs8_der(der) {
        return Ok(Secret::Rsa(Box::new(key)));
    }
    for curve in CURVES {
        if let Ok(d) = on_curve!(curve, c => c::SecretKey::from_pkcs8_der(der).map(|k| Zeroizing::new(k.to_bytes().to_vec())))
        {
            return Ok(Secret::Ec(curve, d));
        }
    }
    if let Some(secret) = explicit_ec(info.private_key, info.algorithm.parameters) {
        return Ok(secret);
    }
    bail!(
        "unsupported PKCS#8 key ({}): Ed25519, X25519, ECDSA or RSA only",
        info.algorithm.oid
    )
}

/// A SEC1 key whose curve is spelled out rather than named, as `ssh-keygen` built on LibreSSL (macOS) writes
/// one: the curve whose base point the parameters give (`params`: PKCS#8's, else the key's own), and the key's
/// public point, when it has one, must be its own.
fn explicit_ec(sec1: &[u8], params: Option<AnyRef<'_>>) -> Option<Secret> {
    let (d, own, public) = SliceReader::new(sec1)
        .ok()?
        .sequence(|r| {
            u8::decode(r)?;
            let d = OctetStringRef::decode(r)?;
            let own = ContextSpecific::<AnyRef>::decode_explicit(r, TagNumber::N0)?;
            let public = ContextSpecific::<BitStringRef>::decode_explicit(r, TagNumber::N1)?;
            Ok((
                d.as_bytes(),
                own.map(|p| p.value),
                public.map(|p| p.value.raw_bytes()),
            ))
        })
        .ok()?;
    let base = base_point(params.or(own)?)?;
    let curve = CURVES.into_iter().find(|&c| {
        let mut one = vec![0; c.size()];
        one[c.size() - 1] = 1;
        Secret::ec_point(c, &one).is_ok_and(|g| g == base || compressed(&g) == base)
    })?;
    // OpenSSL drops a scalar's leading zeros.
    let d = &d[d.iter().take_while(|b| **b == 0).count()..];
    let mut padded = Zeroizing::new(vec![0; curve.size().checked_sub(d.len())?]);
    padded.extend_from_slice(d);
    let point = Secret::ec_point(curve, &padded).ok()?;
    public
        .is_none_or(|p| p == point || p == compressed(&point))
        .then(|| Secret::Ec(curve, padded))
}

/// The base point of explicit EC parameters (RFC 3279: version, field, curve, base, order, cofactor).
fn base_point(params: AnyRef<'_>) -> Option<Vec<u8>> {
    let der = params.to_der().ok()?;
    SliceReader::new(&der)
        .ok()?
        .sequence(|r| {
            u8::decode(r)?;
            AnyRef::decode(r)?;
            AnyRef::decode(r)?;
            let base = OctetStringRef::decode(r)?.as_bytes().to_vec();
            while !r.is_finished() {
                AnyRef::decode(r)?;
            }
            Ok(base)
        })
        .ok()
}

fn compressed(point: &[u8]) -> Vec<u8> {
    let size = (point.len() - 1) / 2;
    let mut out = vec![2 + (point[point.len() - 1] & 1)];
    out.extend_from_slice(&point[1..=size]);
    out
}

/// The first PEM block whose label ends with `suffix` (OpenSSL may put `EC PARAMETERS` first).
fn pem_block<'a>(text: &'a str, suffix: &str) -> Option<(&'a str, &'a str)> {
    let mut rest = text;
    while let Some(start) = rest.find("-----BEGIN ") {
        rest = &rest[start..];
        let label = rest
            .lines()
            .next()?
            .trim()
            .strip_prefix("-----BEGIN ")?
            .strip_suffix("-----")?;
        let end_line = format!("-----END {label}-----");
        let end = rest.find(&end_line)? + end_line.len();
        if label.ends_with(suffix) {
            return Some((label, &rest[..end]));
        }
        rest = &rest[end..];
    }
    None
}

/// An ECDSA key in OpenSSH's format, read here: `ssh-key` 0.6 refuses one whose scalar lost its leading zero
/// bytes, as OpenSSH writes an mpint (one P-256 key in 256, half of the P-521 ones). `None`: not an ECDSA key.
pub(crate) fn openssh_ecdsa(
    block: &str,
    pass: Option<&str>,
) -> Result<Option<(Secret, String, bool)>> {
    fn string<'a>(data: &mut &'a [u8]) -> Option<&'a [u8]> {
        let len = usize::try_from(u32::from_be_bytes(data.get(..4)?.try_into().ok()?)).ok()?;
        let value = data.get(4..4usize.checked_add(len)?)?;
        *data = &data[4 + len..];
        Some(value)
    }
    let body: String = block
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .map(str::trim)
        .collect();
    let Ok(data) = base64ct::Base64::decode_vec(&body) else {
        return Ok(None);
    };
    let data = Zeroizing::new(data);
    let header = (|| {
        let mut rest = data.strip_prefix(b"openssh-key-v1\0".as_slice())?;
        let cipher = string(&mut rest)?;
        string(&mut rest)?;
        string(&mut rest)?;
        (rest.get(..4)? == [0, 0, 0, 1]).then_some(())?;
        rest = &rest[4..];
        let mut public = string(&mut rest)?;
        let kind = string(&mut public)?;
        let section = string(&mut rest)?;
        Some((cipher, kind, section, rest))
    })();
    let Some((cipher, kind, section, tag)) = header else {
        return Ok(None);
    };
    let curve = match kind {
        b"ecdsa-sha2-nistp256" => Curve::P256,
        b"ecdsa-sha2-nistp384" => Curve::P384,
        b"ecdsa-sha2-nistp521" => Curve::P521,
        _ => return Ok(None),
    };
    let invalid = || anyhow!("invalid OpenSSH key");
    let encrypted = cipher != b"none";
    let mut plain = Zeroizing::new(section.to_vec());
    if encrypted {
        let key = PrivateKey::from_openssh(block).map_err(|_| invalid())?;
        let pass = passphrase(pass)?;
        let cipher = key.cipher();
        let tag = match cipher.has_tag() {
            true => Some(tag.get(..16).ok_or_else(invalid)?.try_into()?),
            false => None,
        };
        let (k, iv) = key
            .kdf()
            .derive_key_and_iv(cipher, pass)
            .map_err(|_| invalid())?;
        cipher
            .decrypt(&k, &iv, &mut plain, tag)
            .map_err(|_| Locked::WrongPassphrase)?;
    }
    let mut p: &[u8] = &plain;
    match (p.get(..4), p.get(4..8)) {
        (Some(a), Some(b)) if a == b => p = &p[8..],
        _ if encrypted => return Err(Locked::WrongPassphrase.into()),
        _ => return Err(invalid()),
    }
    let (Some(_), Some(_), Some(point), Some(d), Some(comment)) = (
        string(&mut p),
        string(&mut p),
        string(&mut p),
        string(&mut p),
        string(&mut p),
    ) else {
        return Err(invalid());
    };
    let d = &d[d.iter().take_while(|b| **b == 0).count()..];
    let size = curve.size().checked_sub(d.len()).ok_or_else(invalid)?;
    let mut padded = Zeroizing::new(vec![0; size]);
    padded.extend_from_slice(d);
    ensure!(
        Secret::ec_point(curve, &padded).ok().as_deref() == Some(point),
        invalid()
    );
    let comment = String::from_utf8_lossy(comment).into_owned();
    Ok(Some((Secret::Ec(curve, padded), comment, encrypted)))
}

fn passphrase(passphrase: Option<&str>) -> Result<&str> {
    Ok(passphrase
        .filter(|p| !p.is_empty())
        .ok_or(Locked::NeedsPassphrase)?)
}

/// Reads a private key in any supported format; `passphrase` is only used when it is encrypted.
pub(crate) fn parse(text: &str, pass: Option<&str>) -> Result<Parsed> {
    let text = text.trim_start_matches('\u{feff}').trim();
    let parsed = |secret, format, encrypted| Parsed {
        secret,
        comment: String::new(),
        format,
        encrypted,
    };
    if text.starts_with("PuTTY-User-Key-File-") {
        let (secret, comment, encrypted) = ppk::decode(text, pass)?;
        return Ok(Parsed {
            comment,
            ..parsed(secret, Format::Ppk, encrypted)
        });
    }
    if text.starts_with('{') {
        let (jwk, set) = jwk_object(text, true)?;
        let format = if set { Format::Jwks } else { Format::Jwk };
        return Ok(parsed(jwk_secret(&jwk)?, format, false));
    }
    let (label, block) = pem_block(text, "PRIVATE KEY").context(
        "not a private key TermF reads: OpenSSH, PEM (PKCS#1, SEC1, PKCS#8), JWK or PuTTY",
    )?;
    if block.contains("Proc-Type:") {
        bail!(
            "legacy encrypted PEM (DEK-Info) is not supported: re-export it as PKCS#8 (openssl pkcs8 -topk8)"
        );
    }
    let der = || {
        SecretDocument::from_pem(block)
            .map(|(_, doc)| doc)
            .map_err(|_| anyhow!("invalid PEM block"))
    };
    match label {
        "OPENSSH PRIVATE KEY" => {
            let read = || -> Result<Parsed> {
                let key =
                    PrivateKey::from_openssh(block).map_err(|_| anyhow!("invalid OpenSSH key"))?;
                let encrypted = key.is_encrypted();
                let key = match encrypted {
                    true => key
                        .decrypt(passphrase(pass)?)
                        .map_err(|_| Locked::WrongPassphrase)?,
                    false => key,
                };
                Ok(Parsed {
                    comment: key.comment().to_owned(),
                    ..parsed(
                        Secret::from_ssh(key.key_data())?,
                        Format::Openssh,
                        encrypted,
                    )
                })
            };
            match openssh_ecdsa(block, pass)? {
                Some((secret, comment, encrypted)) => Ok(Parsed {
                    comment,
                    ..parsed(secret, Format::Openssh, encrypted)
                }),
                None => read(),
            }
        }
        "PRIVATE KEY" => Ok(parsed(from_pkcs8(der()?.as_bytes())?, Format::Pkcs8, false)),
        "ENCRYPTED PRIVATE KEY" => {
            let doc = der()?;
            let info = EncryptedPrivateKeyInfo::try_from(doc.as_bytes())
                .map_err(|_| anyhow!("invalid PKCS#8 key"))?;
            let decrypted = match info.decrypt(passphrase(pass)?) {
                Ok(doc) => doc,
                Err(pkcs8::Error::EncryptedPrivateKey(
                    e @ (pkcs8::pkcs5::Error::UnsupportedAlgorithm { .. }
                    | pkcs8::pkcs5::Error::AlgorithmParametersInvalid { .. }
                    | pkcs8::pkcs5::Error::NoPbes1CryptSupport),
                )) => bail!("unsupported PKCS#8 encryption ({e})"),
                // pkcs5 reports a failed unpadding as `EncryptFailed` as well as `DecryptFailed`.
                Err(_) => return Err(Locked::WrongPassphrase.into()),
            };
            // A wrong passphrase can still unpad by chance: what follows is then not DER. Readable DER that is
            // no key TermF takes is said as such, never as a wrong passphrase.
            if PrivateKeyInfo::try_from(decrypted.as_bytes()).is_err() {
                return Err(Locked::WrongPassphrase.into());
            }
            let secret = from_pkcs8(decrypted.as_bytes())?;
            Ok(parsed(secret, Format::Pkcs8, true))
        }
        "RSA PRIVATE KEY" => {
            let key = RsaPrivateKey::from_pkcs1_der(der()?.as_bytes())
                .map_err(|_| anyhow!("invalid PKCS#1 key"))?;
            Ok(parsed(Secret::Rsa(Box::new(key)), Format::Pkcs1, false))
        }
        "EC PRIVATE KEY" => {
            let doc = der()?;
            for curve in CURVES {
                if let Ok(d) = on_curve!(curve, c => c::SecretKey::from_sec1_der(doc.as_bytes()).map(|k| Zeroizing::new(k.to_bytes().to_vec())))
                {
                    return Ok(parsed(Secret::Ec(curve, d), Format::Sec1, false));
                }
            }
            if let Some(secret) = explicit_ec(doc.as_bytes(), None) {
                return Ok(parsed(secret, Format::Sec1, false));
            }
            bail!("unsupported SEC1 key: P-256, P-384 or P-521 only")
        }
        _ => bail!("unsupported key type: Ed25519, X25519, ECDSA or RSA only"),
    }
}

/// Whether `text` holds a private key stored in clear, even one `parse` cannot read (DSA, `-sk`).
pub(crate) fn plain_private(text: &str) -> bool {
    let Some((label, block)) = pem_block(text, "PRIVATE KEY") else {
        return false;
    };
    if label != "OPENSSH PRIVATE KEY" {
        return !label.starts_with("ENCRYPTED") && !block.contains("Proc-Type:");
    }
    // openssh-key-v1: magic, then the cipher name, "none" when the key is not encrypted.
    let body: String = block
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .flat_map(|l| l.chars().filter(|c| !c.is_whitespace()))
        .collect();
    let Ok(bytes) = base64ct::Base64::decode_vec(&body).map(Zeroizing::new) else {
        return false;
    };
    let rest = bytes.strip_prefix(b"openssh-key-v1\0");
    rest.and_then(|r| r.get(..8)) == Some(&[0, 0, 0, 4, b'n', b'o', b'n', b'e'][..])
}

/// The public half of any supported format (or of a private key file), as DER SPKI.
pub(crate) fn parse_public(text: &str) -> Result<Vec<u8>> {
    let text = text.trim_start_matches('\u{feff}').trim();
    if text.starts_with("PuTTY-User-Key-File-") {
        return ssh_spki(&ppk::public(text)?);
    }
    if text.starts_with('{') {
        let (jwk, _) = jwk_object(text, false)?;
        return jwk_public(&jwk);
    }
    if let Some((label, block)) = pem_block(text, "PUBLIC KEY") {
        let (_, doc) = Document::from_pem(block).map_err(|_| anyhow!("invalid PEM block"))?;
        if label == "RSA PUBLIC KEY" {
            use rsa::pkcs1::DecodeRsaPublicKey;
            let key = rsa::RsaPublicKey::from_pkcs1_der(doc.as_bytes())
                .map_err(|_| anyhow!("invalid public key"))?;
            return Ok(key.to_public_key_der()?.into_vec());
        }
        SubjectPublicKeyInfoRef::try_from(doc.as_bytes())
            .map_err(|_| anyhow!("invalid public key"))?;
        return Ok(doc.into_vec());
    }
    if pem_block(text, "PRIVATE KEY").is_some() {
        bail!("this is a private key, not a public one");
    }
    let key = PublicKey::from_openssh(text.lines().next().unwrap_or_default())
        .map_err(|_| anyhow!("not a public key TermF reads: OpenSSH, SPKI PEM, JWK or PuTTY"))?;
    ssh_spki(key.key_data())
}

/// The key of a JWK, or the first of a set (the first private one when `private`).
fn jwk_object(text: &str, private: bool) -> Result<(serde_json::Map<String, Value>, bool)> {
    // serde_json's syntax errors carry a position, never the text: no key material leaks through them.
    let value: Value = serde_json::from_str(text)
        .map_err(|e| anyhow!("invalid JSON (line {}, column {})", e.line(), e.column()))?;
    let found = match value.get("keys").and_then(Value::as_array) {
        Some(keys) => (
            keys.iter()
                .find(|k| !private || k.get("d").is_some())
                .context(if private {
                    "this key set holds no private key"
                } else {
                    "this key set is empty"
                })?,
            true,
        ),
        None => (&value, false),
    };
    match found {
        (Value::Object(map), set) => Ok((map.clone(), set)),
        _ => bail!("not a JWK"),
    }
}

fn jwk_bytes(jwk: &serde_json::Map<String, Value>, name: &str) -> Result<Zeroizing<Vec<u8>>> {
    let text = jwk
        .get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("the JWK has no \"{name}\""))?;
    Base64UrlUnpadded::decode_vec(text)
        .map(Zeroizing::new)
        .map_err(|_| anyhow!("\"{name}\" is not base64url"))
}

fn jwk_kind(jwk: &serde_json::Map<String, Value>) -> (&str, &str) {
    let get = |k| jwk.get(k).and_then(Value::as_str).unwrap_or_default();
    (get("kty"), get("crv"))
}

fn jwk_curve(crv: &str) -> Option<Curve> {
    CURVES.into_iter().find(|c| c.jwk() == crv)
}

fn jwk_public(jwk: &serde_json::Map<String, Value>) -> Result<Vec<u8>> {
    match jwk_kind(jwk) {
        ("OKP", "Ed25519") => okp_spki(ED25519, &jwk_bytes(jwk, "x")?),
        ("OKP", "X25519") => okp_spki(X25519, &jwk_bytes(jwk, "x")?),
        ("EC", crv) => {
            let curve =
                jwk_curve(crv).context("unsupported JWK curve: P-256, P-384 or P-521 only")?;
            let mut point = vec![4u8];
            point.extend_from_slice(&jwk_bytes(jwk, "x")?);
            point.extend_from_slice(&jwk_bytes(jwk, "y")?);
            ec_spki(curve, &point)
        }
        ("RSA", _) => {
            let n = BigUint::from_bytes_be(&jwk_bytes(jwk, "n")?);
            let e = BigUint::from_bytes_be(&jwk_bytes(jwk, "e")?);
            Ok(rsa::RsaPublicKey::new(n, e)?
                .to_public_key_der()?
                .into_vec())
        }
        _ => bail!("unsupported JWK: OKP (Ed25519, X25519), EC or RSA only"),
    }
}

fn jwk_secret(jwk: &serde_json::Map<String, Value>) -> Result<Secret> {
    let seed = || -> Result<Zeroizing<[u8; 32]>> {
        let d = jwk_bytes(jwk, "d")?;
        let mut seed = Zeroizing::new([0u8; 32]);
        ensure!(d.len() == 32, "\"d\" has the wrong length");
        seed.copy_from_slice(&d);
        Ok(seed)
    };
    let secret = match jwk_kind(jwk) {
        ("OKP", "Ed25519") => Secret::Ed25519(seed()?),
        ("OKP", "X25519") => Secret::X25519(seed()?),
        ("EC", crv) => {
            let curve =
                jwk_curve(crv).context("unsupported JWK curve: P-256, P-384 or P-521 only")?;
            // Some producers drop leading zero bytes: stored full length, as RFC 7518 writes it.
            let short = jwk_bytes(jwk, "d")?;
            ensure!(short.len() <= curve.size(), "\"d\" has the wrong length");
            let mut d = Zeroizing::new(vec![0u8; curve.size()]);
            d[curve.size() - short.len()..].copy_from_slice(&short);
            Secret::ec_point(curve, &d).map_err(|_| anyhow!("\"d\" is not a valid private key"))?;
            Secret::Ec(curve, d)
        }
        ("RSA", _) => {
            let int = |name| jwk_bytes(jwk, name).map(|b| BigUint::from_bytes_be(&b));
            let primes = match (int("p"), int("q")) {
                (Ok(p), Ok(q)) => vec![p, q],
                _ => Vec::new(),
            };
            let key = RsaPrivateKey::from_components(int("n")?, int("e")?, int("d")?, primes)
                .map_err(|_| anyhow!("the RSA members do not form a valid key"))?;
            Secret::Rsa(Box::new(key))
        }
        _ => bail!("unsupported JWK: OKP (Ed25519, X25519), EC or RSA only"),
    };
    // A private JWK repeats its public members: they must be those of "d".
    if jwk.contains_key("x") || jwk.contains_key("n") {
        ensure!(
            jwk_public(jwk)? == secret.spki()?,
            "the JWK's public members do not match its private key"
        );
    }
    Ok(secret)
}
