//! The credential store: one internal interface for all stores.
//!
//! A change of the storage model or of the signing method changes only this
//! module. Automated tests use the memory store or the file store, never a
//! native store: the macOS Keychain shows a prompt that an agent cannot
//! accept.

mod file;
mod memory;
mod native;

use std::path::PathBuf;

pub use file::FileStore;
pub use memory::MemoryStore;
pub use native::NativeStore;

use crate::credential::{CredentialKey, Secret};

pub trait CredentialStore: Send + Sync {
    /// # Errors
    ///
    /// Returns an error when the store cannot be read.
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError>;

    /// Adds the secret, or replaces the secret that has the same key.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot be written.
    fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError>;

    /// Removes the secret. Removing a secret that does not exist is not an
    /// error.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot be written.
    fn delete(&self, key: &CredentialKey) -> Result<(), StoreError>;
}

/// Store errors. No variant holds a secret value.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("no credential store is available: {0}")]
    Unavailable(String),
    #[error("access to the credential store is denied: {0}")]
    Denied(String),
    #[error("credential store failure: {0}")]
    Failure(String),
    #[error("credential file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("credential file {0} is open to other users; set its mode to 0600")]
    OpenPermissions(PathBuf),
    #[error("credential file {0} is not valid")]
    Corrupt(PathBuf),
}

#[cfg(test)]
mod contract {
    //! The behavior that each store must have.

    use super::*;
    use crate::{
        credential::{EnvName, SecretKind},
        server::ServerName,
    };

    fn key(server: &str, kind: SecretKind) -> CredentialKey {
        CredentialKey::new(ServerName::parse(server).unwrap(), kind)
    }

    fn secret(text: &str) -> Secret {
        Secret::new(text.to_string())
    }

    pub fn check(store: &dyn CredentialStore) {
        let tokens = key("a", SecretKind::OAuthTokens);
        let env = key("a", SecretKind::Env(EnvName::parse("API_KEY").unwrap()));
        let other_server = key("b", SecretKind::OAuthTokens);

        assert_eq!(store.load(&tokens).unwrap(), None);
        store.save(&tokens, &secret("t1")).unwrap();
        store.save(&env, &secret("e1")).unwrap();
        assert_eq!(store.load(&tokens).unwrap(), Some(secret("t1")));
        assert_eq!(store.load(&env).unwrap(), Some(secret("e1")));
        assert_eq!(store.load(&other_server).unwrap(), None);

        store.save(&tokens, &secret("t2")).unwrap();
        assert_eq!(store.load(&tokens).unwrap(), Some(secret("t2")));

        store.delete(&tokens).unwrap();
        assert_eq!(store.load(&tokens).unwrap(), None);
        assert_eq!(store.load(&env).unwrap(), Some(secret("e1")));
        store.delete(&tokens).unwrap();
    }
}
