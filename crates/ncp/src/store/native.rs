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
        /// Opens the login Keychain. This starts no process. Each run of
        /// the program has the limit.
        ///
        /// # Errors
        ///
        /// Returns no error. The result has the same type as on Linux.
        pub fn open(limit: std::time::Duration) -> Result<Self, StoreError> {
            Ok(Self(SecurityStore::new().with_limit(limit)))
        }
    }

    impl CredentialStore for NativeStore {
        fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
            self.0.load(key)
        }

        fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
            self.0.save(key, secret)
        }

        fn delete(&self, key: &CredentialKey) -> Result<bool, StoreError> {
            self.0.delete(key)
        }

        fn keys(&self) -> Result<Vec<CredentialKey>, StoreError> {
            self.0.keys()
        }
    }
}
