//! Where each machine keeps its key-encryption key, so the password is typed once per machine.

use anyhow::Result;

pub trait KeyStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<Vec<u8>>>;
    fn set(&self, account: &str, secret: &[u8]) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;
}

const SERVICE: &str = "dev.termf.sync";

/// macOS Keychain, Windows Credential Manager or the Secret Service (GNOME Keyring, KWallet) on Linux.
pub struct OsKeyStore;

impl KeyStore for OsKeyStore {
    fn get(&self, account: &str) -> Result<Option<Vec<u8>>> {
        match keyring::Entry::new(SERVICE, account)?.get_secret() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn set(&self, account: &str, secret: &[u8]) -> Result<()> {
        Ok(keyring::Entry::new(SERVICE, account)?.set_secret(secret)?)
    }

    fn delete(&self, account: &str) -> Result<()> {
        match keyring::Entry::new(SERVICE, account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
