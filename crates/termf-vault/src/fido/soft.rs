//! A simulated FIDO2 key for the tests, never a real one: the authenticator side of CTAP2's `clientPin`,
//! `makeCredential` and `getAssertion` with `hmac-secret`, written from the specification on its own so the
//! client's messages are checked rather than echoed.

use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use termf_sync::crypto;

use super::cbor::{Value, decode, encode};
use super::ctap::{self, Device, Shared, Status};
use super::{Ctap, SecurityKeys};

const UP: u8 = 0x01;
const UV: u8 = 0x04;
const AT: u8 = 0x40;
const ED: u8 = 0x80;

struct Credential {
    id: Vec<u8>,
    rp: [u8; 32],
    with_uv: [u8; 32],
    without_uv: [u8; 32],
}

struct State {
    pin: Option<String>,
    /// CTAP 2.1 (protocols 2 and 1, tokens with permissions) or 2.0 (protocol 1 only).
    v21: bool,
    agreement: p256::SecretKey,
    token: Option<([u8; 32], i64)>,
    credentials: Vec<Credential>,
    /// Whether the person touches the key when it asks.
    touched: bool,
    touches: usize,
}

#[derive(Clone)]
pub struct SoftKey(Arc<Mutex<State>>);

impl SoftKey {
    pub fn new(pin: Option<&str>) -> Self {
        Self(Arc::new(Mutex::new(State {
            pin: pin.map(Into::into),
            v21: true,
            agreement: ctap::secret_key(),
            token: None,
            credentials: Vec::new(),
            touched: true,
            touches: 0,
        })))
    }

    /// A CTAP 2.0 key: PIN protocol 1 and `getPinToken` only.
    pub fn v20(self) -> Self {
        self.0.lock().unwrap().v21 = false;
        self
    }

    pub fn touch(&self, touched: bool) {
        self.0.lock().unwrap().touched = touched;
    }

    pub fn touches(&self) -> usize {
        self.0.lock().unwrap().touches
    }

    pub fn device(&self) -> Box<dyn Device> {
        Box::new(self.clone())
    }
}

/// Security keys for a vault in tests: those in `plugged`, as the user plugs and unplugs them.
pub fn keys(plugged: Arc<Mutex<Vec<SoftKey>>>) -> Box<dyn SecurityKeys> {
    Box::new(Ctap(move || {
        Ok(plugged
            .lock()
            .unwrap()
            .iter()
            .map(SoftKey::device)
            .collect())
    }))
}

fn map(entries: Vec<(i64, Value)>) -> Value {
    Value::Map(
        entries
            .into_iter()
            .map(|(k, v)| (Value::Int(k), v))
            .collect(),
    )
}

fn invalid() -> anyhow::Error {
    Status(0x11).into()
}

impl State {
    fn shared(&self, protocol: u8, platform: &Value) -> Result<Shared> {
        let theirs = ctap::public_key(platform)?;
        Ok(Shared::agree(
            protocol,
            &self.agreement,
            &theirs,
            Value::Null,
        ))
    }

    fn info(&self) -> Value {
        let mut options = vec![
            (Value::text("rk"), Value::Bool(true)),
            (Value::text("up"), Value::Bool(true)),
            (Value::text("clientPin"), Value::Bool(self.pin.is_some())),
        ];
        if self.v21 {
            options.push((Value::text("pinUvAuthToken"), Value::Bool(true)));
        }
        let versions = if self.v21 {
            vec!["FIDO_2_0", "FIDO_2_1"]
        } else {
            vec!["FIDO_2_0"]
        };
        map(vec![
            (
                1,
                Value::Array(versions.into_iter().map(Value::text).collect()),
            ),
            (2, Value::Array(vec![Value::text("hmac-secret")])),
            (3, Value::bytes(&[7; 16])),
            (4, Value::Map(options)),
            (
                6,
                Value::Array(if self.v21 {
                    vec![Value::Int(2), Value::Int(1)]
                } else {
                    vec![Value::Int(1)]
                }),
            ),
        ])
    }

    fn client_pin(&mut self, request: &Value) -> Result<Value> {
        let protocol = request.at(1).and_then(Value::as_int).ok_or_else(invalid)? as u8;
        if protocol == 2 && !self.v21 {
            bail!(invalid());
        }
        match request.at(2).and_then(Value::as_int) {
            Some(2) => Ok(map(vec![(1, ctap::encoded(&self.agreement.public_key()))])),
            Some(sub @ (5 | 9)) => {
                if sub == 9 && !self.v21 {
                    bail!(invalid());
                }
                let Some(pin) = self.pin.clone() else {
                    bail!(Status(0x35))
                };
                let shared = self.shared(protocol, request.at(3).ok_or_else(invalid)?)?;
                let sent = shared.decrypt(
                    request
                        .at(6)
                        .and_then(Value::as_bytes)
                        .ok_or_else(invalid)?,
                )?;
                if sent.as_slice() != &ctap::sha256(pin.as_bytes())[..16] {
                    // A new agreement after a wrong PIN, as keys do.
                    self.agreement = ctap::secret_key();
                    bail!(Status(0x31));
                }
                if sub == 9 && request.at(10).and_then(Value::as_text) != Some(super::RP_ID) {
                    bail!(invalid());
                }
                let permissions = if sub == 9 {
                    request.at(9).and_then(Value::as_int).ok_or_else(invalid)?
                } else {
                    3
                };
                let token: [u8; 32] = crypto::random();
                self.token = Some((token, permissions));
                Ok(map(vec![(2, Value::Bytes(shared.encrypt(&token)))]))
            }
            _ => bail!(invalid()),
        }
    }

    /// The PIN was checked for this operation; the token serves once, as CTAP 2.1 has it.
    fn verified(&mut self, request: &Value, at: i64, data: &[u8], permission: i64) -> Result<()> {
        let protocol = request
            .at(at + 1)
            .and_then(Value::as_int)
            .ok_or_else(invalid)? as u8;
        let param = request
            .at(at)
            .and_then(Value::as_bytes)
            .ok_or(Status(0x36))?;
        let Some((token, permissions)) = self.token.take() else {
            bail!(Status(0x33))
        };
        if permissions & permission == 0 || ctap::authenticate(protocol, &token, data) != param {
            bail!(Status(0x33));
        }
        Ok(())
    }

    fn touch(&mut self) -> Result<()> {
        if !self.touched {
            bail!(Status(0x2f));
        }
        self.touches += 1;
        Ok(())
    }

    fn make_credential(&mut self, request: &Value) -> Result<Value> {
        let client_data = request
            .at(1)
            .and_then(Value::as_bytes)
            .ok_or_else(invalid)?
            .to_vec();
        let rp = request
            .at(2)
            .and_then(|r| r.field("id"))
            .and_then(Value::as_text)
            .ok_or_else(invalid)?;
        let rp = ctap::sha256(rp.as_bytes());
        self.verified(request, 8, &client_data, 1)?;
        let excluded = request.at(5).and_then(Value::as_array).unwrap_or_default();
        let held = excluded
            .iter()
            .filter_map(|c| c.field("id")?.as_bytes())
            .any(|id| self.credentials.iter().any(|c| c.id == id));
        self.touch()?;
        if held {
            bail!(Status(0x19));
        }
        let hmac_secret = request
            .at(6)
            .and_then(|e| e.field("hmac-secret"))
            .and_then(Value::as_bool)
            == Some(true);
        let id = crypto::random::<48>().to_vec();
        self.credentials.push(Credential {
            id: id.clone(),
            rp,
            with_uv: crypto::random(),
            without_uv: crypto::random(),
        });
        let mut data = rp.to_vec();
        data.push(UP | UV | AT | ED);
        data.extend(1u32.to_be_bytes());
        data.extend([7u8; 16]);
        data.extend((id.len() as u16).to_be_bytes());
        data.extend(&id);
        data.extend(encode(&ctap::encoded(&ctap::secret_key().public_key())));
        data.extend(encode(&Value::Map(vec![(
            Value::text("hmac-secret"),
            Value::Bool(hmac_secret),
        )])));
        Ok(map(vec![
            (1, Value::text("none")),
            (2, Value::Bytes(data)),
            (3, Value::Map(Vec::new())),
        ]))
    }

    fn get_assertion(&mut self, request: &Value) -> Result<Value> {
        let rp = request.at(1).and_then(Value::as_text).ok_or_else(invalid)?;
        let rp = ctap::sha256(rp.as_bytes());
        let client_data = request
            .at(2)
            .and_then(Value::as_bytes)
            .ok_or_else(invalid)?
            .to_vec();
        let allowed: Vec<&[u8]> = request
            .at(3)
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(|c| c.field("id")?.as_bytes())
            .collect();
        let Some(i) = self
            .credentials
            .iter()
            .position(|c| c.rp == rp && allowed.contains(&c.id.as_slice()))
        else {
            bail!(Status(0x2e))
        };
        let id = self.credentials[i].id.clone();
        let silent = request
            .at(5)
            .and_then(|o| o.field("up"))
            .and_then(Value::as_bool)
            == Some(false);
        let mut flags = 0;
        let mut extensions = None;
        if !silent {
            let uv = request.at(6).is_some();
            if uv {
                self.verified(request, 6, &client_data, 2)?;
            }
            self.touch()?;
            flags = UP | if uv { UV } else { 0 };
            if let Some(input) = request.at(4).and_then(|e| e.field("hmac-secret")) {
                let protocol = input.at(4).and_then(Value::as_int).unwrap_or(1) as u8;
                let shared = self.shared(protocol, input.at(1).ok_or_else(invalid)?)?;
                let salt_enc = input.at(2).and_then(Value::as_bytes).ok_or_else(invalid)?;
                if input.at(3).and_then(Value::as_bytes)
                    != Some(shared.authenticate(salt_enc).as_slice())
                {
                    bail!(Status(0x2c));
                }
                let salt = shared.decrypt(salt_enc)?;
                if salt.len() != 32 {
                    bail!(invalid());
                }
                let c = &self.credentials[i];
                let random = if uv { c.with_uv } else { c.without_uv };
                let output = crate::hmac256(&random, &[&salt]);
                extensions = Some(Value::Map(vec![(
                    Value::text("hmac-secret"),
                    Value::Bytes(shared.encrypt(&output)),
                )]));
                flags |= ED;
            }
        }
        let mut data = rp.to_vec();
        data.push(flags);
        data.extend(2u32.to_be_bytes());
        if let Some(ext) = &extensions {
            data.extend(encode(ext));
        }
        let credential = Value::Map(vec![
            (Value::text("id"), Value::Bytes(id)),
            (Value::text("type"), Value::text("public-key")),
        ]);
        Ok(map(vec![
            (1, credential),
            (2, Value::Bytes(data)),
            (3, Value::bytes(&[0; 64])),
        ]))
    }
}

impl Device for SoftKey {
    fn cbor(&mut self, command: u8, request: &[u8]) -> Result<Vec<u8>> {
        let request = if request.is_empty() {
            Value::Map(Vec::new())
        } else {
            decode(request)?.0
        };
        let mut state = self.0.lock().unwrap();
        let answer = match command {
            0x04 => state.info(),
            0x06 => state.client_pin(&request)?,
            0x01 => state.make_credential(&request)?,
            0x02 => state.get_assertion(&request)?,
            _ => bail!(Status(0x01)),
        };
        Ok(encode(&answer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIN: &str = "4242";

    fn plugged(keys: &[&SoftKey]) -> (Arc<Mutex<Vec<SoftKey>>>, Box<dyn SecurityKeys>) {
        let plugged = Arc::new(Mutex::new(keys.iter().map(|k| (*k).clone()).collect()));
        (plugged.clone(), super::keys(plugged))
    }

    #[test]
    fn gives_the_same_secret_for_the_same_key_and_salt_only() {
        for key in [SoftKey::new(Some(PIN)), SoftKey::new(Some(PIN)).v20()] {
            let (_, keys) = plugged(&[&key]);
            let input = [3u8; 32];
            let (id, secret) = keys.enroll(b"user", &[], &input, Some(PIN)).unwrap();
            assert_eq!(key.touches(), 2, "one touch to make it, one to read it");
            let (used, again) = keys
                .secret(std::slice::from_ref(&id), &input, Some(PIN))
                .unwrap();
            assert_eq!((used.as_slice(), *again), (id.as_slice(), *secret));
            let (_, other) = keys
                .secret(std::slice::from_ref(&id), &[4u8; 32], Some(PIN))
                .unwrap();
            assert_ne!(*other, *secret, "another salt");
        }
    }

    #[test]
    fn says_why_it_cannot() {
        let key = SoftKey::new(Some(PIN));
        let (plugged_in, keys) = plugged(&[&key]);
        let input = [3u8; 32];
        let err =
            |r: Result<(Vec<u8>, super::super::Output)>| r.map(|_| ()).unwrap_err().to_string();
        assert_eq!(
            err(keys.enroll(b"u", &[], &input, None)),
            "the security key's PIN is needed"
        );
        assert_eq!(
            err(keys.enroll(b"u", &[], &input, Some("0000"))),
            "wrong PIN for the security key"
        );
        assert_eq!(
            err(keys.enroll(b"u", &[], &input, Some("12"))),
            "a security key's PIN is 4 to 63 characters"
        );
        let (id, _) = keys.enroll(b"u", &[], &input, Some(PIN)).unwrap();
        assert_eq!(
            err(keys.enroll(b"u", std::slice::from_ref(&id), &input, Some(PIN))),
            "this security key is already enrolled"
        );
        key.touch(false);
        assert_eq!(
            err(keys.secret(std::slice::from_ref(&id), &input, Some(PIN))),
            "the security key was not touched in time"
        );
        key.touch(true);

        let other = SoftKey::new(Some(PIN));
        *plugged_in.lock().unwrap() = vec![other.clone()];
        assert_eq!(
            err(keys.secret(std::slice::from_ref(&id), &input, Some(PIN))),
            "this security key is not one of the vault's"
        );
        plugged_in.lock().unwrap().clear();
        assert_eq!(
            err(keys.secret(std::slice::from_ref(&id), &input, Some(PIN))),
            "plug in your security key"
        );

        let bare = SoftKey::new(None);
        *plugged_in.lock().unwrap() = vec![bare];
        assert!(
            err(keys.enroll(b"u", &[], &input, Some(PIN)))
                .starts_with("this security key has no PIN yet")
        );
    }

    #[test]
    fn finds_the_right_key_among_several() {
        let (first, second) = (SoftKey::new(Some(PIN)), SoftKey::new(Some(PIN)));
        let (plugged_in, keys) = plugged(&[&first]);
        let input = [5u8; 32];
        let (a, secret_a) = keys.enroll(b"u", &[], &input, Some(PIN)).unwrap();
        // The enrolled key stays plugged in while the backup one is added.
        plugged_in.lock().unwrap().push(second.clone());
        let (b, secret_b) = keys
            .enroll(b"u", std::slice::from_ref(&a), &input, Some(PIN))
            .unwrap();
        assert_eq!(
            (first.touches(), second.touches()),
            (2, 2),
            "probed without a touch"
        );
        assert_ne!(*secret_a, *secret_b);
        let both = [a.clone(), b.clone()];
        assert_eq!(keys.secret(&both, &input, Some(PIN)).unwrap().0, a);
        plugged_in.lock().unwrap().remove(0);
        let (used, secret) = keys.secret(&both, &input, Some(PIN)).unwrap();
        assert_eq!((used, *secret), (b, *secret_b));
    }
}
