//! Security keys (FIDO2) as the vault's second factor, through their `hmac-secret` extension: for a credential it
//! made and a salt, a key gives back a 32-byte secret, only once touched and after its PIN. macOS and Linux speak
//! CTAP2 to the keys over USB HID; Windows lets only its WebAuthn API reach them ([`webauthn`]).

use anyhow::{Context, Result, bail};
use zeroize::Zeroizing;

// Windows reaches keys through webauthn.dll only: CTAP2 is spoken there in tests alone.
#[cfg(any(not(windows), test))]
mod cbor;
#[cfg(any(not(windows), test))]
pub(crate) mod ctap;
#[cfg(any(not(windows), test))]
mod hid;
#[cfg(test)]
pub(crate) mod soft;
#[cfg(windows)]
mod webauthn;

#[cfg(any(not(windows), test))]
use ctap::{Device, NO_CREDENTIALS};

/// The relying party every vault credential is made for.
pub const RP_ID: &str = "dev.termf.vault";

pub type Output = Zeroizing<[u8; 32]>;

pub trait SecurityKeys: Send + Sync {
    /// TermF asks for the key's PIN itself (macOS, Linux); Windows asks in its own window.
    fn asks_pin(&self) -> bool;
    /// Makes a credential on the key the user touches, unless it holds one of `exclude`, then reads its secret
    /// for the PRF `input` (a second touch). Returns the credential's id and the secret.
    fn enroll(
        &self,
        user: &[u8],
        exclude: &[Vec<u8>],
        input: &[u8; 32],
        pin: Option<&str>,
    ) -> Result<(Vec<u8>, Output)>;
    /// The secret for the PRF `input` of whichever of `credentials` the key the user touches holds.
    fn secret(
        &self,
        credentials: &[Vec<u8>],
        input: &[u8; 32],
        pin: Option<&str>,
    ) -> Result<(Vec<u8>, Output)>;
}

/// The keys this machine reaches: webauthn.dll on Windows, USB HID elsewhere.
pub fn system() -> Box<dyn SecurityKeys> {
    #[cfg(windows)]
    return Box::new(webauthn::WebAuthn);
    #[cfg(not(windows))]
    Box::new(Ctap(|| {
        Ok(hid::usb::devices()?
            .into_iter()
            .map(|d| Box::new(d) as Box<dyn Device>)
            .collect())
    }))
}

/// No key at all: what a vault has until the app or the CLI gives it the system's.
pub struct Unavailable;

impl SecurityKeys for Unavailable {
    fn asks_pin(&self) -> bool {
        false
    }

    fn enroll(
        &self,
        _: &[u8],
        _: &[Vec<u8>],
        _: &[u8; 32],
        _: Option<&str>,
    ) -> Result<(Vec<u8>, Output)> {
        bail!("security keys are not available here")
    }

    fn secret(&self, _: &[Vec<u8>], _: &[u8; 32], _: Option<&str>) -> Result<(Vec<u8>, Output)> {
        bail!("security keys are not available here")
    }
}

/// CTAP2 devices: the USB keys plugged in, or simulated ones in tests.
#[cfg(any(not(windows), test))]
pub struct Ctap<F>(pub F);

#[cfg(any(not(windows), test))]
type Devices = Vec<Box<dyn Device>>;

#[cfg(any(not(windows), test))]
fn need_pin(pin: Option<&str>) -> Result<&str> {
    pin.filter(|p| !p.is_empty())
        .context("the security key's PIN is needed")
}

#[cfg(any(not(windows), test))]
impl<F: Fn() -> Result<Devices> + Send + Sync> SecurityKeys for Ctap<F> {
    fn asks_pin(&self) -> bool {
        true
    }

    fn enroll(
        &self,
        user: &[u8],
        exclude: &[Vec<u8>],
        input: &[u8; 32],
        pin: Option<&str>,
    ) -> Result<(Vec<u8>, Output)> {
        let pin = need_pin(pin)?;
        let mut devices = (self.0)()?;
        // The keys already enrolled may stay plugged in: the new one is the key that holds none of them.
        if devices.len() > 1 && !exclude.is_empty() {
            let mut fresh = Vec::new();
            for mut device in devices {
                if !ctap::holds(device.as_mut(), exclude)? {
                    fresh.push(device);
                }
            }
            devices = fresh;
            if devices.is_empty() {
                bail!(ctap::CREDENTIAL_EXCLUDED);
            }
        }
        let device = match devices.as_mut_slice() {
            [] => bail!("plug in the security key to add"),
            [device] => device.as_mut(),
            _ => bail!("plug in only the security key to add"),
        };
        let id = ctap::make_credential(device, user, exclude, pin)?;
        let (_, secret) = ctap::get_secret(
            device,
            std::slice::from_ref(&id),
            &ctap::prf_salt(input),
            pin,
        )?;
        Ok((id, secret))
    }

    fn secret(
        &self,
        credentials: &[Vec<u8>],
        input: &[u8; 32],
        pin: Option<&str>,
    ) -> Result<(Vec<u8>, Output)> {
        let pin = need_pin(pin)?;
        let mut devices = (self.0)()?;
        let mut chosen = None;
        match devices.len() {
            0 => bail!("plug in your security key"),
            1 => chosen = Some(0),
            _ => {
                for (i, device) in devices.iter_mut().enumerate() {
                    if ctap::holds(device.as_mut(), credentials)? {
                        chosen = Some(i);
                        break;
                    }
                }
            }
        }
        let i = chosen.ok_or(NO_CREDENTIALS)?;
        ctap::get_secret(
            devices[i].as_mut(),
            credentials,
            &ctap::prf_salt(input),
            pin,
        )
    }
}
