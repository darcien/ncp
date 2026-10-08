//! The credential store: one internal interface for all stores.
//!
//! A change of the storage model or of the signing method changes only this
//! module. Automated tests use the memory store, the file store, or the
//! `security` store with a fake program, never a native store: the macOS
//! Keychain shows a prompt that an agent cannot accept.

mod file;
mod memory;
mod native;
mod security;

use std::{path::PathBuf, time::Duration};

pub use file::FileStore;
pub use memory::MemoryStore;
pub use native::NativeStore;
pub use security::{PROGRAM_LIMIT, SecurityStore};

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

    /// Removes the secret. True when the store had it. Removing a secret
    /// that does not exist is not an error.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot be written.
    fn delete(&self, key: &CredentialKey) -> Result<bool, StoreError>;

    /// The keys of all `ncp` items, in order. The call reads no value, so a
    /// locked store does not need an unlock for it. An item that `ncp` does
    /// not make is left out.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot be read.
    fn keys(&self) -> Result<Vec<CredentialKey>, StoreError>;
}

/// The keys of the accounts that `ncp` makes, in order.
#[must_use]
pub fn keys_of<'a>(accounts: impl IntoIterator<Item = &'a str>) -> Vec<CredentialKey> {
    let mut keys: Vec<CredentialKey> = accounts
        .into_iter()
        .filter_map(CredentialKey::from_account)
        .collect();
    keys.sort();
    keys.dedup();
    keys
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
    #[error(
        "the credential store gave no answer in {} s; it can wait for an unlock prompt",
        .0.as_secs()
    )]
    NoAnswer(Duration),
    /// Ctrl-C during the call.
    #[error("the store call was interrupted")]
    Interrupted,
}

/// The time limit for one store call. A locked store waits for its unlock
/// prompt with no limit, and on a machine with no display nobody can answer
/// the prompt. The limit gives a user time to read a prompt and type a
/// password.
pub const CALL_LIMIT: Duration = Duration::from_secs(60);

/// Runs a store call on a blocking thread, with a time limit.
///
/// A store call blocks its thread. On a runtime thread, a call that waits
/// for a prompt stops all the tasks of that thread. After the limit, the
/// call continues on its thread until the store answers. The runtime waits
/// for blocking threads when it shuts down, so the program must shut down
/// its runtime with a time limit.
///
/// # Errors
///
/// Returns the error of the call, or an error when the call gives no answer
/// in the limit or panics.
pub async fn run_blocking<T: Send + 'static>(
    limit: Duration,
    call: impl FnOnce() -> Result<T, StoreError> + Send + 'static,
) -> Result<T, StoreError> {
    match tokio::time::timeout(limit, tokio::task::spawn_blocking(call)).await {
        Ok(Ok(result)) => result,
        Ok(Err(stopped)) => Err(StoreError::Failure(format!(
            "the store call stopped: {stopped}"
        ))),
        Err(_) => Err(StoreError::NoAnswer(limit)),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::server::ServerName;

    fn tokens_key() -> CredentialKey {
        CredentialKey::OAuthTokens(ServerName::parse("a").unwrap())
    }

    #[tokio::test]
    async fn returns_the_result_of_a_blocking_store_call() {
        let store = Arc::new(MemoryStore::default());
        store
            .save(&tokens_key(), &Secret::new("t1".into()))
            .unwrap();
        let loaded = run_blocking(CALL_LIMIT, move || store.load(&tokens_key())).await;
        assert_eq!(loaded.unwrap(), Some(Secret::new("t1".into())));
    }

    #[tokio::test]
    async fn stops_waiting_when_the_store_gives_no_answer() {
        let limit = Duration::from_millis(20);
        let result = run_blocking(limit, || {
            std::thread::sleep(Duration::from_millis(200));
            Ok(())
        })
        .await;
        assert!(matches!(result, Err(StoreError::NoAnswer(d)) if d == limit));
    }

    #[tokio::test]
    async fn reports_a_store_call_that_panics_as_a_failure() {
        let result: Result<(), _> = run_blocking(CALL_LIMIT, || panic!("store bug")).await;
        assert!(matches!(result, Err(StoreError::Failure(_))));
    }
}

#[cfg(test)]
mod keys_tests {
    use super::*;

    #[test]
    fn keeps_only_the_accounts_that_ncp_makes_in_order() {
        let keys = keys_of(["secret/B", "old/env/X", "secret/A", "secret/A"]);
        let accounts: Vec<String> = keys.iter().map(CredentialKey::account).collect();
        assert_eq!(accounts, ["secret/A", "secret/B"]);
    }
}

#[cfg(test)]
mod contract {
    //! The behavior that each store must have.

    use super::*;
    use crate::{credential::SecretName, server::ServerName};

    fn server(name: &str) -> ServerName {
        ServerName::parse(name).unwrap()
    }

    fn secret(text: &str) -> Secret {
        Secret::new(text.to_string())
    }

    pub fn check(store: &dyn CredentialStore) {
        let tokens = CredentialKey::OAuthTokens(server("a"));
        let env = CredentialKey::Secret(SecretName::parse("API_KEY").unwrap());
        let other_server = CredentialKey::OAuthTokens(server("b"));

        assert_eq!(store.load(&tokens).unwrap(), None);
        store.save(&tokens, &secret("t1")).unwrap();
        store.save(&env, &secret("e1")).unwrap();
        assert_eq!(store.load(&tokens).unwrap(), Some(secret("t1")));
        assert_eq!(store.load(&env).unwrap(), Some(secret("e1")));
        assert_eq!(store.load(&other_server).unwrap(), None);

        store.save(&tokens, &secret("t2")).unwrap();
        assert_eq!(store.load(&tokens).unwrap(), Some(secret("t2")));

        assert_eq!(store.keys().unwrap(), [env.clone(), tokens.clone()]);

        assert!(store.delete(&tokens).unwrap());
        assert_eq!(store.load(&tokens).unwrap(), None);
        assert_eq!(store.load(&env).unwrap(), Some(secret("e1")));
        assert!(!store.delete(&tokens).unwrap());
        assert_eq!(store.keys().unwrap(), [env]);
    }
}
