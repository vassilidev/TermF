//! CTAP 2.0 / 2.1 as the vault needs it: a non-discoverable credential with `hmac-secret`, made and used after
//! the key's PIN (PIN/UV auth protocol 1 or 2), so the secret is the one a key gives with user verification.

use anyhow::{Context, Result, anyhow, bail, ensure};
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use sha2::{Digest, Sha256};
use termf_sync::crypto;
use zeroize::Zeroizing;

use super::cbor::{Value, decode, encode};
use super::{Output, RP_ID};

const MAKE_CREDENTIAL: u8 = 0x01;
const GET_ASSERTION: u8 = 0x02;
const GET_INFO: u8 = 0x04;
const CLIENT_PIN: u8 = 0x06;

/// What a key answers with, besides success: the ones a person can act on get words.
#[derive(Debug, PartialEq, Eq)]
pub struct Status(pub u8);

pub const CREDENTIAL_EXCLUDED: Status = Status(0x19);
pub const NO_CREDENTIALS: Status = Status(0x2e);
pub const PIN_NOT_SET: Status = Status(0x35);
pub const TIMEOUT: Status = Status(0x2f);

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self.0 {
            0x19 => "this security key is already enrolled",
            0x27 | 0x2f => "the security key was not touched in time",
            0x28 => "the security key is full",
            0x2e => "this security key is not one of the vault's",
            0x31 => "wrong PIN for the security key",
            0x32 => "the security key's PIN is blocked after too many tries",
            0x34 => "too many wrong PINs: unplug the security key and plug it in again",
            0x35 => "this security key has no PIN yet: set one with its maker's app or a browser's settings first",
            0x36 => "the security key needs its PIN",
            0x37 => "this PIN is too short or too long for the security key",
            code => return write!(f, "the security key refused (CTAP error 0x{code:02x})"),
        })
    }
}

impl std::error::Error for Status {}

/// One key, spoken to in CTAP2 CBOR messages; `cbor` returns the response after a success status.
pub trait Device {
    fn cbor(&mut self, command: u8, request: &[u8]) -> Result<Vec<u8>>;
}

fn call(device: &mut dyn Device, command: u8, request: Option<Value>) -> Result<Value> {
    let request = request.map(|r| encode(&r)).unwrap_or_default();
    let response = device.cbor(command, &request)?;
    if response.is_empty() {
        return Ok(Value::Map(Vec::new()));
    }
    Ok(decode(&response)
        .context("the security key answered garbage")?
        .0)
}

fn int_map(entries: Vec<(i64, Value)>) -> Value {
    Value::Map(
        entries
            .into_iter()
            .map(|(k, v)| (Value::Int(k), v))
            .collect(),
    )
}

fn text_map(entries: Vec<(&str, Value)>) -> Value {
    Value::Map(
        entries
            .into_iter()
            .map(|(k, v)| (Value::text(k), v))
            .collect(),
    )
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// WebAuthn's PRF extension turns an input into the `hmac-secret` salt this way; Windows does it itself, so the
/// two paths give a key the same salt.
pub fn prf_salt(input: &[u8; 32]) -> [u8; 32] {
    sha256(&[b"WebAuthn PRF".as_slice(), &[0], input].concat())
}

pub struct Info {
    pub hmac_secret: bool,
    pub pin_set: bool,
    /// CTAP 2.1's `getPinUvAuthTokenUsingPinWithPermissions`.
    pub permissions: bool,
    pub protocol: u8,
}

pub fn info(device: &mut dyn Device) -> Result<Info> {
    let info = call(device, GET_INFO, None)?;
    let extensions = info.at(2).and_then(Value::as_array).unwrap_or_default();
    let options = info.at(4);
    let option = |name: &str| options.and_then(|o| o.field(name)).and_then(Value::as_bool);
    let protocols: Vec<i64> = info
        .at(6)
        .and_then(Value::as_array)
        .map(|p| p.iter().filter_map(Value::as_int).collect())
        .unwrap_or_else(|| vec![1]);
    let permissions = option("pinUvAuthToken") == Some(true);
    let protocol = if permissions && protocols.contains(&2) {
        2
    } else if protocols.contains(&1) {
        1
    } else {
        bail!("this security key speaks no PIN protocol TermF knows")
    };
    Ok(Info {
        hmac_secret: extensions
            .iter()
            .any(|e| e.as_text() == Some("hmac-secret")),
        pin_set: option("clientPin") == Some(true),
        permissions,
        protocol,
    })
}

/// A shared secret with the key, by ECDH on P-256 (PIN/UV auth protocol 1 or 2).
pub struct Shared {
    pub protocol: u8,
    hmac_key: Zeroizing<[u8; 32]>,
    aes_key: Zeroizing<[u8; 32]>,
    /// Our half of the agreement, sent with each message that uses the secret.
    pub cose: Value,
}

pub fn cose_key(x: &[u8], y: &[u8]) -> Value {
    int_map(vec![
        (1, Value::Int(2)),
        (3, Value::Int(-25)),
        (-1, Value::Int(1)),
        (-2, Value::bytes(x)),
        (-3, Value::bytes(y)),
    ])
}

/// A P-256 public key from a COSE key.
pub fn public_key(cose: &Value) -> Result<p256::PublicKey> {
    let coordinate = |k| {
        cose.at(k)
            .and_then(Value::as_bytes)
            .filter(|c| c.len() == 32)
            .context("the security key sent a malformed public key")
    };
    let point = [&[4u8][..], coordinate(-2)?, coordinate(-3)?].concat();
    p256::PublicKey::from_sec1_bytes(&point)
        .map_err(|_| anyhow!("the security key sent an invalid public key"))
}

pub fn secret_key() -> p256::SecretKey {
    loop {
        // Out of range once in 2^32 draws.
        if let Ok(key) = p256::SecretKey::from_slice(&crypto::random::<32>()) {
            return key;
        }
    }
}

pub fn encoded(key: &p256::PublicKey) -> Value {
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    let point = key.to_encoded_point(false);
    cose_key(
        point.x().expect("uncompressed"),
        point.y().expect("uncompressed"),
    )
}

impl Shared {
    pub fn agree(
        protocol: u8,
        ours: &p256::SecretKey,
        theirs: &p256::PublicKey,
        cose: Value,
    ) -> Self {
        let z = p256::ecdh::diffie_hellman(ours.to_nonzero_scalar(), theirs.as_affine());
        let z = z.raw_secret_bytes();
        let (hmac_key, aes_key) = if protocol == 1 {
            let k = sha256(z);
            (k, k)
        } else {
            let hk = hkdf::Hkdf::<Sha256>::new(Some(&[0u8; 32]), z);
            let mut h = [0u8; 32];
            let mut a = [0u8; 32];
            hk.expand(b"CTAP2 HMAC key", &mut h).expect("32 bytes");
            hk.expand(b"CTAP2 AES key", &mut a).expect("32 bytes");
            (h, a)
        };
        Self {
            protocol,
            hmac_key: Zeroizing::new(hmac_key),
            aes_key: Zeroizing::new(aes_key),
            cose,
        }
    }

    pub fn encrypt(&self, plain: &[u8]) -> Vec<u8> {
        let iv: [u8; 16] = if self.protocol == 1 {
            [0; 16]
        } else {
            crypto::random()
        };
        let mut buf = plain.to_vec();
        cbc::Encryptor::<aes::Aes256>::new(self.aes_key.as_ref().into(), &iv.into())
            .encrypt_padded_mut::<NoPadding>(&mut buf, plain.len())
            .expect("whole blocks");
        if self.protocol == 1 {
            buf
        } else {
            [&iv[..], &buf].concat()
        }
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        let (iv, body) = if self.protocol == 1 {
            ([0u8; 16], data)
        } else {
            let (iv, body) = data.split_at_checked(16).context("too short")?;
            (iv.try_into()?, body)
        };
        ensure!(
            !body.is_empty() && body.len() % 16 == 0,
            "the security key sent a malformed message"
        );
        let mut buf = Zeroizing::new(body.to_vec());
        cbc::Decryptor::<aes::Aes256>::new(self.aes_key.as_ref().into(), &iv.into())
            .decrypt_padded_mut::<NoPadding>(&mut buf)
            .map_err(|_| anyhow!("the security key sent a malformed message"))?;
        Ok(buf)
    }

    /// The MAC over `message` with the shared secret.
    pub fn authenticate(&self, message: &[u8]) -> Vec<u8> {
        authenticate(self.protocol, self.hmac_key.as_ref(), message)
    }
}

/// Protocol 1 keeps the first 16 bytes of the HMAC, protocol 2 all of it.
pub fn authenticate(protocol: u8, key: &[u8], message: &[u8]) -> Vec<u8> {
    let mac = crate::hmac256(key, &[message]);
    if protocol == 1 {
        mac[..16].to_vec()
    } else {
        mac.to_vec()
    }
}

fn agree(device: &mut dyn Device, protocol: u8) -> Result<Shared> {
    let answer = call(
        device,
        CLIENT_PIN,
        Some(int_map(vec![
            (1, Value::Int(protocol.into())),
            (2, Value::Int(2)),
        ])),
    )?;
    let theirs = public_key(
        answer
            .at(1)
            .context("the security key sent no key agreement")?,
    )?;
    let ours = secret_key();
    let cose = encoded(&ours.public_key());
    Ok(Shared::agree(protocol, &ours, &theirs, cose))
}

const PERMISSION_MAKE: i64 = 0x01;
const PERMISSION_GET: i64 = 0x02;

/// A PIN/UV auth token for one operation, after the PIN.
fn token(
    device: &mut dyn Device,
    info: &Info,
    shared: &Shared,
    pin: &str,
    permission: i64,
) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        (4..=63).contains(&pin.len()),
        "a security key's PIN is 4 to 63 characters"
    );
    let hash = Zeroizing::new(sha256(pin.as_bytes()));
    let mut request = vec![
        (1, Value::Int(shared.protocol.into())),
        (3, shared.cose.clone()),
        (6, Value::Bytes(shared.encrypt(&hash[..16]))),
    ];
    if info.permissions {
        request.extend([
            (2, Value::Int(9)),
            (9, Value::Int(permission)),
            (10, Value::text(RP_ID)),
        ]);
    } else {
        request.push((2, Value::Int(5)));
    }
    let answer = call(device, CLIENT_PIN, Some(int_map(request)))?;
    let sealed = answer
        .at(2)
        .and_then(Value::as_bytes)
        .context("the security key sent no token")?;
    shared.decrypt(sealed)
}

fn check_info(device: &mut dyn Device) -> Result<Info> {
    let info = info(device)?;
    ensure!(
        info.hmac_secret,
        "this security key has no hmac-secret: it cannot protect the vault"
    );
    if !info.pin_set {
        bail!(PIN_NOT_SET);
    }
    Ok(info)
}

/// The authenticator data's parts the vault reads.
pub struct AuthData {
    pub flags: u8,
    pub credential: Option<Vec<u8>>,
    pub extensions: Option<Value>,
}

pub const UV: u8 = 0x04;
const AT: u8 = 0x40;
const ED: u8 = 0x80;

pub fn auth_data(data: &[u8]) -> Result<AuthData> {
    let bad = || anyhow!("the security key sent malformed authenticator data");
    ensure!(data.len() >= 37, bad());
    ensure!(
        data[..32] == sha256(RP_ID.as_bytes()),
        "the security key answered for another site"
    );
    let flags = data[32];
    let mut at = 37;
    let mut credential = None;
    if flags & AT != 0 {
        let len = data.get(53..55).ok_or_else(bad)?;
        let len = usize::from(u16::from_be_bytes([len[0], len[1]]));
        credential = Some(data.get(55..55 + len).ok_or_else(bad)?.to_vec());
        at = 55 + len;
        let (_, used) = decode(data.get(at..).ok_or_else(bad)?)?;
        at += used;
    }
    let extensions = if flags & ED != 0 {
        let (ext, used) = decode(data.get(at..).ok_or_else(bad)?)?;
        at += used;
        Some(ext)
    } else {
        None
    };
    ensure!(at == data.len(), bad());
    Ok(AuthData {
        flags,
        credential,
        extensions,
    })
}

fn public_key_credential(id: &[u8]) -> Value {
    text_map(vec![
        ("id", Value::bytes(id)),
        ("type", Value::text("public-key")),
    ])
}

/// A new credential with `hmac-secret` on this key, unless it holds one of `exclude`.
pub fn make_credential(
    device: &mut dyn Device,
    user: &[u8],
    exclude: &[Vec<u8>],
    pin: &str,
) -> Result<Vec<u8>> {
    let info = check_info(device)?;
    let shared = agree(device, info.protocol)?;
    let token = token(device, &info, &shared, pin, PERMISSION_MAKE)?;
    let client_data = crypto::random::<32>();
    let mut request = vec![
        (1, Value::bytes(&client_data)),
        (
            2,
            text_map(vec![
                ("id", Value::text(RP_ID)),
                ("name", Value::text("TermF")),
            ]),
        ),
        (
            3,
            text_map(vec![
                ("id", Value::bytes(user)),
                ("name", Value::text("vault")),
                ("displayName", Value::text("TermF vault")),
            ]),
        ),
        (
            4,
            Value::Array(vec![text_map(vec![
                ("alg", Value::Int(-7)),
                ("type", Value::text("public-key")),
            ])]),
        ),
        (6, text_map(vec![("hmac-secret", Value::Bool(true))])),
        (7, text_map(vec![("rk", Value::Bool(false))])),
        (
            8,
            Value::Bytes(authenticate(info.protocol, &token, &client_data)),
        ),
        (9, Value::Int(info.protocol.into())),
    ];
    if !exclude.is_empty() {
        request.push((
            5,
            Value::Array(exclude.iter().map(|id| public_key_credential(id)).collect()),
        ));
    }
    let answer = call(device, MAKE_CREDENTIAL, Some(int_map(request)))?;
    let data = auth_data(
        answer
            .at(2)
            .and_then(Value::as_bytes)
            .context("the security key sent no authenticator data")?,
    )?;
    let enabled = data
        .extensions
        .as_ref()
        .and_then(|e| e.field("hmac-secret"))
        .and_then(Value::as_bool);
    ensure!(
        enabled == Some(true),
        "the security key did not turn hmac-secret on for the vault"
    );
    data.credential
        .context("the security key sent no credential")
}

/// Whether this key holds one of `credentials`, asked without a touch.
pub fn holds(device: &mut dyn Device, credentials: &[Vec<u8>]) -> Result<bool> {
    let request = int_map(vec![
        (1, Value::text(RP_ID)),
        (2, Value::bytes(&crypto::random::<32>())),
        (
            3,
            Value::Array(
                credentials
                    .iter()
                    .map(|id| public_key_credential(id))
                    .collect(),
            ),
        ),
        (5, text_map(vec![("up", Value::Bool(false))])),
    ]);
    match call(device, GET_ASSERTION, Some(request)) {
        Ok(_) => Ok(true),
        Err(e) if e.downcast_ref::<Status>() == Some(&NO_CREDENTIALS) => Ok(false),
        Err(e) => Err(e),
    }
}

/// The `hmac-secret` of whichever of `credentials` this key holds, for `salt` (already the PRF salt), after its
/// PIN and a touch.
pub fn get_secret(
    device: &mut dyn Device,
    credentials: &[Vec<u8>],
    salt: &[u8; 32],
    pin: &str,
) -> Result<(Vec<u8>, Output)> {
    let info = check_info(device)?;
    let shared = agree(device, info.protocol)?;
    let token = token(device, &info, &shared, pin, PERMISSION_GET)?;
    let client_data = crypto::random::<32>();
    let salt_enc = shared.encrypt(salt);
    let mut hmac_secret = vec![
        (1, shared.cose.clone()),
        (3, Value::Bytes(shared.authenticate(&salt_enc))),
        (2, Value::Bytes(salt_enc)),
    ];
    if shared.protocol == 2 {
        hmac_secret.push((4, Value::Int(2)));
    }
    let request = int_map(vec![
        (1, Value::text(RP_ID)),
        (2, Value::bytes(&client_data)),
        (
            3,
            Value::Array(
                credentials
                    .iter()
                    .map(|id| public_key_credential(id))
                    .collect(),
            ),
        ),
        (4, text_map(vec![("hmac-secret", int_map(hmac_secret))])),
        (
            6,
            Value::Bytes(authenticate(info.protocol, &token, &client_data)),
        ),
        (7, Value::Int(info.protocol.into())),
    ]);
    let answer = call(device, GET_ASSERTION, Some(request))?;
    let credential = match answer
        .at(1)
        .and_then(|c| c.field("id"))
        .and_then(Value::as_bytes)
    {
        Some(id) => id.to_vec(),
        // A key may leave it out when only one was allowed.
        None if credentials.len() == 1 => credentials[0].clone(),
        None => bail!("the security key did not say which credential it used"),
    };
    ensure!(
        credentials.contains(&credential),
        "the security key used a credential that is not the vault's"
    );
    let data = auth_data(
        answer
            .at(2)
            .and_then(Value::as_bytes)
            .context("the security key sent no authenticator data")?,
    )?;
    ensure!(
        data.flags & UV != 0,
        "the security key did not check its PIN"
    );
    let sealed = data
        .extensions
        .as_ref()
        .and_then(|e| e.field("hmac-secret"))
        .and_then(Value::as_bytes)
        .context("the security key sent no hmac-secret")?;
    let plain = shared.decrypt(sealed)?;
    let mut output = Zeroizing::new([0u8; 32]);
    output.copy_from_slice(
        plain
            .get(..32)
            .filter(|_| plain.len() == 32)
            .context("unexpected hmac-secret length")?,
    );
    Ok((credential, output))
}
