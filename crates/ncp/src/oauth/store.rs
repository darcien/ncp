//! The tokens item of one server as the credential store of rmcp.
//!
//! rmcp reads the tokens before each request. A read of the native store
//! starts a program (macOS) or a D-Bus call (Linux), so this adapter keeps
//! a copy in memory: the store gets one read for each run, and each write.
//! A refresh writes the new tokens back to the place that they came from
//! (design 2.4).

use std::sync::Arc;

use rmcp::transport::auth::{AuthError, CredentialStore as RmcpStore, StoredCredentials};
use tokio::sync::Mutex;

use super::items;
use crate::{
    credential::{CredentialKey, Secret},
    server::ServerName,
    store::{self, CredentialStore, StoreError},
};

/// The copy in memory of the tokens item.
enum Cached {
    Unread,
    /// The tokens, or `None` when the place has none. Boxed, because the
    /// tokens are large.
    Read(Box<Option<StoredCredentials>>),
}

pub struct TokenStore {
    key: CredentialKey,
    store: Arc<dyn CredentialStore>,
    copy: Mutex<Cached>,
}

impl TokenStore {
    #[must_use]
    pub fn new(server: ServerName, store: Arc<dyn CredentialStore>) -> Self {
        Self {
            key: CredentialKey::OAuthTokens(server),
            store,
            copy: Mutex::new(Cached::Unread),
        }
    }

    /// The adapter with the tokens that the caller read already, so that
    /// rmcp does not read the store again.
    #[must_use]
    pub fn with_tokens(self, tokens: StoredCredentials) -> Self {
        Self {
            copy: Mutex::new(Cached::Read(Box::new(Some(tokens)))),
            ..self
        }
    }
}

/// The tokens in a stored secret.
///
/// # Errors
///
/// Returns an error when the secret is not the JSON of the tokens.
pub fn parse_tokens(secret: &Secret) -> Result<StoredCredentials, items::ItemError> {
    serde_json::from_str(secret.expose()).map_err(|error| items::ItemError {
        item: "tokens",
        reason: error.to_string(),
    })
}

fn store_error(error: &StoreError) -> AuthError {
    AuthError::CredentialStoreError(error.to_string())
}

#[async_trait::async_trait]
impl RmcpStore for TokenStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let mut copy = self.copy.lock().await;
        if let Cached::Read(tokens) = &*copy {
            return Ok(tokens.as_ref().clone());
        }
        let (store, key) = (self.store.clone(), self.key.clone());
        let secret = store::run_blocking(store::CALL_LIMIT, move || store.load(&key))
            .await
            .map_err(|error| store_error(&error))?;
        let tokens = secret
            .map(|secret| parse_tokens(&secret))
            .transpose()
            .map_err(|error| AuthError::CredentialStoreError(error.to_string()))?;
        *copy = Cached::Read(Box::new(tokens.clone()));
        Ok(tokens)
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let secret = items::to_secret("tokens", &credentials)
            .map_err(|error| AuthError::CredentialStoreError(error.to_string()))?;
        let (store, key) = (self.store.clone(), self.key.clone());
        store::run_blocking(store::CALL_LIMIT, move || store.save(&key, &secret))
            .await
            .map_err(|error| store_error(&error))?;
        *self.copy.lock().await = Cached::Read(Box::new(Some(credentials)));
        Ok(())
    }

    async fn clear(&self) -> Result<(), AuthError> {
        let (store, key) = (self.store.clone(), self.key.clone());
        store::run_blocking(store::CALL_LIMIT, move || store.delete(&key))
            .await
            .map_err(|error| store_error(&error))?;
        *self.copy.lock().await = Cached::Read(Box::new(None));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::store::MemoryStore;

    /// A memory store that counts its reads.
    #[derive(Default)]
    struct Counting {
        inner: MemoryStore,
        reads: AtomicUsize,
    }

    impl CredentialStore for Counting {
        fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.inner.load(key)
        }
        fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
            self.inner.save(key, secret)
        }
        fn delete(&self, key: &CredentialKey) -> Result<bool, StoreError> {
            self.inner.delete(key)
        }
        fn keys(&self) -> Result<Vec<CredentialKey>, StoreError> {
            self.inner.keys()
        }
    }

    fn server() -> ServerName {
        ServerName::parse("web").unwrap()
    }

    fn tokens(client: &str) -> StoredCredentials {
        StoredCredentials::new(client.into(), None, vec!["a".into()], Some(1))
    }

    #[tokio::test]
    async fn saved_tokens_load_back_from_the_store_and_the_store_is_read_once() {
        let backing = Arc::new(Counting::default());
        let adapter = TokenStore::new(server(), backing.clone());
        assert!(adapter.load().await.unwrap().is_none());
        adapter.save(tokens("c1")).await.unwrap();
        assert_eq!(adapter.load().await.unwrap().unwrap().client_id, "c1");
        assert_eq!(backing.reads.load(Ordering::SeqCst), 1);
        let fresh = TokenStore::new(server(), backing.clone());
        let loaded = fresh.load().await.unwrap().unwrap();
        assert_eq!(loaded.granted_scopes, ["a"]);
        let account = backing.inner.keys().unwrap()[0].account();
        assert_eq!(account, "oauth/web/tokens");
    }

    #[tokio::test]
    async fn given_tokens_need_no_read_and_clear_removes_the_item() {
        let backing = Arc::new(Counting::default());
        let adapter = TokenStore::new(server(), backing.clone()).with_tokens(tokens("c2"));
        assert_eq!(adapter.load().await.unwrap().unwrap().client_id, "c2");
        assert_eq!(backing.reads.load(Ordering::SeqCst), 0);
        adapter.save(tokens("c3")).await.unwrap();
        adapter.clear().await.unwrap();
        assert!(adapter.load().await.unwrap().is_none());
        assert_eq!(backing.inner.keys().unwrap(), []);
    }
}
