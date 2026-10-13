//! OS secret storage (ADR-07). Secrets live in the platform store, never in the database, backups
//! or logs:
//!
//! - Android: values encrypted with a key held in Android Keystore, stored in `SharedPreferences`
//!   (`android-native-keyring-store`; the JNI context comes from Tauri via `ndk-context`);
//! - iOS: Keychain, "protected data" store in the app's default access group;
//! - macOS (development): legacy login keychain;
//! - Windows (development): Credential Manager.
//!
//! Other targets (Linux CI) have no native store; tests use `keyring_core::mock`.
//! Errors never carry secret bytes: `keyring_core::Error::BadEncoding` holds the raw value, so it is
//! mapped to a payload-free variant.

use std::sync::Arc;

use keyring_core::{CredentialStore, Entry, Error as KeyringError};

#[derive(Debug, thiserror::Error)]
pub enum SecretStoreError {
    #[error("no native secret store on this platform")]
    Unsupported,
    #[error("secret store is not accessible")]
    NoAccess,
    #[error("stored secret is not valid UTF-8")]
    BadEncoding,
    #[error("secret store failure: {0}")]
    Platform(String),
}

impl From<KeyringError> for SecretStoreError {
    fn from(error: KeyringError) -> Self {
        match error {
            KeyringError::NoStorageAccess(_) => Self::NoAccess,
            KeyringError::BadEncoding(_) | KeyringError::BadDataFormat(_, _) => Self::BadEncoding,
            // Platform/attribute errors describe the store, not the stored value.
            other => Self::Platform(other.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, SecretStoreError>;

/// Named secrets of one app variant. `service` separates variants that share a platform store.
pub struct SecretStore {
    store: Arc<CredentialStore>,
    service: String,
}

impl SecretStore {
    /// Opens the platform store; fails with [`SecretStoreError::Unsupported`] where none exists.
    pub fn native(service: &str) -> Result<Self> {
        Ok(Self::with_store(native_store()?, service))
    }

    pub fn with_store(store: Arc<CredentialStore>, service: &str) -> Self {
        Self {
            store,
            service: service.to_owned(),
        }
    }

    /// Store vendor, for diagnostics (for example "Android Keystore" or "Windows Credential Manager").
    pub fn backend(&self) -> String {
        self.store.vendor()
    }

    fn entry(&self, name: &str) -> Result<Entry> {
        Ok(self.store.build(&self.service, name, None)?)
    }

    pub fn set(&self, name: &str, secret: &str) -> Result<()> {
        Ok(self.entry(name)?.set_password(secret)?)
    }

    /// Reads a secret inside Rust. There is deliberately no IPC command that returns it to JS.
    pub fn get(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn has(&self, name: &str) -> Result<bool> {
        Ok(self.get(name)?.is_some())
    }

    /// Returns `false` when there was nothing to delete.
    pub fn delete(&self, name: &str) -> Result<bool> {
        match self.entry(name)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(KeyringError::NoEntry) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(target_os = "android")]
fn native_store() -> Result<Arc<CredentialStore>> {
    Ok(android_native_keyring_store::Store::new()?)
}

#[cfg(target_os = "ios")]
fn native_store() -> Result<Arc<CredentialStore>> {
    Ok(apple_native_keyring_store::protected::Store::new()?)
}

#[cfg(target_os = "macos")]
fn native_store() -> Result<Arc<CredentialStore>> {
    Ok(apple_native_keyring_store::keychain::Store::new()?)
}

#[cfg(windows)]
fn native_store() -> Result<Arc<CredentialStore>> {
    Ok(windows_native_keyring_store::Store::new()?)
}

#[cfg(not(any(target_os = "android", target_os = "ios", target_os = "macos", windows)))]
fn native_store() -> Result<Arc<CredentialStore>> {
    Err(SecretStoreError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(service: &str) -> SecretStore {
        SecretStore::with_store(keyring_core::mock::Store::new().unwrap(), service)
    }

    #[test]
    fn set_get_overwrite_delete() {
        let secrets = store("test");
        assert_eq!(secrets.get("api-key").unwrap(), None);
        secrets.set("api-key", "first").unwrap();
        assert_eq!(secrets.get("api-key").unwrap().as_deref(), Some("first"));
        secrets.set("api-key", "zażółć 🔑").unwrap();
        assert_eq!(
            secrets.get("api-key").unwrap().as_deref(),
            Some("zażółć 🔑")
        );
        assert!(secrets.delete("api-key").unwrap());
        assert!(!secrets.has("api-key").unwrap());
        assert!(!secrets.delete("api-key").unwrap());
    }

    #[test]
    fn names_are_independent() {
        let secrets = store("test");
        secrets.set("a", "1").unwrap();
        assert!(!secrets.has("b").unwrap());
    }

    #[test]
    fn bad_encoding_error_does_not_carry_the_value() {
        let error = SecretStoreError::from(KeyringError::BadEncoding(b"top-secret".to_vec()));
        let shown = format!("{error} {error:?}");
        assert!(!shown.contains("top-secret"), "{shown}");
    }
}
