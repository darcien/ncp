//! The native store: the Keychain on macOS, the Secret Service on Linux.
//!
//! Automated tests do not use this store. The calls block, so an async
//! caller runs them on a blocking thread.

use super::{CredentialStore, StoreError};
use crate::credential::{CredentialKey, SERVICE, Secret};

#[derive(Default)]
pub struct NativeStore;

fn entry(key: &CredentialKey) -> Result<keyring::Entry, StoreError> {
    keyring::Entry::new(SERVICE, &key.account()).map_err(store_error)
}

/// Maps a keyring error to a store error. `BadEncoding` holds the stored
/// bytes, so its message does not include them.
fn store_error(error: keyring::Error) -> StoreError {
    match error {
        keyring::Error::PlatformFailure(cause) => StoreError::Unavailable(cause.to_string()),
        keyring::Error::NoStorageAccess(cause) => StoreError::Denied(cause.to_string()),
        keyring::Error::BadEncoding(_) => {
            StoreError::Failure("stored item is not UTF-8 text".into())
        }
        other => StoreError::Failure(other.to_string()),
    }
}

impl CredentialStore for NativeStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
        match entry(key)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(store_error(error)),
        }
    }

    fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
        entry(key)?
            .set_password(secret.expose())
            .map_err(store_error)
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), StoreError> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(store_error(error)),
        }
    }
}
