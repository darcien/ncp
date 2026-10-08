//! The Secret Service on Linux.
//!
//! All calls block, and a locked store can wait for an unlock prompt with no
//! limit. An async caller runs them through [`crate::store::run_blocking`].
//! Automated tests do not use this store.

use std::sync::Arc;

use keyring_core::Error;

use crate::{
    credential::{CredentialKey, SERVICE, Secret},
    store::{CredentialStore, StoreError},
};

pub struct NativeStore {
    store: Arc<keyring_core::CredentialStore>,
}

impl NativeStore {
    /// Opens the Secret Service. This connects to the session bus, so it
    /// blocks.
    ///
    /// # Errors
    ///
    /// Returns an error when there is no usable Secret Service, for example
    /// with no session bus.
    pub fn open() -> Result<Self, StoreError> {
        platform_store()
            .map(|store| Self { store })
            .map_err(store_error)
    }

    fn entry(&self, key: &CredentialKey) -> Result<keyring_core::Entry, StoreError> {
        self.store
            .build(SERVICE, &key.account(), None)
            .map_err(store_error)
    }
}

fn platform_store() -> keyring_core::Result<Arc<keyring_core::CredentialStore>> {
    zbus_secret_service_keyring_store::Store::new()
        .map(|store| store as Arc<keyring_core::CredentialStore>)
}

/// Maps a keyring error to a store error. Some variants hold the stored
/// bytes, so their messages do not include them.
fn store_error(error: Error) -> StoreError {
    match error {
        Error::PlatformFailure(cause) => StoreError::Unavailable(cause.to_string()),
        Error::NoStorageAccess(cause) => StoreError::Denied(cause.to_string()),
        Error::BadEncoding(_) | Error::BadDataFormat(..) => {
            StoreError::Failure("stored item is not UTF-8 text".into())
        }
        Error::Ambiguous(items) => StoreError::Failure(format!(
            "{} stored items match one secret; remove the copies",
            items.len()
        )),
        other => StoreError::Failure(other.to_string()),
    }
}

impl CredentialStore for NativeStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(store_error(error)),
        }
    }

    fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
        self.entry(key)?
            .set_password(secret.expose())
            .map_err(store_error)
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), StoreError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(store_error(error)),
        }
    }
}
