//! Windows lets only its WebAuthn API (webauthn.dll) reach a security key without administrator rights: it asks
//! for the PIN and the touch in its own window, and turns the PRF input into the `hmac-secret` salt as
//! `ctap::prf_salt` does on macOS and Linux.

use std::ptr;

use anyhow::{Context, Result, bail, ensure};
use termf_sync::crypto;
use windows::Win32::Foundation::HWND;
use windows::Win32::Networking::WindowsWebServices::*;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{BOOL, HRESULT, HSTRING, PCWSTR};
use zeroize::Zeroizing;

use super::{Output, RP_ID, SecurityKeys};

const TIMEOUT_MS: u32 = 60_000;

pub struct WebAuthn;

fn wide(text: &str) -> HSTRING {
    HSTRING::from(text)
}

fn failed(hr: HRESULT) -> anyhow::Error {
    // NTE_USER_CANCELLED, and NotAllowedError's HRESULT for a dialog closed or timed out.
    if hr.0 as u32 == 0x8009_0036 || hr.0 as u32 == 0x800704c7 {
        return anyhow::anyhow!("cancelled in the Windows security window");
    }
    // SAFETY: returns a static string for any HRESULT.
    let name = unsafe { WebAuthNGetErrorName(hr) };
    let name = unsafe { name.to_string() }.unwrap_or_default();
    anyhow::anyhow!("Windows could not use the security key ({name})")
}

fn client_data(kind: &str) -> Vec<u8> {
    let challenge = crypto::hex(&crypto::random::<32>());
    format!(r#"{{"type":"{kind}","challenge":"{challenge}","origin":"https://{RP_ID}","crossOrigin":false}}"#).into_bytes()
}

fn window() -> HWND {
    // SAFETY: no arguments; the window in front is TermF or the terminal the CLI runs in.
    unsafe { GetForegroundWindow() }
}

impl WebAuthn {
    fn assertion(&self, credentials: &[Vec<u8>], input: &[u8; 32]) -> Result<(Vec<u8>, Output)> {
        // SAFETY: no arguments.
        let version = unsafe { WebAuthNGetApiVersionNumber() };
        ensure!(
            version >= WEBAUTHN_API_VERSION_4,
            "this Windows cannot read a security key's secret: Windows 11 (22H2) or later is needed"
        );
        let rp = wide(RP_ID);
        let mut json = client_data("webauthn.get");
        let mut list: Vec<WEBAUTHN_CREDENTIAL> = credentials
            .iter()
            .map(|id| WEBAUTHN_CREDENTIAL {
                dwVersion: WEBAUTHN_CREDENTIAL_CURRENT_VERSION,
                cbId: id.len() as u32,
                pbId: id.as_ptr() as *mut u8,
                pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
            })
            .collect();
        let mut first = *input;
        let mut salt = WEBAUTHN_HMAC_SECRET_SALT {
            cbFirst: 32,
            pbFirst: first.as_mut_ptr(),
            cbSecond: 0,
            pbSecond: ptr::null_mut(),
        };
        let mut salts = WEBAUTHN_HMAC_SECRET_SALT_VALUES {
            pGlobalHmacSalt: &mut salt,
            cCredWithHmacSecretSaltList: 0,
            pCredWithHmacSecretSaltList: ptr::null_mut(),
        };
        let data = WEBAUTHN_CLIENT_DATA {
            dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION,
            cbClientDataJSON: json.len() as u32,
            pbClientDataJSON: json.as_mut_ptr(),
            pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256,
        };
        let options = WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS {
            dwVersion: WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS_VERSION_6,
            dwTimeoutMilliseconds: TIMEOUT_MS,
            CredentialList: WEBAUTHN_CREDENTIALS {
                cCredentials: list.len() as u32,
                pCredentials: list.as_mut_ptr(),
            },
            dwAuthenticatorAttachment: WEBAUTHN_AUTHENTICATOR_ATTACHMENT_CROSS_PLATFORM,
            dwUserVerificationRequirement: WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED,
            pHmacSecretSaltValues: &mut salts,
            ..Default::default()
        };
        // SAFETY: every pointer above lives until the call returns; the result is freed below.
        let assertion = unsafe {
            WebAuthNAuthenticatorGetAssertion(
                window(),
                PCWSTR(rp.as_ptr()),
                &data,
                Some(ptr::from_ref(&options)),
            )
        }
        .map_err(|e| failed(e.code()))?;
        // SAFETY: a valid assertion from webauthn.dll until `WebAuthNFreeAssertion`.
        let result = unsafe {
            let a = &*assertion;
            let id =
                std::slice::from_raw_parts(a.Credential.pbId, a.Credential.cbId as usize).to_vec();
            let secret = (a.dwVersion >= WEBAUTHN_ASSERTION_VERSION_3 && !a.pHmacSecret.is_null())
                .then(|| &*a.pHmacSecret)
                .filter(|s| s.cbFirst == 32 && !s.pbFirst.is_null())
                .map(|s| {
                    let mut out = Zeroizing::new([0u8; 32]);
                    out.copy_from_slice(std::slice::from_raw_parts(s.pbFirst, 32));
                    out
                });
            WebAuthNFreeAssertion(assertion);
            (id, secret)
        };
        let (id, secret) = result;
        let secret = secret.context("Windows returned no secret from the security key")?;
        ensure!(
            credentials.contains(&id),
            "the security key used a credential that is not the vault's"
        );
        Ok((id, secret))
    }
}

impl SecurityKeys for WebAuthn {
    fn asks_pin(&self) -> bool {
        false
    }

    fn enroll(
        &self,
        user: &[u8],
        exclude: &[Vec<u8>],
        input: &[u8; 32],
        _pin: Option<&str>,
    ) -> Result<(Vec<u8>, Output)> {
        let rp_id = wide(RP_ID);
        let rp_name = wide("TermF");
        let name = wide("vault");
        let display = wide("TermF vault");
        let rp = WEBAUTHN_RP_ENTITY_INFORMATION {
            dwVersion: WEBAUTHN_RP_ENTITY_INFORMATION_CURRENT_VERSION,
            pwszId: PCWSTR(rp_id.as_ptr()),
            pwszName: PCWSTR(rp_name.as_ptr()),
            pwszIcon: PCWSTR::null(),
        };
        let mut user_id = user.to_vec();
        let person = WEBAUTHN_USER_ENTITY_INFORMATION {
            dwVersion: WEBAUTHN_USER_ENTITY_INFORMATION_CURRENT_VERSION,
            cbId: user_id.len() as u32,
            pbId: user_id.as_mut_ptr(),
            pwszName: PCWSTR(name.as_ptr()),
            pwszIcon: PCWSTR::null(),
            pwszDisplayName: PCWSTR(display.as_ptr()),
        };
        let mut es256 = WEBAUTHN_COSE_CREDENTIAL_PARAMETER {
            dwVersion: WEBAUTHN_COSE_CREDENTIAL_PARAMETER_CURRENT_VERSION,
            pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
            lAlg: WEBAUTHN_COSE_ALGORITHM_ECDSA_P256_WITH_SHA256,
        };
        let params = WEBAUTHN_COSE_CREDENTIAL_PARAMETERS {
            cCredentialParameters: 1,
            pCredentialParameters: &mut es256,
        };
        let mut json = client_data("webauthn.create");
        let data = WEBAUTHN_CLIENT_DATA {
            dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION,
            cbClientDataJSON: json.len() as u32,
            pbClientDataJSON: json.as_mut_ptr(),
            pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256,
        };
        let mut on = BOOL::from(true);
        let mut extension = WEBAUTHN_EXTENSION {
            pwszExtensionIdentifier: WEBAUTHN_EXTENSIONS_IDENTIFIER_HMAC_SECRET,
            cbExtension: std::mem::size_of::<BOOL>() as u32,
            pvExtension: (&mut on as *mut BOOL).cast(),
        };
        let mut excluded: Vec<WEBAUTHN_CREDENTIAL> = exclude
            .iter()
            .map(|id| WEBAUTHN_CREDENTIAL {
                dwVersion: WEBAUTHN_CREDENTIAL_CURRENT_VERSION,
                cbId: id.len() as u32,
                pbId: id.as_ptr() as *mut u8,
                pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
            })
            .collect();
        let options = WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS {
            dwVersion: WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS_VERSION_3,
            dwTimeoutMilliseconds: TIMEOUT_MS,
            CredentialList: WEBAUTHN_CREDENTIALS {
                cCredentials: excluded.len() as u32,
                pCredentials: excluded.as_mut_ptr(),
            },
            Extensions: WEBAUTHN_EXTENSIONS {
                cExtensions: 1,
                pExtensions: &mut extension,
            },
            dwAuthenticatorAttachment: WEBAUTHN_AUTHENTICATOR_ATTACHMENT_CROSS_PLATFORM,
            bRequireResidentKey: BOOL::from(false),
            dwUserVerificationRequirement: WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED,
            dwAttestationConveyancePreference: WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_NONE,
            ..Default::default()
        };
        // SAFETY: every pointer above lives until the call returns; the result is freed below.
        let attestation = unsafe {
            WebAuthNAuthenticatorMakeCredential(
                window(),
                &rp,
                &person,
                &params,
                &data,
                Some(ptr::from_ref(&options)),
            )
        }
        .map_err(|e| failed(e.code()))?;
        // SAFETY: a valid attestation from webauthn.dll until `WebAuthNFreeCredentialAttestation`.
        let id = unsafe {
            let a = &*attestation;
            let id =
                std::slice::from_raw_parts(a.pbCredentialId, a.cbCredentialId as usize).to_vec();
            WebAuthNFreeCredentialAttestation(Some(attestation.cast_const()));
            id
        };
        if id.is_empty() {
            bail!("Windows returned no credential from the security key");
        }
        self.assertion(&[id], input)
    }

    fn secret(
        &self,
        credentials: &[Vec<u8>],
        input: &[u8; 32],
        _pin: Option<&str>,
    ) -> Result<(Vec<u8>, Output)> {
        self.assertion(credentials, input)
    }
}
