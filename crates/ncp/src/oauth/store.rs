//! The tokens item of one server as the credential store of rmcp.
//!
//! rmcp reads the tokens before each request. A read of the native store
//! starts a program (macOS) or a D-Bus call (Linux), so this adapter keeps
//! a copy in memory: the store gets one read for each run, and each write.
//! A refresh writes the new tokens back to the place that they came from
//! (design 2.4).

use std::sync::Arc;

use rmcp::transport::auth::{AuthError, CredentialStore as RmcpStore, StoredCredentials};
use serde_json::Value;
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
    /// The scopes of a `403` since the last login (design B52). rmcp does
    /// not know the field, so a refresh writes it back from here.
    required_scopes: Mutex<Option<Value>>,
}

impl TokenStore {
    #[must_use]
    pub fn new(server: ServerName, store: Arc<dyn CredentialStore>) -> Self {
        Self {
            key: CredentialKey::OAuthTokens(server),
            store,
            copy: Mutex::new(Cached::Unread),
            required_scopes: Mutex::new(None),
        }
    }

    /// The adapter with the item that the caller read already, so that rmcp
    /// does not read the store again.
    #[must_use]
    pub fn with_item(self, item: TokensItem) -> Self {
        Self {
            copy: Mutex::new(Cached::Read(Box::new(Some(item.tokens)))),
            required_scopes: Mutex::new(item.required_scopes),
            ..self
        }
    }

    /// The adapter for a new login: the old tokens and the kept scopes do
    /// not count, and the login replaces them.
    #[must_use]
    pub fn fresh(self) -> Self {
        Self {
            copy: Mutex::new(Cached::Read(Box::new(None))),
            ..self
        }
    }
}

/// The tokens item: the tokens of rmcp and the field of `ncp`.
pub struct TokensItem {
    pub tokens: StoredCredentials,
    pub required_scopes: Option<Value>,
}

/// The tokens item in a stored secret.
///
/// # Errors
///
/// Returns an error when the secret is not the JSON of the tokens.
pub fn parse_item(secret: &Secret) -> Result<TokensItem, items::ItemError> {
    let error = |error: serde_json::Error| items::ItemError {
        item: "tokens",
        reason: error.to_string(),
    };
    let json: Value = serde_json::from_str(secret.expose()).map_err(error)?;
    let required_scopes = json.get(items::REQUIRED_SCOPES).cloned();
    let tokens = serde_json::from_value(json).map_err(error)?;
    Ok(TokensItem {
        tokens,
        required_scopes,
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
        let item = secret
            .map(|secret| parse_item(&secret))
            .transpose()
            .map_err(|error| AuthError::CredentialStoreError(error.to_string()))?;
        let (tokens, required_scopes) = match item {
            Some(item) => (Some(item.tokens), item.required_scopes),
            None => (None, None),
        };
        *self.required_scopes.lock().await = required_scopes;
        *copy = Cached::Read(Box::new(tokens.clone()));
        Ok(tokens)
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let failed = |reason: String| AuthError::CredentialStoreError(reason);
        let mut json =
            serde_json::to_value(&credentials).map_err(|error| failed(error.to_string()))?;
        if let Some(scopes) = self.required_scopes.lock().await.clone() {
            json[items::REQUIRED_SCOPES] = scopes;
        }
        let secret =
            items::to_secret("tokens", &json).map_err(|error| failed(error.to_string()))?;
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
        *self.required_scopes.lock().await = None;
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
    async fn a_refresh_keeps_the_scopes_of_a_403_and_a_login_drops_them() {
        let backing = Arc::new(Counting::default());
        let item = Secret::new(
            r#"{"client_id":"c1","token_response":null,"ncp_required_scopes":["b"]}"#.into(),
        );
        backing
            .inner
            .save(&CredentialKey::OAuthTokens(server()), &item)
            .unwrap();
        let adapter = TokenStore::new(server(), backing.clone());
        adapter.load().await.unwrap();
        adapter.save(tokens("c1")).await.unwrap();
        let key = CredentialKey::OAuthTokens(server());
        let saved = backing.inner.load(&key).unwrap().unwrap();
        assert!(saved.expose().contains("ncp_required_scopes"), "{saved:?}");
        let login = TokenStore::new(server(), backing.clone()).fresh();
        login.save(tokens("c2")).await.unwrap();
        let saved = backing.inner.load(&key).unwrap().unwrap();
        assert!(!saved.expose().contains("ncp_required_scopes"));
    }

    #[tokio::test]
    async fn given_tokens_need_no_read_and_clear_removes_the_item() {
        let backing = Arc::new(Counting::default());
        let adapter = TokenStore::new(server(), backing.clone()).with_item(TokensItem {
            tokens: tokens("c2"),
            required_scopes: None,
        });
        assert_eq!(adapter.load().await.unwrap().unwrap().client_id, "c2");
        assert_eq!(backing.reads.load(Ordering::SeqCst), 0);
        adapter.save(tokens("c3")).await.unwrap();
        adapter.clear().await.unwrap();
        assert!(adapter.load().await.unwrap().is_none());
        assert_eq!(backing.inner.keys().unwrap(), []);
    }
}
