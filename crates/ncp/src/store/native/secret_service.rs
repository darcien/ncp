//! The Secret Service on Linux.
//!
//! All calls block, and a locked store can wait for an unlock prompt with no
//! limit. An async caller runs them through [`crate::store::run_blocking`].
//! Automated tests do not use this store.

use std::{collections::HashMap, sync::Arc};

use keyring_core::Error;
use secret_service::{EncryptionType, blocking::SecretService};

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
    /// The calls run in this process, so the limit of the caller is enough:
    /// nothing stays after the exit of `ncp`.
    pub fn open(_limit: std::time::Duration) -> Result<Self, StoreError> {
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

    fn delete(&self, key: &CredentialKey) -> Result<bool, StoreError> {
        match self.entry(key)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(Error::NoEntry) => Ok(false),
            Err(error) => Err(store_error(error)),
        }
    }

    /// A search by attribute that does not unlock. keyring-core unlocks
    /// each locked item that a search finds, which shows a prompt. The names
    /// of a locked item are not readable, so a locked item gives an error
    /// and no partial list.
    fn keys(&self) -> Result<Vec<CredentialKey>, StoreError> {
        let unavailable = |error: secret_service::Error| StoreError::Unavailable(error.to_string());
        let service = SecretService::connect(EncryptionType::Plain).map_err(unavailable)?;
        let found = service
            .search_items(HashMap::from([("service", SERVICE)]))
            .map_err(unavailable)?;
        if !found.locked.is_empty() {
            return Err(StoreError::Denied(format!(
                "{} items are locked; unlock the keyring to list them",
                found.locked.len()
            )));
        }
        let accounts: Vec<String> = found
            .unlocked
            .iter()
            .filter_map(|item| item.get_attributes().ok())
            .filter_map(|mut attributes| attributes.remove("username"))
            .collect();
        Ok(crate::store::keys_of(accounts.iter().map(String::as_str)))
    }
}
