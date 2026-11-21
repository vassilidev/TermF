//! Where tracker tokens are kept: the system's keychain, never the config, the database or a log.

use anyhow::Result;

pub trait Secrets: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;
}

const SERVICE: &str = "dev.termf.tickets";

/// macOS Keychain, Windows Credential Manager or the Secret Service (GNOME Keyring, KWallet) on Linux.
pub struct Keychain;

impl Secrets for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>> {
        match keyring::Entry::new(SERVICE, account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<()> {
        Ok(keyring::Entry::new(SERVICE, account)?.set_password(secret)?)
    }

    fn delete(&self, account: &str) -> Result<()> {
        match keyring::Entry::new(SERVICE, account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
