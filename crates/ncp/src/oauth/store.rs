//! The tokens item of one server as the credential store of rmcp.
//!
//! rmcp reads the tokens before each request. A read of the native store
//! starts a program (macOS) or a D-Bus call (Linux), so this adapter keeps
//! a copy in memory: the store gets one read for each run, and each write.
//! A refresh writes the new tokens back to the place that they came from
//! (design 2.4).

use std::{path::PathBuf, sync::Arc, time::Duration};

use rmcp::transport::auth::{
    AuthError, CredentialRefreshGuard, CredentialStore as RmcpStore, StoredCredentials,
};
use serde_json::{Map, Value};
use tokio::sync::Mutex;

use super::items;
use crate::{
    credential::{CredentialKey, Secret},
    private_file,
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
    /// The scope fields of `ncp` in the item (design B52, B79). rmcp does
    /// not know them, so a refresh writes them back from here.
    scope_fields: Mutex<Map<String, Value>>,
    /// The server that the tokens are for. A save writes it with the tokens.
    binding: Option<items::TokenBinding>,
    /// The lock file that one refresh at a time holds, across processes.
    refresh_lock: Option<PathBuf>,
    /// Gets the time of each read and write of the store, for `--timings`.
    timer: Option<StoreTimer>,
}

/// Gets the time of one call to the credential store.
pub type StoreTimer = Arc<dyn Fn(Duration) + Send + Sync>;

/// The longest wait for a refresh of another process. A refresh is one
/// token request with a limit of 30 s.
const REFRESH_LOCK_LIMIT: Duration = Duration::from_secs(45);

impl TokenStore {
    #[must_use]
    pub fn new(server: ServerName, store: Arc<dyn CredentialStore>) -> Self {
        Self {
            key: CredentialKey::OAuthTokens(server),
            store,
            copy: Mutex::new(Cached::Unread),
            scope_fields: Mutex::new(Map::new()),
            binding: None,
            refresh_lock: None,
            timer: None,
        }
    }

    /// The adapter that gives the time of each call to the store.
    #[must_use]
    pub fn with_timer(self, timer: Option<StoreTimer>) -> Self {
        Self { timer, ..self }
    }

    fn time(&self, started: std::time::Instant) {
        if let Some(timer) = &self.timer {
            timer(started.elapsed());
        }
    }

    /// The adapter with a lock for each refresh. Two processes that refresh
    /// the same tokens at the same time would use one refresh token twice;
    /// with rotation, the second refresh then fails, and the user must log
    /// in again.
    #[must_use]
    pub fn with_refresh_lock(self, path: PathBuf) -> Self {
        Self {
            refresh_lock: Some(path),
            ..self
        }
    }

    /// The adapter with the item that the caller read and checked already,
    /// so that rmcp does not read the store again.
    #[must_use]
    pub fn with_item(self, item: TokensItem) -> Self {
        Self {
            copy: Mutex::new(Cached::Read(Box::new(Some(item.tokens)))),
            scope_fields: Mutex::new(item.scope_fields),
            binding: item.binding,
            ..self
        }
    }

    /// The adapter that binds the tokens of a login to their server.
    #[must_use]
    pub fn with_binding(self, binding: items::TokenBinding) -> Self {
        Self {
            binding: Some(binding),
            ..self
        }
    }

    /// The adapter that keeps the scopes that a login requests.
    #[must_use]
    pub fn with_requested_scopes(self, scopes: &[String]) -> Self {
        let mut fields = self.scope_fields.into_inner();
        fields.insert(items::REQUESTED_SCOPES.into(), Value::from(scopes));
        Self {
            scope_fields: Mutex::new(fields),
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

/// The tokens item: the tokens of rmcp and the fields of `ncp`.
pub struct TokensItem {
    pub tokens: StoredCredentials,
    /// The scope fields of `ncp`, by field name.
    pub scope_fields: Map<String, Value>,
    /// `None` when the item has no binding or a binding that is not valid.
    pub binding: Option<items::TokenBinding>,
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
    let scope_fields = items::SCOPE_FIELDS
        .iter()
        .filter_map(|&field| Some((field.to_string(), json.get(field)?.clone())))
        .collect();
    let binding = json
        .get(items::BINDING)
        .and_then(|binding| serde_json::from_value(binding.clone()).ok());
    let tokens = serde_json::from_value(json).map_err(error)?;
    Ok(TokensItem {
        tokens,
        scope_fields,
        binding,
    })
}

/// The mark of a failure of the credential file in the text of an rmcp
/// error. rmcp keeps only a text, and the caller needs the kind: a file
/// that fails is a local error, and a native store that fails is an
/// authorization error with the file as the fix.
const FILE_FAILED: &str = "local: ";

fn store_error(error: &StoreError) -> AuthError {
    let text = match error {
        StoreError::Io { .. } | StoreError::OpenPermissions(_) | StoreError::Corrupt(_) => {
            format!("{FILE_FAILED}{error}")
        }
        _ => error.to_string(),
    };
    AuthError::CredentialStoreError(text)
}

/// The text of a failure of the file with no mark, or `None` for a failure
/// of the native store.
#[must_use]
pub fn file_failure(text: &str) -> Option<&str> {
    text.strip_prefix(FILE_FAILED)
}

/// The mark of tokens of another server in the text of an rmcp error. The
/// caller gives the stop of design B70 for it, not "the server needs a
/// login".
const BINDING_FAILED: &str = "binding: ";

fn binding_error(mismatch: &items::BindingMismatch) -> AuthError {
    let kind = match mismatch {
        items::BindingMismatch::Missing => "missing".to_string(),
        items::BindingMismatch::Resource => "resource".to_string(),
        items::BindingMismatch::Issuer { current } => format!("issuer {current}"),
    };
    AuthError::CredentialStoreError(format!("{BINDING_FAILED}{kind}"))
}

/// The binding mismatch in the text of an rmcp error, or `None` for
/// another failure of the store.
#[must_use]
pub fn binding_mismatch(text: &str) -> Option<items::BindingMismatch> {
    match text.strip_prefix(BINDING_FAILED)? {
        "missing" => Some(items::BindingMismatch::Missing),
        "resource" => Some(items::BindingMismatch::Resource),
        kind => kind
            .strip_prefix("issuer ")
            .map(|current| items::BindingMismatch::Issuer {
                current: current.to_string(),
            }),
    }
}

/// The lock of the tokens of one server: a refresh, a login and a logout
/// hold it, so that one of them cannot undo another. It blocks; an async
/// caller runs it on a blocking thread.
///
/// # Errors
///
/// Returns an error when the lock file cannot be opened, or when another
/// process holds the lock past the limit.
pub fn lock_tokens(path: &std::path::Path) -> std::io::Result<private_file::Lock> {
    private_file::lock(path, REFRESH_LOCK_LIMIT)
}

#[async_trait::async_trait]
impl RmcpStore for TokenStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        let mut copy = self.copy.lock().await;
        if let Cached::Read(tokens) = &*copy {
            return Ok(tokens.as_ref().clone());
        }
        let (store, key) = (self.store.clone(), self.key.clone());
        let started = std::time::Instant::now();
        let secret = store::run_blocking(store::CALL_LIMIT, move || store.load(&key)).await;
        self.time(started);
        let secret = secret.map_err(|error| store_error(&error))?;
        let item = secret
            .map(|secret| parse_item(&secret))
            .transpose()
            .map_err(|error| AuthError::CredentialStoreError(error.to_string()))?;
        // A read after the refresh lock can find tokens that another
        // process saved for another server: they must not be used here.
        if let (Some(expected), Some(item)) = (&self.binding, &item) {
            items::check_binding(item.binding.as_ref(), expected)
                .map_err(|mismatch| binding_error(&mismatch))?;
        }
        let (tokens, scope_fields) = match item {
            Some(item) => (Some(item.tokens), item.scope_fields),
            None => (None, Map::new()),
        };
        *self.scope_fields.lock().await = scope_fields;
        *copy = Cached::Read(Box::new(tokens.clone()));
        Ok(tokens)
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        let failed = |reason: String| AuthError::CredentialStoreError(reason);
        let mut json =
            serde_json::to_value(&credentials).map_err(|error| failed(error.to_string()))?;
        for (field, scopes) in self.scope_fields.lock().await.iter() {
            json[field] = scopes.clone();
        }
        if let Some(binding) = &self.binding {
            json[items::BINDING] =
                serde_json::to_value(binding).map_err(|error| failed(error.to_string()))?;
        }
        let secret =
            items::to_secret("tokens", &json).map_err(|error| failed(error.to_string()))?;
        let (store, key) = (self.store.clone(), self.key.clone());
        let started = std::time::Instant::now();
        let saved = store::run_blocking(store::CALL_LIMIT, move || store.save(&key, &secret)).await;
        self.time(started);
        saved.map_err(|error| store_error(&error))?;
        *self.copy.lock().await = Cached::Read(Box::new(Some(credentials)));
        Ok(())
    }

    /// Holds the lock of the tokens through the refresh and its save. The
    /// copy in memory can be old after the wait, so the next load reads the
    /// store: another process can have refreshed the tokens.
    async fn acquire_refresh_guard(&self) -> Result<Option<CredentialRefreshGuard>, AuthError> {
        let Some(path) = self.refresh_lock.clone() else {
            return Ok(None);
        };
        // A lock file that cannot be made (for example a home that is read
        // only) must not stop a refresh of tokens in the native store: the
        // lock only orders refreshes. A wait past the limit is an error.
        let lock = store::run_blocking(store::CALL_LIMIT, move || match lock_tokens(&path) {
            Ok(lock) => Ok(Some(lock)),
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => Err(StoreError::Io {
                path: path.clone(),
                source: error,
            }),
            Err(_) => Ok(None),
        })
        .await
        .map_err(|error| store_error(&error))?;
        let Some(lock) = lock else {
            return Ok(None);
        };
        *self.copy.lock().await = Cached::Unread;
        Ok(Some(CredentialRefreshGuard::new(lock)))
    }

    async fn clear(&self) -> Result<(), AuthError> {
        let (store, key) = (self.store.clone(), self.key.clone());
        let started = std::time::Instant::now();
        let deleted = store::run_blocking(store::CALL_LIMIT, move || store.delete(&key)).await;
        self.time(started);
        deleted.map_err(|error| store_error(&error))?;
        *self.copy.lock().await = Cached::Read(Box::new(None));
        self.scope_fields.lock().await.clear();
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
    async fn each_read_and_write_of_the_store_gives_its_time() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let timer: StoreTimer = Arc::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        let adapter =
            TokenStore::new(server(), Arc::new(MemoryStore::default())).with_timer(Some(timer));
        assert!(adapter.load().await.unwrap().is_none());
        adapter.save(tokens("c1")).await.unwrap();
        // The copy in memory answers this load: no call to the store.
        adapter.load().await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_refresh_keeps_the_scope_fields_and_a_login_drops_them() {
        let backing = Arc::new(Counting::default());
        let item = Secret::new(
            r#"{"client_id":"c1","token_response":null,"ncp_required_scopes":["b"],"ncp_requested_scopes":["a"]}"#.into(),
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
        assert!(saved.expose().contains("ncp_requested_scopes"), "{saved:?}");
        let login = TokenStore::new(server(), backing.clone())
            .fresh()
            .with_requested_scopes(&["c".into()]);
        login.save(tokens("c2")).await.unwrap();
        let saved = backing.inner.load(&key).unwrap().unwrap();
        assert!(!saved.expose().contains("ncp_required_scopes"));
        assert!(saved.expose().contains(r#""ncp_requested_scopes":["c"]"#));
    }

    #[tokio::test]
    async fn a_save_writes_the_binding_and_a_read_gives_it_back() {
        let backing = Arc::new(Counting::default());
        let binding = items::TokenBinding {
            resource: "https://a/mcp".into(),
            issuer: "https://as".into(),
        };
        let adapter = TokenStore::new(server(), backing.clone())
            .fresh()
            .with_binding(binding.clone());
        adapter.save(tokens("c1")).await.unwrap();
        let key = CredentialKey::OAuthTokens(server());
        let item = parse_item(&backing.inner.load(&key).unwrap().unwrap()).unwrap();
        assert_eq!(item.binding, Some(binding));
        // A refresh keeps the binding of the item.
        let refresh = TokenStore::new(server(), backing.clone()).with_item(item);
        refresh.save(tokens("c1")).await.unwrap();
        let again = parse_item(&backing.inner.load(&key).unwrap().unwrap()).unwrap();
        assert!(again.binding.is_some());
    }

    #[tokio::test]
    async fn a_read_refuses_tokens_of_another_binding() {
        let backing = Arc::new(Counting::default());
        let binding = |resource: &str| items::TokenBinding {
            resource: resource.into(),
            issuer: "https://as".into(),
        };
        TokenStore::new(server(), backing.clone())
            .fresh()
            .with_binding(binding("https://b/mcp"))
            .save(tokens("c1"))
            .await
            .unwrap();
        let adapter =
            TokenStore::new(server(), backing.clone()).with_binding(binding("https://a/mcp"));
        let Err(AuthError::CredentialStoreError(text)) = adapter.load().await else {
            panic!("tokens of another binding must not load");
        };
        assert_eq!(
            binding_mismatch(&text),
            Some(items::BindingMismatch::Resource)
        );
        assert_eq!(file_failure(&text), None);
        let same = TokenStore::new(server(), backing).with_binding(binding("https://b/mcp"));
        assert!(same.load().await.unwrap().is_some());
    }

    #[test]
    fn each_binding_mismatch_reads_back_from_its_error_text() {
        let mismatches = [
            items::BindingMismatch::Missing,
            items::BindingMismatch::Resource,
            items::BindingMismatch::Issuer {
                current: "https://as".into(),
            },
        ];
        for mismatch in mismatches {
            let AuthError::CredentialStoreError(text) = binding_error(&mismatch) else {
                unreachable!()
            };
            assert_eq!(binding_mismatch(&text), Some(mismatch));
        }
        assert_eq!(binding_mismatch("keyring is locked"), None);
    }

    #[tokio::test]
    async fn given_tokens_need_no_read_and_clear_removes_the_item() {
        let backing = Arc::new(Counting::default());
        let adapter = TokenStore::new(server(), backing.clone()).with_item(TokensItem {
            tokens: tokens("c2"),
            scope_fields: Map::new(),
            binding: None,
        });
        assert_eq!(adapter.load().await.unwrap().unwrap().client_id, "c2");
        assert_eq!(backing.reads.load(Ordering::SeqCst), 0);
        adapter.save(tokens("c3")).await.unwrap();
        adapter.clear().await.unwrap();
        assert!(adapter.load().await.unwrap().is_none());
        assert_eq!(backing.inner.keys().unwrap(), []);
    }
}
