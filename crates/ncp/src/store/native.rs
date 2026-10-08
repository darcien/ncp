//! The native store: the login Keychain on macOS, the Secret Service on
//! Linux.
//!
//! All calls block, and a locked store can wait for an unlock prompt with no
//! limit. An async caller runs them through [`super::run_blocking`].
//! Automated tests do not use this store.

#[cfg(target_os = "linux")]
mod secret_service;

#[cfg(target_os = "macos")]
pub use keychain::NativeStore;
#[cfg(target_os = "linux")]
pub use secret_service::NativeStore;

#[cfg(target_os = "macos")]
mod keychain {
    use crate::{
        credential::{CredentialKey, Secret},
        store::{CredentialStore, SecurityStore, StoreError},
    };

    /// The login Keychain through `/usr/bin/security`. [`SecurityStore`]
    /// gives the reason.
    pub struct NativeStore(SecurityStore);

    impl NativeStore {
        /// Opens the login Keychain. This starts no process.
        ///
        /// # Errors
        ///
        /// Returns no error. The result has the same type as on Linux.
        pub fn open() -> Result<Self, StoreError> {
            Ok(Self(SecurityStore::new()))
        }
    }

    impl CredentialStore for NativeStore {
        fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
            self.0.load(key)
        }

        fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
            self.0.save(key, secret)
        }

        fn delete(&self, key: &CredentialKey) -> Result<(), StoreError> {
            self.0.delete(key)
        }
    }
}
