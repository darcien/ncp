//! The credential store: one internal interface for all stores.
//!
//! A change of the storage model or of the signing method changes only this
//! module. Automated tests use the memory store, the file store, or the
//! `security` store with a fake program. They never use a native store,
//! because the macOS Keychain shows a prompt that an agent cannot accept.

mod file;
mod memory;
mod native;
mod security;

use std::{path::PathBuf, time::Duration};

pub use file::FileStore;
pub use memory::MemoryStore;
pub use native::NativeStore;
pub use security::{PROGRAM_LIMIT, SecurityStore};

use crate::{
    credential::{CredentialKey, Secret},
    text::code,
};

pub trait CredentialStore: Send + Sync {
    /// # Errors
    ///
    /// Returns an error when a read of the store fails.
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError>;

    /// Adds the secret, or replaces the secret that has the same key.
    ///
    /// # Errors
    ///
    /// Returns an error when a write to the store fails.
    fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError>;

    /// Removes the secret. True when the store had it. Removing a secret
    /// that does not exist is not an error.
    ///
    /// # Errors
    ///
    /// Returns an error when a write to the store fails.
    fn delete(&self, key: &CredentialKey) -> Result<bool, StoreError>;

    /// The keys of all `ncp` items, in order. The call reads no value, so a
    /// locked store does not need an unlock for it. An item that `ncp` does
    /// not make is left out.
    ///
    /// # Errors
    ///
    /// Returns an error when a read of the store fails.
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
    #[error("No credential store is available")]
    Unavailable(#[source] Detail),
    #[error("The credential store denied access")]
    Denied(#[source] Detail),
    /// The Secret Service does not show the names of a locked item.
    #[error("The keyring has {}", items(*.0, "locked item"))]
    Locked(usize),
    /// More than one item of the keyring matches one key.
    #[error("The keyring has {} for one secret", items(*.0, "item"))]
    Ambiguous(usize),
    #[error("The credential store failed")]
    Failure(#[source] Detail),
    #[error("ncp cannot use the credential file {}", code(&.path.display().to_string()))]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("Other users can open the credential file {}", code(&.0.display().to_string()))]
    OpenPermissions(PathBuf),
    #[error("The credential file {} is not valid", code(&.0.display().to_string()))]
    Corrupt(PathBuf),
    #[error("The credential store gave no answer in {} s", .0.as_secs())]
    NoAnswer(Duration),
    /// Ctrl-C during the call.
    #[error("ncp stopped the store call because of an interrupt")]
    Interrupted,
}

// "1 item" or "2 items".
fn items(count: usize, word: &str) -> String {
    match count {
        1 => format!("1 {word}"),
        _ => format!("{count} {word}s"),
    }
}

/// The fix of a store error. It is apart from the error text, so that a
/// caller that gets only a text, as through rmcp, can still give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreFix {
    /// A locked store waits for its unlock prompt.
    AnswerPrompt,
    /// Close the credential file to other users.
    CloseFile(PathBuf),
    UnlockKeyring,
    RemoveCopies,
}

/// Where a store error comes from. A failure of the credential file is a
/// local error (exit 3). A failure of the native store is an authorization
/// error (exit 4), because the file can keep the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    Local,
    Native,
}

impl StoreError {
    #[must_use]
    pub fn kind(&self) -> StoreKind {
        match self {
            Self::Io { .. } | Self::OpenPermissions(_) | Self::Corrupt(_) => StoreKind::Local,
            Self::Unavailable(_)
            | Self::Denied(_)
            | Self::Locked(_)
            | Self::Ambiguous(_)
            | Self::Failure(_)
            | Self::NoAnswer(_)
            | Self::Interrupted => StoreKind::Native,
        }
    }

    #[must_use]
    pub fn fix(&self) -> Option<StoreFix> {
        match self {
            Self::NoAnswer(_) => Some(StoreFix::AnswerPrompt),
            Self::OpenPermissions(path) => Some(StoreFix::CloseFile(path.clone())),
            Self::Locked(_) => Some(StoreFix::UnlockKeyring),
            Self::Ambiguous(_) => Some(StoreFix::RemoveCopies),
            Self::Unavailable(_)
            | Self::Denied(_)
            | Self::Failure(_)
            | Self::Io { .. }
            | Self::Corrupt(_)
            | Self::Interrupted => None,
        }
    }
}

/// What failed in a store, in the words of `ncp`. The text of the store
/// program or library is its source, so that it shows as a separate cause.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{text}")]
pub struct Detail {
    text: String,
    #[source]
    foreign: Option<Foreign>,
}

/// Text from a store program or library.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Foreign(String);

impl Detail {
    /// A text of `ncp` with the text of the store program as its source.
    #[must_use]
    pub fn with_foreign(text: impl Into<String>, foreign: impl Into<String>) -> Self {
        let foreign = foreign.into();
        Self {
            text: text.into(),
            foreign: (!foreign.is_empty()).then_some(Foreign(foreign)),
        }
    }
}

impl From<String> for Detail {
    fn from(text: String) -> Self {
        Self {
            text,
            foreign: None,
        }
    }
}

impl From<&str> for Detail {
    fn from(text: &str) -> Self {
        Self::from(text.to_string())
    }
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
        Ok(Err(stopped)) => Err(StoreError::Failure(Detail::with_foreign(
            "The store call stopped",
            stopped.to_string(),
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
    fn error_texts_follow_the_message_rules() {
        let texts = [
            StoreError::Unavailable("x".into()).to_string(),
            StoreError::Denied("x".into()).to_string(),
            StoreError::Locked(2).to_string(),
            StoreError::Ambiguous(2).to_string(),
            StoreError::Failure("x".into()).to_string(),
            StoreError::Io {
                path: "/c".into(),
                source: std::io::Error::other("x"),
            }
            .to_string(),
            StoreError::OpenPermissions("/c".into()).to_string(),
            StoreError::Corrupt("/c".into()).to_string(),
            StoreError::NoAnswer(Duration::from_secs(60)).to_string(),
            StoreError::Interrupted.to_string(),
        ];
        crate::style::assert_style(&texts);
        assert_eq!(texts[8], "The credential store gave no answer in 60 s");
    }

    #[test]
    fn the_text_of_a_store_program_is_a_separate_cause() {
        let exited = "`security x` exited with code 51";
        let error = StoreError::Denied(Detail::with_foreign(exited, "denied"));
        assert_eq!(
            crate::text::chain(&error),
            ["The credential store denied access", exited, "denied"]
        );
        let error = StoreError::Denied(Detail::with_foreign(exited, ""));
        assert_eq!(crate::text::chain(&error).len(), 2);
    }

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
    use crate::{
        credential::{Destination, SecretName},
        server::{Origin, ServerName},
    };

    fn server(name: &str) -> ServerName {
        ServerName::parse(name).unwrap()
    }

    fn secret(text: &str) -> Secret {
        Secret::new(text.to_string())
    }

    pub fn check(store: &dyn CredentialStore) {
        let tokens = CredentialKey::OAuthTokens(server("a"));
        let secret_for = |destination| CredentialKey::Secret {
            destination,
            name: SecretName::parse("API_KEY").unwrap(),
        };
        let origin = |text| Destination::Origin(Origin::parse(text).unwrap());
        let env = secret_for(Destination::Stdio);
        let other_server = CredentialKey::OAuthTokens(server("b"));
        // One name for two origins gives two items: a value never follows
        // its name to another origin.
        let header = secret_for(origin("https://a.test"));
        store.save(&header, &secret("h1")).unwrap();
        assert_eq!(
            store.load(&secret_for(origin("https://b.test"))).unwrap(),
            None
        );
        assert_eq!(store.load(&header).unwrap(), Some(secret("h1")));
        assert!(store.delete(&header).unwrap());

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
